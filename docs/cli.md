# CLI Reference

> Generated from `arvak --help` by [`scripts/gen-cli-docs.sh`](../scripts/gen-cli-docs.sh).
> Do not edit by hand — rerun the script after CLI changes.

```text
Arvak command-line interface

Usage: arvak [OPTIONS] <COMMAND>

Commands:
  compile   Compile a quantum circuit for a target backend
  run       Run a circuit on a backend
  submit    Submit a circuit to an HPC batch scheduler
  status    Query job status
  result    Retrieve results for a completed job
  auth      Manage authentication for HPC providers
  wait      Wait for a job to complete
  eval      Evaluate a circuit: compilation observability, QDMI contract check, metrics
  backends  List available backends
  version   Show version information
  help      Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose...  Increase verbosity (-v, -vv, -vvv)
  -h, --help        Print help
  -V, --version     Print version
```

## arvak compile

```text
Compile a quantum circuit for a target backend

Usage: arvak compile [OPTIONS] --input <INPUT>

Options:
  -i, --input <INPUT>                            Input file (QASM3 or JSON)
  -v, --verbose...                               Increase verbosity (-v, -vv, -vvv)
  -o, --output <OUTPUT>                          Output file
  -t, --target <TARGET>                          Target backend (iqm, ibm, simulator) [default: iqm]
      --optimization-level <OPTIMIZATION_LEVEL>  Optimization level (0-3) [default: 1]
  -h, --help                                     Print help
```

## arvak run

```text
Run a circuit on a backend

Usage: arvak run [OPTIONS] --input <INPUT>

Options:
  -i, --input <INPUT>      Input file (QASM3 or JSON)
  -v, --verbose...         Increase verbosity (-v, -vv, -vvv)
  -s, --shots <SHOTS>      Number of shots [default: 1024]
  -b, --backend <BACKEND>  Backend to use [default: simulator]
      --compile            Compile before running
      --target <TARGET>    Target for compilation
  -h, --help               Print help
```

## arvak submit

```text
Submit a circuit to an HPC batch scheduler

Usage: arvak submit [OPTIONS] --input <INPUT>

Options:
  -i, --input <INPUT>          Input file (QASM3 or JSON)
  -v, --verbose...             Increase verbosity (-v, -vv, -vvv)
  -b, --backend <BACKEND>      Backend to use (simulator, iqm, ibm) [default: simulator]
  -s, --shots <SHOTS>          Number of shots [default: 1024]
      --scheduler <SCHEDULER>  Batch scheduler (slurm, pbs) [default: slurm]
      --partition <PARTITION>  Scheduler partition/queue name
      --account <ACCOUNT>      Scheduler account/project
      --time <TIME>            Wall time limit (HH:MM:SS)
      --priority <PRIORITY>    Job priority (low, default, high, critical)
  -w, --wait                   Wait for job to complete
  -h, --help                   Print help
```

## arvak status

```text
Query job status

Usage: arvak status [OPTIONS] [JOB_ID]

Arguments:
  [JOB_ID]  Job ID (UUID)

Options:
  -a, --all         List all jobs
  -v, --verbose...  Increase verbosity (-v, -vv, -vvv)
  -h, --help        Print help
```

## arvak result

```text
Retrieve results for a completed job

Usage: arvak result [OPTIONS] <JOB_ID>

Arguments:
  <JOB_ID>  Job ID (UUID)

Options:
  -f, --format <FORMAT>  Output format (table, json) [default: table]
  -v, --verbose...       Increase verbosity (-v, -vv, -vvv)
  -h, --help             Print help
```

## arvak auth

```text
Manage authentication for HPC providers

Usage: arvak auth [OPTIONS] <COMMAND>

Commands:
  login   Log in to an HPC provider
  status  Show authentication status
  logout  Log out and clear cached tokens
  help    Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose...  Increase verbosity (-v, -vv, -vvv)
  -h, --help        Print help
```

## arvak wait

```text
Wait for a job to complete

Usage: arvak wait [OPTIONS] <JOB_ID>

Arguments:
  <JOB_ID>  Job ID (UUID)

Options:
  -t, --timeout <TIMEOUT>  Timeout in seconds [default: 86400]
  -v, --verbose...         Increase verbosity (-v, -vv, -vvv)
  -h, --help               Print help
```

## arvak eval

```text
Evaluate a circuit: compilation observability, QDMI contract check, metrics

Usage: arvak eval [OPTIONS] --input <INPUT>

Options:
  -i, --input <INPUT>
          Input file (QASM3)
  -v, --verbose...
          Increase verbosity (-v, -vv, -vvv)
  -p, --profile <PROFILE>
          Evaluation profile [default: default]
  -t, --target <TARGET>
          Target backend (iqm, ibm, simulator) [default: iqm]
      --optimization-level <OPTIMIZATION_LEVEL>
          Optimization level (0-3) [default: 1]
      --target-qubits <TARGET_QUBITS>
          Number of qubits on target device [default: 20]
  -e, --export <EXPORT>
          Output file for JSON report (stdout if omitted)
      --orchestration
          Include orchestration analysis (hybrid DAG, batchability, critical path)
      --scheduler-site <SCHEDULER_SITE>
          HPC scheduler site for constraints (lrz, lumi)
      --emit <EMIT>
          Emitter compliance target (iqm, ibm, cuda-q)
      --benchmark <BENCHMARK>
          Optional benchmark workload (ghz, qft, grover, random)
      --benchmark-qubits <BENCHMARK_QUBITS>
          Number of qubits for benchmark circuit (defaults to input circuit size)
  -h, --help
          Print help
```

## arvak backends

```text
List available backends

Usage: arvak backends [OPTIONS]

Options:
  -v, --verbose...  Increase verbosity (-v, -vv, -vvv)
  -h, --help        Print help
```
