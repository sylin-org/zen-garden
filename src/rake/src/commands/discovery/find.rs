//! Find command - service discovery with connection strings
//!
//! Finds running services across the garden and returns connection URIs.
//! Supports search by name, category, or tags with cache-first architecture.
//!
//! Wishfully mode: Auto-provision if service not found and query matches a known offering.

use crate::command_manifest::cmd;
use crate::commands::{Command, CommandResult};
use crate::context::{extract_json_field, CommandContext};
use crate::suggestions;
use anyhow::Context;
use async_trait::async_trait;
use futures_util::StreamExt;
use garden_common::offerings::parse_offering_fqn;
use garden_common::tools::{
    event_types as tools_event_types, parse_capability_wish, CapabilitySelector, ToolDelta,
    ToolProjection,
};
use garden_common::ui::rendering as ui;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Output format for find command
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindOutputFormat {
    /// Human-readable output (default)
    Human,
    /// JSON output
    Json,
    /// URI only (hostname-based)
    Uri,
    /// URI only (IP-based fallback)
    UriIp,
}

impl Default for FindOutputFormat {
    fn default() -> Self {
        Self::Human
    }
}

impl FindOutputFormat {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "json" => Self::Json,
            "uri" | "connection-string" => Self::Uri,
            "uri-ip" | "ip" => Self::UriIp,
            _ => Self::Human,
        }
    }
}

/// Find services command
pub struct FindCommand {
    /// Search query (name, c:category, t:tag)
    pub query: String,
    /// Output format
    pub format: FindOutputFormat,
    /// Quiet mode (suppress hints)
    pub quiet_mode: bool,
    /// Fresh discovery (bypass cache)
    pub fresh: bool,
    /// Wishfully mode (auto-provision if not found)
    pub wishfully: bool,
    /// Optional field extraction path (e.g., "services[0].connection.uris[0]")
    pub field: Option<String>,
}

impl FindCommand {
    pub fn new(
        query: String,
        format: FindOutputFormat,
        quiet_mode: bool,
        fresh: bool,
        wishfully: bool,
    ) -> Self {
        Self {
            query,
            format,
            quiet_mode,
            fresh,
            wishfully,
            field: None,
        }
    }

    /// Create command with field extraction support
    pub fn with_field(
        query: String,
        format: FindOutputFormat,
        quiet_mode: bool,
        fresh: bool,
        wishfully: bool,
        field: Option<String>,
    ) -> Self {
        Self {
            query,
            format,
            quiet_mode,
            fresh,
            wishfully,
            field,
        }
    }
}

/// Stone reference in response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoneRef {
    pub id: String,
    pub name: String,
    pub endpoint: String,
}

/// Connection information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub hostname: String,
    pub ip: String,
    pub port: u16,
    pub protocol: String,
    pub uris: Vec<String>,
}

/// Found service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoundService {
    pub name: String,
    pub offering: String,
    pub category: String,
    pub tags: Vec<String>,
    pub status: String,
    pub stone: StoneRef,
    pub connection: ConnectionInfo,
}

/// Service discovery response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDiscoveryResponse {
    pub found: bool,
    pub services: Vec<FoundService>,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_age_seconds: Option<u64>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolsSnapshotEvent {
    pub cursor: u64,
    pub tools: Vec<ToolProjection>,
}

// Use shared ApiResponse from garden-common
use garden_common::api_utils::ApiResponse;

#[async_trait]
impl Command for FindCommand {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        use garden_common::api_utils::{is_suspicious, sanitize_query};

        // Reject suspicious patterns client-side
        if is_suspicious(&self.query) {
            anyhow::bail!("Query contains invalid patterns");
        }

        // Sanitize query input
        let sanitized_query = sanitize_query(&self.query).into_value();
        let discovery = self
            .query_services(ctx, &sanitized_query, self.fresh)
            .await?;

        // Handle not found case
        if !discovery.found {
            return self.handle_not_found(ctx).await;
        }

        // Field extraction mode: extract specific field and output just that value
        if let Some(ref field_path) = self.field {
            return self.render_field(&discovery, field_path);
        }

