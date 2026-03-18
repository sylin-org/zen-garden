//! Hey-tell command for Companion communication
//!
//! Syntax:
//!   hey tell {Companion} [args...]           ? Send to tended stone
//!   hey {stone} tell {Companion} [args...]   ? Send to specific stone
//!
//! Rake is a thin pass-through. All args after Companion name are passed raw.
//! The Companion owns its command structure and validation.

use async_trait::async_trait;

use crate::commands::{Command, CommandResult};
use crate::context::Runtime;
use garden_common::command_manifest::{CommandManifest, CommandResponse, CompanionCommandRequest};

/// Parsed hey command
#[derive(Debug, Clone)]
pub enum HeyCommand {
    /// Show help for hey
    Help,
    /// Show help for a specific token (e.g., "tell", "cricket")
    HelpFor(String),
    /// List all registered Companions
    ListCompanions,
    /// Show Companion's command manifest
    CompanionHelp(String),
    /// Enable Companion
    EnableCompanion(String),
    /// Disable Companion
    DisableCompanion(String),
    /// Send raw command to Companion
    SendCommand {
        companion: String,
        raw_args: Vec<String>,
    },
    /// Unknown subcommand
    Unknown(String),
}

/// Parse hey command from args
///
/// Syntax:
///   hey?                              ? Help
///   hey tell?                         ? Help for tell
///   hey tell                          ? List Companions  
///   hey tell {Companion}?               ? Companion help
///   hey tell {Companion} on             ? Enable
///   hey tell {Companion} off            ? Disable
///   hey tell {Companion} [args...]      ? Send command
///   hey {stone} tell {Companion} [...]  ? Send to specific stone (returns stone name)
pub fn parse_hey_command(args: &[String]) -> (HeyCommand, Option<String>) {
    if args.is_empty() {
        return (HeyCommand::Help, None);
    }

    let first = &args[0];

    // Check for help suffix on first arg
    if first.ends_with('?') {
        let token = first.trim_end_matches('?');
        if token.is_empty() {
            return (HeyCommand::Help, None);
        }
        return (HeyCommand::HelpFor(token.to_string()), None);
    }

    // Check if first arg is "tell" or a stone name
    match first.as_str() {
        "tell" => {
            let (cmd, _) = parse_tell_command(&args[1..]);
            (cmd, None)
        }
        stone_name => {
            // Check if second arg is "tell"
            if args.len() >= 2 && args[1] == "tell" {
                let (cmd, _) = parse_tell_command(&args[2..]);
                (cmd, Some(stone_name.to_string()))
            } else {
                (HeyCommand::Unknown(first.clone()), None)
            }
        }
    }
}

/// Parse the tell subcommand
fn parse_tell_command(args: &[String]) -> (HeyCommand, Option<String>) {
    // No args = list Companions
    if args.is_empty() {
        return (HeyCommand::ListCompanions, None);
    }

    let target = &args[0];

    // Help for companion (ends with ?)
    if target.ends_with('?') {
        let companion = target.trim_end_matches('?');
        if companion.is_empty() {
            return (HeyCommand::HelpFor("tell".to_string()), None);
        }
        return (HeyCommand::CompanionHelp(companion.to_string()), None);
    }

    // Lifecycle commands: up/down
    if args.len() >= 2 {
        match args[1].as_str() {
            "up" => return (HeyCommand::EnableCompanion(target.clone()), None),
            "down" => return (HeyCommand::DisableCompanion(target.clone()), None),
            _ => {}
        }
    }

    // Pass remaining args to Companion (raw, no parsing)
    (
        HeyCommand::SendCommand {
            companion: target.clone(),
            raw_args: args[1..].to_vec(),
        },
        None,
    )
}

/// Hey command handler
pub struct HeyTellCommand {
    /// Raw args after "hey"
    pub args: Vec<String>,
}

