//! Nurturing commands - backup restore, status, and management
//!
//! Provides CLI commands for nurturing (backup) management:
//! - `restore {offering} from slot A|B` - Restore from local A/B slot
//! - `restore {offering} from seed-bank {name}` - Restore from remote seed bank
//! - `status nurturing` - Show all offerings with backup status
//! - `status nurturing {offering}` - Detailed view for single offering
//! - `nurturing list {offering}` - List all backups for offering
//! - `nurturing trigger {offering}` - Trigger backup workflow
//! - `nurturing trigger-all` - Trigger all offerings

use crate::commands::{Command, CommandResult};
use crate::context::Runtime;
use async_trait::async_trait;
use garden_common::api_utils::ApiResponse;
use serde::Deserialize;

// ============================================================================
// Response Types (mirror of API responses)
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct NurturingSnapshot {
    pub slot: String,
    pub offering_id: String,
    pub offering_name: String,
    pub harvest_id: String,
    pub created_at: String,
    pub size_bytes: u64,
    pub is_current: bool,
}

#[derive(Debug, Deserialize)]
pub struct OfferingSlots {
    pub offering_id: String,
    #[serde(default)]
    pub offering_name: Option<String>,
    pub slot_a: Option<NurturingSnapshot>,
    pub slot_b: Option<NurturingSnapshot>,
}

#[derive(Debug, Deserialize)]
pub struct NurturingIndex {
    pub version: u32,
    pub offerings: Vec<OfferingSlots>,
    #[serde(default)]
    pub total_snapshots: usize,
}

#[derive(Debug, Deserialize)]
pub struct RemoteSnapshot {
    pub offering_id: String,
    pub harvest_id: String,
    pub seed_bank_id: String,
    pub object_key: String,
    pub created_at: String,
    #[serde(default)]
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct RemoteNurturingIndex {
    pub seed_bank_id: String,
    pub snapshots: Vec<RemoteSnapshot>,
}

#[derive(Debug, Deserialize)]
pub struct HarvestManifest {
    pub id: String,
    pub offering: String,
    pub original_image: String,
    pub committed_image: Option<String>,
    pub created_at: String,
    #[serde(default)]
    pub volumes: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct WorkflowResult {
    pub success: bool,
    pub offering_name: String,
    pub summary: String,
    pub local_snapshot: Option<serde_json::Value>,
    #[serde(default)]
    pub replications: Vec<serde_json::Value>,
}

// ============================================================================
// Restore from Local Slot Runtime
// ============================================================================

pub struct RestoreLocalCommand {
    /// Offering name to restore
    pub offering: String,
    /// Slot to restore from (A, B, or None for current)
    pub slot: Option<String>,
    /// Dry run mode
    pub dry_run: bool,
    /// Quiet mode
    pub quiet: bool,
}

impl RestoreLocalCommand {
    pub fn new(offering: String, slot: Option<String>, dry_run: bool) -> Self {
        Self {
            offering,
            slot,
            dry_run,
            quiet: false,
        }
    }
}

#[async_trait]
impl Command for RestoreLocalCommand {
    async fn execute(&self, ctx: &Runtime) -> CommandResult {
        use garden_common::ui::rendering as ui;

        let endpoint = ctx
            .endpoint
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Endpoint required for restore command"))?;
        let offering_path = urlencoding::encode(&self.offering);

        // First, show what would be restored (dry-run info)
        let slots_url = format!(
            "{}/api/v1/stone/nurturing/{}",
            endpoint.trim_end_matches('/'),
            offering_path
        );
        let slots_response = ctx.client.get(&slots_url).send().await?;

        if !slots_response.status().is_success() {
            let status = slots_response.status();
            let text = slots_response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to get slots ({}): {}", status, text);
        }

        let slots: ApiResponse<Option<OfferingSlots>> = slots_response.json().await?;

        let slots_data = match slots.data {
            Some(s) => s,
            None => {
                println!(
                    "\n{} No nurturing snapshots found for '{}'",
                    ui::status_indicator("warn", ctx.term.supports_color),
                    self.offering
                );
                return Ok(());
            }
        };

        // Determine which slot to restore from
        let snapshot = match self.slot.as_deref() {
            Some("A") | Some("a") => slots_data.slot_a.as_ref(),
            Some("B") | Some("b") => slots_data.slot_b.as_ref(),
            None => {
                // Use current slot
                if let Some(ref a) = slots_data.slot_a {
                    if a.is_current {
                        Some(a)
                    } else {
                        slots_data.slot_b.as_ref()
                    }
                } else {
                    slots_data.slot_b.as_ref()
                }
            }
            Some(other) => {
                anyhow::bail!("Invalid slot '{}' - must be 'A' or 'B'", other);
            }
        };

        let snapshot = match snapshot {
            Some(s) => s,
            None => {
                let slot_name = self.slot.as_deref().unwrap_or("current");
                println!(
                    "\n{} No snapshot in slot {} for '{}'",
                    ui::status_indicator("warn", ctx.term.supports_color),
                    slot_name,
                    self.offering
                );
                return Ok(());
            }
        };

        // Show restore info
        println!(
            "\n{} Restore Preview",
            ui::section_header("NURTURING", &ctx.term)
        );
        println!("  Offering:    {}", self.offering);
        println!("  Slot:        {}", snapshot.slot);
        println!("  Harvest ID:  {}", snapshot.harvest_id);
        println!("  Created:     {}", snapshot.created_at);
        println!("  Size:        {}", format_bytes(snapshot.size_bytes));

        if self.dry_run {
            println!(
                "\n{} Dry run - no changes made",
                ui::status_indicator("info", ctx.term.supports_color)
            );
            return Ok(());
        }

        // Perform the restore
        println!(
            "\n{} Restoring from slot {}...",
            ui::status_indicator("info", ctx.term.supports_color),
            snapshot.slot
        );

        let restore_url = format!(
            "{}/api/v1/stone/nurturing/{}/restore",
            endpoint.trim_end_matches('/'),
            offering_path
        );

        let body = serde_json::json!({
            "slot": snapshot.slot
        });

        let response = ctx.client.post(&restore_url).json(&body).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("Restore failed ({}): {}", status, text);
        }

