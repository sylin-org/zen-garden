//! Stone admin commands - power management
//!
//! Commands for controlling stone power state:
//! - Rouse: Wake a stone via Wake-on-LAN
//! - Slumber: Shut down a stone (power off)
//! - Stir: Reboot a stone

use crate::commands::{Command, CommandResult};
use crate::context::Runtime;
use crate::suggestions;
use crate::ui::rendering as ui;

// ============================================================================
// Rouse Runtime - Wake-on-LAN
// ============================================================================

/// Wake a stone via Wake-on-LAN magic packet
pub struct RouseCommand {
    /// Stone name to wake
    pub stone_name: String,
    pub quiet: bool,
}

impl RouseCommand {
    pub fn new(stone_name: String, quiet: bool) -> Self {
        Self { stone_name, quiet }
    }
}

impl Command for RouseCommand {
    fn execute<'a>(&'a self, ctx: &'a Runtime) -> std::pin::Pin<Box<dyn std::future::Future<Output = CommandResult> + Send + 'a>> {
        Box::pin(async move {
            let api = ctx.stone_api()?;

            println!(
                "{}{} Rousing {}...",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("info", ctx.term.supports_color),
                self.stone_name
            );

            match api.stone().wake(&self.stone_name).await {
                Ok(body) => {
                    let message = body
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Wake-on-LAN packet sent");
                    let mac = body.get("mac").and_then(|v| v.as_str());
                    let stone_status = body.get("status").and_then(|v| v.as_str());

                    println!(
                        "{}{} {}",
                        " ".repeat(ui::constants::DEFAULT_INDENT),
                        ui::status_indicator("ok", ctx.term.supports_color),
                        message
                    );

                    if let Some(mac_addr) = mac {
                        println!(
                            "{}   MAC: {}",
                            " ".repeat(ui::constants::DEFAULT_INDENT),
                            mac_addr
                        );
                    }

                    if let Some(status) = stone_status {
                        println!(
                            "{}   Status was: {}",
                            " ".repeat(ui::constants::DEFAULT_INDENT),
                            status
                        );
                    }
                }
                Err(e) if e.is_not_found() => {
                    eprintln!(
                        "{}{} Stone '{}' not found in topology cache",
                        " ".repeat(ui::constants::DEFAULT_INDENT),
                        ui::status_indicator("error", ctx.term.supports_color),
                        self.stone_name
                    );
                }
                Err(e) => {
                    eprintln!(
                        "{}{} {}",
                        " ".repeat(ui::constants::DEFAULT_INDENT),
                        ui::status_indicator("error", ctx.term.supports_color),
                        e.display_message()
                    );
                }
            }

            suggestions::print_suggestions("stone wake", self.quiet);

            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        "stone wake"
    }
}

// ============================================================================
// Slumber Runtime - Stone Shutdown
// ============================================================================

/// Shut down a stone (power off)
pub struct SlumberCommand {
    pub quiet: bool,
}

impl SlumberCommand {
    pub fn new(quiet: bool) -> Self {
        Self { quiet }
    }
}

impl Command for SlumberCommand {
    fn execute<'a>(&'a self, ctx: &'a Runtime) -> std::pin::Pin<Box<dyn std::future::Future<Output = CommandResult> + Send + 'a>> {
        Box::pin(async move {
            let api = ctx.stone_api()?;

            println!(
                "{}{} Requesting stone to enter slumber...",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("warn", ctx.term.supports_color)
            );

            match api.stone().shutdown().await {
                Ok(body) => {
                    let message = body
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Shutdown initiated");

                    println!(
                        "{}{} {}",
                        " ".repeat(ui::constants::DEFAULT_INDENT),
                        ui::status_indicator("ok", ctx.term.supports_color),
                        message
                    );
                }
                Err(e) => {
                    eprintln!(
                        "{}{} Failed to initiate shutdown: {}",
                        " ".repeat(ui::constants::DEFAULT_INDENT),
                        ui::status_indicator("error", ctx.term.supports_color),
                        e.display_message()
                    );
                }
            }

            suggestions::print_suggestions("stone shutdown", self.quiet);

            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        "stone shutdown"
    }
}

// ============================================================================
// Stir Runtime - Stone Reboot
// ============================================================================

/// Reboot a stone
pub struct StirCommand {
    pub quiet: bool,
}

impl StirCommand {
    pub fn new(quiet: bool) -> Self {
        Self { quiet }
    }
}

impl Command for StirCommand {
    fn execute<'a>(&'a self, ctx: &'a Runtime) -> std::pin::Pin<Box<dyn std::future::Future<Output = CommandResult> + Send + 'a>> {
        Box::pin(async move {
            let api = ctx.stone_api()?;

            println!(
                "{}{} Requesting stone to stir (reboot)...",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("warn", ctx.term.supports_color)
            );

            match api.stone().reboot().await {
                Ok(body) => {
                    let message = body
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Reboot initiated");

                    println!(
                        "{}{} {}",
                        " ".repeat(ui::constants::DEFAULT_INDENT),
                        ui::status_indicator("ok", ctx.term.supports_color),
                        message
                    );
                }
                Err(e) => {
                    eprintln!(
                        "{}{} Failed to initiate reboot: {}",
                        " ".repeat(ui::constants::DEFAULT_INDENT),
                        ui::status_indicator("error", ctx.term.supports_color),
                        e.display_message()
                    );
                }
            }

            suggestions::print_suggestions("stone reboot", self.quiet);

            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        "stone reboot"
    }
}
