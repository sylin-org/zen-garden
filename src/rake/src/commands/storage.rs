//! Storage commands - seed bank preparation and management
//!
//! Provides CLI commands for USB seed bank onboarding:
//! - `prepare seed-bank` - Format and prepare device as seed bank
//! - `release seed-bank` - Safely unmount seed bank for removal
//! - `show seed-banks` - List all seed banks on stone
//!
//! And S3-compatible object storage:
//! - `store put <bucket> <key> <file>` - Store object
//! - `store get <bucket> <key> [file]` - Retrieve object
//! - `store ls <bucket> [prefix]` - List objects
//! - `store rm <bucket> <key>` - Delete object
//! - `store head <bucket> <key>` - Get object metadata

use crate::commands::{Command, CommandResult};
use crate::context::CommandContext;
use async_trait::async_trait;
use garden_common::api_utils::ApiResponse;
use garden_common::storage::{PrepareSeedBankRequest, SeedBankInfo};
use serde::Deserialize;
use std::io::{self, Write};
use std::path::PathBuf;

// ============================================================================
// Response Types (mirror of API responses)
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct CandidateDevice {
    pub device: String,
    pub capacity_bytes: u64,
    pub label: Option<String>,
    pub state: String,
    pub eligible: bool,
    pub ineligible_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PrepareAcceptedResponse {
    pub accepted: bool,
    pub job_id: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct ReleaseResponse {
    pub released: bool,
    pub name: String,
    pub message: String,
}

// ============================================================================
// Prepare Seed Bank Command
// ============================================================================

pub struct PrepareSeedBankCommand {
    /// Device path (e.g., /dev/sdb, auto for auto-select)
    pub device: Option<String>,
    /// Seed bank name (optional)
    pub name: Option<String>,
    /// Generate random name
    pub random_name: bool,
    /// Filesystem preference
    pub filesystem: String,
    /// Logical group for replicated seed banks
    pub group: Option<String>,
    /// Replica number within a group
    pub replica_id: Option<u32>,
    /// Quiet mode
    pub quiet: bool,
}

impl PrepareSeedBankCommand {
    pub fn new(
        device: Option<String>,
        name: Option<String>,
        random_name: bool,
        filesystem: Option<String>,
        group: Option<String>,
        replica_id: Option<u32>,
    ) -> Self {
        Self {
            device,
            name,
            random_name,
            filesystem: filesystem.unwrap_or_else(|| "btrfs".to_string()),
            group,
            replica_id,
            quiet: false,
        }
    }
}

#[async_trait]
impl Command for PrepareSeedBankCommand {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        use garden_common::ui::rendering as ui;

        let endpoint = ctx
            .endpoint
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Endpoint required for prepare command"))?;

        // First, get list of candidates
        let url = format!(
            "{}/api/v1/stone/storage/candidates",
            endpoint.trim_end_matches('/')
        );
        let response = ctx
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to fetch candidates: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("API error {}: {}", status, text);
        }

        let candidates: Vec<CandidateDevice> = response
            .json::<ApiResponse<Vec<CandidateDevice>>>()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to parse candidates: {}", e))?
            .data;

        // Filter to eligible only
        let eligible: Vec<&CandidateDevice> = candidates.iter().filter(|c| c.eligible).collect();

        if eligible.is_empty() {
            println!(
                "\n{} No eligible devices found.",
                ui::status_indicator("warn", ctx.term.supports_color)
            );
            println!("\nTo prepare a seed bank, insert a USB drive that is:");
            println!("  • Removable (USB, SD card)");
            println!("  • Empty or unformatted");
            println!("  • Not currently in use by Zen Garden");
            return Ok(());
        }

        // Select device
        let device = if let Some(ref d) = self.device {
            if d == "auto" && eligible.len() == 1 {
                eligible[0].device.clone()
            } else {
                d.clone()
            }
        } else if eligible.len() == 1 {
            // Auto-select if only one device
            println!(
                "\n{} Found: {} ({})",
                ui::status_indicator("info", ctx.term.supports_color),
                eligible[0].device,
                format_bytes(eligible[0].capacity_bytes)
            );
            eligible[0].device.clone()
        } else {
            // Interactive selection
            println!(
                "\n{} Multiple devices found:",
                ui::status_indicator("info", ctx.term.supports_color)
            );
            for (i, dev) in eligible.iter().enumerate() {
                let label = dev.label.as_deref().unwrap_or("(no label)");
                println!(
                    "  [{}] {} - {} - {}",
                    i + 1,
                    dev.device,
                    format_bytes(dev.capacity_bytes),
                    label
                );
            }

            print!("\nSelect device [1-{}]: ", eligible.len());
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            let idx: usize = input
                .trim()
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid selection"))?;
            if idx < 1 || idx > eligible.len() {
                anyhow::bail!("Selection out of range");
            }
            eligible[idx - 1].device.clone()
        };

        // Confirm destruction
        println!(
            "\n{} WARNING: This will ERASE ALL DATA on {}",
            ui::status_indicator("warn", ctx.term.supports_color),
            device
        );
        print!("Type 'yes' to continue: ");
        io::stdout().flush()?;

        let mut confirm = String::new();
        io::stdin().read_line(&mut confirm)?;

        if confirm.trim().to_lowercase() != "yes" {
            println!("Cancelled.");
            return Ok(());
        }

        // Build request
        let request = PrepareSeedBankRequest {
            device: device.clone(),
            name: self.name.clone(),
            random_name: self.random_name,
            filesystem: self.filesystem.clone(),
            group: self.group.clone(),
            replica_id: self.replica_id,
        };

        // Submit preparation request
        let url = format!(
            "{}/api/v1/stone/storage/prepare",
            endpoint.trim_end_matches('/')
        );
        let response = ctx
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to submit prepare request: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("Preparation failed ({}): {}", status, text);
        }

        let accepted: PrepareAcceptedResponse = response
            .json::<ApiResponse<PrepareAcceptedResponse>>()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?
            .data;

        if !self.quiet {
            println!(
                "\n{} {}",
                ui::status_indicator("success", ctx.term.supports_color),
                accepted.message
            );
            println!("Job ID: {}", accepted.job_id);
            println!("\nTip: Use 'garden-rake watch' to monitor preparation progress");
        }

        Ok(())
    }

    fn requires_endpoint(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "prepare"
    }
}