        let result: ApiResponse<HarvestManifest> = response.json().await?;

        println!(
            "\n{} Restored successfully",
            ui::status_indicator("success", ctx.term.supports_color)
        );
        println!("  Harvest:  {}", result.data.id);
        println!("  Image:    {}", result.data.original_image);
        println!("  Volumes:  {}", result.data.volumes.len());

        Ok(())
    }

    fn requires_endpoint(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "restore"
    }
}

// ============================================================================
// Restore from Seed Bank Runtime
// ============================================================================

pub struct RestoreRemoteCommand {
    /// Offering name to restore
    pub offering: String,
    /// Seed bank name
    pub storage: String,
    /// Optional specific harvest ID (defaults to latest)
    pub harvest_id: Option<String>,
    /// Dry run mode
    pub dry_run: bool,
    /// Quiet mode
    pub quiet: bool,
}

impl RestoreRemoteCommand {
    pub fn new(
        offering: String,
        seed_bank: String,
        harvest_id: Option<String>,
        dry_run: bool,
    ) -> Self {
        Self {
            offering,
            storage: seed_bank,
            harvest_id,
            dry_run,
            quiet: false,
        }
    }
}

#[async_trait]
impl Command for RestoreRemoteCommand {
    async fn execute(&self, ctx: &Runtime) -> CommandResult {
        use garden_common::ui::rendering as ui;

        let endpoint = ctx
            .endpoint
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Endpoint required for restore command"))?;

        // Get list of remote snapshots from seed bank
        let seed_bank_path = urlencoding::encode(&self.storage);
        let remote_url = format!(
            "{}/api/v1/stone/nurturing/remote/{}",
            endpoint.trim_end_matches('/'),
            seed_bank_path
        );
        let remote_response = ctx.client.get(&remote_url).send().await?;

        if !remote_response.status().is_success() {
            let status = remote_response.status();
            let text = remote_response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to get remote snapshots ({}): {}", status, text);
        }

        let remote_index: ApiResponse<RemoteNurturingIndex> = remote_response.json().await?;

        // Find matching snapshots for this offering
        // We need to look up offering_id from the offering name first
        let services_url = format!("{}/api/v1/stone/services", endpoint.trim_end_matches('/'));
        let services_response = ctx.client.get(&services_url).send().await?;

        let offering_id = if services_response.status().is_success() {
            let services: serde_json::Value = services_response.json().await?;
            services
                .get("data")
                .and_then(|d| d.get("services"))
                .and_then(|s| s.as_array())
                .and_then(|arr| {
                    arr.iter().find_map(|svc| {
                        let name = svc.get("name").and_then(|n| n.as_str())?;
                        if name == self.offering {
                            svc.get("offering_id")
                                .and_then(|id| id.as_str())
                                .map(String::from)
                        } else {
                            None
                        }
                    })
                })
        } else {
            None
        };

        let offering_id = offering_id.unwrap_or_else(|| self.offering.clone());

        let matching_snapshots: Vec<_> = remote_index
            .data
            .snapshots
            .iter()
            .filter(|s| s.offering_id == offering_id)
            .collect();

        if matching_snapshots.is_empty() {
            println!(
                "\n{} No remote snapshots found for '{}' on seed bank '{}'",
                ui::status_indicator("warn", ctx.term.supports_color),
                self.offering,
                self.storage
            );
            return Ok(());
        }

        // Select snapshot to restore
        let snapshot = if let Some(ref harvest_id) = self.harvest_id {
            matching_snapshots
                .iter()
                .find(|s| s.harvest_id == *harvest_id)
                .copied()
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Harvest '{}' not found on seed bank '{}'",
                        harvest_id,
                        self.storage
                    )
                })?
        } else {
            // Use latest (first in list, assuming sorted by date desc)
            matching_snapshots
                .first()
                .copied()
                .ok_or_else(|| anyhow::anyhow!("No snapshots available"))?
        };

        // Show restore info
        println!(
            "\n{} Remote Restore Preview",
            ui::section_header("NURTURING", &ctx.term)
        );
        println!("  Offering:    {}", self.offering);
        println!("  Seed Bank:   {}", self.storage);
        println!("  Harvest ID:  {}", snapshot.harvest_id);
        println!("  Created:     {}", snapshot.created_at);
        if let Some(size) = snapshot.size_bytes {
            println!("  Size:        {}", format_bytes(size));
        }

        if self.dry_run {
            println!(
                "\n{} Dry run - no changes made",
                ui::status_indicator("info", ctx.term.supports_color)
            );
            return Ok(());
        }

        // Perform the remote restore
        println!(
            "\n{} Restoring from seed bank '{}'...",
            ui::status_indicator("info", ctx.term.supports_color),
            self.storage
        );

        let offering_path = urlencoding::encode(&self.offering);
        let restore_url = format!(
            "{}/api/v1/stone/nurturing/{}/restore-remote",
            endpoint.trim_end_matches('/'),
            offering_path
        );

        let body = serde_json::json!({
            "storage": self.storage,
            "harvest_id": snapshot.harvest_id
        });

        let response = ctx.client.post(&restore_url).json(&body).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("Remote restore failed ({}): {}", status, text);
        }

        let result: ApiResponse<HarvestManifest> = response.json().await?;

        println!(
            "\n{} Restored successfully from seed bank",
            ui::status_indicator("success", ctx.term.supports_color)
        );
        println!("  Harvest:  {}", result.data.id);
        println!("  Image:    {}", result.data.original_image);
        println!("  Volumes:  {}", result.data.volumes.len());

        Ok(())
    }

    fn requires_endpoint(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "restore-remote"
    }
}

