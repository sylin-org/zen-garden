# Hey-Tell Syntax Specification

**Status:** Draft  
**Date:** 2026-01-26  
**Scope:** Rake command syntax for adapter communication

---

## Design Philosophy

The "hey tell" syntax provides a **natural, conversational interface** for interacting with Zen Garden adapters. Think of it as talking to your infrastructure:

> "Hey, tell cricket to play the mr-robot tune"

Translated to CLI:
```bash
garden-rake hey tell cricket select mr-robot
```

---

## Command Grammar

```
garden-rake hey [subcommand] [target] [command] [args...]

hey?                        → Help for hey command
hey tell?                   → Help for tell subcommand
hey tell                    → List all registered adapters
hey tell {adapter}?         → Show adapter's command manifest
hey tell {adapter} on       → Enable + start adapter
hey tell {adapter} off      → Disable + stop adapter  
hey tell {adapter} {cmd}    → Send command to adapter
```

### Token Rules

| Position | Token | Description |
|----------|-------|-------------|
| 1 | `hey` | Entry point to communication subsystem |
| 2 | `tell` | Subcommand (currently only one) |
| 3 | `{adapter}` | Target adapter name |
| 4+ | `{command} [args]` | Command and arguments |

### Help Suffix `?`

The `?` suffix can be appended to get context-sensitive help:

```bash
garden-rake hey?              # Help for 'hey'
garden-rake hey tell?         # Help for 'tell'  
garden-rake hey tell cricket? # Help for 'cricket' (command list)
```

**No space before `?`** - it's part of the token.

---

## Complete Examples

### Help & Discovery

```bash
# What can I do with 'hey'?
$ garden-rake hey?

hey - Communicate with Zen Garden adapters

Subcommands:
  tell    Send commands to registered adapters

Usage:
  garden-rake hey tell               List adapters
  garden-rake hey tell {adapter}     Send command

Examples:
  garden-rake hey tell cricket select mr-robot
  garden-rake hey tell firefly brightness 80
```

```bash
# What adapters are registered?
$ garden-rake hey tell

Registered Adapters:

  ● cricket (v0.1.0)
    Audio presence adapter - ambient soundscapes
  ○ firefly (v0.1.0) (disabled)
    LED matrix adapter - visual presence

Tip: Use 'garden-rake hey tell {adapter}?' to see commands
```

```bash
# What can cricket do?
$ garden-rake hey tell cricket?

cricket Commands:

  select <tune>
    Switch to a different tune
    Example: garden-rake hey tell cricket select mr-robot

  list
    List installed tunes

  volume <level>
    Set master volume (0-100)
    Example: garden-rake hey tell cricket volume 40

  pull <url>
    Download and install tune from URL

  remove <tune>
    Uninstall a community tune

  status
    Show current tune and settings
```

### Lifecycle Management

```bash
# Enable cricket
$ garden-rake hey tell cricket on
✓ Adapter 'cricket' enabled and started

# Disable cricket
$ garden-rake hey tell cricket off
✓ Adapter 'cricket' disabled and stopped
```

### Sending Commands

```bash
# List tunes
$ garden-rake hey tell cricket list

Installed Tunes:

  Official:
    ● zen-garden (active)    Calm ambient soundscape
    ○ mr-robot               Industrial/tech atmosphere
    ○ silent                 No audio (debugging)

  Community:
    ○ cyberpunk-night        Neon city ambient

Tip: Use 'select <tune>' to switch
```

```bash
# Select tune
$ garden-rake hey tell cricket select mr-robot
✓ Switched to tune 'mr-robot'

# Set volume
$ garden-rake hey tell cricket volume 40
✓ Volume set to 40%

# Pull new tune
$ garden-rake hey tell cricket pull https://zg-tunes.com/deep-forest.tar.gz
↓ Downloading deep-forest.tar.gz...
✓ Tune 'deep-forest' installed (4 samples, 12.4 MB)

# Show status
$ garden-rake hey tell cricket status

Cricket Status:
  Tune:     mr-robot
  Volume:   40%
  Channels:
    foreground:  ▃▃▃▄▄▅▅▆
    midground:   ▂▂▂▃▃▃▄▄
    ambient:     ▁▁▁▂▂▂▂▂
    background:  ▁▁▁▁▁▁▁▁
```

---

## Rake Implementation

### Command Parser

```rust
pub fn parse_hey_command(args: &[String]) -> HeyCommand {
    if args.is_empty() {
        return HeyCommand::Help;
    }
    
    let first = &args[0];
    
    // Check for help suffix
    if first.ends_with('?') {
        let token = first.trim_end_matches('?');
        return HeyCommand::HelpFor(token.to_string());
    }
    
    match first.as_str() {
        "tell" => parse_tell_command(&args[1..]),
        _ => HeyCommand::Unknown(first.clone()),
    }
}

fn parse_tell_command(args: &[String]) -> HeyCommand {
    // No args = list adapters
    if args.is_empty() {
        return HeyCommand::ListAdapters;
    }
    
    let target = &args[0];
    
    // Help for adapter
    if target.ends_with('?') {
        let adapter = target.trim_end_matches('?');
        return HeyCommand::AdapterHelp(adapter.to_string());
    }
    
    // Lifecycle commands
    if args.len() >= 2 {
        match args[1].as_str() {
            "on" => return HeyCommand::EnableAdapter(target.clone()),
            "off" => return HeyCommand::DisableAdapter(target.clone()),
            _ => {}
        }
    }
    
    // Pass remaining args to adapter
    HeyCommand::SendCommand {
        adapter: target.clone(),
        raw_args: args[1..].to_vec(),
    }
}

pub enum HeyCommand {
    Help,
    HelpFor(String),
    ListAdapters,
    AdapterHelp(String),
    EnableAdapter(String),
    DisableAdapter(String),
    SendCommand {
        adapter: String,
        raw_args: Vec<String>,
    },
    Unknown(String),
}
```

