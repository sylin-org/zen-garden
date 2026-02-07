# Hey-Tell Syntax Specification

**Status:** Draft  
**Date:** 2026-01-26  
**Scope:** Rake command syntax for Companion communication

---

## Design Philosophy

The "hey tell" syntax provides a **natural, conversational interface** for interacting with Zen Garden Companions. Think of it as talking to your infrastructure:

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
hey tell                    → List all registered Companions
hey tell {Companion}?         → Show Companion's command manifest
hey tell {Companion} on       → Enable + start Companion
hey tell {Companion} off      → Disable + stop Companion  
hey tell {Companion} {cmd}    → Send command to Companion
```

### Token Rules

| Position | Token | Description |
|----------|-------|-------------|
| 1 | `hey` | Entry point to communication subsystem |
| 2 | `tell` | Subcommand (currently only one) |
| 3 | `{Companion}` | Target Companion name |
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

hey - Communicate with Zen Garden Companions

Subcommands:
  tell    Send commands to registered Companions

Usage:
  garden-rake hey tell               List Companions
  garden-rake hey tell {Companion}     Send command

Examples:
  garden-rake hey tell cricket select mr-robot
  garden-rake hey tell firefly brightness 80
```

```bash
# What Companions are registered?
$ garden-rake hey tell

Registered Companions:

  ● cricket (v0.1.0)
    Audio presence Companion - ambient soundscapes
  ○ firefly (v0.1.0) (disabled)
    LED matrix Companion - visual presence

Tip: Use 'garden-rake hey tell {Companion}?' to see commands
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
✓ Companion 'cricket' enabled and started

# Disable cricket
$ garden-rake hey tell cricket off
✓ Companion 'cricket' disabled and stopped
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
    // No args = list Companions
    if args.is_empty() {
        return HeyCommand::ListCompanions;
    }
    
    let target = &args[0];
    
    // Help for Companion
    if target.ends_with('?') {
        let Companion = target.trim_end_matches('?');
        return HeyCommand::CompanionHelp(Companion.to_string());
    }
    
    // Lifecycle commands
    if args.len() >= 2 {
        match args[1].as_str() {
            "on" => return HeyCommand::EnableCompanion(target.clone()),
            "off" => return HeyCommand::DisableCompanion(target.clone()),
            _ => {}
        }
    }
    
    // Pass remaining args to Companion
    HeyCommand::SendCommand {
        Companion: target.clone(),
        raw_args: args[1..].to_vec(),
    }
}

pub enum HeyCommand {
    Help,
    HelpFor(String),
    ListCompanions,
    CompanionHelp(String),
    EnableCompanion(String),
    DisableCompanion(String),
    SendCommand {
        Companion: String,
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
                Companion => show_Companion_commands(endpoint, Companion).await?,
            }
        }
        
        HeyCommand::ListCompanions => {
            list_Companions(endpoint).await?;
        }
        
        HeyCommand::CompanionHelp(Companion) => {
            show_Companion_commands(endpoint, &Companion).await?;
        }
        
        HeyCommand::EnableCompanion(Companion) => {
            Companion_lifecycle(endpoint, &Companion, "enable").await?;
        }
        
        HeyCommand::DisableCompanion(Companion) => {
            Companion_lifecycle(endpoint, &Companion, "disable").await?;
        }
        
        HeyCommand::SendCommand { Companion, raw_args } => {
            send_Companion_command(endpoint, &Companion, &raw_args).await?;
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
pub async fn send_Companion_command(
    endpoint: &str, 
    Companion: &str, 
    raw_args: &[String]
) -> Result<()> {
    let url = format!("{}/api/v1/stone/presence/command", endpoint);
    
    let request = CompanionCommandRequest {
        Companion: Companion.to_string(),
        raw_args: raw_args.to_vec(),
    };
    
    let response = reqwest::Client::new()
        .post(&url)
        .json(&request)
        .timeout(Duration::from_secs(6)) // 5s Companion + 1s margin
        .send()
        .await?;
    
    let result: CompanionCommandResponse = response.json().await?;
    
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

### Unknown Companion

```bash
$ garden-rake hey tell foobar status

✗ Unknown Companion: 'foobar'

Did you mean:
  → cricket
  → firefly
```

### Companion Not Running

```bash
$ garden-rake hey tell cricket status

✗ Companion 'cricket' is not running

Suggestions:
  → Enable with: garden-rake hey tell cricket on
```

### Command Timeout

```bash
$ garden-rake hey tell cricket pull https://slow-server.com/huge.tar.gz

✗ Request timed out (Companion did not respond in 5s)

Suggestions:
  → Check Companion logs: journalctl -u garden-cricket -f
  → Restart Companion: garden-rake hey tell cricket off && garden-rake hey tell cricket on
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
garden-rake hey ask {Companion} {question}    # Query state
garden-rake hey watch {Companion}             # Stream events
garden-rake hey link {Companion} {device}     # Associate hardware
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

- [Companion-SERVICE-REGISTRY.md](Companion-SERVICE-REGISTRY.md) - Service registration
- [Companion-COMMAND-PROTOCOL.md](Companion-COMMAND-PROTOCOL.md) - Command flow
- [CRICKET-SPEC.md](cricket-spec.md) - Cricket implementation

---

**Document Status:** Draft  
