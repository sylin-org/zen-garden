//! Garden Adapter SDK
//!
//! A framework for building Zen Garden adapters that connect to Moss.
//!
//! # Overview
//!
//! Adapters are standalone executables that:
//! - Receive commands from Moss via HTTP (`POST /command`)
//! - Optionally subscribe to presence events via SSE
//! - Support graceful shutdown via `POST /shutdown`
//! - Provide health checks via `GET /health`
//!
//! # Quick Start
//!
//! ```ignore
//! use garden_adapter_sdk::prelude::*;
//! use garden_common::command_manifest::CommandResponse;
//!
//! struct MyHandler;
//!
//! #[async_trait]
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
//!     let config = AdapterConfig::from_cli()?;
//!     
//!     AdapterRuntime::new(config, "my-adapter")
//!         .command_handler(MyHandler)
//!         .run()
//!         .await
//! }
//! ```
//!
//! # Modules
//!
//! - [`server`] - HTTP server with standard adapter endpoints
//! - [`sse`] - SSE client for presence event subscription
//! - [`runtime`] - Main loop and shutdown coordination
//! - [`cli`] - Standard CLI argument parsing
//! - [`handler`] - Command handler trait
//! - [`state`] - Adapter state management (on/off, persistence)

pub mod cli;
pub mod dependencies;
pub mod handler;
pub mod runtime;
pub mod server;
pub mod sse;
pub mod state;

/// Prelude for convenient imports
pub mod prelude {
    pub use crate::cli::AdapterConfig;
    pub use crate::dependencies::{ensure_dependencies, SystemDependency, DependencyCheckResult};
    pub use crate::handler::CommandHandler;
    pub use crate::runtime::AdapterRuntime;
    pub use crate::sse::{EventHandler, SseClient, SseEvent};
    pub use crate::state::AdapterState;
    pub use garden_common::command_manifest::CommandResponse;
    pub use anyhow::Result;
    pub use async_trait::async_trait;
}

// Re-export commonly used items at crate root
pub use cli::AdapterConfig;
pub use handler::CommandHandler;
pub use runtime::AdapterRuntime;
pub use sse::{EventHandler, SseClient, SseEvent};
pub use state::AdapterState;

// Re-export async_trait for implementors
pub use async_trait::async_trait;

// Re-export from garden_common for convenience
pub use garden_common::command_manifest::{
    check_dump_commands, CommandArg, CommandDef, CommandManifest, CommandResponse,
};
