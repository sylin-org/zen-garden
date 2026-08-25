//! Adopted command - list adopted services
//!
//! Shows services that were adopted from existing containers.

use crate::commands::{Command, CommandResult};
use crate::context::Context;
use crate::suggestions;
use crate::ui::rendering as ui;

/// List adopted services
pub struct AdoptedCommand {
    pub quiet: bool,
}

impl AdoptedCommand {
    pub fn new(quiet: bool) -> Self {
        Self { quiet }
    }
}

impl Command for AdoptedCommand {
    fn execute<'a>(&'a self, ctx: &'a Context) -> std::pin::Pin<Box<dyn std::future::Future<Output = CommandResult> + Send + 'a>> {
        Box::pin(async move {
            let url = ctx.api_v1_url("stone/offerings/adopted");
            let response = ctx.client.get(&url).send().await?;

            if response.status().is_success() {
                let body: serde_json::Value = response.json().await?;
                let adopted = body.get("data").and_then(|d| d.as_array());

                if let Some(list) = adopted {
                    if list.is_empty() {
                        println!(
                            "{}No adopted services",
                            " ".repeat(ui::constants::DEFAULT_INDENT)
                        );
                    } else {
                        println!(
                            "{}Adopted services:",
                            " ".repeat(ui::constants::DEFAULT_INDENT)
                        );
                        for svc in list {
                            let name = svc
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown");
                            let offering = svc.get("offering").and_then(|v| v.as_str()).unwrap_or("");
                            if !offering.is_empty() && offering != name {
                                println!(
                                    "{}  {} (from: {})",
                                    " ".repeat(ui::constants::DEFAULT_INDENT),
                                    name,
                                    offering
                                );
                            } else {
                                println!("{}  {}", " ".repeat(ui::constants::DEFAULT_INDENT), name);
                            }
                        }
                    }
                }
            } else {
                eprintln!(
                    "{}{} Failed to list adopted services: {}",
                    " ".repeat(ui::constants::DEFAULT_INDENT),
                    ui::status_indicator("error", ctx.term.supports_color),
                    response.status()
                );
            }

            // Self-teaching suggestions
            suggestions::print_suggestions("adopted", self.quiet);

            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        "adopted"
    }
}
