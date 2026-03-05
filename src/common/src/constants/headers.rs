//! Common HTTP header names
//!
//! ## Request headers (client → stone)
//!
//! - [`HEADER_SEED_BANK`] — optional seed bank selector
//! - [`HEADER_REQUESTING_STONE_ID`] / [`HEADER_REQUESTING_STONE_NAME`] — audit metadata
//!
//! ## Response headers (stone → client)
//!
//! - [`HEADER_STONE_ID`] / [`HEADER_STONE_NAME`] — stone identity piggy-backed
//!   on every response so clients never need a dedicated call

/// Optional seed bank selector header
pub const HEADER_SEED_BANK: &str = "x-seed-bank";

/// Requesting stone ID header (used for audit metadata)
pub const HEADER_REQUESTING_STONE_ID: &str = "x-requesting-stone-id";

/// Requesting stone name header (used for audit metadata)
pub const HEADER_REQUESTING_STONE_NAME: &str = "x-requesting-stone-name";

// ── Response headers ────────────────────────────────────────────────────

/// Stone identity: unique GUID v7 (response header, every response)
pub const HEADER_STONE_ID: &str = "x-stone-id";

/// Stone identity: human-readable name (response header, every response)
pub const HEADER_STONE_NAME: &str = "x-stone-name";
