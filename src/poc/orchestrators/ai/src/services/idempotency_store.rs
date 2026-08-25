//! In-memory idempotency store.
//!
//! Idempotency entries are ephemeral — they exist to catch realistic
//! retry windows, not to persist across restarts. A dropped process
//! simply re-executes any retry arriving after the restart, which is
//! the same behavior as a cache miss.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use tokio::sync::RwLock;

use crate::domain::idempotency::{
    CachedResponse, ContentFingerprint, IdempotencyError, IdempotencyKey, IdempotencyRecord,
    IdempotencyStore,
};

/// Default entry TTL. Tunable per deployment if needed; 15 minutes
/// comfortably covers retry loops from SDKs and operator scripts.
pub const DEFAULT_TTL: ChronoDuration = ChronoDuration::minutes(15);

#[derive(Default)]
pub struct InMemoryIdempotencyStore {
    inner: RwLock<HashMap<String, IdempotencyRecord>>,
}

impl InMemoryIdempotencyStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl IdempotencyStore for InMemoryIdempotencyStore {
    async fn lookup(
        &self,
        key: &IdempotencyKey,
    ) -> Result<Option<IdempotencyRecord>, IdempotencyError> {
        let inner = self.inner.read().await;
        let Some(record) = inner.get(key.as_str()) else {
            return Ok(None);
        };
        if record.expires_at <= Utc::now() {
            return Ok(None);
        }
        Ok(Some(record.clone()))
    }

    async fn store(
        &self,
        key: IdempotencyKey,
        fingerprint: ContentFingerprint,
        response: CachedResponse,
    ) -> Result<(), IdempotencyError> {
        let now = Utc::now();
        let record = IdempotencyRecord {
            key: key.clone(),
            fingerprint,
            response,
            stored_at: now,
            expires_at: now + DEFAULT_TTL,
        };
        self.inner
            .write()
            .await
            .insert(key.as_str().to_string(), record);
        Ok(())
    }

    async fn sweep(&self, now: DateTime<Utc>) -> Result<u64, IdempotencyError> {
        let mut inner = self.inner.write().await;
        let before = inner.len();
        inner.retain(|_, record| record.expires_at > now);
        Ok((before - inner.len()) as u64)
    }
}
