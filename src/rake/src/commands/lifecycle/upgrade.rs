//! Upgrade command - nourish/update services
//!
//! Pulls latest images and restarts services with new versions.

use crate::commands::{Command, CommandResult};
use crate::context::Runtime;
use crate::suggestions;
use crate::ui::rendering as ui;

/// Upgrade (nourish) services
pub struct UpgradeCommand {
    pub service: Option<String>,
    pub all: bool,
    pub quiet: bool,
}

impl UpgradeCommand {
    pub fn new(service: Option<String>, all: bool, quiet: bool) -> Self {
        Self {
            service,
            all,
            quiet,
        }
    }
}

impl Command for UpgradeCommand {
    fn execute<'a>(&'a self, ctx: &'a Runtime) -> std::pin::Pin<Box<dyn std::future::Future<Output = CommandResult> + Send + 'a>> {
        Box::pin(async move {
            let endpoint = ctx.endpoint()?;

            if self.all || self.service.is_none() {
                // Batch upgrade all services (iterate v1 nourish endpoints)
                // First, get list of all services
                let list_url = format!("{}/api/v1/stone/services", endpoint.trim_end_matches('/'));
                let list_response = ctx.client.get(&list_url).send().await?;

                if !list_response.status().is_success() {
                    eprintln!(
                        "{}{} Failed to retrieve service list: {}",
                        " ".repeat(ui::constants::DEFAULT_INDENT),
                        ui::status_indicator("error", ctx.term.supports_color),
                        list_response.status()
                    );
                    return Ok(());
                }

                let services_body: serde_json::Value = list_response.json().await?;
                let services = services_body
                    .as_array()
                    .or_else(|| services_body.get("data").and_then(|d| d.as_array()));

                if let Some(service_list) = services {
                    let mut upgraded = Vec::new();
                    let mut failed = Vec::new();

                    for svc in service_list {
                        if let Some(name) = svc.get("name").and_then(|n| n.as_str()) {
                            let name_path = urlencoding::encode(name);
                            let nourish_url = format!(
                                "{}/api/v1/stone/services/{}/upgrade",
                                endpoint.trim_end_matches('/'),
                                name_path
                            );
                            let response = ctx.client.post(&nourish_url).send().await?;

                            if response.status().is_success() {
                                upgraded.push(name.to_string());
                            } else {
                                failed.push(name.to_string());
                            }
                        }
                    }

                    if !upgraded.is_empty() {
                        println!(
                            "{}{} Upgraded {} service(s)",
                            " ".repeat(ui::constants::DEFAULT_INDENT),
                            ui::status_indicator("ok", ctx.term.supports_color),
                            upgraded.len()
                        );
                        for name in &upgraded {
                            println!("{}  - {}", " ".repeat(ui::constants::DEFAULT_INDENT), name);
                        }
                    }

                    if !failed.is_empty() {
                        eprintln!(
                            "{}{} Failed to upgrade {} service(s)",
                            " ".repeat(ui::constants::DEFAULT_INDENT),
                            ui::status_indicator("error", ctx.term.supports_color),
                            failed.len()
                        );
                        for name in &failed {
                            eprintln!("{}  - {}", " ".repeat(ui::constants::DEFAULT_INDENT), name);
                        }
                    }
                } else {
                    eprintln!(
                        "{}{} No services found",
                        " ".repeat(ui::constants::DEFAULT_INDENT),
                        ui::status_indicator("error", ctx.term.supports_color)
                    );
                }
            } else if let Some(svc_name) = &self.service {
                // v1 API: POST /api/v1/stone/services/:service/upgrade
                let name_path = urlencoding::encode(svc_name);
                let url = format!(
                    "{}/api/v1/stone/services/{}/upgrade",
                    endpoint.trim_end_matches('/'),
                    name_path
                );
                let response = ctx.client.post(url).send().await?;
                let status = response.status();

                match status {
                    s if s.is_success() => {
                        // Parse v1 API response
                        if let Ok(body) = response.json::<serde_json::Value>().await {
                            let message = body.get("message").and_then(|v| v.as_str()).unwrap_or("");
                            let api_status = body
                                .get("status")
                                .and_then(|v| v.as_str())
                                .unwrap_or("upgraded");

                            println!(
                                "{}{} Upgraded {} ({})",
                                " ".repeat(ui::constants::DEFAULT_INDENT),
                                ui::status_indicator("ok", ctx.term.supports_color),
                                svc_name,
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
                                "{}{} Upgraded {}",
                                " ".repeat(ui::constants::DEFAULT_INDENT),
                                ui::status_indicator("ok", ctx.term.supports_color),
                                svc_name
                            );
                        }
                    }
                    reqwest::StatusCode::ACCEPTED => {
                        println!(
                            "{}{} Service under maintenance, retry later",
                            " ".repeat(ui::constants::DEFAULT_INDENT),
                            ui::status_indicator("pending", ctx.term.supports_color)
                        );
                    }
                    reqwest::StatusCode::NOT_FOUND => {
                        eprintln!(
                            "{}{} Service '{}' not found",
                            " ".repeat(ui::constants::DEFAULT_INDENT),
                            ui::status_indicator("error", ctx.term.supports_color),
                            svc_name
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
            }

            // Self-teaching suggestions
            suggestions::print_suggestions("nourish", self.quiet);

            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        "nourish"
    }
}
