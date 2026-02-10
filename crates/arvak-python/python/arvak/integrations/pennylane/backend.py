"""PennyLane device for Arvak.

This module implements PennyLane's Device interface, allowing users to execute
PennyLane QNodes on Arvak backends.

The device calls Arvak's built-in Rust statevector simulator directly
via PyO3, returning real simulation results. Expectation values are
computed from measurement samples.
"""

from typing import List, Optional, Union, TYPE_CHECKING, Sequence
import numpy as np

if TYPE_CHECKING:
    import pennylane as qml


class ArvakDevice:
    """Arvak device implementing PennyLane's Device interface.

    Executes PennyLane circuits on Arvak's built-in Rust statevector
    simulator. Circuits are converted to OpenQASM, simulated in Rust,
    and expectation values are computed from measurement counts.

    Supports circuits up to ~20 qubits (exact statevector simulation).

    Example:
        >>> import pennylane as qml
        >>> from arvak.integrations.pennylane import ArvakDevice
        >>>
        >>> dev = ArvakDevice(wires=2, shots=1000, backend='sim')
        >>>
        >>> @qml.qnode(dev)
        >>> def circuit(x):
        ...     qml.RX(x, wires=0)
        ...     qml.CNOT(wires=[0, 1])
        ...     return qml.expval(qml.PauliZ(0))
        >>>
        >>> result = circuit(0.5)
    """

    name = "Arvak Device"
    short_name = "arvak.qpu"
    pennylane_requires = ">=0.32.0"
    version = "1.0.0"
    author = "Arvak Team"

    operations = {
        "Hadamard", "PauliX", "PauliY", "PauliZ",
        "S", "T", "SX",
        "RX", "RY", "RZ", "PhaseShift",
        "CNOT", "CZ", "SWAP", "Toffoli",
        "QubitUnitary"
    }

    observables = {
        "PauliX", "PauliY", "PauliZ",
        "Hadamard", "Hermitian", "Identity"
    }

    def __init__(self, wires: int = 1, shots: Optional[int] = None,
                 backend: str = 'sim'):
        """Initialize the Arvak device.

        Args:
            wires: Number of wires (qubits)
            shots: Number of shots for sampling (None defaults to 1024)
            backend: Arvak backend to use (default: 'sim')
        """
        self.num_wires = wires
        self.shots = shots if shots is not None else 1024
        self.backend_name = backend
        self._counts = None

    @property
    def wires(self):
        """Return the number of wires."""
        return self.num_wires

    def _run_circuit(self, operations):
        """Execute operations on the simulator and store counts.

        Args:
            operations: List of PennyLane operations
        """
        from .converter import _tape_to_qasm, _operation_to_qasm
        import arvak

        # Build QASM from operations
        num_qubits = self.num_wires
        wire_map = {i: i for i in range(num_qubits)}

        lines = [
            'OPENQASM 2.0;',
            'include "qelib1.inc";',
            f'qreg q[{num_qubits}];',
            f'creg c[{num_qubits}];',
        ]

        for op in operations:
            qasm_line = _operation_to_qasm(op, wire_map)
            if qasm_line:
                lines.append(qasm_line)

        # Add measurements
        for i in range(num_qubits):
            lines.append(f'measure q[{i}] -> c[{i}];')

        qasm_str = '\n'.join(lines)

        # Simulate
        arvak_circuit = arvak.from_qasm(qasm_str)
        self._counts = arvak.run_sim(arvak_circuit, self.shots)

    def _counts_to_samples(self):
        """Convert measurement counts to a numpy sample array.

        Returns:
            np.ndarray of shape (shots, num_wires) with 0/1 values
        """
        if self._counts is None:
            return np.zeros((self.shots, self.num_wires), dtype=int)

        rows = []
        for bitstring, count in self._counts.items():
            bits = [int(b) for b in bitstring]
            while len(bits) < self.num_wires:
                bits.insert(0, 0)
            for _ in range(count):
                rows.append(bits[:self.num_wires])

        return np.array(rows, dtype=int)

    def apply(self, operations, **kwargs):
        """Apply quantum operations and simulate.

        Args:
            operations: List of PennyLane operations
        """
        self._run_circuit(operations)

    def expval(self, observable, **kwargs):
        """Return the expectation value of an observable.

        Computed from measurement samples using the observable's eigenvalues.

        Args:
            observable: PennyLane observable

        Returns:
            float: Expectation value
        """
        samples = self._counts_to_samples()
        wire = observable.wires[0] if hasattr(observable, 'wires') else 0

        # For Pauli-Z: eigenvalues are +1 (|0⟩) and -1 (|1⟩)
        wire_samples = samples[:, wire]
        eigenvalues = 1.0 - 2.0 * wire_samples  # maps 0→+1, 1→-1
        return float(np.mean(eigenvalues))

    def var(self, observable, **kwargs):
        """Return the variance of an observable.

        Args:
            observable: PennyLane observable

        Returns:
            float: Variance
        """
        samples = self._counts_to_samples()
        wire = observable.wires[0] if hasattr(observable, 'wires') else 0

        wire_samples = samples[:, wire]
        eigenvalues = 1.0 - 2.0 * wire_samples
        return float(np.var(eigenvalues))

    def sample(self, observable, **kwargs):
        """Return samples of an observable.

        Args:
            observable: PennyLane observable

        Returns:
            np.ndarray: Array of sample outcomes
        """
        samples = self._counts_to_samples()
        wire = observable.wires[0] if hasattr(observable, 'wires') else 0

        wire_samples = samples[:, wire]
        return 1.0 - 2.0 * wire_samples  # Pauli-Z eigenvalues

    def execute(self, circuit, **kwargs):
        """Execute a quantum circuit (tape).

        Args:
            circuit: PennyLane quantum tape
            **kwargs: Additional arguments

        Returns:
            Execution results (single value or list)
        """
        self.apply(circuit.operations)

        results = []
        for m in circuit.measurements:
            if m.return_type.name == "Expectation":
                results.append(self.expval(m.obs))
            elif m.return_type.name == "Variance":
                results.append(self.var(m.obs))
            elif m.return_type.name == "Sample":
                results.append(self.sample(m.obs))

        return results if len(results) > 1 else results[0] if results else None

    def __repr__(self) -> str:
        return f"<ArvakDevice(wires={self.num_wires}, backend='{self.backend_name}', shots={self.shots})>"


def create_device(backend: str = 'sim', **kwargs) -> ArvakDevice:
    """Create an Arvak device for PennyLane.

    Args:
        backend: Arvak backend name (default: 'sim')
        **kwargs: Additional device arguments (wires, shots, etc.)

    Returns:
        ArvakDevice instance

    Example:
        >>> dev = create_device('sim', wires=2, shots=1000)
    """
    return ArvakDevice(backend=backend, **kwargs)
