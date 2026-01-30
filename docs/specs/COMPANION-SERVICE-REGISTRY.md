# Companion Service Registry Specification

**Status:** Draft  
**Date:** 2026-01-26  
**Scope:** Universal service management and communication layer for Zen Garden Companions

---

## Overview

The Companion Service Registry provides a universal mechanism for:
1. **Registration** - Companions declare their capabilities
2. **Discovery** - Rake queries available Companions
3. **Lifecycle** - Enable/disable Companions without uninstalling
4. **Introspection** - Query Companion commands dynamically

**Key principle:** Self-documenting services. Each Companion owns its command manifest.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  STONE                                                      │
│                                                             │
│  ┌───────────────────────────────────────────────────────┐ │
│  │  Companion Registry (Moss-managed)                      │ │
│  │                                                       │ │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐   │ │
│  │  │   Cricket   │  │   Firefly   │  │    OLED     │   │ │
│  │  │   (audio)   │  │    (LED)    │  │  (display)  │   │ │
│  │  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘   │ │
│  │         │                │                │          │ │
│  └─────────┼────────────────┼────────────────┼──────────┘ │
│            │                │                │            │
│            └────────────────┼────────────────┘            │
│                             │                             │
│                    ┌────────▼────────┐                    │
│                    │  Command Bus    │                    │
│                    │  (Moss-managed) │                    │
│                    └────────┬────────┘                    │
│                             │                             │
│                    ┌────────▼────────┐                    │
│                    │    Moss API     │                    │
│                    │  /presence/*    │                    │
│                    └─────────────────┘                    │
└─────────────────────────────────────────────────────────────┘
```

---

## Directory Structure

### Service Manifests

```
/etc/zen-garden/Companions.d/
├── cricket.json           # Cricket's service manifest
├── firefly.json           # Firefly's service manifest (if installed)
└── oled.json              # OLED's service manifest (if installed)
```

### Command Manifests (Read-Only)

```
/usr/share/zen-garden/companions/
└── commands/
    ├── cricket.json       # Cricket's command manifest
    ├── firefly.json       # Firefly's command manifest
    └── oled.json          # OLED's command manifest
```

---

## Service Manifest Schema

**File:** `/etc/zen-garden/Companions.d/{Companion}.json`

```json
{
  "Companion": "cricket",
  "type": "presence",
  "version": "0.1.0",
  "description": "Audio presence Companion - ambient soundscapes",
  "binary": "/usr/local/bin/garden-cricket",
  "systemd_unit": "garden-cricket.service",
  "command_manifest": "/usr/share/zen-garden/companions/commands/cricket.json",
  "enabled": true
}
```

### Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `Companion` | string | ✓ | Unique Companion identifier |
| `type` | string | ✓ | Companion type (`presence`, `display`, `hardware`) |
| `version` | string | ✓ | Semantic version (e.g., "0.1.0") |
| `description` | string | ✓ | Human-readable description |
| `binary` | string | ✓ | Path to executable |
| `systemd_unit` | string | ✓ | Systemd service name |
| `command_manifest` | string | ✓ | Path to command manifest JSON |
| `enabled` | boolean | ✓ | Whether Companion is enabled |

---

## Command Manifest Schema

**File:** `/usr/share/zen-garden/companions/commands/{Companion}.json`

```json
{
  "Companion": "cricket",
  "version": "0.1.0",
  "commands": [
    {
      "name": "select",
      "description": "Switch to a different tune",
      "args": [
        {
          "name": "tune",
          "type": "string",
          "required": true,
          "description": "Tune name (use 'list' to see options)"
        }
      ],
      "examples": [
        "garden-rake hey tell cricket select mr-robot",
        "garden-rake hey tell cricket select silence"
      ]
    },
    {
      "name": "list",
      "description": "List installed tunes",
      "args": [],
      "examples": [
        "garden-rake hey tell cricket list"
      ]
    },
    {
      "name": "volume",
      "description": "Set master volume (0-100)",
      "args": [
        {
          "name": "level",
          "type": "integer",
          "required": true,
          "min": 0,
          "max": 100
        }
      ],
      "examples": [
        "garden-rake hey tell cricket volume 40"
      ]
    },
    {
      "name": "pull",
      "description": "Download and install tune from URL",
      "args": [
        {
          "name": "url",
          "type": "url",
          "required": true
        }
      ],
      "examples": [
        "garden-rake hey tell cricket pull https://zg-tunes.com/?id=3245"
      ]
    },
    {
      "name": "remove",
      "description": "Uninstall a community tune",
      "args": [
        {
          "name": "tune",
          "type": "string",
          "required": true
        }
      ],
      "examples": [
        "garden-rake hey tell cricket remove old-tune"
      ]
    },
    {
      "name": "status",
      "description": "Show current tune and settings",
      "args": [],
      "examples": [
        "garden-rake hey tell cricket status"
      ]
    }
  ]
}
```

### Argument Types

| Type | Validation | Example |
|------|------------|---------|
| `string` | Non-empty string | `"mr-robot"` |
| `integer` | Numeric, optional min/max | `40` |
| `url` | Valid HTTP/HTTPS URL | `"https://..."` |
| `boolean` | `true` or `false` | `true` |
| `enum` | One of listed values | `"on"`, `"off"` |

**Note:** Command manifest is for **help generation only**. Companions parse commands internally and may accept additional undocumented commands.

---

## Rake "hey tell" Syntax

### Full Syntax Reference

```bash
# Help for hey command
garden-rake hey?
# Output: Lists subcommands (currently: tell)

# Help for tell command
garden-rake hey tell?
# Output: "Allows communication with Zen Garden Companions"

# List registered Companions
garden-rake hey tell
# Output: List of Companions with status

# Companion lifecycle
garden-rake hey tell {Companion} on      # Enable + start
garden-rake hey tell {Companion} off     # Disable + stop

# Companion help (query command manifest)
garden-rake hey tell {Companion}?
# Output: Formatted command list from manifest

# Companion commands (passed raw to Companion)
garden-rake hey tell {Companion} {command} [args...]
# Example: garden-rake hey tell cricket select mr-robot
```

### Special Commands (Handled by Rake)

| Command | Handler | Description |
|---------|---------|-------------|
| `on` | Rake/systemd | Enable and start Companion |
| `off` | Rake/systemd | Disable and stop Companion |
| `?` (suffix) | Rake | Show command help |

All other commands are passed **raw** to the Companion.

---

## API Endpoints

### List Companions

**`GET /api/v1/stone/companions`**

Returns all registered Companions with status.

```json
{
  "Companions": [
    {
      "Companion": "cricket",
      "type": "presence",
      "version": "0.1.0",
      "description": "Audio presence Companion",
      "enabled": true,
      "running": true
    },
    {
      "Companion": "firefly",
      "type": "presence",
      "version": "0.1.0",
      "description": "LED matrix Companion",
      "enabled": false,
      "running": false
    }
  ]
}
```

### Get Companion Info

**`GET /api/v1/stone/companions/{Companion}`**

Returns single Companion details including command manifest.

```json
{
  "Companion": "cricket",
  "type": "presence",
  "version": "0.1.0",
  "description": "Audio presence Companion",
  "enabled": true,
  "running": true,
  "commands": [
    {"name": "select", "description": "Switch tune", ...},
    {"name": "list", "description": "List tunes", ...}
  ]
}
```

### Enable/Disable Companion

**`POST /api/v1/stone/companions/{Companion}/enable`**
**`POST /api/v1/stone/companions/{Companion}/disable`**

Manages Companion lifecycle via systemd.

```json
// Response
{
  "status": "success",
  "message": "Companion 'cricket' enabled and started"
}
```

### Send Companion Command

**`POST /api/v1/stone/presence/command`**

See [Companion-COMMAND-PROTOCOL.md](Companion-COMMAND-PROTOCOL.md) for details.

---

## Rake Implementation

### `garden-rake hey tell` (List Companions)

```rust
pub async fn list_Companions(endpoint: &str) -> Result<()> {
    let url = format!("{}/api/v1/stone/companions", endpoint);
    let response: CompanionListResponse = reqwest::get(&url).await?.json().await?;
    
    if response.Companions.is_empty() {
        println!("No Companions registered.");
        println!();
        println!("  → Install an Companion: sudo apt install garden-cricket");
        return Ok(());
    }
    
    println!("Registered Companions:");
    println!();
    
    for Companion in &response.Companions {
        let status_icon = if Companion.running { "●" } else { "○" };
        let enabled_text = if Companion.enabled { "" } else { " (disabled)" };
        
        println!("  {} {} (v{}){}", 
                 status_icon, 
                 Companion.Companion, 
                 Companion.version,
                 enabled_text);
        println!("    {}", Companion.description);
    }
    
    Ok(())
}
```

**Example output:**
```
Registered Companions:

  ● cricket (v0.1.0)
    Audio presence Companion - ambient soundscapes
  ○ firefly (v0.1.0) (disabled)
    LED matrix Companion - visual presence
```

### `garden-rake hey tell {Companion}?` (Show Commands)

```rust
pub async fn show_Companion_commands(endpoint: &str, Companion: &str) -> Result<()> {
    let url = format!("{}/api/v1/stone/companions/{}", endpoint, Companion);
    let response: CompanionInfoResponse = reqwest::get(&url).await?.json().await?;
    
    println!("{} Commands:", Companion);
    println!();
    
    for cmd in &response.commands {
        let args_str = cmd.args.iter()
            .map(|a| if a.required { 
                format!("<{}>", a.name) 
            } else { 
                format!("[{}]", a.name) 
            })
            .collect::<Vec<_>>()
            .join(" ");
        
        println!("  {} {}", cmd.name, args_str);
        println!("    {}", cmd.description);
        
        if let Some(example) = cmd.examples.first() {
            println!("    Example: {}", example);
        }
        println!();
    }
    
    Ok(())
}
```

**Example output:**
```
cricket Commands:

  select <tune>
    Switch to a different tune
    Example: garden-rake hey tell cricket select mr-robot

  list
    List installed tunes
    Example: garden-rake hey tell cricket list

  volume <level>
    Set master volume (0-100)
    Example: garden-rake hey tell cricket volume 40
```

### `garden-rake hey tell {Companion} on/off`

```rust
pub async fn Companion_lifecycle(endpoint: &str, Companion: &str, action: &str) -> Result<()> {
    let url = format!("{}/api/v1/stone/companions/{}/{}", 
                      endpoint, Companion, action);
    
    let response = reqwest::Client::new()
        .post(&url)
        .send()
        .await?;
    
    if response.status().is_success() {
        let result: SuccessResponse = response.json().await?;
        println!("✓ {}", result.message);
    } else {
        let error: ErrorResponse = response.json().await?;
        eprintln!("✗ {}", error.error);
    }
    
    Ok(())
}
```

---

## Moss Implementation

### Companion Registry

```rust
pub struct CompanionRegistry {
    Companions: HashMap<String, CompanionInfo>,
    command_bus: broadcast::Sender<InternalCompanionCommand>,
}

impl CompanionRegistry {
    /// Load Companions from /etc/zen-garden/Companions.d/
    pub async fn load_from_disk() -> Result<Self> {
        let mut Companions = HashMap::new();
        
        let dir = Path::new("/etc/zen-garden/Companions.d");
        if dir.exists() {
            for entry in fs::read_dir(dir)? {
                let path = entry?.path();
                if path.extension() == Some(OsStr::new("json")) {
                    let content = fs::read_to_string(&path)?;
                    let manifest: CompanionManifest = serde_json::from_str(&content)?;
                    Companions.insert(manifest.Companion.clone(), CompanionInfo::from(manifest));
                }
            }
        }
        
        let (tx, _) = broadcast::channel(100);
        
        Ok(Self {
            Companions,
            command_bus: tx,
        })
    }
    
    /// Check if Companion is registered
    pub fn contains(&self, name: &str) -> bool {
        self.Companions.contains_key(name)
    }
    
    /// Get Companion info
    pub fn get(&self, name: &str) -> Option<&CompanionInfo> {
        self.Companions.get(name)
    }
    
    /// List all Companions
    pub fn list(&self) -> Vec<&CompanionInfo> {
        self.Companions.values().collect()
    }
}
```

### Companion Connection Handler

Companions connect to Moss and subscribe to command bus:

```rust
impl Companion {
    pub async fn connect_to_moss(&self) -> Result<()> {
        // Subscribe to command bus
        let mut rx = self.command_rx.subscribe();
        
        loop {
            tokio::select! {
                // Handle incoming commands
                Ok(cmd) = rx.recv() => {
                    if cmd.Companion == self.name {
                        let response = self.handle_command(&cmd.raw_args);
                        let _ = cmd.response_tx.send(response);
                    }
                }
                
                // Handle SSE presence events (for Companion's main purpose)
                event = self.sse_stream.next() => {
                    if let Some(event) = event {
                        self.handle_presence_event(event).await;
                    }
                }
            }
        }
    }
}
```

---

## Companion Lifecycle

### Installation

```bash
# 1. Package installs binary + manifests
sudo dpkg -i garden-cricket.deb
# Installs:
#   /usr/local/bin/garden-cricket
#   /etc/zen-garden/Companions.d/cricket.json
#   /usr/share/zen-garden/companions/commands/cricket.json
#   /etc/systemd/system/garden-cricket.service

# 2. Moss detects new Companion on next API call
#    (or file watcher triggers reload)
```

### Enable

```bash
garden-rake hey tell cricket on
```

1. Rake calls `POST /api/v1/stone/companions/cricket/enable`
2. Moss updates `enabled: true` in manifest
3. Moss runs `systemctl enable garden-cricket`
4. Moss runs `systemctl start garden-cricket`
5. Returns success

### Disable

```bash
garden-rake hey tell cricket off
```

1. Rake calls `POST /api/v1/stone/companions/cricket/disable`
2. Moss runs `systemctl stop garden-cricket`
3. Moss runs `systemctl disable garden-cricket`
4. Moss updates `enabled: false` in manifest
5. Returns success (Companion stays registered)

### Uninstall

```bash
sudo apt remove garden-cricket
# OR
sudo dpkg -r garden-cricket
```

Package manager removes all files. Moss detects removal on next query.

---

## Future Extensions

1. **Auto-discovery** - Companions announce themselves to Moss on startup
2. **Health checks** - Moss pings Companions periodically
3. **Companion groups** - Enable/disable multiple Companions at once
4. **Remote Companions** - Companions on different machines (ESP32, etc.)
5. **Companion dependencies** - Firefly requires specific USB device

---

## Related Documents

- [Companion-COMMAND-PROTOCOL.md](Companion-COMMAND-PROTOCOL.md) - Command flow specification
- [HEY-TELL-SYNTAX.md](HEY-TELL-SYNTAX.md) - Rake syntax specification
- [CRICKET-SPEC.md](CRICKET-SPEC.md) - Cricket implementation

---

**Document Status:** Draft  
**Last Updated:** 2026-01-26
