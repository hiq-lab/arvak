"""Shared OpenQASM 2.0 ↔ 3.0 structural conversion helpers.

Several framework integrations (Cirq, Qrisp) exchange circuits with Arvak
through OpenQASM but only speak QASM 2.0, while Arvak's parser and emitter
speak QASM 3.0. These helpers translate the declaration syntax between the
two versions; gate bodies pass through unchanged.
"""

from __future__ import annotations

import re


def qasm2_to_qasm3(qasm2: str) -> str:
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


def qasm3_to_qasm2(qasm3: str) -> str:
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
