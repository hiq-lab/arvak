#!/usr/bin/env bash
# =============================================================================
# Arvak Smoke Test
#
# Quick validation of the entire Arvak stack:
# - Python SDK core (Circuit, QASM)
# - Simulator (run_sim)
# - Framework converters (Qiskit, Cirq, Qrisp, PennyLane — skipped if not installed)
# - gRPC service (if running)
# - Dashboard (if running)
# - Audit script
#
# Usage: bash scripts/smoke-test.sh
# =============================================================================

set -uo pipefail

# --- Colours (disabled when not a terminal) ----------------------------------
if [[ -t 1 ]]; then
    GREEN='\033[0;32m'; RED='\033[0;31m'; YELLOW='\033[0;33m'
    BOLD='\033[1m'; RESET='\033[0m'
else
    GREEN=''; RED=''; YELLOW=''; BOLD=''; RESET=''
fi

PASS_COUNT=0
FAIL_COUNT=0
SKIP_COUNT=0

pass() { PASS_COUNT=$((PASS_COUNT + 1)); printf "${GREEN}✓${RESET} %s\n" "$1"; }
fail() { FAIL_COUNT=$((FAIL_COUNT + 1)); printf "${RED}✗${RESET} %s\n" "$1"; }
skip() { SKIP_COUNT=$((SKIP_COUNT + 1)); printf "${YELLOW}⊘${RESET} %s\n" "$1"; }

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# Use project venv if available, otherwise system python3
if [[ -f "$ROOT/.venv/bin/python" ]]; then
    PYTHON="$ROOT/.venv/bin/python"
else
    PYTHON="python3"
fi

echo ""
echo "${BOLD}=== Arvak Smoke Test ===${RESET}"
echo ""

# =============================================================================
# 1. Python SDK core
# =============================================================================
echo "--- 1. Python SDK Core ---"

if $PYTHON -c "
import arvak
c = arvak.Circuit.bell()
qasm = arvak.to_qasm(c)
assert 'OPENQASM' in qasm, 'QASM output missing OPENQASM header'
assert c.num_qubits == 2, f'Expected 2 qubits, got {c.num_qubits}'
" 2>/dev/null; then
    pass "Circuit creation + QASM export"
else
    fail "Circuit creation + QASM export"
fi

if $PYTHON -c "
import arvak
qasm = '''OPENQASM 3.0;
qubit[2] q;
h q[0];
cx q[0], q[1];
'''
c = arvak.from_qasm(qasm)
assert c.num_qubits == 2
" 2>/dev/null; then
    pass "QASM import (from_qasm)"
else
    fail "QASM import (from_qasm)"
fi

# =============================================================================
# 2. Simulator
# =============================================================================
echo ""
echo "--- 2. Simulator (run_sim) ---"

if $PYTHON -c "
import arvak
c = arvak.Circuit.bell()
r = arvak.run_sim(c, 1000)
assert sum(r.values()) == 1000, f'Shot count mismatch: {sum(r.values())}'
assert set(r.keys()) <= {'00', '01', '10', '11'}, f'Unexpected outcomes: {set(r.keys())}'
# Bell state should only have 00 and 11
assert set(r.keys()) <= {'00', '11'}, f'Bell state has unexpected outcomes: {r}'
" 2>/dev/null; then
    pass "Bell state simulation (1000 shots)"
else
    fail "Bell state simulation"
fi

if $PYTHON -c "
import arvak
c = arvak.Circuit.ghz(3)
r = arvak.run_sim(c, 1000)
assert sum(r.values()) == 1000
assert set(r.keys()) <= {'000', '111'}, f'GHZ-3 has unexpected outcomes: {r}'
" 2>/dev/null; then
    pass "GHZ-3 simulation"
else
    fail "GHZ-3 simulation"
fi

# =============================================================================
# 3. Framework converters (skip if not installed)
# =============================================================================
echo ""
echo "--- 3. Framework Integrations ---"