// ============================================================================
// Release Seed Bank Command
// ============================================================================

pub struct ReleaseSeedBankCommand {
    /// Seed bank name (or "all" to release all)
    pub name: String,
    /// Quiet mode
    pub quiet: bool,
}

impl ReleaseSeedBankCommand {
    pub fn new(name: String) -> Self {
        Self { name, quiet: false }
    }
}

#[async_trait]
impl Command for ReleaseSeedBankCommand {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        use garden_common::ui::rendering as ui;

        let endpoint = ctx
            .endpoint
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Endpoint required for release command"))?;

        let url = if self.name == "all" {
            format!(
                "{}/api/v1/stone/storage/release-all",
                endpoint.trim_end_matches('/')
            )
        } else {
            format!(
                "{}/api/v1/stone/storage/{}/release",
                endpoint.trim_end_matches('/'),
                self.name
            )
        };

        let response = ctx
            .client
            .post(&url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to release: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("Release failed ({}): {}", status, text);
        }

        if self.name == "all" {
            let results: Vec<ReleaseResponse> = response
                .json::<ApiResponse<Vec<ReleaseResponse>>>()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?
                .data;

            for r in &results {
                if r.released {
                    println!(
                        "{} Released: {}",
                        ui::status_indicator("success", ctx.term.supports_color),
                        r.name
                    );
                } else {
                    println!(
                        "{} Failed: {} - {}",
                        ui::status_indicator("error", ctx.term.supports_color),
                        r.name,
                        r.message
                    );
                }
            }

            println!(
                "\n{} You may now safely remove the devices.",
                ui::status_indicator("info", ctx.term.supports_color)
            );
        } else {
            let result: ReleaseResponse = response
                .json::<ApiResponse<ReleaseResponse>>()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?
                .data;

            if result.released {
                println!(
                    "\n{} {} - You may now safely remove the device.",
                    ui::status_indicator("success", ctx.term.supports_color),
                    result.message
                );
            } else {
                anyhow::bail!("Release failed: {}", result.message);
            }
        }

        Ok(())
    }

    fn requires_endpoint(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "release"
    }
}

