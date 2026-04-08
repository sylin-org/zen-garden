//! The `Provider` trait and its state bundle.
//!
//! A `Provider` is the orchestrator's single extension point. Five
//! methods, one state bundle, published via a `watch::channel` that the
//! [`crate::domain::directory::Directory`] subscribes to at registration
//! time.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures_util::stream::BoxStream;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::domain::errors::ErrorCode;
use crate::domain::field_path::FieldPath;
use crate::domain::ids::{ModelFqn, ProviderName, RegistrationId};
use crate::domain::moniker::Moniker;
use crate::domain::output::Output;
use crate::domain::primitive::Primitive;
use crate::domain::request::OrchestratorRequest;

// ── Provider trait ────────────────────────────────────────────

/// The extension point for every AI vendor adapter.
///
/// Implementations own:
///
/// - Their own instance selection (load balancing, health rotation).
/// - Their own wire-format translation (canonical → vendor JSON and back).
/// - Their own busy semantics (sync, queue, async, refuse).
/// - Their own media staging (for providers using
///   [`crate::domain::media::MediaDelivery::Transfer`]).
///
/// The orchestrator does not peer inside. It calls [`Provider::onboard`]
/// and reads the published [`ProviderState`] snapshot for every policy
/// decision.
#[async_trait]
pub trait Provider: Send + Sync + 'static {
    /// Stable human-readable identity.
    fn name(&self) -> ProviderName;

    /// Current snapshot of the provider's live state. Cheap to call;
    /// equivalent to `self.subscribe().borrow().clone()`.
    fn state(&self) -> Arc<ProviderState>;

    /// Subscribe to state changes. The returned receiver yields the
    /// current value immediately and every subsequent update.
    fn subscribe(&self) -> watch::Receiver<Arc<ProviderState>>;

    /// Take custody of a request. The provider owns instance selection,
    /// protocol translation, and response construction. Returns a
    /// [`ProviderOutcome`] describing how the result will be delivered.
    async fn onboard(
        &self,
        request: OrchestratorRequest,
    ) -> Result<ProviderOutcome, ProviderError>;

    /// Clear any artifacts cached on the provider's instances. Default
    /// is a no-op; providers that stage files override.
    async fn flush_caches(&self) -> Result<FlushReport, ProviderError> {
        Ok(FlushReport::empty())
    }
}

// ── ProviderState ─────────────────────────────────────────────

/// The bundled live state a provider publishes through its
/// `watch::channel`. Every field may change independently; changes are
/// published as a fresh `Arc<ProviderState>`.
#[derive(Debug, Clone, Default)]
pub struct ProviderState {
    pub health: ProviderHealth,
    pub registrations: Vec<Registration>,
    pub models: Vec<Model>,
    pub performance_hints: Vec<PerformanceHint>,
}

impl ProviderState {
    pub fn is_healthy(&self) -> bool {
        matches!(self.health, ProviderHealth::Healthy)
    }

    pub fn is_online(&self) -> bool {
        !matches!(self.health, ProviderHealth::Offline { .. })
    }

    pub fn find_registration(
        &self,
        primitive: Primitive,
        skill: Option<&Moniker>,
    ) -> Option<&Registration> {
        self.registrations.iter().find(|r| {
            if r.primitive != primitive {
                return false;
            }
            match (&r.strategy, skill) {
                (RegistrationStrategy::Skill { moniker, .. }, Some(m)) => moniker == m,
                (RegistrationStrategy::Skill { .. }, None) => false,
                (_, None) => true,
                (_, Some(_)) => false,
            }
        })
    }
}

/// Provider health state.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ProviderHealth {
    /// Provider is accepting requests normally.
    #[default]
    Healthy,
    /// Can accept requests but with impairments.
    Degraded { reason: String },
    /// Cannot accept requests. Removed from routing candidates.
    Offline { reason: String },
}

// ── Registration ──────────────────────────────────────────────

/// A fact emitted by a provider: *"I serve primitive X via strategy S
/// under these constraints."*
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registration {
    pub id: RegistrationId,
    pub provider: ProviderName,
    pub primitive: Primitive,
    pub strategy: RegistrationStrategy,
    pub honored_fields: Vec<HonoredField>,
    pub media_inputs: Vec<MediaInputSpec>,
    pub media_outputs: Vec<MediaOutputSpec>,
}

impl Registration {
    /// Convenience constructor for bare registrations with minimal fields.
    pub fn bare(provider: ProviderName, primitive: Primitive) -> Self {
        Self {
            id: RegistrationId::generate(),
            provider,
            primitive,
            strategy: RegistrationStrategy::Bare,
            honored_fields: Vec::new(),
            media_inputs: Vec::new(),
            media_outputs: Vec::new(),
        }
    }