#[async_trait]
impl Command for HeyTellCommand {
    fn name(&self) -> &'static str {
        "hey"
    }

    fn requires_endpoint(&self) -> bool {
        true
    }

    fn show_stone_header(&self) -> bool {
        false
    }

    async fn execute(&self, ctx: &Runtime) -> CommandResult {
        let (cmd, target_stone) = parse_hey_command(&self.args);

        // Determine endpoint - use target stone if specified, else tended
        let endpoint = if let Some(stone) = &target_stone {
            // Resolve stone name to endpoint via discovery or direct
            resolve_stone_endpoint(stone).await?
        } else {
            ctx.endpoint()?.to_string()
        };

        execute_hey_command(cmd, &endpoint, ctx).await
    }
}

/// Resolve stone name to endpoint
async fn resolve_stone_endpoint(stone: &str) -> anyhow::Result<String> {
    // If it looks like a URL, use directly
    if stone.starts_with("http://") || stone.starts_with("https://") {
        return Ok(stone.to_string());
    }

    // Stone names resolve via mDNS/DNS, so http://{name}:{port} is the standard pattern
    Ok(format!(
        "http://{}:{}",
        stone,
        garden_common::constants::MOSS_HTTP
    ))
}

/// Execute parsed hey command
async fn execute_hey_command(cmd: HeyCommand, endpoint: &str, ctx: &Runtime) -> CommandResult {
    match cmd {
        HeyCommand::Help => {
            print_hey_help();
            Ok(())
        }

        HeyCommand::HelpFor(token) => {
            match token.as_str() {
                "tell" => print_tell_help(),
                companion => show_companion_commands(endpoint, companion, ctx).await?,
            }
            Ok(())
        }

        HeyCommand::ListCompanions => list_companions(endpoint, ctx).await,

        HeyCommand::CompanionHelp(companion) => {
            show_companion_commands(endpoint, &companion, ctx).await
        }

        HeyCommand::EnableCompanion(companion) => {
            companion_lifecycle(endpoint, &companion, "enable", ctx).await
        }

        HeyCommand::DisableCompanion(companion) => {
            companion_lifecycle(endpoint, &companion, "disable", ctx).await
        }

        HeyCommand::SendCommand {
            companion,
            raw_args,
        } => send_companion_command(endpoint, &companion, &raw_args, ctx).await,

        HeyCommand::Unknown(token) => {
            eprintln!("Unknown subcommand: {}", token);
            eprintln!("Try: garden-rake hey?");
            anyhow::bail!("Unknown subcommand: {}", token)
        }
    }
}

// =============================================================================
// Help functions
// =============================================================================

fn print_hey_help() {
    println!("hey - Communicate with Zen Garden Companions");
    println!();
    println!("Subcommands:");
    println!("  tell    Send commands to registered Companions");
    println!();
    println!("Usage:");
    println!("  garden-rake hey tell                     List Companions");
    println!("  garden-rake hey tell {{Companion}}           Send command to Companion");
    println!("  garden-rake hey {{stone}} tell {{Companion}}  Send to specific stone");
    println!();
    println!("Examples:");
    println!("  garden-rake hey tell cricket select mr-robot");
    println!("  garden-rake hey tell firefly brightness 80");
    println!("  garden-rake hey stone-01 tell cricket volume 50");
}

fn print_tell_help() {
    println!("hey tell - Send commands to Companions");
    println!();
    println!("Usage:");
    println!("  hey tell                    List registered Companions");
    println!("  hey tell {{Companion}}?        Show Companion commands");
    println!("  hey tell {{Companion}} on      Enable Companion");
    println!("  hey tell {{Companion}} off     Disable Companion");
    println!("  hey tell {{Companion}} [args]  Send command (args passed raw)");
    println!();
    println!("Tip: Use 'hey tell {{Companion}}?' to see available commands");
}

// =============================================================================
// API functions - thin pass-through
// =============================================================================

