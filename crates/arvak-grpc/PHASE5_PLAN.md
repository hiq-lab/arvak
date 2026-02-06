# Arvak gRPC Service - Phase 5 Implementation Plan
## Production Security & Resilience

## Overview

Phase 5 transforms Arvak gRPC into a production-grade service with enterprise security, high availability, and advanced job scheduling. This phase builds upon the solid foundation of Phases 1-4 to enable secure multi-tenant deployments, distributed execution, and job persistence.

## Architecture Summary

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Phase 5: Production Architecture                  │
├─────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │  TLS/SSL Layer (NEW)                                          │  │
│  │  - Server certificates (Let's Encrypt, custom CA)            │  │
│  │  - mTLS client certificate validation                        │  │
│  │  - Certificate rotation and renewal                          │  │
│  └──────────────────────────────────────────────────────────────┘  │
│                              ▼                                       │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │  Authentication Layer (NEW)                                   │  │
│  │  - API Key validator (header-based)                          │  │
│  │  - JWT token validator (OAuth/OIDC)                          │  │
│  │  - mTLS certificate extractor                                │  │
│  │  - Client identity propagation                               │  │
│  └──────────────────────────────────────────────────────────────┘  │
│                              ▼                                       │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │  Authorization Layer (NEW)                                    │  │
│  │  - Role-based access control (RBAC)                          │  │
│  │  - Backend access policies                                    │  │
│  │  - Quota enforcement per client                              │  │
│  └──────────────────────────────────────────────────────────────┘  │
│                              ▼                                       │
│  │  Existing: Middleware + Interceptors + Service               │  │
│                              ▼                                       │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │  Distributed Job Queue (NEW)                                  │  │
│  │  - Redis-backed shared queue (multi-node)                    │  │
│  │  - Priority queues (high/normal/low)                         │  │
│  │  - Job persistence and recovery                              │  │
│  │  - Leader election for scheduling                            │  │
│  └──────────────────────────────────────────────────────────────┘  │
│                              ▼                                       │
│  ┌─────────────┬────────────────┬──────────────┬────────────────┐  │
│  │ JobStore    │ BackendRegistry│ AuthManager  │ SchedulerState │  │
│  │ (Postgres)  │ (Multi-backend)│ (Policies)   │ (Distributed)  │  │
│  └─────────────┴────────────────┴──────────────┴────────────────┘  │
│                                                                       │
└─────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────┐
│                       Multi-Node Deployment                          │
├─────────────────────────────────────────────────────────────────────┤
│                                                                       │
│   ┌───────────────┐    ┌───────────────┐    ┌───────────────┐     │
│   │ gRPC Node 1   │    │ gRPC Node 2   │    │ gRPC Node 3   │     │
│   │ (Leader)      │    │ (Follower)    │    │ (Follower)    │     │
│   └───────┬───────┘    └───────┬───────┘    └───────┬───────┘     │
│           │                    │                    │               │
│           └────────────────────┼────────────────────┘               │
│                                ▼                                     │
│                    ┌──────────────────────┐                         │
│                    │   Shared Redis       │                         │
│                    │   - Job queue        │                         │
│                    │   - Locks            │                         │
│                    │   - Leader election  │                         │
│                    └──────────┬───────────┘                         │
│                               │                                      │
│                    ┌──────────▼───────────┐                         │
│                    │   PostgreSQL         │                         │
│                    │   - Job history      │                         │
│                    │   - Client configs   │                         │
│                    │   - Auth policies    │                         │
│                    └──────────────────────┘                         │
│                                                                       │
└─────────────────────────────────────────────────────────────────────┘
```

## Key Design Decisions

### 5.1 TLS/SSL Strategy
1. **Dual mode support**: Allow both TLS and plaintext (for dev/testing)
2. **Certificate sources**: File-based (production), rustls for management
3. **mTLS optional**: Enable for high-security environments
4. **SNI support**: Multiple domains on same server

### 5.2 Authentication Strategy
1. **Pluggable auth**: Support multiple methods simultaneously
2. **Priority order**: mTLS > JWT > API Key
3. **Client identity**: Extract from cert CN, JWT sub, or API key ID
4. **Backward compatibility**: Allow unauthenticated mode for migration

### 5.3 Job Persistence Strategy
1. **PostgreSQL as primary**: Leverage existing storage backend
2. **Write-ahead logging**: Jobs persisted before execution starts
3. **Recovery on startup**: Scan for orphaned jobs and resume
4. **Configurable retention**: Automatic cleanup of old completed jobs

### 5.4 Distributed Execution Strategy
1. **Redis for coordination**: Lightweight, fast, proven technology
2. **Leader election**: Raft-like algorithm via Redis
3. **Job claiming**: Atomic operations to prevent double execution
4. **Health monitoring**: Detect and recover from node failures

### 5.5 Scheduling Strategy
1. **Priority-based**: Three priority levels (high/normal/low)
2. **Fair scheduling**: Round-robin within priority levels
3. **Backend affinity**: Jobs can target specific backends
4. **Backpressure**: Reject jobs when queues are full

## Implementation Roadmap

### Week 1: TLS/SSL Foundation

#### Files to Create/Modify:
- `src/tls/mod.rs` - TLS configuration and certificate loading
- `src/tls/server.rs` - Server TLS setup with rustls
- `src/tls/client.rs` - Client certificate validation
- `src/config.rs` - Add TLS configuration options

#### Implementation Details:

**TLS Configuration (`config.rs`):**
```rust
#[derive(Debug, Clone, Deserialize)]
pub struct TlsConfig {
    /// Enable TLS
    pub enabled: bool,
    /// Path to server certificate (PEM)
    pub cert_path: String,
    /// Path to server private key (PEM)
    pub key_path: String,
    /// Enable mutual TLS (client certificates)
    pub mtls_enabled: bool,
    /// Path to client CA certificate (for mTLS)
    pub client_ca_path: Option<String>,
    /// Require client certificates (reject if not provided)
    pub require_client_cert: bool,
}
```

**Server TLS Setup (`tls/server.rs`):**
```rust
use rustls::{ServerConfig, Certificate, PrivateKey};
use rustls_pemfile::{certs, pkcs8_private_keys};
use tonic::transport::ServerTlsConfig;

pub fn load_tls_config(config: &TlsConfig) -> Result<ServerTlsConfig> {
    let cert = tokio::fs::read(&config.cert_path).await?;
    let key = tokio::fs::read(&config.key_path).await?;

    let mut tls_config = ServerTlsConfig::new()
        .identity(Identity::from_pem(cert, key));

    if config.mtls_enabled {
        if let Some(ca_path) = &config.client_ca_path {
            let ca_cert = tokio::fs::read(ca_path).await?;
            tls_config = tls_config.client_ca_root(Certificate::from_pem(ca_cert));
        }
    }

    Ok(tls_config)
}
```

**Integration in Server Binary:**
```rust
#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load()?;
    let addr = config.server.listen_addr.parse()?;

    let mut server_builder = Server::builder();

    // Add TLS if enabled
    if config.tls.enabled {
        let tls_config = load_tls_config(&config.tls).await?;
        server_builder = Server::builder()
            .tls_config(tls_config)?;
    }

    server_builder
        .add_service(service)
        .serve(addr)
        .await?;

    Ok(())
}
```

#### Verification:
```bash
# Generate self-signed certs for testing
openssl req -x509 -newkey rsa:4096 -nodes \
  -keyout server-key.pem -out server-cert.pem \
  -days 365 -subj "/CN=localhost"

