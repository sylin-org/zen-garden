//! Multipart upload state management (STORAGE-0016 §2e)
//!
//! Stores upload state and parts in `.zen-garden/multipart/{upload_id}/`.
//! Parts are staged as numbered files. On `CompleteMultipartUpload`, parts
//! are concatenated and written via `ObjectStore::put_object()` (which enters
//! the changelog for replication).
//!
//! Incomplete uploads are garbage-collected after 24h by the storage lifecycle task.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// State for a single multipart upload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultipartUpload {
    pub upload_id: String,
    pub bucket: String,
    pub key: String,
    pub content_type: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// part_number → (etag, size)
    pub parts: HashMap<u16, PartInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartInfo {
    pub etag: String,
    pub size: u64,
}

/// Manages multipart upload state on the filesystem
pub struct MultipartStore {
    /// Base path: mount_root/.zen-garden/multipart/
    base_path: PathBuf,
}

impl MultipartStore {
    pub fn new(mount_path: &Path) -> Self {
        Self {
            base_path: mount_path.join(".zen-garden").join("multipart"),
        }
    }

    fn upload_dir(&self, upload_id: &str) -> PathBuf {
        self.base_path.join(upload_id)
    }

    fn manifest_path(&self, upload_id: &str) -> PathBuf {
        self.upload_dir(upload_id).join("manifest.json")
    }

    fn part_path(&self, upload_id: &str, part_number: u16) -> PathBuf {
        self.upload_dir(upload_id).join(format!("{:05}", part_number))
    }

    /// Initiate a new multipart upload. Returns the upload ID.
    pub async fn initiate(
        &self,
        bucket: &str,
        key: &str,
        content_type: &str,
    ) -> Result<String> {
        let upload_id = uuid::Uuid::now_v7().to_string();
        let upload = MultipartUpload {
            upload_id: upload_id.clone(),
            bucket: bucket.to_string(),
            key: key.to_string(),
            content_type: content_type.to_string(),
            created_at: chrono::Utc::now(),
            parts: HashMap::new(),
        };

        let dir = self.upload_dir(&upload_id);
        tokio::fs::create_dir_all(&dir)
            .await
            .context("Failed to create multipart upload directory")?;

        let manifest = serde_json::to_string_pretty(&upload)?;
        tokio::fs::write(self.manifest_path(&upload_id), manifest)
            .await
            .context("Failed to write multipart manifest")?;

        debug!(upload_id = %upload_id, bucket = %bucket, key = %key, "Multipart upload initiated");
        Ok(upload_id)
    }

    /// Store a part. Returns the part's ETag.
    pub async fn upload_part(
        &self,
        upload_id: &str,
        part_number: u16,
        data: &[u8],
    ) -> Result<String> {
        let mut upload = self.load_manifest(upload_id).await?;

        // Write part data
        let part_file = self.part_path(upload_id, part_number);
        tokio::fs::write(&part_file, data)
            .await
            .context("Failed to write part data")?;

        // Calculate ETag for this part
        let hash = md5::compute(data);
        let etag = format!("\"{}\"", hex::encode(hash.0));

        // Update manifest
        upload.parts.insert(
            part_number,
            PartInfo {
                etag: etag.clone(),
                size: data.len() as u64,
            },
        );
        self.save_manifest(upload_id, &upload).await?;

        debug!(upload_id = %upload_id, part_number, size = data.len(), "Part uploaded");
        Ok(etag)
    }

    /// Complete the upload: concatenate parts in order, return assembled data.
    pub async fn complete(
        &self,
        upload_id: &str,
        part_numbers: &[u16],
    ) -> Result<(Vec<u8>, MultipartUpload)> {
        let upload = self.load_manifest(upload_id).await?;

        // Validate all requested parts exist
        for pn in part_numbers {
            if !upload.parts.contains_key(pn) {
                anyhow::bail!("Part {} not found in upload {}", pn, upload_id);
            }
        }

        // Concatenate parts in order
        let mut assembled = Vec::new();
        for pn in part_numbers {
            let part_file = self.part_path(upload_id, *pn);
            let data = tokio::fs::read(&part_file)
                .await
                .context(format!("Failed to read part {}", pn))?;
            assembled.extend_from_slice(&data);
        }

        debug!(
            upload_id = %upload_id,
            parts = part_numbers.len(),
            total_size = assembled.len(),
            "Multipart upload assembled"
        );

        Ok((assembled, upload))
    }

