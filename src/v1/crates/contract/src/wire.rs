//! The announcement envelope — every datagram, one shape (R0.5).
//!
//! Byte-compatible with the PoC: JSON object with optional `msg_id`, a
//! `type` discriminator, an opaque `data` payload, and optional pond
//! signature fields v1 does not yet produce.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One garden datagram, decoded. `data` stays opaque here — typed bodies
/// (`chirp::ChirpBody`) are parsed by the handler that registered for the
/// `kind` (R2.9).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Announcement {
    /// Time-ordered identity of this datagram; receivers dedup on it for
    /// [`crate::consts::DEDUP_TTL_SECS`]. v1 always sets it; the PoC allowed
    /// absence (such datagrams bypass dedup — a PoC quirk we do not inherit).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub msg_id: Option<Uuid>,
    /// Discriminator; see [`crate::consts::announcement`].
    #[serde(rename = "type")]
    pub kind: String,
    /// Opaque payload — the handler that claimed this type gives it meaning.
    pub data: serde_json::Value,
    /// Pond signature (base64) — future; v1 emits unsigned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// Signer certificate (PEM) — future; v1 emits unsigned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_cert: Option<String>,
}

impl Announcement {
    /// A new, dedup-carrying announcement of `kind`.
    pub fn new(kind: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            msg_id: Some(Uuid::now_v7()),
            kind: kind.into(),
            data,
            signature: None,
            sender_cert: None,
        }
    }
}
