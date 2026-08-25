//! Response envelope builders — the single shape every `/v1/*` handler
//! emits.

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};

use crate::domain::errors::{ErrorCode, OrchestratorError};
use crate::domain::ids::{CorrelationId, ProviderName, RequestId, ResponseId};
use crate::domain::output::Output;

/// The `_meta` block stamped on every response envelope.
#[derive(Debug, Clone, Serialize)]
pub struct Meta {
    pub correlation_id: String,
    pub request_id: String,
    pub response_id: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotent: Option<bool>,
    pub received_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_path: Option<String>,
}

impl Meta {
    pub fn build(
        correlation_id: &CorrelationId,
        request_id: &RequestId,
        action: String,
        provider: Option<&ProviderName>,
        model: Option<String>,
        mode: &'static str,
        received_at: DateTime<Utc>,
        resolution_path: Option<String>,
    ) -> Self {
        Self {
            correlation_id: correlation_id.as_str().to_string(),
            request_id: request_id.as_str().to_string(),
            response_id: ResponseId::generate().as_str().to_string(),
            action,
            provider: provider.map(|p| p.as_str().to_string()),
            model,
            mode: mode.to_string(),
            idempotent: None,
            received_at,
            completed_at: Utc::now(),
            resolution_path,
        }
    }

    pub fn mark_idempotent(mut self) -> Self {
        self.idempotent = Some(true);
        self
    }
}

/// A successful response envelope.
#[derive(Debug, Clone, Serialize)]
pub struct SuccessEnvelope {
    pub output: Value,
    #[serde(rename = "_meta")]
    pub meta: Meta,
}

impl SuccessEnvelope {
    pub fn from_output(output: &Output, meta: Meta) -> Self {
        Self {
            output: output.to_nested(),
            meta,
        }
    }
}

/// An error response envelope.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
    #[serde(rename = "_meta")]
    pub meta: Meta,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Value::is_null")]
    pub details: Value,
}

impl ErrorEnvelope {
    pub fn from_error(err: OrchestratorError, meta: Meta) -> Self {
        Self {
            error: ErrorBody {
                code: err.code.as_str().to_string(),
                message: err.message,
                details: err.details,
            },
            meta,
        }
    }
}

/// Construct a minimal error envelope for cases where no request has
/// been fully parsed yet (bad URL, invalid body, …).
pub fn quick_error(
    code: ErrorCode,
    message: impl Into<String>,
    correlation: Option<CorrelationId>,
) -> Value {
    let correlation = correlation.unwrap_or_else(CorrelationId::generate);
    json!({
        "error": {
            "code": code.as_str(),
            "message": message.into(),
            "details": {},
        },
        "_meta": {
            "correlation_id": correlation.as_str(),
            "request_id": RequestId::generate().as_str(),
            "response_id": ResponseId::generate().as_str(),
            "action": "unknown",
            "mode": "sync",
            "received_at": Utc::now(),
            "completed_at": Utc::now(),
        },
    })
}
