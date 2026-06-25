//! Remove command - remove service and container
//!
//! Removes a service from management and stops/removes the container.
//! Volumes are preserved by default for data recovery.

use crate::command_manifest::cmd;
use crate::commands::{Command, CommandResult};
use crate::context::Context;
use crate::suggestions;
use crate::ui::rendering as ui;
use std::io::Write;

/// Remove a service (stops and removes container, preserves volumes)
pub struct RemoveCommand {
    pub service: String,
    pub force: bool,
    pub quiet: bool,
}

impl RemoveCommand {
    pub fn new(service: String, force: bool, quiet: bool) -> Self {
        Self {
            service,
            force,
            quiet,
        }
    }
}

impl Command for RemoveCommand {
    fn execute<'a>(&'a self, ctx: &'a Context) -> std::pin::Pin<Box<dyn std::future::Future<Output = CommandResult> + Send + 'a>> {
        Box::pin(async move {
            let service_path = urlencoding::encode(&self.service);
            // First check if the service exists before prompting
            let check_url = ctx.api_v1_url(&format!("stone/services/{}", service_path));
            let check_response = ctx.client.get(&check_url).send().await?;

            if check_response.status() == reqwest::StatusCode::NOT_FOUND {
                eprintln!(
                    "{}{} Service '{}' not found on this stone",
                    " ".repeat(ui::constants::DEFAULT_INDENT),
                    ui::status_indicator("error", ctx.term.supports_color),
                    self.service
                );
                suggestions::print_suggestions(cmd::REMOVE, self.quiet);
                return Ok(());
            }

            // Confirmation prompt (unless --force or quiet mode)
            if !self.force && !self.quiet {
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

            // Signed via the local Moss oracle (Stage 4) when the target name is
            // known; raw response so the bespoke status handling below is preserved.
            let response = ctx
                .api()
                .send_signed_raw(
                    reqwest::Method::DELETE,
                    &format!("/api/v1/stone/services/{}", service_path),
                    None,
                )
                .await?;
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
                        if !self.quiet
                            && let Some(suggestions) =
                                body.get("suggestions").and_then(|v| v.as_array())
                                && !suggestions.is_empty() {
                                    println!("\nSuggestions:");
                                    for suggestion in suggestions {
                                        if let Some(s) = suggestion.as_str() {
                                            println!("  • {}", s);
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
            suggestions::print_suggestions(cmd::REMOVE, self.quiet);

            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        cmd::REMOVE
    }
}
