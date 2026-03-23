//! Object storage operations for managed storage (STORAGE-0016 unified namespace)
//!
//! Provides S3-compatible object storage on managed storage filesystems.
//! Objects stored at: `{mount_path}/{bucket}/{key}` (mount root — same as native files).
//! Metadata sidecars at: `{mount_path}/.zen-garden/meta/{bucket}/{key}.json`.
//!
//! The unified namespace means S3 writes generate changelog entries and are
//! replicated automatically via STORAGE-0006 machinery. S3 objects and native
//! REST/WebDAV files share the same directories.
//!
//! Design: Infrastructure layer — handles actual filesystem I/O.
//! All content I/O flows through [`ContentStore`] (STORAGE-0006 chokepoint).
//! When the store has a DEK, content is encrypted transparently.

use super::store::ContentStore;
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
    /// Custom metadata from x-amz-meta-* headers
    pub custom_metadata: std::collections::HashMap<String, String>,
}

/// Result of a PUT operation
#[derive(Debug)]
pub struct PutResult {
    /// ETag of the stored object (quoted MD5 hex)
    pub etag: String,
}

/// Object store interface for a specific seed bank (STORAGE-0016 unified namespace).
///
/// Objects live at the mount root (`{bucket}/{key}`), sharing the namespace with
/// native REST/WebDAV files. Metadata sidecars live under `.zen-garden/meta/`.
pub struct ObjectStore {
    /// ContentStore chokepoint — all content I/O flows through here
    store: ContentStore,
    /// Full filesystem path for storage mount root (for directory walking)
    root_path: PathBuf,
    /// Relative path from mount root to metadata sidecar dir (.zen-garden/meta)
    meta_rel: PathBuf,
}

impl ObjectStore {
    /// Create a new object store for a seed bank mount (public / unencrypted).
    ///
    /// For encrypted seed banks, use [`ObjectStore::with_store`] instead.
    pub fn new(mount_path: impl AsRef<Path>) -> Self {
        let root_path = mount_path.as_ref().to_path_buf();
        Self {
            store: ContentStore::new_public(mount_path.as_ref()),
            root_path,
            meta_rel: PathBuf::from(paths::STORAGE_OBJECTS_META_DIR),
        }
    }

    /// Create an object store backed by an explicit [`ContentStore`].
    ///
    /// Use this when the seed bank may be encrypted.
    pub fn with_store(store: ContentStore) -> Self {
        let root_path = store.mount_root().to_path_buf();
        Self {
            meta_rel: PathBuf::from(paths::STORAGE_OBJECTS_META_DIR),
            root_path,
            store,
        }
    }

    /// Relative path from mount root for an object (for ContentStore).
    /// Objects live at mount root: `{bucket}/{key}`.
    fn object_rel(&self, bucket: &str, key: &str) -> PathBuf {
        Path::new(bucket).join(key)
    }

    /// Relative path from mount root for a metadata sidecar.
    /// Sidecars live under `.zen-garden/meta/{bucket}/{key}.json`.
    fn sidecar_rel(&self, bucket: &str, key: &str) -> PathBuf {
        self.meta_rel.join(bucket).join(format!("{}.json", key))
    }

    /// Get the full filesystem path for a bucket directory.
    fn bucket_path(&self, bucket: &str) -> PathBuf {
        self.root_path.join(bucket)
    }

