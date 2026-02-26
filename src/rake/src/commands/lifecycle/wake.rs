//! Wake command - start a stopped service
//!
//! Sends REST request to start a stopped service.

use crate::command_manifest::cmd;
use crate::commands::{Command, CommandResult};
use crate::context::CommandContext;
use crate::suggestions;
use async_trait::async_trait;
use garden_common::ui::rendering as ui;

/// Start (wake) a stopped service
pub struct WakeCommand {
    pub service: String,
    pub quiet_mode: bool,
}

impl WakeCommand {
    pub fn new(service: String, quiet_mode: bool) -> Self {
        Self {
            service,
            quiet_mode,
        }
    }
}

#[async_trait]
impl Command for WakeCommand {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        let service_path = urlencoding::encode(&self.service);
        let url = ctx.api_v1_url(&format!("stone/services/{}/wake", service_path))?;
        let response = ctx.client.post(&url).send().await?;
        let status = response.status();

        match status {
            s if s.is_success() => {
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
                    if !self.quiet_mode {
                        if let Some(suggestions) =
                            body.get("suggestions").and_then(|v| v.as_array())
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
                        "{}{} Started {}",
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
                    .and_then(|body| {
                        crate::api::responses::extract_error_message(&body)
                    });
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
        suggestions::print_suggestions(cmd::WAKE, self.quiet_mode);

        Ok(())
    }

    fn name(&self) -> &'static str {
        cmd::WAKE
    }
}
