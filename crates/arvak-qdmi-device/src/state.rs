// SPDX-License-Identifier: Apache-2.0
//! Global state for the QDMI device library.
//!
//! The C ABI uses opaque `*mut c_void` handles for sessions and jobs. We store
//! the actual state in global maps keyed by integer IDs, and cast those IDs to
//! opaque pointers for the QDMI driver.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use tokio::sync::RwLock;

use crate::job::ArvakJob;
use crate::session::ArvakSession;

/// Global state — created once in `device_initialize`, never replaced.
pub struct GlobalState {
    pub runtime: tokio::runtime::Runtime,
    pub sessions: RwLock<HashMap<usize, ArvakSession>>,
    pub jobs: RwLock<HashMap<usize, ArvakJob>>,
    next_session_id: AtomicUsize,
    next_job_id: AtomicUsize,
    pub finalized: AtomicBool,
}

static GLOBAL: OnceLock<GlobalState> = OnceLock::new();

impl GlobalState {
    pub fn alloc_session_id(&self) -> Option<usize> {
        self.next_session_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .ok()
    }

    pub fn alloc_job_id(&self) -> Option<usize> {
        self.next_job_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .ok()
    }
}

/// Initialize global state. Returns `false` if already initialized.
pub fn initialize() -> bool {
    GLOBAL
        .set(GlobalState {
            runtime: tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("failed to create tokio runtime"),
            sessions: RwLock::new(HashMap::new()),
            jobs: RwLock::new(HashMap::new()),
            next_session_id: AtomicUsize::new(1),
            next_job_id: AtomicUsize::new(1),
            finalized: AtomicBool::new(false),
        })
        .is_ok()
}

/// Get the global state, or `None` if not initialized or finalized.
pub fn get() -> Option<&'static GlobalState> {
    GLOBAL
        .get()
        .filter(|s| !s.finalized.load(Ordering::Relaxed))
}

// ---------------------------------------------------------------------------
// Handle conversion
// ---------------------------------------------------------------------------

/// Cast an integer ID to an opaque QDMI handle.
pub fn id_to_handle(id: usize) -> *mut c_void {
    id as *mut c_void
}

/// Cast an opaque QDMI handle back to an integer ID.
///
/// Returns `None` for null pointers or zero.
pub fn handle_to_id(handle: *mut c_void) -> Option<usize> {
    let id = handle as usize;
    if id == 0 { None } else { Some(id) }
}