// ============================================================================
// Show Seed Banks Command
// ============================================================================

pub struct ShowSeedBanksCommand {
    pub quiet: bool,
}

impl Default for ShowSeedBanksCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl ShowSeedBanksCommand {
    pub fn new() -> Self {
        Self { quiet: false }
    }
}

#[async_trait]
impl Command for ShowSeedBanksCommand {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        use garden_common::ui::rendering as ui;

        let endpoint = ctx
            .endpoint
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Endpoint required for seed-banks command"))?;

        // Fetch seed banks from the new bank API
        let banks_url = format!(
            "{}/api/v1/stone/storage/bank",
            endpoint.trim_end_matches('/')
        );
        let banks_response = ctx
            .client
            .get(&banks_url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to fetch seed banks: {}", e))?;

        if !banks_response.status().is_success() {
            let status = banks_response.status();
            let text = banks_response.text().await.unwrap_or_default();
            anyhow::bail!("API error {}: {}", status, text);
        }

        let seed_banks: Vec<SeedBankInfo> = banks_response
            .json::<ApiResponse<Vec<SeedBankInfo>>>()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?
            .data;

        // Fetch candidates if on Linux (optional, don't fail if unavailable)
        let candidates: Vec<CandidateDevice> = {
            let cand_url = format!(
                "{}/api/v1/stone/storage/candidates",
                endpoint.trim_end_matches('/')
            );
            match ctx.client.get(&cand_url).send().await {
                Ok(resp) if resp.status().is_success() => resp
                    .json::<Vec<CandidateDevice>>()
                    .await
                    .unwrap_or_default(),
                _ => Vec::new(),
            }
        };

        // Display seed banks
        if seed_banks.is_empty() {
            println!(
                "\n{} No seed banks configured.",
                ui::status_indicator("info", ctx.term.supports_color)
            );
        } else {
            println!("\n{}", ui::section_header("SEED BANKS", &ctx.term));
            for sb in &seed_banks {
                let status = if sb.online {
                    ui::status_indicator("success", ctx.term.supports_color)
                } else {
                    ui::status_indicator("warn", ctx.term.supports_color)
                };
                let visibility = format!("{:?}", sb.visibility).to_lowercase();
                println!(
                    "  {} {} ({}) - {} - {}",
                    status,
                    sb.name,
                    visibility,
                    format_bytes(sb.capacity_bytes),
                    if sb.online { "mounted" } else { "offline" }
                );
            }
        }

        // Display candidates
        if !candidates.is_empty() {
            let eligible_candidates: Vec<_> = candidates.iter().filter(|c| c.eligible).collect();
            if !eligible_candidates.is_empty() {
                println!("\n{}", ui::section_header("ELIGIBLE DEVICES", &ctx.term));
                for c in eligible_candidates {
                    println!(
                        "  {} {} - {} - available for preparation",
                        ui::status_indicator("info", ctx.term.supports_color),
                        c.device,
                        format_bytes(c.capacity_bytes)
                    );
                }
            }
        }

        if seed_banks.is_empty() && candidates.iter().all(|c| !c.eligible) {
            println!("\nTip: Insert a USB drive to see available devices");
        }

        Ok(())
    }

