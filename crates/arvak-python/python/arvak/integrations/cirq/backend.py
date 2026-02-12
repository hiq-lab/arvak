"""Cirq sampler for Arvak.

This module implements Cirq's Sampler interface, allowing users to execute
Arvak circuits through Cirq's sampling API.

The sampler calls Arvak's built-in Rust statevector simulator directly
via PyO3, returning real simulation results.
"""

from typing import Optional, Sequence, TYPE_CHECKING

if TYPE_CHECKING:
    import cirq


class ArvakSampler:
    """Arvak sampler implementing Cirq's Sampler interface.

    Executes Cirq circuits on Arvak's built-in Rust statevector simulator.
    Circuits are converted to OpenQASM, parsed in Rust, and simulated with
    exact statevector simulation (up to ~20 qubits).

    Example:
        >>> from arvak.integrations.cirq import ArvakSampler
        >>> import cirq
        >>>
        >>> qubits = cirq.LineQubit.range(2)
        >>> circuit = cirq.Circuit(
        ...     cirq.H(qubits[0]),
        ...     cirq.CNOT(qubits[0], qubits[1]),
        ...     cirq.measure(*qubits, key='result')
        ... )
        >>>
        >>> sampler = ArvakSampler('sim')
        >>> result = sampler.run(circuit, repetitions=1000)
        >>> print(result.histogram(key='result'))
    """

    def __init__(self, backend_name: str = 'sim'):
        """Initialize the Arvak sampler.

        Args:
            backend_name: Name of the backend to use (default: 'sim')
        """
        self.backend_name = backend_name
        self.name = f'arvak_{backend_name}'

    def run(self, program: 'cirq.Circuit',
            repetitions: int = 1,
            param_resolver: Optional['cirq.ParamResolver'] = None) -> 'ArvakResult':
        """Run the supplied Circuit on Arvak's statevector simulator.

        Args:
            program: Cirq Circuit to execute
            repetitions: Number of times to execute the circuit
            param_resolver: Parameters to resolve in the circuit

        Returns:
            ArvakResult with real measurement outcomes
        """
        import cirq
        import numpy as np

        # Resolve parameters if provided
        if param_resolver is not None:
            program = cirq.resolve_parameters(program, param_resolver)

        # Convert to Arvak and simulate
        from .converter import cirq_to_arvak
        import arvak

        arvak_circuit = cirq_to_arvak(program)
        counts = arvak.run_sim(arvak_circuit, repetitions)

        # Get measurement keys and qubit count from circuit
        measurement_keys = list(cirq.protocols.measurement_key_names(program))
        if not measurement_keys:
            measurement_keys = ['result']

        num_qubits = len(program.all_qubits())

        # Convert counts dict → numpy measurement arrays
        # counts is like {'00': 487, '11': 513}
        measurements = {}
        for key in measurement_keys:
            # Build bitstring array from counts
            rows = []
            for bitstring, count in counts.items():
                bits = [int(b) for b in bitstring]
                # Pad to num_qubits if needed
                while len(bits) < num_qubits:
                    bits.insert(0, 0)
                for _ in range(count):
                    rows.append(bits[:num_qubits])

            measurements[key] = np.array(rows, dtype=int)

        return ArvakResult(
            params=cirq.ParamResolver({}),
            measurements=measurements,
            repetitions=repetitions
        )

    def run_sweep(self, program: 'cirq.Circuit',
                  params: 'cirq.Sweepable',
                  repetitions: int = 1) -> Sequence['ArvakResult']:
        """Run the supplied Circuit for various parameter sweeps.

        Args:
            program: Cirq Circuit to execute
            params: Parameters to sweep over
            repetitions: Number of times to execute each circuit

        Returns:
            List of ArvakResult objects
        """
        import cirq
        results = []
        for resolver in cirq.to_resolvers(params):
            results.append(self.run(program, repetitions=repetitions, param_resolver=resolver))
        return results

    def __repr__(self) -> str:
        return f"<ArvakSampler('{self.name}')>"


class ArvakResult:
    """Result from Arvak sampler execution.

    Contains real measurement data from the Rust statevector simulator,
    stored as numpy arrays compatible with Cirq's Result interface.
    """

    def __init__(self, params, measurements: dict, repetitions: int):
        """Initialize the result.

        Args:
            params: Parameter resolver used
            measurements: Dict mapping measurement keys to numpy arrays
            repetitions: Number of repetitions
        """
        self.params = params
        self.measurements = measurements
        self.repetitions = repetitions

    def histogram(self, key: str = 'result') -> dict[int, int]:
        """Get histogram of measurement outcomes.

        Args:
            key: Measurement key

        Returns:
            Dictionary mapping integer outcomes to counts
        """
        if key not in self.measurements:
            return {}

        from collections import Counter

        data = self.measurements[key]
        outcomes = [''.join(map(str, row)) for row in data]
        counts = Counter(outcomes)

        return {int(k, 2): v for k, v in counts.items()}

    def multi_measurement_histogram(self, keys: Optional[list[str]] = None) -> dict:
        """Get histograms for multiple measurements."""
        if keys is None:
            keys = list(self.measurements.keys())
        return {key: self.histogram(key) for key in keys}

    def __repr__(self) -> str:
        return f"<ArvakResult(repetitions={self.repetitions}, keys={list(self.measurements.keys())})>"


class ArvakEngine:
    """Arvak engine for Cirq.

    Provides access to Arvak backends through Cirq's Engine interface.
    """

    def __init__(self, backend_name: str = 'sim'):
        self.backend_name = backend_name
        self._sampler = ArvakSampler(backend_name)

    def get_sampler(self, processor_id: Optional[str] = None) -> ArvakSampler:
        """Get a sampler for this engine."""
        return self._sampler

    def __repr__(self) -> str:
        return f"<ArvakEngine(backend='{self.backend_name}')>"
