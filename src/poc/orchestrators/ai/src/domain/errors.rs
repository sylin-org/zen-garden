//! Error taxonomy — stable codes and their HTTP status mapping.
//!
//! Every error produced by the orchestrator carries a typed [`ErrorCode`]
//! from this taxonomy. Handlers serialize it into the canonical error
//! envelope (§ADR Error responses).

use std::fmt;

use serde::{Deserialize, Serialize};

/// The stable error taxonomy (§ADR).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Request violated input vocabulary or provider narrowing.
    ValidationFailed,
    /// Zone constraint, budget, or similar filtered all candidates.
    ConstraintUnsatisfied,
    /// Action, skill, model, media, job, or provider does not exist.
    NotFound,
    /// No provider is registered for this primitive.
    NoCandidates,
    /// Network or health failure talking to the provider.
    ProviderUnreachable,
    /// Provider returned a busy signal.
    ProviderOverloaded,
    /// Cloud provider rejected credentials.
    AuthFailed,
    /// Caller hit a rate limit.
    RateLimited,
    /// Caller hit a configured quota.
    QuotaExhausted,
    /// Exceeded a time budget.
    Timeout,
    /// Same idempotency key used for a different request content.
    IdempotencyConflict,
    /// Provider returned an unclassifiable failure.
    UpstreamError,
    /// Orchestrator bug.
    InternalError,
}

impl ErrorCode {
    /// Canonical snake_case identifier used in responses.
    pub const fn as_str(self) -> &'static str {
        match self {
            ErrorCode::ValidationFailed => "validation_failed",
            ErrorCode::ConstraintUnsatisfied => "constraint_unsatisfied",
            ErrorCode::NotFound => "not_found",
            ErrorCode::NoCandidates => "no_candidates",
            ErrorCode::ProviderUnreachable => "provider_unreachable",
            ErrorCode::ProviderOverloaded => "provider_overloaded",
            ErrorCode::AuthFailed => "auth_failed",
            ErrorCode::RateLimited => "rate_limited",
            ErrorCode::QuotaExhausted => "quota_exhausted",
            ErrorCode::Timeout => "timeout",
            ErrorCode::IdempotencyConflict => "idempotency_conflict",
            ErrorCode::UpstreamError => "upstream_error",
            ErrorCode::InternalError => "internal_error",
        }
    }

    /// HTTP status mapping (§ADR Error responses).
    pub const fn http_status(self) -> u16 {
        match self {
            ErrorCode::ValidationFailed => 400,
            ErrorCode::ConstraintUnsatisfied => 400,
            ErrorCode::NotFound => 404,
            ErrorCode::NoCandidates => 503,
            ErrorCode::ProviderUnreachable => 503,
            ErrorCode::ProviderOverloaded => 503,
            ErrorCode::AuthFailed => 502,
            ErrorCode::RateLimited => 429,
            ErrorCode::QuotaExhausted => 429,
            ErrorCode::Timeout => 504,
            ErrorCode::IdempotencyConflict => 422,
            ErrorCode::UpstreamError => 502,
            ErrorCode::InternalError => 500,
        }
    }

    /// Every error code, in declaration order.
    pub const ALL: &'static [ErrorCode] = &[
        ErrorCode::ValidationFailed,
        ErrorCode::ConstraintUnsatisfied,
        ErrorCode::NotFound,
        ErrorCode::NoCandidates,
        ErrorCode::ProviderUnreachable,
        ErrorCode::ProviderOverloaded,
        ErrorCode::AuthFailed,
        ErrorCode::RateLimited,
        ErrorCode::QuotaExhausted,
        ErrorCode::Timeout,
        ErrorCode::IdempotencyConflict,
        ErrorCode::UpstreamError,
        ErrorCode::InternalError,
    ];
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The canonical error record emitted on the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub details: serde_json::Value,
}

impl OrchestratorError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: serde_json::Value::Null,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = details;
        self
    }
}

impl fmt::Display for OrchestratorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for OrchestratorError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_has_unique_identifier() {
        let mut seen = std::collections::HashSet::new();
        for code in ErrorCode::ALL {
            assert!(seen.insert(code.as_str()), "duplicate: {}", code.as_str());
        }
        assert_eq!(ErrorCode::ALL.len(), 13);
    }

    #[test]
    fn http_status_codes_are_in_expected_ranges() {
        for code in ErrorCode::ALL {
            let status = code.http_status();
            assert!(
                (400..=599).contains(&status),
                "status {status} out of range for {code}"
            );
        }
    }

    #[test]
    fn serialize_uses_snake_case() {
        let err = OrchestratorError::new(ErrorCode::ValidationFailed, "bad field");
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "validation_failed");
        assert_eq!(json["message"], "bad field");
    }
}
