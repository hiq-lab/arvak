#!/usr/bin/env bash
# Continuously submit demo jobs to the Arvak dashboard.
# Usage: ./demo-jobs.sh [interval_seconds] [dashboard_url]

INTERVAL="${1:-30}"
BASE_URL="${2:-http://127.0.0.1:3000}"
API="$BASE_URL/api/jobs"

# QASM circuits for demo jobs
CIRCUITS=(
'OPENQASM 3.0;\ninclude \"stdgates.inc\";\nqubit[2] q;\nbit[2] c;\nh q[0];\ncx q[0], q[1];\nc = measure q;'
'OPENQASM 3.0;\ninclude \"stdgates.inc\";\nqubit[3] q;\nbit[3] c;\nh q[0];\ncx q[0], q[1];\ncx q[1], q[2];\nc = measure q;'
'OPENQASM 3.0;\ninclude \"stdgates.inc\";\nqubit[2] q;\nbit[2] c;\nx q[0];\nh q[0];\ncx q[0], q[1];\nc = measure q;'
'OPENQASM 3.0;\ninclude \"stdgates.inc\";\nqubit[4] q;\nbit[4] c;\nh q[0];\ncx q[0], q[1];\nh q[2];\ncx q[2], q[3];\nc = measure q;'
'OPENQASM 3.0;\ninclude \"stdgates.inc\";\nqubit[3] q;\nbit[3] c;\nh q[0];\nh q[1];\nh q[2];\nc = measure q;'
)

NAMES=(
  "Bell State Preparation"
  "GHZ State (3 qubits)"
  "X+Bell Circuit"
  "Dual Entanglement (4 qubits)"
  "Uniform Superposition (3 qubits)"
)

BACKENDS=("simulator" "" "simulator" "" "simulator")
SHOTS=(1024 2048 512 4096 1024)
PRIORITIES=(100 150 80 200 100)

counter=0

echo "Arvak Demo Job Feeder"
echo "Dashboard: $BASE_URL"
echo "Interval:  ${INTERVAL}s"
echo "---"

while true; do
    idx=$((counter % ${#CIRCUITS[@]}))
    name="${NAMES[$idx]} #$((counter + 1))"
    qasm="${CIRCUITS[$idx]}"
    shots="${SHOTS[$idx]}"
    priority="${PRIORITIES[$idx]}"
    backend="${BACKENDS[$idx]}"

    # Note: QASM strings must not contain unescaped double quotes or printf format specifiers
    if [ -n "$backend" ]; then
        payload=$(printf '{"name":"%s","qasm":"%s","shots":%d,"priority":%d,"backend":"%s"}' \
            "$name" "$qasm" "$shots" "$priority" "$backend")
    else
        payload=$(printf '{"name":"%s","qasm":"%s","shots":%d,"priority":%d}' \
            "$name" "$qasm" "$shots" "$priority")
    fi

    response=$(curl -s -w "\n%{http_code}" -X POST "$API" \
        -H "Content-Type: application/json" \
        -d "$payload")

    http_code=$(echo "$response" | tail -1)
    body=$(echo "$response" | head -1)

    if [ "$http_code" = "200" ]; then
        job_id=$(echo "$body" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
        echo "[$(date '+%H:%M:%S')] Submitted: $name (${job_id:0:8}...)"
    else
        echo "[$(date '+%H:%M:%S')] ERROR $http_code: $body"
    fi

    counter=$((counter + 1))
    sleep "$INTERVAL"
done
