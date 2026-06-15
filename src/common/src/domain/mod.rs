//! Shared domain vocabulary.
//!
//! The curated set of typed values every part of Zen Garden — moss, the
//! companion SDK, cricket, firefly, and future companions — speaks in.
//! Introduced by [COMPANION-0005] as Book IV of the Companion Integration
//! Platform epic ([COMPANION-0001]).
//!
//! # What lives here
//!
//! - [`Health`] — five-valued vitality enum (`Thriving`, `NeedsAttention`,
//!   `Withering`, `Wilting`, `Dormant`). Serde-compatible with the existing
//!   wire strings defined by the `VITALITY_*` constants.
//! - [`Load`] — cohesive snapshot of a stone's resource load (CPU / memory
//!   / disk / I/O / GPU / network). Built from [`Percent`], a clamped `0..=100`
//!   value type.
//! - [`SeedBank`] — managed storage summary with derived `free_gb` and
//!   `fill_percent` helpers.
//! - [`Pond`] — stone's membership state (Solo / Member / Cornerstone).
//! - Re-exports of [`Stone`] and [`Offering`] (already canonical elsewhere
//!   in `garden-common`) so all domain imports come from one module path.
//!
//! # What does **not** live here
//!
//! - Wire types — remain in `garden-common::presence::types`.
//! - Moss-internal aggregates — stay in `src/moss/src/domain/`.
//! - Behaviour (methods that mutate domain state) — Book IV is *vocabulary*,
//!   not behaviour.
//!
//! # Conventions
//!
//! Every value in this module is `Clone`, `Debug`, `Serialize`,
//! `Deserialize`. Most are `Copy` when small enough. Wire compatibility is
//! preserved by matching serde string representations; round-tripping
//! through JSON with existing moss payloads must be lossless.
//!
//! [COMPANION-0005]: https://github.com/zen-garden/zen-garden/blob/dev/docs/decisions/COMPANION-0005-domain-types.md
//! [COMPANION-0001]: https://github.com/zen-garden/zen-garden/blob/dev/docs/decisions/COMPANION-0001-companion-integration-epic.md

pub mod health;
pub mod load;
pub mod pond;
pub mod seed_bank;

pub use health::Health;
pub use load::{Load, Percent};
pub use pond::Pond;
pub use seed_bank::SeedBank;

// Re-exports of canonical types already defined elsewhere in garden-common.
// One import path — `use garden_common::domain::*` — for all domain values.
//
// Note: `Offering` is a moss-internal aggregate; only its identity type
// (`OfferingFqn`) is shared here. `StoneStatus` lives in the discovery
// types module. Future books may promote more of moss's domain types
// into `garden-common::domain` as sharing needs emerge.
pub use crate::offerings::OfferingFqn;
pub use crate::types::discovery::StoneStatus;