// ============================================================================
// Nurturing Status Runtime
// ============================================================================

pub struct NurturingStatusCommand {
    /// Optional specific offering to show detailed status for
    pub offering: Option<String>,
    /// Quiet mode
    pub quiet: bool,
}

impl NurturingStatusCommand {
    pub fn new(offering: Option<String>) -> Self {
        Self {
            offering,
            quiet: false,
        }
    }
}

#[async_trait]
impl Command for NurturingStatusCommand {
    async fn execute(&self, ctx: &Runtime) -> CommandResult {
        use garden_common::ui::rendering as ui;

        let endpoint = ctx
            .endpoint
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Endpoint required for status command"))?;

        if let Some(ref offering) = self.offering {
            // Detailed view for single offering
            return self.show_offering_detail(ctx, endpoint, offering).await;
        }

        // Overview of all offerings
        let url = format!("{}/api/v1/stone/nurturing", endpoint.trim_end_matches('/'));
        let response = ctx.client.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to get nurturing status ({}): {}", status, text);
        }

        let index: ApiResponse<NurturingIndex> = response.json().await?;

        println!("\n{}", ui::section_header("NURTURING STATUS", &ctx.term));

        if index.data.offerings.is_empty() {
            println!(
                "  {} No nurturing snapshots configured",
                ui::status_indicator("info", ctx.term.supports_color)
            );
            return Ok(());
        }

