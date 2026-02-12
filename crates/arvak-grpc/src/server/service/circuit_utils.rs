//! Circuit parsing utilities shared across gRPC service modules.

use arvak_ir::circuit::Circuit;
use crate::error::Result;
use crate::proto::{CircuitPayload, circuit_payload};
use crate::error::Error;

/// Parse circuit from protobuf payload (static version for use in async contexts).
pub(super) fn parse_circuit_static(payload: Option<CircuitPayload>) -> Result<Circuit> {
    let payload =
        payload.ok_or_else(|| Error::InvalidCircuit("Missing circuit payload".to_string()))?;

    match payload.format {
        Some(circuit_payload::Format::Qasm3(qasm)) => {
            let circuit = arvak_qasm3::parse(&qasm)?;
            Ok(circuit)
        }
        Some(circuit_payload::Format::ArvakIrJson(_json)) => Err(Error::InvalidCircuit(
            "Arvak IR JSON format not yet supported. Use OpenQASM 3 format instead."
                .to_string(),
        )),
        None => Err(Error::InvalidCircuit(
            "No circuit format specified".to_string(),
        )),
    }
}
