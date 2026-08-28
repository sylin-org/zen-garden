//! The ask/tell pair: how a newcomer learns who is here, fast.
//!
//! Grammar follows the frame law (CODE-RULES P3): sections hold facts.
//! The request declares its DEPTH; the response carries a `stone:` block
//! always and an inventory block when the ask was rich. Rich shapes are
//! IDENTICAL to the chirp frame's (one canonical shape, many mouths — B1).

use serde::{Deserialize, Serialize};

/// The `discover` value meaning "stones, answer me".
pub const TARGET_MOSS: &str = "moss";

/// What a discovery request asks for. `"moss"` finds stones ([`TARGET_MOSS`]).
#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscoveryRequest {
    /// What kind of speaker we are looking for.
    pub discover: String,
    /// Echoed by responders' logs; correlates one round of answers.
    pub request_id: String,
    /// Who is asking (a stone name or client label).
    pub requester: String,
    /// Rich ask (ADR-0004 §1): responders include their service inventory
    /// so a newcomer populates its cache in one exchange. Absent from the
    /// wire while false — legacy request bytes stay lean.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rich: bool,
}

impl DiscoveryRequest {
    /// A request for moss stones, fresh `request_id`.
    pub fn for_moss(requester: impl Into<String>) -> Self {
        Self {
            discover: TARGET_MOSS.into(),
            request_id: uuid::Uuid::now_v7().to_string(),
            requester: requester.into(),
            rich: false,
        }
    }

    /// The newcomer's opening question, rich form: "who are you guys, and
    /// what do you have?"
    pub fn for_moss_rich(requester: impl Into<String>) -> Self {
        Self {
            rich: true,
            ..Self::for_moss(requester)
        }
    }
}

/// Where a willing respondent lives, and (when the ask was rich) what it
/// hosts. The `stone:` block always answers "who are you"; the inventory
/// MAP answers "what do you have" — every domain, identical shapes to the
/// chirp frame's (A2.1: the revision vector is a shape, not a field list).
#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscoveryResponse {
    /// WHO answered: identity and reachability (frame's `stone:` block).
    pub stone: crate::chirp::Stone,
    /// Legacy Lantern registry endpoint (v0 field; v1 emits absent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lantern_endpoint: Option<String>,
    /// The full inventory map, iff the request carried rich:true — services
    /// AND banks AND whatever domains the future brings (W7 finding: a
    /// newcomer learns the whole room in one exchange).
    #[serde(default, skip_serializing_if = "crate::chirp::InventoryMap::is_empty")]
    pub inventory: crate::chirp::InventoryMap,
}
