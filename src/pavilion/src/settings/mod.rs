//! User-facing settings — quiet hours, per-source suppression,
//! autostart toggle.
//!
//! Layout mirrors [`crate::tending`]: typed wire types in `types`,
//! file persistence in `store`. The module is deliberately small —
//! the policy seams that consume settings live in their own modules
//! ([`crate::announce::policy`]) and just hold an
//! `Arc<SettingsStore>`.
//!
//! See [PAVILION-0002 §"Promote Settings to an M0.5 blocker"](
//! ../../../../docs/decisions/PAVILION-0002-revised-milestone-shape.md).

pub mod store;
pub mod types;

pub use store::SettingsStore;
pub use types::{Settings, SettingsPatch};
