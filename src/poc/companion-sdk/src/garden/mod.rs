//! Event mesh — Pulse, Transports, Event envelope, and core payload
//! types.
//!
//! Originally also housed the client-side `Garden` aggregate; that was
//! retired in [COMPANION-0014]. Companions now query moss directly via
//! [`crate::moss_client::MossLocalClient`] for state and use this
//! module strictly for the live-event delta path.
//!
//! Module name is preserved to avoid touching every adapter import.
//!
//! [COMPANION-0014]: https://github.com/zen-garden/zen-garden/blob/dev/docs/decisions/COMPANION-0014-companions-query-moss-directly.md

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
