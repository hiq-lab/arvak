# HPC Deployment Guide

Arvak submits quantum jobs through HPC batch schedulers (SLURM and
PBS/Torque) so that QPU access follows the same workflow as classical
HPC workloads: `arvak submit` renders a batch script, the scheduler
queues it, and Arvak tracks the job through to results.

All CLI flags shown here are real — regenerate the full reference with
[`scripts/gen-cli-docs.sh`](../scripts/gen-cli-docs.sh) or check
`arvak submit --help`.

## Installation on a login node

```bash
# Python API
pip install --user arvak

# CLI from source (hal-contract is a workspace path dependency)
git clone https://github.com/hiq-lab/arvak
git clone https://github.com/hiq-lab/hal-contract
cd arvak && ln -s ../hal-contract .hal-contract
cargo install --path crates/arvak-cli
```

## Authentication

OIDC login for supported HPC providers:

```bash
arvak auth login --provider csc      # csc, lumi, lrz
arvak auth status
arvak auth logout
```

Vendor tokens are read from environment variables:

| Variable | Backend |
|----------|---------|
| `IQM_TOKEN` | IQM (Resonance / on-premise) |
| `IBM_QUANTUM_TOKEN`, `IBM_API_KEY`, `IBM_SERVICE_CRN` | IBM Quantum |
| `ARVAK_STATE_DIR` | Override scheduler state directory (default: `$XDG_RUNTIME_DIR/arvak-scheduler`) |

Never hardcode tokens in job scripts; export them in your shell profile
or use the OIDC flow.

## Submitting jobs

Compile locally first to catch errors before queueing:

```bash
arvak compile -i circuit.qasm --target iqm \
    --optimization-level 2 -o compiled.qasm
```

Submit through the scheduler:

```bash
arvak submit -i compiled.qasm --backend iqm \
    --scheduler slurm \
    --partition q_fiqci \
    --account project_462000xxx \
    --time "00:30:00" \
    --shots 1024
```

Track and collect:

```bash
arvak status <job-id>            # single job
arvak status --all               # everything you submitted
arvak wait <job-id>              # block until terminal state
arvak result <job-id> --format json > results.json
```

`--wait` on `submit` combines submit + wait. `--scheduler pbs` targets
PBS/Torque; `--priority low|default|high|critical` sets queue priority.

## Site notes

### LUMI (CSC, Finland)

IQM QPU behind the `q_fiqci` SLURM partition:

```bash
arvak auth login --provider csc --project project_462000xxx
arvak submit -i circuit.qasm --backend iqm \
    --scheduler slurm --partition q_fiqci \
    --account project_462000xxx --time "00:15:00" --wait
```

### LRZ (Germany)

Same workflow with `--provider lrz` and your local partition/account
names. Ask your HPC support desk for the quantum partition name.

## Programmatic use (Rust)

The `arvak-sched` crate exposes the same functionality as a library —
scheduler configuration, batch/array jobs, persistence (JSON/SQLite),
and resource matching. See the crate docs
([`crates/arvak-sched`](../crates/arvak-sched)) for the API; the CLI
above is a thin wrapper over it.

## Troubleshooting

| Symptom | Check |
|---------|-------|
| `Authentication failed` | `arvak auth status`; token env var set and unexpired? |
| `Invalid partition` | `sinfo -a` — partition names are site-specific |
| Job pending forever | `squeue -u $USER`; QPU partitions often have narrow time windows |
| Job exceeded walltime | Raise `--time`, or compile with a higher `--optimization-level` to shrink the circuit |

Verbose logging for bug reports:

```bash
arvak -vvv submit -i circuit.qasm --backend iqm ...
```

## Security

- Tokens only via environment variables or OIDC — never in job scripts
  or shared filesystems.
- Result files land in the submission directory; mind permissions on
  shared storage.