        // Get seed banks for remote status
        let banks_url = format!(
            "{}/api/v1/stone/storage/bank",
            endpoint.trim_end_matches('/')
        );
        let banks: Vec<serde_json::Value> = match ctx.client.get(&banks_url).send().await {
            Ok(resp) => match resp.json::<ApiResponse<Vec<serde_json::Value>>>().await {
                Ok(r) => r.data,
                Err(_) => Vec::new(),
            },
            Err(_) => Vec::new(),
        };

        let online_banks: Vec<&serde_json::Value> = banks
            .iter()
            .filter(|b: &&serde_json::Value| {
                b.get("online").and_then(|o| o.as_bool()).unwrap_or(false)
            })
            .collect();

        println!(
            "  Total Snapshots: {}  |  Offerings: {}  |  Seed Banks: {} online",
            index.data.total_snapshots,
            index.data.offerings.len(),
            online_banks.len()
        );
        println!();

        for slots in &index.data.offerings {
            let name = slots
                .slot_a
                .as_ref()
                .or(slots.slot_b.as_ref())
                .map(|s| s.offering_name.as_str())
                .unwrap_or(&slots.offering_id[..8]);

            let slot_a_info = slots.slot_a.as_ref().map(|s| {
                let current = if s.is_current { " (current)" } else { "" };
                format!("{}{}", &s.harvest_id[..8], current)
            });
            let slot_b_info = slots.slot_b.as_ref().map(|s| {
                let current = if s.is_current { " (current)" } else { "" };
                format!("{}{}", &s.harvest_id[..8], current)
            });

            let status_icon = ui::status_indicator("success", ctx.term.supports_color);

            println!("  {} {}", status_icon, name);
            println!(
                "      Slot A: {}",
                slot_a_info.as_deref().unwrap_or("(empty)")
            );
            println!(
                "      Slot B: {}",
                slot_b_info.as_deref().unwrap_or("(empty)")
            );
        }

        Ok(())
    }

    fn requires_endpoint(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "status-nurturing"
    }
}

