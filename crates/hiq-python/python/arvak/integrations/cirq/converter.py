"""Cirq circuit conversion utilities.

This module provides functions to convert between Cirq and HIQ circuit formats
using OpenQASM as an interchange format.
"""

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    import cirq
    import arvak


def cirq_to_hiq(circuit: 'cirq.Circuit') -> 'hiq.Circuit':
    """Convert a Cirq Circuit to HIQ Circuit.

    This function uses OpenQASM as an interchange format:
    1. Export Cirq circuit to QASM
    2. Import QASM into HIQ

    Args:
        circuit: Cirq Circuit instance

    Returns:
        HIQ Circuit instance

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
        >>> hiq_circuit = cirq_to_hiq(circuit)
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

    # Import into HIQ
    hiq_circuit = hiq.from_qasm(qasm_str)

    return hiq_circuit


def hiq_to_cirq(circuit: 'hiq.Circuit') -> 'cirq.Circuit':
    """Convert HIQ Circuit to Cirq Circuit.

    This function uses OpenQASM as an interchange format:
    1. Export HIQ circuit to QASM
    2. Import QASM into Cirq

    Args:
        circuit: HIQ Circuit instance

    Returns:
        Cirq Circuit instance

    Raises:
        ImportError: If cirq is not installed
        ValueError: If circuit cannot be converted

    Example:
        >>> import arvak
        >>> hiq_circuit = hiq.Circuit.bell()
        >>> cirq_circuit = hiq_to_cirq(hiq_circuit)
    """
    try:
        import cirq
    except ImportError:
        raise ImportError(
            "Cirq is required for this operation. "
            "Install with: pip install cirq>=1.0.0"
        )

    import arvak

    # Export HIQ circuit to OpenQASM
    qasm_str = hiq.to_qasm(circuit)

    # Import into Cirq
    # Cirq can parse QASM strings
    cirq_circuit = cirq.circuits.qasm_input.circuit_from_qasm(qasm_str)

    return cirq_circuit
