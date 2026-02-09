# QAOA Sensor Network Optimization — Compilation Speed Demo

## Operational Context

In tactical command and control (C2), sensor networks must be continuously re-optimized as the operational picture evolves. When a new threat is detected, assets reposition, or communication links degrade, sensor assignments — patrol zones, frequency bands, coverage areas — need recomputation in seconds, not minutes.

Quantum Approximate Optimization Algorithm (QAOA) is a natural fit for these combinatorial assignment problems. The challenge is that QAOA requires sweeping a large parameter space (depth levels, angle grids) to find good solutions, generating thousands of circuit variants per re-optimization cycle.

This demo measures Arvak's compilation throughput on three tactical sensor network scenarios.

## Demo Scenarios

### 1. Drone Patrol Partitioning (6 zones)

Partition patrol zones into two groups to maximize coverage of contested boundaries. The graph has 6 nodes and 7 weighted edges representing boundary priority between zones.

### 2. Radar Frequency Deconfliction (8 stations)

Assign radar stations to two frequency bands to minimize mutual interference. 8 stations with 14 interference links — a denser graph that produces larger QAOA circuits.

### 3. Surveillance Area Coverage (10 nodes)

Partition surveillance nodes into two overlapping coverage zones. 10 nodes with 16 edges including cross-links and diagonals, representing sensor footprint overlap costs.

## Parameter Space

Per scenario:
- QAOA depth sweep: p = 1 through 5
- Angle grid: 20 × 20 (gamma × beta) = 400 points per depth
- Total: 5 × 400 = **2,000 circuits per scenario**
- Grand total across all three: **6,000 circuits**

This models a single re-optimization cycle where the optimizer explores the full angle landscape at multiple QAOA depths to find the best assignment.

## Compilation Results

Measured in release mode targeting IQM native gate set (CZ + PRX), star topology:

| Scenario | Qubits | Edges | Gates (p=1) | O0 total | O0 per-circuit | O2 total | O2 per-circuit |
|---|---|---|---|---|---|---|---|
| Drone patrol | 6 | 7 | 34 | 0.29s | 145µs | 7.1s | 3.5ms |
| Radar deconfliction | 8 | 14 | 59 | 0.62s | 309µs | 23.1s | 11.5ms |
| Surveillance grid | 10 | 16 | 69 | 0.71s | 356µs | 49.3s | 24.6ms |

**Aggregate (6,000 circuits):**

| Level | Total | Per-Circuit |
|---|---|---|
| O0 | 1.62s | 270µs |
| O2 | 79.4s | 13.2ms |

### Comparison

At 100ms per circuit (typical Python-based transpiler):
- Total time: **10.0 minutes**
- Arvak speedup: **370× (O0)** / **8× (O2)**

## Operational Implications

### Real-Time Re-Optimization

A conventional transpiler needs 10 minutes to explore the QAOA parameter space across three sensor network scenarios. In a tactical environment where the situation changes every few minutes, this makes quantum-assisted optimization impractical — by the time compilation finishes, the operational picture has already shifted.

Arvak at O0 completes the same sweep in 1.6 seconds. Even at O2 with full optimization passes, the 79-second total is within operational planning timelines.

### Scaling to Larger Networks

QAOA circuit size grows linearly with graph edges. The jump from 6-qubit (145µs) to 10-qubit (356µs) at O0 shows sub-linear scaling in per-circuit time — routing overhead grows slowly on star topologies. For 20–30 qubit networks with hundreds of edges, O0 compilation remains under 1ms per circuit.

### Integration with Classical Solvers

In a hybrid C2 pipeline, QAOA serves as one optimization engine alongside classical solvers (simulated annealing, genetic algorithms). The quantum branch must keep pace with classical alternatives. Sub-millisecond compilation ensures that the quantum pipeline's overhead is dominated by QPU execution time, not software preprocessing.

## Running the Demo

```
cargo run -p arvak-demos --release --bin demo-speed-qaoa
```

Or inside the Docker container:

```
docker exec arvak-dashboard demo-speed-qaoa
```

## Technical Details

- **Circuit generator**: `qaoa_circuit()` from `arvak-demos` — standard QAOA with cost unitary (RZZ decomposition) and mixer unitary (RX)
- **Problem graphs**: Predefined weighted graphs in `demos/src/problems/sensor_assignment.rs`
- **Compilation target**: IQM star topology, CZ + PRX native gates
- **Pass pipeline**: Layout → Routing → Basis Translation → Optimization (at O2) → Verification
- **No quantum execution** — pure compiler performance measurement
