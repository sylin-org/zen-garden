//! Capabilities command - discover and manage offering capabilities
//!
//! Lists, adds, and removes capabilities (models, extensions, modules, etc.) for an offering.

use crate::command_manifest::cmd;
use crate::commands::{Command, CommandResult};
use crate::context::CommandContext;
use crate::suggestions;
use anyhow::{bail, Context};
use async_trait::async_trait;
use garden_common::api_utils::ApiResponse;
use garden_common::ui::rendering::{self as ui};
use garden_common::{CapabilityCollection, OfferingMode};
use serde::{Deserialize, Serialize};

/// Response from capabilities endpoint
#[derive(Debug, Deserialize)]
pub struct CapabilitiesResponse {
    pub offering: String,
    pub mode: OfferingMode,
    pub capabilities: Vec<CapabilityCollection>,
}

/// List capabilities for an offering
pub struct CapabilitiesCommand {
    /// Offering name to query
    pub offering: String,
    /// Quiet mode (no suggestions)
    pub quiet_mode: bool,
}

impl CapabilitiesCommand {
    pub fn new(offering: String, quiet_mode: bool) -> Self {
        Self {
            offering,
            quiet_mode,
        }
    }
}

#[async_trait]
impl Command for CapabilitiesCommand {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        let offering_path = urlencoding::encode(&self.offering);
        let url = ctx.api_v1_url(&format!("stone/offerings/{}/capabilities", offering_path))?;
        let response = ctx.client.get(&url).send().await?;

        // Check for error status
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();

            // Try to parse error response
            if let Ok(error) =
                serde_json::from_str::<garden_common::api_utils::ApiErrorResponse>(&body)
            {
                eprintln!(
                    "{} {}",
                    ui::status_indicator("error", ctx.term.supports_color),
                    error.error.message
                );
                return Ok(());
            }

            anyhow::bail!("Request failed with status {}: {}", status, body);
        }

        let api_response: ApiResponse<CapabilitiesResponse> = response
            .json()
            .await
            .context("Failed to parse capabilities response")?;

        let data = api_response.data;

        // Header
        let mode_str = match data.mode {
            OfferingMode::Managed => "managed",
            OfferingMode::Adopted => "adopted",
            OfferingMode::Borrowed => "borrowed",
        };

        println!(
            "{}",
            ui::section_header(
                &format!(
                    "{} CAPABILITIES ({})",
                    data.offering.to_uppercase(),
                    mode_str
                ),
                &ctx.term
            )
        );
        println!();

        // Show each capability type
        for collection in &data.capabilities {
            // Subsection header
            println!(
                "  {} ({})",
                collection.display.plural.to_uppercase(),
                collection.items.len()
            );
            println!();

            if collection.items.is_empty() {
                println!("    (none)");
            } else {
                // Build table for items
                let mut table = ui::TableBuilder::new()
                    .add_column(40, ui::Align::Left) // Name
                    .add_column(12, ui::Align::Right); // Size

                for item in &collection.items {
                    let size_str = item.size.clone().unwrap_or_default();
                    table.add_row(vec![item.name.clone(), size_str]);
                }

                for line in table.render().lines() {
                    println!("    {}", line);
                }
            }
            println!();
        }

        if data.capabilities.is_empty() {
            println!(
                "{}",
                ui::empty_state(
                    "No capabilities found",
                    Some("The offering may not support capability discovery")
                )
            );
        }

        // Self-teaching suggestions
        suggestions::print_suggestions(cmd::CAPABILITIES, self.quiet_mode);

        Ok(())
    }

    fn name(&self) -> &'static str {
        cmd::CAPABILITIES
    }
}

/// Response from capability mutation endpoints
#[derive(Debug, Deserialize)]
pub struct CapabilityMutationResponse {
    pub success: bool,
    pub capability: String,
    pub operation: String,
    #[serde(default)]
    pub error: Option<String>,
}

/// Request body for adding a capability
#[derive(Debug, Serialize)]
pub struct AddCapabilityRequest {
    pub name: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub cap_type: Option<String>,
    #[serde(default)]
    pub dry_run: bool,
}

/// Response for add capability operation (job-based)
#[derive(Debug, Deserialize)]
#[serde(tag = "status")]
pub enum AddCapabilityResponse {
    /// Capability already exists
    #[serde(rename = "exists")]
    AlreadyExists {
        offering: String,
        capability: String,
        cap_type: String,
        message: String,
    },

