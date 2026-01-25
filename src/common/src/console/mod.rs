//! Console output module
//! 
//! Provides structured console events with multiple output modes (Silent, Minimal, Informative, Verbose).
//! Supports remote console control via API and graceful deduplication of high-frequency events.
//!
//! Also provides TTY functions for first-boot output.

use std::io::IsTerminal;

pub mod modes;
pub mod events;
pub mod formatters;
pub mod printer;
pub mod tty;

pub use modes::ConsoleMode;
pub use events::{EventCategory, EventStatus, ConsoleEvent, FormatHint, AnsiColor, Severity};
pub use formatters::{OutputFormatter, TtyFormatter, SseFormatter};
pub use printer::{ConsolePrinter, EventDeduplicator};
pub use tty::*;

/// Detect platform-appropriate console mode default
pub fn detect_platform_console_mode() -> ConsoleMode {
    // Windows service detection
    #[cfg(target_os = "windows")]
    {
        if std::env::var("USERDOMAIN").is_ok() && !std::io::stdin().is_terminal() {
            return ConsoleMode::Silent; // Windows service
        }
        if std::io::stdin().is_terminal() {
            return ConsoleMode::Informative; // Windows interactive
        }
    }
    
    // Linux systemd/interactive detection
    #[cfg(not(target_os = "windows"))]
    {
        // Check for systemd without TTY
        if std::env::var("INVOCATION_ID").is_ok() && !std::io::stdin().is_terminal() {
            return ConsoleMode::Minimal; // systemd daemon
        }
        
        if std::io::stdin().is_terminal() {
            return ConsoleMode::Informative; // Interactive terminal
        }
    }
    
    ConsoleMode::Minimal // Safe default
}
