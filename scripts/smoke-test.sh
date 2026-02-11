#!/usr/bin/env bash
# =============================================================================
# Arvak Smoke Test
#
# End-to-end validation of the entire Arvak stack:
#
# INPUT CHANNELS  — framework → Arvak → simulate → framework results
#   Qiskit   : QuantumCircuit → QASM3 → Arvak → run_sim → ArvakResult
#   Cirq     : cirq.Circuit   → QASM2 → Arvak → run_sim → ArvakResult
#   Qrisp    : QuantumCircuit → QASM2 → Arvak → run_sim → counts dict
#   PennyLane: QNode/Tape     → QASM3 → Arvak → run_sim → expval/samples
#
# OUTPUT CHANNELS — Arvak → external backend
#   QDMI/DDSIM: Arvak → QASM → QDMI FFI → MQT DDSIM → histogram
#   gRPC      : Arvak → protobuf → gRPC service → job result
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

CARGO="${CARGO:-$HOME/.cargo/bin/cargo}"

# Configurable service ports (used by sections 8 & 9)
DASHBOARD_PORT="${ARVAK_DASHBOARD_PORT:-3000}"
GRPC_HTTP_PORT="${ARVAK_GRPC_HTTP_PORT:-9090}"

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
# 3. Input channels — framework → Arvak → simulate → framework results
# =============================================================================
echo ""
echo "--- 3. Input: Qiskit ---"

# Qiskit: to_arvak converter
if $PYTHON -c "
from qiskit import QuantumCircuit
import arvak
qc = QuantumCircuit(2, 2); qc.h(0); qc.cx(0, 1); qc.measure([0, 1], [0, 1])
i = arvak.get_integration('qiskit')
ac = i.to_arvak(qc)
assert ac.num_qubits == 2, f'Expected 2 qubits, got {ac.num_qubits}'
" 2>/dev/null; then
    pass "Qiskit → Arvak converter"
else
    $PYTHON -c "import qiskit" 2>/dev/null && fail "Qiskit → Arvak converter" || skip "Qiskit not installed"
fi

# Qiskit: from_arvak converter
if $PYTHON -c "
import arvak
from arvak.integrations.qiskit import arvak_to_qiskit
ac = arvak.Circuit.bell()
qc = arvak_to_qiskit(ac)
assert qc.num_qubits >= 2, f'Expected >=2 qubits, got {qc.num_qubits}'
" 2>/dev/null; then
    pass "Arvak → Qiskit converter"
else
    $PYTHON -c "import qiskit" 2>/dev/null && fail "Arvak → Qiskit converter" || skip "Qiskit not installed"
fi

# Qiskit: full round-trip via ArvakSimulatorBackend
if $PYTHON -c "
from qiskit import QuantumCircuit
import arvak
i = arvak.get_integration('qiskit')
provider = i.get_backend_provider()
backend = provider.get_backend('sim')
qc = QuantumCircuit(2, 2); qc.h(0); qc.cx(0, 1); qc.measure([0, 1], [0, 1])
job = backend.run(qc, shots=500)
counts = job.result().get_counts()
assert sum(counts.values()) == 500, f'Expected 500 shots, got {sum(counts.values())}'
assert set(counts.keys()) <= {'00', '11'}, f'Bell: unexpected outcomes {counts}'
" 2>/dev/null; then
    pass "Qiskit full round-trip (backend.run → ArvakResult)"
else
    $PYTHON -c "import qiskit" 2>/dev/null && fail "Qiskit full round-trip" || skip "Qiskit not installed"
fi

echo ""
echo "--- 4. Input: Cirq ---"

# Cirq: to_arvak converter
if $PYTHON -c "
import cirq, arvak
q = cirq.LineQubit.range(2)
c = cirq.Circuit(cirq.H(q[0]), cirq.CNOT(q[0], q[1]), cirq.measure(*q, key='r'))
i = arvak.get_integration('cirq')
ac = i.to_arvak(c)
assert ac.num_qubits >= 2
" 2>/dev/null; then
    pass "Cirq → Arvak converter"
else
    $PYTHON -c "import cirq" 2>/dev/null && fail "Cirq → Arvak converter" || skip "Cirq not installed"
fi

