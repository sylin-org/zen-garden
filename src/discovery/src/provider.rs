//! `DiscoveryProvider` trait — the typed entry point for cascade
//! resolution and stone lookup.
//!
//! Moved here from `garden-common::traits::discovery` per
//! [DISC-0001](../../../docs/decisions/DISC-0001-discovery-as-first-class-crate.md).
//! Implementation `DefaultDiscoveryProvider` lives at the bottom of
//! this file — it wraps the free functions in [`crate`].

use std::time::Duration;

/// Discovery result containing stone endpoint
#[derive(Debug, Clone)]
pub struct DiscoveryResult {
    pub stone_name: String,
    pub endpoint: String,
    pub moss_version: String,
    pub lantern_endpoint: Option<String>,
}

/// Error types for discovery operations
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("No stones found via broadcast after {0:?}")]
    NoStonesFound(Duration),

    #[error("Stone '{0}' not found")]
    StoneNotFound(String),

    #[error("UDP broadcast failed: {0}")]
    BroadcastFailed(String),

    #[error("Network error: {0}")]
    NetworkError(#[from] std::io::Error),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}

/// Stone discovery provider
pub trait DiscoveryProvider: Send + Sync {
    /// Discover stones via UDP broadcast
    ///
    /// Sends broadcast on port 3999, waits for responses.
    /// Returns all discovered stones within timeout.
    fn discover_all(
        &self,
        timeout: Duration,
    ) -> impl std::future::Future<Output = Result<Vec<DiscoveryResult>, DiscoveryError>> + Send;

    /// Find a specific stone by name
    ///
    /// Priority chain:
    /// 1. Check if stone_name matches explicit endpoint format (http://...)
    /// 2. Try UDP broadcast to find stone by name
    /// 3. Check tended stone from config
    /// 4. Check GARDEN_STONE environment variable
    fn find_stone(
        &self,
        stone_name: &str,
    ) -> impl std::future::Future<Output = Result<DiscoveryResult, DiscoveryError>> + Send;

    /// Resolve stone endpoint using priority chain
    ///
    /// Returns the first valid stone found via:
    /// 1. explicit_target if Some
    /// 2. GARDEN_STONE env var
    /// 3. Tended stone from config
    /// 4. UDP broadcast (first responder)
    fn resolve_stone(
        &self,
        explicit_target: Option<&str>,
    ) -> impl std::future::Future<Output = Result<DiscoveryResult, DiscoveryError>> + Send;
}

// ── Default implementation ─────────────────────────────────────────

/// The canonical [`DiscoveryProvider`] backed by this crate's free
/// functions ([`crate::discover_moss_auto`], [`crate::discover_moss`]).
///
/// Constructed as a unit; no state. Intended as the default resolver
/// for clients that don't need their own implementation.
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultDiscoveryProvider;

impl DefaultDiscoveryProvider {
    pub fn new() -> Self {
        Self
    }
}

impl DiscoveryProvider for DefaultDiscoveryProvider {
    fn discover_all(
        &self,
        timeout: Duration,
    ) -> impl std::future::Future<Output = Result<Vec<DiscoveryResult>, DiscoveryError>> + Send
    {
        async move {
            let stones = crate::discover_moss_auto(timeout)
                .await
                .map_err(|e| DiscoveryError::BroadcastFailed(e.to_string()))?;
            Ok(stones.into_iter().map(into_result).collect())
        }
    }

    fn find_stone(
        &self,
        stone_name: &str,
    ) -> impl std::future::Future<Output = Result<DiscoveryResult, DiscoveryError>> + Send {
        let needle = stone_name.to_string();
        async move {
            let stones = crate::discover_moss_auto(Duration::from_secs(3))
                .await
                .map_err(|e| DiscoveryError::BroadcastFailed(e.to_string()))?;
            stones
                .into_iter()
                .find(|s| s.stone_name == needle)
                .map(into_result)
                .ok_or_else(|| DiscoveryError::StoneNotFound(needle))
        }
    }

    fn resolve_stone(
        &self,
        explicit_target: Option<&str>,
    ) -> impl std::future::Future<Output = Result<DiscoveryResult, DiscoveryError>> + Send {
        let explicit = explicit_target.map(str::to_string);
        async move {
            // 1. Explicit (passed-in or env var fallback)
            let target = explicit.or_else(|| std::env::var("GARDEN_STONE").ok());
            if let Some(target) = target {
                if target.starts_with("http://") || target.starts_with("https://") {
                    return Ok(DiscoveryResult {
                        stone_name: target.clone(),
                        endpoint: target,
                        moss_version: String::new(),
                        lantern_endpoint: None,
                    });
                }
                // Treat as stone-name; fall through to find_stone-style logic.
                let needle = target.clone();
                let stones = crate::discover_moss_auto(Duration::from_secs(3))
                    .await
                    .map_err(|e| DiscoveryError::BroadcastFailed(e.to_string()))?;
                return stones
                    .into_iter()
                    .find(|s| s.stone_name == needle)
                    .map(into_result)
                    .ok_or(DiscoveryError::StoneNotFound(target));
            }

            // 2. Broadcast — first responder wins.
            let timeout = Duration::from_secs(3);
            let stones = crate::discover_moss_auto(timeout)
                .await
                .map_err(|e| DiscoveryError::BroadcastFailed(e.to_string()))?;
            stones
                .into_iter()
                .next()
                .map(into_result)
                .ok_or(DiscoveryError::NoStonesFound(timeout))
        }
    }
}

fn into_result(r: garden_common::DiscoveryResponse) -> DiscoveryResult {
    DiscoveryResult {
        stone_name: r.stone_name,
        endpoint: r.address.http_base(),
        moss_version: r.moss_version,
        lantern_endpoint: r.lantern_endpoint,
    }
}
