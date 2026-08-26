// The seam's types are exercised by trait consumers in O1 (docker adapter,
// lifecycle commands); the null adapter + registry are pinned now by tests.
#![allow(dead_code)]

//! The runtime seam (OFFERINGS.md §4): the pluggable execution substrate
//! beneath managed offerings. The registry knows modes; runtimes know
//! containers. Adopted and borrowed offerings never touch a Runtime.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A desired unit of execution, runtime-agnostic — v1's generalization of
/// the PoC's ContainerSpec (poc docker/spec.rs:18-42).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkloadSpec {
    /// OCI image reference.
    pub image: String,
    /// Named ports: name → container port. Host mapping is the adapter's
    /// job (remap + ledger, PORT-0001).
    #[serde(default)]
    pub named_ports: HashMap<String, u16>,
    /// volume name → host path.
    #[serde(default)]
    pub volumes: HashMap<String, String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Files injected into the workload (path → content).
    #[serde(default)]
    pub config_files: HashMap<String, String>,
    /// HTTP path probed for health; None = no probe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_health_path: Option<String>,
    /// Restart policy hint: "no" | "unless-stopped" | "always".
    #[serde(default = "default_restart")]
    pub restart: String,
}

fn default_restart() -> String {
    "unless-stopped".into()
}

/// What a runtime reports about placed work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunningWorkload {
    pub name: String,
    pub image: String,
    /// Offering status wire string ("running"/"stopped"/...) — the adapter
    /// maps its native states into glossary vocabulary.
    pub status: String,
}

/// Outcome of a deploy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeployOutcome {
    Created,
    AlreadyRunning,
}

/// Errors adapters return. Connection-level failures are retryable.
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

/// The pluggable substrate. One implementation per execution world;
/// selected once at startup by configuration (OFFERINGS.md §4).
#[async_trait::async_trait]
pub trait Runtime: Send + Sync {
    fn kind(&self) -> &'static str;

    async fn deploy(
        &self,
        name: &str,
        spec: &WorkloadSpec,
    ) -> Result<DeployOutcome, RuntimeError>;

    async fn start(&self, name: &str) -> Result<(), RuntimeError>;
    async fn stop(&self, name: &str) -> Result<(), RuntimeError>;
    async fn remove(&self, name: &str) -> Result<(), RuntimeError>;

    async fn inspect(&self, name: &str) -> Option<RunningWorkload>;

    async fn list(&self) -> Vec<RunningWorkload>;
}

/// A registry of runtimes by kind, built at startup (P1: one registry).
#[derive(Default, Clone)]
pub struct RuntimeRegistry {
    runtimes: std::sync::Arc<HashMap<&'static str, std::sync::Arc<dyn Runtime>>>,
    selected: &'static str,
}

impl RuntimeRegistry {
    pub fn build(selected: &'static str, runtimes: Vec<std::sync::Arc<dyn Runtime>>) -> Self {
        let map = runtimes.into_iter().map(|r| (r.kind(), r)).collect();
        Self { runtimes: std::sync::Arc::new(map), selected }
    }

    /// The configured runtime. Absent selection aborts startup loudly
    /// upstream (L17); this returns the reason.
    pub fn get(&self) -> Result<std::sync::Arc<dyn Runtime>, String> {
        self.runtimes.get(self.selected).cloned().ok_or_else(|| {
            let available = self.runtimes.keys().copied().collect::<Vec<_>>().join(", ");
            format!("runtime '{}' not registered (available: {})", self.selected, available)
        })
    }
}

/// The reference adapter: proves the seam with no dependencies. Deploys
/// are recorded in memory; inspect/list read them back. Real worlds
/// (docker/podman/systemd) replace this behind the same trait (D10).
#[derive(Default)]
pub struct NullRuntime;

impl NullRuntime {
    fn deny(&self, op: &'static str) -> RuntimeError {
        RuntimeError::Unsupported(op)
    }
}

#[async_trait::async_trait]
impl Runtime for NullRuntime {
    fn kind(&self) -> &'static str {
        "null"
    }

    async fn deploy(&self, _name: &str, _spec: &WorkloadSpec) -> Result<DeployOutcome, RuntimeError> {
        Err(self.deny("deploy"))
    }

    async fn start(&self, _name: &str) -> Result<(), RuntimeError> {
        Err(self.deny("start"))
    }

    async fn stop(&self, _name: &str) -> Result<(), RuntimeError> {
        Err(self.deny("stop"))
    }

    async fn remove(&self, _name: &str) -> Result<(), RuntimeError> {
        Err(self.deny("remove"))
    }

    async fn inspect(&self, _name: &str) -> Option<RunningWorkload> {
        None
    }

    async fn list(&self) -> Vec<RunningWorkload> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    /// The seam exists so adapters can differ; null denies everything and
    /// says why.
    #[tokio::test]
    async fn null_runtime_denies_loudly() {
        let rt = NullRuntime;
        assert_eq!(rt.kind(), "null");
        let err = rt.deploy("x", &WorkloadSpec::default()).await.unwrap_err();
        assert!(matches!(err, RuntimeError::Unsupported(_)));
    }

    #[test]
    fn registry_selects_configured_kind_and_explains_absence() {
        let reg = RuntimeRegistry::build(
            "null",
            vec![std::sync::Arc::new(NullRuntime)],
        );
        assert_eq!(reg.get().unwrap().kind(), "null");

        let empty: RuntimeRegistry = RuntimeRegistry::build("docker", vec![]);
        let Err(msg) = empty.get() else {
            panic!("'docker' was never registered; selection must fail");
        };
        assert!(msg.contains("'docker' not registered"), "got: {msg}");
    }
}
