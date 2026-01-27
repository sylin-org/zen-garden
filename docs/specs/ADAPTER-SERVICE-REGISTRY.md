# Adapter Service Registry Specification

**Status:** Draft  
**Date:** 2026-01-26  
**Scope:** Universal service management and communication layer for Zen Garden adapters

---

## Overview

The Adapter Service Registry provides a universal mechanism for:
1. **Registration** - Adapters declare their capabilities
2. **Discovery** - Rake queries available adapters
3. **Lifecycle** - Enable/disable adapters without uninstalling
4. **Introspection** - Query adapter commands dynamically

**Key principle:** Self-documenting services. Each adapter owns its command manifest.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  STONE                                                      │
│                                                             │
│  ┌───────────────────────────────────────────────────────┐ │
│  │  Adapter Registry (Moss-managed)                      │ │
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
/etc/zen-garden/adapters.d/
├── cricket.json           # Cricket's service manifest
├── firefly.json           # Firefly's service manifest (if installed)
└── oled.json              # OLED's service manifest (if installed)
```

### Command Manifests (Read-Only)

```
/usr/share/zen-garden/adapters/
└── commands/
    ├── cricket.json       # Cricket's command manifest
    ├── firefly.json       # Firefly's command manifest
    └── oled.json          # OLED's command manifest
