//! Local commands for garden-rake
//!
//! Commands that don't require a stone endpoint:
//! - ceremony: Guided workflow placeholders
//! - browse: Browse command manifest
//! - template: Browse service templates
//! - launch: Open stone portrait in browser

pub mod browse;
pub mod ceremony;
pub mod launch;
pub mod template;

pub use browse::BrowseCommand;
pub use ceremony::CeremonyCommand;
pub use launch::LaunchCommand;
pub use template::{TemplateAction, TemplateCommand};