/// List all registered Companions
async fn list_companions(endpoint: &str, ctx: &Runtime) -> CommandResult {
    let url = format!("{}/api/v1/stone/companions", endpoint);
    let response = ctx.client.get(&url).send().await?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to list Companions: {}", response.status());
    }

    // ApiResponse wraps data in { "data": { "companions": [...] } }
    let body: serde_json::Value = response.json().await?;

    let companions = body
        .get("data")
        .and_then(|d| d.get("companions"))
        .and_then(|a| a.as_array())
        .map(|a| a.to_vec())
        .unwrap_or_default();

    if companions.is_empty() {
        println!("No Companions registered.");
        println!();
        println!("  ? Install an Companion: sudo apt install garden-cricket");
        println!(
            "  ? Or copy Companion to {}/companions/",
            garden_common::constants::paths::data_dir()
        );
        return Ok(());
    }

    println!("Registered Companions:");
    println!();

    for companion in &companions {
        let id = companion.get("id").and_then(|v| v.as_str()).unwrap_or("?");
        let version = companion
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let description = companion
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let running = companion
            .get("running")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let pid = companion.get("pid").and_then(|v| v.as_u64());

        let status_icon = "?";
        let status_text = if running {
            if let Some(p) = pid {
                format!(" [PID {}]", p)
            } else {
                " [running]".to_string()
            }
        } else {
            " [stopped]".to_string()
        };

        println!("  {} {} (v{}){}", status_icon, id, version, status_text);
        if !description.is_empty() {
            println!("    {}", description);
        }
    }

    println!();
    println!(
        "Tip: Use 'hey tell {{companion}} up' to start, 'hey tell {{companion}}?' for commands"
    );

    Ok(())
}

/// Show companion's command manifest (fetched from Moss)
async fn show_companion_commands(endpoint: &str, companion: &str, ctx: &Runtime) -> CommandResult {
    let url = format!("{}/api/v1/stone/companions/{}", endpoint, companion);
    let response = ctx.client.get(&url).send().await?;

    if !response.status().is_success() {
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            anyhow::bail!("Companion '{}' not found", companion);
        }
        anyhow::bail!("Failed to get companion info: {}", response.status());
    }

    // ApiResponse wraps in { "data": { ...manifest fields... } }
    let body: serde_json::Value = response.json().await?;
    let data = body
        .get("data")
        .ok_or_else(|| anyhow::anyhow!("Invalid response format"))?;
    let manifest: CommandManifest = serde_json::from_value(data.clone())?;

    println!("{} - {}", manifest.name, manifest.description);
    println!("Version: {}", manifest.version);
    println!();
    println!("Commands:");
    println!();

    for cmd in &manifest.commands {
        // Build args string
        let args_str = cmd.args_syntax();

        if args_str.is_empty() {
            println!("  {}", cmd.name);
        } else {
            println!("  {} {}", cmd.name, args_str);
        }
        println!("    {}", cmd.description);

        // Show long description if available
        if let Some(ref long_desc) = cmd.long_description {
            // Print wrapped
            for line in long_desc.lines() {
                println!("    {}", line);
            }
        }

        // Show first example if available
        if let Some(first) = cmd.examples.first() {
            println!("    Example: {}", first.command);
        }

        println!();
    }

    Ok(())
}

/// Start or stop Companion
async fn companion_lifecycle(
    endpoint: &str,
    companion: &str,
    action: &str,
    _ctx: &Runtime,
) -> CommandResult {
    // Map enable/disable to up/down
    let api_action = match action {
        "enable" => "up",
        "disable" => "down",
        _ => action,
    };

    let url = format!(
        "{}/api/v1/stone/companions/{}/{}",
        endpoint, companion, api_action
    );
    let client = reqwest::Client::new();
    let response = client.post(&url).send().await?;

    if response.status().is_success() {
        let body: serde_json::Value = response.json().await.unwrap_or_default();
        let running = body
            .get("running")
            .and_then(|r| r.as_bool())
            .unwrap_or(false);
        let pid = body.get("pid").and_then(|p| p.as_u64());
        let message = body.get("message").and_then(|m| m.as_str()).unwrap_or("");

        if running {
            if let Some(p) = pid {
                println!("Companion '{}' started (PID {})", companion, p);
            } else {
                println!("Companion '{}' started", companion);
            }
        } else {
            println!("Companion '{}' stopped", companion);
        }

        if !message.is_empty() && !message.contains(&companion.to_string()) {
            println!("  {}", message);
        }

        Ok(())
    } else {
        let status = response.status();
        let body: serde_json::Value = response.json().await.unwrap_or_default();
        let msg = body
            .get("message")
            .and_then(|e| e.as_str())
            .unwrap_or("Unknown error");
        anyhow::bail!("{}: {}", status, msg)
    }
}

