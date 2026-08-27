//! The ask/tell pair: how a newcomer learns who is here, fast.
//!
//! Wire-compatible with the PoC's `DiscoveryRequest`/`DiscoveryResponse`
//! (transcribed from `poc/common/src/types/discovery.rs`). The response is
//! deliberately lightweight — an endpoint to talk to, not a full chirp;
//! the full picture arrives by heartbeat.

use serde::{Deserialize, Serialize};

/// What a discovery request asks for. `"moss"` finds stones ([`TARGET_MOSS`]).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscoveryRequest {
    /// What kind of speaker we are looking for.
    pub discover: String,
    /// Echoed by responders' logs; correlates one round of answers.
    pub request_id: String,
    /// Who is asking (a stone name or client label).
    pub requester: String,
    /// Rich ask (ADR-0004 §1): responders include their service inventory
    /// so a newcomer populates its cache in one exchange. Absent/false =
    /// lean (legacy answer shape). Absent from the wire while false —
    /// legacy request bytes stay identical.
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

/// Where a willing respondent lives. Lightweight on purpose: enough to
/// open an HTTP conversation, no more — UNLESS the ask was rich, in which
/// case the inventory rides here and hearts beats confirm it later.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscoveryResponse {
    /// Responding stone's identity; absent on some v0 speakers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stone_id: Option<String>,
    pub stone_name: String,
    /// Network address of the responding stone.
    pub address: crate::chirp::PeerAddress,
    pub moss_version: String,
    /// Legacy Lantern registry endpoint (v0 field; v1 emits absent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lantern_endpoint: Option<String>,
    /// Rich answer payload (present iff the request carried rich:true):
    /// the responder's own `services[]` projection + revision anchors,
    /// identical shapes to [`crate::chirp::ChirpBody`] fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub services: Option<Vec<crate::chirp::ServiceEntry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub svc_rev: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub svc_total: Option<u32>,
}

/// The `discover` value meaning "stones, answer me".
pub const TARGET_MOSS: &str = "moss";
