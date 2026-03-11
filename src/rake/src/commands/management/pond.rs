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
use crate::context::Runtime;
use crate::enrollment;
use crate::suggestions;
use anyhow::Context;
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
    /// Join pond with TOTP code (stone enrollment via Moss)
    Join { code: String },
    /// Enroll this client machine in a pond (direct to cornerstone)
    Enroll,
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

#[async_trait]
impl Command for PondCommand {
    async fn execute(&self, ctx: &Runtime) -> CommandResult {
        // Enroll and Trust are local operations — they don't require
        // a tended stone endpoint.
        match &self.action {
            PondActionType::Enroll => {
                execute_pond_enroll(ctx).await?;
                if !ctx.wants_json() {
                    suggestions::print_suggestions(cmd::POND, self.quiet);
                }
                return Ok(());
            }
            PondActionType::Trust => {
                execute_pond_trust(ctx).await?;
                if !ctx.wants_json() {
                    suggestions::print_suggestions(cmd::POND, self.quiet);
                }
                return Ok(());
            }
            _ => {}
        }

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
            PondActionType::Enroll | PondActionType::Trust => {
                // Already handled above (no endpoint needed)
                unreachable!();
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
            suggestions::print_suggestions(cmd::POND, self.quiet);
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        cmd::POND
    }

    fn requires_endpoint(&self) -> bool {
        !matches!(&self.action, PondActionType::Enroll | PondActionType::Trust)
    }
}

async fn execute_pond_init(
    ctx: &Runtime,
    endpoint: &str,
    passphrase: Option<String>,
    profile: Option<String>,
) -> anyhow::Result<()> {
    let ceremony_url = format!("{}/api/v1/pond/ceremony", endpoint.trim_end_matches('/'));

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

async fn execute_pond_status(ctx: &Runtime, endpoint: &str) -> anyhow::Result<()> {
    let url = format!("{}/api/v1/pond/status", endpoint.trim_end_matches('/'));

    match ctx.client.get(&url).send().await {
        Ok(response) if response.status().is_success() => {
            if let Ok(body) = response.json::<serde_json::Value>().await {
                // JSON output mode — emit raw API response
                if ctx.wants_json() {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&body).unwrap_or_default()
                    );
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
    ctx: &Runtime,
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

async fn execute_pond_join(ctx: &Runtime, endpoint: &str, code: &str) -> anyhow::Result<()> {
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

/// Client enrollment — discover cornerstone, authenticate, receive and install certs.
///
/// Unlike `pond join` (which delegates to the tended Moss stone), `pond enroll`
/// contacts the cornerstone directly via mDNS and installs certificates on this
/// machine for mTLS access to the pond.
async fn execute_pond_enroll(ctx: &Runtime) -> anyhow::Result<()> {
    use std::time::Duration;

    let indent = " ".repeat(ui::constants::DEFAULT_INDENT);

    // 1. Check if already enrolled
    let hostname = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "unknown".to_string());

    if enrollment::is_enrolled(&hostname) {
        if let Some(meta) = enrollment::load_enrollment(&hostname) {
            println!(
                "{}{} Already enrolled in pond '{}'",
                indent,
                ui::status_indicator("ok", ctx.term.supports_color),
                meta.pond_name
            );
            println!("   Cornerstone: {}", meta.cornerstone);
            println!("   CA fingerprint: {}", meta.ca_fingerprint);
            println!("   Enrolled at: {}", meta.enrolled_at);
            println!("   Cert expires: {}", meta.cert_expires);
            return Ok(());
        }
    }

    // 2. Admin privilege check (advisory — enrollment works without admin,
    //    but OS trust store installation will be skipped)
    let is_admin = is_elevated();

    if !is_admin {
        println!(
            "{}{} Not running with administrator privileges.",
            indent,
            ui::status_indicator("warning", ctx.term.supports_color)
        );
        println!(
            "{}Enrollment will proceed, but the CA certificate will NOT be installed",
            indent
        );
        println!(
            "{}into the OS trust store. Rake mTLS will still work.",
            indent
        );
        println!(
            "{}To install the CA later, run as admin: garden-rake pond trust",
            indent
        );
    }

    // 3. Discover cornerstone via mDNS
    println!("{}Discovering cornerstone via mDNS...", indent);

    let cornerstone = crate::discovery::discover_certmesh_ca(Duration::from_secs(5))?;

    let cornerstone = match cornerstone {
        Some(cs) => cs,
        None => {
            eprintln!(
                "{}{} No cornerstone found on the network.",
                indent,
                ui::status_indicator("error", ctx.term.supports_color)
            );
            eprintln!(
                "{}Ensure a pond is initialized and the cornerstone is online.",
                indent
            );
            return Ok(());
        }
    };

    println!(
        "{}Found: {} at {}",
        indent, cornerstone.name, cornerstone.endpoint
    );
    println!("   CA fingerprint: {}", cornerstone.fingerprint);

    // 4. Prompt for TOTP code
    let code = if cornerstone.auth_method == "totp" {
        print!(
            "{}Enter the 6-digit code from your authenticator app: ",
            indent
        );
        use std::io::Write;
        std::io::stdout().flush()?;
        let mut code = String::new();
        std::io::stdin().read_line(&mut code)?;
        code.trim().to_string()
    } else {
        eprintln!(
            "{}{} Unsupported auth method: {}",
            indent,
            ui::status_indicator("error", ctx.term.supports_color),
            cornerstone.auth_method
        );
        return Ok(());
    };

    if code.is_empty() {
        eprintln!(
            "{}{} No code entered. Enrollment cancelled.",
            indent,
            ui::status_indicator("error", ctx.term.supports_color)
        );
        return Ok(());
    }

    // 5. POST to cornerstone's enroll-client endpoint
    let url = format!(
        "{}/api/v1/pond/enroll-client",
        cornerstone.endpoint.trim_end_matches('/')
    );

    let mut sans = vec![format!("{}.local", hostname)];
    // Add local IPs as SANs
    if let Ok(ip) = local_ip_address::local_ip() {
        sans.push(ip.to_string());
    }

    let payload = serde_json::json!({
        "hostname": hostname,
        "code": code,
        "sans": sans,
    });

    println!("{}Enrolling as '{}'...", indent, hostname);

    let response = ctx
        .client
        .post(&url)
        .json(&payload)
        .timeout(Duration::from_secs(15))
        .send()
        .await;

    match response {
        Ok(resp) if resp.status().is_success() => {
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            let data = body.get("data");

            let ca_cert = data
                .and_then(|d| d.get("ca_cert"))
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let service_cert = data
                .and_then(|d| d.get("service_cert"))
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let service_key = data
                .and_then(|d| d.get("service_key"))
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let ca_fingerprint = data
                .and_then(|d| d.get("ca_fingerprint"))
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let enrolled_hostname = data
                .and_then(|d| d.get("hostname"))
                .and_then(|v| v.as_str())
                .unwrap_or(&hostname);
            let cert_expires = data
                .and_then(|d| d.get("cert_expires"))
                .and_then(|v| v.as_str())
                .unwrap_or_default();

            if ca_cert.is_empty() || service_cert.is_empty() || service_key.is_empty() {
                eprintln!(
                    "{}{} Enrollment response missing certificate data.",
                    indent,
                    ui::status_indicator("error", ctx.term.supports_color)
                );
                return Ok(());
            }

            // 6a. Write certificate files
            let certs_dir = enrollment::write_enrollment_certs(
                enrolled_hostname,
                ca_cert,
                service_cert,
                service_key,
            )?;
            println!("{}Certificates written to {}", indent, certs_dir.display());

            // 6b. Install CA in system trust store (only if admin)
            if is_admin {
                match enrollment::install_ca_in_trust_store(ca_cert) {
                    Ok(()) => {
                        println!("{}CA certificate installed in system trust store", indent);
                    }
                    Err(e) => {
                        eprintln!(
                            "{}{} {}",
                            indent,
                            ui::status_indicator("warning", ctx.term.supports_color),
                            e
                        );
                        eprintln!("{}Browsers may not trust pond HTTPS connections.", indent);
                    }
                }
            } else {
                println!(
                    "{}Skipped OS trust store (not admin). Run: garden-rake pond trust",
                    indent
                );
            }

            // 6c. Write enrollment metadata
            // Extract cornerstone name from mDNS service name ("koi-ca-stone-xxx" -> "stone-xxx")
            let cornerstone_name = cornerstone
                .name
                .strip_prefix("koi-ca-")
                .unwrap_or(&cornerstone.name)
                .to_string();

            let metadata = enrollment::PondEnrollment {
                pond_name: String::new(), // Will be populated from health check later
                cornerstone: cornerstone_name.clone(),
                ca_fingerprint: ca_fingerprint.to_string(),
                enrolled_at: chrono::Utc::now().to_rfc3339(),
                cert_expires: cert_expires.to_string(),
                role: "client".to_string(),
            };
            enrollment::write_enrollment_metadata(enrolled_hostname, &metadata)?;

            println!(
                "\n{}{} Enrolled in pond. HTTPS connections enabled.",
                indent,
                ui::status_indicator("ok", ctx.term.supports_color)
            );
            println!("   Hostname: {}", enrolled_hostname);
            println!("   Cornerstone: {}", cornerstone_name);
            println!("   CA fingerprint: {}", ca_fingerprint);
            println!("   Cert expires: {}", cert_expires);
        }
        Ok(resp) => {
            let status = resp.status();
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            let msg = crate::api::responses::extract_error_message(&body)
                .unwrap_or_else(|| format!("Enrollment failed (HTTP {status})"));
            eprintln!(
                "{}{} {}",
                indent,
                ui::status_indicator("error", ctx.term.supports_color),
                msg
            );
        }
        Err(e) => {
            eprintln!(
                "{}{} Request failed: {}",
                indent,
                ui::status_indicator("error", ctx.term.supports_color),
                e
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
async fn execute_pond_trust(ctx: &Runtime) -> anyhow::Result<()> {
    let indent = " ".repeat(ui::constants::DEFAULT_INDENT);
    let hostname = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "unknown".to_string());

    // Must be enrolled first
    if !enrollment::is_enrolled(&hostname) {
        eprintln!(
            "{}{} Not enrolled in a pond. Run: garden-rake pond enroll",
            indent,
            ui::status_indicator("error", ctx.term.supports_color)
        );
        return Ok(());
    }

    // Already installed?
    if enrollment::is_ca_installed() {
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

    // Read CA cert from enrollment directory
    let ca_path = enrollment::certs_dir(&hostname).join("ca.pem");
    let ca_pem = std::fs::read_to_string(&ca_path)
        .with_context(|| format!("Failed to read CA cert: {}", ca_path.display()))?;

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

async fn execute_pond_remove(ctx: &Runtime, endpoint: &str) -> anyhow::Result<()> {
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
    ctx: &Runtime,
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
    ctx: &Runtime,
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
    ctx: &Runtime,
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
    ctx: &Runtime,
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
