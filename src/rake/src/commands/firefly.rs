//! Firefly operator commands (FIREFLY-0004 Chapter 4).
//!
//! - `garden-rake firefly inventory`         — print the local roster
//! - `garden-rake firefly roster push <stone>` — copy the local roster
//!                                                to a stone via scp
//!
//! The inventory command is purely local — it reads
//! [`paths::operator_firefly_roster`] and pretty-prints each entry.
//! The push command shells out to `scp` (present on Linux / macOS /
//! Windows 10+) so it does not require any PuTTY-specific tooling.

use crate::command_manifest::cmd;
use crate::commands::Command;
use crate::context::Context;
use anyhow::{Context as _, Result};
use garden_common::constants::paths;
use garden_common::firefly_roster::FireflyRoster;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum FireflyAction {
    Inventory,
    RosterPush { stone: String },
}

pub struct FireflyCommand {
    action: FireflyAction,
    quiet: bool,
}

impl FireflyCommand {
    pub fn inventory(quiet: bool) -> Self {
        Self {
            action: FireflyAction::Inventory,
            quiet,
        }
    }

    pub fn roster_push(stone: String, quiet: bool) -> Self {
        Self {
            action: FireflyAction::RosterPush { stone },
            quiet,
        }
    }
}

impl Command for FireflyCommand {
    fn name(&self) -> &'static str {
        cmd::FIREFLY_CMD
    }

    fn requires_endpoint(&self) -> bool {
        false
    }

    fn show_stone_header(&self) -> bool {
        false
    }

    fn execute<'a>(
        &'a self,
        _ctx: &'a Context,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            match &self.action {
                FireflyAction::Inventory => run_inventory(self.quiet),
                FireflyAction::RosterPush { stone } => run_roster_push(stone, self.quiet),
            }
        })
    }
}

// ---------------------------------------------------------------------------
// inventory
// ---------------------------------------------------------------------------

fn run_inventory(_quiet: bool) -> Result<()> {
    let path = paths::operator_firefly_roster();
    let roster = FireflyRoster::load(&path)
        .with_context(|| format!("Failed to read roster at {}", path.display()))?;

    if roster.fireflies.is_empty() {
        println!("No fireflies provisioned yet.");
        println!("Run NewFirefly.ps1 against a connected device to mint one.");
        println!("Roster path: {}", path.display());
        return Ok(());
    }

    let total = roster.fireflies.len();
    let distinct = roster.device_count();
    let label = if total == 1 { "entry" } else { "entries" };
    println!();
    println!(
        "=== Firefly Inventory ({total} {label}, {distinct} distinct devices) ==="
    );
    println!("Roster: {}", path.display());
    println!();

    for entry in &roster.fireflies {
        let label = entry.label.as_deref().unwrap_or("(unlabeled)");
        let firmware = entry
            .firmware_version_at_provisioning
            .as_deref()
            .unwrap_or("?");
        let stone = entry.stone_assigned_to.as_deref().unwrap_or("(unassigned)");
        println!("  {label}  [{variant}]", variant = entry.variant);
        println!("    device_id:  {}", entry.device_id);
        println!("    minted:     {} by {}", entry.minted_at, entry.minted_by);
        println!("    firmware:   v{firmware}  stone: {stone}");
        println!();
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// roster push
// ---------------------------------------------------------------------------

fn run_roster_push(stone: &str, quiet: bool) -> Result<()> {
    let local_path = paths::operator_firefly_roster();
    if !local_path.exists() {
        anyhow::bail!(
            "Roster file not found at {}. Run NewFirefly.ps1 to provision at least one device first.",
            local_path.display()
        );
    }

    // Validate by loading so we don't push a corrupt file.
    let roster = FireflyRoster::load(&local_path)
        .with_context(|| format!("Roster at {} failed to parse", local_path.display()))?;

    if !quiet {
        println!(
            "Pushing {} entries ({} distinct devices) to {stone}",
            roster.fireflies.len(),
            roster.device_count()
        );
    }

    // Copy to a staging path on the stone via scp, then move under
    // sudo into the managed location. Using the standard `scp` +
    // `ssh` binaries keeps this cross-platform (macOS / Linux /
    // Windows 10+ all ship these).
    let staging_remote = format!("/tmp/firefly-roster.json");
    scp_upload(&local_path, stone, &staging_remote)
        .context("scp upload failed")?;

    let target = paths::stone_firefly_roster();
    ssh_exec(
        stone,
        &format!(
            "sudo mkdir -p {parent} && sudo mv {staging_remote} {target} && sudo chown root:root {target} && sudo chmod 644 {target}",
            parent = std::path::Path::new(&target)
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "/var/lib/zen-garden".to_string())
        ),
    )
    .context("failed to finalize roster move on stone")?;

    if !quiet {
        println!("Roster synced to {stone}:{target}");
    }
    Ok(())
}

fn scp_upload(local: &PathBuf, stone: &str, remote: &str) -> Result<()> {
    let status = std::process::Command::new("scp")
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new")
        .arg(local)
        .arg(format!("stone@{stone}:{remote}"))
        .status()
        .context("failed to spawn scp")?;
    if !status.success() {
        anyhow::bail!("scp exited with {status}");
    }
    Ok(())
}

fn ssh_exec(stone: &str, remote_cmd: &str) -> Result<()> {
    let status = std::process::Command::new("ssh")
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new")
        .arg(format!("stone@{stone}"))
        .arg(remote_cmd)
        .status()
        .context("failed to spawn ssh")?;
    if !status.success() {
        anyhow::bail!("ssh exited with {status}");
    }
    Ok(())
}
