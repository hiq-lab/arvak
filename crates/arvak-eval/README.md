# arvak-eval

Compiler, orchestration, and emitter observability for quantum circuits.

`arvak-eval` is the evaluation framework within the [Arvak](../../README.md) quantum compiler toolkit. It observes and analyzes quantum circuit compilation pipelines, producing structured JSON reports covering input analysis, compilation deltas, QDMI contract compliance, orchestration insights, and emitter materialization coverage.

## Features

| Module | Description | Flag |
|--------|-------------|------|
| **Input Analysis** | Parse QASM3, compute structural metrics (qubits, depth, gate counts), content hash | always |
| **Compilation Observer** | Record per-pass metrics with before/after deltas | always |
| **QDMI Contract Checker** | Classify every gate as Safe / Conditional / Violating against device capabilities | always |
| **Orchestration** | Build hybrid quantum-classical DAG, critical path analysis, batchability | `--orchestration` |
| **Scheduler Context** | LRZ / LUMI walltime estimation, batch capacity, fitness scoring | `--orchestration` |
| **Emitter Compliance** | Native gate coverage, decomposition costs, loss documentation | `--emit <target>` |
| **Benchmark Loader** | Generate standard circuit workloads (GHZ, QFT, Grover, Random) | `--benchmark <suite>` |
| **Metrics Aggregation** | Unified compilation + orchestration + emitter deltas | always |
| **Reproducibility** | CLI snapshot, crate version, content hash, timestamps | always |

## Pipeline

```text
[QASM3 Input]
     |
     v
Input Analysis ──> structural metrics, content hash
     |
     v
Compilation Observer ──> per-pass snapshots, before/after deltas
     |
     v
QDMI Contract Checker ──> Safe / Conditional / Violating per gate
     |
     v
Orchestration (opt) ──> hybrid DAG, critical path, batchability
     |
     v
Emitter Compliance (opt) ──> native coverage, decomposition costs, losses
     |
     v
Metrics Aggregator ──> unified deltas
     |
     v
JSON Report (schema 0.3.0)
```

## CLI Usage

```bash
# Basic evaluation against IQM target
arvak eval --input circuit.qasm3 --target iqm

# With orchestration and LRZ scheduler context
arvak eval --input circuit.qasm3 --target iqm --orchestration --scheduler-site lrz

# With emitter compliance for IBM backend
arvak eval --input circuit.qasm3 --target ibm --emit ibm

# Full pipeline with benchmark reference
arvak eval --input circuit.qasm3 --target iqm \
    --orchestration --scheduler-site lrz \
    --emit iqm --benchmark ghz --benchmark-qubits 5

# Write report to file
arvak eval --input circuit.qasm3 --target iqm -o report.json
```

## Library Usage

```rust
use arvak_eval::{EvalConfig, Evaluator};

let config = EvalConfig {
    target: "iqm".into(),
    target_qubits: 20,
    optimization_level: 2,
    orchestration: true,
    scheduler_site: Some("lrz".into()),
    emit_target: Some("iqm".into()),
    ..Default::default()
};

let evaluator = Evaluator::new(config);
let report = evaluator.evaluate(qasm3_source, &cli_args)?;

// Access report fields
println!("Depth: {} -> {}", report.compilation.initial.depth, report.compilation.final_snapshot.depth);
println!("Contract: {}", if report.contract.compliant { "COMPLIANT" } else { "NON-COMPLIANT" });

if let Some(emitter) = &report.emitter {
    println!("Native coverage: {:.0}%", emitter.coverage.native_coverage * 100.0);
}

// Export to JSON
let json = arvak_eval::export::to_json(&report, &config.export)?;
```

## Targets

| Target | Native Gates | Scheduler | Notes |
|--------|-------------|-----------|-------|
| `iqm` | PRX, CZ, ID | LRZ (20q), LUMI (5q) | Star topology, IQM Garnet |
| `ibm` | SX, RZ, CX, ID, X | - | Linear topology |
| `simulator` | Universal | Local | Full connectivity |
| `cuda-q` | Universal (30+ gates) | - | CUDA-Q / NVIDIA |

## QDMI Contract Tags

Every gate in the compiled circuit receives a safety classification:

- **Safe**: Gate is natively supported by the target device
- **Conditional**: Gate can be decomposed into native gates (adds overhead)
- **Violating**: Gate cannot be executed on the target (contract violation)

A circuit is **compliant** when `violating_count == 0`.

## Emitter Materialization

When `--emit` is specified, the evaluator analyzes how gates map to the target's native gate set:

- **Native**: Direct hardware execution, no overhead
- **Decomposed**: Requires decomposition (cost documented per gate type)
- **Lost**: Cannot be materialized (circuit is not fully materializable)

Coverage metrics include native coverage ratio, materializable coverage ratio, and estimated gate expansion factor.

## JSON Report Schema (v0.3.0)

```json
{
  "schema_version": "0.3.0",
  "timestamp": "2025-01-15T10:30:00Z",
  "profile": "default",
  "input": { "num_qubits": 2, "total_ops": 4, "depth": 3, "content_hash": "..." },
  "compilation": { "passes": [...], "initial": {...}, "final_snapshot": {...} },
  "contract": { "compliant": true, "safe_count": 4, "conditional_count": 0, "violating_count": 0 },
  "metrics": { "compilation_effect": {...}, "compliance": {...} },
  "orchestration": { "...if --orchestration" },
  "scheduler": { "...if --orchestration" },
  "emitter": { "...if --emit" },
  "benchmark": { "...if --benchmark" },
  "reproducibility": { "arvak_version": "1.4.0", "cli_args": [...] }
}
```

Optional sections (`orchestration`, `scheduler`, `emitter`, `benchmark`) are omitted from the JSON when their respective flags are not used.

## Testing

```bash
cargo test -p arvak-eval
```

62 unit and integration tests covering all modules.

## Version History

| Version | Features |
|---------|----------|
| 0.1.0 | Input analysis, compilation observer, QDMI contract checker, metrics, JSON export |
| 0.2.0 | Orchestration (hybrid DAG, critical path, batchability), scheduler context (LRZ/LUMI) |
| 0.3.0 | Emitter compliance (IQM/IBM/CUDA-Q), benchmark loader (GHZ/QFT/Grover/Random) |
