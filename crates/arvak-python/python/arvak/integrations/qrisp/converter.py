"""Qrisp circuit conversion utilities.

This module provides functions to convert between Qrisp and HIQ circuit formats
using OpenQASM 3.0 as an interchange format.
"""

from typing import TYPE_CHECKING, Union

if TYPE_CHECKING:
    from qrisp import QuantumCircuit, QuantumSession
    import arvak


def qrisp_to_hiq(circuit: Union['QuantumCircuit', 'QuantumSession']) -> 'hiq.Circuit':
    """Convert a Qrisp QuantumCircuit or QuantumSession to HIQ Circuit.

    This function uses OpenQASM as an interchange format:
    1. Export Qrisp circuit to QASM
    2. Import QASM into HIQ

    Args:
        circuit: Qrisp QuantumCircuit or QuantumSession instance

    Returns:
        HIQ Circuit instance

    Raises:
        ImportError: If qrisp is not installed
        ValueError: If circuit cannot be converted

    Example:
        >>> from qrisp import QuantumCircuit
        >>> qc = QuantumCircuit(2)
        >>> qc.h(0)
        >>> qc.cx(0, 1)
        >>> hiq_circuit = qrisp_to_hiq(qc)
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

    # Import into HIQ
    hiq_circuit = hiq.from_qasm(qasm_str)

    return hiq_circuit


def hiq_to_qrisp(circuit: 'hiq.Circuit') -> 'QuantumCircuit':
    """Convert HIQ Circuit to Qrisp QuantumCircuit.

    This function uses OpenQASM as an interchange format:
    1. Export HIQ circuit to QASM
    2. Import QASM into Qrisp

    Args:
        circuit: HIQ Circuit instance

    Returns:
        Qrisp QuantumCircuit instance

    Raises:
        ImportError: If qrisp is not installed
        ValueError: If circuit cannot be converted

    Example:
        >>> import arvak
        >>> hiq_circuit = hiq.Circuit.bell()
        >>> qrisp_circuit = hiq_to_qrisp(hiq_circuit)
    """
    try:
        from qrisp import QuantumCircuit
    except ImportError:
        raise ImportError(
            "Qrisp is required for this operation. "
            "Install with: pip install qrisp>=0.4.0"
        )

    import arvak

    # Export HIQ circuit to OpenQASM
    qasm_str = hiq.to_qasm(circuit)

    # Import into Qrisp
    # Qrisp can import from QASM string
    qrisp_circuit = QuantumCircuit.from_qasm_str(qasm_str)

    return qrisp_circuit
