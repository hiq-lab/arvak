"""Qrisp circuit conversion utilities.

This module provides functions to convert between Qrisp and Arvak circuit formats
using OpenQASM 3.0 as an interchange format.
"""

from typing import TYPE_CHECKING, Union

if TYPE_CHECKING:
    from qrisp import QuantumCircuit, QuantumSession
    import arvak


def qrisp_to_arvak(circuit: Union['QuantumCircuit', 'QuantumSession']) -> 'arvak.Circuit':
    """Convert a Qrisp QuantumCircuit or QuantumSession to Arvak Circuit.

    This function uses OpenQASM as an interchange format:
    1. Export Qrisp circuit to QASM
    2. Import QASM into Arvak

    Args:
        circuit: Qrisp QuantumCircuit or QuantumSession instance

    Returns:
        Arvak Circuit instance

    Raises:
        ImportError: If qrisp is not installed
        ValueError: If circuit cannot be converted

    Example:
        >>> from qrisp import QuantumCircuit
        >>> qc = QuantumCircuit(2)
        >>> qc.h(0)
        >>> qc.cx(0, 1)
        >>> arvak_circuit = qrisp_to_arvak(qc)
    """
    try:
        from qrisp import QuantumCircuit, QuantumSession
    except ImportError:
        raise ImportError(
            "Qrisp is required for this operation. "
            "Install with: pip install qrisp>=0.4.0"
        )

    import arvak

    # Handle QuantumSession by getting its circuit
    if isinstance(circuit, QuantumSession):
        circuit = circuit.compile()

    # Convert Qrisp circuit to OpenQASM
    # Qrisp uses qasm() method for QASM 2.0 export
    qasm_str = circuit.qasm()

    # Import into Arvak
    arvak_circuit = arvak.from_qasm(qasm_str)

    return arvak_circuit


def arvak_to_qrisp(circuit: 'arvak.Circuit') -> 'QuantumCircuit':
    """Convert Arvak Circuit to Qrisp QuantumCircuit.

    This function uses OpenQASM as an interchange format:
    1. Export Arvak circuit to QASM
    2. Import QASM into Qrisp

    Args:
        circuit: Arvak Circuit instance

    Returns:
        Qrisp QuantumCircuit instance

    Raises:
        ImportError: If qrisp is not installed
        ValueError: If circuit cannot be converted

    Example:
        >>> import arvak
        >>> arvak_circuit = arvak.Circuit.bell()
        >>> qrisp_circuit = arvak_to_qrisp(arvak_circuit)
    """
    try:
        from qrisp import QuantumCircuit
    except ImportError:
        raise ImportError(
            "Qrisp is required for this operation. "
            "Install with: pip install qrisp>=0.4.0"
        )

    import arvak

    # Export Arvak circuit to OpenQASM
    qasm_str = arvak.to_qasm(circuit)

    # Import into Qrisp
    # Qrisp can import from QASM string
    qrisp_circuit = QuantumCircuit.from_qasm_str(qasm_str)

    return qrisp_circuit