    /// Dry run - validation passed
    #[serde(rename = "dry_run")]
    DryRun {
        offering: String,
        capability: String,
        cap_type: String,
        message: String,
    },

    /// Job already running
    #[serde(rename = "in_progress")]
    InProgress {
        offering: String,
        capability: String,
        job_id: String,
        message: String,
    },

    /// Job started
    #[serde(rename = "started")]
    Started {
        offering: String,
        capability: String,
        job_id: String,
        message: String,
    },
}

/// Add a capability to an offering
pub struct AddCapabilityCommand {
    /// Offering name
    pub offering: String,
    /// Capability name to add
    pub name: String,
    /// Capability type (optional)
    pub cap_type: Option<String>,
    /// Dry run mode
    pub dry_run: bool,
    /// Quiet mode
    pub quiet_mode: bool,
}

impl AddCapabilityCommand {
    pub fn new(
        offering: String,
        name: String,
        cap_type: Option<String>,
        dry_run: bool,
        quiet_mode: bool,
    ) -> Self {
        Self {
            offering,
            name,
            cap_type,
            dry_run,
            quiet_mode,
        }
    }
}

#[async_trait]
impl Command for AddCapabilityCommand {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        let action = if self.dry_run { "Validating" } else { "Adding" };
        println!(
            "{} {} {} to {}...",
            ui::status_indicator("info", ctx.term.supports_color),
            action,
            self.name,
            self.offering
        );

        let offering_path = urlencoding::encode(&self.offering);
        let url = ctx.api_v1_url(&format!("stone/offerings/{}/capabilities", offering_path))?;
        let request_body = AddCapabilityRequest {
            name: self.name.clone(),
            cap_type: self.cap_type.clone(),
            dry_run: self.dry_run,
        };

        let response = ctx.client.post(&url).json(&request_body).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();

            if let Ok(error) =
                serde_json::from_str::<garden_common::api_utils::ApiErrorResponse>(&body)
            {
                eprintln!(
                    "{} {}",
                    ui::status_indicator("error", ctx.term.supports_color),
                    error.error.message
                );
                return Ok(());
            }

            anyhow::bail!("Request failed with status {}: {}", status, body);
        }

        let api_response: ApiResponse<AddCapabilityResponse> =
            response.json().await.context("Failed to parse response")?;

        match api_response.data {
            AddCapabilityResponse::AlreadyExists {
                offering,
                capability,
                cap_type,
                message,
            } => {
                println!(
                    "{} {} '{}' already exists in {}",
                    ui::status_indicator("info", ctx.term.supports_color),
                    cap_type,
                    capability,
                    offering
                );
                println!("  {}", message);
            }

            AddCapabilityResponse::DryRun {
                offering,
                capability,
                cap_type,
                message,
            } => {
                println!(
                    "\n{} DRY RUN - No changes made",
                    ui::status_indicator("info", ctx.term.supports_color)
                );
                println!();
                println!(
                    "  {} '{}' can be added to {}",
                    cap_type, capability, offering
                );
                println!("  {}", message);
            }

            AddCapabilityResponse::InProgress {
                offering,
                capability,
                job_id,
                message,
            } => {
                println!(
                    "{} Add operation already in progress",
                    ui::status_indicator("info", ctx.term.supports_color)
                );
                println!();
                println!("  Offering:   {}", offering);
                println!("  Capability: {}", capability);
                println!("  Job ID:     {}", &job_id[..16.min(job_id.len())]);
                println!();
                println!("  {}", message);
            }

            AddCapabilityResponse::Started {
                offering,
                capability,
                job_id,
                message,
            } => {
                println!(
                    "{} {}",
                    ui::status_indicator("success", ctx.term.supports_color),
                    message
                );
                println!();
                println!("  Offering:   {}", offering);
                println!("  Capability: {}", capability);
                println!("  Job ID:     {}", &job_id[..16.min(job_id.len())]);
                println!();
                println!("  The download is running in the background.");
                println!(
                    "  Use 'rake capabilities {}' to verify when complete.",
                    offering
                );
            }
        }

        suggestions::print_suggestions(cmd::CAPABILITIES, self.quiet_mode);
        Ok(())
    }

    fn name(&self) -> &'static str {
        "capabilities-add"
    }
}

