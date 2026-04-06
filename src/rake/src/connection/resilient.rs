//! Layer 1: Resilient connection with automatic recovery
//!
//! Wraps a [`Stone`] and adds re-resolution on TCP connection failure.
//! HTTP errors (404, 500) are NOT connection failures -- they mean
//! the service exists and responded.

use std::pin::Pin;
use std::sync::Arc;

use crate::connection::resolution::{self, CachedStoneOps, Origin, Resolved};
use crate::connection::stone::Stone;
use crate::tending;

/// A connection that recovers from TCP failures by re-resolving.
///
/// On connection failure (refused, timeout, DNS -- not HTTP errors):
/// 1. If the origin is soft (tending/discovered), flush stale tending
/// 2. Re-resolve via the priority cascade
/// 3. Retry once with the new endpoint
///
/// `Runtime::execute` builds this for standard commands.
pub struct Resilient {
    stone: Stone,
    client: reqwest::Client,
    at: Option<String>,
    cache: Option<Arc<dyn CachedStoneOps>>,
}

impl Resilient {
    pub fn new(
        stone: Stone,
        client: reqwest::Client,
        at: Option<String>,
        cache: Option<Arc<dyn CachedStoneOps>>,
    ) -> Self {
        Self {
            stone,
            client,
            at,
            cache,
        }
    }

    /// The current bound connection.
    pub fn stone(&self) -> &Stone {
        &self.stone
    }

    /// Execute an operation with automatic recovery on TCP failure.
    ///
    /// 1. Run the operation against the current stone.
    /// 2. If TCP failure AND soft origin: flush tending, re-resolve, retry once.
    /// 3. If same endpoint or hard origin: propagate the original error.
    pub async fn execute<F, T>(&mut self, operation: F) -> anyhow::Result<T>
    where
        F: for<'a> Fn(&'a Stone) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<T>> + 'a>>,
    {
        let result = operation(&self.stone).await;

        let Err(ref e) = result else {
            return result;
        };

        if !is_connection_failure(e) || !self.stone.is_reclaimable() {
            return result;
        }

        tracing::warn!(
            endpoint = %self.stone.endpoint(),
            error = %e,
            "TCP failure on soft endpoint -- re-resolving"
        );

        if matches!(self.stone.origin(), Origin::Tending) {
            let _ = tending::clear_tending();
        }

        let cache_ref: Option<&dyn CachedStoneOps> =
            self.cache.as_deref();

        let new_resolved = resolution::resolve(
            &self.client,
            self.at.as_deref(),
            cache_ref,
            Some(self.stone.endpoint()),
        )
        .await?;

        if new_resolved.endpoint == self.stone.endpoint() {
            // Same endpoint after re-resolution -- nothing to do
            return result;
        }

        // Got a different endpoint -- rebuild and retry once
        self.stone = bind_stone(&self.client, new_resolved);
        operation(&self.stone).await
    }
}

/// Bind a resolved endpoint to a stone, seeding from tending when available.
pub fn bind_stone(client: &reqwest::Client, resolved: Resolved) -> Stone {
    if let Ok(state) = tending::read_tending()
        && state.endpoint == resolved.endpoint
        && state.capabilities.is_some()
    {
        tracing::debug!(stone = %state.stone_name, "Stone: seeded from tending cache");
        return Stone::bind_seeded(client.clone(), resolved, &state);
    }
    Stone::bind(client.clone(), resolved)
}

/// Returns `true` if the error chain contains a TCP-level connection failure
/// (refused, timeout, DNS) -- NOT an HTTP error (404, 500).
pub fn is_connection_failure(err: &anyhow::Error) -> bool {
    for cause in err.chain() {
        if let Some(e) = cause.downcast_ref::<reqwest::Error>() {
            return e.is_connect() || e.is_timeout();
        }
    }
    false
}
