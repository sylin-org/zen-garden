//! Directory domain — model catalog (ORCH-0020).
//!
//! Owns the ModelDirectory. Publishes snapshots via watch.

use std::sync::Arc;
use tokio::sync::{watch, Mutex};

use super::types::*;

// ── Snapshot ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DirectorySnapshot {
    pub directory: Arc<ModelDirectory>,
}

impl DirectorySnapshot {
    pub fn empty() -> Self {
        Self {
            directory: Arc::new(ModelDirectory::new()),
        }
    }
}

// ── Domain ─────────────────────────────────────────────────────

pub struct DirectoryDomain {
    state: Mutex<ModelDirectory>,
    tx: watch::Sender<Arc<DirectorySnapshot>>,
}

impl DirectoryDomain {
    pub fn new(tx: watch::Sender<Arc<DirectorySnapshot>>) -> Self {
        Self {
            state: Mutex::new(ModelDirectory::new()),
            tx,
        }
    }

    pub fn snapshot(&self) -> watch::Ref<'_, Arc<DirectorySnapshot>> {
        self.tx.borrow()
    }

    pub fn subscribe(&self) -> watch::Receiver<Arc<DirectorySnapshot>> {
        self.tx.subscribe()
    }

    pub async fn upsert(
        &self,
        fqn: ModelFqn,
        capabilities: Vec<Capability>,
        specializations: Vec<String>,
        metadata: ModelMetadata,
    ) {
        let mut dir = self.state.lock().await;
        dir.upsert(fqn, capabilities, specializations, metadata);
        self.publish(&dir);
    }

    pub async fn remove_provider(&self, source: &str, locator: &str) {
        let mut dir = self.state.lock().await;
        dir.remove_provider(source, locator);
        self.publish(&dir);
    }

    pub async fn remove_fqn(&self, fqn: &ModelFqn) {
        let mut dir = self.state.lock().await;
        dir.remove_fqn(fqn);
        self.publish(&dir);
    }

    fn publish(&self, dir: &ModelDirectory) {
        let snapshot = Arc::new(DirectorySnapshot {
            directory: Arc::new(dir.clone()),
        });
        let _ = self.tx.send(snapshot);
    }
}
