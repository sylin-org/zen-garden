//! List command - display services on a stone
//!
//! Shows all services installed on the target stone with their status.

use crate::command_manifest::cmd;
use crate::commands::{Command, CommandResult};
use crate::context::Context;
use crate::suggestions;
use crate::ui::rendering::{self as ui, TerminalInfo};
use anyhow::Context as _;
use colored::Colorize;
use garden_common::constants::CATEGORY_ORCHESTRATOR;
use garden_common::SubCapability;

// Re-use canonical types from garden-common
use garden_common::discovery::{FoundService, ServiceDiscoveryResponse};

// Use shared ApiResponse from garden-common
use garden_common::api_utils::ApiResponse;

/// List services on a stone
pub struct ListCommand {
    pub quiet: bool,
}

impl ListCommand {
    pub fn new(quiet: bool) -> Self {
        Self { quiet }
    }
}

impl Command for ListCommand {
    fn execute<'a>(&'a self, ctx: &'a Context) -> std::pin::Pin<Box<dyn std::future::Future<Output = CommandResult> + Send + 'a>> {
        Box::pin(async move {
            let url = ctx.api_v1_url("stone/services");
            let response = ctx.client.get(&url).send().await?;

            let api_response: ApiResponse<ServiceDiscoveryResponse> = response
                .json()
                .await
                .context("Failed to parse services response")?;

            let services = api_response.data.services;

            if services.is_empty() {
                println!(
                    "{}",
                    ui::empty_state(
                        "No services installed",
                        Some("Use: garden-rake offer <service>")
                    )
                );
            } else {
                println!("{}", ui::section_header("SERVICES", &ctx.term));
                println!();
                render_services_table(&services, &ctx.term);
            }

            // Self-teaching suggestions
            suggestions::print_suggestions(cmd::LIST, self.quiet);

            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        cmd::LIST
    }
}

/// Render services in a formatted table
fn render_services_table(services: &[FoundService], term: &TerminalInfo) {
    // Check if any service has sub-capabilities to show the column
    let has_capabilities = services.iter().any(|s| !s.sub_capabilities.is_empty());

    let mut table = if has_capabilities {
        ui::TableBuilder::new()
            .add_column(ui::constants::MAX_SERVICE_NAME_LEN, ui::Align::Left)
            .add_column(20, ui::Align::Left)
            .add_column(16, ui::Align::Left)
            .add_column(14, ui::Align::Left)
    } else {
        ui::TableBuilder::new()
            .add_column(ui::constants::MAX_SERVICE_NAME_LEN, ui::Align::Left)
            .add_column(20, ui::Align::Left)
            .add_column(16, ui::Align::Left)
    };

    let mut running_count = 0;
    let mut stopped_count = 0;

    for svc in services {
        let is_registered = svc.category == CATEGORY_ORCHESTRATOR;

        if !is_registered {
            let status_lower = svc.status.to_lowercase();
            if status_lower.contains(garden_common::constants::SERVICE_RUNNING) {
                running_count += 1;
            } else {
                stopped_count += 1;
            }
        }

        let status_display = if is_registered {
            if term.supports_color {
                "[registered]".blue().to_string()
            } else {
                "[registered]".to_string()
            }
        } else {
            let status_lower = svc.status.to_lowercase();
            ui::status_indicator(&status_lower, term.supports_color)
        };

        let offering_display = if is_registered {
            if svc.source.is_empty() {
                CATEGORY_ORCHESTRATOR.to_string()
            } else {
                svc.source.clone()
            }
        } else if svc.offering.is_empty() {
            garden_common::constants::VALUE_UNKNOWN.to_string()
        } else {
            svc.offering.clone()
        };

        if has_capabilities {
            // Show capability summary (e.g., "12 models")
            let cap_summary = format_capability_summary(&svc.sub_capabilities);
            table.add_row(vec![
                ui::truncate_name(&svc.name, ui::constants::MAX_SERVICE_NAME_LEN),
                status_display,
                offering_display,
                cap_summary,
            ]);
        } else {
            table.add_row(vec![
                ui::truncate_name(&svc.name, ui::constants::MAX_SERVICE_NAME_LEN),
                status_display,
                offering_display,
            ]);
        }
    }

    println!("{}", table.render());
    println!();

    let registered_count = services
        .iter()
        .filter(|s| s.category == CATEGORY_ORCHESTRATOR)
        .count();
    let offering_count = services.len() - registered_count;

    if registered_count > 0 {
        println!(
            "{}  {} services ({} running, {} stopped) + {} registered",
            " ".repeat(ui::constants::DEFAULT_INDENT),
            offering_count,
            running_count,
            stopped_count,
            registered_count,
        );
    } else {
        println!(
            "{}  {} services ({} running, {} stopped)",
            " ".repeat(ui::constants::DEFAULT_INDENT),
            offering_count,
            running_count,
            stopped_count,
        );
    }
}

/// Format capability summary (e.g., "12 models", "3 ext.")
fn format_capability_summary(caps: &[SubCapability]) -> String {
    if caps.is_empty() {
        return String::new();
    }

    // Sum all items across capability types
    let total: usize = caps.iter().map(|c| c.items.len()).sum();
    if total == 0 {
        return String::new();
    }

    // Use first capability type for label
    if let Some(first) = caps.first() {
        let label = if total == 1 {
            // Truncate singular type for display
            truncate_cap_type(&first.cap_type, false)
        } else {
            // Use plural form
            truncate_cap_type(&first.cap_type, true)
        };
        format!("{} {}", total, label)
    } else {
        format!("{}", total)
    }
}

/// Truncate capability type for compact display
fn truncate_cap_type(cap_type: &str, plural: bool) -> String {
    let base = match cap_type.to_lowercase().as_str() {
        "model" => {
            if plural {
                "models"
            } else {
                "model"
            }
        }
        "extension" => "ext.",
        "module" => {
            if plural {
                "mods."
            } else {
                "mod."
            }
        }
        "plugin" => "plug.",
        "collection" => "coll.",
        _ => cap_type,
    };
    base.to_string()
}
