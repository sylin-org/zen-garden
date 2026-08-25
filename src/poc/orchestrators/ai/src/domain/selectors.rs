//! Request selectors and constraints.
//!
//! Selectors are caller overrides extracted from the request body or
//! URL: `provider`, `model`, `skill`, `variant`. Constraints shape
//! what the orchestrator will allow (zone, idempotency key, explicit
//! sync/async mode, ...).

use serde::{Deserialize, Serialize};

use crate::domain::ids::ProviderName;
use crate::domain::moniker::Moniker;

/// Caller-supplied routing selectors.
///
/// Conflict semantics: any two selectors set inconsistently (e.g., a
/// skill that pins a provider disagrees with an explicit
/// `selectors.provider`) must be rejected by the contextualizer with
/// `validation_failed`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selectors {
    /// Caller-supplied provider override (e.g., `"ollama"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<ProviderName>,
    /// Caller-supplied model hint — short name or fully-qualified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Caller-supplied skill moniker (normally carried in the URL).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill: Option<Moniker>,
    /// Skill-meta variant selector for multi-workflow skills
    /// (ORCH-0029). The skill's catalog metadata declares the
    /// available variants; the dashboard renders a dropdown; the
    /// request carries the chosen variant here. Skill-aware providers
    /// consume it during `onboard`. Generic across providers — not
    /// ComfyUI-specific.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

impl Selectors {
    pub fn is_empty(&self) -> bool {
        self.provider.is_none()
            && self.model.is_none()
            && self.skill.is_none()
            && self.variant.is_none()
    }
}

/// Request-shaping constraints applied by the contextualizer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Constraints {
    /// Zone constraint (internal/external/off-line). Defaults to
    /// `Any` — whichever provider is chosen by recommendation.
    #[serde(default)]
    pub zone: ZoneConstraint,
    /// Execution mode requested by the caller. When absent the
    /// provider's chosen mode wins.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<ExecutionMode>,
    /// Idempotency key copied from the `Idempotency-Key` header.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}

/// Zone constraint (§ADR Contextualization — pass 8).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoneConstraint {
    /// No zone restriction (default).
    #[default]
    Any,
    /// Prefer providers inside the pond.
    Internal,
    /// Allow providers outside the pond (cloud).
    External,
}

/// Execution mode the caller is asking for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Sync,
    Async,
    Stream,
}