# Start server with TLS
ARVAK_TLS_ENABLED=true \
ARVAK_TLS_CERT_PATH=server-cert.pem \
ARVAK_TLS_KEY_PATH=server-key.pem \
cargo run --bin arvak-grpc-server

# Test with grpcurl
grpcurl -insecure localhost:50051 list
```

### Week 2: Authentication Layer

#### Files to Create:
- `src/auth/mod.rs` - Authentication module exports
- `src/auth/api_key.rs` - API key validation
- `src/auth/jwt.rs` - JWT token validation
- `src/auth/mtls.rs` - mTLS certificate extraction
- `src/auth/identity.rs` - Client identity abstraction
- `src/auth/middleware.rs` - Authentication interceptor

#### Implementation Details:

**Client Identity (`auth/identity.rs`):**
```rust
#[derive(Debug, Clone)]
pub struct ClientIdentity {
    /// Unique client ID
    pub client_id: String,
    /// Authentication method used
    pub auth_method: AuthMethod,
    /// Optional subject from JWT
    pub subject: Option<String>,
    /// Optional groups/roles
    pub groups: Vec<String>,
    /// Certificate DN (for mTLS)
    pub certificate_dn: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMethod {
    ApiKey,
    Jwt,
    MutualTls,
    Unauthenticated,
}
```

**API Key Validator (`auth/api_key.rs`):**
```rust
pub struct ApiKeyValidator {
    keys: Arc<RwLock<HashMap<String, ClientConfig>>>,
}

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub client_id: String,
    pub api_key_hash: String, // bcrypt hash
    pub enabled: bool,
    pub rate_limit: Option<u32>,
    pub allowed_backends: Option<Vec<String>>,
}

impl ApiKeyValidator {
    pub async fn validate(&self, api_key: &str) -> Result<ClientIdentity> {
        let keys = self.keys.read().await;

        for (_, config) in keys.iter() {
            if config.enabled && bcrypt::verify(api_key, &config.api_key_hash)? {
                return Ok(ClientIdentity {
                    client_id: config.client_id.clone(),
                    auth_method: AuthMethod::ApiKey,
                    subject: None,
                    groups: vec![],
                    certificate_dn: None,
                });
            }
        }

        Err(Error::Unauthenticated("Invalid API key".into()))
    }

    pub async fn load_from_storage(&mut self, storage: &dyn ClientStorage) -> Result<()> {
        // Load API keys from database
        let clients = storage.list_clients().await?;
        let mut keys = self.keys.write().await;
        for client in clients {
            keys.insert(client.client_id.clone(), client);
        }
        Ok(())
    }
}
```

**JWT Validator (`auth/jwt.rs`):**
```rust
use jsonwebtoken::{decode, decode_header, DecodingKey, Validation, Algorithm};

pub struct JwtValidator {
    /// JWKS endpoint for fetching public keys
    jwks_url: Option<String>,
    /// Static public key (for testing)
    public_key: Option<DecodingKey>,
    /// Allowed issuers
    issuers: Vec<String>,
    /// Allowed audiences
    audiences: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Claims {
    sub: String,
    iss: String,
    aud: String,
    exp: usize,
    groups: Option<Vec<String>>,
}

impl JwtValidator {
    pub async fn validate(&self, token: &str) -> Result<ClientIdentity> {
        let header = decode_header(token)?;
        let key = self.get_decoding_key(&header).await?;

        let mut validation = Validation::new(header.alg);
        validation.set_issuer(&self.issuers);
        validation.set_audience(&self.audiences);

        let token_data = decode::<Claims>(token, &key, &validation)?;

        Ok(ClientIdentity {
            client_id: token_data.claims.sub.clone(),
            auth_method: AuthMethod::Jwt,
            subject: Some(token_data.claims.sub),
            groups: token_data.claims.groups.unwrap_or_default(),
            certificate_dn: None,
        })
    }

    async fn get_decoding_key(&self, header: &Header) -> Result<DecodingKey> {
        if let Some(key) = &self.public_key {
            return Ok(key.clone());
        }

        // Fetch from JWKS endpoint
        if let Some(jwks_url) = &self.jwks_url {
            return self.fetch_jwks_key(jwks_url, &header.kid).await;
        }

        Err(Error::Configuration("No JWT key configured".into()))
    }
}
```

**Authentication Interceptor (`auth/middleware.rs`):**
```rust
pub struct AuthInterceptor {
    api_key_validator: Arc<ApiKeyValidator>,
    jwt_validator: Arc<JwtValidator>,
    mtls_enabled: bool,
    allow_unauthenticated: bool,
}

impl Interceptor for AuthInterceptor {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        let auth_result = tokio::runtime::Handle::current()
            .block_on(self.authenticate(&request));

        match auth_result {
            Ok(identity) => {
                // Store identity in request extensions
                request.extensions_mut().insert(identity);
                Ok(request)
            }
            Err(e) if self.allow_unauthenticated => {
                request.extensions_mut().insert(ClientIdentity::unauthenticated());
                Ok(request)
            }
            Err(e) => {
                Err(Status::unauthenticated(e.to_string()))
            }
        }
    }
}

