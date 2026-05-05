//! `announce` — SSE observers, policy layer, activity store, and
//! toast dispatcher.
//!
//! Wiring (see `app.rs`):
//!
//! ```text
//! Awareness         ──┐
//! SSE storage stream──┼──► Announcer ──┬──► ActivityStore ──► get_activity command
//!                     │  (policy)      │
//!                     │                └──► ToastDispatcher (when promoted)
//! ```
//!
//! See the [interaction-design spec §6 + §10](../../../../docs/specs/pavilion-interaction-design.md)
//! for the toast policy this layer implements (calm by default,
//! present when needed; coalesce on jitter; respect quiet hours —
//! the last is deferred until a Settings store exists).

pub mod event;
pub mod observer;
pub mod policy;
pub mod store;
pub mod toast;

pub use event::{ActivityEntry, GardenEvent};
pub use policy::Announcer;
pub use store::ActivityStore;
