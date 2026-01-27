//! Hey-tell command for adapter communication
//!
//! Syntax:
//!   hey tell {adapter} [args...]           → Send to tended stone
//!   hey {stone} tell {adapter} [args...]   → Send to specific stone
//!
//! Rake is a thin pass-through. All args after adapter name are passed raw.
//! The adapter owns its command structure and validation.

use async_trait::async_trait;

use crate::context::CommandContext;
use crate::commands::{Command, CommandResult};
use garden_common::command_manifest::{AdapterCommandRequest, CommandManifest, CommandResponse};

/// Parsed hey command
#[derive(Debug, Clone)]
pub enum HeyCommand {
    /// Show help for hey
    Help,
    /// Show help for a specific token (e.g., "tell", "cricket")
    HelpFor(String),
    /// List all registered adapters
    ListAdapters,
    /// Show adapter's command manifest
    AdapterHelp(String),
    /// Enable adapter
    EnableAdapter(String),
    /// Disable adapter
    DisableAdapter(String),
    /// Send raw command to adapter
    SendCommand {
        adapter: String,
        raw_args: Vec<String>,
    },
    /// Unknown subcommand
    Unknown(String),
}

/// Parse hey command from args
/// 
/// Syntax:
///   hey?                              → Help
///   hey tell?                         → Help for tell
///   hey tell                          → List adapters  
///   hey tell {adapter}?               → Adapter help
///   hey tell {adapter} on             → Enable
///   hey tell {adapter} off            → Disable
///   hey tell {adapter} [args...]      → Send command
///   hey {stone} tell {adapter} [...]  → Send to specific stone (returns stone name)
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
    // No args = list adapters
    if args.is_empty() {
        return (HeyCommand::ListAdapters, None);
    }
    
    let target = &args[0];
    
    // Help for adapter (ends with ?)
    if target.ends_with('?') {
        let adapter = target.trim_end_matches('?');
        if adapter.is_empty() {
            return (HeyCommand::HelpFor("tell".to_string()), None);
        }
        return (HeyCommand::AdapterHelp(adapter.to_string()), None);
    }
    
    // Lifecycle commands: up/down
    if args.len() >= 2 {
        match args[1].as_str() {
            "up" => return (HeyCommand::EnableAdapter(target.clone()), None),
            "down" => return (HeyCommand::DisableAdapter(target.clone()), None),
            _ => {}
        }
    }
    
    // Pass remaining args to adapter (raw, no parsing)
    (HeyCommand::SendCommand {
        adapter: target.clone(),
        raw_args: args[1..].to_vec(),
    }, None)
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
    
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
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
    
    // Try to resolve via observe/topology
    // For now, assume format http://{stone}:7185
    // TODO: Proper discovery lookup
    Ok(format!("http://{}:{}", stone, garden_common::constants::MOSS_HTTP))
}

