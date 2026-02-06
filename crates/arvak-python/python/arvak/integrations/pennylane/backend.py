"""PennyLane device for HIQ.

This module implements PennyLane's Device interface, allowing users to execute
PennyLane QNodes on HIQ backends.
"""

from typing import List, Optional, Union, TYPE_CHECKING, Sequence
import warnings
import numpy as np

if TYPE_CHECKING:
    import pennylane as qml


class HIQDevice:
    """HIQ device implementing PennyLane's Device interface.

    This device allows PennyLane programs to execute on HIQ backends using
    PennyLane's standard device API.

    Example:
        >>> import pennylane as qml
        >>> from arvak.integrations.pennylane import HIQDevice
        >>>
        >>> dev = HIQDevice(wires=2, backend='sim')
        >>>
        >>> @qml.qnode(dev)
        >>> def circuit(x):
        ...     qml.RX(x, wires=0)
        ...     qml.CNOT(wires=[0, 1])
        ...     return qml.expval(qml.PauliZ(0))
        >>>
        >>> result = circuit(0.5)
    """

    name = "HIQ Device"
    short_name = "hiq.qpu"
    pennylane_requires = ">=0.32.0"
    version = "0.1.0"
    author = "HIQ Team"

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
        """Initialize the HIQ device.

        Args:
            wires: Number of wires (qubits)
            shots: Number of shots for sampling (None = exact expectation values)
            backend: HIQ backend to use (default: 'sim')
        """
        self.num_wires = wires
        self.shots = shots
        self.backend_name = backend
        self._state = None
        self._samples = None

    @property
    def wires(self):
        """Return the number of wires."""
        return self.num_wires

    def apply(self, operations, **kwargs):
        """Apply quantum operations.

        Args:
            operations: List of PennyLane operations
            **kwargs: Additional arguments
        """
        warnings.warn(
            "HIQ device execution is not yet fully implemented. "
            "For now, please use HIQ CLI for execution: "
            "'hiq run circuit.qasm --backend sim --shots 1000'. "
            "This device will return mock results.",
            RuntimeWarning
        )

        # Convert operations to HIQ circuit
        from .converter import _tape_to_qasm
        import arvak

        # Build a simple tape-like object
        class MockTape:
            def __init__(self, ops, wires):
                self.operations = ops
                self.measurements = []
                self.wires = wires

        tape = MockTape(operations, range(self.num_wires))

        # Convert to QASM
        qasm_str = _tape_to_qasm(tape)

        # Import to HIQ
        hiq_circuit = hiq.from_qasm(qasm_str)

        # Store for later (mock implementation)
        self._circuit = hiq_circuit

    def expval(self, observable, **kwargs):
        """Return the expectation value of an observable.

        Args:
            observable: PennyLane observable
            **kwargs: Additional arguments

        Returns:
            Expectation value (mock)
        """
        # Return mock expectation value
        # In real implementation, would execute circuit and compute expectation
        return 0.0

    def var(self, observable, **kwargs):
        """Return the variance of an observable.

        Args:
            observable: PennyLane observable
            **kwargs: Additional arguments

        Returns:
            Variance (mock)
        """
        return 1.0

    def sample(self, observable, **kwargs):
        """Return samples of an observable.

        Args:
            observable: PennyLane observable
            **kwargs: Additional arguments

        Returns:
            Samples array (mock)
        """
        if self.shots is None:
            raise ValueError("Number of shots must be specified for sampling")

        # Return mock samples
        return np.random.choice([0, 1], size=self.shots)

    def execute(self, circuit, **kwargs):
        """Execute a quantum circuit.

        Args:
            circuit: PennyLane quantum circuit (tape)
            **kwargs: Additional arguments

        Returns:
            Execution results
        """
        self.apply(circuit.operations)

        # Collect results based on measurements
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
        """String representation of the device."""
        return f"<HIQDevice(wires={self.num_wires}, backend='{self.backend_name}', shots={self.shots})>"


def create_device(backend: str = 'sim', **kwargs) -> HIQDevice:
    """Create an HIQ device for PennyLane.

    Args:
        backend: HIQ backend name (default: 'sim')
        **kwargs: Additional device arguments (wires, shots, etc.)

    Returns:
        HIQDevice instance

    Example:
        >>> dev = create_device('sim', wires=2, shots=1000)
    """
    return HIQDevice(backend=backend, **kwargs)
