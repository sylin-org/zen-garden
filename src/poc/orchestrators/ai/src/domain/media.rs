//! Media model — content-addressed store, delivery modes, transfer targets,
//! and lifecycle.
//!
//! The orchestrator's media model is a first-class, orthogonal concern.
//! Callers upload bytes once; every provider that consumes those bytes
//! sees them in the shape it declared at registration time.
//!
//! Key types:
//!
//! - [`MediaStore`] — trait implemented by in-memory (v1) and future
//!   on-disk stores. Content-addressed: the same SHA-512 returns the
//!   same `MediaId`.
//! - [`MediaEntry`] — a stored media item. Carries bytes, metadata,
//!   source, and lifecycle.
//! - [`MediaSink`] — a streaming writer handed to providers producing
//!   media progressively.
//! - [`MediaDelivery`] — three delivery modes a provider may declare:
//!   `ById`, `Base64`, `Transfer`.
//! - [`TransferTarget`] — where the media store should stage bytes when
//!   a provider requests a transfer.
//! - [`MediaLifecycle`] — Active (touch-refresh TTL) vs Reserved
//!   (30-day job-bound window).

use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

use crate::domain::ids::{JobId, MediaId, ProviderName, RequestId};

// ── MediaStore trait ──────────────────────────────────────────

/// The boundary the orchestrator uses to read and write media.
#[async_trait]
pub trait MediaStore: Send + Sync + 'static {
    /// Store bytes atomically. Content-addressed via SHA-512, so the
    /// same bytes uploaded twice return the same `MediaId`.
    async fn put(
        &self,
        bytes: Bytes,
        content_type: String,
        source: MediaSource,
    ) -> Result<MediaEntry, MediaError>;

    /// Open a streaming writer. The `MediaId` is allocated at open
    /// time so it can be announced in a streaming response's `initial`
    /// output before any bytes arrive.
    async fn open_writer(
        &self,
        content_type: String,
        source: MediaSource,
    ) -> Result<MediaSink, MediaError>;

    async fn get_bytes(&self, id: &MediaId) -> Result<Bytes, MediaError>;
    async fn get_metadata(&self, id: &MediaId) -> Result<MediaEntry, MediaError>;
    async fn delete(&self, id: &MediaId) -> Result<(), MediaError>;

    /// Refresh the TTL for this media. Cheap and idempotent.
    async fn touch(&self, id: &MediaId) -> Result<(), MediaError>;

    async fn list(&self, filter: MediaFilter) -> Result<Vec<MediaEntry>, MediaError>;

    /// Transfer bytes to a provider-specified target.
    async fn transfer_to(
        &self,
        id: &MediaId,
        target: TransferTarget,
    ) -> Result<TransferHandle, MediaError>;

    /// Bind media to a job so the GC cannot reclaim it before the job
    /// reaches a terminal state.
    async fn reserve(
        &self,
        id: &MediaId,
        reservation: MediaReservation,
    ) -> Result<(), MediaError>;

    async fn release_reservation(&self, id: &MediaId, job_id: &JobId) -> Result<(), MediaError>;

    /// Release every reservation bound to `job_id`. Called when a job
    /// reaches a terminal state so the Reserved lifetimes don't linger
    /// past their useful window.
    async fn release_reservations_for_job(&self, job_id: &JobId) -> Result<u64, MediaError>;

    /// Bulk delete by filter (operator scope).
    async fn flush(&self, filter: MediaFilter) -> Result<FlushReport, MediaError>;
}

// ── Streaming writer ──────────────────────────────────────────

/// Handle returned by [`MediaStore::open_writer`]. Providers write
/// chunks through this sink and close it when done.
///
/// Implementations vary per backend; the in-memory store uses an
/// async channel that accumulates into a `BytesMut`. An on-disk store
/// would append chunks to a temp file and atomically rename on close.
#[async_trait]
pub trait MediaSinkWriter: Send + Sync + 'static {
    fn media_id(&self) -> &MediaId;
    async fn write(&mut self, chunk: Bytes) -> Result<(), MediaError>;
    async fn close(self: Box<Self>) -> Result<MediaEntry, MediaError>;
    async fn abort(self: Box<Self>);
}