impl NurturingStatusCommand {
    async fn show_offering_detail(
        &self,
        ctx: &Runtime,
        endpoint: &str,
        offering: &str,
    ) -> CommandResult {
        use garden_common::ui::rendering as ui;

        // Get local slots
        let offering_path = urlencoding::encode(offering);
        let slots_url = format!(
            "{}/api/v1/stone/nurturing/{}",
            endpoint.trim_end_matches('/'),
            offering_path
        );
        let slots_response = ctx.client.get(&slots_url).send().await?;

        if !slots_response.status().is_success() {
            let status = slots_response.status();
            let text = slots_response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to get slots ({}): {}", status, text);
        }

        let slots: ApiResponse<Option<OfferingSlots>> = slots_response.json().await?;

        println!(
            "\n{} {}",
            ui::section_header("NURTURING DETAIL", &ctx.term),
            offering
        );

        match slots.data {
            Some(slots_data) => {
                println!("\n  Local A/B Slots:");
                println!("  ────────────────");

                if let Some(ref a) = slots_data.slot_a {
                    let current = if a.is_current { " ← current" } else { "" };
                    println!("    Slot A:{}", current);
                    println!("      Harvest:  {}", a.harvest_id);
                    println!("      Created:  {}", a.created_at);
                    println!("      Size:     {}", format_bytes(a.size_bytes));
                } else {
                    println!("    Slot A: (empty)");
                }

                if let Some(ref b) = slots_data.slot_b {
                    let current = if b.is_current { " ← current" } else { "" };
                    println!("    Slot B:{}", current);
                    println!("      Harvest:  {}", b.harvest_id);
                    println!("      Created:  {}", b.created_at);
                    println!("      Size:     {}", format_bytes(b.size_bytes));
                } else {
                    println!("    Slot B: (empty)");
                }

                // Get remote snapshots from each seed bank
                let banks_url = format!(
                    "{}/api/v1/stone/storage/bank",
                    endpoint.trim_end_matches('/')
                );
                if let Ok(banks_resp) = ctx.client.get(&banks_url).send().await {
                    if let Ok(banks) = banks_resp
                        .json::<ApiResponse<Vec<serde_json::Value>>>()
                        .await
                    {
                        let online_banks: Vec<_> = banks
                            .data
                            .iter()
                            .filter(|b| b.get("online").and_then(|o| o.as_bool()).unwrap_or(false))
                            .collect();

                        if !online_banks.is_empty() {
                            println!("\n  Remote Snapshots:");
                            println!("  ─────────────────");

                            for bank in online_banks {
                                let bank_name = bank
                                    .get("name")
                                    .and_then(|n| n.as_str())
                                    .unwrap_or("unknown");

                                let remote_url = format!(
                                    "{}/api/v1/stone/nurturing/remote/{}",
                                    endpoint.trim_end_matches('/'),
                                    urlencoding::encode(bank_name)
                                );

                                if let Ok(remote_resp) = ctx.client.get(&remote_url).send().await {
                                    if let Ok(remote) = remote_resp
                                        .json::<ApiResponse<RemoteNurturingIndex>>()
                                        .await
                                    {
                                        let matching: Vec<_> = remote
                                            .data
                                            .snapshots
                                            .iter()
                                            .filter(|s| s.offering_id == slots_data.offering_id)
                                            .collect();

                                        println!(
                                            "    {} ({} snapshots):",
                                            bank_name,
                                            matching.len()
                                        );
                                        for snap in matching.iter().take(5) {
                                            let size_str = snap
                                                .size_bytes
                                                .map(format_bytes)
                                                .unwrap_or_else(|| "?".to_string());
                                            println!(
                                                "      {} - {} ({})",
                                                &snap.harvest_id[..8],
                                                snap.created_at,
                                                size_str
                                            );
                                        }
                                        if matching.len() > 5 {
                                            println!("      ... and {} more", matching.len() - 5);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            None => {
                println!(
                    "  {} No nurturing snapshots for this offering",
                    ui::status_indicator("info", ctx.term.supports_color)
                );
            }
        }

        Ok(())
    }
}

// ============================================================================
// Nurturing List Runtime
// ============================================================================

pub struct NurturingListCommand {
    /// Offering name to list backups for
    pub offering: String,
    /// Show only local backups
    pub local_only: bool,
    /// Show only remote backups
    pub remote_only: bool,
    /// Quiet mode
    pub quiet: bool,
}

impl NurturingListCommand {
    pub fn new(offering: String, local_only: bool, remote_only: bool) -> Self {
        Self {
            offering,
            local_only,
            remote_only,
            quiet: false,
        }
    }
}

#[async_trait]
impl Command for NurturingListCommand {
    async fn execute(&self, ctx: &Runtime) -> CommandResult {
        use garden_common::ui::rendering as ui;

        let endpoint = ctx
            .endpoint
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Endpoint required for list command"))?;

        println!(
            "\n{} {}",
            ui::section_header("NURTURING BACKUPS", &ctx.term),
            self.offering
        );

        let mut total_count = 0;
        let offering_path = urlencoding::encode(&self.offering);

        // Local backups
        if !self.remote_only {
            let slots_url = format!(
                "{}/api/v1/stone/nurturing/{}",
                endpoint.trim_end_matches('/'),
                offering_path
            );

            if let Ok(resp) = ctx.client.get(&slots_url).send().await {
                if let Ok(slots) = resp.json::<ApiResponse<Option<OfferingSlots>>>().await {
                    if let Some(slots_data) = slots.data {
                        println!("\n  Local (A/B Slots):");

                        if let Some(ref a) = slots_data.slot_a {
                            total_count += 1;
                            let current = if a.is_current { " *" } else { "" };
                            println!(
                                "    [A]{} {} - {} - {}",
                                current,
                                &a.harvest_id[..12],
                                a.created_at,
                                format_bytes(a.size_bytes)
                            );
                        }

                        if let Some(ref b) = slots_data.slot_b {
                            total_count += 1;
                            let current = if b.is_current { " *" } else { "" };
                            println!(
                                "    [B]{} {} - {} - {}",
                                current,
                                &b.harvest_id[..12],
                                b.created_at,
                                format_bytes(b.size_bytes)
                            );
                        }

                        if slots_data.slot_a.is_none() && slots_data.slot_b.is_none() {
                            println!("    (no local backups)");
                        }
                    } else {
                        println!("\n  Local: (no backups)");
                    }
                }
            }
        }

        // Remote backups
        if !self.local_only {
            // Get offering_id first
            let services_url = format!("{}/api/v1/stone/services", endpoint.trim_end_matches('/'));

            let offering_id: Option<String> = match ctx.client.get(&services_url).send().await {
                Ok(resp) => match resp.json::<serde_json::Value>().await {
                    Ok(v) => v
                        .get("data")
                        .and_then(|d| d.get("services"))
                        .and_then(|s| s.as_array())
                        .and_then(|arr| {
                            arr.iter().find_map(|svc| {
                                let name = svc.get("name").and_then(|n| n.as_str())?;
                                if name == self.offering {
                                    svc.get("offering_id")
                                        .and_then(|id| id.as_str())
                                        .map(String::from)
                                } else {
                                    None
                                }
                            })
                        }),
                    Err(_) => None,
                },
                Err(_) => None,
            };

            // Get seed banks
            let banks_url = format!(
                "{}/api/v1/stone/storage/bank",
                endpoint.trim_end_matches('/')
            );

            if let Ok(banks_resp) = ctx.client.get(&banks_url).send().await {
                if let Ok(banks) = banks_resp
                    .json::<ApiResponse<Vec<serde_json::Value>>>()
                    .await
                {
                    for bank in &banks.data {
                        let bank_name = bank
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("unknown");
                        let online = bank
                            .get("online")
                            .and_then(|o| o.as_bool())
                            .unwrap_or(false);

                        if !online {
                            continue;
                        }

                        let remote_url = format!(
                            "{}/api/v1/stone/nurturing/remote/{}",
                            endpoint.trim_end_matches('/'),
                            urlencoding::encode(bank_name)
                        );

                        if let Ok(remote_resp) = ctx.client.get(&remote_url).send().await {
                            if let Ok(remote) = remote_resp
                                .json::<ApiResponse<RemoteNurturingIndex>>()
                                .await
                            {
                                let matching: Vec<_> = remote
                                    .data
                                    .snapshots
                                    .iter()
                                    .filter(|s| {
                                        offering_id
                                            .as_ref()
                                            .map(|id| s.offering_id == *id)
                                            .unwrap_or(false)
                                    })
                                    .collect();

                                if !matching.is_empty() {
                                    println!("\n  Remote ({}):", bank_name);
                                    for snap in &matching {
                                        total_count += 1;
                                        let size_str = snap
                                            .size_bytes
                                            .map(format_bytes)
                                            .unwrap_or_else(|| "?".to_string());
                                        println!(
                                            "    {} - {} - {}",
                                            &snap.harvest_id[..12],
                                            snap.created_at,
                                            size_str
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        println!(
            "\n  {} Total: {} backup(s)",
            ui::status_indicator("info", ctx.term.supports_color),
            total_count
        );

        Ok(())
    }

    fn requires_endpoint(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "nurturing-list"
    }
}

// ============================================================================
// Nurturing Trigger Runtime
// ============================================================================

pub struct NurturingTriggerCommand {
    /// Offering name to trigger backup for (None = all)
    pub offering: Option<String>,
    /// Quiet mode
    pub quiet: bool,
}

impl NurturingTriggerCommand {
    pub fn new(offering: Option<String>) -> Self {
        Self {
            offering,
            quiet: false,
        }
    }
}

#[async_trait]
impl Command for NurturingTriggerCommand {
    async fn execute(&self, ctx: &Runtime) -> CommandResult {
        use garden_common::ui::rendering as ui;

        let endpoint = ctx
            .endpoint
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Endpoint required for trigger command"))?;

        if let Some(ref offering) = self.offering {
            // Single offering
            println!(
                "\n{} Triggering nurturing workflow for '{}'...",
                ui::status_indicator("info", ctx.term.supports_color),
                offering
            );

            let offering_path = urlencoding::encode(offering);
            let url = format!(
                "{}/api/v1/nurturing/{}/trigger",
                endpoint.trim_end_matches('/'),
                offering_path
            );

            let response = ctx
                .client
                .post(&url)
                .json(&serde_json::json!({}))
                .send()
                .await?;

            if !response.status().is_success() {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                anyhow::bail!("Trigger failed ({}): {}", status, text);
            }

            let result: ApiResponse<WorkflowResult> = response.json().await?;

            if result.data.success {
                println!(
                    "{} {}",
                    ui::status_indicator("success", ctx.term.supports_color),
                    result.data.summary
                );
            } else {
                println!(
                    "{} {}",
                    ui::status_indicator("error", ctx.term.supports_color),
                    result.data.summary
                );
            }
        } else {
            // All offerings
            println!(
                "\n{} Triggering nurturing workflow for all offerings...",
                ui::status_indicator("info", ctx.term.supports_color)
            );

            let url = format!(
                "{}/api/v1/nurturing/trigger-all",
                endpoint.trim_end_matches('/')
            );

            let response = ctx
                .client
                .post(&url)
                .json(&serde_json::json!({}))
                .send()
                .await?;

            if !response.status().is_success() {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                anyhow::bail!("Trigger-all failed ({}): {}", status, text);
            }

            let results: ApiResponse<Vec<WorkflowResult>> = response.json().await?;

            let success_count = results.data.iter().filter(|r| r.success).count();
            let total_count = results.data.len();

            println!(
                "\n{} {}/{} offerings nurtured successfully",
                if success_count == total_count {
                    ui::status_indicator("success", ctx.term.supports_color)
                } else {
                    ui::status_indicator("warn", ctx.term.supports_color)
                },
                success_count,
                total_count
            );

            for result in &results.data {
                let icon = if result.success {
                    ui::status_indicator("success", ctx.term.supports_color)
                } else {
                    ui::status_indicator("error", ctx.term.supports_color)
                };
                println!("  {} {}: {}", icon, result.offering_name, result.summary);
            }
        }

        Ok(())
    }

    fn requires_endpoint(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "nurturing-trigger"
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn format_bytes(bytes: u64) -> String {
    const GB: u64 = 1024 * 1024 * 1024;
    const MB: u64 = 1024 * 1024;
    const KB: u64 = 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
