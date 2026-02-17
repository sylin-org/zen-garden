//! Object storage operations for seed banks
//!
//! Provides S3-compatible object storage operations on seed bank filesystems.
//! Objects are stored under: {mount_path}/garden/storage/{bucket}/{key}
//!
//! Design: This is the infrastructure layer - handles actual filesystem I/O.
//! All content I/O flows through [`SeedBankStore`] (STORAGE-0006 chokepoint).
//! When the store has a DEK, content is encrypted transparently.

use super::store::SeedBankStore;
use anyhow::{Context, Result};
use garden_common::constants::paths;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Metadata about a stored object
#[derive(Debug, Clone)]
pub struct ObjectMetadata {
    /// Key path within the bucket
    pub key: String,
    /// Size in bytes
    pub size: u64,
    /// Last modified timestamp (RFC3339)
    pub last_modified: String,
    /// MD5 hash of content (quoted hex string for ETag)
    pub etag: String,
    /// Content type (MIME)
    pub content_type: String,
}

/// Result of a PUT operation
#[derive(Debug)]
pub struct PutResult {
    /// ETag of the stored object (quoted MD5 hex)
    pub etag: String,
}

/// Object store interface for a specific seed bank
pub struct ObjectStore {
    /// SeedBankStore chokepoint — all content I/O flows through here
    store: SeedBankStore,
    /// Full filesystem path for storage root (for directory walking)
    root_path: PathBuf,
    /// Relative path from mount root to storage dir (e.g., "garden/storage")
    storage_rel: PathBuf,
}

impl ObjectStore {
    /// Create a new object store for a seed bank mount (public / unencrypted).
    ///
    /// For encrypted seed banks, use [`ObjectStore::with_store`] instead.
    pub fn new(mount_path: impl AsRef<Path>) -> Self {
        let root_path = mount_path.as_ref().join(paths::SEED_BANK_STORAGE_DIR);
        Self {
            store: SeedBankStore::new_public(mount_path.as_ref()),
            root_path,
            storage_rel: PathBuf::from(paths::SEED_BANK_STORAGE_DIR),
        }
    }

    /// Create an object store backed by an explicit [`SeedBankStore`].
    ///
    /// Use this when the seed bank may be encrypted.
    pub fn with_store(store: SeedBankStore) -> Self {
        let root_path = store.mount_root().join(paths::SEED_BANK_STORAGE_DIR);
        Self {
            storage_rel: PathBuf::from(paths::SEED_BANK_STORAGE_DIR),
            root_path,
            store,
        }
    }

    /// Relative path from mount root for an object (for SeedBankStore)
    fn object_rel(&self, bucket: &str, key: &str) -> PathBuf {
        self.storage_rel.join(bucket).join(key)
    }

    /// Relative path from mount root for a metadata sidecar
    fn meta_rel(&self, object_rel: &Path) -> PathBuf {
        let file_name = object_rel.file_name().unwrap_or_default().to_string_lossy();
        object_rel.with_file_name(format!("{}.meta.json", file_name))
    }

    /// Get the full filesystem path for a bucket
    fn bucket_path(&self, bucket: &str) -> PathBuf {
        self.root_path.join(bucket)
    }

