//! Backend-related gRPC RPC implementations.

use tonic::{Request, Response, Status};

use crate::proto::{
    BackendInfo, GetBackendInfoRequest, GetBackendInfoResponse, ListBackendsRequest,
    ListBackendsResponse,
};

use super::super::ArvakServiceImpl;

impl ArvakServiceImpl {
    pub(in crate::server) async fn list_backends_impl(
        &self,
        _request: Request<ListBackendsRequest>,
    ) -> std::result::Result<Response<ListBackendsResponse>, Status> {
        let backend_ids = self.backends.list();
        let mut backends = Vec::new();

        for id in backend_ids {
            let backend = self.backends.get(&id).map_err(Status::from)?;

            let caps = backend.capabilities();

            let is_available = backend.availability().await.is_ok_and(|a| a.is_available);

            let topology_json =
                serde_json::to_string(&caps.topology).unwrap_or_else(|_| "{}".to_string());

            let mut supported_gates = caps.gate_set.single_qubit.clone();
            supported_gates.extend(caps.gate_set.two_qubit.clone());

            backends.push(BackendInfo {
                backend_id: id.clone(),
                name: caps.name.clone(),
                is_available,
                max_qubits: caps.num_qubits,
                max_shots: caps.max_shots,
                description: format!("{} ({} qubits)", backend.name(), caps.num_qubits),
                supported_gates,
                topology_json,
            });
        }

        Ok(Response::new(ListBackendsResponse { backends }))
    }

    pub(in crate::server) async fn get_backend_info_impl(
        &self,
        request: Request<GetBackendInfoRequest>,
    ) -> std::result::Result<Response<GetBackendInfoResponse>, Status> {
        let req = request.into_inner();

        let backend = self.backends.get(&req.backend_id).map_err(Status::from)?;

        let caps = backend.capabilities();

        let is_available = backend.availability().await.is_ok_and(|a| a.is_available);

        let topology_json =
            serde_json::to_string(&caps.topology).unwrap_or_else(|_| "{}".to_string());

        let mut supported_gates = caps.gate_set.single_qubit.clone();
        supported_gates.extend(caps.gate_set.two_qubit.clone());

        let backend_info = BackendInfo {
            backend_id: req.backend_id.clone(),
            name: caps.name.clone(),
            is_available,
            max_qubits: caps.num_qubits,
            max_shots: caps.max_shots,
            description: format!("{} ({} qubits)", backend.name(), caps.num_qubits),
            supported_gates,
            topology_json,
        };

        Ok(Response::new(GetBackendInfoResponse {
            backend: Some(backend_info),
        }))
    }
}
