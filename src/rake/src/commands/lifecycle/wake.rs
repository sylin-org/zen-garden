//! Wake command - start a stopped service
//!
//! Sends REST request to start a stopped service.

use crate::command_manifest::cmd;
use crate::commands::{Command, CommandResult};
use crate::context::Context;
use crate::suggestions;
use crate::ui::rendering as ui;

/// Start (wake) a stopped service
pub struct WakeCommand {
    pub service: String,
    pub quiet: bool,
}

impl WakeCommand {
    pub fn new(service: String, quiet: bool) -> Self {
        Self { service, quiet }
    }
}

impl Command for WakeCommand {
    fn execute<'a>(&'a self, ctx: &'a Context) -> std::pin::Pin<Box<dyn std::future::Future<Output = CommandResult> + Send + 'a>> {
        Box::pin(async move {
            let api = ctx.api();
            let result = api.services().wake(&self.service).await;

            match result {
                Ok(response) => {
                    if let Ok(body) = response.json::<serde_json::Value>().await {
                        let message = body.get("message").and_then(|v| v.as_str()).unwrap_or("");
                        let api_status = body
                            .get("status")
                            .and_then(|v| v.as_str())
                            .unwrap_or("running");

                        println!(
                            "{}{} Started {} ({})",
                            " ".repeat(ui::constants::DEFAULT_INDENT),
                            ui::status_indicator("ok", ctx.term.supports_color),
                            self.service,
                            api_status
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
                            "{}{} Started {}",
                            " ".repeat(ui::constants::DEFAULT_INDENT),
                            ui::status_indicator("ok", ctx.term.supports_color),
                            self.service
                        );
                    }
                }
                Err(ref e) if e.is_not_found() => {
                    eprintln!(
                        "{}{} Service '{}' not found",
                        " ".repeat(ui::constants::DEFAULT_INDENT),
                        ui::status_indicator("error", ctx.term.supports_color),
                        self.service
                    );
                }
                Err(e) => {
                    eprintln!(
                        "{}{} Failed: {}",
                        " ".repeat(ui::constants::DEFAULT_INDENT),
                        ui::status_indicator("error", ctx.term.supports_color),
                        e.display_message()
                    );
                }
            }

            // Self-teaching suggestions
            suggestions::print_suggestions(cmd::WAKE, self.quiet);

            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        cmd::WAKE
    }
}
