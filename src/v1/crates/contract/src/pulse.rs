//! The pulse (ADR-0013): everything a stone experiences, in one typed,
//! sequence-numbered envelope. One news shape; many readers — the wall,
//! the web page, a future chime or phone bridge (notification-ready by
//! construction). Kind and category speak glossary nouns.

use serde::{Deserialize, Serialize};

/// The levels a pulse event may carry (glossary::health kin).
pub const LEVEL_INFO: &str = "info";
pub const LEVEL_WARN: &str = "warn";
pub const LEVEL_ERROR: &str = "error";

/// One pulse event. `seq` is per-bus, monotonic, gap = missed news.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
pub struct PulseEvent {
    /// Monotonic per-bus sequence; a gap on reconnect means missed news.
    pub seq: u64,
    /// RFC 3339 wall-clock time at the speaking stone.
    pub ts: String,
    /// Glossary noun for what happened ("offering.placed", "topology.goodbye",
    /// "job.failed", "load.tick", "wire.delta", "snapshot").
    pub kind: String,
    /// Coarse category for filtering ("offering", "topology", "job",
    /// "storage", "stone", "wire").
    pub category: String,
    /// info | warn | error.
    pub level: String,
    /// The stone this news is about, when about a stone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stone: Option<String>,
    /// The offering this news is about, when about one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offering: Option<String>,
    /// One plain-English sentence — the wire line.
    pub summary: String,
    /// Structured detail, sections per kind (R3.9).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl PulseEvent {
    /// A convenience constructor for adapters; seq is assigned by the bus.
    pub fn new(
        kind: impl Into<String>,
        category: impl Into<String>,
        level: impl Into<String>,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            seq: 0,
            ts: chrono::Utc::now().to_rfc3339(),
            kind: kind.into(),
            category: category.into(),
            level: level.into(),
            stone: None,
            offering: None,
            summary: summary.into(),
            data: None,
        }
    }

    pub fn with_stone(mut self, stone: impl Into<String>) -> Self {
        self.stone = Some(stone.into());
        self
    }

    pub fn with_offering(mut self, offering: impl Into<String>) -> Self {
        self.offering = Some(offering.into());
        self
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    /// The envelope survives a JSON round-trip with its sections intact.
    #[test]
    fn envelope_round_trips() {
        let e = PulseEvent::new(
            "topology.goodbye",
            "topology",
            LEVEL_INFO,
            "tranquil-pass said goodbye - removed from the room",
        )
        .with_stone("tranquil-pass");
        let bytes = serde_json::to_vec(&e).unwrap();
        let back: PulseEvent = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back, e);
        assert_eq!(back.stone.as_deref(), Some("tranquil-pass"));
    }
}