    /// PUT object - store data with atomic write through ContentStore
    pub async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        content_type: &str,
        data: &[u8],
    ) -> Result<PutResult> {
        self.put_object_with_metadata(bucket, key, content_type, data, Default::default())
            .await
    }

    /// PUT object with custom metadata (x-amz-meta-* headers)
    pub async fn put_object_with_metadata(
        &self,
        bucket: &str,
        key: &str,
        content_type: &str,
        data: &[u8],
        custom_metadata: std::collections::HashMap<String, String>,
    ) -> Result<PutResult> {
        let rel = self.object_rel(bucket, key);

        // Calculate MD5 hash on plaintext (before any encryption)
        let hash = md5::compute(data);
        let etag = format!("\"{}\"", hex::encode(hash.0));

        // Write content through ContentStore (encrypts if dek is set)
        self.store
            .write(&rel, data)
            .await
            .context("Failed to write object data")?;

        // Store metadata in sidecar file (.zen-garden/meta/{bucket}/{key}.json)
        let meta = ObjectMetadataSidecar {
            content_type: content_type.to_string(),
            etag: etag.clone(),
            size: data.len() as u64,
            created_at: chrono::Utc::now().to_rfc3339(),
            custom_metadata,
        };
        let meta_json = serde_json::to_string(&meta)?;
        let sidecar = self.sidecar_rel(bucket, key);
        self.store
            .write_string(&sidecar, &meta_json)
            .await
            .context("Failed to write metadata")?;

        debug!(bucket = %bucket, key = %key, size = data.len(), etag = %etag, "Object stored");

        Ok(PutResult { etag })
    }

    /// Check if a bucket directory exists at the mount root.
    pub fn bucket_exists(&self, bucket: &str) -> bool {
        self.bucket_path(bucket).is_dir()
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

        let metadata = self.get_metadata(bucket, key).await?;

        Ok(Some((data, metadata)))
    }

    /// GET object range — retrieve a byte range (decrypted if encrypted).
    ///
    /// Returns `(range_data, total_size, metadata)` or `None` if object does not exist.
    pub async fn get_object_range(
        &self,
        bucket: &str,
        key: &str,
        offset: u64,
        length: u64,
    ) -> Result<Option<(Vec<u8>, u64, ObjectMetadata)>> {
        let rel = self.object_rel(bucket, key);

        if !self.store.exists(&rel).await {
            return Ok(None);
        }

        let metadata = self.get_metadata(bucket, key).await?;
        let total_size = metadata.size;

        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let data = self
            .store
            .read_range(&rel_str, offset, length)
            .await
            .context("Failed to read object range")?;

        Ok(Some((data, total_size, metadata)))
    }

    /// HEAD object - get metadata only
    pub async fn head_object(&self, bucket: &str, key: &str) -> Result<Option<ObjectMetadata>> {
        let rel = self.object_rel(bucket, key);

        if !self.store.exists(&rel).await {
            return Ok(None);
        }

        self.get_metadata(bucket, key).await.map(Some)
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
        let sidecar = self.sidecar_rel(bucket, key);
        let _ = self.store.delete(&sidecar).await;

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

        let mut contents = Vec::with_capacity(max_keys);
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

    /// LIST buckets - list all directories at mount root as buckets.
    /// Excludes dotfolders (.zen-garden, etc.) since those are infrastructure.
    pub async fn list_buckets(&self) -> Result<Vec<String>> {
        let root_dir = &self.root_path;

        if !root_dir.exists() {
            return Ok(Vec::new());
        }

        let mut buckets = Vec::new();
        let mut entries = tokio::fs::read_dir(root_dir)
            .await
            .context("Failed to read storage root directory")?;

        while let Ok(Some(entry)) = entries.next_entry().await {
            if entry.path().is_dir()
                && let Some(name) = entry.file_name().to_str() {
                    // Skip dotfolders (.zen-garden, .Trash, etc.)
                    if name.starts_with('.') {
                        continue;
                    }
                    buckets.push(name.to_string());
                }
        }

        buckets.sort();
        Ok(buckets)
    }

    /// CREATE bucket - ensure a bucket directory exists at mount root.
    pub async fn create_bucket(&self, bucket: &str) -> Result<()> {
        let bucket_path = self.bucket_path(bucket);
        tokio::fs::create_dir_all(&bucket_path)
            .await
            .context("Failed to create bucket directory")?;
        debug!(bucket = %bucket, "Bucket created");
        Ok(())
    }

    /// Recursively walk directory and collect objects.
    ///
    /// Directory walking uses the filesystem directly (just listing entries).
    /// Content reads for metadata go through ContentStore.
    #[expect(clippy::too_many_arguments)]
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

                // Skip tmp files and the .zen-garden dotfolder
                if name.ends_with(".tmp") || name == ".zen-garden" || name.starts_with('.') {
                    continue;
                }

                // Get relative key from bucket root
                let relative = path
                    .strip_prefix(base_path)
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default();

                if path.is_dir() {
                    let dir_prefix = format!("{}/", relative);

                    if let Some(_delim) = delimiter {
                        // With delimiter: if this directory IS part of the prefix path,
                        // we must recurse into it. Otherwise, report as common prefix.
                        if prefix.starts_with(&dir_prefix) {
                            // prefix extends deeper into this dir — recurse to reach it
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
                        } else if dir_prefix.starts_with(prefix) {
                            // This dir is under the prefix — report as common prefix
                            common_prefixes.insert(dir_prefix);
                        }
                        // else: dir doesn't match prefix at all — skip
                    } else {
                        // No delimiter - recurse unconditionally
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

                    // Get metadata
                    match self.get_metadata(bucket, &relative).await {
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

    /// Get metadata for an object identified by bucket and key.
    async fn get_metadata(&self, bucket: &str, key: &str) -> Result<ObjectMetadata> {
        let rel = self.object_rel(bucket, key);
        let full_path = self.store.full_path(&rel);
        let file_meta = tokio::fs::metadata(&full_path)
            .await
            .context("Failed to get file metadata")?;

        // Try to read sidecar metadata from .zen-garden/meta/{bucket}/{key}.json
        let sidecar = self.sidecar_rel(bucket, key);
        let (content_type, etag, size, custom_metadata) = if self.store.exists(&sidecar).await {
            match self.store.read_string(&sidecar).await {
                Ok(json) => match serde_json::from_str::<ObjectMetadataSidecar>(&json) {
                    Ok(m) => (m.content_type, m.etag, m.size, m.custom_metadata),
                    Err(_) => {
                        let (ct, et, sz) = self.compute_metadata(&rel, &full_path).await?;
                        (ct, et, sz, Default::default())
                    }
                },
                Err(_) => {
                    let (ct, et, sz) = self.compute_metadata(&rel, &full_path).await?;
                    (ct, et, sz, Default::default())
                }
            }
        } else {
            let (ct, et, sz) = self.compute_metadata(&rel, &full_path).await?;
            (ct, et, sz, Default::default())
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
            custom_metadata,
        })
    }

    /// Compute metadata when sidecar is missing — reads through ContentStore (decrypts if needed)
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
    /// Custom metadata from x-amz-meta-* headers (STORAGE-0016 §2d)
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    custom_metadata: std::collections::HashMap<String, String>,
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ── STORAGE-0016 unified namespace path assertions ────────────────

    #[test]
    fn object_rel_is_at_mount_root() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ObjectStore::new(tmp.path());
        let rel = store.object_rel("my-bucket", "photos/jan/pic.jpg");
        // Object should be at {bucket}/{key} — no .zen-garden prefix
        assert_eq!(rel, PathBuf::from("my-bucket").join("photos/jan/pic.jpg"));
    }

    #[test]
    fn sidecar_rel_is_under_zen_garden_meta() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ObjectStore::new(tmp.path());
        let rel = store.sidecar_rel("my-bucket", "report.pdf");
        assert!(rel.starts_with(".zen-garden/meta"));
        assert!(rel.ends_with("my-bucket/report.pdf.json"));
    }

    #[test]
    fn bucket_path_is_at_mount_root() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ObjectStore::new(tmp.path());
        let bp = store.bucket_path("photos");
        assert_eq!(bp, tmp.path().join("photos"));
    }

    // ── CRUD integration tests (filesystem-backed) ────────────────────

    #[tokio::test]
    async fn put_get_delete_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ObjectStore::new(tmp.path());

        // PUT
        let result = store
            .put_object("test-bucket", "hello.txt", "text/plain", b"hello world")
            .await
            .unwrap();
        assert!(!result.etag.is_empty());

        // Object file at mount root
        assert!(tmp.path().join("test-bucket/hello.txt").exists());
        // Sidecar under .zen-garden/meta
        assert!(tmp
            .path()
            .join(".zen-garden/meta/test-bucket/hello.txt.json")
            .exists());

        // GET
        let (data, meta) = store
            .get_object("test-bucket", "hello.txt")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(data, b"hello world");
        assert_eq!(meta.content_type, "text/plain");
        assert_eq!(meta.size, 11);

        // HEAD
        let head = store
            .head_object("test-bucket", "hello.txt")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(head.key, "hello.txt");
        assert_eq!(head.size, 11);

        // DELETE
        store
            .delete_object("test-bucket", "hello.txt")
            .await
            .unwrap();
        assert!(store
            .get_object("test-bucket", "hello.txt")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn get_object_range_returns_slice() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ObjectStore::new(tmp.path());

        store
            .put_object("b", "data.bin", "application/octet-stream", b"0123456789")
            .await
            .unwrap();

        // Read bytes 2-5 (4 bytes)
        let (data, total_size, _meta) = store
            .get_object_range("b", "data.bin", 2, 4)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&data, b"2345");
        assert_eq!(total_size, 10);
    }

    #[tokio::test]
    async fn get_object_range_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ObjectStore::new(tmp.path());
        assert!(store
            .get_object_range("b", "missing.txt", 0, 10)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn create_bucket_creates_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ObjectStore::new(tmp.path());

        store.create_bucket("new-bucket").await.unwrap();
        assert!(tmp.path().join("new-bucket").is_dir());

        // Idempotent
        store.create_bucket("new-bucket").await.unwrap();
    }

    #[tokio::test]
    async fn list_buckets_excludes_dotfolders() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ObjectStore::new(tmp.path());

        // Create visible bucket + .zen-garden infrastructure dir
        store.create_bucket("photos").await.unwrap();
        tokio::fs::create_dir_all(tmp.path().join(".zen-garden/meta"))
            .await
            .unwrap();

        let buckets = store.list_buckets().await.unwrap();
        assert!(buckets.contains(&"photos".to_string()));
        assert!(!buckets.iter().any(|b| b.starts_with('.')));
    }

    #[tokio::test]
    async fn list_objects_with_prefix_and_delimiter() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ObjectStore::new(tmp.path());

        store
            .put_object("b", "photos/jan/a.jpg", "image/jpeg", b"img1")
            .await
            .unwrap();
        store
            .put_object("b", "photos/feb/b.jpg", "image/jpeg", b"img2")
            .await
            .unwrap();
        store
            .put_object("b", "docs/readme.txt", "text/plain", b"readme")
            .await
            .unwrap();

        // List with prefix "photos/" and delimiter "/"
        let result = store
            .list_objects("b", Some("photos/"), Some("/"), None, 1000)
            .await
            .unwrap();

        // Should get common prefixes for photos/jan/ and photos/feb/, no direct contents
        assert!(result.common_prefixes.contains(&"photos/jan/".to_string()));
        assert!(result.common_prefixes.contains(&"photos/feb/".to_string()));
        assert!(result.contents.is_empty());
    }

    #[tokio::test]
    async fn custom_metadata_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ObjectStore::new(tmp.path());

        let mut custom = std::collections::HashMap::new();
        custom.insert("author".to_string(), "alice".to_string());
        custom.insert("camera".to_string(), "Canon R5".to_string());

        store
            .put_object_with_metadata("b", "photo.jpg", "image/jpeg", b"data", custom)
            .await
            .unwrap();

        let meta = store.head_object("b", "photo.jpg").await.unwrap().unwrap();
        assert_eq!(meta.custom_metadata.get("author").unwrap(), "alice");
        assert_eq!(meta.custom_metadata.get("camera").unwrap(), "Canon R5");
    }
}
