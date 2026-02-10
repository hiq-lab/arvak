"""Cirq circuit conversion utilities.

This module provides functions to convert between Cirq and Arvak circuit formats
using OpenQASM as an interchange format.
"""

import re
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    import cirq
    import arvak


def _qasm2_to_qasm3(qasm2: str) -> str:
    """Convert QASM 2.0 output to QASM 3.0 for Arvak's parser.

    Arvak's parser only accepts QASM 3.0 declaration syntax.

    Handles the structural differences:
    - ``OPENQASM 2.0;`` → ``OPENQASM 3.0;``
    - ``include "qelib1.inc";`` → removed
    - ``qreg name[N];`` → ``qubit[N] name;``
    - ``creg name[N];`` → ``bit[N] name;``
    - ``measure q[i] -> c[j];`` → ``c[j] = measure q[i];``
    - Comments (``//``) are passed through.
    """
    lines = qasm2.splitlines()
    out = []
    for line in lines:
        stripped = line.strip()

        if stripped.startswith('OPENQASM'):
            out.append('OPENQASM 3.0;')
            continue

        if stripped.startswith('include'):
            continue

        m = re.match(r'qreg\s+(\w+)\[(\d+)\];', stripped)
        if m:
            out.append(f'qubit[{m.group(2)}] {m.group(1)};')
            continue

        m = re.match(r'creg\s+(\w+)\[(\d+)\];', stripped)
        if m:
            out.append(f'bit[{m.group(2)}] {m.group(1)};')
            continue

        m = re.match(r'measure\s+(\w+)\[(\d+)\]\s*->\s*(\w+)\[(\d+)\];', stripped)
        if m:
            out.append(f'{m.group(3)}[{m.group(4)}] = measure {m.group(1)}[{m.group(2)}];')
            continue

        out.append(line)

    return '\n'.join(out)


def _qasm3_to_qasm2(qasm3: str) -> str:
    """Convert Arvak's QASM 3.0 output to QASM 2.0 for frameworks that need it.

    Handles the structural differences:
    - ``OPENQASM 3.0;`` → ``OPENQASM 2.0;`` + ``include "qelib1.inc";``
    - ``qubit[N] q;`` → ``qreg q[N];``
    - ``bit[N] c;`` → ``creg c[N];``
    - ``c[i] = measure q[i];`` → ``measure q[i] -> c[i];``
    """
    lines = qasm3.splitlines()
    out = []
    for line in lines:
        stripped = line.strip()

        if stripped.startswith('OPENQASM'):
            out.append('OPENQASM 2.0;')
            out.append('include "qelib1.inc";')
            continue

        if stripped.startswith('include'):
            continue

        m = re.match(r'qubit\[(\d+)\]\s+(\w+);', stripped)
        if m:
            out.append(f'qreg {m.group(2)}[{m.group(1)}];')
            continue

        m = re.match(r'bit\[(\d+)\]\s+(\w+);', stripped)
        if m:
            out.append(f'creg {m.group(2)}[{m.group(1)}];')
            continue

        m = re.match(r'(\w+)\[(\d+)\]\s*=\s*measure\s+(\w+)\[(\d+)\];', stripped)
        if m:
            out.append(f'measure {m.group(3)}[{m.group(4)}] -> {m.group(1)}[{m.group(2)}];')
            continue

        out.append(line)

    return '\n'.join(out)


def cirq_to_arvak(circuit: 'cirq.Circuit') -> 'arvak.Circuit':
    """Convert a Cirq Circuit to Arvak Circuit.

    This function uses OpenQASM as an interchange format:
    1. Export Cirq circuit to QASM 2.0
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

    # Convert Cirq circuit to OpenQASM 2.0, then up-convert to 3.0
    # (Arvak's parser only supports QASM 3.0 declaration syntax)
    qasm2_str = cirq.qasm(circuit)
    qasm3_str = _qasm2_to_qasm3(qasm2_str)

    # Import into Arvak
    arvak_circuit = arvak.from_qasm(qasm3_str)

    return arvak_circuit


def arvak_to_cirq(circuit: 'arvak.Circuit') -> 'cirq.Circuit':
    """Convert Arvak Circuit to Cirq Circuit.

    This function uses OpenQASM as an interchange format:
    1. Export Arvak circuit to QASM 3.0
    2. Down-convert to QASM 2.0 (Cirq only supports 2.0)
    3. Import QASM into Cirq

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
        from cirq.contrib.qasm_import import circuit_from_qasm
    except ImportError:
        raise ImportError(
            "Cirq is required for this operation. "
            "Install with: pip install cirq>=1.0.0 ply"
        )

    import arvak

    # Export Arvak circuit to OpenQASM 3.0, then down-convert to 2.0
    qasm3_str = arvak.to_qasm(circuit)
    qasm2_str = _qasm3_to_qasm2(qasm3_str)

    # Import into Cirq
    cirq_circuit = circuit_from_qasm(qasm2_str)

    return cirq_circuit
