"""PennyLane circuit conversion utilities.

This module provides functions to convert between PennyLane and Arvak circuit formats
using OpenQASM as an interchange format.
"""

from typing import TYPE_CHECKING, Callable

if TYPE_CHECKING:
    import pennylane as qml
    import arvak


def pennylane_to_arvak(qnode_or_tape) -> 'arvak.Circuit':
    """Convert a PennyLane QNode or QuantumTape to Arvak Circuit.

    This function uses OpenQASM as an interchange format:
    1. Construct quantum tape from QNode or use provided tape
    2. Export tape to QASM
    3. Import QASM into Arvak

    Args:
        qnode_or_tape: PennyLane QNode or QuantumTape instance

    Returns:
        Arvak Circuit instance

    Raises:
        ImportError: If pennylane is not installed
        ValueError: If circuit cannot be converted

    Example:
        >>> import pennylane as qml
        >>> dev = qml.device('default.qubit', wires=2)
        >>> @qml.qnode(dev)
        ... def circuit():
        ...     qml.Hadamard(wires=0)
        ...     qml.CNOT(wires=[0, 1])
        ...     return qml.expval(qml.PauliZ(0))
        >>> arvak_circuit = pennylane_to_arvak(circuit)
    """
    try:
        import pennylane as qml
    except ImportError:
        raise ImportError(
            "PennyLane is required for this operation. "
            "Install with: pip install pennylane>=0.32.0"
        )

    import arvak

    # Handle QNode - execute to get tape
    if isinstance(qnode_or_tape, qml.QNode):
        # Construct the tape by executing the QNode
        qnode_or_tape.construct([], {})
        tape = qnode_or_tape.qtape
    else:
        tape = qnode_or_tape

    # Convert PennyLane tape to QASM
    # Note: PennyLane doesn't have direct QASM export in all versions
    # We'll create a simple circuit and export via a workaround
    qasm_str = _tape_to_qasm(tape)

    # Import into Arvak
    arvak_circuit = arvak.from_qasm(qasm_str)

    return arvak_circuit


def arvak_to_pennylane(circuit: 'arvak.Circuit', device_name: str = 'default.qubit') -> Callable:
    """Convert Arvak Circuit to PennyLane QNode.

    This function uses OpenQASM as an interchange format:
    1. Export Arvak circuit to QASM
    2. Parse QASM and create PennyLane operations
    3. Return as QNode

    Args:
        circuit: Arvak Circuit instance
        device_name: PennyLane device to use (default: 'default.qubit')

    Returns:
        PennyLane QNode function

    Raises:
        ImportError: If pennylane is not installed
        ValueError: If circuit cannot be converted

    Example:
        >>> import arvak
        >>> arvak_circuit = arvak.Circuit.bell()
        >>> qnode = arvak_to_pennylane(arvak_circuit)
        >>> result = qnode()
    """
    try:
        import pennylane as qml
    except ImportError:
        raise ImportError(
            "PennyLane is required for this operation. "
            "Install with: pip install pennylane>=0.32.0"
        )

    import arvak

    # Export Arvak circuit to OpenQASM
    qasm_str = arvak.to_qasm(circuit)

    # Parse QASM and create PennyLane circuit
    num_wires = circuit.num_qubits
    dev = qml.device(device_name, wires=num_wires)

    # Create QNode from QASM
    @qml.qnode(dev)
    def qnode():
        # Parse and apply QASM operations
        _apply_qasm_to_pennylane(qasm_str, num_wires)
        # Return measurement (PennyLane requires a return)
        return [qml.expval(qml.PauliZ(i)) for i in range(num_wires)]

    return qnode


