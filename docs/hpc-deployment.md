# HIQ HPC Deployment Guide

## Overview

HIQ is designed for deployment in High-Performance Computing (HPC) environments, with first-class support for job schedulers like Slurm and PBS. This guide covers deployment at HPC centers with quantum computing resources.

## Architecture in HPC Context

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              HPC Center                                      │
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                         Login Node                                   │   │
│  │  ┌─────────────┐     ┌─────────────┐     ┌─────────────────────┐   │   │
│  │  │  User CLI   │────▶│  HIQ Core   │────▶│  Scheduler Adapter  │   │   │
│  │  │  (hiq)      │     │             │     │  (Slurm/PBS)        │   │   │
│  │  └─────────────┘     └─────────────┘     └──────────┬──────────┘   │   │
│  └──────────────────────────────────────────────────────┼──────────────┘   │
│                                                         │                   │
│                                                         ▼                   │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │                      Job Scheduler (Slurm)                            │  │
│  │  ┌────────────────────────────────────────────────────────────────┐  │  │
│  │  │  Job Queue                                                      │  │  │
│  │  │  - quantum partition                                            │  │  │
│  │  │  - standard partitions                                          │  │  │
│  │  └────────────────────────────────────────────────────────────────┘  │  │
│  └──────────────────────────────────────────────────────┬───────────────┘  │
│                                                         │                   │
│                                                         ▼                   │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │                       Compute Node                                    │  │
│  │  ┌─────────────────┐                      ┌────────────────────┐     │  │
│  │  │   hiq-runner    │─────────────────────▶│   Quantum System   │     │  │
│  │  │                 │                      │   (IQM/IBM)        │     │  │
│  │  └─────────────────┘                      └────────────────────┘     │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Scheduler Integration

### Slurm Adapter

The Slurm adapter generates and submits sbatch scripts.

```rust
pub struct SlurmAdapter {
    config: SlurmConfig,
}

pub struct SlurmConfig {
    pub partition: String,
    pub account: String,
    pub default_walltime: Duration,
    pub qos: Option<String>,
    pub constraint: Option<String>,
}

#[async_trait]
impl Scheduler for SlurmAdapter {
    async fn submit(&self, job: &QuantumJob) -> SchedulerResult<SchedulerJobId>;
    async fn status(&self, job_id: &SchedulerJobId) -> SchedulerResult<SchedulerStatus>;
    async fn cancel(&self, job_id: &SchedulerJobId) -> SchedulerResult<()>;
    async fn output(&self, job_id: &SchedulerJobId) -> SchedulerResult<JobOutput>;
}
```

### Generated Slurm Script

```bash
#!/bin/bash
#SBATCH --job-name=hiq-quantum-job
#SBATCH --partition=q_fiqci
#SBATCH --account=project_462000xxx
#SBATCH --time=00:30:00
#SBATCH --nodes=1
#SBATCH --ntasks=1
#SBATCH --output=hiq_%j.out
#SBATCH --error=hiq_%j.err

# Load required modules
module load iqm-client

# Set environment
export HIQ_JOB_ID="${HIQ_JOB_ID}"
export HIQ_BACKEND="${HIQ_BACKEND}"

# Run the quantum job
hiq-runner --job-id="${HIQ_JOB_ID}"
```

### PBS Adapter

Similar interface for PBS Pro environments.

```rust
pub struct PbsAdapter {
    config: PbsConfig,
}

pub struct PbsConfig {
    pub queue: String,
    pub account: String,
    pub default_walltime: Duration,
}
```

## Site-Specific Configuration

### LUMI (CSC, Finland)

LUMI hosts IQM's Helmi quantum computer (5 qubits).

**Configuration:**
```yaml
# ~/.hiq/config.yaml
site: lumi
scheduler:
  type: slurm
  partition: q_fiqci
  account: project_462000xxx

backend:
  type: iqm
  endpoint: https://qpu.lumi.csc.fi
  auth_method: oidc
  oidc_provider: https://auth.csc.fi

defaults:
  walltime: "00:30:00"
  shots: 1024
```

**Module Setup:**
```bash
# Load HIQ module (if installed system-wide)
module load hiq

# Or use local installation
export PATH="$HOME/.local/bin:$PATH"
```

**Authentication:**
```bash
# OIDC authentication via CSC
hiq auth login --provider csc

# Or set token directly
export IQM_TOKEN="your-token-here"
```

### LRZ (Germany)

LRZ hosts IQM quantum systems.

**Configuration:**
```yaml
# ~/.hiq/config.yaml
site: lrz
scheduler:
  type: slurm
  partition: quantum
  account: your-project

backend:
  type: iqm
  endpoint: https://qpu.lrz.de
  auth_method: oidc
  oidc_provider: https://auth.lrz.de
```

### Generic HPC Site

For sites without pre-configured profiles:

```yaml
# ~/.hiq/config.yaml
site: custom
scheduler:
  type: slurm
  partition: default
  account: myaccount

backend:
  type: ibm
  endpoint: https://api.quantum-computing.ibm.com
  token: ${IBM_QUANTUM_TOKEN}
```

## Installation on HPC Systems

### Method 1: Pre-built Binary

