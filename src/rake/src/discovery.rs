//! Backward-compatibility re-export shim.
//!
//! Stone discovery moved to the `garden-discovery` workspace crate per
//! [DISC-0001](../../../docs/decisions/DISC-0001-discovery-as-first-class-crate.md).
//! Rake call sites continue to use `crate::discovery::*` — they resolve
//! through this re-export. Future cleanup may update those sites to
//! import directly from `garden_discovery`.

pub use garden_discovery::*;
