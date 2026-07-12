"""PennyLane circuit conversion utilities.

This module provides functions to convert between PennyLane and Arvak
circuit formats using OpenQASM as an interchange format. The
PennyLane → Arvak direction uses PennyLane's built-in ``qml.to_openqasm``
serializer (which decomposes composite operations itself); the reverse
direction parses the gate set Arvak's QASM3 emitter produces.
"""

from __future__ import annotations

import math
import re
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    import arvak


def pennylane_to_arvak(qnode_or_tape, *args, **kwargs) -> 'arvak.Circuit':
    """Convert a PennyLane QNode or QuantumTape to Arvak Circuit.

    This function uses OpenQASM as an interchange format:
    1. Construct quantum tape from QNode or use provided tape
    2. Serialize with ``qml.to_openqasm`` (decomposes composite gates)
    3. Import QASM into Arvak

    Args:
        qnode_or_tape: PennyLane QNode or QuantumTape instance
        *args: Positional arguments to pass to the QNode (for parameterized circuits)
        **kwargs: Keyword arguments to pass to the QNode

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
    from .._qasm import qasm2_to_qasm3

    # Handle QNode - construct the tape with the provided arguments
    if isinstance(qnode_or_tape, qml.QNode):
        from pennylane.workflow import construct_tape
        tape = construct_tape(qnode_or_tape)(*args, **kwargs)
    else:
        tape = qnode_or_tape

    # rotations=False: export the circuit as written; observables'
    # diagonalizing gates are an execution concern (see backend.py).
    qasm2_str = qml.to_openqasm(tape, rotations=False, measure_all=True)

    return arvak.from_qasm(qasm2_to_qasm3(qasm2_str))


def arvak_to_pennylane(circuit: 'arvak.Circuit', device_name: str = 'default.qubit'):
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
        ValueError: If the QASM contains a gate with no PennyLane mapping

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


def _make_gate_table():
    """Gate-name → PennyLane-constructor table for the QASM gate set
    Arvak's QASM3 emitter produces. Each entry: (n_params, callable)."""
    import pennylane as qml

    def adj(op_cls):
        return lambda params, wires: qml.adjoint(op_cls(wires=wires))

    return {
        # single-qubit, no params
        'id':    (0, lambda p, w: qml.Identity(wires=w)),
        'h':     (0, lambda p, w: qml.Hadamard(wires=w)),
        'x':     (0, lambda p, w: qml.PauliX(wires=w)),
        'y':     (0, lambda p, w: qml.PauliY(wires=w)),
        'z':     (0, lambda p, w: qml.PauliZ(wires=w)),
        's':     (0, lambda p, w: qml.S(wires=w)),
        'sdg':   (0, adj(qml.S)),
        't':     (0, lambda p, w: qml.T(wires=w)),
        'tdg':   (0, adj(qml.T)),
        'sx':    (0, lambda p, w: qml.SX(wires=w)),
        'sxdg':  (0, adj(qml.SX)),
        # single-qubit, parameterized
        'rx':    (1, lambda p, w: qml.RX(p[0], wires=w)),
        'ry':    (1, lambda p, w: qml.RY(p[0], wires=w)),
        'rz':    (1, lambda p, w: qml.RZ(p[0], wires=w)),
        'p':     (1, lambda p, w: qml.PhaseShift(p[0], wires=w)),
        'u3':    (3, lambda p, w: qml.U3(p[0], p[1], p[2], wires=w)),
        # prx(theta, phi) == Rz(phi) . Rx(theta) . Rz(-phi)
        'prx':   (2, lambda p, w: [qml.RZ(-p[1], wires=w),
                                   qml.RX(p[0], wires=w),
                                   qml.RZ(p[1], wires=w)]),
        # two-qubit
        'cx':    (0, lambda p, w: qml.CNOT(wires=w)),
        'cy':    (0, lambda p, w: qml.CY(wires=w)),
        'cz':    (0, lambda p, w: qml.CZ(wires=w)),
        'ch':    (0, lambda p, w: qml.CH(wires=w)),
        'swap':  (0, lambda p, w: qml.SWAP(wires=w)),
        'iswap': (0, lambda p, w: qml.ISWAP(wires=w)),
        'ecr':   (0, lambda p, w: qml.ECR(wires=w)),
        'cp':    (1, lambda p, w: qml.ControlledPhaseShift(p[0], wires=w)),
        'crx':   (1, lambda p, w: qml.CRX(p[0], wires=w)),
        'cry':   (1, lambda p, w: qml.CRY(p[0], wires=w)),
        'crz':   (1, lambda p, w: qml.CRZ(p[0], wires=w)),
        'rxx':   (1, lambda p, w: qml.IsingXX(p[0], wires=w)),
        'ryy':   (1, lambda p, w: qml.IsingYY(p[0], wires=w)),
        'rzz':   (1, lambda p, w: qml.IsingZZ(p[0], wires=w)),
        # three-qubit
        'ccx':   (0, lambda p, w: qml.Toffoli(wires=w)),
        'cswap': (0, lambda p, w: qml.CSWAP(wires=w)),
    }


def _parse_param(expr: str) -> float:
    """Evaluate a QASM angle expression (numbers, pi, + - * /, parens)."""
    allowed = set('0123456789.eE+-*/() ')
    if not set(expr.replace('pi', '')) <= allowed:
        raise ValueError(f"Unsupported QASM parameter expression: {expr!r}")
    return float(eval(expr, {"__builtins__": {}}, {"pi": math.pi}))  # noqa: S307


def _apply_qasm_to_pennylane(qasm_str: str, num_wires: int):
    """Apply QASM operations to the current PennyLane queuing context.

    Supports the full gate set Arvak's QASM3 emitter produces. Raises
    ``ValueError`` for gate lines with no PennyLane mapping instead of
    silently dropping them.

    Args:
        qasm_str: OpenQASM string (2.0 or 3.0 declarations)
        num_wires: Number of wires
    """
    table = _make_gate_table()

    skip = re.compile(
        r'^(//|OPENQASM|include|qreg|creg|qubit|bit|barrier|measure'
        r'|\w+\[\d+\]\s*=\s*measure)'
    )
    gate_re = re.compile(
        r'^([a-z][a-z0-9_]*)\s*(?:\(([^)]*)\))?\s+(.+);$'
    )
    wire_re = re.compile(r'\w+\[(\d+)\]')

    for line in qasm_str.split('\n'):
        line = line.strip()
        if not line or skip.match(line):
            continue

        m = gate_re.match(line)
        if not m:
            raise ValueError(f"Cannot parse QASM line: {line!r}")

        name, param_str, args = m.group(1), m.group(2), m.group(3)
        if name not in table:
            raise ValueError(
                f"QASM gate '{name}' has no PennyLane mapping "
                f"(line: {line!r})"
            )

        n_params, ctor = table[name]
        params = []
        if param_str is not None:
            params = [_parse_param(p) for p in param_str.split(',')]
        if len(params) != n_params:
            raise ValueError(
                f"Gate '{name}' expects {n_params} parameter(s), "
                f"got {len(params)} (line: {line!r})"
            )

        wires = [int(w) for w in wire_re.findall(args)]
        ctor(params, wires if len(wires) > 1 else wires[0])
