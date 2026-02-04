# HIQ Hardware Abstraction Layer (HAL)

## Overview

The Hardware Abstraction Layer (HAL) provides a unified interface for quantum backends, enabling portable code that can run on different quantum hardware without modification.

## Backend Trait

The core abstraction for quantum computing backends.

```rust
#[async_trait]
pub trait Backend: Send + Sync {
    /// Get the backend name.
    fn name(&self) -> &str;

    /// Get backend capabilities.
    async fn capabilities(&self) -> HalResult<Capabilities>;

    /// Check if the backend is available.
    async fn is_available(&self) -> HalResult<bool>;

    /// Submit a circuit for execution.
    async fn submit(&self, circuit: &Circuit, shots: u32) -> HalResult<JobId>;

    /// Get the status of a job.
    async fn status(&self, job_id: &JobId) -> HalResult<JobStatus>;

    /// Get the result of a completed job.
    async fn result(&self, job_id: &JobId) -> HalResult<ExecutionResult>;

    /// Cancel a job.
    async fn cancel(&self, job_id: &JobId) -> HalResult<()>;

    /// Wait for a job to complete (default implementation polls).
    async fn wait(&self, job_id: &JobId) -> HalResult<ExecutionResult>;
}
```

## Backend Configuration

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendConfig {
    /// Backend name/identifier.
    pub name: String,
    /// API endpoint URL.
    pub endpoint: Option<String>,
    /// Authentication token.
    #[serde(skip_serializing)]
    pub token: Option<String>,
    /// Additional configuration.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}
```

### Configuration Examples

**IQM Resonance (Cloud):**
```yaml
name: iqm-resonance
endpoint: https://cocos.resonance.meetiqm.com
token: ${IQM_RESONANCE_TOKEN}
```

**IQM LUMI (On-Premise):**
```yaml
name: iqm-lumi
endpoint: https://qpu.lumi.csc.fi
auth_method: oidc
oidc_provider: https://auth.csc.fi
```

**IBM Quantum:**
```yaml
name: ibm-quantum
endpoint: https://api.quantum-computing.ibm.com
token: ${IBM_QUANTUM_TOKEN}
hub: ibm-q
group: open
project: main
```

## Capabilities

Description of backend capabilities.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    /// Backend name.
    pub name: String,
    /// Number of qubits.
    pub num_qubits: u32,
    /// Supported gate set.
    pub gate_set: GateSet,
    /// Qubit connectivity.
    pub topology: Topology,
    /// Maximum number of shots per job.
    pub max_shots: u32,
    /// Whether the backend is a simulator.
    pub is_simulator: bool,
    /// Backend-specific features.
    pub features: Vec<String>,
}
```

### GateSet

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateSet {
    /// Single-qubit gates.
    pub single_qubit: Vec<String>,
    /// Two-qubit gates.
    pub two_qubit: Vec<String>,
    /// Native gate set (for transpilation target).
    pub native: Vec<String>,
}

impl GateSet {
    /// IQM native gate set.
    pub fn iqm() -> Self {
        Self {
            single_qubit: vec!["prx".into()],
            two_qubit: vec!["cz".into()],
            native: vec!["prx".into(), "cz".into(), "measure".into()],
        }
    }

    /// IBM native gate set.
    pub fn ibm() -> Self {
        Self {
            single_qubit: vec!["id".into(), "rz".into(), "sx".into(), "x".into()],
            two_qubit: vec!["cx".into()],
            native: vec!["id".into(), "rz".into(), "sx".into(), "x".into(), "cx".into(), "measure".into()],
        }
    }
}
```

### Topology

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Topology {
    pub kind: TopologyKind,
    pub edges: Vec<(u32, u32)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TopologyKind {
    FullyConnected,
    Linear,
    Star,
    Grid { rows: u32, cols: u32 },
    Custom,
}
```

**Common Topologies:**

| Type | Description | Example Devices |
|------|-------------|-----------------|
| Linear | Chain: 0-1-2-3-... | Small test devices |
| Star | Central qubit connected to all | IQM 5-qubit |
| Grid | 2D lattice | IBM heavy-hex |
| FullyConnected | All-to-all | IonQ (trapped ions) |

## Job Management

### JobId

Unique identifier for submitted jobs.

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JobId(pub String);
```

### JobStatus

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed(String),
    Cancelled,
}

impl JobStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed(_) | Self::Cancelled)
    }
}
```

### Job Lifecycle

```
┌─────────┐     ┌─────────┐     ┌───────────┐
│ Queued  │────▶│ Running │────▶│ Completed │
└─────────┘     └─────────┘     └───────────┘
     │               │
     │               │          ┌──────────┐
     │               └─────────▶│  Failed  │
     │                          └──────────┘
     │
     │                          ┌───────────┐
     └─────────────────────────▶│ Cancelled │
                                └───────────┘
```

## Execution Results

### Counts

Measurement counts from circuit execution.

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Counts {
    counts: HashMap<String, u64>,
}

impl Counts {
    pub fn insert(&mut self, bitstring: impl Into<String>, count: u64);
    pub fn get(&self, bitstring: &str) -> u64;
    pub fn iter(&self) -> impl Iterator<Item = (&String, &u64)>;
    pub fn total_shots(&self) -> u64;
    pub fn most_frequent(&self) -> Option<(&String, &u64)>;
    pub fn probabilities(&self) -> HashMap<String, f64>;
}
```

### ExecutionResult

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Measurement counts.
    pub counts: Counts,
    /// Number of shots executed.
    pub shots: u32,
    /// Execution time in milliseconds (if available).
    pub execution_time_ms: Option<u64>,
    /// Backend-specific metadata.
    pub metadata: serde_json::Value,
}
```

### Example Usage

```rust
let result = backend.wait(&job_id).await?;

// Get counts
for (bitstring, count) in result.counts.iter() {
    println!("{}: {}", bitstring, count);
}

// Get probabilities
let probs = result.counts.probabilities();
println!("P(00) = {}", probs.get("00").unwrap_or(&0.0));

// Most frequent outcome
if let Some((bitstring, count)) = result.counts.most_frequent() {
    println!("Most frequent: {} ({} times)", bitstring, count);
}
```

## Error Handling

```rust
#[derive(Debug, Error)]
pub enum HalError {
    #[error("Backend not available: {0}")]
    BackendUnavailable(String),

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Job submission failed: {0}")]
    SubmissionFailed(String),

    #[error("Job failed: {0}")]
    JobFailed(String),

    #[error("Job cancelled")]
    JobCancelled,

    #[error("Job not found: {0}")]
    JobNotFound(String),

    #[error("Invalid circuit: {0}")]
    InvalidCircuit(String),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Configuration error: {0}")]
    Configuration(String),
}

pub type HalResult<T> = Result<T, HalError>;
```

## Backend Implementations

### IQM Adapter

Located in `adapters/hiq-adapter-iqm/`.

**Features:**
- IQM Resonance (cloud) support
- On-premise support (LUMI, LRZ)
- OIDC authentication for HPC centers
- Calibration set pinning

**Configuration:**
```rust
pub struct IqmConfig {
    pub endpoint: String,
    pub auth_method: AuthMethod,
    pub calibration_set: String,
    pub timeout: Duration,
}

pub enum AuthMethod {
    ApiToken(String),
    Oidc { provider: String, client_id: String },
}
```

**Native Gates:**
| Gate | Parameters | Description |
|------|------------|-------------|
| PRX | θ, φ | Phased X rotation |
| CZ | - | Controlled-Z |
| Measure | - | Measurement |

### IBM Adapter

Located in `adapters/hiq-adapter-ibm/`.

**Features:**
- IBM Quantum cloud access
- Multiple backend selection
- Dynamic circuits support (planned)

**Configuration:**
```rust
pub struct IbmConfig {
    pub token: String,
    pub hub: String,
    pub group: String,
    pub project: String,
    pub backends: Vec<String>,
}
```

**Native Gates:**
| Gate | Parameters | Description |
|------|------------|-------------|
| ID | - | Identity |
| RZ | θ | Z rotation |
| SX | - | √X gate |
| X | - | Pauli X |
| CX | - | CNOT |
| Measure | - | Measurement |

### Simulator Adapter

Located in `adapters/hiq-adapter-sim/`.

**Features:**
- Local state vector simulation
- No external dependencies
- Ideal for development/testing

**Limitations:**
- Limited qubit count (depends on memory)
- No noise model (initially)

## Backend Factory

For constructing backends from configuration.

```rust
pub trait BackendFactory: Backend + Sized {
    fn from_config(config: BackendConfig) -> HalResult<Self>;
}

// Usage
let config = BackendConfig::from_file("iqm.yaml")?;
let backend = IqmBackend::from_config(config)?;
```

## Backend Registry

Managing multiple backends.

```rust
pub struct BackendRegistry {
    backends: HashMap<String, Box<dyn Backend>>,
}

impl BackendRegistry {
    pub fn register(&mut self, name: &str, backend: impl Backend + 'static);
    pub fn get(&self, name: &str) -> Option<&dyn Backend>;
    pub fn list(&self) -> Vec<&str>;
}
```

## Example: Complete Workflow

```rust
use hiq_ir::Circuit;
use hiq_compile::{PassManagerBuilder, PropertySet, CouplingMap, BasisGates};
use hiq_hal::{Backend, BackendConfig};
use hiq_adapter_iqm::IqmBackend;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create circuit
    let circuit = Circuit::bell()?;

    // Setup compilation for IQM
    let capabilities = backend.capabilities().await?;
    let properties = PropertySet::new()
        .with_target(
            CouplingMap::from_topology(&capabilities.topology),
            BasisGates::from_gate_set(&capabilities.gate_set),
        );

    let (pm, mut props) = PassManagerBuilder::new()
        .with_optimization_level(2)
        .with_properties(properties)
        .build();

    // Compile
    let mut dag = circuit.clone().into_dag();
    pm.run(&mut dag, &mut props)?;
    let compiled = Circuit::from_dag(dag);

    // Create backend
    let config = BackendConfig {
        name: "iqm-resonance".into(),
        endpoint: Some("https://cocos.resonance.meetiqm.com".into()),
        token: std::env::var("IQM_TOKEN").ok(),
        extra: Default::default(),
    };
    let backend = IqmBackend::from_config(config)?;

    // Check availability
    if !backend.is_available().await? {
        anyhow::bail!("Backend not available");
    }

    // Submit job
    let job_id = backend.submit(&compiled, 1024).await?;
    println!("Submitted job: {}", job_id);

    // Wait for result
    let result = backend.wait(&job_id).await?;

    // Print results
    println!("Results ({} shots):", result.shots);
    for (bitstring, count) in result.counts.iter() {
        let prob = *count as f64 / result.shots as f64;
        println!("  {}: {} ({:.2}%)", bitstring, count, prob * 100.0);
    }

    Ok(())
}
```

## Authentication

### API Token

Standard for cloud services.

```rust
// Environment variable
let token = std::env::var("IQM_TOKEN")?;

// Or from config file (not recommended for production)
let config = BackendConfig::from_file("backend.yaml")?;
```

### OIDC (HPC Centers)

For on-premise installations at HPC centers.

```rust
pub struct OidcConfig {
    pub provider: String,      // e.g., https://auth.csc.fi
    pub client_id: String,
    pub scopes: Vec<String>,
}

// LUMI example
let oidc = OidcConfig {
    provider: "https://auth.csc.fi".into(),
    client_id: "hiq-client".into(),
    scopes: vec!["openid".into(), "quantum".into()],
};
```

## Performance Considerations

1. **Connection Pooling** — Reuse HTTP connections
2. **Async I/O** — Non-blocking backend operations
3. **Batch Submission** — Submit multiple circuits in one request (where supported)
4. **Result Caching** — Cache immutable job results
5. **Timeout Handling** — Configurable timeouts for long-running jobs