    /// Access the moniker when this registration is skill-shaped.
    pub fn moniker(&self) -> Option<&Moniker> {
        match &self.strategy {
            RegistrationStrategy::Skill { moniker, .. } => Some(moniker),
            _ => None,
        }
    }

    /// `true` if the provider honors (publishes as supported) this
    /// canonical field.
    pub fn honors_field(&self, path: &FieldPath) -> bool {
        self.honored_fields.iter().any(|f| f.path == *path)
    }

    /// Look up the narrowing record for a specific field, if any.
    pub fn honored_field(&self, path: &FieldPath) -> Option<&HonoredField> {
        self.honored_fields.iter().find(|f| f.path == *path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistrationStrategy {
    /// Bare primitive: the provider serves any caller matching the
    /// primitive. Typical for model-oriented cloud APIs.
    Bare,
    /// Model-oriented: the provider publishes a catalog of models it
    /// offers for this primitive.
    Models { catalog: Vec<ModelDescriptor> },
    /// Skill-oriented: the provider offers this primitive under a named
    /// moniker (e.g., ComfyUI workflows).
    Skill {
        moniker: Moniker,
        display_name: String,
        description: Option<String>,
    },
}

/// A field the provider honors, optionally narrowed relative to the
/// vocabulary spec.
///
/// **Skill specialization (ORCH-0029)**: skills extend the provider's
/// honored fields with three optional overlays:
///
/// - `label` — override the vocabulary's description for the dashboard.
/// - `default` — pre-fill value the dispatcher applies when the caller
///   omits the field.
/// - `constraint` — narrow the vocabulary's `FieldType` (Options /
///   Range / Auto). Validated by the contextualizer; rendered by the
///   dashboard.
///
/// All three are `None` for plain provider registrations and only set
/// by skill-aware adapters.
#[derive(Debug, Clone, PartialEq)]
pub struct HonoredField {
    pub path: FieldPath,
    /// Provider narrowing: the vocabulary may mark a field optional; the
    /// provider may require it for its own wire format.
    pub required: bool,
    /// Provider narrowing: the vocabulary may have a broader range; the
    /// provider may tighten it via [`FieldRange`]. Subsumed by
    /// [`FieldConstraint::Range`] when set; kept for backward
    /// compatibility with non-skill registrations.
    pub range: Option<FieldRange>,
    /// Skill-specific dashboard label override.
    pub label: Option<String>,
    /// Skill-specific default value used by the dispatcher and
    /// pre-filled by the dashboard form.
    pub default: Option<serde_json::Value>,
    /// Skill-specific narrowing of the vocabulary type. The dashboard
    /// renders this overlay; the contextualizer validates against it.
    pub constraint: Option<FieldConstraint>,
}

impl Eq for HonoredField {}

impl HonoredField {
    pub fn new(path: FieldPath) -> Self {
        Self {
            path,
            required: false,
            range: None,
            label: None,
            default: None,
            constraint: None,
        }
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub fn with_range(mut self, range: FieldRange) -> Self {
        self.range = Some(range);
        self
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn with_default(mut self, default: serde_json::Value) -> Self {
        self.default = Some(default);
        self
    }

    pub fn with_constraint(mut self, constraint: FieldConstraint) -> Self {
        self.constraint = Some(constraint);
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldRange {
    Integer { min: Option<i64>, max: Option<i64> },
    Number { min: Option<f64>, max: Option<f64> },
}

impl Eq for FieldRange {}

/// Narrows a vocabulary `FieldType` for a specific skill (ORCH-0029).
///
/// Skills declare these on their honored fields. The dashboard reads
/// the vocabulary's base `FieldType` and applies this overlay to pick
/// the right widget (slider, dropdown, autofill). The contextualizer
/// validates incoming values against the constraint after passing the
/// vocabulary's broader type check.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FieldConstraint {
    /// Restrict to a finite set of values. Compatible with vocabulary
    /// types `String`, `Integer`, `Number`.
    Options { options: Vec<ParamOption> },
    /// Tighten a numeric range. Compatible with `Integer`, `Number`.
    /// `min`/`max` MUST be inside the vocabulary's declared range.
    Range {
        min: f64,
        max: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        step: Option<f64>,
    },
    /// Auto-generated value (e.g., random seed). The dispatcher fills
    /// the field if the caller omits it. The dashboard renders a
    /// "regenerate" button.
    Auto {
        #[serde(rename = "auto")]
        kind_inner: AutoKind,
    },
}

impl Eq for FieldConstraint {}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ParamOption {
    pub value: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl Eq for ParamOption {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoKind {
    /// Random unsigned 64-bit integer per request (seeds).
    RandomInt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaInputSpec {
    pub field: FieldPath,
    pub delivery: crate::domain::media::MediaDelivery,
    pub accepted_types: Vec<String>,
    /// When set, the dashboard renders this slot as a paint overlay
    /// on the named role's image. Used by inpaint skills (ORCH-0029
    /// migrates the prior `ContentSlot.overlay` to this field).
    pub overlay: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaOutputSpec {
    pub field: FieldPath,
    pub content_type: String,
}

// ── Model catalog ─────────────────────────────────────────────

/// A model advertised by a provider. Models are keyed by
/// [`ModelFqn`] (`provider|short_name`) to avoid collisions.
///
/// Metadata fields are populated best-effort by the provider — the
/// recommendation engine treats absent values as "unknown" and
/// either skips that scoring layer or applies a neutral score.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    pub fqn: ModelFqn,
    pub short_name: String,
    pub primitives: Vec<Primitive>,
    /// Vendor-native capability tags. Recommendation profiles
    /// declare which tag they require to consider a model eligible.
    /// Examples: `"completion"`, `"tools"`, `"thinking"`,
    /// `"vision"`, `"embedding"`.
    pub capability_tags: Vec<String>,
    pub size_bytes: Option<u64>,
    /// Context window in tokens. Used by capability profiles whose
    /// scoring weights value long context (synthesis, thinking, …).
    pub context_length: Option<u64>,
    /// Total parameter count. Used by quality-bonus scoring and by
    /// size-constrained capability profiles (e.g. `quickchat` caps
    /// at small models, `think` requires large ones).
    pub parameter_count: Option<u64>,
}

/// Provider-supplied descriptor used inside
/// [`RegistrationStrategy::Models`] to declare model catalogs at
/// registration time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelDescriptor {
    pub short_name: String,
    pub capability_tags: Vec<String>,
    pub size_bytes: Option<u64>,
    pub context_length: Option<u64>,
    pub parameter_count: Option<u64>,
}

// ── Performance hints ─────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct PerformanceHint {
    pub registration_id: RegistrationId,
    pub verdict: PerformanceVerdict,
    pub sample_count: u32,
    pub measured_at: DateTime<Utc>,
}

impl Eq for PerformanceHint {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceVerdict {
    Fast,
    Degraded,
    Vetoed,
    Blocked,
    Unmeasured,
}

// ── Provider outcomes ─────────────────────────────────────────

/// The three ways a provider can deliver work.
pub enum ProviderOutcome {
    /// Provider produced a complete result inline.
    Sync(Output),
    /// Provider accepted the request and is processing asynchronously.
    /// The output carries `job.id`, `job.status`, and optionally
    /// `job.eta_seconds`.
    Async(Output),
    /// Provider is producing a stream of deltas.
    /// `initial` is the pre-stream announcement (carries any
    /// pre-allocated `media_id`, `job.id`, etc.).
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
    /// Caller pinned a model (via `selectors.model`) that no healthy
    /// instance of this provider is currently serving. Distinct from
    /// `Unsupported` because the model may be a valid choice later
    /// when the instance comes back — this is not a caller error, it
    /// is a transient availability error.
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

// ── Helper for providers: watch publisher shell ───────────────

/// A small utility that adapters can embed to hold their published
/// state. Wraps a `watch::Sender<Arc<ProviderState>>` and an `Arc`
/// snapshot read cheaply by [`Provider::state`].
pub struct ProviderStatePublisher {
    tx: watch::Sender<Arc<ProviderState>>,
}

impl ProviderStatePublisher {
    pub fn new(initial: ProviderState) -> Self {
        let (tx, _rx) = watch::channel(Arc::new(initial));
        Self { tx }
    }

    pub fn snapshot(&self) -> Arc<ProviderState> {
        self.tx.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<Arc<ProviderState>> {
        self.tx.subscribe()
    }

    pub fn publish(&self, state: ProviderState) {
        // `send_replace` rather than `send`: the publisher may
        // advance the state before any receiver subscribes (e.g.
        // an adapter's discovery subscriber fires before the
        // Directory's per-provider forwarder gets attached).
        // `send` would silently drop the update on the floor.
        let _ = self.tx.send_replace(Arc::new(state));
    }

    pub fn modify<F>(&self, update: F)
    where
        F: FnOnce(ProviderState) -> ProviderState,
    {
        let current = (**self.tx.borrow()).clone();
        let next = update(current);
        let _ = self.tx.send_replace(Arc::new(next));
    }
}