# Cirq: from_arvak converter
if $PYTHON -c "
import cirq, arvak
from arvak.integrations.cirq import arvak_to_cirq
ac = arvak.Circuit.bell()
cc = arvak_to_cirq(ac)
assert isinstance(cc, cirq.Circuit)
assert len(cc.all_qubits()) >= 2
" 2>/dev/null; then
    pass "Arvak → Cirq converter"
else
    $PYTHON -c "import cirq" 2>/dev/null && fail "Arvak → Cirq converter" || skip "Cirq not installed"
fi

# Cirq: full round-trip via ArvakSampler
if $PYTHON -c "
import cirq, arvak
q = cirq.LineQubit.range(2)
c = cirq.Circuit(cirq.H(q[0]), cirq.CNOT(q[0], q[1]), cirq.measure(*q, key='result'))
i = arvak.get_integration('cirq')
engine = i.get_backend_provider()
sampler = engine.get_sampler()
result = sampler.run(c, repetitions=500)
hist = result.histogram(key='result')
assert sum(hist.values()) == 500, f'Expected 500 reps, got {sum(hist.values())}'
for outcome in hist.keys():
    assert outcome in (0, 3), f'Bell: unexpected outcome {outcome}'
" 2>/dev/null; then
    pass "Cirq full round-trip (sampler.run → histogram)"
else
    $PYTHON -c "import cirq" 2>/dev/null && fail "Cirq full round-trip" || skip "Cirq not installed"
fi

echo ""
echo "--- 5. Input: Qrisp ---"

# Qrisp: to_arvak converter
if $PYTHON -c "
from qrisp import QuantumCircuit
import arvak
qc = QuantumCircuit(2); qc.h(0); qc.cx(0, 1)
i = arvak.get_integration('qrisp')
ac = i.to_arvak(qc)
assert ac.num_qubits >= 2
" 2>/dev/null; then
    pass "Qrisp → Arvak converter"
else
    $PYTHON -c "import qrisp" 2>/dev/null && fail "Qrisp → Arvak converter" || skip "Qrisp not installed"
fi

# Qrisp: from_arvak converter
if $PYTHON -c "
from qrisp import QuantumCircuit as QC
import arvak
from arvak.integrations.qrisp import arvak_to_qrisp
ac = arvak.Circuit.bell()
qc = arvak_to_qrisp(ac)
assert isinstance(qc, QC)
assert qc.num_qubits() >= 2
" 2>/dev/null; then
    pass "Arvak → Qrisp converter"
else
    $PYTHON -c "import qrisp" 2>/dev/null && fail "Arvak → Qrisp converter" || skip "Qrisp not installed"
fi

# Qrisp: full round-trip via ArvakBackendClient
if $PYTHON -c "
from qrisp import QuantumCircuit
import arvak
qc = QuantumCircuit(2); qc.h(0); qc.cx(0, 1)
for qubit in qc.qubits:
    qc.measure(qubit)
i = arvak.get_integration('qrisp')
provider = i.get_backend_provider()
backend = provider.get_backend('sim')
counts = backend.run(qc, shots=500)
assert sum(counts.values()) == 500, f'Expected 500 shots, got {sum(counts.values())}'
assert set(counts.keys()) <= {'00', '11'}, f'Bell: unexpected outcomes {counts}'
" 2>/dev/null; then
    pass "Qrisp full round-trip (backend.run → counts)"
else
    $PYTHON -c "import qrisp" 2>/dev/null && fail "Qrisp full round-trip" || skip "Qrisp not installed"
fi

echo ""
echo "--- 6. Input: PennyLane ---"

# PennyLane: to_arvak converter
if $PYTHON -c "
import pennylane as qml, arvak
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
    pass "PennyLane → Arvak converter"
else
    $PYTHON -c "import pennylane" 2>/dev/null && fail "PennyLane → Arvak converter" || skip "PennyLane not installed"
fi

# PennyLane: from_arvak converter
if $PYTHON -c "
import pennylane as qml, arvak
from arvak.integrations.pennylane import arvak_to_pennylane
ac = arvak.Circuit.bell()
qnode = arvak_to_pennylane(ac)
result = qnode()
assert result is not None
" 2>/dev/null; then
    pass "Arvak → PennyLane converter"
else
    $PYTHON -c "import pennylane" 2>/dev/null && fail "Arvak → PennyLane converter" || skip "PennyLane not installed"
fi

