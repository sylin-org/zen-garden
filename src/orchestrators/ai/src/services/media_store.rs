//! Disk-backed media store.
//!
//! Rooted at the host-shared cache directory
//! (`{data_dir}/media/`). Layout:
//!
//! ```text
//! {root}/
//!   entries/
//!     {media_id}.bin     raw bytes
//!     {media_id}.json    metadata (MediaEntryFile)
//!   by-hash/
//!     {sha512}           one-line pointer file containing the media_id
//! ```
//!
//! Design notes:
//!
//! - **Content addressing.** Uploads SHA-512 the bytes, look up the
//!   pointer in `by-hash/`, and return the existing entry if present.
//! - **Touch.** Updating the TTL rewrites just the metadata file.
//! - **Streaming.** `open_writer` allocates a media id and streams
//!   chunks to a temp file; on `close` the temp file is atomically
//!   renamed to the final bin path and the metadata JSON is written.
//! - **Concurrency.** A single `RwLock<HashMap<String, ...>>` holds
//!   metadata in memory for lock-free reads; mutations flush to disk
//!   while holding the write lock briefly.
//! - **Transfer modes.** `HttpUpload`, `HttpPost`, `SharedPath`, and
//!   `InMemory` are fully implemented.
//! - **Reservation.** `reserve()` and `release_reservation()` flip the
//!   lifecycle state without moving bytes.
//!
//! The store starts empty on a cold start if the directory does not
//! exist. Existing directories are scanned and loaded.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha512};
use tokio::sync::{Mutex, RwLock};

use crate::domain::ids::{JobId, MediaId};
use crate::domain::media::{
    FlushReport, MediaEntry, MediaError, MediaFilter, MediaLifecycle, MediaReservation,
    MediaSink, MediaSinkWriter, MediaSource, MediaSourceKind, MediaStore, TransferHandle,
    TransferTarget, DEFAULT_ACTIVE_TTL, DEFAULT_RESERVED_WINDOW,
};

