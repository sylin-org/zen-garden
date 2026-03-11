//! Config command - service configuration query for automation
//!
//! Retrieves detailed configuration for a service by name.
//! Designed for automation and scripting scenarios.
//!
//! Examples:
//!   garden-rake config mongodb                           # Full config (human)
//!   garden-rake config mongodb --output json             # Full config (JSON)
//!   garden-rake config mongodb --field "connection.uris[0]"  # Just the URI

use crate::command_manifest::cmd;
use crate::commands::{Command, CommandResult};
use crate::context::{extract_json_field, CommandContext};
use anyhow::Context;
use async_trait::async_trait;
use garden_common::ui::rendering as ui;
use serde::{Deserialize, Serialize};

/// Service configuration response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfigResponse {
    /// Service name
    pub name: String,
    /// Offering template name
    pub offering: String,
    /// Service version
    pub version: String,
    /// Current status
    pub status: String,
    /// Health status
    pub health: String,
    /// Stone where service is running
    pub stone: StoneInfo,
    /// Connection information
    pub connection: ConnectionConfig,
    /// Port configuration
    pub ports: PortConfig,
}

/// Stone information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoneInfo {
    pub name: String,
    pub endpoint: String,
}

/// Connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    /// Primary connection URI (hostname-based)
    pub uri: String,
    /// All available URIs
    pub uris: Vec<String>,
    /// Hostname
    pub hostname: String,
    /// IP address
    pub ip: String,
    /// Port
    pub port: u16,
    /// Protocol (e.g., mongodb, redis, http)
    pub protocol: String,
}

/// Port configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortConfig {
    /// Native service port
    pub native: u16,
    /// Agnostic port (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agnostic: Option<u16>,
}

/// Config command - query service configuration
pub struct ConfigCommand {
    /// Service name to query
    pub service: String,
    /// Quiet mode (suppress hints)
    pub quiet: bool,
    /// Field extraction path (e.g., "connection.uri")
    pub field: Option<String>,
    /// Output JSON instead of human-readable
    pub json_output: bool,
}

impl ConfigCommand {
    pub fn new(
        service: String,
        quiet: bool,
        json_output: bool,
        field: Option<String>,
    ) -> Self {
        Self {
            service,
            quiet,
            json_output,
            field,
        }
    }
}

// Reuse find command's response types for API compatibility
use super::find::{FoundService, ServiceDiscoveryResponse};

#[async_trait]
impl Command for ConfigCommand {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        use garden_common::api_utils::{is_suspicious, sanitize_fqn_input, ApiResponse};

        // Validate service name
        if is_suspicious(&self.service) {
            anyhow::bail!("Service name contains invalid patterns");
        }

        let sanitized = sanitize_fqn_input(&self.service).into_value();

        // Use the find endpoint with exact name match
        let url = format!(
            "{}?q={}",
            ctx.api_v1_url("garden/services")?,
            urlencoding::encode(&sanitized)
        );

        tracing::debug!(service = %sanitized, url = %url, "ConfigCommand: querying service");

        let response = ctx
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to connect to moss")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("API error ({}): {}", status, body);
        }

        let api_response: ApiResponse<ServiceDiscoveryResponse> =
            response.json().await.context("Failed to parse response")?;

        let discovery = api_response.data;

        // Check if service was found
        if !discovery.found || discovery.services.is_empty() {
            if self.json_output || self.field.is_some() {
                // For automation, just exit with code 1
                eprintln!("Service '{}' not found", self.service);
                std::process::exit(1);
            }
            println!(
                "{}Service '{}' not found",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                self.service
            );
            println!();
            println!("{}Suggestions:", " ".repeat(ui::constants::DEFAULT_INDENT));
            println!(
                "{}  garden-rake find {}              # Search garden-wide",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                self.service
            );
            println!(
                "{}  garden-rake offer {} wishfully   # Auto-provision",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                self.service
            );
            std::process::exit(1);
        }

        // Get first matching service
        let svc = &discovery.services[0];

        // Build config response
        let config = self.build_config(svc, ctx);

        // Handle field extraction
        if let Some(ref field_path) = self.field {
            return self.render_field(&config, field_path);
        }

        // Handle output format
        if self.json_output {
            self.render_json(&config)?;
        } else {
            self.render_human(&config, ctx);
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        cmd::CONFIG
    }
}

