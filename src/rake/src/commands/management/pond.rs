//! Pond command - pond security management
//!
//! Manages multi-stone trust network (pond) operations:
//! - init: Initialize pond security (place keystone)
//! - status: Show pond status
//! - invite: Generate TOTP invitation for enrollment
//! - join: Join pond with TOTP code
//! - unlock: Unlock pond CA after restart
//! - drain: Drain pond (destroy CA)
//! - remove: Remove a stone from the pond (alias for untrust)
//! - untrust: Revoke a stone from pond
//! - promote: Promote this stone to standby CA
//! - rename: Rename the pond (decorative)

use crate::command_manifest::cmd;
use crate::commands::{Command, CommandResult};
use crate::context::Context;
use crate::enrollment;
use crate::suggestions;
use crate::ui::rendering as ui;
use anyhow::Context as _;
use garden_common::client::StoneApi;

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
    /// Join pond with TOTP code (stone enrollment via Moss)
    Join { code: String },
    /// Install enrolled CA certificate into the OS trust store (requires admin)
    Trust,
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
    pub quiet: bool,
}

impl PondCommand {
    pub fn new(action: PondActionType, quiet: bool) -> Self {
        Self { action, quiet }
    }
}

impl Command for PondCommand {
    fn execute<'a>(&'a self, ctx: &'a Context) -> std::pin::Pin<Box<dyn std::future::Future<Output = CommandResult> + Send + 'a>> {
        Box::pin(async move {
            // Trust is a local operation — it doesn't require a tended stone endpoint.
            if let PondActionType::Trust = &self.action {
                execute_pond_trust(ctx).await?;
                if !ctx.wants_json() {
                    suggestions::print_suggestions(cmd::POND, self.quiet);
                }
                return Ok(());
            }

            let api = ctx.api();

            match &self.action {
                PondActionType::Init {
                    passphrase,
                    profile,
                } => {
                    // Init uses ceremony render loop (raw HTTP protocol)
                    execute_pond_init(ctx, api.endpoint(), passphrase.clone(), profile.clone()).await?;
                }
                PondActionType::Status => {
                    execute_pond_status(ctx, api).await?;
                }
                PondActionType::Invite { passphrase } => {
                    execute_pond_invite(ctx, api, passphrase.clone()).await?;
                }
                PondActionType::Join { code } => {
                    execute_pond_join(ctx, api, code).await?;
                }
                PondActionType::Trust => {
                    // Already handled above (no endpoint needed)
                    unreachable!();
                }
                PondActionType::Unlock { passphrase, totp } => {
                    execute_pond_unlock(ctx, api, passphrase.clone(), totp.clone()).await?;
                }
                PondActionType::Remove => {
                    execute_pond_remove(ctx, api).await?;
                }
                PondActionType::Untrust { stone_name } => {
                    execute_pond_untrust(ctx, api, stone_name).await?;
                }
                PondActionType::Promote { passphrase } => {
                    execute_pond_promote(ctx, api, passphrase.clone()).await?;
                }
                PondActionType::Rename { name } => {
                    execute_pond_rename(ctx, api, name.clone()).await?;
                }
            }

            // Self-teaching suggestions (suppress in JSON mode)
            if !ctx.wants_json() {
                suggestions::print_suggestions(cmd::POND, self.quiet);
            }

            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        cmd::POND
    }

    fn requires_endpoint(&self) -> bool {
        !matches!(&self.action, PondActionType::Trust)
    }
}

async fn execute_pond_init(
    ctx: &Context,
    endpoint: &str,
    passphrase: Option<String>,
    profile: Option<String>,
) -> anyhow::Result<()> {
    let api = garden_common::client::StoneApi::new(
        ctx.client.clone(),
        endpoint.to_string(),
    );
    let ceremony_url = format!("{}/api/v1/pond/ceremony", api.endpoint());

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

async fn execute_pond_status(ctx: &Context, api: &StoneApi) -> anyhow::Result<()> {
    match api.pond().status().await {
        Ok(data) => {
            // JSON output mode — emit raw API response
            if ctx.wants_json() {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&data).unwrap_or_default()
                );
                return Ok(());
            }

            // StoneApi unwraps ApiResponse, so `data` is the inner payload
            let active = data
                .get("active")
                .and_then(|a| a.as_bool())
                .unwrap_or(false);
            let locked = data
                .get("locked")
                .and_then(|l| l.as_bool())
                .unwrap_or(false);
            let name = data.get("name").and_then(|n| n.as_str()).unwrap_or("");
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
        Err(e) => {
            eprintln!(
                "{}{} Failed to get pond status: {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                e.display_message()
            );
        }
    }

    Ok(())
}

async fn execute_pond_invite(
    ctx: &Context,
    api: &StoneApi,
    passphrase: Option<String>,
) -> anyhow::Result<()> {
    let pass = passphrase.ok_or_else(|| {
        anyhow::anyhow!(
            "--passphrase is required for invite (it protects the invitation; there is no default)"
        )
    })?;

    let payload = serde_json::json!({ "passphrase": pass });

    match api.pond().invite(&payload).await {
        Ok(data) => {
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
        Err(e) => {
            eprintln!(
                "{}{} Failed to generate invitation: {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                e.display_message()
            );
        }
    }

    Ok(())
}

async fn execute_pond_join(ctx: &Context, api: &StoneApi, code: &str) -> anyhow::Result<()> {
    let payload = serde_json::json!({ "code": code });

    match api.pond().join(&payload).await {
        Ok(data) => {
            println!(
                "{}{} Joined pond successfully",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("ok", ctx.term.supports_color)
            );
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
        Err(e) => {
            eprintln!(
                "{}{} Failed to join pond: {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                e.display_message()
            );
        }
    }

    Ok(())
}

/// Check if running with elevated / root privileges.
fn is_elevated() -> bool {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("net")
            .args(["session"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(unix)]
    {
        std::process::Command::new("id")
            .arg("-u")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
            .unwrap_or(false)
    }
    #[cfg(not(any(target_os = "windows", unix)))]
    {
        false
    }
}

/// Install CA from existing enrollment certs into the OS trust store.
async fn execute_pond_trust(ctx: &Context) -> anyhow::Result<()> {
    let indent = " ".repeat(ui::constants::DEFAULT_INDENT);
    let hostname = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "unknown".to_string());

    // Must be enrolled first
    if !enrollment::is_enrolled(&hostname) {
        eprintln!(
            "{}{} Not enrolled in a pond. Have this stone's Moss join first: garden-rake pond join",
            indent,
            ui::status_indicator("error", ctx.term.supports_color)
        );
        return Ok(());
    }

    // Read CA cert from enrollment directory
    let ca_path = enrollment::certs_dir(&hostname).join("ca.pem");
    let ca_pem = std::fs::read_to_string(&ca_path)
        .with_context(|| format!("Failed to read CA cert: {}", ca_path.display()))?;

    // Already installed?
    if enrollment::is_ca_installed(&ca_pem) {
        println!(
            "{}{} CA certificate is already in the OS trust store.",
            indent,
            ui::status_indicator("ok", ctx.term.supports_color)
        );
        return Ok(());
    }

    // Need admin
    if !is_elevated() {
        eprintln!(
            "{}{} Installing the CA certificate requires administrator privileges.",
            indent,
            ui::status_indicator("error", ctx.term.supports_color)
        );
        #[cfg(target_os = "windows")]
        eprintln!("{}Re-run in an elevated (Administrator) prompt.", indent);
        #[cfg(unix)]
        eprintln!("{}Re-run with: sudo garden-rake pond trust", indent);
        return Ok(());
    }

    match enrollment::install_ca_in_trust_store(&ca_pem) {
        Ok(()) => {
            println!(
                "{}{} CA certificate installed in OS trust store.",
                indent,
                ui::status_indicator("ok", ctx.term.supports_color)
            );
            println!(
                "{}Browsers and other TLS clients will now trust pond connections.",
                indent
            );
        }
        Err(e) => {
            eprintln!(
                "{}{} {}",
                indent,
                ui::status_indicator("error", ctx.term.supports_color),
                e
            );
        }
    }

    Ok(())
}

async fn execute_pond_remove(ctx: &Context, api: &StoneApi) -> anyhow::Result<()> {
    match api.pond().drain().await {
        Ok(_) => {
            println!(
                "{}{} Pond drained — CA destroyed, all certificates invalidated",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("ok", ctx.term.supports_color)
            );
        }
        Err(e) => {
            eprintln!(
                "{}{} Failed to remove pond: {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                e.display_message()
            );
        }
    }

    Ok(())
}

async fn execute_pond_untrust(
    ctx: &Context,
    api: &StoneApi,
    stone_name: &str,
) -> anyhow::Result<()> {
    match api.pond().revoke(stone_name).await {
        Ok(_) => {
            println!(
                "{}{} Revoked {} from pond",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("ok", ctx.term.supports_color),
                stone_name
            );
        }
        Err(e) => {
            eprintln!(
                "{}{} Failed to untrust stone: {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                e.display_message()
            );
        }
    }

    Ok(())
}

async fn execute_pond_unlock(
    ctx: &Context,
    api: &StoneApi,
    passphrase: Option<String>,
    totp: Option<String>,
) -> anyhow::Result<()> {
    let payload = if let Some(code) = totp {
        serde_json::json!({ "totp_code": code })
    } else {
        let pass = passphrase.ok_or_else(|| {
            anyhow::anyhow!(
                "provide --totp or --passphrase to unlock (there is no default passphrase)"
            )
        })?;
        serde_json::json!({ "passphrase": pass })
    };

    match api.pond().unlock(&payload).await {
        Ok(data) => {
            let msg = data
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Pond unlocked \u{2014} CA key decrypted");
            println!(
                "{}{} {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("ok", ctx.term.supports_color),
                msg
            );
        }
        Err(e) => {
            eprintln!(
                "{}{} {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                e.display_message()
            );
        }
    }

    Ok(())
}

async fn execute_pond_promote(
    ctx: &Context,
    api: &StoneApi,
    passphrase: Option<String>,
) -> anyhow::Result<()> {
    let pass = passphrase.ok_or_else(|| {
        anyhow::anyhow!("--passphrase is required for promote (there is no default passphrase)")
    })?;

    let payload = serde_json::json!({ "passphrase": pass });

    match api.pond().promote(&payload).await {
        Ok(data) => {
            println!(
                "{}{} Stone promoted — received CA key material",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("ok", ctx.term.supports_color)
            );
            if let Some(fp) = data.get("ca_fingerprint").and_then(|f| f.as_str()) {
                println!("   CA fingerprint: {}", fp);
            }
        }
        Err(e) => {
            eprintln!(
                "{}{} Failed to promote stone: {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                e.display_message()
            );
        }
    }

    Ok(())
}

async fn execute_pond_rename(
    ctx: &Context,
    api: &StoneApi,
    name: Option<String>,
) -> anyhow::Result<()> {
    let payload = match name {
        Some(ref n) => serde_json::json!({ "name": n }),
        None => serde_json::json!({}),
    };

    match api.pond().rename(&payload).await {
        Ok(data) => {
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
        Err(e) => {
            eprintln!(
                "{}{} Failed to rename pond: {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                e.display_message()
            );
        }
    }

    Ok(())
}
