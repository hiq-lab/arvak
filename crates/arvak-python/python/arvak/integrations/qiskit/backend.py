"""Qiskit backend provider for Arvak.

This module implements Qiskit's provider and backend interfaces, allowing
users to execute Arvak circuits through Qiskit's familiar backend.run() API.

The simulator backend calls Arvak's built-in Rust statevector simulator
directly via PyO3, returning real simulation results.
"""

from typing import List, Optional, Union, TYPE_CHECKING

if TYPE_CHECKING:
    from qiskit import QuantumCircuit
    from qiskit.providers import BackendV2, JobV1, Options


class ArvakProvider:
    """Qiskit provider for Arvak backends.

    This provider allows users to access Arvak execution capabilities through
    Qiskit's standard provider interface.

    Example:
        >>> from arvak.integrations.qiskit import ArvakProvider
        >>> provider = ArvakProvider()
        >>> backend = provider.get_backend('sim')
        >>> job = backend.run(qiskit_circuit, shots=1000)
        >>> result = job.result()
    """

    def __init__(self):
        """Initialize the Arvak provider."""
        self._backends = {}

    def backends(self, name: Optional[str] = None, **filters) -> List['BackendV2']:
        """Get list of available backends.

        Args:
            name: Optional backend name filter
            **filters: Additional filters (currently unused)

        Returns:
            List of ArvakSimulatorBackend instances
        """
        if not self._backends:
            self._backends = {
                'sim': ArvakSimulatorBackend(provider=self),
            }

        if name:
            backend = self._backends.get(name)
            return [backend] if backend else []

        return list(self._backends.values())

    def get_backend(self, name: str = 'sim') -> 'BackendV2':
        """Get a specific backend by name.

        Args:
            name: Backend name (default: 'sim')

        Returns:
            ArvakSimulatorBackend instance

        Raises:
            ValueError: If backend name is unknown
        """
        backends = self.backends(name=name)
        if not backends:
            available = list(self._backends.keys())
            raise ValueError(
                f"Unknown backend: {name}. "
                f"Available backends: {', '.join(available)}"
            )
        return backends[0]

    def __repr__(self) -> str:
        return f"<ArvakProvider(backends={list(self._backends.keys())})>"


class ArvakSimulatorBackend:
    """Arvak simulator backend with Qiskit-compatible interface.

    Wraps Arvak's built-in Rust statevector simulator. Circuits are converted
    to OpenQASM 3, compiled and simulated in Rust, and results are returned
    as standard Qiskit-compatible count dictionaries.

    Supports circuits up to ~20 qubits (exact statevector simulation).
    """

    def __init__(self, provider: ArvakProvider):
        self._provider = provider
        self.name = 'arvak_simulator'
        self.description = 'Arvak Rust statevector simulator'
        self.online_date = '2024-01-01'
        self.backend_version = '1.0.0'

    @property
    def max_circuits(self) -> Optional[int]:
        return None

    @property
    def num_qubits(self) -> int:
        return 20

    @property
    def basis_gates(self) -> List[str]:
        return ['id', 'h', 'x', 'y', 'z', 's', 't', 'sx',
                'rx', 'ry', 'rz', 'cx', 'cy', 'cz', 'swap',
                'ccx', 'measure']

    @property
    def coupling_map(self) -> Optional[List[List[int]]]:
        return None  # All-to-all connectivity

    def run(self, circuits: Union['QuantumCircuit', List['QuantumCircuit']],
            shots: int = 1024, **options) -> 'ArvakJob':
        """Run circuits on Arvak's statevector simulator.

        Args:
            circuits: Single circuit or list of circuits to execute
            shots: Number of measurement shots (default: 1024)
            **options: Additional execution options

        Returns:
            ArvakJob with real simulation results
        """
        if not isinstance(circuits, list):
            circuits = [circuits]

        import arvak

        # Execute each circuit on the simulator
        all_counts = []
        for qc in circuits:
            # Convert Qiskit circuit to QASM → Arvak circuit
            try:
                from qiskit.qasm3 import dumps
                qasm_str = dumps(qc)
            except Exception:
                from qiskit.qasm2 import dumps as dumps2
                qasm_str = dumps2(qc)

            arvak_circuit = arvak.from_qasm(qasm_str)
            counts = arvak.run_sim(arvak_circuit, shots)
            all_counts.append(counts)

        return ArvakJob(
            backend=self,
            counts=all_counts,
            shots=shots
        )

    def __repr__(self) -> str:
        return f"<ArvakSimulatorBackend('{self.name}')>"


class ArvakJob:
    """Job returned by ArvakSimulatorBackend.run().

    Contains real simulation results from the Rust statevector simulator.
    """

    def __init__(self, backend, counts, shots):
        self._backend = backend
        self._counts = counts  # List[Dict[str, int]], one per circuit
        self._shots = shots

    def result(self) -> 'ArvakResult':
        """Get job result."""
        return ArvakResult(
            backend_name=self._backend.name,
            counts=self._counts,
            shots=self._shots
        )

    def status(self) -> str:
        return "DONE"

    def __repr__(self) -> str:
        return f"<ArvakJob(circuits={len(self._counts)}, shots={self._shots})>"


class ArvakResult:
    """Result from Arvak simulator execution.

    Contains real measurement counts from the Rust statevector simulator.
    """

    def __init__(self, backend_name, counts, shots):
        self.backend_name = backend_name
        self._counts = counts  # List[Dict[str, int]]
        self._shots = shots

    def get_counts(self, circuit=None):
        """Get measurement counts for a circuit.

        Args:
            circuit: Circuit index (default: 0)

        Returns:
            Dictionary mapping bitstrings to counts
        """
        idx = 0 if circuit is None else circuit
        if idx >= len(self._counts):
            idx = 0
        return self._counts[idx]

    def __repr__(self) -> str:
        return f"<ArvakResult(backend='{self.backend_name}', circuits={len(self._counts)})>"
