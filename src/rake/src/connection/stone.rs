//! Layer 2: Bound connection to a stone
//!
//! Binds a resolved endpoint to an HTTP client. Provides typed API access,
//! raw HTTP, and endpoint metadata. No recovery -- caller controls lifecycle.

use crate::connection::resolution::{Origin, Resolved};
use crate::stone_bag::StoneBag;
use crate::tending::TendingState;
use garden_common::client::StoneApi;

/// A bound connection to a stone.
///
/// Owns the resolved endpoint, the typed API client, and the lazy
/// capabilities bag. Created by binding a [`Resolved`] endpoint to
/// an HTTP client.
///
/// Streaming commands (pulse, log tailing) use this directly.
/// Standard commands receive it through [`Resilient`](super::resilient::Resilient).
pub struct Stone {
    resolved: Resolved,
    api: StoneApi,
    bag: StoneBag,
}

impl Stone {
    /// Bind a resolved endpoint to an HTTP client.
    /// No network call -- pure construction.
    pub fn bind(client: reqwest::Client, resolved: Resolved) -> Self {
        let api = StoneApi::new(client.clone(), resolved.endpoint.clone());
        let bag = StoneBag::new(client, resolved.endpoint.clone());
        Self { resolved, api, bag }
    }

    /// Bind with pre-seeded capabilities from tending cache.
    pub fn bind_seeded(client: reqwest::Client, resolved: Resolved, tending: &TendingState) -> Self {
        let api = StoneApi::new(client.clone(), resolved.endpoint.clone());
        let bag = StoneBag::from_tending(tending, client);
        Self { resolved, api, bag }
    }

    /// The typed Stone API (ARCH-0012).
    pub fn api(&self) -> &StoneApi {
        &self.api
    }

    /// Raw HTTP client for SSE streams and escape hatches.
    pub fn http(&self) -> &reqwest::Client {
        self.api.http()
    }

    /// The resolved endpoint URL.
    pub fn endpoint(&self) -> &str {
        &self.resolved.endpoint
    }

    /// How this endpoint was resolved.
    pub fn origin(&self) -> Origin {
        self.resolved.origin
    }

    /// Whether this endpoint can be invalidated on connection failure.
    pub fn is_reclaimable(&self) -> bool {
        self.resolved.origin.is_soft()
    }

    /// Lazy stone name (from capabilities or tending cache).
    pub async fn name(&self) -> Option<&str> {
        self.bag.stone_name().await
    }

    /// Lazy stone capabilities.
    pub async fn capabilities_owned(&self) -> Option<garden_common::HardwareCapabilities> {
        self.bag.capabilities_owned().await
    }
}
