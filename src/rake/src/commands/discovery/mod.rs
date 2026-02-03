//! Discovery commands
//!
//! Commands for discovering and observing stones and services:
//! - observe - Garden-wide overview
//! - watch - Real-time event streaming
//! - list - List services on a stone
//! - status - Detailed stone status
//! - adopted - List adopted services
//! - borrowed - List borrowed services
//! - find - Service discovery with connection strings
//! - config - Service configuration for automation
//! - capabilities - List offering capabilities (models, extensions, etc.)

pub mod adopted;
pub mod borrowed;
pub mod capabilities;
pub mod config;
pub mod find;
pub mod list;
pub mod observe;
pub mod status;
pub mod watch;

pub use adopted::AdoptedCommand;
pub use borrowed::BorrowedCommand;
pub use capabilities::CapabilitiesCommand;
pub use config::ConfigCommand;
pub use find::{FindCommand, FindOutputFormat};
pub use list::ListCommand;
pub use observe::ObserveCommand;
pub use status::StatusCommand;
pub use watch::{WatchCommand, WatchTargetType};
