//! Intelligence domain — reactive recommendations (ORCH-0020).
//!
//! Subscribes to Registry + Directory changes, recomputes recommendations
//! asynchronously. Never blocks any writer.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{watch, RwLock};
use tokio_util::sync::CancellationToken;

use super::recommendation;
use super::registry::RegistrySnapshot;
use super::directory_domain::DirectorySnapshot;
use super::fitness::BenchmarkRun;
use super::types::OrchestratorConfig;

// ── Snapshot ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct IntelligenceSnapshot {
    pub recommendations: Arc<HashMap<String, String>>,
}

impl IntelligenceSnapshot {
    pub fn empty() -> Self {
        Self {
            recommendations: Arc::new(HashMap::new()),
        }
    }
}

// ── Domain ─────────────────────────────────────────────────────

/// The domain object — lives in AppState behind Arc.
pub struct IntelligenceDomain {
    tx: watch::Sender<Arc<IntelligenceSnapshot>>,
}

/// The reactive runner — spawned as a background task, owns the watch receivers.
pub struct IntelligenceRunner {
    tx: watch::Sender<Arc<IntelligenceSnapshot>>,
    registry_rx: watch::Receiver<Arc<RegistrySnapshot>>,
    directory_rx: watch::Receiver<Arc<DirectorySnapshot>>,
}

/// All capabilities that recommendations are computed for.
const ALL_CAPABILITIES: &[&str] = &[
    "quick", "chat", "synthesis", "vision", "ocr", "tools", "thinking",
    "embedding", "image", "video", "transcribe", "speech", "music",
    "rerank", "translate",
];

impl IntelligenceDomain {
    pub fn new(tx: watch::Sender<Arc<IntelligenceSnapshot>>) -> Self {
        Self { tx }
    }

    pub fn snapshot(&self) -> watch::Ref<'_, Arc<IntelligenceSnapshot>> {
        self.tx.borrow()
    }

    pub fn subscribe(&self) -> watch::Receiver<Arc<IntelligenceSnapshot>> {
        self.tx.subscribe()
    }

    /// Clone the sender for the IntelligenceRunner.
    pub fn tx_clone(&self) -> watch::Sender<Arc<IntelligenceSnapshot>> {
        self.tx.clone()
    }
}

impl IntelligenceRunner {
    pub fn new(
        tx: watch::Sender<Arc<IntelligenceSnapshot>>,
        registry_rx: watch::Receiver<Arc<RegistrySnapshot>>,
        directory_rx: watch::Receiver<Arc<DirectorySnapshot>>,
    ) -> Self {
        Self {
            tx,
            registry_rx,
            directory_rx,
        }
    }

    /// Reactive loop — subscribes to Registry + Directory, recomputes on change.
    ///
    /// Debounces 50ms to batch rapid discovery updates.
    pub async fn run(
        mut self,
        config: Arc<RwLock<OrchestratorConfig>>,
        benchmark_run: Arc<RwLock<BenchmarkRun>>,
        shutdown: CancellationToken,
    ) {
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => return,
                _ = self.registry_rx.changed() => {}
                _ = self.directory_rx.changed() => {}
            }

            // Debounce: batch rapid updates
            tokio::time::sleep(Duration::from_millis(50)).await;

            // Drain any additional pending changes
            let _ = self.registry_rx.has_changed();
            let _ = self.directory_rx.has_changed();

            let reg = self.registry_rx.borrow().clone();
            let dir = self.directory_rx.borrow().clone();
            let gpu_matrix = benchmark_run.read().await.gpu_matrix.clone();
            let pins = config.read().await.features.pins.clone();

            let mut cache = HashMap::with_capacity(ALL_CAPABILITIES.len());
            for &cap in ALL_CAPABILITIES {
                let pin = pins.get(cap).map(|s| s.as_str());
                let resp = recommendation::recommend(
                    cap,
                    &dir.directory,
                    &reg.instances,
                    &gpu_matrix,
                    pin,
                );
                if let Some(selected) = resp.selected {
                    cache.insert(cap.to_string(), selected);
                }
            }

            let _ = self.tx.send(Arc::new(IntelligenceSnapshot {
                recommendations: Arc::new(cache),
            }));
        }
    }
}
