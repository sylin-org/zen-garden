//! Facilitators (PAVILION-0002 §"Treat Facilitators as a sibling of
//! the Announcer").
//!
//! ```text
//! SuggestionSource ──► FacilitatorEngine ──► current Suggestion (or None)
//!                      (policy)                    ▼
//!                                            inline banner
//!                                            on the relevant view
//! ```
//!
//! Same shape as [`crate::announce`]: producers feed an engine
//! whose job is policy (dedup, dismissal, priority), and the
//! output is consumed by the UI.
//!
//! ## What a facilitator looks like
//!
//! Per the interaction-design spec §5: a tentative-voiced banner
//! offering one constructive action, dismissable two ways:
//!
//! - **Not now** — session-local cooldown, the suggestion may
//!   return.
//! - **Hide this kind** — persistent, stored in
//!   [`crate::settings::Settings::suppressed_kinds`] alongside
//!   announcer toast suppressions. The kind string is the
//!   discriminator (`"facilitator:tend_a_stone"`, etc.).
//!
//! Dismissals stack: a kind-level suppression always wins over a
//! pending session "Not now."
//!
//! ## v0 sources
//!
//! Two, both reading awareness + tending state directly:
//!
//! 1. `tend_a_stone` — stones in awareness but none tended.
//!    Picks the first localhost-or-wired stone as the suggested
//!    target.
//! 2. `enable_pond` — at least 2 stones aware and the tended
//!    stone reports no pond. Action: open the Pond destination.
//!
//! More sources land in M2; they plug in the same way.

pub mod engine;
pub mod source;
pub mod types;

pub use engine::FacilitatorEngine;
pub use types::Suggestion;