# PennyLane: full round-trip via ArvakDevice
if $PYTHON -c "
import pennylane as qml
from arvak.integrations.pennylane import ArvakDevice
dev = ArvakDevice(wires=1, shots=1000)
# X gate flips |0> to |1>; PauliZ on |1> should give expval = -1
dev.apply([qml.PauliX(wires=0)])
expval = dev.expval(qml.PauliZ(wires=0))
assert abs(expval - (-1.0)) < 0.1, f'Expected ~-1.0, got {expval}'
" 2>/dev/null; then
    pass "PennyLane full round-trip (ArvakDevice → expval)"
else
    $PYTHON -c "import pennylane" 2>/dev/null && fail "PennyLane full round-trip" || skip "PennyLane not installed"
fi

# PennyLane: variance and sampling
if $PYTHON -c "
import pennylane as qml
from arvak.integrations.pennylane import ArvakDevice
dev = ArvakDevice(wires=1, shots=10000)
dev.apply([qml.Hadamard(wires=0)])
var = dev.var(qml.PauliZ(wires=0))
assert abs(var - 1.0) < 0.15, f'H|0> var(Z) should be ~1.0, got {var}'
samples = dev.sample(qml.PauliZ(wires=0))
assert len(samples) == 10000
assert all(s in (1.0, -1.0) for s in samples), 'Samples must be +1 or -1'
" 2>/dev/null; then
    pass "PennyLane ArvakDevice (variance + sampling)"
else
    $PYTHON -c "import pennylane" 2>/dev/null && fail "PennyLane variance + sampling" || skip "PennyLane not installed"
fi

# =============================================================================
# 7. Output: QDMI / DDSIM
# =============================================================================
echo ""
echo "--- 7. Output: QDMI / DDSIM ---"

if [[ -n "${DDSIM_QDMI_DEVICE_PATH:-}" ]] && [[ -f "${DDSIM_QDMI_DEVICE_PATH}" ]]; then
    # Run the Rust DDSIM integration tests via Cargo
    if $CARGO test -p arvak-qdmi --test ddsim_integration -q 2>/dev/null; then
        pass "QDMI/DDSIM device load + Bell state (QASM2)"
        pass "QDMI/DDSIM Bell state (QASM3)"
    else
        fail "QDMI/DDSIM integration tests"
    fi
else
    skip "QDMI/DDSIM not available (set DDSIM_QDMI_DEVICE_PATH)"
fi

# QDMI mock backend (always available, no device needed)
if $CARGO test -p arvak-adapter-qdmi -q 2>/dev/null; then
    pass "QDMI mock backend (unit tests)"
else
    fail "QDMI mock backend (unit tests)"
fi

# =============================================================================
# 8. Output: gRPC service
# =============================================================================
echo ""
echo "--- 8. Output: gRPC Service ---"

if curl -sf "http://localhost:${GRPC_HTTP_PORT}/health" >/dev/null 2>&1; then
    pass "gRPC health endpoint (localhost:${GRPC_HTTP_PORT})"

    # Submit a Bell state circuit via gRPC HTTP gateway
    if curl -sf -X POST "http://localhost:${GRPC_HTTP_PORT}/v1/jobs" \
        -H 'Content-Type: application/json' \
        -d '{"circuit":{"qasm":"OPENQASM 3.0;\nqubit[2] q;\nbit[2] c;\nh q[0];\ncx q[0], q[1];\nc[0] = measure q[0];\nc[1] = measure q[1];"},"backend":"sim","shots":100}' \
        -o /dev/null 2>/dev/null; then
        pass "gRPC job submission (Bell via HTTP gateway)"
    else
        fail "gRPC job submission"
    fi
else
    skip "gRPC server not running on localhost:${GRPC_HTTP_PORT}"
fi

# gRPC Rust unit + integration tests (always available)
if $CARGO test -p arvak-grpc -q 2>/dev/null; then
    pass "gRPC service tests (unit + integration)"
else
    fail "gRPC service tests"
fi

# =============================================================================
# 9. Dashboard (if running)
# =============================================================================
echo ""
echo "--- 9. Dashboard ---"

if curl -sf "http://localhost:${DASHBOARD_PORT}/api/health" >/dev/null 2>&1; then
    pass "Dashboard health endpoint (localhost:${DASHBOARD_PORT})"
else
    skip "Dashboard not running on localhost:${DASHBOARD_PORT}"
fi

# =============================================================================
# 10. Audit
# =============================================================================
echo ""
echo "--- 10. Audit ---"

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