/// Boxed sink handle.
pub struct MediaSink {
    inner: Box<dyn MediaSinkWriter>,
}

impl MediaSink {
    pub fn new(inner: Box<dyn MediaSinkWriter>) -> Self {
        Self { inner }
    }

    pub fn media_id(&self) -> &MediaId {
        self.inner.media_id()
    }

    pub async fn write(&mut self, chunk: Bytes) -> Result<(), MediaError> {
        self.inner.write(chunk).await
    }

    pub async fn close(self) -> Result<MediaEntry, MediaError> {
        self.inner.close().await
    }

    pub async fn abort(self) {
        self.inner.abort().await
    }
}

// ── MediaEntry ────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MediaEntry {
    pub id: MediaId,
    pub content_hash: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub metadata: Value,
    pub source: MediaSource,
    pub lifecycle: MediaLifecycle,
    pub created_at: DateTime<Utc>,
}

/// Serializable projection of a [`MediaEntry`] for HTTP responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaEntryView {
    pub media_id: String,
    pub content_hash: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub metadata: Value,
    pub source: MediaSourceView,
    pub lifecycle: LifecycleView,
    pub created_at: DateTime<Utc>,
}

impl From<&MediaEntry> for MediaEntryView {
    fn from(entry: &MediaEntry) -> Self {
        Self {
            media_id: entry.id.as_str().to_string(),
            content_hash: entry.content_hash.clone(),
            content_type: entry.content_type.clone(),
            size_bytes: entry.size_bytes,
            metadata: entry.metadata.clone(),
            source: MediaSourceView::from(&entry.source),
            lifecycle: LifecycleView::from(&entry.lifecycle),
            created_at: entry.created_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaSourceView {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_request_id: Option<String>,
}

impl From<&MediaSource> for MediaSourceView {
    fn from(src: &MediaSource) -> Self {
        Self {
            kind: match src.kind {
                MediaSourceKind::Uploaded => "uploaded".to_string(),
                MediaSourceKind::Generated => "generated".to_string(),
            },
            provider: src.provider.as_ref().map(|p| p.as_str().to_string()),
            action: src.action.clone(),
            origin_request_id: src.origin_request_id.as_ref().map(|r| r.as_str().to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LifecycleView {
    Active {
        expires_at: DateTime<Utc>,
    },
    Reserved {
        expires_at: DateTime<Utc>,
        job_id: Option<String>,
        reason: String,
    },
}

impl From<&MediaLifecycle> for LifecycleView {
    fn from(lc: &MediaLifecycle) -> Self {
        match lc {
            MediaLifecycle::Active { expires_at } => LifecycleView::Active {
                expires_at: *expires_at,
            },
            MediaLifecycle::Reserved {
                expires_at,
                reservation,
            } => LifecycleView::Reserved {
                expires_at: *expires_at,
                job_id: reservation.job_id.as_ref().map(|j| j.as_str().to_string()),
                reason: reservation.reason.clone(),
            },
        }
    }
}

// ── Lifecycle ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum MediaLifecycle {
    /// Default state. TTL clock runs; `touch` refreshes.
    Active { expires_at: DateTime<Utc> },
    /// Bound to an in-flight job. Protected from GC until the
    /// reservation releases or the 30-day window elapses.
    Reserved {
        expires_at: DateTime<Utc>,
        reservation: MediaReservation,
    },
}

impl MediaLifecycle {
    pub fn active_for(now: DateTime<Utc>, ttl: Duration) -> Self {
        Self::Active {
            expires_at: now + ttl,
        }
    }

    pub fn expires_at(&self) -> DateTime<Utc> {
        match self {
            Self::Active { expires_at } => *expires_at,
            Self::Reserved { expires_at, .. } => *expires_at,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MediaReservation {
    pub job_id: Option<JobId>,
    pub reason: String,
}

// ── Source tracking ───────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MediaSource {
    pub kind: MediaSourceKind,
    pub provider: Option<ProviderName>,
    pub action: Option<String>,
    pub origin_request_id: Option<RequestId>,
}

impl MediaSource {
    pub fn uploaded() -> Self {
        Self {
            kind: MediaSourceKind::Uploaded,
            provider: None,
            action: None,
            origin_request_id: None,
        }
    }

    pub fn generated(
        provider: ProviderName,
        action: impl Into<String>,
        request_id: RequestId,
    ) -> Self {
        Self {
            kind: MediaSourceKind::Generated,
            provider: Some(provider),
            action: Some(action.into()),
            origin_request_id: Some(request_id),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaSourceKind {
    Uploaded,
    Generated,
}

// ── Delivery modes ────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaDelivery {
    /// Provider reads the `media_id` from the payload and uses the
    /// media store handle in [`crate::domain::request::ExecutionContext`]
    /// to fetch bytes or initiate a transfer itself. Payload is
    /// untouched by the media resolver.
    ById,
    /// Media resolver fetches bytes, base64-encodes, and substitutes
    /// `{media_id: "..."}` for `{base64, content_type, size_bytes}`
    /// before dispatch.
    Base64,
    /// Provider handles staging inside its `onboard` method. The
    /// resolver validates the media reference (content type,
    /// existence, metadata) but does not move bytes.
    Transfer,
}

// ── Transfer targets ──────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum TransferTarget {
    /// Multipart HTTP upload with a single file field.
    HttpUpload {
        endpoint: String,
        field_name: String,
    },
    /// Raw HTTP POST with bytes as the body.
    HttpPost {
        endpoint: String,
        content_type: String,
    },
    /// Copy to a shared filesystem path.
    SharedPath {
        directory: PathBuf,
        filename: Option<String>,
    },
    /// Return bytes as an in-memory buffer.
    InMemory,
}

#[derive(Debug, Clone)]
pub struct TransferHandle {
    /// Opaque reference (filename, upload id, …) understood by the
    /// requesting provider.
    pub reference: String,
    /// Identifier of the instance the transfer was bound to. The
    /// provider must route the subsequent execution to the same
    /// instance.
    pub instance_fqn: String,
    /// When this handle stops being valid on the target instance.
    pub expires_at: DateTime<Utc>,
    /// In-memory byte payload (for `TransferTarget::InMemory`).
    pub bytes: Option<Bytes>,
}

// ── Filters and reports ───────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct MediaFilter {
    pub source_kind: Option<MediaSourceKind>,
    pub provider: Option<ProviderName>,
    pub content_type_prefix: Option<String>,
    pub created_before: Option<DateTime<Utc>>,
    pub only_expired: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct FlushReport {
    pub removed_count: u64,
    pub freed_bytes: u64,
}

// ── Errors ────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("media not found: {0}")]
    NotFound(MediaId),
    #[error("media content-type mismatch: expected one of {expected:?}, got {actual}")]
    ContentTypeMismatch {
        expected: Vec<String>,
        actual: String,
    },
    #[error("media sink is closed")]
    SinkClosed,
    #[error("transfer failed: {0}")]
    Transfer(String),
    #[error("i/o: {0}")]
    Io(String),
    #[error("invalid input: {0}")]
    Invalid(String),
}

// ── Resolution state in the request pipeline ─────────────────

/// Outcome of resolving a single media reference during the media
/// resolver stage. Attached to the
/// [`crate::domain::request::MediaContext`] under the reference's
/// `MediaId`.
#[derive(Debug, Clone)]
pub enum ResolvedMedia {
    /// Payload unchanged; provider will pull bytes on its own.
    ById,
    /// Payload rewritten in place; no further action for the provider.
    Base64Embedded,
    /// Resolver is deferring staging to the provider's `onboard`.
    DeferredToProvider,
}

// ── Module-level helper ───────────────────────────────────────

/// Default active-state TTL (§ADR: 24 hours from last touch).
pub const DEFAULT_ACTIVE_TTL: chrono::Duration = chrono::Duration::hours(24);

/// Default reserved-state window (§ADR: 30 days).
pub const DEFAULT_RESERVED_WINDOW: chrono::Duration = chrono::Duration::days(30);

// Bundle `Arc<dyn MediaStore>` for convenient cloning in the pipeline.
pub type SharedMediaStore = Arc<dyn MediaStore>;
