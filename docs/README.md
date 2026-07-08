# Arvak Documentation

## Using Arvak

| Doc | What it covers |
|-----|----------------|
| [Quick Start](quickstart.md) | Install, first circuit in Python / CLI / Rust, hardware submission |
| [Python API Reference](python-api.md) | `Circuit`, `run_sim`, `compile`, backends, QASM 3 I/O, integrations |
| [CLI Reference](cli.md) | All `arvak` commands and flags (generated from `--help`) |
| [HPC Deployment](hpc-deployment.md) | SLURM/PBS submission, OIDC auth, LUMI/LRZ |

## Extending Arvak

| Doc | What it covers |
|-----|----------------|
| [Integration Guide](INTEGRATION_GUIDE.md) | Adding a framework integration (Qiskit/Qrisp/Cirq/PennyLane pattern) |
| [HAL Contract](hal-contract.md) | The backend abstraction every adapter implements |
| [HAL Specification](hal-specification.md) | Formal HAL semantics |

## Internals

| Doc | What it covers |
|-----|----------------|
| [Architecture](architecture.md) | Crate layout, data flow, design decisions |
| [Compilation Pipeline](compilation.md) | Passes, optimization levels, layout/routing/translation |
| [IR Specification](ir-specification.md) | Circuit DAG representation |
| [Code Specification](code-specification.md) | Conventions for contributors |
| [Release Process](release-process.md) | Versioning, tagging, PyPI publishing |

## How this documentation stays correct

- **Python snippets are CI-tested.** Every ` ```python ` block in
  `quickstart.md` and `python-api.md` is executed by
  [`test_doc_snippets.py`](../crates/arvak-python/tests/test_doc_snippets.py)
  in the Python Bindings CI job. Blocks that need credentials or optional
  extras carry a `# doc-test: skip` marker with the reason.
- **The CLI reference is generated**, not written: rerun
  [`scripts/gen-cli-docs.sh`](../scripts/gen-cli-docs.sh) after CLI changes.
- **Keep prose thin.** Reference the code for details that would go stale;
  document behavior, defaults, and intent — not signatures the docstrings
  already carry.