/// Remove a capability from an offering
pub struct RemoveCapabilityCommand {
    /// Offering name
    pub offering: String,
    /// Capability name to remove
    pub name: String,
    /// Capability type (optional)
    pub cap_type: Option<String>,
    /// Quiet mode
    pub quiet_mode: bool,
}

impl RemoveCapabilityCommand {
    pub fn new(offering: String, name: String, cap_type: Option<String>, quiet_mode: bool) -> Self {
        Self {
            offering,
            name,
            cap_type,
            quiet_mode,
        }
    }
}

#[async_trait]
impl Command for RemoveCapabilityCommand {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        println!(
            "{} Removing {} from {}...",
            ui::status_indicator("info", ctx.term.supports_color),
            self.name,
            self.offering
        );

        let offering_path = urlencoding::encode(&self.offering);
        let mut url = ctx.api_v1_url(&format!(
            "stone/offerings/{}/capabilities/{}",
            offering_path,
            urlencoding::encode(&self.name)
        ))?;

        // Add type query parameter if specified
        if let Some(ref cap_type) = self.cap_type {
            url = format!("{}?type={}", url, urlencoding::encode(cap_type));
        }

        let response = ctx.client.delete(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();

            if let Ok(error) =
                serde_json::from_str::<garden_common::api_utils::ApiErrorResponse>(&body)
            {
                eprintln!(
                    "{} {}",
                    ui::status_indicator("error", ctx.term.supports_color),
                    error.error.message
                );
                return Ok(());
            }

            anyhow::bail!("Request failed with status {}: {}", status, body);
        }

        let api_response: ApiResponse<CapabilityMutationResponse> =
            response.json().await.context("Failed to parse response")?;

        let data = api_response.data;

        if data.success {
            println!(
                "{} Successfully removed '{}' from {}",
                ui::status_indicator("success", ctx.term.supports_color),
                data.capability,
                self.offering
            );
        } else {
            eprintln!(
                "{} Failed to remove '{}': {}",
                ui::status_indicator("error", ctx.term.supports_color),
                data.capability,
                data.error.unwrap_or_else(|| "Unknown error".to_string())
            );
        }

        suggestions::print_suggestions(cmd::CAPABILITIES, self.quiet_mode);
        Ok(())
    }

    fn name(&self) -> &'static str {
        "capabilities-remove"
    }
}

/// Response from refresh capabilities endpoint (tagged enum)
#[derive(Debug, Deserialize)]
#[serde(tag = "status")]
pub enum RefreshCapabilitiesResponse {
    /// No capabilities found to refresh
    #[serde(rename = "no_updates")]
    NoUpdates {
        offering: String,
        cap_type: Option<String>,
        message: String,
    },

    /// Dry run - showing what would be refreshed
    #[serde(rename = "dry_run")]
    DryRun {
        offering: String,
        capabilities: Vec<CapabilityToRefresh>,
        total: usize,
    },

    /// Job already running - return current progress
    #[serde(rename = "in_progress")]
    InProgress {
        offering: String,
        job_id: String,
        progress_percent: u8,
        completed: usize,
        failed: usize,
        total: usize,
    },

    /// Job started - return job_id for tracking
    #[serde(rename = "started")]
    Started {
        offering: String,
        job_id: String,
        total: usize,
        message: String,
    },
}

/// Capability that would be refreshed (for dry run)
#[derive(Debug, Deserialize)]
pub struct CapabilityToRefresh {
    pub name: String,
    pub cap_type: String,
}

/// Request body for refreshing capabilities
#[derive(Debug, Serialize)]
pub struct RefreshCapabilitiesRequest {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub cap_type: Option<String>,
    pub dry_run: bool,
}

/// Request body for mirroring capabilities
#[derive(Debug, Serialize)]
pub struct MirrorCapabilitiesRequest {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub dry_run: bool,
}

/// Failure details when mirroring capabilities
#[derive(Debug, Deserialize)]
pub struct MirrorCapabilityFailure {
    pub name: String,
    pub cap_type: String,
    pub error: String,
}

/// Response from mirror capabilities endpoint
#[derive(Debug, Deserialize)]
pub struct MirrorCapabilitiesResponse {
    pub offering: String,
    pub from: String,
    pub to: String,
    pub added: usize,
    pub skipped: usize,
    #[serde(default)]
    pub failed: usize,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub failures: Vec<MirrorCapabilityFailure>,
}