        // Render output based on format
        match self.format {
            FindOutputFormat::Human => {
                self.render_human(&discovery, ctx);
            }
            FindOutputFormat::Json => {
                self.render_json(&discovery)?;
            }
            FindOutputFormat::Uri => {
                self.render_uri(&discovery, false);
            }
            FindOutputFormat::UriIp => {
                self.render_uri(&discovery, true);
            }
        }

        // Self-teaching suggestions (unless quiet or non-human format)
        if self.format == FindOutputFormat::Human {
            suggestions::print_suggestions(cmd::FIND, self.quiet_mode);
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        cmd::FIND
    }
}

/// Offering info for wishfully mode
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OfferingInfo {
    pub name: String,
    pub category: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
struct CapabilityListEnvelope {
    data: CapabilityListPayload,
}

#[derive(Debug, Clone, Deserialize)]
struct CapabilityListPayload {
    #[serde(default)]
    capabilities: Vec<CapabilityTypeInfo>,
}

#[derive(Debug, Clone, Deserialize)]
struct CapabilityTypeInfo {
    #[serde(rename = "type")]
    cap_type: String,
}

impl FindCommand {
    async fn query_services(
        &self,
        ctx: &CommandContext,
        query: &str,
        fresh: bool,
    ) -> anyhow::Result<ServiceDiscoveryResponse> {
        let mut url = ctx.api_v1_url("garden/services")?;
        url = format!("{}?q={}", url, urlencoding::encode(query));
        if fresh {
            url = format!("{}&fresh=true", url);
        }

        tracing::debug!(
            query = %query,
            url = %url,
            endpoint = ?ctx.endpoint,
            "FindCommand: sending request to services?q="
        );

        let response = ctx
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to connect to moss")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::error!(
                status = %status,
                url = %url,
                body = %body,
                "FindCommand: API request failed"
            );
            anyhow::bail!("API error ({}): {}", status, body);
        }

        let api_response: ApiResponse<ServiceDiscoveryResponse> =
            response.json().await.context("Failed to parse response")?;

        Ok(api_response.data)
    }

    /// Check if query is a name search (not category or tag prefix)
    fn is_name_search(&self) -> bool {
        let q = self.query.trim().to_lowercase();
        !q.starts_with("c:")
            && !q.starts_with("cat:")
            && !q.starts_with("category:")
            && !q.starts_with("t:")
            && !q.starts_with("tag:")
            && !q.starts_with("tags:")
    }

    /// Check if the query matches a known offering
    async fn check_offering_exists_for(
        &self,
        ctx: &CommandContext,
        query: &str,
    ) -> Option<OfferingInfo> {
        let endpoint = ctx.endpoint.as_ref()?;
        let url = format!(
            "{}/api/v1/stone/offerings/{}",
            endpoint.trim_end_matches('/'),
            urlencoding::encode(query)
        );

        let response = ctx.client.get(&url).send().await.ok()?;
        if !response.status().is_success() {
            return None;
        }

        let body: serde_json::Value = response.json().await.ok()?;
        let data = body.get("data")?;

        Some(OfferingInfo {
            name: data.get("name")?.as_str()?.to_string(),
            category: data
                .get("category")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            description: data
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        })
    }

    /// Install an offering and wait for completion
    async fn install_offering(&self, ctx: &CommandContext, offering: &str) -> anyhow::Result<()> {
        let endpoint = ctx
            .endpoint
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No endpoint available"))?;

        let url = format!("{}/api/v1/stone/services", endpoint.trim_end_matches('/'));
        let payload = serde_json::json!({
            "offering": offering,
            "ports": [],
            "environment": {}
        });

        println!(
            "{}{} Provisioning '{}' service...",
            " ".repeat(ui::constants::DEFAULT_INDENT),
            ui::status_indicator("pending", ctx.term.supports_color),
            offering
        );

        let response = ctx.client.post(&url).json(&payload).send().await?;
        let status = response.status();

        if !status.is_success() && status != reqwest::StatusCode::ACCEPTED {
            let body: serde_json::Value = response.json().await.unwrap_or_default();
            let error_msg = body
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or("Installation failed");
            anyhow::bail!("{}", error_msg);
        }

        let body: serde_json::Value = response.json().await?;

        // Extract job_id from response
        let job_id = body
            .get("message")
            .and_then(|v| v.as_str())
            .and_then(|msg| {
                // Parse "Job ID: <uuid>" from message
                if msg.contains("Job ID:") {
                    msg.split("Job ID:")
                        .nth(1)
                        .map(|s| s.trim().split_whitespace().next().unwrap_or(""))
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                } else {
                    None
                }
            });

        // If no job_id, check if it was immediately adopted
        if job_id.is_none() {
            let message = body.get("message").and_then(|v| v.as_str()).unwrap_or("");
            if message.contains("Adopted") {
                println!(
                    "{}{} Service already exists (adopted)",
                    " ".repeat(ui::constants::DEFAULT_INDENT),
                    ui::status_indicator("ok", ctx.term.supports_color)
                );
                return Ok(());
            }
        }

        if let Some(job_id) = job_id {
            tracing::debug!(job_id = %job_id, offering, "Install job accepted, waiting on tools stream");
        }

        let tool_fqid = format!("offering:{}", offering.to_ascii_lowercase());
        self.wait_for_tool_ready(ctx, &tool_fqid, &[], Duration::from_secs(240))
            .await?;
        println!(
            "{}{} Service ready",
            " ".repeat(ui::constants::DEFAULT_INDENT),
            ui::status_indicator("ok", ctx.term.supports_color)
        );

        Ok(())
    }

    /// Re-run find query after provisioning
    async fn retry_find(&self, ctx: &CommandContext) -> anyhow::Result<ServiceDiscoveryResponse> {
        self.retry_find_with_query(ctx, &self.query).await
    }

    async fn retry_find_with_query(
        &self,
        ctx: &CommandContext,
        query: &str,
    ) -> anyhow::Result<ServiceDiscoveryResponse> {
        use garden_common::api_utils::sanitize_query;

        tokio::time::sleep(Duration::from_millis(500)).await;
        let sanitized_query = sanitize_query(query).into_value();
        self.query_services(ctx, &sanitized_query, true).await
    }

    async fn handle_capability_wishfully(&self, ctx: &CommandContext) -> anyhow::Result<bool> {
        let query = self.query.trim();
        if !query.contains(':') && !query.contains('[') {
            return Ok(false);
        }

        // If the raw query is itself a valid offering FQN, this is a classic
        // offering wishful path and should not be interpreted as capability wish.
        if !query.contains('[') && self.check_offering_exists_for(ctx, query).await.is_some() {
            return Ok(false);
        }

        // Keep defensive guard for common environment suffixes that are often
        // offering instances, not capability requests.
        if !query.contains('[') && query.matches(':').count() == 1 {
            let suffix = query
                .split(':')
                .nth(1)
                .unwrap_or_default()
                .to_ascii_lowercase();
            if matches!(
                suffix.as_str(),
                "dev" | "prod" | "staging" | "test" | "local" | "default"
            ) {
                return Ok(false);
            }
        }

        let Some(offering_hint) = capability_wish_offering_hint(query) else {
            return Ok(false);
        };

        let Some(offering) = self.check_offering_exists_for(ctx, &offering_hint).await else {
            return Ok(false);
        };

        println!(
            "{}No running capability '{}' found",
            " ".repeat(ui::constants::DEFAULT_INDENT),
            self.query
        );
        println!(
            "{}Found matching offering: {} ({})",
            " ".repeat(ui::constants::DEFAULT_INDENT),
            offering.name,
            offering.category
        );
        println!();

        // Ensure base offering is running before capability ensure.
        let existing = self.retry_find_with_query(ctx, &offering_hint).await?;
        if !existing.found {
            self.install_offering(ctx, &offering_hint).await?;
        }

        let capability_types = self.fetch_capability_types(ctx, &offering_hint).await?;
        let default_capability_type = if capability_types.len() == 1 {
            Some(capability_types[0].as_str())
        } else {
            None
        };

        let wish = parse_capability_wish(query, default_capability_type).map_err(|e| {
            anyhow::anyhow!(
                "{}. Use explicit typed selectors when needed, e.g. {}[type:item,type:item]",
                e,
                offering_hint
            )
        })?;

        self.ensure_capabilities(ctx, &wish).await?;

        let tool_fqid = format!("offering:{}", wish.offering_fqn);
        self.wait_for_tool_ready(ctx, &tool_fqid, &wish.selectors, Duration::from_secs(240))
            .await?;

        let required_items = wish
            .selectors
            .iter()
            .map(|selector| selector.item.clone())
            .collect::<Vec<_>>()
            .join(",");
        let cap_query = format!("{}[{}]", wish.offering_fqn, required_items);

        let mut discovery = self.retry_find_with_query(ctx, &cap_query).await?;
        if !discovery.found {
            discovery = self.retry_find_with_query(ctx, &wish.offering_fqn).await?;
        }

        if !discovery.found {
            anyhow::bail!("Capabilities are ready but service discovery has not converged yet");
        }

        match self.format {
            FindOutputFormat::Human => self.render_human(&discovery, ctx),
            FindOutputFormat::Json => self.render_json(&discovery)?,
            FindOutputFormat::Uri => self.render_uri(&discovery, false),
            FindOutputFormat::UriIp => self.render_uri(&discovery, true),
        }

        if self.format == FindOutputFormat::Human {
            suggestions::print_suggestions(cmd::FIND, self.quiet_mode);
        }

        Ok(true)
    }

    async fn fetch_capability_types(
        &self,
        ctx: &CommandContext,
        offering_fqn: &str,
    ) -> anyhow::Result<Vec<String>> {
        let endpoint = ctx
            .endpoint
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No endpoint available"))?;
        let url = format!(
            "{}/api/v1/stone/offerings/{}/capabilities",
            endpoint.trim_end_matches('/'),
            urlencoding::encode(offering_fqn)
        );

        let response = ctx.client.get(&url).send().await?;
        if !response.status().is_success() {
            let status = response.status();
            let body: serde_json::Value = response.json().await.unwrap_or_default();
            let code = body
                .get("error")
                .and_then(|v| v.get("code"))
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let message = body
                .get("error")
                .and_then(|v| v.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or("Failed to query capability metadata");

            if matches!(
                code,
                "NO_CAPABILITY_MANIFEST" | "UNKNOWN_CAPABILITY_TYPE" | "ADD_NOT_SUPPORTED"
            ) {
                anyhow::bail!("Capability ensure is not supported for this offering");
            }

            anyhow::bail!("API error ({}): {}", status, message);
        }

        let body: CapabilityListEnvelope = response.json().await?;
        let mut cap_types = Vec::new();
        for cap in body.data.capabilities {
            let cap_type = cap.cap_type.trim().to_ascii_lowercase();
            if !cap_type.is_empty() && !cap_types.iter().any(|t| t == &cap_type) {
                cap_types.push(cap_type);
            }
        }
        Ok(cap_types)
    }

    async fn ensure_capabilities(
        &self,
        ctx: &CommandContext,
        wish: &garden_common::tools::CapabilityWish,
    ) -> anyhow::Result<()> {
        let endpoint = ctx
            .endpoint
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No endpoint available"))?;
        let url = format!(
            "{}/api/v1/stone/offerings/{}/capabilities",
            endpoint.trim_end_matches('/'),
            urlencoding::encode(&wish.offering_fqn)
        );

        for selector in &wish.selectors {
            let payload = serde_json::json!({
                "name": selector.item,
                "type": selector.cap_type,
                "dry_run": false,
            });

            let response = ctx.client.post(&url).json(&payload).send().await?;
            if !response.status().is_success() {
                let status = response.status();
                let body: serde_json::Value = response.json().await.unwrap_or_default();
                let code = body
                    .get("error")
                    .and_then(|v| v.get("code"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();

                if matches!(
                    code.as_str(),
                    "NO_CAPABILITY_MANIFEST" | "UNKNOWN_CAPABILITY_TYPE" | "ADD_NOT_SUPPORTED"
                ) {
                    anyhow::bail!("Capability ensure is not supported for this offering");
                }

                let message = body
                    .get("error")
                    .and_then(|v| v.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Capability ensure failed");
                anyhow::bail!("API error ({}): {}", status, message);
            }

            let body: serde_json::Value = response.json().await?;
            let status = body
                .get("data")
                .and_then(|v| v.get("status"))
                .and_then(|v| v.as_str())
                .unwrap_or("started");

            match status {
                "exists" => {
                    println!(
                        "{}{} Capability already present: {}:{}",
                        " ".repeat(ui::constants::DEFAULT_INDENT),
                        ui::status_indicator("ok", ctx.term.supports_color),
                        selector.cap_type,
                        selector.item
                    );
                }
                "started" | "in_progress" => {
                    println!(
                        "{}{} Ensuring capability {}:{}...",
                        " ".repeat(ui::constants::DEFAULT_INDENT),
                        ui::status_indicator("pending", ctx.term.supports_color),
                        selector.cap_type,
                        selector.item
                    );
                }
                _ => {}
            }
        }

        Ok(())
    }

    async fn wait_for_tool_ready(
        &self,
        ctx: &CommandContext,
        tool_fqid: &str,
        requirements: &[CapabilitySelector],
        timeout: Duration,
    ) -> anyhow::Result<()> {
        let endpoint = ctx
            .endpoint
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No endpoint available"))?;
        let mut url = format!(
            "{}/api/v1/garden/tools/stream?tool_fqid={}",
            endpoint.trim_end_matches('/'),
            urlencoding::encode(tool_fqid)
        );
        if !requirements.is_empty() {
            let selector = requirements
                .iter()
                .map(|cap| format!("{}:{}", cap.cap_type, cap.item))
                .collect::<Vec<_>>()
                .join(",");
            url = format!("{}&capability={}", url, urlencoding::encode(&selector));
        }

        let response = ctx
            .client
            .get(&url)
            .header("accept", "text/event-stream")
            .send()
            .await
            .context("Failed to connect to tools stream")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Tools stream error ({}): {}", status, body);
        }

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let start = std::time::Instant::now();

        while start.elapsed() < timeout {
            let remaining = timeout.saturating_sub(start.elapsed());
            let next = tokio::time::timeout(remaining, stream.next()).await;
            let Some(chunk) = next
                .context("Timed out waiting for tools stream update")?
                .transpose()
                .context("Failed to read tools stream")?
            else {
                break;
            };

            let text = String::from_utf8_lossy(&chunk);
            buffer.push_str(&text.replace("\r\n", "\n"));

            while let Some(idx) = buffer.find("\n\n") {
                let frame = buffer[..idx].to_string();
                buffer = buffer[idx + 2..].to_string();

                let mut event_name = String::new();
                let mut data_lines = Vec::new();
                for line in frame.lines() {
                    if let Some(rest) = line.strip_prefix("event:") {
                        event_name = rest.trim().to_string();
                    } else if let Some(rest) = line.strip_prefix("data:") {
                        data_lines.push(rest.trim_start().to_string());
                    }
                }

                if data_lines.is_empty() {
                    continue;
                }

                let data = data_lines.join("\n");
                if event_name == tools_event_types::TOOLS_SNAPSHOT {
                    if let Ok(snapshot) = serde_json::from_str::<ToolsSnapshotEvent>(&data) {
                        if snapshot.tools.iter().any(|projection| {
                            projection.tool_fqid.eq_ignore_ascii_case(tool_fqid)
                                && projection_ready(projection, requirements)
                        }) {
                            return Ok(());
                        }
                    }
                    continue;
                }

                if event_name == tools_event_types::TOOL_UPSERT {
                    if let Ok(delta) = serde_json::from_str::<ToolDelta>(&data) {
                        if let Some(projection) = delta.projection.as_ref() {
                            if projection.tool_fqid.eq_ignore_ascii_case(tool_fqid)
                                && projection_ready(projection, requirements)
                            {
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }

        anyhow::bail!("Timeout waiting for tool readiness");
    }

    /// Handle not found case
    async fn handle_not_found(&self, ctx: &CommandContext) -> CommandResult {
        if self.wishfully && self.is_name_search() {
            if self.query.contains(':') || self.query.contains('[') {
                match self.handle_capability_wishfully(ctx).await {
                    Ok(true) => return Ok(()),
                    Ok(false) => {}
                    Err(e) => {
                        if self.query.contains('[') {
                            println!(
                                "{}{} Capability provisioning failed: {}",
                                " ".repeat(ui::constants::DEFAULT_INDENT),
                                ui::status_indicator("error", ctx.term.supports_color),
                                e
                            );
                            std::process::exit(3);
                        }
                        tracing::debug!(error = ?e, "Capability wishful path failed, falling back to offering wishful");
                    }
                }
            }

            // Check if query matches a known offering
            if let Some(offering) = self.check_offering_exists_for(ctx, &self.query).await {
                println!(
                    "{}No running '{}' service found",
                    " ".repeat(ui::constants::DEFAULT_INDENT),
                    self.query
                );
                println!(
                    "{}Found matching offering: {} ({})",
                    " ".repeat(ui::constants::DEFAULT_INDENT),
                    offering.name,
                    offering.category
                );
                println!();

                // Install the offering
                match self.install_offering(ctx, &offering.name).await {
                    Ok(()) => {
                        // Retry find after installation
                        println!();
                        match self.retry_find(ctx).await {
                            Ok(discovery) if discovery.found => {
                                // Success! Render the result
                                match self.format {
                                    FindOutputFormat::Human => {
                                        self.render_human(&discovery, ctx);
                                    }
                                    FindOutputFormat::Json => {
                                        self.render_json(&discovery)?;
                                    }
                                    FindOutputFormat::Uri => {
                                        self.render_uri(&discovery, false);
                                    }
                                    FindOutputFormat::UriIp => {
                                        self.render_uri(&discovery, true);
                                    }
                                }

                                if self.format == FindOutputFormat::Human {
                                    suggestions::print_suggestions(cmd::FIND, self.quiet_mode);
                                }

                                return Ok(());
                            }
                            Ok(_) => {
                                // Service installed but not found yet
                                println!(
                                    "{}{} Service installed but not yet ready",
                                    " ".repeat(ui::constants::DEFAULT_INDENT),
                                    ui::status_indicator("warn", ctx.term.supports_color)
                                );
                                println!(
                                    "{}Try again in a few seconds: garden-rake find {}",
                                    " ".repeat(ui::constants::DEFAULT_INDENT),
                                    self.query
                                );
                            }
                            Err(e) => {
                                println!(
                                    "{}{} Failed to verify service: {}",
                                    " ".repeat(ui::constants::DEFAULT_INDENT),
                                    ui::status_indicator("warn", ctx.term.supports_color),
                                    e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        println!(
                            "{}{} Provisioning failed: {}",
                            " ".repeat(ui::constants::DEFAULT_INDENT),
                            ui::status_indicator("error", ctx.term.supports_color),
                            e
                        );
                        std::process::exit(3); // Exit code 3 for provisioning failed
                    }
                }

                std::process::exit(1);
            } else {
                // No matching offering found
                println!(
                    "{}No running '{}' service found",
                    " ".repeat(ui::constants::DEFAULT_INDENT),
                    self.query
                );
                println!(
                    "{}{} No matching offering available to provision",
                    " ".repeat(ui::constants::DEFAULT_INDENT),
                    ui::status_indicator("warn", ctx.term.supports_color)
                );
                println!();
                println!("{}Suggestions:", " ".repeat(ui::constants::DEFAULT_INDENT));
                println!(
                    "{}  garden-rake offer              # View available offerings",
                    " ".repeat(ui::constants::DEFAULT_INDENT)
                );
                println!(
                    "{}  garden-rake find c:database    # Find any database",
                    " ".repeat(ui::constants::DEFAULT_INDENT)
                );
            }
        } else if self.wishfully {
            // Wishfully mode with category/tag search
            println!(
                "{}No running services found matching '{}'",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                self.query
            );
            println!(
                "{}{} Wishfully mode requires a specific offering name",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("info", ctx.term.supports_color)
            );
            println!();
            println!(
                "{}Try: garden-rake find mongodb wishfully",
                " ".repeat(ui::constants::DEFAULT_INDENT)
            );
        } else {
            println!(
                "{}No running '{}' service found",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                self.query
            );
            println!();
            println!("{}Suggestions:", " ".repeat(ui::constants::DEFAULT_INDENT));
            println!(
                "{}  garden-rake find {} wishfully  # Auto-provision {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                self.query,
                self.query
            );
            println!(
                "{}  garden-rake offer              # View available offerings",
                " ".repeat(ui::constants::DEFAULT_INDENT)
            );
            println!(
                "{}  garden-rake find c:database    # Find any database",
                " ".repeat(ui::constants::DEFAULT_INDENT)
            );
        }

        // Return exit code 1 for not found
        std::process::exit(1);
    }

    /// Render human-readable output
    fn render_human(&self, discovery: &ServiceDiscoveryResponse, _ctx: &CommandContext) {
        let services = &discovery.services;

        for svc in services {
            println!();
            println!(
                "{}  {} ({}) on {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                svc.name,
                svc.category,
                svc.stone.name
            );

            // Primary URI (hostname-based)
            if let Some(uri) = svc.connection.uris.first() {
                println!("{}  {}", " ".repeat(ui::constants::DEFAULT_INDENT), uri);
            }
        }

        // Summary for multiple results
        if services.len() > 1 {
            let stone_count = services
                .iter()
                .map(|s| &s.stone.id)
                .collect::<std::collections::HashSet<_>>()
                .len();

            println!();
            println!(
                "{}Found {} services across {} stone{}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                services.len(),
                stone_count,
                if stone_count != 1 { "s" } else { "" }
            );
        }

        // Hint for JSON output
        if !self.quiet_mode && self.format == FindOutputFormat::Human {
            println!();
            println!(
                "{}Hint: Use `garden-rake find {} --format json` for machine-readable output",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                self.query
            );
        }
    }

    /// Render JSON output
    fn render_json(&self, discovery: &ServiceDiscoveryResponse) -> CommandResult {
        let json =
            serde_json::to_string_pretty(discovery).context("Failed to serialize response")?;
        println!("{}", json);
        Ok(())
    }

    /// Render URI-only output
    fn render_uri(&self, discovery: &ServiceDiscoveryResponse, use_ip: bool) {
        for svc in &discovery.services {
            let uri = if use_ip {
                // IP-based URI (second in list, fallback)
                svc.connection.uris.get(1).or(svc.connection.uris.first())
            } else {
                // Hostname-based URI (first in list)
                svc.connection.uris.first()
            };

            if let Some(u) = uri {
                println!("{}", u);
            }
        }
    }

    /// Render a specific field from the response (for automation)
    ///
    /// Supports dot notation with array indexing:
    /// - "services[0].connection.uris[0]" -> first service's first URI
    /// - "services[0].name" -> first service's name
    /// - "found" -> boolean found status
    fn render_field(
        &self,
        discovery: &ServiceDiscoveryResponse,
        field_path: &str,
    ) -> CommandResult {
        // Convert to JSON value for field extraction
        let json_value =
            serde_json::to_value(discovery).context("Failed to convert response to JSON")?;

        match extract_json_field(&json_value, field_path) {
            Some(value) => {
                println!("{}", value);
                Ok(())
            }
            None => {
                // Field not found - exit with error code
                eprintln!("Field '{}' not found in response", field_path);
                std::process::exit(1);
            }
        }
    }
}

fn capability_wish_offering_hint(query: &str) -> Option<String> {
    let query = query.trim();
    if query.is_empty() {
        return None;
    }

    if let Some((offering, _)) = query.split_once('[') {
        return parse_offering_fqn(offering.trim())
            .ok()
            .map(|fqn| fqn.fqn());
    }

    let idx = query.rfind(':')?;
    let offering = query[..idx].trim();
    parse_offering_fqn(offering).ok().map(|fqn| fqn.fqn())
}

fn projection_ready(projection: &ToolProjection, requirements: &[CapabilitySelector]) -> bool {
    if !projection.ready {
        return false;
    }

    if requirements.is_empty() {
        return true;
    }

    requirements.iter().all(|requirement| {
        let cap_type = requirement.cap_type.to_ascii_lowercase();
        let item = requirement.item.to_ascii_lowercase();
        projection
            .capabilities
            .get(&cap_type)
            .map(|items| {
                items
                    .iter()
                    .any(|candidate| candidate.eq_ignore_ascii_case(&item))
            })
            .unwrap_or(false)
    })
}
