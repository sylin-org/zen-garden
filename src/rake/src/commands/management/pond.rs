//! Pond command - pond security management
//!
//! Manages multi-stone trust network (pond) operations:
//! - init: Initialize pond security (place keystone)
//! - status: Show pond status
//! - invite: Generate TOTP invitation for enrollment
//! - join: Join pond with TOTP code
//! - unlock: Unlock pond CA after restart
//! - remove: Drain pond (destroy CA)
//! - untrust: Revoke a stone from pond
//! - promote: Promote this stone to standby CA
//! - rename: Rename the pond (decorative)

use crate::command_manifest::cmd;
use crate::commands::{Command, CommandResult};
use crate::context::CommandContext;
use crate::suggestions;
use async_trait::async_trait;
use garden_common::ui::rendering as ui;

/// Pond action to perform
pub enum PondActionType {
    /// Initialize pond security (place keystone)
    Init {
        passphrase: Option<String>,
        profile: Option<String>,
    },
    /// Show pond status
    Status,
    /// Generate TOTP invitation for enrollment
    Invite { passphrase: Option<String> },
    /// Join pond with TOTP code
    Join { code: String },
    /// Unlock pond CA after restart
    Unlock {
        /// Passphrase to decrypt the CA key
        passphrase: Option<String>,
        /// TOTP code for authenticator-based unlock
        totp: Option<String>,
    },
    /// Drain pond (destroy CA)
    Remove,
    /// Revoke a stone from the pond
    Untrust { stone_name: String },
    /// Promote this stone to standby CA
    Promote { passphrase: Option<String> },
    /// Rename the pond (decorative, no cryptographic impact)
    Rename { name: Option<String> },
}

/// Pond command for security management
pub struct PondCommand {
    pub action: PondActionType,
    pub quiet_mode: bool,
}

impl PondCommand {
    pub fn new(action: PondActionType, quiet_mode: bool) -> Self {
        Self { action, quiet_mode }
    }
}

#[async_trait]
impl Command for PondCommand {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        let endpoint = ctx.endpoint()?;

        match &self.action {
            PondActionType::Init {
                passphrase,
                profile,
            } => {
                execute_pond_init(ctx, endpoint, passphrase.clone(), profile.clone()).await?;
            }
            PondActionType::Status => {
                execute_pond_status(ctx, endpoint).await?;
            }
            PondActionType::Invite { passphrase } => {
                execute_pond_invite(ctx, endpoint, passphrase.clone()).await?;
            }
            PondActionType::Join { code } => {
                execute_pond_join(ctx, endpoint, code).await?;
            }
            PondActionType::Unlock { passphrase, totp } => {
                execute_pond_unlock(ctx, endpoint, passphrase.clone(), totp.clone()).await?;
            }
            PondActionType::Remove => {
                execute_pond_remove(ctx, endpoint).await?;
            }
            PondActionType::Untrust { stone_name } => {
                execute_pond_untrust(ctx, endpoint, stone_name).await?;
            }
            PondActionType::Promote { passphrase } => {
                execute_pond_promote(ctx, endpoint, passphrase.clone()).await?;
            }
            PondActionType::Rename { name } => {
                execute_pond_rename(ctx, endpoint, name.clone()).await?;
            }
        }

        // Self-teaching suggestions (suppress in JSON mode)
        if !ctx.wants_json() {
            suggestions::print_suggestions(cmd::POND, self.quiet_mode);
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        cmd::POND
    }
}

