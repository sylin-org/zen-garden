//! Storage commands — unified storage management (STORAGE-0010)
//!
//! All storage operations live under `garden-rake storage`:
//! - `storage` (bare) — List all storages in the garden
//! - `storage add` — Add a device or directory
//! - `storage list` — List all storages and eligible devices
//! - `storage status` — Detailed capacity/health breakdown
//! - `storage release` — Safely unmount for removal
//! - `storage pin` — Claim Primary role
//! - `storage unpin` — Release Primary role
//!
//! S3-compatible object storage (separate `store` command):
//! - `store put <bucket> <key> <file>` — Store object
//! - `store get <bucket> <key> [file]` — Retrieve object
//! - `store ls <bucket> [prefix]` — List objects
//! - `store rm <bucket> <key>` — Delete object
//! - `store head <bucket> <key>` — Get object metadata

use crate::commands::{Command, CommandResult};
use crate::context::Runtime;
use async_trait::async_trait;
use garden_common::api_utils::ApiResponse;
use garden_common::storage::{
    AddStorageRequest, CandidatesResponse, MediumAction, StorageInfo, StorageRole,
};
use serde::Deserialize;
use std::io::{self, Write};
use std::path::PathBuf;

/// Storage overview response from GET /api/v1/stone/storage
#[derive(Debug, Deserialize)]
pub struct StorageOverview {
    pub bank_count: usize,
    pub garden_banks: Vec<GardenBankInfo>,
}

/// Garden-wide bank info from overview
#[derive(Debug, Deserialize)]
pub struct GardenBankInfo {
    pub id: String,
    /// Replica set display name (user-facing identity)
    pub name: String,
    /// Individual volume/device name
    #[serde(default)]
    pub volume_name: String,
    /// Replica set ID (STORAGE-0013).
    #[serde(default)]
    pub replica_set_id: String,
    /// Replica set display name (STORAGE-0013).
    #[serde(default)]
    pub replica_set_name: String,
    pub stone_name: String,
    pub is_local: bool,
    pub capacity_bytes: u64,
    #[serde(default)]
    pub role: StorageRole,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub encrypted: bool,
    #[serde(default)]
    pub roles: Vec<String>,
}

/// Response from add storage endpoint
#[derive(Debug, Deserialize)]
pub struct AddStorageResponseData {
    pub id: String,
    pub name: String,
    pub mount_path: String,
    #[serde(default)]
    pub formatted: bool,
    #[serde(default)]
    pub cataloged: usize,
    pub job_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReleaseResponse {
    pub released: bool,
    pub name: String,
    pub message: String,
}

// ============================================================================
// Add Storage Runtime (STORAGE-0010)
// ============================================================================

pub struct AddStorageCommand {
    /// Target — block device path (e.g. /dev/sdb) or directory path
    pub target: Option<String>,
    /// Human-readable name (zen: `as <name>`)
    pub name: Option<String>,
    /// Roles to assign (zen: `role <role>`)
    pub roles: Vec<String>,
    /// Format the device before adding (destructive)
    pub format: bool,
    /// Filesystem preference (only when format=true)
    pub filesystem: String,
    /// Encrypt content
    pub encrypted: bool,
    /// Skip confirmation prompt
    pub yes: bool,
    /// Quiet mode
    pub quiet: bool,
}

impl AddStorageCommand {
    pub fn new(
        target: Option<String>,
        name: Option<String>,
        roles: Vec<String>,
        format: bool,
        filesystem: Option<String>,
        encrypted: bool,
        yes: bool,
    ) -> Self {
        Self {
            target,
            name,
            roles,
            format,
            filesystem: filesystem.unwrap_or_else(|| "btrfs".to_string()),
            encrypted,
            yes,
            quiet: false,
        }
    }
}

#[async_trait]
impl Command for AddStorageCommand {
    async fn execute(&self, ctx: &Runtime) -> CommandResult {
        use crate::ui::rendering as ui;

        let endpoint = ctx
            .endpoint
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Endpoint required for storage add command"))?;
        let base = endpoint.trim_end_matches('/');

        // Resolve target — if not provided, list candidates and let user pick
        let target = match &self.target {
            Some(t) => t.clone(),
            None => self.pick_target(ctx, base).await?,
        };

        let is_block_device = target.starts_with("/dev/");

        // Destructive format confirmation
        if is_block_device && self.format && !self.yes {
            println!(
                "\n{} WARNING: This will FORMAT and ERASE ALL DATA on {}",
                ui::status_indicator("warn", ctx.term.supports_color),
                target
            );
            print!("Type 'yes' to continue: ");
            io::stdout().flush()?;

            let mut confirm = String::new();
            io::stdin().read_line(&mut confirm)?;

            if confirm.trim().to_lowercase() != "yes" {
                println!("Cancelled.");
                return Ok(());
            }
        }

        // Build unified request
        let request = AddStorageRequest {
            target: target.clone(),
            name: self.name.clone(),
            format: self.format,
            filesystem: self.filesystem.clone(),
            encrypted: self.encrypted,
            roles: self.roles.clone(),
        };

        // POST /api/v1/stone/storage/add
        let url = format!("{}/api/v1/stone/storage/add", base);
        let response = ctx
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to submit storage add request: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("Storage add failed ({}): {}", status, text);
        }