    fn requires_endpoint(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "seed-banks"
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn format_bytes(bytes: u64) -> String {
    const GB: u64 = 1024 * 1024 * 1024;
    const MB: u64 = 1024 * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}

// ============================================================================
// S3-Compatible Object Storage Commands
// ============================================================================

/// Default application name for logical namespacing
const DEFAULT_APP_NAME: &str = "zen-garden";

fn apply_app_prefix(bucket: &str, key: &str, app: &str) -> (String, String) {
    let key = key.trim_start_matches('/');
    (app.to_string(), format!("{}/{}", bucket, key))
}

fn apply_app_prefix_for_list(
    bucket: &str,
    prefix: Option<String>,
    app: &str,
) -> (String, Option<String>) {
    let prefix = match prefix {
        Some(p) => format!("{}/{}", bucket, p.trim_start_matches('/')),
        None => format!("{}/", bucket),
    };
    (app.to_string(), Some(prefix))
}

// ============================================================================
// Store Put Command
// ============================================================================

pub struct StorePutCommand {
    pub bucket: String,
    pub key: String,
    pub file: PathBuf,
    pub app: Option<String>,
    pub quiet: bool,
}

impl StorePutCommand {
    pub fn new(bucket: String, key: String, file: PathBuf, app: Option<String>) -> Self {
        Self {
            bucket,
            key,
            file,
            app,
            quiet: false,
        }
    }
}

#[async_trait]
impl Command for StorePutCommand {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        use garden_common::ui::rendering as ui;

        let endpoint = ctx
            .endpoint
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Endpoint required for store command"))?;

        // Read file content
        let data = tokio::fs::read(&self.file)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read file '{}': {}", self.file.display(), e))?;

        // Guess content type
        let content_type = mime_guess::from_path(&self.file)
            .first_or_octet_stream()
            .to_string();

        let app = self.app.as_deref().unwrap_or(DEFAULT_APP_NAME);
        let (bucket, key) = apply_app_prefix(&self.bucket, &self.key, app);
        let url = format!(
            "{}/api/v1/storage/s3/{}/{}",
            endpoint.trim_end_matches('/'),
            bucket,
            key
        );

        let response = ctx
            .client
            .put(&url)
            .header("Content-Type", &content_type)
            .body(data.clone())
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to upload: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("Upload failed ({}): {}", status, text);
        }

        let etag = response
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("(unknown)");

        if !self.quiet {
            println!(
                "{} Stored {} ({} bytes) → {}/{}",
                ui::status_indicator("success", ctx.term.supports_color),
                self.file.display(),
                data.len(),
                self.bucket,
                self.key
            );
            println!("  ETag: {}", etag);
        }

        Ok(())
    }

    fn requires_endpoint(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "store-put"
    }
}

// ============================================================================
// Store Get Command
// ============================================================================

pub struct StoreGetCommand {
    pub bucket: String,
    pub key: String,
    pub output: Option<PathBuf>,
    pub app: Option<String>,
    pub quiet: bool,
}

impl StoreGetCommand {
    pub fn new(bucket: String, key: String, output: Option<PathBuf>, app: Option<String>) -> Self {
        Self {
            bucket,
            key,
            output,
            app,
            quiet: false,
        }
    }
}

#[async_trait]
impl Command for StoreGetCommand {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        use garden_common::ui::rendering as ui;

        let endpoint = ctx
            .endpoint
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Endpoint required for store command"))?;

        let app = self.app.as_deref().unwrap_or(DEFAULT_APP_NAME);
        let (bucket, key) = apply_app_prefix(&self.bucket, &self.key, app);
        let url = format!(
            "{}/api/v1/storage/s3/{}/{}",
            endpoint.trim_end_matches('/'),
            bucket,
            key
        );

        let response = ctx
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to fetch: {}", e))?;

        if response.status().as_u16() == 404 {
            anyhow::bail!("Object not found: {}/{}", self.bucket, self.key);
        }

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("Download failed ({}): {}", status, text);
        }

        // Extract header values before consuming response
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();

        let data = response
            .bytes()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read response: {}", e))?;

        if let Some(ref output) = self.output {
            // Write to file
            if let Some(parent) = output.parent() {
                if !parent.as_os_str().is_empty() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .map_err(|e| anyhow::anyhow!("Failed to create directory: {}", e))?;
                }
            }
            tokio::fs::write(output, &data)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to write file: {}", e))?;

            if !self.quiet {
                println!(
                    "{} Downloaded {}/{} → {} ({} bytes)",
                    ui::status_indicator("success", ctx.term.supports_color),
                    self.bucket,
                    self.key,
                    output.display(),
                    data.len()
                );
            }
        } else {
            // Write to stdout
            if content_type.starts_with("text/") || content_type == "application/json" {
                print!("{}", String::from_utf8_lossy(&data));
            } else {
                std::io::stdout().write_all(&data)?;
            }
        }

        Ok(())
    }

    fn requires_endpoint(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "store-get"
    }
}

// ============================================================================
// Store List Command
// ============================================================================

pub struct StoreListCommand {
    pub bucket: String,
    pub prefix: Option<String>,
    pub delimiter: Option<String>,
    pub app: Option<String>,
    pub quiet: bool,
}