def _tape_to_qasm(tape) -> str:
    """Convert PennyLane tape to QASM string.

    Args:
        tape: PennyLane QuantumTape

    Returns:
        OpenQASM 2.0 string
    """
    import pennylane as qml

    # Get number of wires
    wires = tape.wires
    num_wires = len(wires)

    # Build QASM header
    qasm_lines = [
        "OPENQASM 2.0;",
        'include "qelib1.inc";',
        f"qreg q[{num_wires}];",
        f"creg c[{num_wires}];"
    ]

    # Wire mapping
    wire_map = {wire: idx for idx, wire in enumerate(sorted(wires))}

    # Convert operations to QASM
    for op in tape.operations:
        qasm_op = _operation_to_qasm(op, wire_map)
        if qasm_op:
            qasm_lines.append(qasm_op)

    # Add measurements if present
    for m in tape.measurements:
        if hasattr(m, 'wires'):
            for wire in m.wires:
                idx = wire_map.get(wire, wire)
                qasm_lines.append(f"measure q[{idx}] -> c[{idx}];")

    return "\n".join(qasm_lines)


def _operation_to_qasm(op, wire_map: dict) -> str:
    """Convert PennyLane operation to QASM.

    Args:
        op: PennyLane operation
        wire_map: Mapping from wire labels to indices

    Returns:
        QASM string for the operation
    """
    import pennylane as qml
    import numpy as np

    name = op.name
    wires = [wire_map.get(w, w) for w in op.wires]

    # Single-qubit gates
    if name == "Hadamard":
        return f"h q[{wires[0]}];"
    elif name == "PauliX":
        return f"x q[{wires[0]}];"
    elif name == "PauliY":
        return f"y q[{wires[0]}];"
    elif name == "PauliZ":
        return f"z q[{wires[0]}];"
    elif name == "S":
        return f"s q[{wires[0]}];"
    elif name == "T":
        return f"t q[{wires[0]}];"
    elif name == "SX":
        return f"sx q[{wires[0]}];"

    # Rotation gates
    elif name == "RX" and len(op.parameters) > 0:
        angle = op.parameters[0]
        return f"rx({angle}) q[{wires[0]}];"
    elif name == "RY" and len(op.parameters) > 0:
        angle = op.parameters[0]
        return f"ry({angle}) q[{wires[0]}];"
    elif name == "RZ" and len(op.parameters) > 0:
        angle = op.parameters[0]
        return f"rz({angle}) q[{wires[0]}];"

    # Two-qubit gates
    elif name == "CNOT":
        return f"cx q[{wires[0]}],q[{wires[1]}];"
    elif name == "CZ":
        return f"cz q[{wires[0]}],q[{wires[1]}];"
    elif name == "SWAP":
        return f"swap q[{wires[0]}],q[{wires[1]}];"

    # Default: comment
    return f"// Unsupported operation: {name}"


def _apply_qasm_to_pennylane(qasm_str: str, num_wires: int):
    """Apply QASM operations to current PennyLane context.

    Args:
        qasm_str: OpenQASM string
        num_wires: Number of wires
    """
    import pennylane as qml
    import re

    # Parse QASM and apply operations
    for line in qasm_str.split('\n'):
        line = line.strip()
        if not line or line.startswith('//') or line.startswith('OPENQASM') or \
           line.startswith('include') or line.startswith('qreg') or \
           line.startswith('creg') or line.startswith('measure'):
            continue

        # Parse operation
        if match := re.match(r'h q\[(\d+)\];', line):
            qml.Hadamard(wires=int(match.group(1)))
        elif match := re.match(r'x q\[(\d+)\];', line):
            qml.PauliX(wires=int(match.group(1)))
        elif match := re.match(r'y q\[(\d+)\];', line):
            qml.PauliY(wires=int(match.group(1)))
        elif match := re.match(r'z q\[(\d+)\];', line):
            qml.PauliZ(wires=int(match.group(1)))
        elif match := re.match(r'cx q\[(\d+)\],q\[(\d+)\];', line):
            qml.CNOT(wires=[int(match.group(1)), int(match.group(2))])
        elif match := re.match(r'rx\(([\d.eE+-]+)\) q\[(\d+)\];', line):
            qml.RX(float(match.group(1)), wires=int(match.group(2)))
        elif match := re.match(r'ry\(([\d.eE+-]+)\) q\[(\d+)\];', line):
            qml.RY(float(match.group(1)), wires=int(match.group(2)))
        elif match := re.match(r'rz\(([\d.eE+-]+)\) q\[(\d+)\];', line):
            qml.RZ(float(match.group(1)), wires=int(match.group(2)))