        let result: AddStorageResponseData = response
            .json::<ApiResponse<AddStorageResponseData>>()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?
            .data;

        if !self.quiet {
            println!(
                "\n{} Storage '{}' added successfully",
                ui::status_indicator("success", ctx.term.supports_color),
                result.name
            );
            println!("  Mount: {}", result.mount_path);
            if result.formatted {
                println!("  Formatted: yes ({})", self.filesystem);
            }
            if result.cataloged > 0 {
                println!("  Cataloged: {} existing items", result.cataloged);
            }
            if let Some(ref job_id) = result.job_id {
                println!("  Job ID: {}", job_id);
                println!("\nTip: Use 'garden-rake watch' to monitor format progress");
            }
        }

        Ok(())
    }

    fn requires_endpoint(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "storage-add"
    }
}

impl AddStorageCommand {
    /// Interactive device selection when no target provided.
    async fn pick_target(&self, ctx: &Runtime, base: &str) -> anyhow::Result<String> {
        use crate::ui::rendering as ui;

        let url = format!("{}/api/v1/stone/storage/candidates", base);
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

        let resp: CandidatesResponse = response
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to parse candidates: {}", e))?;

        // Build a flat list of eligible targets from spaces (mounted volumes).
        struct EligibleTarget {
            device: String,
            capacity_bytes: u64,
            label: String,
        }
        let eligible: Vec<EligibleTarget> = resp
            .spaces
            .iter()
            .filter(|s| s.eligible)
            .map(|s| EligibleTarget {
                device: s.device.clone(),
                capacity_bytes: s.capacity_bytes,
                label: s.label.as_deref().unwrap_or("(no label)").to_string(),
            })
            .collect();

        if eligible.is_empty() {
            // Check if there are media that need action
            let actionable: Vec<_> = resp
                .media
                .iter()
                .filter(|m| matches!(m.suggested_action, MediumAction::NeedsPartition | MediumAction::NeedsFormat))
                .collect();
            if !actionable.is_empty() {
                let mut msg = String::from("No ready volumes found, but detected physical media:\n");
                for m in &actionable {
                    msg.push_str(&format!(
                        "  {} ({}) — {}\n",
                        m.model.as_deref().unwrap_or(&m.device_id),
                        format_bytes(m.size_bytes),
                        m.suggested_action,
                    ));
                }
                msg.push_str("\nPartition and format these disks first, then try again.");
                anyhow::bail!(msg);
            }
            anyhow::bail!(
                "No eligible devices found. Insert a USB drive or specify a directory path."
            );
        }

        if eligible.len() == 1 {
            println!(
                "\n{} Found: {} ({})",
                ui::status_indicator("info", ctx.term.supports_color),
                eligible[0].device,
                format_bytes(eligible[0].capacity_bytes)
            );
            return Ok(eligible[0].device.clone());
        }

        // Interactive selection
        println!(
            "\n{} Multiple devices found:",
            ui::status_indicator("info", ctx.term.supports_color)
        );
        for (i, dev) in eligible.iter().enumerate() {
            println!(
                "  [{}] {} - {} - {}",
                i + 1,
                dev.device,
                format_bytes(dev.capacity_bytes),
                dev.label,
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
        Ok(eligible[idx - 1].device.clone())
    }
}

// ============================================================================
// Release Storage Runtime
// ============================================================================

pub struct ReleaseStorageCommand {
    /// Storage name (or "all" to release all)
    pub name: String,
    /// Quiet mode
    pub quiet: bool,
}

impl ReleaseStorageCommand {
    pub fn new(name: String) -> Self {
        Self { name, quiet: false }
    }
}

#[async_trait]
impl Command for ReleaseStorageCommand {
    async fn execute(&self, ctx: &Runtime) -> CommandResult {
        use crate::ui::rendering as ui;

        let endpoint = ctx
            .endpoint
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Endpoint required for release command"))?;
        let base = endpoint.trim_end_matches('/');

        // "all" → bulk release
        if self.name == "all" {
            let url = format!("{}/api/v1/stone/storage/release-all", base);
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
            return Ok(());
        }

        let url = format!("{}/api/v1/stone/storage/banks/{}/release", base, self.name);
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
// List Storage Runtime
// ============================================================================

pub struct ListStorageCommand {
    pub quiet: bool,
}

impl Default for ListStorageCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl ListStorageCommand {
    pub fn new() -> Self {
        Self { quiet: false }
    }
}

#[async_trait]
impl Command for ListStorageCommand {
    async fn execute(&self, ctx: &Runtime) -> CommandResult {
        use crate::ui::rendering as ui;

        let endpoint = ctx
            .endpoint
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Endpoint required for storage list command"))?;

        // Fetch local storages
        let banks_url = format!(
            "{}/api/v1/stone/storage/banks",
            endpoint.trim_end_matches('/')
        );
        let banks_response = ctx
            .client
            .get(&banks_url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to fetch storages: {}", e))?;

        if !banks_response.status().is_success() {
            let status = banks_response.status();
            let text = banks_response.text().await.unwrap_or_default();
            anyhow::bail!("API error {}: {}", status, text);
        }

        let storages: Vec<StorageInfo> = banks_response
            .json::<ApiResponse<Vec<StorageInfo>>>()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?
            .data;

        // Fetch garden-wide overview (includes roles from beacons)
        let overview_url = format!("{}/api/v1/stone/storage", endpoint.trim_end_matches('/'));
        let garden_banks: Vec<GardenBankInfo> = match ctx.client.get(&overview_url).send().await {
            Ok(resp) if resp.status().is_success() => resp
                .json::<ApiResponse<StorageOverview>>()
                .await
                .map(|r| r.data.garden_banks)
                .unwrap_or_default(),
            _ => Vec::new(),
        };

        // Fetch candidates (cross-platform)
        let candidates: Option<CandidatesResponse> = {
            let cand_url = format!(
                "{}/api/v1/stone/storage/candidates",
                endpoint.trim_end_matches('/')
            );
            match ctx.client.get(&cand_url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    resp.json::<CandidatesResponse>().await.ok()
                }
                _ => None,
            }
        };

        // Display storages — grouped by replica set name with volumes underneath
        if storages.is_empty() && garden_banks.is_empty() {
            println!(
                "\n{} No managed storage found.",
                ui::status_indicator("info", ctx.term.supports_color)
            );
        } else if !garden_banks.is_empty() {
            // Group by replica set name (user-facing identity)
            let mut by_name: std::collections::BTreeMap<String, Vec<&GardenBankInfo>> =
                std::collections::BTreeMap::new();
            for gb in &garden_banks {
                by_name.entry(gb.name.clone()).or_default().push(gb);
            }

            println!("\n{}", ui::section_header("STORAGE", &ctx.term));
            for (name, replicas) in &by_name {
                let replica_count = replicas.len();
                let is_encrypted = replicas.iter().any(|r| r.encrypted);
                let is_pinned = replicas.iter().any(|r| r.pinned);

                let enc_label = if is_encrypted { " [encrypted]" } else { "" };
                let pin_label = if is_pinned { " pinned" } else { "" };

                // Total capacity across all volumes in this replica set
                let total_cap: u64 = replicas.iter().map(|r| r.capacity_bytes).sum();
                let cap_str = format_bytes(total_cap);

                println!(
                    "\n  {} {}  ({} volume{}, {}){}{}",
                    ui::status_indicator("success", ctx.term.supports_color),
                    name,
                    replica_count,
                    if replica_count == 1 { "" } else { "s" },
                    cap_str,
                    enc_label,
                    pin_label,
                );

                for r in replicas {
                    let vol_name = if r.volume_name.is_empty() {
                        &r.id[..8.min(r.id.len())]
                    } else {
                        &r.volume_name
                    };
                    let role_tag = match r.role {
                        StorageRole::Primary => " [primary]",
                        StorageRole::Dormant => " [dormant]",
                    };
                    let cap = format_bytes(r.capacity_bytes);
                    println!(
                        "      {} — {} ({}){}",
                        vol_name,
                        r.stone_name,
                        cap,
                        role_tag,
                    );
                }
            }
        } else {
            // Fallback: only local storages (no garden view)
            println!("\n{}", ui::section_header("STORAGE", &ctx.term));
            for sb in &storages {
                let status = if sb.online {
                    ui::status_indicator("success", ctx.term.supports_color)
                } else {
                    ui::status_indicator("warn", ctx.term.supports_color)
                };
                let visibility = format!("{:?}", sb.visibility).to_lowercase();
                let enc = if sb.encrypted { " [encrypted]" } else { "" };
                println!(
                    "  {} {} ({}) - {} - {}{}",
                    status,
                    sb.name,
                    visibility,
                    format_bytes(sb.capacity_bytes),
                    if sb.online { "mounted" } else { "offline" },
                    enc,
                );
            }
        }

        // Display candidates
        if let Some(ref cands) = candidates {
            // Ready volumes (can be added immediately)
            let eligible: Vec<_> = cands.spaces.iter().filter(|s| s.eligible).collect();
            if !eligible.is_empty() {
                println!("\n{}", ui::section_header("ELIGIBLE DEVICES", &ctx.term));
                for s in &eligible {
                    let label = s.label.as_deref().unwrap_or("");
                    let mount = s.mount_path.as_deref().unwrap_or(&s.device);
                    println!(
                        "  {} {} - {} {} (use: storage add {})",
                        ui::status_indicator("info", ctx.term.supports_color),
                        mount,
                        format_bytes(s.capacity_bytes),
                        label,
                        mount,
                    );
                }
            }

            // Physical media needing action
            let actionable: Vec<_> = cands
                .media
                .iter()
                .filter(|m| {
                    matches!(
                        m.suggested_action,
                        MediumAction::NeedsPartition | MediumAction::NeedsFormat
                    )
                })
                .collect();
            if !actionable.is_empty() {
                println!("\n{}", ui::section_header("PHYSICAL MEDIA (needs setup)", &ctx.term));
                for m in &actionable {
                    let name = m.model.as_deref().unwrap_or(&m.device_id);
                    println!(
                        "  {} {} - {} via {} — {}",
                        ui::status_indicator("warn", ctx.term.supports_color),
                        name,
                        format_bytes(m.size_bytes),
                        m.bus_type,
                        m.suggested_action,
                    );
                }
            }
        }

        let has_candidates = candidates
            .as_ref()
            .map(|c| !c.spaces.is_empty() || !c.media.is_empty())
            .unwrap_or(false);
        if storages.is_empty() && garden_banks.is_empty() && !has_candidates {
            println!("\nTip: Insert a USB drive to see available devices");
        }

        Ok(())
    }

    fn requires_endpoint(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "storage-list"
    }
}

// ============================================================================
// Pin / Unpin Storage Commands
// ============================================================================

/// Response from pin/unpin API
#[derive(Debug, Deserialize)]
pub struct PinStorageResponse {
    pub name: String,
    pub pinned: bool,
    pub message: String,
}

/// Pin the Primary role for a storage
pub struct PinStorageCommand {
    pub name: String,
}

impl PinStorageCommand {
    pub fn new(name: String) -> Self {
        Self { name }
    }
}

#[async_trait]
impl Command for PinStorageCommand {
    async fn execute(&self, ctx: &Runtime) -> CommandResult {
        use crate::ui::rendering as ui;

        let endpoint = ctx
            .endpoint
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Endpoint required for pin command"))?;

        let url = format!(
            "{}/api/v1/stone/storage/banks/{}/pin",
            endpoint.trim_end_matches('/'),
            self.name
        );

        let response = ctx
            .client
            .post(&url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to pin: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("Pin failed ({}): {}", status, text);
        }

        let result: PinStorageResponse = response
            .json::<ApiResponse<PinStorageResponse>>()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?
            .data;

        println!(
            "\n{} {}",
            ui::status_indicator("success", ctx.term.supports_color),
            result.message
        );

        Ok(())
    }

    fn name(&self) -> &'static str {
        "pin"
    }
}

/// Unpin the Primary role for a storage
pub struct UnpinStorageCommand {
    pub name: String,
}

impl UnpinStorageCommand {
    pub fn new(name: String) -> Self {
        Self { name }
    }
}

#[async_trait]
impl Command for UnpinStorageCommand {
    async fn execute(&self, ctx: &Runtime) -> CommandResult {
        use crate::ui::rendering as ui;

        let endpoint = ctx
            .endpoint
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Endpoint required for unpin command"))?;

        let url = format!(
            "{}/api/v1/stone/storage/banks/{}/unpin",
            endpoint.trim_end_matches('/'),
            self.name
        );

        let response = ctx
            .client
            .post(&url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to unpin: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("Unpin failed ({}): {}", status, text);
        }

        let result: PinStorageResponse = response
            .json::<ApiResponse<PinStorageResponse>>()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?
            .data;

        println!(
            "\n{} {}",
            ui::status_indicator("success", ctx.term.supports_color),
            result.message
        );

        Ok(())
    }

    fn name(&self) -> &'static str {
        "unpin"
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
// Store Put Runtime
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
    async fn execute(&self, ctx: &Runtime) -> CommandResult {
        use crate::ui::rendering as ui;

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
// Store Get Runtime
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
    async fn execute(&self, ctx: &Runtime) -> CommandResult {
        use crate::ui::rendering as ui;

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
// Store List Runtime
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
    async fn execute(&self, ctx: &Runtime) -> CommandResult {
        use crate::ui::rendering as ui;

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
// Store Delete Runtime
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
    async fn execute(&self, ctx: &Runtime) -> CommandResult {
        use crate::ui::rendering as ui;

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
// Store Head Runtime
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
    async fn execute(&self, ctx: &Runtime) -> CommandResult {
        use crate::ui::rendering as ui;

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

// ============================================================================
// Storage Status Runtime (STORAGE-0009 Phase 6)
// ============================================================================

pub struct StorageStatusCommand {
    pub quiet: bool,
}

impl StorageStatusCommand {
    pub fn new() -> Self {
        Self { quiet: false }
    }
}

impl Default for StorageStatusCommand {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Command for StorageStatusCommand {
    async fn execute(&self, ctx: &Runtime) -> CommandResult {
        use crate::ui::rendering as ui;

        let endpoint = ctx
            .endpoint
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Endpoint required for storage status command"))?;

        // Fetch local banks
        let banks_url = format!(
            "{}/api/v1/stone/storage/banks",
            endpoint.trim_end_matches('/')
        );
        let banks_response = ctx
            .client
            .get(&banks_url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to fetch storage banks: {}", e))?;

        if !banks_response.status().is_success() {
            let status = banks_response.status();
            let text = banks_response.text().await.unwrap_or_default();
            anyhow::bail!("API error {}: {}", status, text);
        }

        let banks: Vec<StorageInfo> = banks_response
            .json::<ApiResponse<Vec<StorageInfo>>>()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?
            .data;

        // Fetch garden-wide overview for roles
        let overview_url = format!("{}/api/v1/stone/storage", endpoint.trim_end_matches('/'));
        let garden_banks: Vec<GardenBankInfo> = match ctx.client.get(&overview_url).send().await {
            Ok(resp) if resp.status().is_success() => resp
                .json::<ApiResponse<StorageOverview>>()
                .await
                .map(|r| r.data.garden_banks)
                .unwrap_or_default(),
            _ => Vec::new(),
        };

        if banks.is_empty() && garden_banks.is_empty() {
            println!(
                "\n{} No managed storage found.",
                ui::status_indicator("info", ctx.term.supports_color)
            );
            println!("\nTip: Use 'garden-rake storage add <device-or-path>' to add storage");
            return Ok(());
        }

        println!("\n{}", ui::section_header("STORAGE STATUS", &ctx.term));

        // Build a role lookup from garden banks
        let mut role_map: std::collections::HashMap<String, StorageRole> =
            std::collections::HashMap::new();
        let mut pin_map: std::collections::HashMap<String, bool> =
            std::collections::HashMap::new();
        for gb in &garden_banks {
            if gb.is_local {
                role_map.insert(gb.id.clone(), gb.role);
                pin_map.insert(gb.id.clone(), gb.pinned);
            }
        }

        // Totals
        let mut total_capacity: u64 = 0;
        let mut total_used: u64 = 0;

        for bank in &banks {
            let role = role_map
                .get(&bank.id)
                .copied()
                .unwrap_or(StorageRole::Dormant);
            let pinned = pin_map.get(&bank.id).copied().unwrap_or(false);

            let status_icon = if bank.online {
                ui::status_indicator("success", ctx.term.supports_color)
            } else {
                ui::status_indicator("error", ctx.term.supports_color)
            };

            let available = bank.capacity_bytes.saturating_sub(bank.used_bytes);
            let usage_pct = if bank.capacity_bytes > 0 {
                (bank.used_bytes as f64 / bank.capacity_bytes as f64 * 100.0) as u32
            } else {
                0
            };

            let pin_label = if pinned { " ★ pinned" } else { "" };
            let enc_label = if bank.encrypted { " [encrypted]" } else { "" };
            let roles_label = if bank.roles.is_empty() {
                String::new()
            } else {
                format!("  roles: {}", bank.roles.join(", "))
            };

            println!(
                "\n  {} {}  ({}){}{}\n",
                status_icon,
                bank.name,
                role,
                pin_label,
                enc_label,
            );
            println!(
                "    Capacity:   {}",
                format_bytes(bank.capacity_bytes)
            );
            println!(
                "    Used:       {} ({}%)",
                format_bytes(bank.used_bytes),
                usage_pct,
            );
            println!(
                "    Available:  {}",
                format_bytes(available)
            );
            println!(
                "    Device:     {}",
                bank.device
            );
            println!(
                "    Mount:      {}",
                bank.mount_path
            );
            println!(
                "    Visibility: {}",
                bank.visibility,
            );
            if !bank.replica_set_id.is_empty() {
                let rs_display = if bank.replica_set_name.is_empty() {
                    "storage".to_string()
                } else {
                    bank.replica_set_name.clone()
                };
                let rs_short = StorageInfo::short_id(&bank.replica_set_id);
                println!("    Replica set: {} ({})", rs_display, rs_short);
            }
            if !roles_label.is_empty() {
                println!("   {}", roles_label);
            }

            total_capacity += bank.capacity_bytes;
            total_used += bank.used_bytes;
        }

        // Summary line
        if banks.len() > 1 {
            let total_available = total_capacity.saturating_sub(total_used);
            let total_pct = if total_capacity > 0 {
                (total_used as f64 / total_capacity as f64 * 100.0) as u32
            } else {
                0
            };
            println!("\n{}", ui::section_header("TOTALS", &ctx.term));
            println!(
                "  {} storage{}: {} used of {} ({}% used, {} available)",
                banks.len(),
                if banks.len() == 1 { "" } else { "s" },
                format_bytes(total_used),
                format_bytes(total_capacity),
                total_pct,
                format_bytes(total_available),
            );
        }

        Ok(())
    }

    fn requires_endpoint(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "storage-status"
    }
}

