//! Launch command - open stone UI in browser
//!
//! Opens the stone's web interface in the default web browser.
//! Works on Windows, macOS, and Linux with graphical environment.

use crate::command_manifest::cmd;
use crate::commands::{Command, CommandResult};
use crate::context::CommandContext;
use async_trait::async_trait;

/// Launch stone UI in browser
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
            anyhow::bail!(
                "No stone endpoint available. Use --at to specify a stone or tend a stone first."
            );
        };

        // Construct the URL (base endpoint, portrait is the default page)
        let url = endpoint.trim_end_matches('/').to_string();

        // Open in default browser
        match open::that(&url) {
            Ok(()) => {
                println!("Opening {} in browser...", url);
                Ok(())
            }
            Err(e) => {
                anyhow::bail!("Failed to open browser: {}. URL: {}", e, url);
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