```

---

## Service Manifest Schema

**File:** `/etc/zen-garden/adapters.d/{adapter}.json`

```json
{
  "adapter": "cricket",
  "type": "presence",
  "version": "0.1.0",
  "description": "Audio presence adapter - ambient soundscapes",
  "binary": "/usr/local/bin/garden-cricket",
  "systemd_unit": "garden-cricket.service",
  "command_manifest": "/usr/share/zen-garden/adapters/commands/cricket.json",
  "enabled": true
}
```

### Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `adapter` | string | ✓ | Unique adapter identifier |
| `type` | string | ✓ | Adapter type (`presence`, `display`, `hardware`) |
| `version` | string | ✓ | Semantic version (e.g., "0.1.0") |
| `description` | string | ✓ | Human-readable description |
| `binary` | string | ✓ | Path to executable |
| `systemd_unit` | string | ✓ | Systemd service name |
| `command_manifest` | string | ✓ | Path to command manifest JSON |
| `enabled` | boolean | ✓ | Whether adapter is enabled |

---

## Command Manifest Schema

**File:** `/usr/share/zen-garden/adapters/commands/{adapter}.json`

```json
{
  "adapter": "cricket",
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

**Note:** Command manifest is for **help generation only**. Adapters parse commands internally and may accept additional undocumented commands.

---

## Rake "hey tell" Syntax

### Full Syntax Reference

```bash
# Help for hey command
garden-rake hey?
# Output: Lists subcommands (currently: tell)

# Help for tell command
garden-rake hey tell?
# Output: "Allows communication with Zen Garden adapters"

# List registered adapters
garden-rake hey tell
# Output: List of adapters with status

# Adapter lifecycle
garden-rake hey tell {adapter} on      # Enable + start
garden-rake hey tell {adapter} off     # Disable + stop

# Adapter help (query command manifest)
garden-rake hey tell {adapter}?
# Output: Formatted command list from manifest

# Adapter commands (passed raw to adapter)
garden-rake hey tell {adapter} {command} [args...]
# Example: garden-rake hey tell cricket select mr-robot
```

### Special Commands (Handled by Rake)

| Command | Handler | Description |
|---------|---------|-------------|
| `on` | Rake/systemd | Enable and start adapter |
| `off` | Rake/systemd | Disable and stop adapter |
| `?` (suffix) | Rake | Show command help |

All other commands are passed **raw** to the adapter.

---

## API Endpoints

### List Adapters

**`GET /api/v1/stone/adapters`**

Returns all registered adapters with status.

```json
{
  "adapters": [
    {
      "adapter": "cricket",
      "type": "presence",
      "version": "0.1.0",
      "description": "Audio presence adapter",
      "enabled": true,
      "running": true
    },
    {
      "adapter": "firefly",
      "type": "presence",
      "version": "0.1.0",
      "description": "LED matrix adapter",
      "enabled": false,
      "running": false
    }
  ]
}
```

### Get Adapter Info

**`GET /api/v1/stone/adapters/{adapter}`**

Returns single adapter details including command manifest.

```json
{
  "adapter": "cricket",
  "type": "presence",
  "version": "0.1.0",
  "description": "Audio presence adapter",
  "enabled": true,
  "running": true,
  "commands": [
    {"name": "select", "description": "Switch tune", ...},
    {"name": "list", "description": "List tunes", ...}
  ]
}
```

### Enable/Disable Adapter

**`POST /api/v1/stone/adapters/{adapter}/enable`**
**`POST /api/v1/stone/adapters/{adapter}/disable`**

Manages adapter lifecycle via systemd.

```json
// Response
{
  "status": "success",
  "message": "Adapter 'cricket' enabled and started"
}
```

### Send Adapter Command

**`POST /api/v1/stone/presence/command`**

See [ADAPTER-COMMAND-PROTOCOL.md](ADAPTER-COMMAND-PROTOCOL.md) for details.

---

## Rake Implementation

### `garden-rake hey tell` (List Adapters)

```rust
pub async fn list_adapters(endpoint: &str) -> Result<()> {
    let url = format!("{}/api/v1/stone/adapters", endpoint);
    let response: AdapterListResponse = reqwest::get(&url).await?.json().await?;
    
    if response.adapters.is_empty() {
        println!("No adapters registered.");
        println!();
        println!("  → Install an adapter: sudo apt install garden-cricket");
        return Ok(());
    }
    
    println!("Registered Adapters:");
    println!();
    
    for adapter in &response.adapters {
        let status_icon = if adapter.running { "●" } else { "○" };
        let enabled_text = if adapter.enabled { "" } else { " (disabled)" };
        
        println!("  {} {} (v{}){}", 
                 status_icon, 
                 adapter.adapter, 
                 adapter.version,
                 enabled_text);
        println!("    {}", adapter.description);
    }
    
    Ok(())
}
```

**Example output:**
```
Registered Adapters:

  ● cricket (v0.1.0)
    Audio presence adapter - ambient soundscapes
  ○ firefly (v0.1.0) (disabled)
    LED matrix adapter - visual presence
```

### `garden-rake hey tell {adapter}?` (Show Commands)

```rust
pub async fn show_adapter_commands(endpoint: &str, adapter: &str) -> Result<()> {
    let url = format!("{}/api/v1/stone/adapters/{}", endpoint, adapter);
    let response: AdapterInfoResponse = reqwest::get(&url).await?.json().await?;
    
    println!("{} Commands:", adapter);
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

### `garden-rake hey tell {adapter} on/off`

```rust
pub async fn adapter_lifecycle(endpoint: &str, adapter: &str, action: &str) -> Result<()> {
    let url = format!("{}/api/v1/stone/adapters/{}/{}", 
                      endpoint, adapter, action);
    
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

### Adapter Registry

```rust
pub struct AdapterRegistry {
    adapters: HashMap<String, AdapterInfo>,
    command_bus: broadcast::Sender<InternalAdapterCommand>,
}

impl AdapterRegistry {
    /// Load adapters from /etc/zen-garden/adapters.d/
    pub async fn load_from_disk() -> Result<Self> {
        let mut adapters = HashMap::new();
        
        let dir = Path::new("/etc/zen-garden/adapters.d");
        if dir.exists() {
            for entry in fs::read_dir(dir)? {
                let path = entry?.path();
                if path.extension() == Some(OsStr::new("json")) {
                    let content = fs::read_to_string(&path)?;
                    let manifest: AdapterManifest = serde_json::from_str(&content)?;
                    adapters.insert(manifest.adapter.clone(), AdapterInfo::from(manifest));
                }
            }
        }
        
        let (tx, _) = broadcast::channel(100);
        
        Ok(Self {
            adapters,
            command_bus: tx,
        })
    }
    
    /// Check if adapter is registered
    pub fn contains(&self, name: &str) -> bool {
        self.adapters.contains_key(name)
    }
    
    /// Get adapter info
    pub fn get(&self, name: &str) -> Option<&AdapterInfo> {
        self.adapters.get(name)
    }
    
    /// List all adapters
    pub fn list(&self) -> Vec<&AdapterInfo> {
        self.adapters.values().collect()
    }
}
```

### Adapter Connection Handler

Adapters connect to Moss and subscribe to command bus:

```rust
impl Adapter {
    pub async fn connect_to_moss(&self) -> Result<()> {
        // Subscribe to command bus
        let mut rx = self.command_rx.subscribe();
        
        loop {
            tokio::select! {
                // Handle incoming commands
                Ok(cmd) = rx.recv() => {
                    if cmd.adapter == self.name {
                        let response = self.handle_command(&cmd.raw_args);
                        let _ = cmd.response_tx.send(response);
                    }
                }
                
                // Handle SSE presence events (for adapter's main purpose)
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

## Adapter Lifecycle

### Installation

```bash
# 1. Package installs binary + manifests
sudo dpkg -i garden-cricket.deb
# Installs:
#   /usr/local/bin/garden-cricket
#   /etc/zen-garden/adapters.d/cricket.json
#   /usr/share/zen-garden/adapters/commands/cricket.json
#   /etc/systemd/system/garden-cricket.service

# 2. Moss detects new adapter on next API call
#    (or file watcher triggers reload)
```

### Enable

```bash
garden-rake hey tell cricket on
```

1. Rake calls `POST /api/v1/stone/adapters/cricket/enable`
2. Moss updates `enabled: true` in manifest
3. Moss runs `systemctl enable garden-cricket`
4. Moss runs `systemctl start garden-cricket`
5. Returns success

### Disable

```bash
garden-rake hey tell cricket off
```

1. Rake calls `POST /api/v1/stone/adapters/cricket/disable`
2. Moss runs `systemctl stop garden-cricket`
3. Moss runs `systemctl disable garden-cricket`
4. Moss updates `enabled: false` in manifest
5. Returns success (adapter stays registered)

### Uninstall

```bash
sudo apt remove garden-cricket
# OR
sudo dpkg -r garden-cricket
```

Package manager removes all files. Moss detects removal on next query.

---

## Future Extensions

1. **Auto-discovery** - Adapters announce themselves to Moss on startup
2. **Health checks** - Moss pings adapters periodically
3. **Adapter groups** - Enable/disable multiple adapters at once
4. **Remote adapters** - Adapters on different machines (ESP32, etc.)
5. **Adapter dependencies** - Firefly requires specific USB device

---

## Related Documents

- [ADAPTER-COMMAND-PROTOCOL.md](ADAPTER-COMMAND-PROTOCOL.md) - Command flow specification
- [HEY-TELL-SYNTAX.md](HEY-TELL-SYNTAX.md) - Rake syntax specification
- [CRICKET-SPEC.md](CRICKET-SPEC.md) - Cricket implementation

---

**Document Status:** Draft  
**Last Updated:** 2026-01-26
