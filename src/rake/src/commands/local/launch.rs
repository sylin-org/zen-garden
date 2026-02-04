//! Launch command - open stone portrait in browser
//!
//! Opens the stone's portrait page in the default web browser.
//! Works on Windows, macOS, and Linux with graphical environment.

use crate::command_manifest::cmd;
use crate::commands::{Command, CommandResult};
use crate::context::CommandContext;
use async_trait::async_trait;

/// Launch stone portrait in browser
pub struct LaunchCommand {
    /// Optional stone endpoint override
    pub endpoint: Option<String>,
}

impl LaunchCommand {
    pub fn new(endpoint: Option<String>) -> Self {
        Self { endpoint }
    }
}

#[async_trait]
impl Command for LaunchCommand {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        // Determine the endpoint to use
        let endpoint = if let Some(ref ep) = self.endpoint {
            ep.clone()
        } else if let Some(ref ep) = ctx.endpoint {
            ep.clone()
        } else {
            // No endpoint available - we need discovery
            anyhow::bail!("No stone endpoint available. Use --at to specify a stone or tend a stone first.");
        };

        // Construct the portrait URL
        let portrait_url = format!("{}/portrait", endpoint.trim_end_matches('/'));

        // Open in default browser
        match open::that(&portrait_url) {
            Ok(()) => {
                println!("Opening {} in browser...", portrait_url);
                Ok(())
            }
            Err(e) => {
                anyhow::bail!("Failed to open browser: {}. URL: {}", e, portrait_url);
            }
        }
    }

    fn requires_endpoint(&self) -> bool {
        // We need an endpoint, but we handle it ourselves to provide better error messages
        true
    }

    fn show_stone_header(&self) -> bool {
        false
    }

    fn name(&self) -> &'static str {
        cmd::LAUNCH
    }
}
