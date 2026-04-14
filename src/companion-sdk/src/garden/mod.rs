//! Garden bounded context.
//!
//! Owns event ingestion, canonicalization, projection, and fan-out. Under
//! [COMPANION-0001] this is one of two bounded contexts inside the SDK (the
//! other is [Adapters]). Adapters subscribe to Garden's event stream and
//! query Garden's read-model; Garden never knows which adapters exist.
//!
//! Book I lands [`event`] — the uniform event envelope. Subsequent books
//! add sibling modules (`pulse`, `transport`, `garden` aggregate, core
//! payload types).
//!
//! [COMPANION-0001]: https://github.com/zen-garden/zen-garden/blob/dev/docs/decisions/COMPANION-0001-companion-integration-epic.md
//! [Adapters]: super

pub mod event;

pub use event::{
    Event, EventId, EventPayload, is_valid_kind, kind_namespace, new_event_id,
};