impl StoreListCommand {
    pub fn new(
        bucket: String,
        prefix: Option<String>,
        delimiter: Option<String>,
        app: Option<String>,
    ) -> Self {
        Self {
            bucket,
            prefix,
            delimiter,
            app,
            quiet: false,
        }
    }
}

#[derive(Debug, Deserialize)]
struct ListBucketResult {
    #[serde(rename = "Contents", default)]
    contents: Vec<S3Object>,
    #[serde(rename = "CommonPrefixes", default)]
    common_prefixes: Vec<CommonPrefix>,
    #[serde(rename = "IsTruncated", default)]
    is_truncated: bool,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct S3Object {
    #[serde(rename = "Key")]
    key: String,
    #[serde(rename = "Size")]
    size: u64,
    #[serde(rename = "LastModified")]
    last_modified: String,
    #[serde(rename = "ETag")]
    etag: String,
}

#[derive(Debug, Deserialize)]
struct CommonPrefix {
    #[serde(rename = "Prefix")]
    prefix: String,
}

#[async_trait]
impl Command for StoreListCommand {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        use garden_common::ui::rendering as ui;

        let endpoint = ctx
            .endpoint
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Endpoint required for store command"))?;

        let app = self.app.as_deref().unwrap_or(DEFAULT_APP_NAME);
        let (bucket, prefix) = apply_app_prefix_for_list(&self.bucket, self.prefix.clone(), app);

        let mut query_parts = Vec::new();
        if let Some(ref prefix) = prefix {
            query_parts.push(format!("prefix={}", urlencoding::encode(prefix)));
        }
        if let Some(ref delimiter) = self.delimiter {
            query_parts.push(format!("delimiter={}", urlencoding::encode(delimiter)));
        }

        let query_string = if query_parts.is_empty() {
            String::new()
        } else {
            format!("?{}", query_parts.join("&"))
        };

        let url = format!(
            "{}/api/v1/storage/s3/{}{}",
            endpoint.trim_end_matches('/'),
            bucket,
            query_string
        );

        let response = ctx
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to list: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("List failed ({}): {}", status, text);
        }

        // Parse XML response
        let text = response
            .text()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read response: {}", e))?;

        // Simple XML parsing (the response is well-formed S3 XML)
        let result = parse_list_bucket_result(&text)?;

        if result.contents.is_empty() && result.common_prefixes.is_empty() {
            println!(
                "{} No objects found in bucket '{}'",
                ui::status_indicator("info", ctx.term.supports_color),
                self.bucket
            );
            return Ok(());
        }

        // Display common prefixes (directories)
        for cp in &result.common_prefixes {
            println!("  PRE {}", cp.prefix);
        }

        // Display objects
        for obj in &result.contents {
            println!(
                "{:>12}  {}  {}",
                format_bytes(obj.size),
                &obj.last_modified[..10], // Just the date
                obj.key
            );
        }

        if result.is_truncated {
            println!(
                "\n{} Results truncated. Use marker to continue.",
                ui::status_indicator("warn", ctx.term.supports_color)
            );
        }

        Ok(())
    }

    fn requires_endpoint(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "store-ls"
    }
}

/// Simple XML parser for ListBucketResult
fn parse_list_bucket_result(xml: &str) -> anyhow::Result<ListBucketResult> {
    let mut result = ListBucketResult {
        contents: vec![],
        common_prefixes: vec![],
        is_truncated: false,
    };

    // Parse IsTruncated
    if let Some(start) = xml.find("<IsTruncated>") {
        if let Some(end) = xml[start..].find("</IsTruncated>") {
            let value = &xml[start + 13..start + end];
            result.is_truncated = value == "true";
        }
    }

    // Parse Contents
    let mut search_start = 0;
    while let Some(start) = xml[search_start..].find("<Contents>") {
        let abs_start = search_start + start;
        if let Some(end) = xml[abs_start..].find("</Contents>") {
            let content_xml = &xml[abs_start..abs_start + end + 11];

            let key = extract_xml_value(content_xml, "Key").unwrap_or_default();
            let size = extract_xml_value(content_xml, "Size")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let last_modified = extract_xml_value(content_xml, "LastModified").unwrap_or_default();
            let etag = extract_xml_value(content_xml, "ETag").unwrap_or_default();

            result.contents.push(S3Object {
                key,
                size,
                last_modified,
                etag,
            });
            search_start = abs_start + end + 11;
        } else {
            break;
        }
    }

    // Parse CommonPrefixes
    search_start = 0;
    while let Some(start) = xml[search_start..].find("<CommonPrefixes>") {
        let abs_start = search_start + start;
        if let Some(end) = xml[abs_start..].find("</CommonPrefixes>") {
            let cp_xml = &xml[abs_start..abs_start + end + 17];

            if let Some(prefix) = extract_xml_value(cp_xml, "Prefix") {
                result.common_prefixes.push(CommonPrefix { prefix });
            }
            search_start = abs_start + end + 17;
        } else {
            break;
        }
    }

    Ok(result)
}