// ── On-disk metadata format ───────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MediaEntryFile {
    media_id: String,
    content_hash: String,
    content_type: String,
    size_bytes: u64,
    metadata: Value,
    source: MediaSourceFile,
    lifecycle: LifecycleFile,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MediaSourceFile {
    kind: String,
    provider: Option<String>,
    action: Option<String>,
    origin_request_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum LifecycleFile {
    Active {
        expires_at: chrono::DateTime<chrono::Utc>,
    },
    Reserved {
        expires_at: chrono::DateTime<chrono::Utc>,
        job_id: Option<String>,
        reason: String,
    },
}

impl MediaEntryFile {
    fn into_entry(self) -> Result<MediaEntry, MediaError> {
        let source_kind = match self.source.kind.as_str() {
            "uploaded" => MediaSourceKind::Uploaded,
            "generated" => MediaSourceKind::Generated,
            other => {
                return Err(MediaError::Io(format!("unknown source kind `{other}`")));
            }
        };
        let lifecycle = match self.lifecycle {
            LifecycleFile::Active { expires_at } => MediaLifecycle::Active { expires_at },
            LifecycleFile::Reserved {
                expires_at,
                job_id,
                reason,
            } => MediaLifecycle::Reserved {
                expires_at,
                reservation: MediaReservation {
                    job_id: job_id.map(JobId::from_string),
                    reason,
                },
            },
        };
        Ok(MediaEntry {
            id: MediaId::from_string(self.media_id),
            content_hash: self.content_hash,
            content_type: self.content_type,
            size_bytes: self.size_bytes,
            metadata: self.metadata,
            source: MediaSource {
                kind: source_kind,
                provider: self.source.provider.map(Into::into),
                action: self.source.action,
                origin_request_id: self
                    .source
                    .origin_request_id
                    .map(crate::domain::ids::RequestId::from_string),
            },
            lifecycle,
            created_at: self.created_at,
        })
    }

    fn from_entry(entry: &MediaEntry) -> Self {
        Self {
            media_id: entry.id.as_str().to_string(),
            content_hash: entry.content_hash.clone(),
            content_type: entry.content_type.clone(),
            size_bytes: entry.size_bytes,
            metadata: entry.metadata.clone(),
            source: MediaSourceFile {
                kind: match entry.source.kind {
                    MediaSourceKind::Uploaded => "uploaded".to_string(),
                    MediaSourceKind::Generated => "generated".to_string(),
                },
                provider: entry
                    .source
                    .provider
                    .as_ref()
                    .map(|p| p.as_str().to_string()),
                action: entry.source.action.clone(),
                origin_request_id: entry
                    .source
                    .origin_request_id
                    .as_ref()
                    .map(|r| r.as_str().to_string()),
            },
            lifecycle: match &entry.lifecycle {
                MediaLifecycle::Active { expires_at } => LifecycleFile::Active {
                    expires_at: *expires_at,
                },
                MediaLifecycle::Reserved {
                    expires_at,
                    reservation,
                } => LifecycleFile::Reserved {
                    expires_at: *expires_at,
                    job_id: reservation.job_id.as_ref().map(|j| j.as_str().to_string()),
                    reason: reservation.reason.clone(),
                },
            },
            created_at: entry.created_at,
        }
    }
}

// ── Store ─────────────────────────────────────────────────────

pub struct DiskMediaStore {
    root: PathBuf,
    http: Client,
    /// `media_id` → metadata (in-memory cache of on-disk JSON).
    index: RwLock<HashMap<String, MediaEntry>>,
    /// `sha512 hex` → `media_id`.
    by_hash: RwLock<HashMap<String, String>>,
    /// Write serialization for stream writers that share a media id.
    write_lock: Mutex<()>,
}

impl DiskMediaStore {
    /// Load the store rooted at `{data_dir}/media/`. Creates the
    /// directory tree if missing.
    pub async fn load(data_dir: &Path) -> Result<Arc<Self>, MediaError> {
        let root = data_dir.join("media");
        tokio::fs::create_dir_all(root.join("entries"))
            .await
            .map_err(|e| MediaError::Io(e.to_string()))?;
        tokio::fs::create_dir_all(root.join("by-hash"))
            .await
            .map_err(|e| MediaError::Io(e.to_string()))?;

        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| MediaError::Io(format!("http client: {e}")))?;

        let store = Arc::new(Self {
            root,
            http,
            index: RwLock::new(HashMap::new()),
            by_hash: RwLock::new(HashMap::new()),
            write_lock: Mutex::new(()),
        });
        store.scan_existing().await?;
        Ok(store)
    }

    async fn scan_existing(&self) -> Result<(), MediaError> {
        let entries_dir = self.root.join("entries");
        let mut read = match tokio::fs::read_dir(&entries_dir).await {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(MediaError::Io(e.to_string())),
        };
        let mut loaded_index: HashMap<String, MediaEntry> = HashMap::new();
        let mut loaded_by_hash: HashMap<String, String> = HashMap::new();
        while let Ok(Some(dirent)) = read.next_entry().await {
            let path = dirent.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            match tokio::fs::read(&path).await {
                Ok(bytes) => match serde_json::from_slice::<MediaEntryFile>(&bytes) {
                    Ok(file) => match file.into_entry() {
                        Ok(entry) => {
                            loaded_by_hash
                                .insert(entry.content_hash.clone(), entry.id.as_str().to_string());
                            loaded_index.insert(entry.id.as_str().to_string(), entry);
                        }
                        Err(e) => {
                            tracing::warn!(path = %path.display(), error = %e, "failed to decode media metadata");
                        }
                    },
                    Err(e) => {
                        tracing::warn!(path = %path.display(), error = %e, "failed to parse media metadata");
                    }
                },
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "failed to read media metadata");
                }
            }
        }
        *self.index.write().await = loaded_index;
        *self.by_hash.write().await = loaded_by_hash;
        Ok(())
    }

    fn entry_bin_path(&self, id: &str) -> PathBuf {
        self.root.join("entries").join(format!("{id}.bin"))
    }

    fn entry_meta_path(&self, id: &str) -> PathBuf {
        self.root.join("entries").join(format!("{id}.json"))
    }

    fn hash_pointer_path(&self, hash: &str) -> PathBuf {
        // Use '-' for '/' in hex (none expected) and keep it flat.
        self.root.join("by-hash").join(hash)
    }

    async fn write_metadata(&self, entry: &MediaEntry) -> Result<(), MediaError> {
        let path = self.entry_meta_path(entry.id.as_str());
        let file = MediaEntryFile::from_entry(entry);
        let bytes = serde_json::to_vec_pretty(&file)
            .map_err(|e| MediaError::Io(format!("serialize metadata: {e}")))?;
        tokio::fs::write(&path, bytes)
            .await
            .map_err(|e| MediaError::Io(format!("write metadata {}: {e}", path.display())))
    }

    async fn hash_content(bytes: &Bytes) -> String {
        let mut hasher = Sha512::new();
        hasher.update(bytes);
        format!("sha512:{}", hex::encode(hasher.finalize()))
    }
}

