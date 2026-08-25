//! Lifecycle commands
//!
//! Commands for managing service lifecycle:
//! - offer - Install/list offerings
//! - rest - Stop a service
//! - wake - Start a service
//! - remove - Remove service and container (preserves volumes)
//! - uproot - Destroy service completely (including volumes)
//! - upgrade/nourish - Update a service

pub mod remove;
pub mod rest;
pub mod upgrade;
pub mod uproot;
pub mod wake;

pub use remove::RemoveCommand;
pub use rest::RestCommand;
pub use upgrade::UpgradeCommand;
pub use uproot::UprootCommand;
pub use wake::WakeCommand;
