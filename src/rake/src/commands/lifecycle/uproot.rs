//! Uproot command - hard delete a service
//!
//! Permanently destroys a service and its container.

use crate::command_manifest::cmd;
use crate::commands::{Command, CommandResult};
use crate::context::Context;
use crate::suggestions;
use crate::ui::rendering as ui;
use std::io::Write;

/// Uproot a service (hard delete - destroy container completely)
pub struct UprootCommand {
    pub service: String,
    pub force: bool,
    pub quiet: bool,
}

impl UprootCommand {
    pub fn new(service: String, force: bool, quiet: bool) -> Self {
        Self {
            service,
            force,
            quiet,
        }
    }
}

impl Command for UprootCommand {
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
                suggestions::print_suggestions(cmd::UPROOT, self.quiet);
                return Ok(());
            }

            // Confirmation prompt (unless --force or quiet mode)
            if !self.force && !self.quiet {
                println!(
                    "{}⚠️  WARNING: This will PERMANENTLY DESTROY service '{}' and its container",
                    " ".repeat(ui::constants::DEFAULT_INDENT),
                    self.service
                );
                println!(
                    "{}This action cannot be undone. The container will be deleted with all volumes.",
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

            let url = ctx.api_v1_url(&format!("stone/services/{}/destroy", service_path));
            let response = ctx.client.post(&url).send().await?;
            let status = response.status();

            match status {
                s if s.is_success() => {
                    if let Ok(body) = response.json::<serde_json::Value>().await {
                        let message = body
                            .get("data")
                            .and_then(|d| d.get("message"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        println!(
                            "{}{} Uprooted {} (container destroyed)",
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
                        "{}{} Failed to destroy: {}",
                        " ".repeat(ui::constants::DEFAULT_INDENT),
                        ui::status_indicator("error", ctx.term.supports_color),
                        status
                    );
                }
            }

            // Self-teaching suggestions
            suggestions::print_suggestions(cmd::UPROOT, self.quiet);

            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        cmd::UPROOT
    }
}
