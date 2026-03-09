// SPDX-License-Identifier: Apache-2.0
//! Session state for an active QDMI connection to the Arvak server.

use arvak_grpc::proto::arvak_service_client::ArvakServiceClient;
use arvak_grpc::proto::{BackendInfo, ListBackendsRequest};
use tonic::Request;
use tonic::transport::Channel;

/// An active session connected to the Arvak gRPC server.
pub struct ArvakSession {
    /// gRPC client (available after `session_init`).
    pub client: Option<ArvakServiceClient<Channel>>,
    /// Server URL — set via `session_set_parameter(BASEURL)` or env.
    pub server_url: Option<String>,
    /// Auth token — set via `session_set_parameter(TOKEN)`.
    pub token: Option<String>,
    /// Selected backend (default: "simulator").
    pub backend_id: String,
    /// Cached backend info from `ListBackends`.
    pub backends: Vec<BackendInfo>,
    /// The backend info for the selected backend.
    pub active_backend: Option<BackendInfo>,
}

impl ArvakSession {
    pub fn new() -> Self {
        Self {
            client: None,
            server_url: None,
            token: None,
            backend_id: "simulator".into(),
            backends: Vec::new(),
            active_backend: None,
        }
    }

    /// Connect to the Arvak server and fetch backend info.
    pub async fn connect(&mut self) -> Result<(), String> {
        let url = self
            .server_url
            .clone()
            .or_else(|| std::env::var("ARVAK_QDMI_URL").ok())
            .unwrap_or_else(|| "https://qdmi.arvak.io".into());

        let mut client = ArvakServiceClient::connect(url.clone())
            .await
            .map_err(|e| format!("gRPC connect to {url} failed: {e}"))?;

        // Fetch available backends
        let resp = client
            .list_backends(Request::new(ListBackendsRequest {}))
            .await
            .map_err(|e| format!("ListBackends failed: {e}"))?;

        self.backends = resp.into_inner().backends;
        self.active_backend = self
            .backends
            .iter()
            .find(|b| b.backend_id == self.backend_id)
            .cloned();

        tracing::info!(
            "connected to Arvak at {url}, {} backends available",
            self.backends.len()
        );

        self.client = Some(client);
        Ok(())
    }

    /// Get a mutable reference to the gRPC client.
    pub fn client_mut(&mut self) -> Result<&mut ArvakServiceClient<Channel>, String> {
        self.client
            .as_mut()
            .ok_or_else(|| "session not initialized".into())
    }
}
