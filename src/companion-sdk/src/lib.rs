//! Garden Companion SDK.
//!
//! A framework for building Zen Garden Companions that connect to Moss.
//! See [COMPANION-0001](https://github.com/zen-garden/zen-garden/blob/dev/docs/decisions/COMPANION-0001-companion-integration-epic.md)
//! for the architecture.
//!
//! # Quick start
//!
//! ```ignore
//! use garden_companion_sdk::prelude::*;
//! use garden_companion_sdk::garden::{CommandTransport, SseTransport};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     init_tracing();
//!
//!     Companion::new("my-companion")
//!         .with_transport(SseTransport::new("http://stone:7185"))
//!         .with_transport(CommandTransport::new(7190))
//!         .with_adapter_factory(MyFactory::default())
//!         .run()
//!         .await
//! }
//! ```
//!
//! # Modules
//!
//! - [`garden`] — event envelope, Pulse, Garden aggregate, transports.
//! - [`adapters`] — [`Adapter`] / [`AdapterFactory`] / [`Adapters`] supervisor.
//! - [`companion`] — top-level runtime wiring transports + adapters + pulse.
//! - [`cli`] — standard `--dump-commands` parsing for moss's companion registry.
//! - [`state`] — persisted on/off flag (carry-over from legacy companions).
//! - [`testing`] — `MockTransport`, `RecordingAdapter`, `FakeFactory`, `TestHarness`.
//!
//! [`Adapter`]: adapters::Adapter
//! [`AdapterFactory`]: adapters::AdapterFactory
//! [`Adapters`]: adapters::Adapters

pub mod adapters;
pub mod cli;
pub mod companion;
pub mod dependencies;
pub mod garden;
pub mod state;
pub mod testing;

/// Initialize tracing with the standard companion configuration:
/// env-filter-driven, info-level default, plaintext format. Call
/// this as the first line of `main`.
pub fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
}

/// Prelude for convenient imports.
pub mod prelude {
    pub use crate::adapters::{
        Adapter, AdapterFactory, AdapterInfo, AdapterProfile, AdapterStatus, Adapters,
        DeliveryPolicy,
    };
    pub use crate::cli::CompanionConfig;
    pub use crate::companion::Companion;
    pub use crate::dependencies::{ensure_dependencies, DependencyCheckResult, SystemDependency};
    pub use crate::garden::{
        BoxFuture, CommandInvocation, CommandOutcome, CommandResult, CommandTransport, DynPayload,
        Event, EventId, EventPayload, Garden, GardenSnapshot, GardenState, GardenSubscription,
        IngestResult, Pulse, PulseConfig, PulseMetricsSnapshot, RejectReason,
        ServiceStartedPayload, ServiceStoppedPayload, SseTransport, StoneTendedPayload,
        StorageConnectedPayload, StorageDetectedPayload, StorageRemovedPayload, Transport,
        is_valid_kind, kind_namespace, new_event_id, wire_to_core_kind,
    };
    pub use crate::state::CompanionState;
    pub use anyhow::Result;
    pub use garden_common::command_manifest::CommandResponse;
}

// Re-export commonly used items at crate root.
pub use cli::CompanionConfig;
pub use state::CompanionState;

// Re-export from garden_common for convenience.
pub use garden_common::command_manifest::{
    check_dump_commands, CommandArg, CommandDef, CommandManifest, CommandResponse,
};