/// Execute parsed hey command
async fn execute_hey_command(cmd: HeyCommand, endpoint: &str, ctx: &CommandContext) -> CommandResult {
    match cmd {
        HeyCommand::Help => {
            print_hey_help();
            Ok(())
        }
        
        HeyCommand::HelpFor(token) => {
            match token.as_str() {
                "tell" => print_tell_help(),
                adapter => show_adapter_commands(endpoint, adapter, ctx).await?,
            }
            Ok(())
        }
        
        HeyCommand::ListAdapters => {
            list_adapters(endpoint, ctx).await
        }
        
        HeyCommand::AdapterHelp(adapter) => {
            show_adapter_commands(endpoint, &adapter, ctx).await
        }
        
        HeyCommand::EnableAdapter(adapter) => {
            adapter_lifecycle(endpoint, &adapter, "enable", ctx).await
        }
        
        HeyCommand::DisableAdapter(adapter) => {
            adapter_lifecycle(endpoint, &adapter, "disable", ctx).await
        }
        
        HeyCommand::SendCommand { adapter, raw_args } => {
            send_adapter_command(endpoint, &adapter, &raw_args, ctx).await
        }
        
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
    println!("hey - Communicate with Zen Garden adapters");
    println!();
    println!("Subcommands:");
    println!("  tell    Send commands to registered adapters");
    println!();
    println!("Usage:");
    println!("  garden-rake hey tell                     List adapters");
    println!("  garden-rake hey tell {{adapter}}           Send command to adapter");
    println!("  garden-rake hey {{stone}} tell {{adapter}}  Send to specific stone");
    println!();
    println!("Examples:");
    println!("  garden-rake hey tell cricket select mr-robot");
    println!("  garden-rake hey tell firefly brightness 80");
    println!("  garden-rake hey stone-01 tell cricket volume 50");
}

fn print_tell_help() {
    println!("hey tell - Send commands to adapters");
    println!();
    println!("Usage:");
    println!("  hey tell                    List registered adapters");
    println!("  hey tell {{adapter}}?        Show adapter commands");
    println!("  hey tell {{adapter}} on      Enable adapter");
    println!("  hey tell {{adapter}} off     Disable adapter");
    println!("  hey tell {{adapter}} [args]  Send command (args passed raw)");
    println!();
    println!("Tip: Use 'hey tell {{adapter}}?' to see available commands");
}

// =============================================================================
// API functions - thin pass-through
// =============================================================================

/// List all registered adapters
async fn list_adapters(endpoint: &str, ctx: &CommandContext) -> CommandResult {
    let url = format!("{}/api/v1/stone/adapters", endpoint);
    let response = ctx.client.get(&url).send().await?;
    
    if !response.status().is_success() {
        anyhow::bail!("Failed to list adapters: {}", response.status());
    }
    
    // New format from AdapterListResponse
    let body: serde_json::Value = response.json().await?;
    
    let adapters = body.get("adapters")
        .and_then(|a| a.as_array())
        .map(|a| a.to_vec())
        .unwrap_or_default();
    
    if adapters.is_empty() {
        println!("No adapters registered.");
        println!();
        println!("  → Install an adapter: sudo apt install garden-cricket");
        println!("  → Or copy adapter to {}/adapters/", garden_common::constants::paths::data_dir());
        return Ok(());
    }
    
    println!("Registered Adapters:");
    println!();
    
    for adapter in &adapters {
        let id = adapter.get("id").and_then(|v| v.as_str()).unwrap_or("?");
        let version = adapter.get("version").and_then(|v| v.as_str()).unwrap_or("?");
        let description = adapter.get("description").and_then(|v| v.as_str()).unwrap_or("");
        let running = adapter.get("running").and_then(|v| v.as_bool()).unwrap_or(false);
        let pid = adapter.get("pid").and_then(|v| v.as_u64());
        
        let status_icon = if running { "●" } else { "○" };
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
    println!("Tip: Use 'hey tell {{adapter}} up' to start, 'hey tell {{adapter}}?' for commands");
    
    Ok(())
}

/// Show adapter's command manifest (fetched from Moss)
async fn show_adapter_commands(endpoint: &str, adapter: &str, ctx: &CommandContext) -> CommandResult {
    let url = format!("{}/api/v1/stone/adapters/{}", endpoint, adapter);
    let response = ctx.client.get(&url).send().await?;
    
    if !response.status().is_success() {
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            anyhow::bail!("Adapter '{}' not found", adapter);
        }
        anyhow::bail!("Failed to get adapter info: {}", response.status());
    }
    
    // Parse full CommandManifest
    let manifest: CommandManifest = response.json().await?;
    
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

/// Start or stop adapter
async fn adapter_lifecycle(endpoint: &str, adapter: &str, action: &str, _ctx: &CommandContext) -> CommandResult {
    // Map enable/disable to up/down
    let api_action = match action {
        "enable" => "up",
        "disable" => "down",
        _ => action,
    };
    
    let url = format!("{}/api/v1/stone/adapters/{}/{}", endpoint, adapter, api_action);
    let client = reqwest::Client::new();
    let response = client.post(&url).send().await?;
    
    if response.status().is_success() {
        let body: serde_json::Value = response.json().await.unwrap_or_default();
        let running = body.get("running").and_then(|r| r.as_bool()).unwrap_or(false);
        let pid = body.get("pid").and_then(|p| p.as_u64());
        let message = body.get("message").and_then(|m| m.as_str()).unwrap_or("");
        
        if running {
            if let Some(p) = pid {
                println!("✓ Adapter '{}' started (PID {})", adapter, p);
            } else {
                println!("✓ Adapter '{}' started", adapter);
            }
        } else {
            println!("✓ Adapter '{}' stopped", adapter);
        }
        
        if !message.is_empty() && !message.contains(&adapter.to_string()) {
            println!("  {}", message);
        }
        
        Ok(())
    } else {
        let status = response.status();
        let body: serde_json::Value = response.json().await.unwrap_or_default();
        let msg = body.get("message")
            .and_then(|e| e.as_str())
            .unwrap_or("Unknown error");
        anyhow::bail!("{}: {}", status, msg)
    }
}

/// Send command to adapter - raw pass-through
async fn send_adapter_command(
    endpoint: &str, 
    adapter: &str, 
    raw_args: &[String],
    ctx: &CommandContext,
) -> CommandResult {
    let url = format!("{}/api/v1/stone/adapters/{}/command", endpoint, adapter);
    
    let request = AdapterCommandRequest::new(adapter, raw_args.to_vec());
    
    let response = ctx.client
        .post(&url)
        .json(&request)
        .send()
        .await?;
    
    let status = response.status();
    
    if status.is_success() {
        let body: CommandResponse = response.json().await?;
        
        // Display based on status
        match body.status {
            garden_common::command_manifest::ResponseStatus::Success => {
                println!("✓ {}", body.message);
            }
            garden_common::command_manifest::ResponseStatus::Warning => {
                println!("⚠ {}", body.message);
            }
            garden_common::command_manifest::ResponseStatus::Error => {
                eprintln!("✗ {}", body.message);
            }
        }
        
        // Show output if present
        if let Some(ref output) = body.output {
            println!("{}", output);
        }
        
        // Show suggestions
        for suggestion in &body.suggestions {
            println!("  → {}", suggestion);
        }
        
        if body.is_error() {
            anyhow::bail!("{}", body.message);
        }
        
        Ok(())
    } else {
        let body: serde_json::Value = response.json().await.unwrap_or_default();
        let msg = body.get("error")
            .and_then(|e| e.as_str())
            .unwrap_or("Unknown error");
        anyhow::bail!("{}: {}", status, msg)
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
        assert!(matches!(cmd, HeyCommand::ListAdapters));
        assert!(stone.is_none());
    }
    
    #[test]
    fn test_parse_hey_tell_adapter_raw_args() {
        let (cmd, stone) = parse_hey_command(&[
            "tell".to_string(),
            "cricket".to_string(),
            "select".to_string(),
            "mr-robot".to_string(),
        ]);
        
        match cmd {
            HeyCommand::SendCommand { adapter, raw_args } => {
                assert_eq!(adapter, "cricket");
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
            HeyCommand::SendCommand { adapter, raw_args } => {
                assert_eq!(adapter, "cricket");
                assert_eq!(raw_args, vec!["volume", "50"]);
            }
            _ => panic!("Expected SendCommand"),
        }
        assert_eq!(stone, Some("stone-01".to_string()));
    }
    
    #[test]
    fn test_parse_adapter_help() {
        let (cmd, _) = parse_hey_command(&["tell".to_string(), "cricket?".to_string()]);
        assert!(matches!(cmd, HeyCommand::AdapterHelp(a) if a == "cricket"));
    }
    
    #[test]
    fn test_parse_adapter_on_off() {
        let (cmd, _) = parse_hey_command(&["tell".to_string(), "cricket".to_string(), "up".to_string()]);
        assert!(matches!(cmd, HeyCommand::EnableAdapter(a) if a == "cricket"));
        
        let (cmd, _) = parse_hey_command(&["tell".to_string(), "cricket".to_string(), "down".to_string()]);
        assert!(matches!(cmd, HeyCommand::DisableAdapter(a) if a == "cricket"));
    }
}
