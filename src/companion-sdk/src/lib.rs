//! Garden Companion SDK
//!
//! A framework for building Zen Garden Companions that connect to Moss.
//!
//! # Overview
//!
//! Companions are standalone executables that:
//! - Receive commands from Moss via HTTP (`POST /command`)
//! - Optionally subscribe to presence events via SSE
//! - Support graceful shutdown via `POST /shutdown`
//! - Provide health checks via `GET /health`
//!
//! # Quick Start
//!
//! ```ignore
//! use garden_companion_sdk::prelude::*;
//! use garden_common::command_manifest::CommandResponse;
//!
//! struct MyHandler;
//!
//! impl CommandHandler for MyHandler {
//!     async fn handle(&self, args: &[String]) -> CommandResponse {
//!         match args.first().map(|s| s.as_str()) {
//!             Some("hello") => CommandResponse::success("Hello!"),
//!             Some(cmd) => CommandResponse::error(format!("Unknown: {}", cmd)),
//!             None => CommandResponse::error("No command"),
//!         }
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let config = CompanionConfig::from_cli()?;
//!     
//!     CompanionRuntime::new(config, "my-Companion")
//!         .command_handler(MyHandler)
//!         .run()
//!         .await
//! }
//! ```
//!
//! # Modules
//!
//! - [`garden`] - Garden bounded context (event envelope today; Pulse, Garden aggregate, and Transport in subsequent COMPANION-0001 books)
//! - [`server`] - HTTP server with standard Companion endpoints
//! - [`sse`] - SSE client for presence event subscription
//! - [`runtime`] - Main loop and shutdown coordination
//! - [`cli`] - Standard CLI argument parsing
//! - [`handler`] - Command handler trait
//! - [`state`] - Companion state management (on/off, persistence)

pub mod adapters;
pub mod cli;
pub mod companion;
pub mod dependencies;
pub mod garden;
pub mod handler;
pub mod runtime;
pub mod server;
pub mod sse;
pub mod state;
pub mod testing;

/// Prelude for convenient imports
pub mod prelude {
    pub use crate::cli::CompanionConfig;
    pub use crate::dependencies::{ensure_dependencies, DependencyCheckResult, SystemDependency};
    pub use crate::adapters::{
        Adapter, AdapterFactory, AdapterInfo, AdapterProfile, AdapterStatus, Adapters,
        DeliveryPolicy,
    };
    pub use crate::companion::Companion;
    pub use crate::garden::{
        BoxFuture, CommandInvocation, CommandOutcome, CommandResult, CommandTransport, DynPayload,
        Event, EventId, EventPayload, Garden, GardenSnapshot, GardenState, GardenSubscription,
        IngestResult, Pulse, PulseConfig, PulseMetricsSnapshot, RejectReason,
        ServiceStartedPayload, ServiceStoppedPayload, SseTransport, StoneTendedPayload,
        StorageConnectedPayload, StorageDetectedPayload, StorageRemovedPayload, Transport,
        is_valid_kind, kind_namespace, new_event_id, wire_to_core_kind,
    };
    pub use crate::handler::CommandHandler;
    pub use crate::runtime::CompanionRuntime;
    pub use crate::sse::{EventHandler, SseClient, SseEvent};
    pub use crate::state::CompanionState;
    pub use anyhow::Result;
    pub use garden_common::command_manifest::CommandResponse;
}

// Re-export commonly used items at crate root
pub use cli::CompanionConfig;
pub use handler::CommandHandler;
pub use runtime::CompanionRuntime;
pub use sse::{EventHandler, SseClient, SseEvent};
pub use state::CompanionState;

// Re-export from garden_common for convenience
pub use garden_common::command_manifest::{
    check_dump_commands, CommandArg, CommandDef, CommandManifest, CommandResponse,
};
