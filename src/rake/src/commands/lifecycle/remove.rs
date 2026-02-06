//! Remove command - remove service and container
//!
//! Removes a service from management and stops/removes the container.
//! Volumes are preserved by default for data recovery.

use crate::command_manifest::cmd;
use crate::commands::{Command, CommandResult};
use crate::context::CommandContext;
use crate::suggestions;
use garden_common::ui::rendering as ui;
use async_trait::async_trait;
use std::io::Write;

/// Remove a service (stops and removes container, preserves volumes)
pub struct RemoveCommand {
    pub service: String,
    pub force: bool,
    pub quiet_mode: bool,
}

impl RemoveCommand {
    pub fn new(service: String, force: bool, quiet_mode: bool) -> Self {
        Self {
            service,
            force,
            quiet_mode,
        }
    }
}

#[async_trait]
impl Command for RemoveCommand {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        let service_path = urlencoding::encode(&self.service);
        // First check if the service exists before prompting
        let check_url = ctx.api_v1_url(&format!("stone/services/{}", service_path))?;
        let check_response = ctx.client.get(&check_url).send().await?;

        if check_response.status() == reqwest::StatusCode::NOT_FOUND {
            eprintln!(
                "{}{} Service '{}' not found on this stone",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                self.service
            );
            suggestions::print_suggestions(cmd::REMOVE, self.quiet_mode);
            return Ok(());
        }

        // Confirmation prompt (unless --force or quiet mode)
        if !self.force && !self.quiet_mode {
            println!(
                "{}⚠️  This will remove service '{}' and stop its container",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                self.service
            );
            println!(
                "{}Volumes will be preserved for data recovery.",
                " ".repeat(ui::constants::DEFAULT_INDENT)
            );
            println!(
                "{}Use 'uproot' to completely destroy including volumes.",
                " ".repeat(ui::constants::DEFAULT_INDENT)
            );
            print!(
                "{}Continue? [y/N]: ",
                " ".repeat(ui::constants::DEFAULT_INDENT)
            );
            std::io::stdout().flush()?;

            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;

            if !input.trim().eq_ignore_ascii_case("y") {
                println!("{}Cancelled", " ".repeat(ui::constants::DEFAULT_INDENT));
                return Ok(());
            }
            println!();
        }

        let url = ctx.api_v1_url(&format!("stone/services/{}", service_path))?;
        let response = ctx.client.delete(&url).send().await?;
        let status = response.status();

        match status {
            s if s.is_success() => {
                if let Ok(body) = response.json::<serde_json::Value>().await {
                    let message = body.get("message").and_then(|v| v.as_str()).unwrap_or("");

                    println!(
                        "{}{} Removed {}",
                        " ".repeat(ui::constants::DEFAULT_INDENT),
                        ui::status_indicator("ok", ctx.term.supports_color),
                        self.service
                    );
                    if !message.is_empty() {
                        println!(
                            "{}   {}",
                            " ".repeat(ui::constants::DEFAULT_INDENT),
                            message
                        );
                    }

                    // Display suggestions if present and not in quiet mode
                    if !self.quiet_mode {
                        if let Some(suggestions) = body.get("suggestions").and_then(|v| v.as_array())
                        {
                            if !suggestions.is_empty() {
                                println!("\nSuggestions:");
                                for suggestion in suggestions {
                                    if let Some(s) = suggestion.as_str() {
                                        println!("  • {}", s);
                                    }
                                }
                            }
                        }
                    }
                } else {
                    println!(
                        "{}{} Removed {}",
                        " ".repeat(ui::constants::DEFAULT_INDENT),
                        ui::status_indicator("ok", ctx.term.supports_color),
                        self.service
                    );
                }
            }
            reqwest::StatusCode::NOT_FOUND => {
                eprintln!(
                    "{}{} Service '{}' not found",
                    " ".repeat(ui::constants::DEFAULT_INDENT),
                    ui::status_indicator("error", ctx.term.supports_color),
                    self.service
                );
            }
            _ => {
                eprintln!(
                    "{}{} Failed: {}",
                    " ".repeat(ui::constants::DEFAULT_INDENT),
                    ui::status_indicator("error", ctx.term.supports_color),
                    status
                );
            }
        }

        // Self-teaching suggestions
        suggestions::print_suggestions(cmd::REMOVE, self.quiet_mode);

        Ok(())
    }

    fn name(&self) -> &'static str {
        cmd::REMOVE
    }
}