/// Send command to companion - raw pass-through
async fn send_companion_command(
    endpoint: &str,
    companion: &str,
    raw_args: &[String],
    ctx: &Runtime,
) -> CommandResult {
    let url = format!("{}/api/v1/stone/companions/{}/command", endpoint, companion);

    let request = CompanionCommandRequest::new(companion, raw_args.to_vec());

    let response = ctx.client.post(&url).json(&request).send().await?;

    let status = response.status();

    if status.is_success() {
        let body: CommandResponse = response.json().await?;

        // Display based on status
        match body.status {
            garden_common::command_manifest::ResponseStatus::Success => {
                println!("? {}", body.message);
            }
            garden_common::command_manifest::ResponseStatus::Warning => {
                println!("? {}", body.message);
            }
            garden_common::command_manifest::ResponseStatus::Error => {
                eprintln!("? {}", body.message);
            }
        }

        // Show output if present
        if let Some(ref output) = body.output {
            println!("{}", output);
        }

        // Show suggestions
        for suggestion in &body.suggestions {
            println!("  ? {}", suggestion);
        }

        if body.is_error() {
            anyhow::bail!("{}", body.message);
        }

        Ok(())
    } else {
        // Parse CommandResponse structure (status, message, suggestions)
        let body: CommandResponse = response
            .json()
            .await
            .unwrap_or_else(|_| CommandResponse::error("Unknown error"));

        // Show suggestions if available
        for suggestion in &body.suggestions {
            eprintln!("  ? {}", suggestion);
        }

        anyhow::bail!("{}: {}", status, body.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hey_help() {
        let (cmd, stone) = parse_hey_command(&[]);
        assert!(matches!(cmd, HeyCommand::Help));
        assert!(stone.is_none());
    }

    #[test]
    fn test_parse_hey_tell_list() {
        let (cmd, stone) = parse_hey_command(&["tell".to_string()]);
        assert!(matches!(cmd, HeyCommand::ListCompanions));
        assert!(stone.is_none());
    }

    #[test]
    fn test_parse_hey_tell_companion_raw_args() {
        let (cmd, stone) = parse_hey_command(&[
            "tell".to_string(),
            "cricket".to_string(),
            "select".to_string(),
            "mr-robot".to_string(),
        ]);

        match cmd {
            HeyCommand::SendCommand {
                companion,
                raw_args,
            } => {
                assert_eq!(companion, "cricket");
                assert_eq!(raw_args, vec!["select", "mr-robot"]);
            }
            _ => panic!("Expected SendCommand"),
        }
        assert!(stone.is_none());
    }

    #[test]
    fn test_parse_hey_stone_tell() {
        let (cmd, stone) = parse_hey_command(&[
            "stone-01".to_string(),
            "tell".to_string(),
            "cricket".to_string(),
            "volume".to_string(),
            "50".to_string(),
        ]);

        match cmd {
            HeyCommand::SendCommand {
                companion,
                raw_args,
            } => {
                assert_eq!(companion, "cricket");
                assert_eq!(raw_args, vec!["volume", "50"]);
            }
            _ => panic!("Expected SendCommand"),
        }
        assert_eq!(stone, Some("stone-01".to_string()));
    }

    #[test]
    fn test_parse_companion_help() {
        let (cmd, _) = parse_hey_command(&["tell".to_string(), "cricket?".to_string()]);
        assert!(matches!(cmd, HeyCommand::CompanionHelp(a) if a == "cricket"));
    }

    #[test]
    fn test_parse_companion_on_off() {
        let (cmd, _) =
            parse_hey_command(&["tell".to_string(), "cricket".to_string(), "up".to_string()]);
        assert!(matches!(cmd, HeyCommand::EnableCompanion(a) if a == "cricket"));

        let (cmd, _) = parse_hey_command(&[
            "tell".to_string(),
            "cricket".to_string(),
            "down".to_string(),
        ]);
        assert!(matches!(cmd, HeyCommand::DisableCompanion(a) if a == "cricket"));
    }
}