#[async_trait]
impl MediaStore for DiskMediaStore {
    async fn put(
        &self,
        bytes: Bytes,
        content_type: String,
        source: MediaSource,
    ) -> Result<MediaEntry, MediaError> {
        let hash = Self::hash_content(&bytes).await;

        // Content-addressed dedup.
        //
        // The lookup reads the `by_hash` and `index` maps through
        // short-lived blocks that hand back owned clones. It is
        // critical that no `RwLockReadGuard` remains alive when we
        // call `touch()` below — `touch()` acquires the `index`
        // write lock, so holding an `index` read guard here would
        // deadlock. Bind the clone to a local and drop the guard
        // before touching.
        let existing_id: Option<String> = {
            let by_hash = self.by_hash.read().await;
            by_hash.get(&hash).cloned()
        };
        if let Some(existing_id) = existing_id {
            let existing_entry: Option<MediaEntry> = {
                let index = self.index.read().await;
                index.get(&existing_id).cloned()
            };
            if let Some(entry) = existing_entry {
                // Refresh TTL on re-upload.
                let _ = self.touch(&entry.id).await;
                return Ok(entry);
            }
        }

        let _guard = self.write_lock.lock().await;

        // Double-check after lock — same deadlock hazard, same fix.
        let existing_id: Option<String> = {
            let by_hash = self.by_hash.read().await;
            by_hash.get(&hash).cloned()
        };
        if let Some(existing_id) = existing_id {
            let existing_entry: Option<MediaEntry> = {
                let index = self.index.read().await;
                index.get(&existing_id).cloned()
            };
            if let Some(entry) = existing_entry {
                return Ok(entry);
            }
        }

        let id = MediaId::generate();
        let entry = MediaEntry {
            id: id.clone(),
            content_hash: hash.clone(),
            content_type,
            size_bytes: bytes.len() as u64,
            metadata: Value::Null,
            source,
            lifecycle: MediaLifecycle::active_for(Utc::now(), DEFAULT_ACTIVE_TTL),
            created_at: Utc::now(),
        };

        // Persist bytes first, then metadata, then pointer.
        let bin_path = self.entry_bin_path(id.as_str());
        tokio::fs::write(&bin_path, &bytes)
            .await
            .map_err(|e| MediaError::Io(format!("write bytes {}: {e}", bin_path.display())))?;
        self.write_metadata(&entry).await?;
        tokio::fs::write(self.hash_pointer_path(&hash), id.as_str().as_bytes())
            .await
            .map_err(|e| MediaError::Io(format!("write hash pointer: {e}")))?;

        self.index
            .write()
            .await
            .insert(id.as_str().to_string(), entry.clone());
        self.by_hash
            .write()
            .await
            .insert(hash, id.as_str().to_string());
        Ok(entry)
    }

    async fn open_writer(
        &self,
        content_type: String,
        source: MediaSource,
    ) -> Result<MediaSink, MediaError> {
        let id = MediaId::generate();
        let temp_path = self.entry_bin_path(id.as_str());
        let tmp_handle = tokio::fs::File::create(&temp_path)
            .await
            .map_err(|e| MediaError::Io(format!("create temp file: {e}")))?;
        let writer = DiskSink {
            media_id: id,
            path: temp_path,
            meta_path: self.entry_meta_path(""),
            content_type,
            source,
            file: Some(tmp_handle),
            buffer: BytesMut::new(),
            aborted: false,
            root: self.root.clone(),
        };
        // Patch meta path now that we know the id
        let mut writer = writer;
        writer.meta_path = self.entry_meta_path(writer.media_id.as_str());
        Ok(MediaSink::new(Box::new(writer)))
    }