impl AuthInterceptor {
    async fn authenticate(&self, request: &Request<()>) -> Result<ClientIdentity> {
        // Try mTLS first (strongest)
        if self.mtls_enabled {
            if let Some(cert_info) = request.peer_certs() {
                return self.authenticate_mtls(cert_info);
            }
        }

        let metadata = request.metadata();

        // Try JWT (Authorization: Bearer <token>)
        if let Some(auth) = metadata.get("authorization") {
            if let Ok(auth_str) = auth.to_str() {
                if auth_str.starts_with("Bearer ") {
                    let token = &auth_str[7..];
                    return self.jwt_validator.validate(token).await;
                }
            }
        }

        // Try API Key (x-api-key header)
        if let Some(api_key) = metadata.get("x-api-key") {
            if let Ok(key_str) = api_key.to_str() {
                return self.api_key_validator.validate(key_str).await;
            }
        }

        Err(Error::Unauthenticated("No valid credentials provided".into()))
    }
}
```

#### Configuration:
```yaml
auth:
  # Allow unauthenticated access (for migration)
  allow_unauthenticated: false

  # API Key authentication
  api_keys:
    enabled: true
    storage: "postgres"  # or "file"
    file_path: "clients.yaml"

  # JWT authentication
  jwt:
    enabled: true
    jwks_url: "https://auth.example.com/.well-known/jwks.json"
    issuers: ["https://auth.example.com"]
    audiences: ["arvak-api"]

  # mTLS authentication
  mtls:
    enabled: false
    extract_cn_as_client_id: true
```

#### Verification:
```bash
# Test API key auth
grpcurl -H 'x-api-key: test-key-12345' \
  localhost:50051 arvak.v1.ArvakService/ListBackends

# Test JWT auth
grpcurl -H 'authorization: Bearer eyJhbG...' \
  localhost:50051 arvak.v1.ArvakService/SubmitJob