/// Refresh/update all capabilities for an offering
pub struct RefreshCapabilitiesCommand {
    /// Offering name
    pub offering: String,
    /// Capability type to refresh (optional)
    pub cap_type: Option<String>,
    /// Dry run mode
    pub dry_run: bool,
    /// Quiet mode
    pub quiet_mode: bool,
}

/// Mirror capabilities from one stone to another
pub struct MirrorCapabilitiesCommand {
    /// Offering name
    pub offering: String,
    /// Raw mirror args (from/to pairs)
    pub args: Vec<String>,
    /// Quiet mode
    pub quiet_mode: bool,
}

impl MirrorCapabilitiesCommand {
    pub fn new(offering: String, args: Vec<String>, quiet_mode: bool) -> Self {
        Self {
            offering,
            args,
            quiet_mode,
        }
    }
}

impl RefreshCapabilitiesCommand {
    pub fn new(
        offering: String,
        cap_type: Option<String>,
        dry_run: bool,
        quiet_mode: bool,
    ) -> Self {
        Self {
            offering,
            cap_type,
            dry_run,
            quiet_mode,
        }
    }
}

#[async_trait]
impl Command for RefreshCapabilitiesCommand {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        let action = if self.dry_run {
            "Checking"
        } else {
            "Refreshing"
        };
        println!(
            "{} {} capabilities for {}...",
            ui::status_indicator("info", ctx.term.supports_color),
            action.to_lowercase(),
            self.offering
        );

        let offering_path = urlencoding::encode(&self.offering);
        let url = ctx.api_v1_url(&format!(
            "stone/offerings/{}/capabilities/refresh",
            offering_path
        ))?;
        let request_body = RefreshCapabilitiesRequest {
            cap_type: self.cap_type.clone(),
            dry_run: self.dry_run,
        };

        let response = ctx.client.post(&url).json(&request_body).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();

            if let Ok(error) =
                serde_json::from_str::<garden_common::api_utils::ApiErrorResponse>(&body)
            {
                eprintln!(
                    "{} {}",
                    ui::status_indicator("error", ctx.term.supports_color),
                    error.error.message
                );
                return Ok(());
            }

            anyhow::bail!("Request failed with status {}: {}", status, body);
        }

        let api_response: ApiResponse<RefreshCapabilitiesResponse> =
            response.json().await.context("Failed to parse response")?;

        match api_response.data {
            RefreshCapabilitiesResponse::NoUpdates {
                offering,
                cap_type,
                message,
            } => {
                let type_label = cap_type.as_deref().unwrap_or("capabilities");
                println!(
                    "{} No {} to refresh for {}",
                    ui::status_indicator("info", ctx.term.supports_color),
                    type_label,
                    offering
                );
                println!("  {}", message);
            }

            RefreshCapabilitiesResponse::DryRun {
                offering,
                capabilities,
                total,
            } => {
                println!(
                    "\n{} DRY RUN - No changes made",
                    ui::status_indicator("info", ctx.term.supports_color)
                );
                println!();
                println!(
                    "{}",
                    ui::section_header(
                        &format!("{} WOULD REFRESH", offering.to_uppercase()),
                        &ctx.term
                    )
                );
                println!();

                if capabilities.is_empty() {
                    println!("  No capabilities found to refresh.");
                } else {
                    let mut table = ui::TableBuilder::new()
                        .add_column(35, ui::Align::Left) // Name
                        .add_column(12, ui::Align::Left); // Type

                    for cap in &capabilities {
                        table.add_row(vec![cap.name.clone(), cap.cap_type.clone()]);
                    }

                    for line in table.render().lines() {
                        println!("    {}", line);
                    }
                }

                println!();
                println!(
                    "{} {} capabilities would be refreshed",
                    ui::status_indicator("info", ctx.term.supports_color),
                    total
                );
            }

            RefreshCapabilitiesResponse::InProgress {
                offering,
                job_id,
                progress_percent,
                completed,
                failed,
                total,
            } => {
                println!(
                    "{} Refresh already in progress for {}",
                    ui::status_indicator("info", ctx.term.supports_color),
                    offering
                );
                println!();
                println!("  Job ID:   {}", &job_id[..16.min(job_id.len())]);
                println!("  Progress: {}%", progress_percent);
                println!(
                    "  Status:   {}/{} completed, {} failed",
                    completed, total, failed
                );
                println!();
                println!(
                    "  Run again later to check progress, or use: rake jobs {}",
                    &job_id[..8.min(job_id.len())]
                );
            }

            RefreshCapabilitiesResponse::Started {
                offering,
                job_id,
                total,
                message,
            } => {
                println!(
                    "{} {}",
                    ui::status_indicator("success", ctx.term.supports_color),
                    message
                );
                println!();
                println!("  Offering: {}", offering);
                println!("  Job ID:   {}", &job_id[..16.min(job_id.len())]);
                println!("  Total:    {} capabilities", total);
                println!();
                println!(
                    "  Run 'rake capabilities refresh {}' again to check progress",
                    offering
                );
            }
        }

        suggestions::print_suggestions(cmd::CAPABILITIES, self.quiet_mode);
        Ok(())
    }

    fn name(&self) -> &'static str {
        "capabilities-refresh"
    }
}