async fn execute_pond_init(
    ctx: &CommandContext,
    endpoint: &str,
    passphrase: Option<String>,
    profile: Option<String>,
) -> anyhow::Result<()> {
    let ceremony_url = format!(
        "{}/api/v1/pond/ceremony",
        endpoint.trim_end_matches('/')
    );

    // Pre-fill data from CLI flags (same pattern as koi certmesh create)
    let mut initial_data = serde_json::Map::new();
    if let Some(p) = profile {
        initial_data.insert("profile".into(), serde_json::json!(p));
    }
    if let Some(pass) = passphrase {
        initial_data.insert("passphrase".into(), serde_json::json!(pass));
    }

    // Drive the ceremony — all prompts, messages, and validation
    // come from the server. This is a dumb render loop.
    let result = crate::commands::ceremony_render::run_ceremony_http(
        &ctx.client,
        &ceremony_url,
        "init",
        initial_data,
    )
    .await;

    match result {
        Ok(result_data) => {
            // The server already created the CA and returned safe result data.
            // Show a final confirmation with the creation details.
            let pond_name = result_data
                .get("pond_name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let cornerstone = result_data
                .get("cornerstone")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let fingerprint = result_data
                .get("ca_fingerprint")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            println!(
                "\n{}{} Pond initialized — keystone placed",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("ok", ctx.term.supports_color)
            );
            println!("   Pond:         {}", pond_name);
            println!("   Cornerstone:  {}", cornerstone);
            println!("   Fingerprint:  {}", fingerprint);
            println!();
        }
        Err(e) => {
            eprintln!(
                "{}{} {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                e
            );
        }
    }

    Ok(())
}

async fn execute_pond_status(ctx: &CommandContext, endpoint: &str) -> anyhow::Result<()> {
    let url = format!("{}/api/v1/pond/status", endpoint.trim_end_matches('/'));

    match ctx.client.get(&url).send().await {
        Ok(response) if response.status().is_success() => {
            if let Ok(body) = response.json::<serde_json::Value>().await {
                // JSON output mode — emit raw API response
                if ctx.wants_json() {
                    println!("{}", serde_json::to_string_pretty(&body).unwrap_or_default());
                    return Ok(());
                }

                if let Some(data) = body.get("data") {
                    let active = data
                        .get("active")
                        .and_then(|a| a.as_bool())
                        .unwrap_or(false);
                    let locked = data
                        .get("locked")
                        .and_then(|l| l.as_bool())
                        .unwrap_or(false);
                    let name = data
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("");
                    let profile = data
                        .get("profile")
                        .and_then(|p| p.as_str())
                        .unwrap_or("unknown");
                    let enrollment = data
                        .get("enrollment_state")
                        .and_then(|e| e.as_str())
                        .unwrap_or("unknown");

                    if active {
                        println!(
                            "{}{} Pond active",
                            " ".repeat(ui::constants::DEFAULT_INDENT),
                            ui::status_indicator("ok", ctx.term.supports_color)
                        );
                    } else if locked {
                        println!(
                            "{}{} Pond locked (run 'garden-rake pond unlock')",
                            " ".repeat(ui::constants::DEFAULT_INDENT),
                            ui::status_indicator("warning", ctx.term.supports_color)
                        );
                    } else {
                        println!(
                            "{}o Pond not initialized",
                            " ".repeat(ui::constants::DEFAULT_INDENT),
                        );
                    }

                    if !name.is_empty() {
                        println!("   Name: {}", name);
                    }
                    if let Some(cornerstone) = data.get("cornerstone").and_then(|c| c.as_str()) {
                        println!("   Cornerstone: {}", cornerstone);
                    }
                    if let Some(fp) = data.get("ca_fingerprint").and_then(|f| f.as_str()) {
                        println!("   CA fingerprint: {}", fp);
                    }
                    println!("   Profile: {}", profile);
                    println!("   Enrollment: {}", enrollment);

                    if let Some(stones) = data.get("stones").and_then(|s| s.as_array()) {
                        println!("   Stones: {}", stones.len());
                        for stone in stones {
                            if let Some(name) = stone.get("name").and_then(|n| n.as_str()) {
                                let role = stone
                                    .get("role")
                                    .and_then(|r| r.as_str())
                                    .unwrap_or("member");
                                let status = stone
                                    .get("status")
                                    .and_then(|s| s.as_str())
                                    .unwrap_or("unknown");
                                println!("     * {} [{}] ({})", name, role, status);
                            }
                        }
                    }
                }
            }
        }
        Ok(response) => {
            eprintln!(
                "{}{} Failed to get pond status: {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                response.status()
            );
        }
        Err(e) => {
            eprintln!(
                "{}{} Request failed: {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                e
            );
        }
    }

    Ok(())
}

async fn execute_pond_invite(
    ctx: &CommandContext,
    endpoint: &str,
    passphrase: Option<String>,
) -> anyhow::Result<()> {
    let pass = passphrase.unwrap_or_else(|| {
        println!(
            "{}{} Using default passphrase for invite. Use --passphrase to specify.",
            " ".repeat(ui::constants::DEFAULT_INDENT),
            ui::status_indicator("info", ctx.term.supports_color)
        );
        "changeme".to_string()
    });

    let url = format!("{}/api/v1/pond/invite", endpoint.trim_end_matches('/'));
    let payload = serde_json::json!({ "passphrase": pass });

    match ctx.client.post(&url).json(&payload).send().await {
        Ok(response) if response.status().is_success() => {
            if let Ok(body) = response.json::<serde_json::Value>().await {
                if let Some(data) = body.get("data") {
                    println!(
                        "{}{} Enrollment invitation generated",
                        " ".repeat(ui::constants::DEFAULT_INDENT),
                        ui::status_indicator("ok", ctx.term.supports_color)
                    );
                    if let Some(totp_uri) = data.get("totp_uri").and_then(|t| t.as_str()) {
                        println!("   TOTP URI: {}", totp_uri);
                        println!("   Add to authenticator app and share code with joining stone.");
                    }
                    if let Some(ttl) = data.get("ttl_seconds").and_then(|t| t.as_u64()) {
                        println!("   Valid for: {} seconds", ttl);
                    }
                    if let Some(expires) = data.get("expires_at").and_then(|e| e.as_str()) {
                        println!("   Expires at: {}", expires);
                    }
                    if let Some(inviter) = data.get("inviter_stone").and_then(|i| i.as_str()) {
                        println!("   From: {}", inviter);
                    }
                }
            }
        }
        Ok(response) => {
            eprintln!(
                "{}{} Failed to generate invitation: {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                response.status()
            );
        }
        Err(e) => {
            eprintln!(
                "{}{} Request failed: {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                e
            );
        }
    }

    Ok(())
}

async fn execute_pond_join(ctx: &CommandContext, endpoint: &str, code: &str) -> anyhow::Result<()> {
    let url = format!("{}/api/v1/pond/join", endpoint.trim_end_matches('/'));
    let payload = serde_json::json!({ "code": code });

    match ctx.client.post(&url).json(&payload).send().await {
        Ok(response) if response.status().is_success() => {
            println!(
                "{}{} Joined pond successfully",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("ok", ctx.term.supports_color)
            );
            if let Ok(body) = response.json::<serde_json::Value>().await {
                if let Some(data) = body.get("data") {
                    if let Some(stone_name) = data.get("stone_name").and_then(|s| s.as_str()) {
                        println!("   Stone: {}", stone_name);
                    }
                    if let Some(cornerstone) = data.get("cornerstone").and_then(|c| c.as_str()) {
                        println!("   Cornerstone: {}", cornerstone);
                    }
                    if let Some(fp) = data.get("ca_fingerprint").and_then(|f| f.as_str()) {
                        println!("   CA fingerprint: {}", fp);
                    }
                }
            }
        }
        Ok(response) => {
            eprintln!(
                "{}{} Failed to join pond: {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                response.status()
            );
        }
        Err(e) => {
            eprintln!(
                "{}{} Request failed: {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                e
            );
        }
    }

    Ok(())
}

async fn execute_pond_remove(ctx: &CommandContext, endpoint: &str) -> anyhow::Result<()> {
    let url = format!("{}/api/v1/pond", endpoint.trim_end_matches('/'));

    match ctx.client.delete(&url).send().await {
        Ok(response) if response.status().is_success() => {
            println!(
                "{}{} Pond drained — CA destroyed, all certificates invalidated",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("ok", ctx.term.supports_color)
            );
        }
        Ok(response) => {
            eprintln!(
                "{}{} Failed to remove pond: {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                response.status()
            );
        }
        Err(e) => {
            eprintln!(
                "{}{} Request failed: {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                e
            );
        }
    }

    Ok(())
}

async fn execute_pond_untrust(
    ctx: &CommandContext,
    endpoint: &str,
    stone_name: &str,
) -> anyhow::Result<()> {
    let url = format!(
        "{}/api/v1/pond/stones/{}",
        endpoint.trim_end_matches('/'),
        stone_name
    );

    match ctx.client.delete(&url).send().await {
        Ok(response) if response.status().is_success() => {
            println!(
                "{}{} Revoked {} from pond",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("ok", ctx.term.supports_color),
                stone_name
            );
        }
        Ok(response) => {
            eprintln!(
                "{}{} Failed to untrust stone: {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                response.status()
            );
        }
        Err(e) => {
            eprintln!(
                "{}{} Request failed: {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                e
            );
        }
    }

    Ok(())
}

async fn execute_pond_unlock(
    ctx: &CommandContext,
    endpoint: &str,
    passphrase: Option<String>,
    totp: Option<String>,
) -> anyhow::Result<()> {
    let url = format!("{}/api/v1/pond/unlock", endpoint.trim_end_matches('/'));

    let payload = if let Some(code) = totp {
        serde_json::json!({ "totp_code": code })
    } else {
        let pass = passphrase.unwrap_or_else(|| {
            println!(
                "{}{} Using default passphrase for unlock. Use --passphrase to specify.",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("info", ctx.term.supports_color)
            );
            "changeme".to_string()
        });
        serde_json::json!({ "passphrase": pass })
    };

    match ctx.client.post(&url).json(&payload).send().await {
        Ok(response) if response.status().is_success() => {
            let body: serde_json::Value = response.json().await.unwrap_or_default();
            let msg = body
                .pointer("/data/message")
                .and_then(|v| v.as_str())
                .unwrap_or("Pond unlocked \u{2014} CA key decrypted");
            println!(
                "{}{} {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("ok", ctx.term.supports_color),
                msg
            );
        }
        Ok(response) => {
            let status = response.status();
            let body: serde_json::Value = response.json().await.unwrap_or_default();
            let msg = crate::api::responses::extract_error_message(&body)
                .unwrap_or_else(|| format!("Unexpected error (HTTP {status})"));
            eprintln!(
                "{}{} {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                msg
            );
        }
        Err(e) => {
            eprintln!(
                "{}{} Request failed: {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                e
            );
        }
    }

    Ok(())
}

async fn execute_pond_promote(
    ctx: &CommandContext,
    endpoint: &str,
    passphrase: Option<String>,
) -> anyhow::Result<()> {
    let pass = passphrase.unwrap_or_else(|| {
        println!(
            "{}{} Using default passphrase for promote. Use --passphrase to specify.",
            " ".repeat(ui::constants::DEFAULT_INDENT),
            ui::status_indicator("info", ctx.term.supports_color)
        );
        "changeme".to_string()
    });

    let url = format!("{}/api/v1/pond/promote", endpoint.trim_end_matches('/'));
    let payload = serde_json::json!({ "passphrase": pass });

    match ctx.client.post(&url).json(&payload).send().await {
        Ok(response) if response.status().is_success() => {
            println!(
                "{}{} Stone promoted — received CA key material",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("ok", ctx.term.supports_color)
            );
            if let Ok(body) = response.json::<serde_json::Value>().await {
                if let Some(data) = body.get("data") {
                    if let Some(fp) = data.get("ca_fingerprint").and_then(|f| f.as_str()) {
                        println!("   CA fingerprint: {}", fp);
                    }
                }
            }
        }
        Ok(response) => {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            eprintln!(
                "{}{} Failed to promote stone: {} {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                status,
                body
            );
        }
        Err(e) => {
            eprintln!(
                "{}{} Request failed: {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                e
            );
        }
    }

    Ok(())
}

async fn execute_pond_rename(
    ctx: &CommandContext,
    endpoint: &str,
    name: Option<String>,
) -> anyhow::Result<()> {
    let url = format!("{}/api/v1/pond/name", endpoint.trim_end_matches('/'));
    let payload = match name {
        Some(ref n) => serde_json::json!({ "name": n }),
        None => serde_json::json!({}),
    };

    match ctx.client.put(&url).json(&payload).send().await {
        Ok(response) if response.status().is_success() => {
            if let Ok(body) = response.json::<serde_json::Value>().await {
                if let Some(data) = body.get("data") {
                    let new_name = data
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("unknown");
                    println!(
                        "{}{} Pond renamed to '{}'",
                        " ".repeat(ui::constants::DEFAULT_INDENT),
                        ui::status_indicator("ok", ctx.term.supports_color),
                        new_name
                    );
                }
            }
        }
        Ok(response) => {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            eprintln!(
                "{}{} Failed to rename pond: {} {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                status,
                body
            );
        }
        Err(e) => {
            eprintln!(
                "{}{} Request failed: {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                e
            );
        }
    }

    Ok(())
}
