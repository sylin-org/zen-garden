//! Idempotency store — caches request outcomes by the user's
//! `Idempotency-Key` header and detects content conflicts.
//!
//! Two distinct hashes split the responsibility (§ADR Acceptance-7):
//!
//! - [`IdempotencyKey`] derives from the user-supplied header plus the
//!   action, and identifies the cache slot. It is the user's *promise*
//!   that "these two requests are the same logical operation."
//! - [`ContentFingerprint`] derives from the canonicalized payload and
//!   selectors *after* the contextualizer normalizes alias forms, and
//!   captures *what was actually requested*. Two semantically equal
//!   requests sent in different alias forms produce identical
//!   fingerprints; a request that reuses the key with different bytes
//!   produces a different fingerprint and surfaces as a conflict.
//!
//! On lookup, the dispatcher compares the incoming fingerprint with
//! the stored one:
//!
//! - no record       → cache miss, dispatch and store
//! - same fingerprint → cache hit, short-circuit
//! - new fingerprint → [`IdempotencyError::Conflict`] → 422

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::domain::ids::JobId;
use crate::domain::output::Output;

/// The cache slot identity — the user's `Idempotency-Key` header
/// scoped to the action so two unrelated actions reusing the same
/// header are not collisions.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    /// Compute a slot key from the header and the action it targets.
    ///
    /// The payload and selectors are intentionally **not** included —
    /// they live in [`ContentFingerprint`] so the store can distinguish
    /// "same key, same content" (hit) from "same key, different
    /// content" (conflict).
    pub fn from_header(header: &str, action_dotted: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(header.as_bytes());
        hasher.update(b"\x1f");
        hasher.update(action_dotted.as_bytes());
        let digest = hasher.finalize();
        Self(hex::encode(digest))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A content hash over the canonicalized payload and selectors. Equal
/// fingerprints mean the dispatcher would have produced the same
/// upstream call; differing fingerprints under the same
/// [`IdempotencyKey`] are a conflict.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContentFingerprint(String);

impl ContentFingerprint {
    pub fn compute(
        canonical_payload: &serde_json::Value,
        selectors_json: &serde_json::Value,
    ) -> Self {
        let payload_str = canonicalize(canonical_payload);
        let selectors_str = canonicalize(selectors_json);
        let mut hasher = Sha256::new();
        hasher.update(payload_str.as_bytes());
        hasher.update(b"\x1f");
        hasher.update(selectors_str.as_bytes());
        let digest = hasher.finalize();
        Self(hex::encode(digest))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Deterministic JSON canonicalization: sort keys, strip whitespace,
/// no trailing commas. Enough to give two logically-equal payloads
/// the same byte string.
fn canonicalize(value: &serde_json::Value) -> String {
    use serde_json::Value;
    match value {
        Value::Object(map) => {
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            let body = entries
                .into_iter()
                .map(|(k, v)| format!("{}:{}", canonicalize(&Value::String(k.clone())), canonicalize(v)))
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{body}}}")
        }
        Value::Array(arr) => {
            let body = arr
                .iter()
                .map(canonicalize)
                .collect::<Vec<_>>()
                .join(",");
            format!("[{body}]")
        }
        Value::String(s) => serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into()),
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
    }
}

/// What an idempotency cache entry stores.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CachedResponse {
    /// Complete sync output.
    Sync { output: Output },
    /// Reference to an async job — the job store remains the single
    /// source of truth for the actual result.
    AsyncJob { job_id: JobId },
}

#[derive(Debug, Clone)]
pub struct IdempotencyRecord {
    pub key: IdempotencyKey,
    pub fingerprint: ContentFingerprint,
    pub response: CachedResponse,
    pub stored_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

// ── IdempotencyStore trait ────────────────────────────────────

#[async_trait]
pub trait IdempotencyStore: Send + Sync + 'static {
    async fn lookup(
        &self,
        key: &IdempotencyKey,
    ) -> Result<Option<IdempotencyRecord>, IdempotencyError>;

    async fn store(
        &self,
        key: IdempotencyKey,
        fingerprint: ContentFingerprint,
        response: CachedResponse,
    ) -> Result<(), IdempotencyError>;

    /// Sweep of stale entries.
    async fn sweep(&self, now: DateTime<Utc>) -> Result<u64, IdempotencyError>;
}

#[derive(Debug, thiserror::Error)]
pub enum IdempotencyError {
    #[error("idempotency key {0:?} collision — same key, different content")]
    Conflict(IdempotencyKey),
    #[error("storage error: {0}")]
    Storage(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn same_header_and_action_produce_same_key() {
        let a = IdempotencyKey::from_header("k1", "text.chat");
        let b = IdempotencyKey::from_header("k1", "text.chat");
        assert_eq!(a, b);
    }

    #[test]
    fn different_action_under_same_header_is_a_distinct_slot() {
        let a = IdempotencyKey::from_header("k1", "text.chat");
        let b = IdempotencyKey::from_header("k1", "text.translate");
        assert_ne!(a, b, "different actions are unrelated requests");
    }

    #[test]
    fn fingerprint_collapses_equal_payloads() {
        let payload_a = json!({"text": {"prompt": {"user": "hi"}}});
        let payload_b = json!({"text": {"prompt": {"user": "hi"}}});
        let a = ContentFingerprint::compute(&payload_a, &json!({}));
        let b = ContentFingerprint::compute(&payload_b, &json!({}));
        assert_eq!(a, b);
    }

    #[test]
    fn fingerprint_separates_different_payloads() {
        let payload_a = json!({"text": {"prompt": {"user": "hi"}}});
        let payload_b = json!({"text": {"prompt": {"user": "hello"}}});
        let a = ContentFingerprint::compute(&payload_a, &json!({}));
        let b = ContentFingerprint::compute(&payload_b, &json!({}));
        assert_ne!(a, b);
    }

    #[test]
    fn canonicalize_is_key_order_independent() {
        let a = canonicalize(&json!({"b": 1, "a": 2}));
        let b = canonicalize(&json!({"a": 2, "b": 1}));
        assert_eq!(a, b);
    }
}