### Command Executor

```rust
pub async fn execute_hey_command(cmd: HeyCommand, endpoint: &str) -> Result<()> {
    match cmd {
        HeyCommand::Help => {
            print_hey_help();
        }
        
        HeyCommand::HelpFor(token) => {
            match token.as_str() {
                "" | "hey" => print_hey_help(),
                "tell" => print_tell_help(),
                adapter => show_adapter_commands(endpoint, adapter).await?,
            }
        }
        
        HeyCommand::ListAdapters => {
            list_adapters(endpoint).await?;
        }
        
        HeyCommand::AdapterHelp(adapter) => {
            show_adapter_commands(endpoint, &adapter).await?;
        }
        
        HeyCommand::EnableAdapter(adapter) => {
            adapter_lifecycle(endpoint, &adapter, "enable").await?;
        }
        
        HeyCommand::DisableAdapter(adapter) => {
            adapter_lifecycle(endpoint, &adapter, "disable").await?;
        }
        
        HeyCommand::SendCommand { adapter, raw_args } => {
            send_adapter_command(endpoint, &adapter, &raw_args).await?;
        }
        
        HeyCommand::Unknown(token) => {
            eprintln!("Unknown subcommand: {}", token);
            eprintln!("Try: garden-rake hey?");
            std::process::exit(1);
        }
    }
    
    Ok(())
}
```

---

## Output Formatting

### Status Icons

| Icon | Meaning |
|------|---------|
| `●` | Running/Active |
| `○` | Stopped/Inactive |
| `✓` | Success |
| `✗` | Error |
| `↓` | Downloading |
| `⚠` | Warning |

**Note:** Falls back to ASCII (`*`, `o`, `[OK]`, etc.) when `NO_COLOR` or unicode unsupported.

### Response Handling

```rust
pub async fn send_adapter_command(
    endpoint: &str, 
    adapter: &str, 
    raw_args: &[String]
) -> Result<()> {
    let url = format!("{}/api/v1/stone/presence/command", endpoint);
    
    let request = AdapterCommandRequest {
        adapter: adapter.to_string(),
        raw_args: raw_args.to_vec(),
    };
    
    let response = reqwest::Client::new()
        .post(&url)
        .json(&request)
        .timeout(Duration::from_secs(6)) // 5s adapter + 1s margin
        .send()
        .await?;
    
    let result: AdapterCommandResponse = response.json().await?;
    
    // Format based on status
    match result.status {
        ResponseStatus::Success => {
            println!("✓ {}", result.message);
            if let Some(output) = result.output {
                println!("{}", output);
            }
        }
        ResponseStatus::Warning => {
            println!("⚠ {}", result.message);
            if !result.suggestions.is_empty() {
                println!("\nSuggestions:");
                for suggestion in &result.suggestions {
                    println!("  → {}", suggestion);
                }
            }
        }
        ResponseStatus::Error => {
            eprintln!("✗ {}", result.message);
            if !result.suggestions.is_empty() {
                println!("\nDid you mean:");
                for suggestion in &result.suggestions {
                    println!("  → {}", suggestion);
                }
            }
            std::process::exit(1);
        }
    }
    
    Ok(())
}
```

---

## Error Handling

### Unknown Adapter

```bash
$ garden-rake hey tell foobar status

✗ Unknown adapter: 'foobar'

Did you mean:
  → cricket
  → firefly
```

### Adapter Not Running

```bash
$ garden-rake hey tell cricket status

✗ Adapter 'cricket' is not running

Suggestions:
  → Enable with: garden-rake hey tell cricket on
```

### Command Timeout

```bash
$ garden-rake hey tell cricket pull https://slow-server.com/huge.tar.gz

✗ Request timed out (adapter did not respond in 5s)

Suggestions:
  → Check adapter logs: journalctl -u garden-cricket -f
  → Restart adapter: garden-rake hey tell cricket off && garden-rake hey tell cricket on
```

### Invalid Command

```bash
$ garden-rake hey tell cricket foo

✗ Unknown command: 'foo'

Did you mean:
  → volume
  → status

Available commands: select, list, volume, pull, remove, status
```

---

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `ZG_STONE` | (auto-discover) | Target stone endpoint |
| `ZG_UNICODE` | (auto-detect) | Force unicode output |
| `ZG_NO_COLOR` | `false` | Disable colors |
| `ZG_QUIET` | `false` | Minimal output |

---

## Future Extensions

### Planned Subcommands

```bash
garden-rake hey ask {adapter} {question}    # Query state
garden-rake hey watch {adapter}             # Stream events
garden-rake hey link {adapter} {device}     # Associate hardware
```

### Natural Language (Maybe)

```bash
# These could parse to the same command:
garden-rake hey tell cricket to play mr-robot
garden-rake hey tell cricket select mr-robot
garden-rake hey tell cricket play mr-robot
```

---

## Related Documents

- [ADAPTER-SERVICE-REGISTRY.md](ADAPTER-SERVICE-REGISTRY.md) - Service registration
- [ADAPTER-COMMAND-PROTOCOL.md](ADAPTER-COMMAND-PROTOCOL.md) - Command flow
- [CRICKET-SPEC.md](CRICKET-SPEC.md) - Cricket implementation

---

**Document Status:** Draft  
**Last Updated:** 2026-01-26
