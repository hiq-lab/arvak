"""Qiskit circuit conversion utilities.

This module provides functions to convert between Qiskit and HIQ circuit formats
using OpenQASM 3.0 as an interchange format.
"""

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from qiskit import QuantumCircuit
    import arvak


def qiskit_to_hiq(circuit: 'QuantumCircuit') -> 'hiq.Circuit':
    """Convert a Qiskit QuantumCircuit to HIQ Circuit.

    This function uses OpenQASM 3.0 as an interchange format:
    1. Export Qiskit circuit to QASM3
    2. Import QASM3 into HIQ

    Args:
        circuit: Qiskit QuantumCircuit instance

    Returns:
        HIQ Circuit instance

    Raises:
        ImportError: If qiskit is not installed
        ValueError: If circuit cannot be converted

    Example:
        >>> from qiskit import QuantumCircuit
        >>> qc = QuantumCircuit(2)
        >>> qc.h(0)
        >>> qc.cx(0, 1)
        >>> hiq_circuit = qiskit_to_hiq(qc)
    """
    try:
        from qiskit.qasm3 import dumps
    except ImportError:
        raise ImportError(
            "Qiskit is required for this operation. "
            "Install with: pip install qiskit>=1.0.0"
        )

    import arvak

    # Convert Qiskit circuit to OpenQASM 3.0
    qasm_str = dumps(circuit)

    # Import into HIQ
    hiq_circuit = hiq.from_qasm(qasm_str)

    return hiq_circuit


def hiq_to_qiskit(circuit: 'hiq.Circuit') -> 'QuantumCircuit':
    """Convert HIQ Circuit to Qiskit QuantumCircuit.

    This function uses OpenQASM 3.0 as an interchange format:
    1. Export HIQ circuit to QASM3
    2. Import QASM3 into Qiskit

    Args:
        circuit: HIQ Circuit instance

    Returns:
        Qiskit QuantumCircuit instance

    Raises:
        ImportError: If qiskit is not installed
        ValueError: If circuit cannot be converted

    Example:
        >>> import arvak
        >>> hiq_circuit = hiq.Circuit.bell()
        >>> qiskit_circuit = hiq_to_qiskit(hiq_circuit)
    """
    try:
        from qiskit import qasm3
    except ImportError:
        raise ImportError(
            "Qiskit is required for this operation. "
            "Install with: pip install qiskit>=1.0.0"
        )

    import arvak

    # Export HIQ circuit to OpenQASM 3.0
    qasm_str = hiq.to_qasm(circuit)

    # Import into Qiskit
    qiskit_circuit = qasm3.loads(qasm_str)

    return qiskit_circuit