```

### Week 3: Authorization & Access Control

#### Files to Create:
- `src/auth/authorization.rs` - Authorization policy engine
- `src/auth/policy.rs` - Policy types and storage
- `src/auth/rbac.rs` - Role-based access control

#### Implementation Details:

**Authorization Policy (`auth/policy.rs`):**
```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthorizationPolicy {
    /// Client ID or pattern (supports wildcards)
    pub client_id: String,
    /// Allowed operations
    pub allowed_operations: Vec<Operation>,
    /// Allowed backends (None = all backends)
    pub allowed_backends: Option<Vec<String>>,
    /// Maximum queue size per client
    pub max_queued_jobs: Option<usize>,
    /// Rate limit (jobs per minute)
    pub rate_limit_per_minute: Option<u32>,
    /// Maximum shots per job
    pub max_shots_per_job: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum Operation {
    SubmitJob,
    SubmitBatch,
    GetJobStatus,
    GetJobResult,
    CancelJob,
    ListBackends,
    GetBackendInfo,
    WatchJob,
    StreamResults,
    SubmitBatchStream,
}
```

**Policy Enforcement (`auth/authorization.rs`):**
```rust
pub struct Authorizer {
    policies: Arc<RwLock<HashMap<String, AuthorizationPolicy>>>,
    default_policy: AuthorizationPolicy,
}

impl Authorizer {
    pub async fn authorize(
        &self,
        identity: &ClientIdentity,
        operation: Operation,
        backend_id: Option<&str>,
    ) -> Result<()> {
        let policies = self.policies.read().await;

        let policy = policies
            .get(&identity.client_id)
            .unwrap_or(&self.default_policy);

        // Check operation allowed
        if !policy.allowed_operations.contains(&operation) {
            return Err(Error::PermissionDenied(
                format!("Operation {:?} not allowed for client {}",
                    operation, identity.client_id)
            ));
        }

        // Check backend access
        if let Some(backend) = backend_id {
            if let Some(allowed) = &policy.allowed_backends {
                if !allowed.contains(&backend.to_string()) {
                    return Err(Error::PermissionDenied(
                        format!("Backend {} not allowed for client {}",
                            backend, identity.client_id)
                    ));
                }
            }
        }

        Ok(())
    }

    pub async fn get_policy(&self, client_id: &str) -> AuthorizationPolicy {
        let policies = self.policies.read().await;
        policies
            .get(client_id)
            .cloned()
            .unwrap_or_else(|| self.default_policy.clone())
    }
}
```

**Service Integration:**
```rust
impl ArvakServiceImpl {
    async fn submit_job(&self, request: Request<SubmitJobRequest>)
        -> Result<Response<SubmitJobResponse>, Status>
    {
        // Extract client identity (set by auth middleware)
        let identity = request
            .extensions()
            .get::<ClientIdentity>()
            .ok_or_else(|| Status::unauthenticated("No identity"))?;

        let req = request.into_inner();

        // Authorize operation
        self.authorizer
            .authorize(identity, Operation::SubmitJob, Some(&req.backend_id))
            .await
            .map_err(|e| Status::permission_denied(e.to_string()))?;

        // Get client's policy for quota enforcement
        let policy = self.authorizer.get_policy(&identity.client_id).await;

        // Enforce max_shots_per_job
        if let Some(max_shots) = policy.max_shots_per_job {
            if req.shots > max_shots {
                return Err(Status::invalid_argument(
                    format!("Shots {} exceeds limit {}", req.shots, max_shots)
                ));
            }
        }

        // Check queue capacity
        if let Some(max_queued) = policy.max_queued_jobs {
            let current = self.job_store
                .count_queued_jobs(&identity.client_id)
                .await?;
            if current >= max_queued {
                return Err(Status::resource_exhausted(
                    format!("Queue full: {}/{}", current, max_queued)
                ));
            }
        }

        // Proceed with job submission...
        // (existing logic)
    }
}
```

#### Configuration:
```yaml
authorization:
  policies_file: "policies.yaml"
  reload_interval_seconds: 60

  default_policy:
    allowed_operations: ["SubmitJob", "GetJobStatus", "GetJobResult"]
    max_queued_jobs: 10
    rate_limit_per_minute: 60
    max_shots_per_job: 10000
```

**Policies File (`policies.yaml`):**
```yaml
policies:
  - client_id: "research-team"
    allowed_operations:
      - SubmitJob
      - SubmitBatch
      - GetJobStatus
      - GetJobResult
      - CancelJob
      - ListBackends
      - WatchJob
    allowed_backends: ["simulator", "iqm-apollo"]
    max_queued_jobs: 100
    rate_limit_per_minute: 300
    max_shots_per_job: 100000

  - client_id: "demo-users-*"  # Wildcard pattern
    allowed_operations: ["SubmitJob", "GetJobStatus", "GetJobResult"]
    allowed_backends: ["simulator"]
    max_queued_jobs: 5
    rate_limit_per_minute: 10
    max_shots_per_job: 1000
```

### Week 4: Job Persistence & Recovery

#### Files to Create/Modify:
- `src/persistence/mod.rs` - Persistence module
- `src/persistence/checkpoint.rs` - Job checkpoint system
- `src/persistence/recovery.rs` - Recovery logic on startup
- `src/storage/postgres.rs` - Add recovery queries

#### Implementation Details:

**Checkpoint System (`persistence/checkpoint.rs`):**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobCheckpoint {
    pub job_id: JobId,
    pub state: CheckpointState,
    pub created_at: DateTime<Utc>,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CheckpointState {
    /// Job created but not started
    Created,
    /// Job submitted to backend
    Submitted { backend_job_id: String },
    /// Job running on backend
    Running { backend_job_id: String, progress: f32 },
    /// Job completed, result pending storage
    CompletedPending { result_json: String },
}

pub struct CheckpointManager {
    storage: Arc<dyn JobStorage>,
}

impl CheckpointManager {
    /// Save checkpoint before critical operations
    pub async fn checkpoint(&self, job_id: &JobId, state: CheckpointState) -> Result<()> {
        let checkpoint = JobCheckpoint {
            job_id: job_id.clone(),
            state,
            created_at: Utc::now(),
            data: serde_json::json!({}),
        };

        self.storage.save_checkpoint(&checkpoint).await?;
        tracing::debug!("Checkpointed job {}: {:?}", job_id, checkpoint.state);
        Ok(())
    }

    /// Get last checkpoint for recovery
    pub async fn get_checkpoint(&self, job_id: &JobId) -> Result<Option<JobCheckpoint>> {
        self.storage.get_checkpoint(job_id).await
    }
}
```

**Recovery System (`persistence/recovery.rs`):**
```rust
pub struct RecoveryManager {
    job_store: Arc<dyn JobStorage>,
    backend_registry: Arc<BackendRegistry>,
    checkpoint_manager: Arc<CheckpointManager>,
}

impl RecoveryManager {
    /// Scan for orphaned jobs on startup
    pub async fn recover_orphaned_jobs(&self) -> Result<RecoveryStats> {
        tracing::info!("Starting job recovery...");

        let mut stats = RecoveryStats::default();

        // Find jobs that were running when server stopped
        let orphaned_jobs = self.job_store
            .list_jobs(JobFilter::new()
                .with_state(JobStatus::Running)
                .with_limit(1000))
            .await?;

        stats.found = orphaned_jobs.len();

        for job in orphaned_jobs {
            match self.recover_job(&job).await {
                Ok(action) => {
                    stats.record(action);
                    tracing::info!("Recovered job {}: {:?}", job.id, action);
                }
                Err(e) => {
                    stats.failed += 1;
                    tracing::error!("Failed to recover job {}: {}", job.id, e);
                }
            }
        }

        tracing::info!("Recovery complete: {:?}", stats);
        Ok(stats)
    }

    async fn recover_job(&self, job: &StoredJob) -> Result<RecoveryAction> {
        // Get checkpoint to determine recovery strategy
        let checkpoint = self.checkpoint_manager
            .get_checkpoint(&job.id)
            .await?;

        match checkpoint {
            Some(JobCheckpoint { state: CheckpointState::Submitted { backend_job_id }, .. }) => {
                // Job was submitted - try to query backend
                let backend = self.backend_registry.get(&job.backend_id)?;

                match backend.status(&backend_job_id).await {
                    Ok(JobStatus::Completed) => {
                        // Job finished while we were down - fetch result
                        let result = backend.result(&backend_job_id).await?;
                        self.job_store.store_result(&job.id, result).await?;
                        Ok(RecoveryAction::CompletedWhileDown)
                    }
                    Ok(JobStatus::Running) => {
                        // Still running - reattach and monitor
                        self.reattach_job(job, backend_job_id).await?;
                        Ok(RecoveryAction::Reattached)
                    }
                    Ok(JobStatus::Failed(msg)) => {
                        // Failed while we were down
                        self.job_store
                            .update_status(&job.id, JobStatus::Failed(msg))
                            .await?;
                        Ok(RecoveryAction::FailedWhileDown)
                    }
                    Err(_) => {
                        // Backend doesn't know about this job - resubmit
                        self.resubmit_job(job).await?;
                        Ok(RecoveryAction::Resubmitted)
                    }
                    _ => {
                        // Unexpected state - mark as failed
                        self.job_store
                            .update_status(&job.id, JobStatus::Failed("Unknown state".into()))
                            .await?;
                        Ok(RecoveryAction::MarkedFailed)
                    }
                }
            }

            Some(JobCheckpoint { state: CheckpointState::Created, .. }) => {
                // Job never made it to backend - resubmit
                self.resubmit_job(job).await?;
                Ok(RecoveryAction::Resubmitted)
            }

            None => {
                // No checkpoint - assume failed
                self.job_store
                    .update_status(&job.id, JobStatus::Failed("No checkpoint".into()))
                    .await?;
                Ok(RecoveryAction::MarkedFailed)
            }

            _ => Ok(RecoveryAction::Skipped)
        }
    }

    async fn reattach_job(&self, job: &StoredJob, backend_job_id: String) -> Result<()> {
        let backend = self.backend_registry.get(&job.backend_id)?;
        let job_store = self.job_store.clone();
        let job_id = job.id.clone();

        // Spawn monitoring task
        tokio::spawn(async move {
            loop {
                match backend.status(&backend_job_id).await {
                    Ok(JobStatus::Completed) => {
                        if let Ok(result) = backend.result(&backend_job_id).await {
                            let _ = job_store.store_result(&job_id, result).await;
                        }
                        break;
                    }
                    Ok(JobStatus::Failed(msg)) => {
                        let _ = job_store.update_status(&job_id, JobStatus::Failed(msg)).await;
                        break;
                    }
                    _ => {
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
        });

        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct RecoveryStats {
    pub found: usize,
    pub resubmitted: usize,
    pub reattached: usize,
    pub completed_while_down: usize,
    pub failed_while_down: usize,
    pub marked_failed: usize,
    pub failed: usize,
}

#[derive(Debug)]
pub enum RecoveryAction {
    Resubmitted,
    Reattached,
    CompletedWhileDown,
    FailedWhileDown,
    MarkedFailed,
    Skipped,
}
```

**Integration in Server Startup:**
```rust
#[tokio::main]
async fn main() -> Result<()> {
    // ... existing setup ...

    // Run recovery before starting server
    if config.persistence.enable_recovery {
        let recovery_manager = RecoveryManager::new(
            job_store.clone(),
            backend_registry.clone(),
            checkpoint_manager.clone(),
        );

        let stats = recovery_manager.recover_orphaned_jobs().await?;
        tracing::info!("Job recovery complete: {:?}", stats);
    }

    // Start server
    server.serve(addr).await?;
    Ok(())
}
```

#### Configuration:
```yaml
persistence:
  enable_recovery: true
  checkpoint_interval_seconds: 30
  retention:
    completed_jobs_days: 30
    failed_jobs_days: 90
  cleanup:
    enabled: true
    interval_hours: 24
```

### Week 5: Distributed Execution with Redis

#### Files to Create:
- `src/distributed/mod.rs` - Distributed module exports
- `src/distributed/queue.rs` - Redis-backed job queue
- `src/distributed/lock.rs` - Distributed locking
- `src/distributed/leader.rs` - Leader election

#### Dependencies to Add:
```toml
redis = { version = "0.24", features = ["tokio-comp", "connection-manager"] }
```

#### Implementation Details:

**Distributed Queue (`distributed/queue.rs`):**
```rust
use redis::aio::ConnectionManager;
use redis::AsyncCommands;

pub struct DistributedJobQueue {
    redis: ConnectionManager,
    queue_prefix: String,
}

impl DistributedJobQueue {
    pub async fn new(redis_url: &str, queue_prefix: String) -> Result<Self> {
        let client = redis::Client::open(redis_url)?;
        let redis = ConnectionManager::new(client).await?;
        Ok(Self { redis, queue_prefix })
    }

    /// Push job to queue with priority
    pub async fn enqueue(&mut self, job_id: &JobId, priority: Priority) -> Result<()> {
        let queue_key = self.queue_key(priority);
        let score = self.compute_score(priority);

        self.redis
            .zadd(&queue_key, job_id.to_string(), score)
            .await?;

        // Update metrics
        METRICS.distributed_queue_size.with_label_values(&[&priority.to_string()]).inc();

        Ok(())
    }

    /// Claim next job atomically
    pub async fn claim_next(&mut self, worker_id: &str) -> Result<Option<JobId>> {
        // Try each priority queue in order
        for priority in [Priority::High, Priority::Normal, Priority::Low] {
            if let Some(job_id) = self.try_claim_from_queue(priority, worker_id).await? {
                return Ok(Some(job_id));
            }
        }
        Ok(None)
    }

    async fn try_claim_from_queue(
        &mut self,
        priority: Priority,
        worker_id: &str
    ) -> Result<Option<JobId>> {
        let queue_key = self.queue_key(priority);
        let claimed_key = format!("{}:claimed", self.queue_prefix);

        // Lua script for atomic claim
        let script = redis::Script::new(r"
            local queue_key = KEYS[1]
            local claimed_key = KEYS[2]
            local worker_id = ARGV[1]
            local ttl = ARGV[2]

            -- Pop lowest score (oldest) job
            local jobs = redis.call('ZRANGE', queue_key, 0, 0)
            if #jobs == 0 then
                return nil
            end

            local job_id = jobs[1]
            redis.call('ZREM', queue_key, job_id)
            redis.call('HSET', claimed_key, job_id, worker_id)
            redis.call('EXPIRE', claimed_key, ttl)

            return job_id
        ");

        let job_id: Option<String> = script
            .key(&queue_key)
            .key(&claimed_key)
            .arg(worker_id)
            .arg(300) // 5 minute claim TTL
            .invoke_async(&mut self.redis)
            .await?;

        Ok(job_id.map(JobId::new))
    }

    /// Release claimed job back to queue (on failure)
    pub async fn release(&mut self, job_id: &JobId, priority: Priority) -> Result<()> {
        let queue_key = self.queue_key(priority);
        let claimed_key = format!("{}:claimed", self.queue_prefix);

        // Remove from claimed set
        self.redis.hdel(&claimed_key, job_id.to_string()).await?;

        // Re-add to queue with higher priority (retry boost)
        let score = self.compute_score(priority) - 1000.0;
        self.redis.zadd(&queue_key, job_id.to_string(), score).await?;

        Ok(())
    }

    /// Mark job as complete and remove from claimed set
    pub async fn complete(&mut self, job_id: &JobId) -> Result<()> {
        let claimed_key = format!("{}:claimed", self.queue_prefix);
        self.redis.hdel(&claimed_key, job_id.to_string()).await?;
        Ok(())
    }

    fn queue_key(&self, priority: Priority) -> String {
        format!("{}:queue:{}", self.queue_prefix, priority.to_string().to_lowercase())
    }

    fn compute_score(&self, priority: Priority) -> f64 {
        let base = match priority {
            Priority::High => 1000.0,
            Priority::Normal => 5000.0,
            Priority::Low => 10000.0,
        };
        base + Utc::now().timestamp() as f64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    High,
    Normal,
    Low,
}
```

**Leader Election (`distributed/leader.rs`):**
```rust
pub struct LeaderElection {
    redis: ConnectionManager,
    node_id: String,
    lease_key: String,
    lease_duration: Duration,
}

impl LeaderElection {
    pub async fn new(
        redis_url: &str,
        node_id: String,
        lease_duration: Duration,
    ) -> Result<Self> {
        let client = redis::Client::open(redis_url)?;
        let redis = ConnectionManager::new(client).await?;
        let lease_key = "arvak:leader".to_string();

        Ok(Self {
            redis,
            node_id,
            lease_key,
            lease_duration,
        })
    }

    /// Try to acquire leadership
    pub async fn try_acquire(&mut self) -> Result<bool> {
        let ttl_secs = self.lease_duration.as_secs();

        // SET NX EX (set if not exists with expiration)
        let result: bool = redis::cmd("SET")
            .arg(&self.lease_key)
            .arg(&self.node_id)
            .arg("NX")
            .arg("EX")
            .arg(ttl_secs)
            .query_async(&mut self.redis)
            .await?;

        if result {
            tracing::info!("Acquired leadership: node={}", self.node_id);
            METRICS.leader_status.set(1.0);
        }

        Ok(result)
    }

    /// Renew leadership lease
    pub async fn renew(&mut self) -> Result<bool> {
        // Lua script for atomic renewal (only if we're the current leader)
        let script = redis::Script::new(r"
            local key = KEYS[1]
            local node_id = ARGV[1]
            local ttl = ARGV[2]

            local current = redis.call('GET', key)
            if current == node_id then
                redis.call('EXPIRE', key, ttl)
                return 1
            end
            return 0
        ");

        let renewed: i32 = script
            .key(&self.lease_key)
            .arg(&self.node_id)
            .arg(self.lease_duration.as_secs())
            .invoke_async(&mut self.redis)
            .await?;

        Ok(renewed == 1)
    }

    /// Check if this node is leader
    pub async fn is_leader(&mut self) -> Result<bool> {
        let current: Option<String> = self.redis.get(&self.lease_key).await?;
        Ok(current.as_ref() == Some(&self.node_id))
    }

    /// Release leadership
    pub async fn release(&mut self) -> Result<()> {
        // Only delete if we're still the leader
        let script = redis::Script::new(r"
            local key = KEYS[1]
            local node_id = ARGV[1]

            local current = redis.call('GET', key)
            if current == node_id then
                redis.call('DEL', key)
            end
        ");

        script
            .key(&self.lease_key)
            .arg(&self.node_id)
            .invoke_async(&mut self.redis)
            .await?;

        tracing::info!("Released leadership: node={}", self.node_id);
        METRICS.leader_status.set(0.0);
        Ok(())
    }

    /// Run leader election loop
    pub async fn run(mut self, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        let mut interval = tokio::time::interval(self.lease_duration / 3);

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    match self.try_acquire().await {
                        Ok(true) => {
                            // Acquired or renewed
                        }
                        Ok(false) => {
                            // Someone else is leader
                            METRICS.leader_status.set(0.0);
                        }
                        Err(e) => {
                            tracing::error!("Leader election error: {}", e);
                        }
                    }
                }
                _ = shutdown.changed() => {
                    tracing::info!("Shutting down leader election");
                    let _ = self.release().await;
                    break;
                }
            }
        }
    }
}
```

**Distributed Worker (`distributed/worker.rs`):**
```rust
pub struct DistributedWorker {
    worker_id: String,
    queue: Arc<Mutex<DistributedJobQueue>>,
    job_store: Arc<dyn JobStorage>,
    backend_registry: Arc<BackendRegistry>,
    max_concurrent: usize,
}

impl DistributedWorker {
    pub async fn run(self, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        let semaphore = Arc::new(Semaphore::new(self.max_concurrent));
        let mut poll_interval = tokio::time::interval(Duration::from_secs(1));

        loop {
            tokio::select! {
                _ = poll_interval.tick() => {
                    // Try to claim and process jobs
                    let permit = match semaphore.clone().try_acquire_owned() {
                        Ok(p) => p,
                        Err(_) => continue, // All workers busy
                    };

                    let mut queue = self.queue.lock().await;
                    match queue.claim_next(&self.worker_id).await {
                        Ok(Some(job_id)) => {
                            drop(queue); // Release lock

                            // Spawn job execution
                            let job_store = self.job_store.clone();
                            let backends = self.backend_registry.clone();
                            let queue_ref = self.queue.clone();

                            tokio::spawn(async move {
                                let _permit = permit; // Hold permit until done

                                match Self::execute_job(job_id.clone(), job_store, backends).await {
                                    Ok(priority) => {
                                        // Mark complete
                                        let mut q = queue_ref.lock().await;
                                        let _ = q.complete(&job_id).await;
                                    }
                                    Err(e) => {
                                        tracing::error!("Job {} execution failed: {}", job_id, e);
                                        // Release back to queue for retry
                                        let mut q = queue_ref.lock().await;
                                        let _ = q.release(&job_id, Priority::Normal).await;
                                    }
                                }
                            });
                        }
                        Ok(None) => {
                            // No jobs available
                            drop(permit);
                        }
                        Err(e) => {
                            tracing::error!("Failed to claim job: {}", e);
                            drop(permit);
                        }
                    }
                }
                _ = shutdown.changed() => {
                    tracing::info!("Worker shutting down");
                    break;
                }
            }
        }
    }

    async fn execute_job(
        job_id: JobId,
        job_store: Arc<dyn JobStorage>,
        backends: Arc<BackendRegistry>,
    ) -> Result<Priority> {
        // Get job details
        let job = job_store.get_job(&job_id).await?
            .ok_or_else(|| Error::JobNotFound(job_id.0.clone()))?;

        // Update status to running
        job_store.update_status(&job_id, JobStatus::Running).await?;

        // Get backend
        let backend = backends.get(&job.backend_id)?;

        // Execute
        let backend_job_id = backend.submit(&job.circuit, job.shots).await?;
        let result = backend.wait(&backend_job_id).await?;

        // Store result
        job_store.store_result(&job_id, result).await?;

        Ok(Priority::Normal) // Return original priority
    }
}
```

#### Configuration:
```yaml
distributed:
  enabled: false
  redis_url: "redis://localhost:6379"
  node_id: "node-1"  # Auto-generated if not specified

  queue:
    prefix: "arvak"
    claim_ttl_seconds: 300

  leader_election:
    enabled: true
    lease_duration_seconds: 30

  worker:
    max_concurrent_jobs: 10
    poll_interval_ms: 1000
```

### Week 6: Advanced Scheduling

#### Files to Create:
- `src/scheduler/mod.rs` - Scheduler module exports
- `src/scheduler/priority.rs` - Priority assignment logic
- `src/scheduler/fair.rs` - Fair scheduling algorithms
- `src/scheduler/policy.rs` - Scheduling policies

#### Implementation Details:

**Priority Scheduler (`scheduler/priority.rs`):**
```rust
pub struct PriorityScheduler {
    queue: Arc<Mutex<DistributedJobQueue>>,
    policies: Arc<RwLock<HashMap<String, SchedulingPolicy>>>,
}

#[derive(Debug, Clone)]
pub struct SchedulingPolicy {
    /// Default priority for this client
    pub default_priority: Priority,
    /// Can submit high priority jobs
    pub can_use_high_priority: bool,
    /// Fair share weight (for fair scheduling)
    pub fair_share_weight: f32,
}

impl PriorityScheduler {
    pub async fn submit_with_priority(
        &self,
        job_id: &JobId,
        client_id: &str,
        requested_priority: Option<Priority>,
    ) -> Result<Priority> {
        let policies = self.policies.read().await;
        let policy = policies
            .get(client_id)
            .cloned()
            .unwrap_or_default();

        // Determine actual priority
        let priority = match requested_priority {
            Some(Priority::High) if policy.can_use_high_priority => Priority::High,
            Some(p) => p,
            None => policy.default_priority,
        };

        // Enqueue with priority
        let mut queue = self.queue.lock().await;
        queue.enqueue(job_id, priority).await?;

        tracing::info!(
            "Job {} queued with priority {:?} for client {}",
            job_id, priority, client_id
        );

        Ok(priority)
    }
}
```

**Fair Scheduler (`scheduler/fair.rs`):**
```rust
pub struct FairScheduler {
    queue: Arc<Mutex<DistributedJobQueue>>,
    usage_tracker: Arc<RwLock<HashMap<String, UsageStats>>>,
    policies: Arc<RwLock<HashMap<String, SchedulingPolicy>>>,
}

#[derive(Debug, Default)]
struct UsageStats {
    jobs_submitted: u64,
    jobs_completed: u64,
    total_execution_time_ms: u64,
    last_job_time: Option<DateTime<Utc>>,
}

impl FairScheduler {
    /// Compute effective priority based on fair share
    pub async fn compute_effective_priority(
        &self,
        client_id: &str,
        requested_priority: Priority,
    ) -> Priority {
        let usage = self.usage_tracker.read().await;
        let policies = self.policies.read().await;

        let policy = policies.get(client_id);
        let stats = usage.get(client_id);

        // Compute fair share ratio
        let fair_share = policy.map(|p| p.fair_share_weight).unwrap_or(1.0);
        let actual_share = stats
            .map(|s| s.jobs_completed as f32)
            .unwrap_or(0.0);

        // If client is under fair share, boost priority
        // If over fair share, reduce priority
        let ratio = actual_share / fair_share.max(1.0);

        if ratio < 0.5 {
            // Under-served: boost priority
            match requested_priority {
                Priority::Low => Priority::Normal,
                Priority::Normal => Priority::High,
                p => p,
            }
        } else if ratio > 2.0 {
            // Over-served: reduce priority
            match requested_priority {
                Priority::High => Priority::Normal,
                Priority::Normal => Priority::Low,
                p => p,
            }
        } else {
            requested_priority
        }
    }

    pub async fn record_job_completion(
        &self,
        client_id: &str,
        execution_time_ms: u64,
    ) {
        let mut usage = self.usage_tracker.write().await;
        let stats = usage.entry(client_id.to_string()).or_default();

        stats.jobs_completed += 1;
        stats.total_execution_time_ms += execution_time_ms;
        stats.last_job_time = Some(Utc::now());
    }
}
```

**Backend-Specific Queues:**
```rust
pub struct BackendAwareScheduler {
    queues: HashMap<String, Arc<Mutex<DistributedJobQueue>>>,
}

impl BackendAwareScheduler {
    pub async fn submit_to_backend(
        &self,
        job_id: &JobId,
        backend_id: &str,
        priority: Priority,
    ) -> Result<()> {
        let queue = self.queues
            .get(backend_id)
            .ok_or_else(|| Error::BackendNotFound(backend_id.to_string()))?;

        let mut q = queue.lock().await;
        q.enqueue(job_id, priority).await?;

        Ok(())
    }

    pub async fn claim_for_backend(
        &self,
        backend_id: &str,
        worker_id: &str,
    ) -> Result<Option<JobId>> {
        let queue = self.queues
            .get(backend_id)
            .ok_or_else(|| Error::BackendNotFound(backend_id.to_string()))?;

        let mut q = queue.lock().await;
        q.claim_next(worker_id).await
    }
}
```

#### Configuration:
```yaml
scheduler:
  type: "fair"  # "priority", "fair", or "backend-aware"

  # Fair scheduler settings
  fair:
    usage_window_hours: 24
    rebalance_interval_seconds: 60

  # Backend-specific queues
  backend_queues:
    enabled: true
    backends:
      - simulator
      - iqm-apollo
      - ibm-quantum
```

## Testing Strategy

### Integration Tests

**Test TLS Setup (`tests/tls_test.rs`):**
```rust
#[tokio::test]
async fn test_tls_connection() {
    let server = start_test_server_with_tls().await;

    let cert = tokio::fs::read("test-certs/server-cert.pem").await.unwrap();
    let ca = Certificate::from_pem(cert);

    let tls = ClientTlsConfig::new()
        .ca_certificate(ca)
        .domain_name("localhost");

    let channel = Channel::from_static("https://localhost:50051")
        .tls_config(tls).unwrap()
        .connect()
        .await
        .unwrap();

    let mut client = ArvakServiceClient::new(channel);
    let response = client.list_backends(Request::new(ListBackendsRequest {}))
        .await
        .unwrap();

    assert!(!response.into_inner().backends.is_empty());
}
```

**Test Authentication (`tests/auth_test.rs`):**
```rust
#[tokio::test]
async fn test_api_key_auth() {
    let server = start_test_server_with_auth().await;

    let mut request = Request::new(ListBackendsRequest {});
    request.metadata_mut().insert(
        "x-api-key",
        "test-key-12345".parse().unwrap(),
    );

    let response = server.list_backends(request).await.unwrap();
    assert!(!response.into_inner().backends.is_empty());
}

#[tokio::test]
async fn test_unauthorized() {
    let server = start_test_server_with_auth().await;

    let request = Request::new(ListBackendsRequest {});
    let result = server.list_backends(request).await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), Code::Unauthenticated);
}
```

**Test Distributed Queue (`tests/distributed_test.rs`):**
```rust
#[tokio::test]
async fn test_distributed_queue() {
    let redis = start_test_redis().await;
    let mut queue = DistributedJobQueue::new(&redis.url(), "test".into()).await.unwrap();

    let job_id = JobId::new("test-job-1".into());
    queue.enqueue(&job_id, Priority::Normal).await.unwrap();

    let claimed = queue.claim_next("worker-1").await.unwrap();
    assert_eq!(claimed, Some(job_id.clone()));

    // Second claim should return None (job already claimed)
    let claimed2 = queue.claim_next("worker-2").await.unwrap();
    assert_eq!(claimed2, None);
}
```

## Deployment Guide

### Production Deployment Architecture

```yaml
# docker-compose.yml
version: '3.8'

services:
  postgres:
    image: postgres:15
    environment:
      POSTGRES_DB: arvak
      POSTGRES_USER: arvak
      POSTGRES_PASSWORD: ${DB_PASSWORD}
    volumes:
      - postgres_data:/var/lib/postgresql/data

  redis:
    image: redis:7-alpine
    command: redis-server --appendonly yes
    volumes:
      - redis_data:/data

  arvak-grpc-node1:
    image: arvak-grpc:latest
    depends_on:
      - postgres
      - redis
    environment:
      ARVAK_NODE_ID: "node-1"
      ARVAK_TLS_ENABLED: "true"
      ARVAK_TLS_CERT_PATH: "/certs/server-cert.pem"
      ARVAK_TLS_KEY_PATH: "/certs/server-key.pem"
      ARVAK_AUTH_API_KEYS_ENABLED: "true"
      ARVAK_AUTH_JWT_ENABLED: "true"
      ARVAK_DISTRIBUTED_ENABLED: "true"
      ARVAK_DISTRIBUTED_REDIS_URL: "redis://redis:6379"
      ARVAK_STORAGE_TYPE: "postgres"
      ARVAK_STORAGE_POSTGRES_URL: "postgres://arvak:${DB_PASSWORD}@postgres/arvak"
    volumes:
      - ./certs:/certs:ro
      - ./config.yaml:/app/config.yaml:ro
    ports:
      - "50051:50051"
      - "8080:8080"  # Health/metrics

  arvak-grpc-node2:
    image: arvak-grpc:latest
    depends_on:
      - postgres
      - redis
    environment:
      ARVAK_NODE_ID: "node-2"
      # ... same as node1 ...
    ports:
      - "50052:50051"
      - "8081:8080"

  arvak-grpc-node3:
    image: arvak-grpc:latest
    depends_on:
      - postgres
      - redis
    environment:
      ARVAK_NODE_ID: "node-3"
      # ... same as node1 ...
    ports:
      - "50053:50051"
      - "8082:8080"

  envoy:
    image: envoyproxy/envoy:v1.28-latest
    ports:
      - "443:443"
    volumes:
      - ./envoy.yaml:/etc/envoy/envoy.yaml:ro
      - ./certs:/certs:ro

volumes:
  postgres_data:
  redis_data:
```

### Kubernetes Deployment

```yaml
# deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: arvak-grpc
spec:
  replicas: 3
  selector:
    matchLabels:
      app: arvak-grpc
  template:
    metadata:
      labels:
        app: arvak-grpc
    spec:
      containers:
      - name: arvak-grpc
        image: arvak-grpc:latest
        ports:
        - containerPort: 50051
          name: grpc
        - containerPort: 8080
          name: http
        env:
        - name: ARVAK_NODE_ID
          valueFrom:
            fieldRef:
              fieldPath: metadata.name
        - name: ARVAK_DISTRIBUTED_ENABLED
          value: "true"
        - name: ARVAK_DISTRIBUTED_REDIS_URL
          valueFrom:
            secretKeyRef:
              name: arvak-secrets
              key: redis-url
        - name: ARVAK_STORAGE_POSTGRES_URL
          valueFrom:
            secretKeyRef:
              name: arvak-secrets
              key: postgres-url
        volumeMounts:
        - name: config
          mountPath: /app/config.yaml
          subPath: config.yaml
        - name: tls-certs
          mountPath: /certs
        livenessProbe:
          httpGet:
            path: /health/live
            port: 8080
          initialDelaySeconds: 10
          periodSeconds: 30
        readinessProbe:
          httpGet:
            path: /health/ready
            port: 8080
          initialDelaySeconds: 5
          periodSeconds: 10
      volumes:
      - name: config
        configMap:
          name: arvak-config
      - name: tls-certs
        secret:
          secretName: arvak-tls-certs
---
apiVersion: v1
kind: Service
metadata:
  name: arvak-grpc
spec:
  type: LoadBalancer
  ports:
  - port: 443
    targetPort: 50051
    name: grpc
  - port: 8080
    targetPort: 8080
    name: http
  selector:
    app: arvak-grpc
```

## Verification Checklist

### Phase 5.1: TLS/SSL ✓
- [ ] Server starts with TLS enabled
- [ ] Client can connect via TLS
- [ ] mTLS client certificate validation works
- [ ] Certificate rotation doesn't require restart
- [ ] TLS and plaintext modes both work

### Phase 5.2: Authentication ✓
- [ ] API key authentication accepts valid keys
- [ ] API key authentication rejects invalid keys
- [ ] JWT validation with JWKS endpoint works
- [ ] mTLS extracts client identity from certificate
- [ ] Multiple auth methods work simultaneously
- [ ] Unauthenticated mode works when enabled

### Phase 5.3: Authorization ✓
- [ ] Policy enforcement blocks unauthorized operations
- [ ] Backend access control works
- [ ] Per-client quotas enforced
- [ ] Rate limiting works
- [ ] Max shots per job enforced

### Phase 5.4: Job Persistence ✓
- [ ] Jobs survive server restart
- [ ] Orphaned jobs recovered on startup
- [ ] Running jobs reattached to backends
- [ ] Checkpoints saved at critical points
- [ ] Job history cleanup works

### Phase 5.5: Distributed Execution ✓
- [ ] Multiple nodes can run simultaneously
- [ ] Redis queue shared across nodes
- [ ] Leader election works
- [ ] Job claiming is atomic (no double execution)
- [ ] Node failure detected and handled
- [ ] Fair scheduling across clients works
- [ ] Priority queues respected

## Success Criteria

### Security
- ✅ All connections encrypted with TLS
- ✅ Client authentication required in production
- ✅ Authorization policies enforced
- ✅ No plaintext secrets in configuration

### Reliability
- ✅ Jobs survive server restarts
- ✅ No job loss during crashes
- ✅ Multi-node deployment without downtime
- ✅ Automatic recovery from failures

### Scalability
- ✅ Horizontal scaling with multiple nodes
- ✅ Shared job queue across nodes
- ✅ No single point of failure
- ✅ 1000+ jobs/sec throughput (multi-node)

### Operational
- ✅ Zero-downtime deployments
- ✅ Graceful node shutdown
- ✅ Comprehensive metrics for monitoring
- ✅ Clear documentation for operators

## Performance Targets

- **Single Node**: 100-200 jobs/sec
- **3-Node Cluster**: 500-1000 jobs/sec
- **TLS Overhead**: < 5% latency increase
- **Auth Overhead**: < 1ms per request
- **Job Recovery**: < 30 seconds on startup
- **Leader Election**: < 5 seconds on failure

## Migration Guide

### From Phase 4 to Phase 5

**Step 1: Enable TLS (backward compatible)**
```yaml
tls:
  enabled: true
  cert_path: "certs/server-cert.pem"
  key_path: "certs/server-key.pem"
  mtls_enabled: false  # Start with server-only TLS
```

**Step 2: Enable Authentication (allow unauthenticated initially)**
```yaml
auth:
  allow_unauthenticated: true  # Transition mode
  api_keys:
    enabled: true
```

**Step 3: Migrate Clients**
- Issue API keys to existing clients
- Update client code to send x-api-key header
- Verify clients work with authentication

**Step 4: Enforce Authentication**
```yaml
auth:
  allow_unauthenticated: false  # Require auth
```

**Step 5: Enable Distributed Mode (optional)**
```yaml
distributed:
  enabled: true
  redis_url: "redis://redis:6379"
```

**Step 6: Deploy Multiple Nodes**
- Start additional nodes
- Verify leader election
- Test job distribution

## Next Steps

After Phase 5, the service will be production-ready with enterprise features. Future enhancements could include:

- **Phase 6: Advanced Features**
  - Circuit optimization before execution
  - Result caching and deduplication
  - Cost estimation and billing
  - Multi-region deployment

- **Phase 7: Enterprise Features**
  - LDAP/Active Directory integration
  - SAML SSO support
  - Audit logging to external systems
  - Compliance reporting (SOC2, HIPAA)

This completes Phase 5 - the service is now truly production-ready!