fn extract_xml_value(xml: &str, tag: &str) -> Option<String> {
    let open_tag = format!("<{}>", tag);
    let close_tag = format!("</{}>", tag);

    let start = xml.find(&open_tag)?;
    let end = xml[start..].find(&close_tag)?;

    let value = &xml[start + open_tag.len()..start + end];
    Some(unescape_xml(value))
}

fn unescape_xml(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

// ============================================================================
// Store Delete Command
// ============================================================================

pub struct StoreDeleteCommand {
    pub bucket: String,
    pub key: String,
    pub app: Option<String>,
    pub quiet: bool,
}

impl StoreDeleteCommand {
    pub fn new(bucket: String, key: String, app: Option<String>) -> Self {
        Self {
            bucket,
            key,
            app,
            quiet: false,
        }
    }
}

#[async_trait]
impl Command for StoreDeleteCommand {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        use garden_common::ui::rendering as ui;

        let endpoint = ctx
            .endpoint
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Endpoint required for store command"))?;

        let app = self.app.as_deref().unwrap_or(DEFAULT_APP_NAME);
        let (bucket, key) = apply_app_prefix(&self.bucket, &self.key, app);
        let url = format!(
            "{}/api/v1/storage/s3/{}/{}",
            endpoint.trim_end_matches('/'),
            bucket,
            key
        );

        let response = ctx
            .client
            .delete(&url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to delete: {}", e))?;

        if !response.status().is_success() && response.status().as_u16() != 204 {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("Delete failed ({}): {}", status, text);
        }

        if !self.quiet {
            println!(
                "{} Deleted {}/{}",
                ui::status_indicator("success", ctx.term.supports_color),
                self.bucket,
                self.key
            );
        }

        Ok(())
    }

    fn requires_endpoint(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "store-rm"
    }
}

// ============================================================================
// Store Head Command
// ============================================================================

pub struct StoreHeadCommand {
    pub bucket: String,
    pub key: String,
    pub app: Option<String>,
}

impl StoreHeadCommand {
    pub fn new(bucket: String, key: String, app: Option<String>) -> Self {
        Self { bucket, key, app }
    }
}

#[async_trait]
impl Command for StoreHeadCommand {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        use garden_common::ui::rendering as ui;

        let endpoint = ctx
            .endpoint
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Endpoint required for store command"))?;

        let app = self.app.as_deref().unwrap_or(DEFAULT_APP_NAME);
        let (bucket, key) = apply_app_prefix(&self.bucket, &self.key, app);
        let url = format!(
            "{}/api/v1/storage/s3/{}/{}",
            endpoint.trim_end_matches('/'),
            bucket,
            key
        );

        let response = ctx
            .client
            .head(&url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to head: {}", e))?;

        if response.status().as_u16() == 404 {
            anyhow::bail!("Object not found: {}/{}", self.bucket, self.key);
        }

        if !response.status().is_success() {
            anyhow::bail!("HEAD failed: {}", response.status());
        }

        let headers = response.headers();

        let content_type = headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("(unknown)");
        let content_length = headers
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("(unknown)");
        let etag = headers
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("(unknown)");
        let last_modified = headers
            .get("last-modified")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("(unknown)");

        println!(
            "{} {}/{}",
            ui::status_indicator("info", ctx.term.supports_color),
            self.bucket,
            self.key
        );
        println!("  Content-Type:   {}", content_type);
        println!("  Content-Length: {} bytes", content_length);
        println!("  ETag:           {}", etag);
        println!("  Last-Modified:  {}", last_modified);

        Ok(())
    }

    fn requires_endpoint(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "store-head"
    }
}
