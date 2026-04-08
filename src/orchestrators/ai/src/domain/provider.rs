//! The lean `Provider` trait (ORCH-0030 R2 M3).
//!
//! After M3, a `Provider` is a single extension point with three
//! methods:
//!
//! - [`Provider::name`] — stable, compile-time provider identity.
//! - [`Provider::onboard`] — take custody of a request and produce a
//!   [`ProviderOutcome`]. The provider owns instance selection,
//!   model resolution, protocol translation, and response
//!   construction.
//! - [`Provider::flush_caches`] — clear any artifacts cached on the
//!   provider's instances. Default no-op; providers that stage
//!   files override.
//!
//! Adapters publish their capability set to the bus via
//! [`crate::domain::capability_announcement::CapabilityAnnouncement`]
//! events under the topic `directory.provider.{name}.capabilities`.
//! The [`crate::services::directory_subscriber::CapabilityDirectory`]
//! consumes those events and is the single source of truth for
//! routing decisions. The dispatcher looks providers up by name in
//! the [`crate::services::provider_registry::ProviderRegistry`] and
//! calls `onboard` directly.
//!
//! # What was removed in M3
//!
//! - `state(&self)`, `subscribe(&self)`, `ProviderState`,
//!   `ProviderStatePublisher`, `ProviderHealth`
//! - `Registration`, `RegistrationStrategy`, `HonoredField`,
//!   `FieldRange`
//! - `MediaInputSpec`, `MediaOutputSpec` (replaced by
//!   `CapabilityMediaInput` in the announcement schema)
//! - `Model`, `ModelDescriptor` (model resolution is adapter-local)
//! - `PerformanceHint`, `PerformanceVerdict` (the recommendation
//!   engine is gone — adapters that need scoring own their own
//!   matrix, e.g. `OllamaCapabilityMatrix`)
//!
//! `FieldConstraint`, `ParamOption`, and `AutoKind` moved to
//! [`crate::services::skills::types`] — they are skill-schema
//! types, not provider-trait types, and the v3 disk schema depends
//! on them.

use async_trait::async_trait;
use futures_util::stream::BoxStream;
use serde::Serialize;

use crate::domain::errors::ErrorCode;
use crate::domain::ids::ProviderName;
use crate::domain::output::Output;
use crate::domain::request::OrchestratorRequest;

// ── The lean Provider trait ───────────────────────────────────

#[async_trait]
pub trait Provider: Send + Sync + 'static {
    /// Stable, compile-time provider identity.
    ///
    /// Adapters return a fixed [`ProviderName`] constructed from a
    /// const string in [`crate::domain::keys::providers`]. The name
    /// appears in URLs, logs, the bus topic
    /// `directory.provider.{name}.capabilities`, and as the lookup
    /// key in [`crate::services::provider_registry::ProviderRegistry`].
    fn name(&self) -> ProviderName;

    /// Take custody of a request. The provider owns instance
    /// selection, model resolution (reading
    /// `request.selectors.model`), protocol translation, and
    /// response construction.
    ///
    /// On success returns a [`ProviderOutcome`] describing how the
    /// result will be delivered (`Sync`, `Async`, or `Streaming`).
    /// On failure returns a [`ProviderError`] from the canonical
    /// taxonomy.
    async fn onboard(
        &self,
        request: OrchestratorRequest,
    ) -> Result<ProviderOutcome, ProviderError>;

    /// Clear any artifacts cached on the provider's instances.
    /// Default is a no-op; providers that stage files (ComfyUI,
    /// WhisperCpp uploads, etc.) override.
    async fn flush_caches(&self) -> Result<FlushReport, ProviderError> {
        Ok(FlushReport::empty())
    }
}

// ── Provider outcomes ─────────────────────────────────────────

/// The three ways a provider can deliver work.
pub enum ProviderOutcome {
    /// Provider produced a complete result inline.
    Sync(Output),
    /// Provider accepted the request and is processing
    /// asynchronously. The output carries `job.id`, `job.status`,
    /// and optionally `job.eta_seconds`.
    Async(Output),
    /// Provider is producing a stream of deltas. `initial` is the
    /// pre-stream announcement (carries any pre-allocated
    /// `media_id`, `job.id`, etc.).
    Streaming {
        initial: Output,
        stream: BoxStream<'static, Result<Output, ProviderError>>,
    },
}

impl std::fmt::Debug for ProviderOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderOutcome::Sync(o) => f.debug_tuple("Sync").field(o).finish(),
            ProviderOutcome::Async(o) => f.debug_tuple("Async").field(o).finish(),
            ProviderOutcome::Streaming { initial, .. } => f
                .debug_struct("Streaming")
                .field("initial", initial)
                .field("stream", &"<BoxStream>")
                .finish(),
        }
    }
}

// ── Provider errors ───────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("provider is unreachable: {0}")]
    Unreachable(String),
    #[error("provider is overloaded: {0}")]
    Overloaded(String),
    #[error("authentication failed: {0}")]
    AuthFailed(String),
    #[error("rate limited: {0}")]
    RateLimited(String),
    #[error("quota exhausted: {0}")]
    QuotaExhausted(String),
    #[error("timeout: {0}")]
    Timeout(String),
    #[error("upstream error: {0}")]
    Upstream(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("internal: {0}")]
    Internal(String),
    /// Caller pinned a model (via `selectors.model`) that no
    /// healthy instance of this provider is currently serving.
    /// Distinct from `Unsupported` because the model may be a valid
    /// choice later when the instance comes back — this is not a
    /// caller error, it is a transient availability error.
    #[error("pinned model `{model}` not servable: {reason}")]
    PinNotServable { model: String, reason: String },
}

impl ProviderError {
    /// Map to the canonical [`ErrorCode`] for response envelopes.
    pub fn code(&self) -> ErrorCode {
        match self {
            ProviderError::Unreachable(_) => ErrorCode::ProviderUnreachable,
            ProviderError::Overloaded(_) => ErrorCode::ProviderOverloaded,
            ProviderError::AuthFailed(_) => ErrorCode::AuthFailed,
            ProviderError::RateLimited(_) => ErrorCode::RateLimited,
            ProviderError::QuotaExhausted(_) => ErrorCode::QuotaExhausted,
            ProviderError::Timeout(_) => ErrorCode::Timeout,
            ProviderError::Upstream(_) => ErrorCode::UpstreamError,
            ProviderError::Unsupported(_) => ErrorCode::ValidationFailed,
            ProviderError::Internal(_) => ErrorCode::InternalError,
            ProviderError::PinNotServable { .. } => ErrorCode::NotFound,
        }
    }

    /// The human-readable message.
    pub fn message(&self) -> String {
        self.to_string()
    }
}

// ── Flush report ──────────────────────────────────────────────

/// Result of a [`Provider::flush_caches`] call.
#[derive(Debug, Clone, Default, Serialize)]
pub struct FlushReport {
    pub cleared_entries: u64,
    pub cleared_bytes: u64,
    pub notes: Vec<String>,
}

impl FlushReport {
    pub fn empty() -> Self {
        Self::default()
    }
}
