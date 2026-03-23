//! Zen Rake Library
//!
//! CLI tool for service discovery and management.
//!
//! Architecture:
//! - commands/  - Command handlers by category
//! - api/       - HTTP client and response parsing
//! - ui/        - Terminal output and formatting
//! - discovery/ - Stone discovery (UDP, mDNS)

pub mod api;
pub mod arg_spec;
pub mod cli_build;
pub mod client;
pub mod command_manifest;
pub mod commands;
pub mod context;
pub mod discovery;
pub mod enrollment;
pub mod stone_bag;
pub mod stone_cache;
pub mod suggestions;
pub mod tending;
pub mod ui;

// Re-exports for convenience
pub use client::{resolve_target_endpoint, CachedStoneInfo, CachedStoneOps};
pub use context::Runtime;
pub use ui::layout::{IndentLevel, Layout};
pub use ui::rendering::{OutputWriter, TerminalInfo};