#[async_trait]
impl Command for MirrorCapabilitiesCommand {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        let (from_arg, to_arg) = parse_mirror_targets(&self.args)?;
        let local_stone = ctx.stone_name.clone();

        let from = match from_arg {
            Some(value) => value,
            None => local_stone.clone().ok_or_else(|| {
                anyhow::anyhow!(
                    "Mirror requires a source stone. Use 'from <stone>' or tend a stone first."
                )
            })?,
        };

        let to = match to_arg {
            Some(value) => value,
            None => local_stone.ok_or_else(|| {
                anyhow::anyhow!(
                    "Mirror requires a destination stone. Use 'to <stone>' or tend a stone first."
                )
            })?,
        };

        if from == to {
            bail!("Mirror source and destination are the same ('{}').", from);
        }

        println!(
            "{} Mirroring capabilities for {} ({} → {})...",
            ui::status_indicator("info", ctx.term.supports_color),
            self.offering,
            from,
            to
        );

        let offering_path = urlencoding::encode(&self.offering);
        let url = ctx.api_v1_url(&format!(
            "stone/offerings/{}/capabilities/mirror",
            offering_path
        ))?;
        let request_body = MirrorCapabilitiesRequest {
            from: from.clone(),
            to: to.clone(),
            dry_run: false,
        };

        let response = ctx.client.post(&url).json(&request_body).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();

            if let Ok(error) =
                serde_json::from_str::<garden_common::api_utils::ApiErrorResponse>(&body)
            {
                eprintln!(
                    "{} {}",
                    ui::status_indicator("error", ctx.term.supports_color),
                    error.error.message
                );
                return Ok(());
            }

            anyhow::bail!("Request failed with status {}: {}", status, body);
        }

        let api_response: ApiResponse<MirrorCapabilitiesResponse> = response
            .json()
            .await
            .context("Failed to parse mirror response")?;

        let data = api_response.data;

        if let Some(message) = &data.message {
            println!("  {}", message);
        }

        println!("  Offering: {}", data.offering);
        println!("  From:     {}", data.from);
        println!("  To:       {}", data.to);
        println!("  Added:    {}", data.added);
        println!("  Skipped:  {}", data.skipped);
        if data.failed > 0 {
            println!("  Failed:   {}", data.failed);
        }

        if !data.failures.is_empty() {
            println!();
            println!("{}", ui::section_header("FAILURES", &ctx.term));
            for failure in &data.failures {
                println!("  {} {}: {}", failure.cap_type, failure.name, failure.error);
            }
        }

        suggestions::print_suggestions(cmd::CAPABILITIES, self.quiet_mode);
        Ok(())
    }

    fn name(&self) -> &'static str {
        "capabilities-mirror"
    }
}

fn parse_mirror_targets(args: &[String]) -> anyhow::Result<(Option<String>, Option<String>)> {
    let mut from = None;
    let mut to = None;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "from" => {
                i += 1;
                if i >= args.len() {
                    bail!("'from' requires a stone name");
                }
                from = Some(args[i].clone());
            }
            "to" => {
                i += 1;
                if i >= args.len() {
                    bail!("'to' requires a stone name");
                }
                to = Some(args[i].clone());
            }
            other => {
                bail!("Unexpected token '{}' in mirror command. Use 'from <stone>' and/or 'to <stone>'.", other);
            }
        }
        i += 1;
    }

    if from.is_none() && to.is_none() {
        bail!("Mirror requires a source or destination stone.");
    }

    Ok((from, to))
}
