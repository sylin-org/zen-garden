//! Disk-backed request store (ORCH-0033).
//!
//! Mirrors the `DiskJobStore` pattern: in-memory HashMap index,
//! file-backed persistence at `{data_dir}/requests/{id}.json`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

use crate::domain::persisted_request::{
    ErrorSnapshot, PersistedRequest, RequestFilter, RequestMedia, RequestMeta, RequestStatus,
};

#[derive(Debug, thiserror::Error)]
pub enum RequestStoreError {
    #[error("request not found: {0}")]
    NotFound(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

pub struct DiskRequestStore {
    root: PathBuf,
    requests: RwLock<HashMap<String, PersistedRequest>>,
}

impl DiskRequestStore {
    /// Load existing requests from disk and return the store.
    pub async fn load(data_dir: &Path) -> Result<Arc<Self>, RequestStoreError> {
        let root = data_dir.join("requests");
        tokio::fs::create_dir_all(&root).await?;
        let store = Arc::new(Self {
            root,
            requests: RwLock::new(HashMap::new()),
        });
        store.scan().await?;
        Ok(store)
    }

    /// Scan the requests directory and load all valid JSON files.
    async fn scan(&self) -> Result<(), RequestStoreError> {
        let mut entries = tokio::fs::read_dir(&self.root).await?;
        let mut loaded = 0u32;
        let mut skipped = 0u32;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                match tokio::fs::read_to_string(&path).await {
                    Ok(content) => match serde_json::from_str::<PersistedRequest>(&content) {
                        Ok(req) => {
                            self.requests.write().await.insert(req.id.clone(), req);
                            loaded += 1;
                        }
                        Err(e) => {
                            tracing::warn!(path = %path.display(), error = %e, "skipping malformed request file");
                            skipped += 1;
                        }
                    },
                    Err(e) => {
                        tracing::warn!(path = %path.display(), error = %e, "skipping unreadable request file");
                        skipped += 1;
                    }
                }
            }
        }

        if loaded > 0 || skipped > 0 {
            tracing::info!(loaded, skipped, "request store scan complete");
        }
        Ok(())
    }

    /// Persist a request record to disk.
    async fn persist(&self, req: &PersistedRequest) -> Result<(), RequestStoreError> {
        let path = self.root.join(format!("{}.json", req.id));
        let json = serde_json::to_string_pretty(req)?;
        tokio::fs::write(&path, json).await?;
        Ok(())
    }

    /// Insert a new request (status: Running).
    pub async fn create(&self, req: PersistedRequest) -> Result<(), RequestStoreError> {
        self.persist(&req).await?;
        self.requests.write().await.insert(req.id.clone(), req);
        Ok(())
    }

    /// Get a request by ID.
    pub async fn get(&self, id: &str) -> Result<PersistedRequest, RequestStoreError> {
        self.requests
            .read()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| RequestStoreError::NotFound(id.to_string()))
    }

    /// List requests matching the given filter, newest first.
    pub async fn list(&self, filter: &RequestFilter) -> Vec<PersistedRequest> {
        let requests = self.requests.read().await;
        let mut results: Vec<&PersistedRequest> = requests
            .values()
            .filter(|r| {
                if let Some(ref action) = filter.action {
                    if !r.action.contains(action.as_str()) {
                        return false;
                    }
                }
                if let Some(status) = filter.status {
                    if r.status != status {
                        return false;
                    }
                }
                if let Some(pinned) = filter.pinned {
                    if r.pinned != pinned {
                        return false;
                    }
                }
                if let Some(ref parent_id) = filter.parent_id {
                    if r.parent_id.as_deref() != Some(parent_id.as_str()) {
                        return false;
                    }
                }
                if let Some(before) = filter.before {
                    if r.created_at >= before {
                        return false;
                    }
                }
                if let Some(after) = filter.after {
                    if r.created_at <= after {
                        return false;
                    }
                }
                true
            })
            .collect();

        // Newest first.
        results.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        if let Some(limit) = filter.limit {
            results.truncate(limit);
        }

        results.into_iter().cloned().collect()
    }

    /// Mark a request as successfully completed.
    pub async fn complete(
        &self,
        id: &str,
        output: serde_json::Value,
        media_outputs: Vec<RequestMedia>,
        meta: RequestMeta,
    ) -> Result<(), RequestStoreError> {
        let mut requests = self.requests.write().await;
        let req = requests
            .get_mut(id)
            .ok_or_else(|| RequestStoreError::NotFound(id.to_string()))?;
        req.complete(output, media_outputs, meta);
        let cloned = req.clone();
        drop(requests);
        self.persist(&cloned).await
    }

    /// Mark a request as failed.
    pub async fn fail(
        &self,
        id: &str,
        error: ErrorSnapshot,
    ) -> Result<(), RequestStoreError> {
        let mut requests = self.requests.write().await;
        let req = requests
            .get_mut(id)
            .ok_or_else(|| RequestStoreError::NotFound(id.to_string()))?;
        req.fail(error);
        let cloned = req.clone();
        drop(requests);
        self.persist(&cloned).await
    }

    /// Toggle the pinned status of a request.
    pub async fn toggle_pin(&self, id: &str) -> Result<bool, RequestStoreError> {
        let mut requests = self.requests.write().await;
        let req = requests
            .get_mut(id)
            .ok_or_else(|| RequestStoreError::NotFound(id.to_string()))?;
        req.pinned = !req.pinned;
        let pinned = req.pinned;
        let cloned = req.clone();
        drop(requests);
        self.persist(&cloned).await?;
        Ok(pinned)
    }

    /// Walk the ancestor chain (parent_id → parent's parent_id → ...).
    pub async fn lineage(&self, id: &str) -> Result<Vec<PersistedRequest>, RequestStoreError> {
        let requests = self.requests.read().await;
        let mut chain = Vec::new();
        let mut current_id = Some(id.to_string());

        while let Some(ref cid) = current_id {
            if let Some(req) = requests.get(cid.as_str()) {
                chain.push(req.clone());
                current_id = req.parent_id.clone();
            } else {
                break;
            }
            // Safety: cap at 100 to prevent cycles.
            if chain.len() >= 100 {
                break;
            }
        }

        Ok(chain)
    }

    /// Flush unpinned requests older than the given timestamp.
    /// Returns the number of requests removed.
    pub async fn flush(
        &self,
        before: DateTime<Utc>,
    ) -> Result<u32, RequestStoreError> {
        let mut requests = self.requests.write().await;
        let to_remove: Vec<String> = requests
            .values()
            .filter(|r| !r.pinned && r.created_at < before)
            .map(|r| r.id.clone())
            .collect();

        let count = to_remove.len() as u32;
        for id in &to_remove {
            requests.remove(id);
            let path = self.root.join(format!("{id}.json"));
            let _ = tokio::fs::remove_file(&path).await;
        }
        drop(requests);

        if count > 0 {
            tracing::info!(count, %before, "flushed unpinned requests");
        }
        Ok(count)
    }

    /// Get all media IDs referenced by pinned requests (for reaper exemption).
    pub async fn pinned_media_ids(&self) -> Vec<String> {
        self.requests
            .read()
            .await
            .values()
            .filter(|r| r.pinned)
            .flat_map(|r| {
                r.media_inputs
                    .iter()
                    .chain(r.media_outputs.iter())
                    .map(|m| m.media_id.clone())
            })
            .collect()
    }
}
