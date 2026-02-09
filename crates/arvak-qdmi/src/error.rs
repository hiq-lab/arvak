// SPDX-License-Identifier: Apache-2.0
//! Error types for QDMI device interaction.

use crate::ffi;
use crate::format::CircuitFormat;

/// Errors arising from QDMI device operations.
#[derive(Debug, thiserror::Error)]
pub enum QdmiError {
    #[error("failed to load device library at '{path}': {cause}")]
    LoadFailed { path: String, cause: String },

    #[error("symbol '{symbol}' not found in device library: {cause}")]
    SymbolNotFound { symbol: String, cause: String },

    #[error("property not supported by this device")]
    NotSupported,

    #[error("invalid argument passed to QDMI function")]
    InvalidArgument,

    #[error("QDMI device is in a bad state")]
    BadState,

    #[error("QDMI operation timed out")]
    Timeout,

    #[error("QDMI operation not implemented")]
    NotImplemented,

    #[error("QDMI out of memory")]
    OutOfMemory,

    #[error("QDMI permission denied")]
    PermissionDenied,

    #[error("QDMI operation failed with error code {0}")]
    DeviceError(i32),

    #[error("failed to parse QDMI response: {0}")]
    ParseError(String),

    #[error("no compatible circuit format; device supports: {supported:?}")]
    NoCompatibleFormat { supported: Vec<CircuitFormat> },

    #[error("no devices found")]
    NoDevicesFound,

    #[error("session error: {0}")]
    SessionError(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl QdmiError {
    /// Convert a raw QDMI C error code into a typed error.
    pub fn from_code(code: i32) -> Self {
        match code {
            ffi::QDMI_ERROR_NOTSUPPORTED => QdmiError::NotSupported,
            ffi::QDMI_ERROR_INVALIDARGUMENT => QdmiError::InvalidArgument,
            ffi::QDMI_ERROR_BADSTATE => QdmiError::BadState,
            ffi::QDMI_ERROR_TIMEOUT => QdmiError::Timeout,
            ffi::QDMI_ERROR_NOTIMPLEMENTED => QdmiError::NotImplemented,
            ffi::QDMI_ERROR_OUTOFMEM => QdmiError::OutOfMemory,
            ffi::QDMI_ERROR_PERMISSIONDENIED => QdmiError::PermissionDenied,
            other => QdmiError::DeviceError(other),
        }
    }
}

pub type Result<T> = std::result::Result<T, QdmiError>;
