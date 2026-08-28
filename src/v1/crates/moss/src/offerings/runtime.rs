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

    /// Follow an offering's logs (docker-logs semantics: history first,
    /// then live). `tail` bounds the history to the last N lines; the
    /// stream runs until the container stops or the client leaves.
    /// Returns `None` when this world cannot stream logs — the default,
    /// so worlds opt IN at their own seam.
    fn logs_stream(
        &self,
        _name: &str,
        _tail: Option<u64>,
        _timestamps: bool,
    ) -> Option<LogStream> {
        None
    }

    /// Refresh an image reference (the pull half of nourish, J3): pull
    /// the tag, compare the image ID the offering would now run against
    /// what it ran before. Returns old and new IDs; `changed` says
    /// whether an update exists. `None` when this world cannot check.
    async fn refresh_image(
        &self,
        _image: &str,
    ) -> Option<Result<super::runtime::ImageRefresh, super::runtime::RuntimeError>> {
        None
    }

    /// Rehearse a workload in isolation: create WITHOUT published ports,
    /// start, hold for `wait_secs`, observe the fate, remove. The proof
    /// loop of restore rehearsal (J2) — does the restored data boot?
    /// Returns `None` when this world cannot rehearse (the null world).
    async fn rehearse_run(
        &self,
        _name: &str,
        _spec: &super::model::WorkloadSpec,
        _volumes_root: &std::path::Path,
        _wait_secs: u64,
    ) -> Option<RehearsalFate> {
        None
    }
}

/// One log line as the wire wants it: which channel spoke, what it said,
/// and the engine's timestamp when asked for.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LogLine {
    /// stdout | stderr | console
    pub stream: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

/// The result of refreshing one image reference: the IDs before and
/// after the pull, and whether anything changed.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ImageRefresh {
    pub changed: bool,
    /// The image ID the tag resolves to AFTER the pull.
    pub id: String,
}

/// A long-lived logs stream: history first, then follow.
pub type LogStream =
    std::pin::Pin<Box<dyn futures::Stream<Item = Result<LogLine, String>> + Send>>;

/// How a rehearsal container fared (the J2 proof loop's raw material).
#[derive(Debug, Clone, serde::Serialize)]
pub struct RehearsalFate {
    /// The container stayed up the whole wait window without dying.
    pub stayed_running: bool,
    /// Docker's state string after the wait (running / exited / ...).
    pub state: String,
    /// The container's exit code, when it exited.
    pub exit_code: Option<i64>,
    /// Seconds the container actually ran.
    pub ran_secs: u64,
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