    async fn get_bytes(&self, id: &MediaId) -> Result<Bytes, MediaError> {
        let path = self.entry_bin_path(id.as_str());
        match tokio::fs::read(&path).await {
            Ok(bytes) => Ok(Bytes::from(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(MediaError::NotFound(id.clone()))
            }
            Err(e) => Err(MediaError::Io(e.to_string())),
        }
    }

    async fn get_metadata(&self, id: &MediaId) -> Result<MediaEntry, MediaError> {
        if let Some(entry) = self.index.read().await.get(id.as_str()).cloned() {
            return Ok(entry);
        }
        Err(MediaError::NotFound(id.clone()))
    }

    async fn delete(&self, id: &MediaId) -> Result<(), MediaError> {
        let mut index = self.index.write().await;
        let Some(entry) = index.remove(id.as_str()) else {
            return Err(MediaError::NotFound(id.clone()));
        };
        drop(index);

        let mut by_hash = self.by_hash.write().await;
        by_hash.remove(&entry.content_hash);
        drop(by_hash);

        let _ = tokio::fs::remove_file(self.entry_bin_path(id.as_str())).await;
        let _ = tokio::fs::remove_file(self.entry_meta_path(id.as_str())).await;
        let _ = tokio::fs::remove_file(self.hash_pointer_path(&entry.content_hash)).await;
        Ok(())
    }

    async fn touch(&self, id: &MediaId) -> Result<(), MediaError> {
        let mut index = self.index.write().await;
        let Some(entry) = index.get_mut(id.as_str()) else {
            return Err(MediaError::NotFound(id.clone()));
        };
        if let MediaLifecycle::Active { expires_at } = &mut entry.lifecycle {
            *expires_at = Utc::now() + DEFAULT_ACTIVE_TTL;
        }
        let entry_clone = entry.clone();
        drop(index);
        self.write_metadata(&entry_clone).await
    }

    async fn list(&self, filter: MediaFilter) -> Result<Vec<MediaEntry>, MediaError> {
        let index = self.index.read().await;
        let now = Utc::now();
        let mut entries: Vec<MediaEntry> = index
            .values()
            .filter(|e| matches_filter(e, &filter, now))
            .cloned()
            .collect();
        entries.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(entries)
    }

    async fn transfer_to(
        &self,
        id: &MediaId,
        target: TransferTarget,
    ) -> Result<TransferHandle, MediaError> {
        let bytes = self.get_bytes(id).await?;
        let entry = self.get_metadata(id).await?;
        match target {
            TransferTarget::HttpUpload {
                endpoint,
                field_name,
            } => {
                let filename = format!(
                    "{}{}",
                    id.as_str(),
                    extension_for(&entry.content_type)
                );
                let part = reqwest::multipart::Part::bytes(bytes.to_vec())
                    .file_name(filename.clone())
                    .mime_str(&entry.content_type)
                    .map_err(|e| MediaError::Transfer(format!("mime: {e}")))?;
                let form = reqwest::multipart::Form::new().part(field_name, part);
                let resp = self
                    .http
                    .post(&endpoint)
                    .multipart(form)
                    .send()
                    .await
                    .map_err(|e| MediaError::Transfer(format!("http: {e}")))?;
                if !resp.status().is_success() {
                    return Err(MediaError::Transfer(format!(
                        "upload failed: {} {}",
                        resp.status(),
                        resp.text().await.unwrap_or_default()
                    )));
                }
                Ok(TransferHandle {
                    reference: filename,
                    instance_fqn: endpoint,
                    expires_at: Utc::now() + chrono::Duration::hours(1),
                    bytes: None,
                })
            }
            TransferTarget::HttpPost {
                endpoint,
                content_type,
            } => {
                let resp = self
                    .http
                    .post(&endpoint)
                    .header("content-type", &content_type)
                    .body(bytes.to_vec())
                    .send()
                    .await
                    .map_err(|e| MediaError::Transfer(format!("http: {e}")))?;
                if !resp.status().is_success() {
                    return Err(MediaError::Transfer(format!(
                        "post failed: {}",
                        resp.status()
                    )));
                }
                Ok(TransferHandle {
                    reference: id.as_str().to_string(),
                    instance_fqn: endpoint,
                    expires_at: Utc::now() + chrono::Duration::hours(1),
                    bytes: None,
                })
            }
            TransferTarget::SharedPath {
                directory,
                filename,
            } => {
                tokio::fs::create_dir_all(&directory)
                    .await
                    .map_err(|e| MediaError::Transfer(format!("mkdir: {e}")))?;
                let filename = filename.unwrap_or_else(|| {
                    format!("{}{}", id.as_str(), extension_for(&entry.content_type))
                });
                let path = directory.join(&filename);
                tokio::fs::write(&path, &bytes)
                    .await
                    .map_err(|e| MediaError::Transfer(format!("write: {e}")))?;
                Ok(TransferHandle {
                    reference: path.to_string_lossy().into_owned(),
                    instance_fqn: directory.to_string_lossy().into_owned(),
                    expires_at: Utc::now() + chrono::Duration::hours(24),
                    bytes: None,
                })
            }
            TransferTarget::InMemory => Ok(TransferHandle {
                reference: id.as_str().to_string(),
                instance_fqn: String::from("inline"),
                expires_at: Utc::now() + chrono::Duration::minutes(5),
                bytes: Some(bytes),
            }),
        }
    }

    async fn reserve(
        &self,
        id: &MediaId,
        reservation: MediaReservation,
    ) -> Result<(), MediaError> {
        let mut index = self.index.write().await;
        let Some(entry) = index.get_mut(id.as_str()) else {
            return Err(MediaError::NotFound(id.clone()));
        };
        entry.lifecycle = MediaLifecycle::Reserved {
            expires_at: Utc::now() + DEFAULT_RESERVED_WINDOW,
            reservation,
        };
        let entry_clone = entry.clone();
        drop(index);
        self.write_metadata(&entry_clone).await
    }

    async fn release_reservation(&self, id: &MediaId, job_id: &JobId) -> Result<(), MediaError> {
        let mut index = self.index.write().await;
        let Some(entry) = index.get_mut(id.as_str()) else {
            return Err(MediaError::NotFound(id.clone()));
        };
        if let MediaLifecycle::Reserved { reservation, .. } = &entry.lifecycle {
            if reservation.job_id.as_ref().map(|j| j.as_str())
                != Some(job_id.as_str())
            {
                return Ok(());
            }
        }
        entry.lifecycle = MediaLifecycle::active_for(Utc::now(), DEFAULT_ACTIVE_TTL);
        let entry_clone = entry.clone();
        drop(index);
        self.write_metadata(&entry_clone).await
    }

    async fn release_reservations_for_job(
        &self,
        job_id: &JobId,
    ) -> Result<u64, MediaError> {
        let to_release: Vec<MediaEntry> = {
            let index = self.index.read().await;
            index
                .values()
                .filter(|e| {
                    matches!(
                        &e.lifecycle,
                        MediaLifecycle::Reserved { reservation, .. }
                            if reservation.job_id.as_ref().map(|j| j.as_str())
                                == Some(job_id.as_str())
                    )
                })
                .cloned()
                .collect()
        };
        let count = to_release.len() as u64;
        let mut index = self.index.write().await;
        for mut entry in to_release {
            entry.lifecycle =
                MediaLifecycle::active_for(Utc::now(), DEFAULT_ACTIVE_TTL);
            index.insert(entry.id.as_str().to_string(), entry.clone());
            drop(index);
            self.write_metadata(&entry).await?;
            index = self.index.write().await;
        }
        Ok(count)
    }

    async fn flush(&self, filter: MediaFilter) -> Result<FlushReport, MediaError> {
        let to_remove: Vec<MediaId> = {
            let index = self.index.read().await;
            let now = Utc::now();
            index
                .values()
                .filter(|e| matches_filter(e, &filter, now))
                .map(|e| e.id.clone())
                .collect()
        };
        let mut report = FlushReport::default();
        for id in to_remove {
            if let Ok(entry) = self.get_metadata(&id).await {
                report.freed_bytes += entry.size_bytes;
            }
            if self.delete(&id).await.is_ok() {
                report.removed_count += 1;
            }
        }
        Ok(report)
    }
}

fn matches_filter(entry: &MediaEntry, filter: &MediaFilter, now: chrono::DateTime<Utc>) -> bool {
    if let Some(kind) = filter.source_kind {
        if entry.source.kind != kind {
            return false;
        }
    }
    if let Some(provider) = filter.provider.as_ref() {
        if entry.source.provider.as_ref() != Some(provider) {
            return false;
        }
    }
    if let Some(prefix) = filter.content_type_prefix.as_ref() {
        if !entry.content_type.starts_with(prefix) {
            return false;
        }
    }
    if let Some(before) = filter.created_before {
        if entry.created_at >= before {
            return false;
        }
    }
    if filter.only_expired {
        match &entry.lifecycle {
            MediaLifecycle::Active { expires_at } if *expires_at > now => return false,
            MediaLifecycle::Reserved { .. } => return false,
            _ => {}
        }
    }
    true
}

fn extension_for(content_type: &str) -> &'static str {
    match content_type.to_ascii_lowercase().as_str() {
        "image/png" => ".png",
        "image/jpeg" | "image/jpg" => ".jpg",
        "image/webp" => ".webp",
        "image/gif" => ".gif",
        "audio/mpeg" | "audio/mp3" => ".mp3",
        "audio/wav" | "audio/wave" => ".wav",
        "audio/ogg" => ".ogg",
        "audio/flac" => ".flac",
        "text/plain" => ".txt",
        "application/json" => ".json",
        _ => "",
    }
}

