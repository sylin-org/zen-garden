//! Logical sets API surface (ARCH-0038).
//!
//! A *set* is a same-FQN replication group that emerges from the garden
//! registry. Two kinds today:
//!
//! - **Offering sets** — same-FQN elected offerings across stones.
//! - **Bank sets** — same-`replica_set_name` storage volumes across stones.
//!
//! Both kinds project from the [`GardenRegistry`](crate::domain::tool::registry)
//! hotcache; no fan-out, no orchestrator dashboard cross-call.
//!
//! ## Routes
//!
//! ```text
//! GET /api/v1/sets                       → index of available kinds
//! GET /api/v1/sets/offerings             → all offering sets
//! GET /api/v1/sets/offerings/{fqn}       → single offering set
//! GET /api/v1/sets/banks                 → all bank sets (incl. singletons)
//! GET /api/v1/sets/banks/{moniker}       → single bank set
//! ```
//!
//! See [ARCH-0038](../../../../docs/decisions/ARCH-0038-logical-sets-as-first-class-surface.md)
//! for the noun, membership rules, role vocabulary per kind, and the
//! relationship to existing surfaces (`/garden/services`, `/garden/banks`,
//! orchestrator dashboards).
//!
//! ## Module layout
//!
//! Phase C added `offerings.rs`; Phase D added `banks.rs`. The index
//! endpoint here advertises whichever sub-modules are mounted in the
//! router; it has no compile-time list to keep in sync beyond what the
//! kind constants below carry.

pub mod banks;
pub mod offerings;

use serde::Serialize;

use crate::Moss;
use crate::api::ApiResult;

/// Index entry returned by `GET /api/v1/sets`. Lets a consumer discover
/// which kinds this Moss exposes without hardcoding the list. New kinds
/// (companions? pond? — speculative) appear here when added under a
/// matching `pub mod` and route registration.
#[derive(Debug, Serialize)]
pub struct SetsIndex {
    /// Available kinds. Each value matches a sub-path: `"offerings"`
    /// → `/api/v1/sets/offerings`, etc.
    pub kinds: Vec<&'static str>,
}

/// `GET /api/v1/sets` — index of available set kinds.
///
/// Cheap, static, no state read. Mounted on both the public and full
/// routers because consumers (Pavilion canvas, Rake, future tools) may
/// want to discover the surface before the pond is up.
pub async fn list_kinds(
    axum::extract::State(_state): axum::extract::State<Moss>,
) -> ApiResult<SetsIndex> {
    crate::api::ok(SetsIndex {
        kinds: vec!["offerings", "banks"],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_lists_known_kinds() {
        // Snapshot the index shape so adding a kind without updating
        // documentation/the index handler shows up as a test failure.
        let idx = SetsIndex {
            kinds: vec!["offerings", "banks"],
        };
        let json = serde_json::to_value(&idx).unwrap();
        assert_eq!(json["kinds"], serde_json::json!(["offerings", "banks"]));
    }
}
