"""Qiskit circuit conversion utilities.

This module provides functions to convert between Qiskit and Arvak circuit formats
using OpenQASM 3.0 as an interchange format.
"""

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from qiskit import QuantumCircuit
    import arvak


def qiskit_to_arvak(circuit: 'QuantumCircuit') -> 'arvak.Circuit':
    """Convert a Qiskit QuantumCircuit to Arvak Circuit.

    This function uses OpenQASM 3.0 as an interchange format:
    1. Export Qiskit circuit to QASM3
    2. Import QASM3 into Arvak

    Args:
        circuit: Qiskit QuantumCircuit instance

    Returns:
        Arvak Circuit instance

    Raises:
        ImportError: If qiskit is not installed
        ValueError: If circuit cannot be converted

    Example:
        >>> from qiskit import QuantumCircuit
        >>> qc = QuantumCircuit(2)
        >>> qc.h(0)
        >>> qc.cx(0, 1)
        >>> arvak_circuit = qiskit_to_arvak(qc)
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

    # Import into Arvak
    arvak_circuit = arvak.from_qasm(qasm_str)

    return arvak_circuit


def arvak_to_qiskit(circuit: 'arvak.Circuit') -> 'QuantumCircuit':
    """Convert Arvak Circuit to Qiskit QuantumCircuit.

    This function uses OpenQASM 3.0 as an interchange format:
    1. Export Arvak circuit to QASM3
    2. Import QASM3 into Qiskit

    Args:
        circuit: Arvak Circuit instance

    Returns:
        Qiskit QuantumCircuit instance

    Raises:
        ImportError: If qiskit is not installed
        ValueError: If circuit cannot be converted

    Example:
        >>> import arvak
        >>> arvak_circuit = arvak.Circuit.bell()
        >>> qiskit_circuit = arvak_to_qiskit(arvak_circuit)
    """
    try:
        from qiskit import qasm3
    except ImportError:
        raise ImportError(
            "Qiskit is required for this operation. "
            "Install with: pip install qiskit>=1.0.0"
        )

    import arvak

    # Export Arvak circuit to OpenQASM 3.0
    qasm_str = arvak.to_qasm(circuit)

    # Inject 'include "stdgates.inc";' if not present (Qiskit requires it)
    if 'stdgates.inc' not in qasm_str:
        qasm_str = qasm_str.replace(
            'OPENQASM 3.0;',
            'OPENQASM 3.0;\ninclude "stdgates.inc";',
            1
        )

    # Import into Qiskit
    qiskit_circuit = qasm3.loads(qasm_str)

    return qiskit_circuit
