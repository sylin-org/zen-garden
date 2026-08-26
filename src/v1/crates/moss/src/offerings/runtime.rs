// list()/PlacedRef serve O2's posture and reconcile surfaces.
#![allow(dead_code)]

//! The runtime port (OFFERINGS.md §4): the pluggable execution substrate
//! beneath managed offerings. This module defines only the SEAM and its
//! shared vocabulary; adapters live beside it (docker.rs, null here) and
//! the domain (model.rs) owns the spec types they consume.

use crate::offerings::model::{Status, WorkloadSpec};
use std::collections::HashMap;

/// What placement produced — already translated to port NAMES (PORT-0001).
/// Adapter wire formats (e.g. Docker's "80/tcp" keys) never leave the
/// adapter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Placement {
    pub named_host_ports: HashMap<String, u16>,
}

/// What a runtime observes about a placed offering right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observed {
    pub running: bool,
    pub named_host_ports: HashMap<String, u16>,
}

/// Errors adapters return. Connection-level failures are retryable;
/// unsupported means this world simply doesn't do that.
#[derive(Debug)]
pub enum RuntimeError {
    Unsupported(&'static str),
    Unavailable(String),
    Failed(String),
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(why) => write!(f, "runtime unsupported here: {why}"),
            Self::Unavailable(e) => write!(f, "runtime unavailable: {e}"),
            Self::Failed(e) => write!(f, "runtime operation failed: {e}"),
        }
    }
}

impl std::error::Error for RuntimeError {}

/// The pluggable substrate. One implementation per execution world
/// (docker / podman / systemd / null); probed and adopted at boot (L25),
/// bound per-offering at placement, remembered forever.
#[async_trait::async_trait]
pub trait Runtime: Send + Sync {
    fn kind(&self) -> &'static str;

    /// Make reality match the spec: pull if missing, create if absent,
    /// ensure started. Idempotent by contract. Returns the named host
    /// ports as actually placed.
    async fn place(
        &self,
        name: &str,
        spec: &WorkloadSpec,
    ) -> Result<Placement, RuntimeError>;

    async fn start(&self, name: &str) -> Result<(), RuntimeError>;
    async fn stop(&self, name: &str) -> Result<(), RuntimeError>;
    async fn remove(&self, name: &str) -> Result<(), RuntimeError>;

    /// Right now: present? running? publishing what?
    async fn observe(&self, name: &str) -> Option<Observed>;

    /// Everything this world currently hosts for the garden.
    async fn list(&self) -> Vec<PlacedRef>;
}

/// A lightweight row for listings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacedRef {
    pub name: String,
    pub status: Status,
}

/// The host's worlds: every runtime this stone adopted at boot (L25).
/// Placement binds an offering to one of these kinds and remembers it
/// forever (OFFERINGS.md §4, multi-runtime hosts).
#[derive(Default, Clone)]
pub struct RuntimeRegistry {
    runtimes: std::sync::Arc<HashMap<&'static str, std::sync::Arc<dyn Runtime>>>,
}

impl RuntimeRegistry {
    pub fn build(runtimes: Vec<std::sync::Arc<dyn Runtime>>) -> Self {
        let map = runtimes.into_iter().map(|r| (r.kind(), r)).collect();
        Self { runtimes: std::sync::Arc::new(map) }
    }

    pub fn by_kind(&self, kind: &str) -> Result<std::sync::Arc<dyn Runtime>, String> {
        self.runtimes.get(kind).cloned().ok_or_else(|| {
            let available = self.runtimes.keys().copied().collect::<Vec<_>>().join(", ");
            format!("runtime '{kind}' not available on this stone (available: {available})")
        })
    }

    /// Kinds present — advertised in posture so the garden knows what this
    /// stone may host.
    pub fn kinds(&self) -> Vec<&'static str> {
        let mut k: Vec<&'static str> = self.runtimes.keys().copied().collect();
        k.sort_unstable();
        k
    }
}

/// The reference adapter: proves the seam by refusing everything, loudly.
pub struct NullRuntime;

#[async_trait::async_trait]
impl Runtime for NullRuntime {
    fn kind(&self) -> &'static str {
        "null"
    }

    async fn place(
        &self,
        _name: &str,
        _spec: &WorkloadSpec,
    ) -> Result<Placement, RuntimeError> {
        Err(RuntimeError::Unsupported("the null world places nothing"))
    }

    async fn start(&self, _name: &str) -> Result<(), RuntimeError> {
        Err(RuntimeError::Unsupported("start"))
    }

    async fn stop(&self, _name: &str) -> Result<(), RuntimeError> {
        Err(RuntimeError::Unsupported("stop"))
    }

    async fn remove(&self, _name: &str) -> Result<(), RuntimeError> {
        Err(RuntimeError::Unsupported("remove"))
    }

    async fn observe(&self, _name: &str) -> Option<Observed> {
        None
    }

    async fn list(&self) -> Vec<PlacedRef> {
        Vec::new()
    }
}
