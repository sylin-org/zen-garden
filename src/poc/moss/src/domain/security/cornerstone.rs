//! Cornerstone discovery — find the stone that holds the pond CA.
//!
//! Two pond operations need to reach the cornerstone from a non-CA stone:
//! enrollment (proxy a CSR for signing) and renewal (rotate a member leaf). Both
//! walk the topology the same way, so the walk lives here once.
//!
//! The walk queries online peers' `/api/v1/pond/status` (most-recently-seen
//! first) until one reports the cornerstone's name, then resolves that name to a
//! [`PeerAddress`] via the topology cache. The cornerstone reports *itself* as
//! `role == "primary"`, so the first reachable peer that knows the pond answers.

use garden_common::PeerAddress;

use crate::Moss;

/// The discovered cornerstone — the stone holding the pond CA. Carries both the
/// name (the audience a renewal envelope is bound to) and the address (where to
/// send the request).
pub struct Cornerstone {
    pub name: String,
    pub address: PeerAddress,
}

/// Why cornerstone discovery could not resolve a CA host. Each variant maps to a
/// distinct caller response (the enrollment handler turns these into specific
/// HTTP codes; the renewal loop logs and retries on the next tick).
#[derive(Debug, thiserror::Error)]
pub enum CornerstoneError {
    /// No online peers are known, so there is no one to ask. Discovery cannot
    /// even begin until other stones are seen on the network.
    #[error(
        "no online peers discovered — cannot find the cornerstone; \
         ensure other stones are running and on the same network"
    )]
    NoPeers,
    /// A peer named the cornerstone, but it is not in the local topology cache,
    /// so its address is unknown. Resolves once discovery catches up.
    #[error("cornerstone '{name}' identified but not yet present in the topology cache")]
    NotInTopology { name: String },
    /// Every reachable peer was queried and none reported a pond cornerstone —
    /// no keystone has been placed in the garden (or none is reachable).
    #[error("no active pond discovered in the garden")]
    NoPond,
}

/// Discover the cornerstone (name + address) by asking online peers.
///
/// Skips this stone (a member never asks itself) and prefers the most recently
/// seen peers. Returns the first cornerstone a reachable peer reports and whose
/// address is in the topology cache.
pub async fn discover(state: &Moss) -> Result<Cornerstone, CornerstoneError> {
    // Collect online peers, most recently seen first.
    let mut candidates: Vec<_> = state
        .topology
        .online_stones()
        .await
        .into_iter()
        .filter(|e| e.stone_name != state.current.stone.name)
        .collect();
    candidates.sort_by_key(|e| std::cmp::Reverse(e.last_seen));

    if candidates.is_empty() {
        return Err(CornerstoneError::NoPeers);
    }

    for entry in &candidates {
        let resp = match state
            .security
            .stone_client()
            .get(&entry.address, "/api/v1/pond/status")
            .timeout(garden_common::constants::timeouts::pond_operation_timeout())
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => r,
            _ => continue,
        };

        let body: serde_json::Value = match resp.json().await {
            Ok(b) => b,
            Err(_) => continue,
        };

        let cornerstone_name = body
            .get("data")
            .and_then(|d| d.get("cornerstone"))
            .and_then(|c| c.as_str());

        if let Some(name) = cornerstone_name {
            // Found the cornerstone hostname — resolve its address via topology.
            if let Some(found) = state.topology.get_by_name(name).await {
                tracing::info!(
                    cornerstone = %name,
                    endpoint = %found.address,
                    via = %entry.stone_name,
                    "Cornerstone discovered via peer"
                );
                return Ok(Cornerstone {
                    name: name.to_string(),
                    address: found.address.clone(),
                });
            }
            // Cornerstone identified but not in our topology cache yet.
            return Err(CornerstoneError::NotInTopology {
                name: name.to_string(),
            });
        }
    }

    Err(CornerstoneError::NoPond)
}
