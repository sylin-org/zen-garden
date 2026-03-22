//! Rest command - stop a service
//!
//! Sends REST request to stop a running service.

use crate::command_manifest::cmd;
use crate::commands::{Command, CommandResult};
use crate::context::Runtime;
use crate::suggestions;
use crate::ui::rendering as ui;

/// Stop (rest) a service
pub struct RestCommand {
    pub service: String,
    pub quiet: bool,
}

impl RestCommand {
    pub fn new(service: String, quiet: bool) -> Self {
        Self { service, quiet }
    }
}

impl Command for RestCommand {
    fn execute<'a>(&'a self, ctx: &'a Runtime) -> std::pin::Pin<Box<dyn std::future::Future<Output = CommandResult> + Send + 'a>> {
        Box::pin(async move {
            let service_path = urlencoding::encode(&self.service);
            let url = ctx.api_v1_url(&format!("stone/services/{}/rest", service_path))?;
            let response = ctx.client.post(&url).send().await?;
            let status = response.status();

            match status {
                s if s.is_success() => {
                    if let Ok(body) = response.json::<serde_json::Value>().await {
                        let message = body.get("message").and_then(|v| v.as_str()).unwrap_or("");
                        let api_status = body
                            .get("status")
                            .and_then(|v| v.as_str())
                            .unwrap_or("stopped");

                        println!(
                            "{}{} Stopped {} ({})",
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
                            "{}{} Stopped {}",
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
                    let detail = response
                        .json::<serde_json::Value>()
                        .await
                        .ok()
                        .and_then(|body| crate::api::responses::extract_error_message(&body));
                    if let Some(msg) = detail {
                        eprintln!(
                            "{}{} Failed: {} - {}",
                            " ".repeat(ui::constants::DEFAULT_INDENT),
                            ui::status_indicator("error", ctx.term.supports_color),
                            status,
                            msg
                        );
                    } else {
                        eprintln!(
                            "{}{} Failed: {}",
                            " ".repeat(ui::constants::DEFAULT_INDENT),
                            ui::status_indicator("error", ctx.term.supports_color),
                            status
                        );
                    }
                }
            }

            // Self-teaching suggestions
            suggestions::print_suggestions(cmd::REST, self.quiet);

            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        cmd::REST
    }
}
