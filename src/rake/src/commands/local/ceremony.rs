//! Ceremony command - guided workflow placeholders
//!
//! Future workflows for common operations.

use crate::command_manifest::cmd;
use crate::commands::{Command, CommandResult};
use crate::context::Runtime;
use crate::suggestions;
use async_trait::async_trait;

/// Ceremony command - guided workflows (placeholder)
pub struct CeremonyCommand {
    pub name: Option<String>,
    pub quiet: bool,
}

impl CeremonyCommand {
    pub fn new(name: Option<String>, quiet: bool) -> Self {
        Self { name, quiet }
    }
}

#[async_trait]
impl Command for CeremonyCommand {
    async fn execute(&self, _ctx: &Runtime) -> CommandResult {
        println!();
        println!("  ⏳ Ceremony workflows are not yet implemented.");
        println!();
        if let Some(ceremony_name) = &self.name {
            println!("  Requested ceremony: {}", ceremony_name);
        }
        println!("  Future ceremonies may include:");
        println!("    • ceremony bootstrap    - First-time setup wizard");
        println!("    • ceremony migrate      - Service migration workflow");
        println!("    • ceremony backup       - Guided backup configuration");

        // Self-teaching suggestions
        suggestions::print_suggestions(cmd::CEREMONY, self.quiet);

        Ok(())
    }

    fn requires_endpoint(&self) -> bool {
        false
    }

    fn show_stone_header(&self) -> bool {
        false
    }

    fn name(&self) -> &'static str {
        cmd::CEREMONY
    }
}