impl ConfigCommand {
    /// Build config response from found service
    fn build_config(&self, svc: &FoundService, _ctx: &CommandContext) -> ServiceConfigResponse {
        ServiceConfigResponse {
            name: svc.name.clone(),
            offering: svc.offering.clone(),
            version: svc.connection.protocol.clone(), // Use protocol as version hint
            status: svc.status.clone(),
            health: "unknown".to_string(), // Not in find response
            stone: StoneInfo {
                name: svc.stone.name.clone(),
                endpoint: svc.stone.endpoint.clone(),
            },
            connection: ConnectionConfig {
                uri: svc.connection.uris.first().cloned().unwrap_or_default(),
                uris: svc.connection.uris.clone(),
                hostname: svc.connection.hostname.clone(),
                ip: svc.connection.ip.clone(),
                port: svc.connection.port,
                protocol: svc.connection.protocol.clone(),
            },
            ports: PortConfig {
                native: svc.connection.port,
                agnostic: None,
            },
        }
    }

    /// Render human-readable output
    fn render_human(&self, config: &ServiceConfigResponse, _ctx: &CommandContext) {
        println!();
        println!(
            "{}Configuration for {}",
            " ".repeat(ui::constants::DEFAULT_INDENT),
            config.name
        );
        println!(
            "{}{}",
            " ".repeat(ui::constants::DEFAULT_INDENT),
            "-".repeat(40)
        );
        println!();

        println!(
            "{}  Offering:  {}",
            " ".repeat(ui::constants::DEFAULT_INDENT),
            config.offering
        );
        println!(
            "{}  Status:    {}",
            " ".repeat(ui::constants::DEFAULT_INDENT),
            config.status
        );
        println!(
            "{}  Stone:     {}",
            " ".repeat(ui::constants::DEFAULT_INDENT),
            config.stone.name
        );
        println!();

        println!("{}Connection:", " ".repeat(ui::constants::DEFAULT_INDENT));
        println!(
            "{}  URI:       {}",
            " ".repeat(ui::constants::DEFAULT_INDENT),
            config.connection.uri
        );
        println!(
            "{}  Host:      {}",
            " ".repeat(ui::constants::DEFAULT_INDENT),
            config.connection.hostname
        );
        println!(
            "{}  IP:        {}",
            " ".repeat(ui::constants::DEFAULT_INDENT),
            config.connection.ip
        );
        println!(
            "{}  Port:      {}",
            " ".repeat(ui::constants::DEFAULT_INDENT),
            config.connection.port
        );
        println!(
            "{}  Protocol:  {}",
            " ".repeat(ui::constants::DEFAULT_INDENT),
            config.connection.protocol
        );

        if !self.quiet {
            println!();
            println!(
                "{}Hint: Use --output json for machine-readable output",
                " ".repeat(ui::constants::DEFAULT_INDENT)
            );
            println!(
                "{}      Use --field \"connection.uri\" to extract a specific value",
                " ".repeat(ui::constants::DEFAULT_INDENT)
            );
        }
    }

    /// Render JSON output
    fn render_json(&self, config: &ServiceConfigResponse) -> CommandResult {
        let json = serde_json::to_string_pretty(config).context("Failed to serialize config")?;
        println!("{}", json);
        Ok(())
    }

    /// Render a specific field (for automation)
    fn render_field(&self, config: &ServiceConfigResponse, field_path: &str) -> CommandResult {
        let json_value =
            serde_json::to_value(config).context("Failed to convert config to JSON")?;

        match extract_json_field(&json_value, field_path) {
            Some(value) => {
                println!("{}", value);
                Ok(())
            }
            None => {
                eprintln!("Field '{}' not found in config", field_path);
                std::process::exit(1);
            }
        }
    }
}
