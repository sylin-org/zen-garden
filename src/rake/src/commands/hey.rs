//! Hey-tell command for Companion communication
//!
//! Syntax:
//!   hey tell {Companion} [args...]           ? Send to tended stone
//!   hey {stone} tell {Companion} [args...]   ? Send to specific stone
//!
//! Rake is a thin pass-through. All args after Companion name are passed raw.
//! The Companion owns its command structure and validation.


use crate::commands::{Command, CommandResult};
use crate::context::Runtime;
use garden_common::client::StoneApi;
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

    fn execute<'a>(&'a self, ctx: &'a Runtime) -> std::pin::Pin<Box<dyn std::future::Future<Output = CommandResult> + Send + 'a>> {
        Box::pin(async move {
            let (cmd, target_stone) = parse_hey_command(&self.args);

            // Determine endpoint - use target stone if specified, else tended
            let endpoint = if let Some(stone) = &target_stone {
                // Resolve stone name to endpoint via discovery or direct
                resolve_stone_endpoint(stone).await?
            } else {
                ctx.endpoint()?.to_string()
            };

            let api = StoneApi::new(ctx.client.clone(), endpoint);
            execute_hey_command(cmd, &api, ctx).await
        })
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
async fn execute_hey_command(cmd: HeyCommand, api: &StoneApi, ctx: &Runtime) -> CommandResult {
    match cmd {
        HeyCommand::Help => {
            print_hey_help();
            Ok(())
        }

        HeyCommand::HelpFor(token) => {
            match token.as_str() {
                "tell" => print_tell_help(),
                companion => show_companion_commands(api, companion).await?,
            }
            Ok(())
        }

        HeyCommand::ListCompanions => list_companions(api).await,

        HeyCommand::CompanionHelp(companion) => {
            show_companion_commands(api, &companion).await
        }

        HeyCommand::EnableCompanion(companion) => {
            companion_lifecycle(api, &companion, "enable").await
        }

        HeyCommand::DisableCompanion(companion) => {
            companion_lifecycle(api, &companion, "disable").await
        }

        HeyCommand::SendCommand {
            companion,
            raw_args,
        } => send_companion_command(api, &companion, &raw_args, ctx).await,

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
// API functions - thin pass-through via StoneApi
// =============================================================================

/// List all registered Companions
async fn list_companions(api: &StoneApi) -> CommandResult {
    let data: serde_json::Value = api
        .companions()
        .list()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to list Companions: {}", e.display_message()))?;

    // StoneApi unwraps ApiResponse, so `data` is the inner payload
    let companions = data
        .get("companions")
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
async fn show_companion_commands(api: &StoneApi, companion: &str) -> CommandResult {
    let data: serde_json::Value = match api.companions().get(companion).await {
        Ok(d) => d,
        Err(e) if e.is_not_found() => {
            anyhow::bail!("Companion '{}' not found", companion);
        }
        Err(e) => {
            anyhow::bail!("Failed to get companion info: {}", e.display_message());
        }
    };

    // StoneApi unwraps ApiResponse, so `data` is the inner payload directly
    let manifest: CommandManifest = serde_json::from_value(data)?;

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
    api: &StoneApi,
    companion: &str,
    action: &str,
) -> CommandResult {
    let result = match action {
        "enable" => api.companions().up(companion).await,
        "disable" => api.companions().down(companion).await,
        _ => unreachable!(),
    };

    match result {
        Ok(body) => {
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
        }
        Err(e) => {
            anyhow::bail!("{}", e.display_message())
        }
    }
}

/// Send command to companion - raw pass-through
async fn send_companion_command(
    api: &StoneApi,
    companion: &str,
    raw_args: &[String],
    _ctx: &Runtime,
) -> CommandResult {
    let request = CompanionCommandRequest::new(companion, raw_args.to_vec());

    // companions().command() returns serde_json::Value (unwrapped from ApiResponse)
    // But we need CommandResponse. Try to deserialize directly.
    // The command endpoint may return the CommandResponse as the data payload.
    match api.companions().command(companion, &request).await {
        Ok(data) => {
            // Try to parse as CommandResponse
            let body: CommandResponse = serde_json::from_value(data)
                .unwrap_or_else(|_| CommandResponse::error("Unexpected response format"));

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

            if let Some(ref output) = body.output {
                println!("{}", output);
            }

            for suggestion in &body.suggestions {
                println!("  ? {}", suggestion);
            }

            if body.is_error() {
                anyhow::bail!("{}", body.message);
            }

            Ok(())
        }
        Err(e) => {
            anyhow::bail!("{}", e.display_message())
        }
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