```bash
# Download release binary
curl -LO https://github.com/hiq-project/hiq/releases/latest/download/hiq-linux-x86_64.tar.gz

# Extract
tar xzf hiq-linux-x86_64.tar.gz

# Install to user directory
mkdir -p ~/.local/bin
mv hiq hiq-runner ~/.local/bin/

# Add to PATH
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc
```

### Method 2: Build from Source

```bash
# Load build dependencies (site-specific)
module load rust/1.83

# Clone and build
git clone https://github.com/hiq-project/hiq
cd hiq
cargo build --release

# Install
cp target/release/hiq target/release/hiq-runner ~/.local/bin/
```

### Method 3: Environment Module

For system-wide installation, create a module file:

```lua
-- /opt/modulefiles/hiq/0.1.0.lua
whatis("HIQ: Rust-native quantum compilation stack")

local base = "/opt/hiq/0.1.0"

prepend_path("PATH", pathJoin(base, "bin"))
prepend_path("LD_LIBRARY_PATH", pathJoin(base, "lib"))

-- Load dependencies
depends_on("iqm-client")
```

## Usage Examples

### Basic Submission

```bash
# Submit a quantum job
hiq submit -i circuit.qasm \
    --backend iqm \
    --shots 1024 \
    --scheduler slurm \
    --partition q_fiqci \
    --account project_462000xxx \
    --time 00:30:00

# Output: Job submitted: hiq-12345 (Slurm job: 98765)
```

### Check Status

```bash
# HIQ job status
hiq status hiq-12345

# Or directly via Slurm
squeue -j 98765
```

### Retrieve Results

```bash
# Get results
hiq result hiq-12345 --format json > results.json

# Or as table
hiq result hiq-12345 --format table
```

### Batch Submission

```bash
# Submit multiple circuits
for circuit in circuits/*.qasm; do
    hiq submit -i "$circuit" --backend iqm --shots 1024
done
```

### Interactive Mode (Not Recommended)

For debugging only:

```bash
# Request interactive session
salloc --partition=q_fiqci --account=project_xxx --time=00:15:00

# Run directly
hiq run -i circuit.qasm --backend iqm --shots 100
```

## Job Workflow

### 1. Local Compilation

Compile circuit before submission to catch errors early.

```bash
# Compile for target
hiq compile -i circuit.qasm -o compiled.qasm --target iqm

# Verify
hiq validate compiled.qasm --backend iqm
```

### 2. Submit to Scheduler

```bash
hiq submit -i compiled.qasm \
    --backend iqm \
    --shots 1024 \
    --scheduler slurm
```

### 3. Job Execution

The scheduler runs `hiq-runner` on a compute node:

```
hiq-runner workflow:
1. Load job specification
2. Connect to quantum backend
3. Submit circuit
4. Poll for completion
5. Store results
6. Exit
```

### 4. Result Retrieval

```bash
# Wait for completion
hiq wait hiq-12345

# Get results
hiq result hiq-12345
```

## Advanced Configuration

### Resource Limits

```yaml
# ~/.hiq/config.yaml
limits:
  max_shots: 100000
  max_circuits_per_job: 100
  max_walltime: "02:00:00"
  max_concurrent_jobs: 10
```

### Retry Policy

```yaml
# ~/.hiq/config.yaml
retry:
  max_attempts: 3
  backoff_base: 5  # seconds
  backoff_max: 300  # seconds
```

### Offline Mode

For air-gapped compute nodes:

```yaml
# ~/.hiq/config.yaml
offline:
  enabled: true
  cache_dir: /scratch/hiq-cache
  sync_interval: 300  # seconds
```

## Troubleshooting

### Common Issues

**1. Authentication Failure**
```
Error: OIDC authentication failed
```
Solution: Refresh your authentication token:
```bash
hiq auth login --provider csc
```

**2. Partition Not Found**
```
Error: Invalid partition: q_fiqci
```
Solution: Check available partitions:
```bash
sinfo -a
```

**3. Backend Unavailable**
```
Error: Backend not available: iqm-lumi
```
Solution: Check backend status:
```bash
hiq backends --status
```

**4. Job Timeout**
```
Error: Job exceeded walltime
```
Solution: Increase walltime or reduce circuit complexity:
```bash
hiq submit -i circuit.qasm --time 01:00:00
```

### Debug Mode

```bash
# Enable verbose logging
hiq -vvv submit -i circuit.qasm --backend iqm

# Check job logs
cat hiq_98765.out
cat hiq_98765.err
```

### Support Channels

- HPC center support desk
- HIQ GitHub issues
- IQM/IBM support (for backend issues)

## Security Considerations

1. **Credential Storage** — Never store tokens in job scripts
2. **File Permissions** — Protect config files: `chmod 600 ~/.hiq/config.yaml`
3. **Shared Filesystems** — Be cautious with results on shared storage
4. **OIDC Tokens** — Use short-lived tokens when possible

## Performance Tips

1. **Compile Locally** — Compile circuits before submission to reduce node time
2. **Batch Jobs** — Submit multiple circuits in one job where possible
3. **Result Streaming** — Use callbacks for large result sets
4. **Partition Selection** — Use appropriate partition for job size
5. **Off-Peak Hours** — Queue times are shorter during off-peak hours
