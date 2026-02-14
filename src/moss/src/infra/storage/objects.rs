//! Object storage operations for seed banks
//!
//! Provides S3-compatible object storage operations on seed bank filesystems.
//! Objects are stored under: {mount_path}/garden/storage/{bucket}/{key}
//!
//! Design: This is the infrastructure layer - handles actual filesystem I/O.
//! Business logic (path validation, quota enforcement) should be in domain layer.

use anyhow::{Context, Result};
use garden_common::constants::paths;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
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
    /// Root path for storage objects (garden/storage)
    root_path: PathBuf,
}

impl ObjectStore {
    /// Create a new object store for a seed bank mount
    pub fn new(mount_path: impl AsRef<Path>) -> Self {
        let root_path = mount_path.as_ref().join(paths::SEED_BANK_STORAGE_DIR);
        Self { root_path }
    }

    /// Get the full filesystem path for an object
    fn object_path(&self, bucket: &str, key: &str) -> PathBuf {
        // Structure: {mount}/garden/storage/{bucket}/{key}
        self.root_path.join(bucket).join(key)
    }

    /// Get the full filesystem path for a bucket
    fn bucket_path(&self, bucket: &str) -> PathBuf {
        self.root_path.join(bucket)
    }

    /// PUT object - store data with streaming hash
    pub async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        content_type: &str,
        data: &[u8],
    ) -> Result<PutResult> {
        let path = self.object_path(bucket, key);

        // Ensure parent directories exist
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .context("Failed to create object directories")?;
        }

        // Calculate MD5 hash using md5::compute
        let hash = md5::compute(data);
        let etag = format!("\"{}\"", hex::encode(hash.0));

        // Write object atomically: temp file → fsync → rename
        let tmp_path = path.with_extension("tmp");

        let mut file = tokio::fs::File::create(&tmp_path)
            .await
            .context("Failed to create temp file")?;

        file.write_all(data)
            .await
            .context("Failed to write object data")?;

        file.sync_all()
            .await
            .context("Failed to sync object file")?;

        drop(file);

        tokio::fs::rename(&tmp_path, &path)
            .await
            .context("Failed to rename temp file")?;

        // Store metadata in sidecar file (.meta.json)
        let meta = ObjectMetadataSidecar {
            content_type: content_type.to_string(),
            etag: etag.clone(),
            size: data.len() as u64,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        let meta_path = self.meta_path(&path);
        let meta_json = serde_json::to_string(&meta)?;
        tokio::fs::write(&meta_path, meta_json)
            .await
            .context("Failed to write metadata")?;

        debug!(bucket = %bucket, key = %key, size = data.len(), etag = %etag, "Object stored");

        Ok(PutResult { etag })
    }

    /// GET object - retrieve data
    pub async fn get_object(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<Option<(Vec<u8>, ObjectMetadata)>> {
        let path = self.object_path(bucket, key);

        if !path.exists() {
            return Ok(None);
        }

        let data = tokio::fs::read(&path)
            .await
            .context("Failed to read object")?;

        let metadata = self.get_metadata_for_path(&path, key).await?;

        Ok(Some((data, metadata)))
    }

    /// HEAD object - get metadata only
    pub async fn head_object(&self, bucket: &str, key: &str) -> Result<Option<ObjectMetadata>> {
        let path = self.object_path(bucket, key);

        if !path.exists() {
            return Ok(None);
        }

        self.get_metadata_for_path(&path, key).await.map(Some)
    }

    /// DELETE object
    pub async fn delete_object(&self, bucket: &str, key: &str) -> Result<bool> {
        let path = self.object_path(bucket, key);

        if !path.exists() {
            return Ok(false);
        }

        tokio::fs::remove_file(&path)
            .await
            .context("Failed to delete object")?;

        // Also remove metadata sidecar
        let meta_path = self.meta_path(&path);
        let _ = tokio::fs::remove_file(&meta_path).await;

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

    /// Recursively walk directory and collect objects
    fn walk_directory<'a>(
        &'a self,
        base_path: &'a Path,
        current_path: &'a Path,
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

                // Skip metadata sidecar files
                if name.ends_with(".meta.json") {
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

                    // Get metadata
                    match self.get_metadata_for_path(&path, &relative).await {
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

    /// Get metadata for a file path
    async fn get_metadata_for_path(&self, path: &Path, key: &str) -> Result<ObjectMetadata> {
        let file_meta = tokio::fs::metadata(path)
            .await
            .context("Failed to get file metadata")?;

        // Try to read sidecar metadata
        let meta_path = self.meta_path(path);
        let (content_type, etag) = if meta_path.exists() {
            match tokio::fs::read_to_string(&meta_path).await {
                Ok(json) => match serde_json::from_str::<ObjectMetadataSidecar>(&json) {
                    Ok(m) => (m.content_type, m.etag),
                    Err(_) => self.compute_metadata(path).await?,
                },
                Err(_) => self.compute_metadata(path).await?,
            }
        } else {
            self.compute_metadata(path).await?
        };

        let last_modified = file_meta
            .modified()
            .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
            .unwrap_or_else(|_| chrono::Utc::now().to_rfc3339());

        Ok(ObjectMetadata {
            key: key.to_string(),
            size: file_meta.len(),
            last_modified,
            etag,
            content_type,
        })
    }

    /// Compute metadata when sidecar is missing
    async fn compute_metadata(&self, path: &Path) -> Result<(String, String)> {
        let data = tokio::fs::read(path).await?;
        let hash = md5::compute(&data);
        let etag = format!("\"{}\"", hex::encode(hash.0));

        // Guess content type from extension
        let content_type = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();

        Ok((content_type, etag))
    }

    /// Get sidecar metadata file path
    fn meta_path(&self, object_path: &Path) -> PathBuf {
        let file_name = object_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        object_path.with_file_name(format!("{}.meta.json", file_name))
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
