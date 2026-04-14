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

pub mod command_transport;
pub mod core_payloads;
pub mod event;
pub mod pulse;
pub mod sse_transport;
pub mod transport;

pub use core_payloads::{
    KIND_PRESENCE_SNAPSHOT, KIND_SERVICE_STARTED, KIND_SERVICE_STOPPED,
    KIND_STONE_HEALTH_CHANGED, KIND_STONE_LOAD_UPDATED, KIND_STONE_TENDED,
    KIND_STORAGE_CONNECTED, KIND_STORAGE_DETECTED, KIND_STORAGE_REMOVED,
    PresenceSnapshotExt, ServiceStartedPayload, ServiceStoppedPayload,
    StoneHealthChangedExt, StoneLoadUpdatedExt, StoneTendedPayload, StorageConnectedPayload,
    StorageDetectedPayload, StorageRemovedPayload, wire_to_core_kind,
};
pub use event::{
    DynPayload, Event, EventId, EventPayload, is_valid_kind, kind_namespace, new_event_id,
};
pub use pulse::{
    IngestResult, Pulse, PulseConfig, PulseMetricsSnapshot, RejectReason,
};
pub use command_transport::{
    CommandInvocation, CommandOutcome, CommandResult, CommandTransport, KIND_COMMAND_INVOCATION,
    KIND_COMMAND_RESULT,
};
pub use sse_transport::SseTransport;
pub use transport::{BoxFuture, Transport};
