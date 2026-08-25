//! `OrchestratorRequest` and execution context.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use tracing::Span;

use crate::domain::field_path::FieldPath;
use crate::domain::ids::{CorrelationId, MediaId, ProviderName, RequestId};
use crate::domain::media::{ResolvedMedia, SharedMediaStore};
use crate::domain::moniker::Moniker;
use crate::domain::primitive::Primitive;
use crate::domain::selectors::{Constraints, Selectors};

// ── Action ────────────────────────────────────────────────────

/// What the caller wants done: a primitive, optionally scoped to a
/// skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub primitive: Primitive,
    pub skill: Option<Moniker>,
}

impl Action {
    pub fn bare(primitive: Primitive) -> Self {
        Self {
            primitive,
            skill: None,
        }
    }

    pub fn skill(primitive: Primitive, moniker: Moniker) -> Self {
        Self {
            primitive,
            skill: Some(moniker),
        }
    }

    /// Canonical dotted form, e.g. `"text.chat"` or `"image.generate.outpaint"`.
    pub fn dotted(&self) -> String {
        match &self.skill {
            Some(m) => format!("{}.{}", self.primitive.dotted(), m),
            None => self.primitive.dotted().to_string(),
        }
    }

    /// Parse a dotted action string.
    ///
    /// The first two segments identify the primitive (`text.chat`,
    /// `image.generate`, …). Any remaining segment is the skill
    /// moniker.
    pub fn parse_dotted(s: &str) -> Result<Self, ActionError> {
        let segments: Vec<&str> = s.split('.').collect();
        if segments.len() < 2 {
            return Err(ActionError::Malformed(s.to_string()));
        }
        let modality = segments[0];
        let leaf = segments[1];
        let primitive = Primitive::from_segments(modality, leaf)
            .map_err(|_| ActionError::UnknownPrimitive(format!("{modality}.{leaf}")))?;
        let skill = match segments.len() {
            2 => None,
            3 => Some(Moniker::new(segments[2]).map_err(ActionError::Moniker)?),
            _ => return Err(ActionError::TooManySegments(s.to_string())),
        };
        Ok(Self { primitive, skill })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ActionError {
    #[error("action `{0}` is malformed (expected at least two dotted segments)")]
    Malformed(String),
    #[error("action has too many dotted segments: `{0}`")]
    TooManySegments(String),
    #[error("unknown primitive in action: `{0}`")]
    UnknownPrimitive(String),
    #[error(transparent)]
    Moniker(#[from] crate::domain::moniker::MonikerError),
}

// ── MediaReference / MediaContext ─────────────────────────────

#[derive(Debug, Clone)]
pub struct MediaReference {
    pub id: MediaId,
    pub field: FieldPath,
    pub content_type: String,
    pub metadata: Value,
}

#[derive(Debug, Default, Clone)]
pub struct MediaContext {
    pub referenced: Vec<MediaReference>,
    pub resolutions: HashMap<String, ResolvedMedia>,
}

impl MediaContext {
    pub fn find_at_field(&self, field: &FieldPath) -> Option<&MediaReference> {
        self.referenced.iter().find(|r| &r.field == field)
    }
}

// ── ExecutionContext ──────────────────────────────────────────

/// Non-data dependencies providers receive alongside the request. The
/// context is on the request itself so there is only one object to
/// pass through the pipeline.
#[derive(Clone)]
pub struct ExecutionContext {
    pub media_store: SharedMediaStore,
    pub job_sink: Arc<crate::domain::jobs::JobSink>,
    pub cancel: CancellationToken,
    pub span: Span,
}

impl std::fmt::Debug for ExecutionContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionContext")
            .field("job_id", self.job_sink.job_id())
            .finish()
    }
}

// ── RawRequest ────────────────────────────────────────────────

/// What the HTTP handler hands to the dispatcher. Carries caller
/// intent plus the non-state runtime primitives (cancel, span) — but
/// not the `ExecutionContext` and not a job. The dispatcher builds
/// both internally as part of `dispatch()`.
#[derive(Clone)]
pub struct RawRequest {
    pub id: RequestId,
    pub correlation_id: CorrelationId,
    pub received_at: DateTime<Utc>,
    pub action: Action,
    pub payload: Value,
    pub selectors: Selectors,
    pub constraints: Constraints,
    pub cancel: CancellationToken,
    pub span: Span,
}

impl std::fmt::Debug for RawRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RawRequest")
            .field("id", &self.id)
            .field("correlation_id", &self.correlation_id)
            .field("action", &self.action.dotted())
            .finish()
    }
}

// ── OrchestratorRequest ───────────────────────────────────────

#[derive(Clone)]
pub struct OrchestratorRequest {
    // Identity
    pub id: RequestId,
    pub correlation_id: CorrelationId,
    pub received_at: DateTime<Utc>,

    // Intent
    pub action: Action,
    pub payload: Value,
    pub selectors: Selectors,
    pub constraints: Constraints,

    // Media
    pub media: MediaContext,

    /// The provider chosen by the contextualizer to serve this
    /// request. Adapter `onboard` reads this to confirm it is the
    /// addressee. Set to `None` until the contextualizer runs.
    ///
    /// Note (ORCH-0030 R2 M3): the legacy `resolved_model` field
    /// has been removed — model resolution is now adapter-local
    /// and lives inside each adapter's `onboard`.
    pub resolved_provider: Option<ProviderName>,

    // Execution context
    pub context: ExecutionContext,
}

impl std::fmt::Debug for OrchestratorRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrchestratorRequest")
            .field("id", &self.id)
            .field("correlation_id", &self.correlation_id)
            .field("action", &self.action.dotted())
            .field("payload_keys", &payload_keys(&self.payload))
            .field("resolved_provider", &self.resolved_provider)
            .finish()
    }
}

fn payload_keys(payload: &Value) -> Vec<&str> {
    match payload {
        Value::Object(map) => map.keys().map(|k| k.as_str()).collect(),
        _ => Vec::new(),
    }
}