# Qiskit
if $PYTHON -c "
from qiskit import QuantumCircuit
import arvak
qc = QuantumCircuit(2, 2); qc.h(0); qc.cx(0, 1); qc.measure([0, 1], [0, 1])
i = arvak.get_integration('qiskit')
ac = i.to_arvak(qc)
assert ac.num_qubits == 2, f'Expected 2 qubits, got {ac.num_qubits}'
" 2>/dev/null; then
    pass "Qiskit converter"
else
    $PYTHON -c "import qiskit" 2>/dev/null && fail "Qiskit converter" || skip "Qiskit not installed"
fi

# Cirq
if $PYTHON -c "
import cirq
import arvak
q = cirq.LineQubit.range(2)
c = cirq.Circuit(cirq.H(q[0]), cirq.CNOT(q[0], q[1]), cirq.measure(*q, key='r'))
i = arvak.get_integration('cirq')
ac = i.to_arvak(c)
assert ac.num_qubits >= 2
" 2>/dev/null; then
    pass "Cirq converter"
else
    $PYTHON -c "import cirq" 2>/dev/null && fail "Cirq converter" || skip "Cirq not installed"
fi

# Qrisp
if $PYTHON -c "
from qrisp import QuantumCircuit
import arvak
qc = QuantumCircuit(2); qc.h(0); qc.cx(0, 1)
i = arvak.get_integration('qrisp')
ac = i.to_arvak(qc)
assert ac.num_qubits >= 2
" 2>/dev/null; then
    pass "Qrisp converter"
else
    $PYTHON -c "import qrisp" 2>/dev/null && fail "Qrisp converter" || skip "Qrisp not installed"
fi

# PennyLane
if $PYTHON -c "
import pennylane as qml
import arvak
dev = qml.device('default.qubit', wires=2)
@qml.qnode(dev)
def bell():
    qml.Hadamard(wires=0)
    qml.CNOT(wires=[0, 1])
    return qml.expval(qml.PauliZ(0))
i = arvak.get_integration('pennylane')
ac = i.to_arvak(bell)
assert ac.num_qubits == 2
" 2>/dev/null; then
    pass "PennyLane converter"
else
    $PYTHON -c "import pennylane" 2>/dev/null && fail "PennyLane converter" || skip "PennyLane not installed"
fi

# =============================================================================
# 4. gRPC service (if running)
# =============================================================================
echo ""
echo "--- 4. gRPC Service ---"

if curl -sf http://localhost:9090/health >/dev/null 2>&1; then
    pass "gRPC health endpoint (localhost:9090)"
else
    skip "gRPC server not running on localhost:9090"
fi

# =============================================================================
# 5. Dashboard (if running)
# =============================================================================
echo ""
echo "--- 5. Dashboard ---"

if curl -sf http://localhost:3000/api/health >/dev/null 2>&1; then
    pass "Dashboard health endpoint (localhost:3000)"
else
    skip "Dashboard not running on localhost:3000"
fi

# =============================================================================
# 6. Audit
# =============================================================================
echo ""
echo "--- 6. Audit ---"

if bash scripts/audit.sh >/dev/null 2>&1; then
    pass "Audit script (all checks pass)"
else
    fail "Audit script (failures detected — run 'bash scripts/audit.sh' for details)"
fi

# =============================================================================
# Summary
# =============================================================================
echo ""
echo "==========================================="
TOTAL=$((PASS_COUNT + FAIL_COUNT + SKIP_COUNT))
printf "Summary: ${GREEN}%d passed${RESET}, ${RED}%d failed${RESET}, ${YELLOW}%d skipped${RESET} (%d total)\n" \
    "$PASS_COUNT" "$FAIL_COUNT" "$SKIP_COUNT" "$TOTAL"

if [[ "$FAIL_COUNT" -gt 0 ]]; then
    echo ""
    echo "Some checks FAILED. Investigate before deploying."
    exit 1
fi

echo ""
echo "All checks passed."
exit 0
