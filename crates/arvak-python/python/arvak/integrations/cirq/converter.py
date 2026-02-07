"""Cirq circuit conversion utilities.

This module provides functions to convert between Cirq and Arvak circuit formats
using OpenQASM as an interchange format.
"""

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    import cirq
    import arvak


def cirq_to_arvak(circuit: 'cirq.Circuit') -> 'arvak.Circuit':
    """Convert a Cirq Circuit to Arvak Circuit.

    This function uses OpenQASM as an interchange format:
    1. Export Cirq circuit to QASM
    2. Import QASM into Arvak

    Args:
        circuit: Cirq Circuit instance

    Returns:
        Arvak Circuit instance

    Raises:
        ImportError: If cirq is not installed
        ValueError: If circuit cannot be converted

    Example:
        >>> import cirq
        >>> qubits = cirq.LineQubit.range(2)
        >>> circuit = cirq.Circuit(
        ...     cirq.H(qubits[0]),
        ...     cirq.CNOT(qubits[0], qubits[1])
        ... )
        >>> arvak_circuit = cirq_to_arvak(circuit)
    """
    try:
        import cirq
    except ImportError:
        raise ImportError(
            "Cirq is required for this operation. "
            "Install with: pip install cirq>=1.0.0"
        )

    import arvak

    # Convert Cirq circuit to OpenQASM 2.0
    # Cirq uses qasm() method for QASM 2.0 export
    qasm_str = cirq.qasm(circuit)

    # Import into Arvak
    arvak_circuit = arvak.from_qasm(qasm_str)

    return arvak_circuit


def arvak_to_cirq(circuit: 'arvak.Circuit') -> 'cirq.Circuit':
    """Convert Arvak Circuit to Cirq Circuit.

    This function uses OpenQASM as an interchange format:
    1. Export Arvak circuit to QASM
    2. Import QASM into Cirq

    Args:
        circuit: Arvak Circuit instance

    Returns:
        Cirq Circuit instance

    Raises:
        ImportError: If cirq is not installed
        ValueError: If circuit cannot be converted

    Example:
        >>> import arvak
        >>> arvak_circuit = arvak.Circuit.bell()
        >>> cirq_circuit = arvak_to_cirq(arvak_circuit)
    """
    try:
        import cirq
    except ImportError:
        raise ImportError(
            "Cirq is required for this operation. "
            "Install with: pip install cirq>=1.0.0"
        )

    import arvak

    # Export Arvak circuit to OpenQASM
    qasm_str = arvak.to_qasm(circuit)

    # Import into Cirq
    # Cirq can parse QASM strings
    cirq_circuit = cirq.circuits.qasm_input.circuit_from_qasm(qasm_str)

    return cirq_circuit