    /// Abort and clean up an upload
    pub async fn abort(&self, upload_id: &str) -> Result<()> {
        let dir = self.upload_dir(upload_id);
        if dir.exists() {
            tokio::fs::remove_dir_all(&dir)
                .await
                .context("Failed to remove multipart upload directory")?;
            debug!(upload_id = %upload_id, "Multipart upload aborted");
        }
        Ok(())
    }

    /// Clean up the upload directory after successful completion
    pub async fn cleanup(&self, upload_id: &str) -> Result<()> {
        self.abort(upload_id).await
    }

    /// Garbage-collect expired uploads older than max_age
    pub async fn gc(&self, max_age: chrono::Duration) {
        let cutoff = chrono::Utc::now() - max_age;

        let Ok(mut entries) = tokio::fs::read_dir(&self.base_path).await else {
            return;
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let upload_id = entry.file_name().to_string_lossy().to_string();
            if let Ok(upload) = self.load_manifest(&upload_id).await {
                if upload.created_at < cutoff {
                    warn!(upload_id = %upload_id, created = %upload.created_at, "GC: removing expired multipart upload");
                    let _ = self.abort(&upload_id).await;
                }
            }
        }
    }

    async fn load_manifest(&self, upload_id: &str) -> Result<MultipartUpload> {
        let path = self.manifest_path(upload_id);
        let data = tokio::fs::read_to_string(&path)
            .await
            .context("Upload not found or manifest unreadable")?;
        let upload: MultipartUpload =
            serde_json::from_str(&data).context("Failed to parse multipart manifest")?;
        Ok(upload)
    }

    async fn save_manifest(&self, upload_id: &str, upload: &MultipartUpload) -> Result<()> {
        let manifest = serde_json::to_string_pretty(upload)?;
        tokio::fs::write(self.manifest_path(upload_id), manifest)
            .await
            .context("Failed to write multipart manifest")?;
        Ok(())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn multipart_initiate_creates_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MultipartStore::new(tmp.path());

        let id = store.initiate("bucket", "key.dat", "application/octet-stream").await.unwrap();
        assert!(!id.is_empty());
        assert!(store.manifest_path(&id).exists());
    }

    #[tokio::test]
    async fn multipart_upload_part_stores_data() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MultipartStore::new(tmp.path());

        let id = store.initiate("b", "k", "text/plain").await.unwrap();
        let etag = store.upload_part(&id, 1, b"hello").await.unwrap();
        assert!(!etag.is_empty());

        // Part file should exist
        assert!(store.part_path(&id, 1).exists());
    }

    #[tokio::test]
    async fn multipart_complete_concatenates_parts() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MultipartStore::new(tmp.path());

        let id = store.initiate("b", "k", "text/plain").await.unwrap();
        store.upload_part(&id, 1, b"hello ").await.unwrap();
        store.upload_part(&id, 2, b"world").await.unwrap();

        let (data, upload) = store.complete(&id, &[1, 2]).await.unwrap();
        assert_eq!(&data, b"hello world");
        assert_eq!(upload.bucket, "b");
        assert_eq!(upload.key, "k");
    }

    #[tokio::test]
    async fn multipart_complete_fails_missing_part() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MultipartStore::new(tmp.path());

        let id = store.initiate("b", "k", "text/plain").await.unwrap();
        store.upload_part(&id, 1, b"hello").await.unwrap();

        let result = store.complete(&id, &[1, 2]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn multipart_abort_cleans_up() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MultipartStore::new(tmp.path());

        let id = store.initiate("b", "k", "text/plain").await.unwrap();
        store.upload_part(&id, 1, b"data").await.unwrap();
        assert!(store.upload_dir(&id).exists());

        store.abort(&id).await.unwrap();
        assert!(!store.upload_dir(&id).exists());
    }
}