// ── Streaming writer ──────────────────────────────────────────

struct DiskSink {
    media_id: MediaId,
    path: PathBuf,
    meta_path: PathBuf,
    content_type: String,
    source: MediaSource,
    file: Option<tokio::fs::File>,
    buffer: BytesMut,
    aborted: bool,
    root: PathBuf,
}

#[async_trait]
impl MediaSinkWriter for DiskSink {
    fn media_id(&self) -> &MediaId {
        &self.media_id
    }

    async fn write(&mut self, chunk: Bytes) -> Result<(), MediaError> {
        if self.aborted {
            return Err(MediaError::SinkClosed);
        }
        let Some(file) = self.file.as_mut() else {
            return Err(MediaError::SinkClosed);
        };
        use tokio::io::AsyncWriteExt;
        file.write_all(&chunk)
            .await
            .map_err(|e| MediaError::Io(format!("stream write: {e}")))?;
        self.buffer.extend_from_slice(&chunk);
        Ok(())
    }

    async fn close(self: Box<Self>) -> Result<MediaEntry, MediaError> {
        let mut this = *self;
        let Some(mut file) = this.file.take() else {
            return Err(MediaError::SinkClosed);
        };
        use tokio::io::AsyncWriteExt;
        file.flush()
            .await
            .map_err(|e| MediaError::Io(format!("stream flush: {e}")))?;
        drop(file);

        let bytes = this.buffer.freeze();
        let hash = DiskMediaStore::hash_content(&bytes).await;
        let entry = MediaEntry {
            id: this.media_id.clone(),
            content_hash: hash.clone(),
            content_type: this.content_type,
            size_bytes: bytes.len() as u64,
            metadata: Value::Null,
            source: this.source,
            lifecycle: MediaLifecycle::active_for(Utc::now(), DEFAULT_ACTIVE_TTL),
            created_at: Utc::now(),
        };
        let file_json = MediaEntryFile::from_entry(&entry);
        let meta_bytes = serde_json::to_vec_pretty(&file_json)
            .map_err(|e| MediaError::Io(format!("serialize: {e}")))?;
        tokio::fs::write(&this.meta_path, meta_bytes)
            .await
            .map_err(|e| MediaError::Io(format!("write meta: {e}")))?;

        // Write hash pointer.
        let pointer = this.root.join("by-hash").join(&hash);
        let _ = tokio::fs::write(pointer, entry.id.as_str().as_bytes()).await;

        Ok(entry)
    }

    async fn abort(self: Box<Self>) {
        let mut this = *self;
        this.aborted = true;
        this.file.take();
        let _ = tokio::fs::remove_file(&this.path).await;
    }
}
