//! The one wire truth (CHARTER B1, CODE-RULES R0.5/R1.7).
//!
//! Everything that crosses a process boundary — announcement envelopes,
//! chirp bodies, beacons — is defined here exactly once, with per-domain
//! constants, and pinned by fixture tests. v1 bodies are supersets of the
//! PoC's required fields: v0 stones parse v1 chirps (unknown fields are
//! ignored by serde), v1 stones parse v0 chirps. Wire compatibility is a
//! tested property, not a hope.

pub mod chirp;
pub mod consts;
pub mod discovery;
pub mod faces;
pub mod surface;
pub mod song;
pub mod wire;
