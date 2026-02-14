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
//! - [`server`] - HTTP server with standard Companion endpoints
//! - [`sse`] - SSE client for presence event subscription
//! - [`runtime`] - Main loop and shutdown coordination
//! - [`cli`] - Standard CLI argument parsing
//! - [`handler`] - Command handler trait
//! - [`state`] - Companion state management (on/off, persistence)

pub mod cli;
pub mod dependencies;
pub mod handler;
pub mod runtime;
pub mod server;
pub mod sse;
pub mod state;

/// Prelude for convenient imports
pub mod prelude {
    pub use crate::cli::CompanionConfig;
    pub use crate::dependencies::{ensure_dependencies, DependencyCheckResult, SystemDependency};
    pub use crate::handler::CommandHandler;
    pub use crate::runtime::CompanionRuntime;
    pub use crate::sse::{EventHandler, SseClient, SseEvent};
    pub use crate::state::CompanionState;
    pub use anyhow::Result;
    pub use async_trait::async_trait;
    pub use garden_common::command_manifest::CommandResponse;
}

// Re-export commonly used items at crate root
pub use cli::CompanionConfig;
pub use handler::CommandHandler;
pub use runtime::CompanionRuntime;
pub use sse::{EventHandler, SseClient, SseEvent};
pub use state::CompanionState;

// Re-export async_trait for implementors
pub use async_trait::async_trait;

// Re-export from garden_common for convenience
pub use garden_common::command_manifest::{
    check_dump_commands, CommandArg, CommandDef, CommandManifest, CommandResponse,
};