    /// PUT object - store data with atomic write through SeedBankStore
    pub async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        content_type: &str,
        data: &[u8],
    ) -> Result<PutResult> {
        let rel = self.object_rel(bucket, key);

        // Calculate MD5 hash on plaintext (before any encryption)
        let hash = md5::compute(data);
        let etag = format!("\"{}\"", hex::encode(hash.0));

        // Write content through SeedBankStore (encrypts if dek is set)
        self.store
            .write(&rel, data)
            .await
            .context("Failed to write object data")?;

        // Store metadata in sidecar file (.meta.json)
        let meta = ObjectMetadataSidecar {
            content_type: content_type.to_string(),
            etag: etag.clone(),
            size: data.len() as u64,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        let meta_json = serde_json::to_string(&meta)?;
        let sidecar_rel = self.meta_rel(&rel);
        self.store
            .write_string(&sidecar_rel, &meta_json)
            .await
            .context("Failed to write metadata")?;

        debug!(bucket = %bucket, key = %key, size = data.len(), etag = %etag, "Object stored");

        Ok(PutResult { etag })
    }

    /// GET object - retrieve data (decrypted if encrypted)
    pub async fn get_object(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<Option<(Vec<u8>, ObjectMetadata)>> {
        let rel = self.object_rel(bucket, key);

        if !self.store.exists(&rel).await {
            return Ok(None);
        }

        let data = self
            .store
            .read(&rel)
            .await
            .context("Failed to read object")?;

        let metadata = self.get_metadata_for_rel(&rel, key).await?;

        Ok(Some((data, metadata)))
    }

    /// HEAD object - get metadata only
    pub async fn head_object(&self, bucket: &str, key: &str) -> Result<Option<ObjectMetadata>> {
        let rel = self.object_rel(bucket, key);

        if !self.store.exists(&rel).await {
            return Ok(None);
        }

        self.get_metadata_for_rel(&rel, key).await.map(Some)
    }

    /// DELETE object
    pub async fn delete_object(&self, bucket: &str, key: &str) -> Result<bool> {
        let rel = self.object_rel(bucket, key);

        if !self.store.exists(&rel).await {
            return Ok(false);
        }

        self.store
            .delete(&rel)
            .await
            .context("Failed to delete object")?;

        // Also remove metadata sidecar
        let sidecar_rel = self.meta_rel(&rel);
        let _ = self.store.delete(&sidecar_rel).await;

        debug!(bucket = %bucket, key = %key, "Object deleted");

        Ok(true)
    }

    /// LIST objects - list objects with optional prefix/delimiter
    pub async fn list_objects(
        &self,
        bucket: &str,
        prefix: Option<&str>,
        delimiter: Option<&str>,
        marker: Option<&str>,
        max_keys: usize,
    ) -> Result<ListResult> {
        let bucket_path = self.bucket_path(bucket);

        if !bucket_path.exists() {
            return Ok(ListResult {
                contents: vec![],
                common_prefixes: vec![],
                is_truncated: false,
                next_marker: None,
            });
        }

        let prefix = prefix.unwrap_or("");
        let delimiter = delimiter.map(|s| s.to_string());

        let mut contents = vec![];
        let mut common_prefixes = std::collections::HashSet::new();

        // Recursively list all files
        self.walk_directory(
            &bucket_path,
            &bucket_path,
            bucket,
            prefix,
            &delimiter,
            &mut contents,
            &mut common_prefixes,
        )
        .await?;

        // Sort by key
        contents.sort_by(|a, b| a.key.cmp(&b.key));

        // Apply marker filter
        if let Some(marker) = marker {
            contents.retain(|o| o.key.as_str() > marker);
        }

        // Truncate to max_keys
        let is_truncated = contents.len() > max_keys;
        let next_marker = if is_truncated {
            contents.get(max_keys).map(|o| o.key.clone())
        } else {
            None
        };
        contents.truncate(max_keys);

        // Sort and dedup common prefixes
        let mut common_prefixes: Vec<String> = common_prefixes.into_iter().collect();
        common_prefixes.sort();

        Ok(ListResult {
            contents,
            common_prefixes,
            is_truncated,
            next_marker,
        })
    }

    /// LIST buckets - list all buckets at storage root
    pub async fn list_buckets(&self) -> Result<Vec<String>> {
        let root_dir = self.root_path.clone();

        if !root_dir.exists() {
            return Ok(Vec::new());
        }

        let mut buckets = Vec::new();
        let mut entries = tokio::fs::read_dir(&root_dir)
            .await
            .context("Failed to read storage root directory")?;

        while let Ok(Some(entry)) = entries.next_entry().await {
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    buckets.push(name.to_string());
                }
            }
        }

        buckets.sort();
        Ok(buckets)
    }

    /// Recursively walk directory and collect objects.
    ///
    /// Directory walking uses the filesystem directly (just listing entries).
    /// Content reads for metadata go through SeedBankStore.
    #[allow(clippy::too_many_arguments)]
    fn walk_directory<'a>(
        &'a self,
        base_path: &'a Path,
        current_path: &'a Path,
        bucket: &'a str,
        prefix: &'a str,
        delimiter: &'a Option<String>,
        contents: &'a mut Vec<ObjectMetadata>,
        common_prefixes: &'a mut std::collections::HashSet<String>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let mut entries = match tokio::fs::read_dir(current_path).await {
                Ok(e) => e,
                Err(_) => return Ok(()),
            };

            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();

                // Skip metadata sidecar files and tmp files
                if name.ends_with(".meta.json") || name.ends_with(".tmp") {
                    continue;
                }

                // Get relative key from bucket root
                let relative = path
                    .strip_prefix(base_path)
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default();

                if path.is_dir() {
                    // Handle directory with delimiter
                    if let Some(_delim) = delimiter {
                        // If we have a delimiter, report common prefix
                        let dir_prefix = format!("{}/", relative);
                        if dir_prefix.starts_with(prefix) {
                            common_prefixes.insert(dir_prefix);
                        }
                    } else {
                        // No delimiter - recurse
                        self.walk_directory(
                            base_path,
                            &path,
                            bucket,
                            prefix,
                            delimiter,
                            contents,
                            common_prefixes,
                        )
                        .await?;
                    }
                } else {
                    // It's a file
                    if !relative.starts_with(prefix) {
                        continue;
                    }

                    // With delimiter, check if this file is directly under prefix
                    if let Some(delim) = delimiter {
                        let suffix = &relative[prefix.len()..];
                        if let Some(idx) = suffix.find(delim.as_str()) {
                            // There's a delimiter in the suffix, add as common prefix
                            let common = format!("{}{}", prefix, &suffix[..=idx]);
                            common_prefixes.insert(common);
                            continue;
                        }
                    }

                    // Build store-relative path for reads through SeedBankStore
                    let store_rel = self.object_rel(bucket, &relative);

                    // Get metadata
                    match self.get_metadata_for_rel(&store_rel, &relative).await {
                        Ok(meta) => contents.push(meta),
                        Err(e) => {
                            warn!(path = %path.display(), error = %e, "Failed to get object metadata")
                        }
                    }
                }
            }

            Ok(())
        })
    }

    /// Get metadata for a store-relative path.
    async fn get_metadata_for_rel(&self, rel: &Path, key: &str) -> Result<ObjectMetadata> {
        let full_path = self.store.full_path(rel);
        let file_meta = tokio::fs::metadata(&full_path)
            .await
            .context("Failed to get file metadata")?;

        // Try to read sidecar metadata through SeedBankStore
        let sidecar_rel = self.meta_rel(rel);
        let (content_type, etag, size) = if self.store.exists(&sidecar_rel).await {
            match self.store.read_string(&sidecar_rel).await {
                Ok(json) => match serde_json::from_str::<ObjectMetadataSidecar>(&json) {
                    Ok(m) => (m.content_type, m.etag, m.size),
                    Err(_) => self.compute_metadata(rel, &full_path).await?,
                },
                Err(_) => self.compute_metadata(rel, &full_path).await?,
            }
        } else {
            self.compute_metadata(rel, &full_path).await?
        };

        let last_modified = file_meta
            .modified()
            .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
            .unwrap_or_else(|_| chrono::Utc::now().to_rfc3339());

        Ok(ObjectMetadata {
            key: key.to_string(),
            size,
            last_modified,
            etag,
            content_type,
        })
    }

    /// Compute metadata when sidecar is missing — reads through SeedBankStore (decrypts if needed)
    async fn compute_metadata(
        &self,
        rel: &Path,
        full_path: &Path,
    ) -> Result<(String, String, u64)> {
        let data = self.store.read(rel).await?;
        let hash = md5::compute(&data);
        let etag = format!("\"{}\"", hex::encode(hash.0));

        // Guess content type from extension
        let content_type = mime_guess::from_path(full_path)
            .first_or_octet_stream()
            .to_string();

        Ok((content_type, etag, data.len() as u64))
    }
}

/// Sidecar metadata stored alongside objects
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct ObjectMetadataSidecar {
    content_type: String,
    etag: String,
    size: u64,
    created_at: String,
}

/// Result of a LIST operation
#[derive(Debug)]
pub struct ListResult {
    /// Objects matching the criteria
    pub contents: Vec<ObjectMetadata>,
    /// Common prefixes when delimiter is used
    pub common_prefixes: Vec<String>,
    /// True if there are more results
    pub is_truncated: bool,
    /// Marker for next page
    pub next_marker: Option<String>,
}
