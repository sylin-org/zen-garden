//! Command implementations for garden-rake
//!
//! Commands implement the `Command` trait which provides:
//! - `execute()` - The command's business logic
//! - `requires_endpoint()` - Whether stone resolution is needed
//! - `show_stone_header()` - Whether to show stone banner

pub mod api;
pub mod ceremony_render;
pub mod help;
pub mod hey;

pub mod admin;
pub mod adoption;
pub mod discovery;
pub mod election;
pub mod lifecycle;
pub mod local;
pub mod management;
pub mod manifest;
pub mod nourish;
pub mod nurturing;
pub mod offering;
pub mod presence;
pub mod pulse;
pub mod storage;

use crate::context::Context;

/// Command execution result
pub type CommandResult = anyhow::Result<()>;

/// Trait for command handlers.
///
/// Connected commands receive a `Context` with a bound `Stone` --
/// `ctx.api()` and `ctx.endpoint()` are always available.
/// Local commands receive a `Context` without a stone.
pub trait Command: Send + Sync {
    /// Execute the command with the given context
    fn execute<'a>(
        &'a self,
        ctx: &'a Context,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = CommandResult> + Send + 'a>>;

    /// Whether this command requires a resolved stone endpoint
    ///
    /// Default: true (most commands need a stone)
    fn requires_endpoint(&self) -> bool {
        true
    }

    /// Whether to display the stone header banner
    ///
    /// Default: same as requires_endpoint()
    fn show_stone_header(&self) -> bool {
        self.requires_endpoint()
    }

    /// Command name for suggestions lookup
    fn name(&self) -> &'static str;
}

pub use help::{display_all_commands, display_command_detail};
