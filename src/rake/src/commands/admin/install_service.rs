//! Install service command - stone system service installation
//!
//! Installs the target stone as a system service:
//! - Zen syntax: garden-rake take-root at <stone>
//! - Normative syntax: garden-rake install-service --at <stone>

use crate::command_manifest::cmd;
use crate::commands::{Command, CommandResult};
use crate::context::Runtime;
use crate::suggestions;
use crate::ui::rendering as ui;
use std::time::Duration;

/// CLI style for service installation
pub enum InstallStyle {
    /// Zen syntax: take-root at <stone>
    TakeRoot,
    /// Normative syntax: install-service --at <stone>
    InstallService,
}

/// Install service command for system service installation
pub struct InstallServiceCommand {
    pub style: InstallStyle,
    pub quiet: bool,
}

impl InstallServiceCommand {
    pub fn new(style: InstallStyle, quiet: bool) -> Self {
        Self { style, quiet }
    }

    /// Create for zen syntax (take-root)
    pub fn take_root(quiet: bool) -> Self {
        Self::new(InstallStyle::TakeRoot, quiet)
    }

    /// Create for normative syntax (install-service)
    pub fn install_service(quiet: bool) -> Self {
        Self::new(InstallStyle::InstallService, quiet)
    }
}

impl Command for InstallServiceCommand {
    fn execute<'a>(&'a self, ctx: &'a Runtime) -> std::pin::Pin<Box<dyn std::future::Future<Output = CommandResult> + Send + 'a>> {
        Box::pin(async move {
            let endpoint = ctx.endpoint()?;
            let url = format!("{}/admin/take-root", endpoint.trim_end_matches('/'));

            // Show initial message based on style
            match self.style {
                InstallStyle::TakeRoot => {
                    println!(
                        "{}{} Instructing stone to take root as system service...",
                        " ".repeat(ui::constants::DEFAULT_INDENT),
                        ui::status_indicator("info", ctx.term.supports_color)
                    );
                }
                InstallStyle::InstallService => {
                    println!(
                        "{}{} Instructing stone to install as system service...",
                        " ".repeat(ui::constants::DEFAULT_INDENT),
                        ui::status_indicator("info", ctx.term.supports_color)
                    );
                }
            }
            println!();

            match ctx
                .client
                .post(&url)
                .timeout(Duration::from_secs(30))
                .send()
                .await
            {
                Ok(response) if response.status().is_success() => {
                    if let Ok(body) = response.json::<serde_json::Value>().await {
                        if let Some(message) = body.get("message").and_then(|v| v.as_str()) {
                            println!(
                                "{}{} {}",
                                " ".repeat(ui::constants::DEFAULT_INDENT),
                                ui::status_indicator("ok", ctx.term.supports_color),
                                message
                            );
                        } else {
                            print_success_message(ctx, &self.style);
                        }
                    } else {
                        print_success_message(ctx, &self.style);
                    }
                }
                Ok(response) => {
                    let status = response.status();
                    if let Ok(body) = response.text().await {
                        eprintln!(
                            "{}{} Failed to install service: {}",
                            " ".repeat(ui::constants::DEFAULT_INDENT),
                            ui::status_indicator("error", ctx.term.supports_color),
                            body
                        );
                    } else {
                        eprintln!(
                            "{}{} Failed to install service: HTTP {}",
                            " ".repeat(ui::constants::DEFAULT_INDENT),
                            ui::status_indicator("error", ctx.term.supports_color),
                            status
                        );
                    }
                }
                Err(e) => {
                    eprintln!(
                        "{}{} Request failed: {}",
                        " ".repeat(ui::constants::DEFAULT_INDENT),
                        ui::status_indicator("error", ctx.term.supports_color),
                        e
                    );
                    eprintln!(
                        "{}Ensure the target stone is running and accessible",
                        " ".repeat(ui::constants::DEFAULT_INDENT)
                    );
                }
            }

            // Self-teaching suggestions
            let cmd_name = match self.style {
                InstallStyle::TakeRoot => cmd::TAKE_ROOT,
                InstallStyle::InstallService => "install-service",
            };
            suggestions::print_suggestions(cmd_name, self.quiet);

            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        match self.style {
            InstallStyle::TakeRoot => cmd::TAKE_ROOT,
            InstallStyle::InstallService => "install-service",
        }
    }
}

fn print_success_message(ctx: &Runtime, style: &InstallStyle) {
    let message = match style {
        InstallStyle::TakeRoot => "Stone has taken root as a system service",
        InstallStyle::InstallService => "Service installed successfully",
    };
    println!(
        "{}{} {}",
        " ".repeat(ui::constants::DEFAULT_INDENT),
        ui::status_indicator("ok", ctx.term.supports_color),
        message
    );
}
