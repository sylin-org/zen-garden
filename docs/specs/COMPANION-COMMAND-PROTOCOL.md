# Companion Command Protocol Specification

**Status:** Draft  
**Date:** 2026-01-26  
**Scope:** Synchronous command flow between Rake → Moss → Companions

---

## Overview

The Companion Command Protocol defines how Rake communicates with presence Companions (Cricket, Firefly, OLED, etc.) through Moss as a synchronous proxy. Commands are passed **raw** to Companions, which parse and validate internally.

**Key principle:** Moss is a thin proxy. Companions own their command structure.

---

## Request/Response Flow

```
┌─────────┐       ┌─────────┐       ┌─────────────┐
│  RAKE   │──────▶│  MOSS   │──────▶│  Companion    │
│         │       │ (proxy) │       │  (Cricket)  │
│         │◀──────│         │◀──────│             │
└─────────┘       └─────────┘       └─────────────┘
    HTTP              IPC              Internal
```

1. **Rake** sends HTTP POST to Moss with Companion name + raw args
2. **Moss** validates Companion exists, forwards via internal channel
3. **Companion** parses args, executes command, returns response
4. **Moss** forwards response back to Rake (5s timeout)
5. **Rake** formats and displays response

---

## API Endpoint

### `POST /api/v1/stone/presence/command`

Send a command to a presence Companion.

**Request:**
```json
{
  "Companion": "cricket",
  "raw_args": ["select", "mr-robot"]
}
```

**Response (Success):**
```json
{
  "status": "success",
  "output": "Active tune: mr-robot",
  "message": "Switched to tune: mr-robot",
  "suggestions": [
    "Adjust volume: garden-rake hey tell cricket volume <0-100>"
  ]
}
```

**Response (Error):**
```json
{
  "status": "error",
  "output": null,
  "message": "Tune not found: nonexistent",
  "suggestions": [
    "Check available tunes: garden-rake hey tell cricket list"
  ]
}
```

**Response (Timeout):**
```
HTTP 504 Gateway Timeout
{
  "error": "Companion 'cricket' did not respond within 5 seconds"
}
```

---

## Common Types

### CompanionCommandRequest

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionCommandRequest {
    /// Target Companion name (e.g., "cricket", "firefly")
    pub Companion: String,
    
    /// Raw command arguments (Companion parses these)
    pub raw_args: Vec<String>,
}
```

### CompanionCommandResponse

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionCommandResponse {
    /// Command result status
    pub status: ResponseStatus,
    
    /// Primary output text (optional, for data display)
    pub output: Option<String>,
    
    /// Human-readable result message
    pub message: String,
    
    /// Suggested next actions (hints for user)
    pub suggestions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResponseStatus {
    Success,
    Error,
    Warning,
}
```

**Location:** `garden_common/src/presence/types.rs`

---

## Moss Implementation

### Endpoint Handler

```rust
/// POST /api/v1/stone/presence/command
pub async fn send_Companion_command(
    State(state): State<AppState>,
    Json(cmd): Json<CompanionCommandRequest>,
) -> Result<Json<CompanionCommandResponse>, (StatusCode, Json<ErrorResponse>)> {
    // 1. Validate Companion is registered
    if !state.Companion_registry.contains(&cmd.Companion) {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::from_message(&format!(
                "Companion '{}' not registered", cmd.Companion
            )))
        ));
    }
    
    // 2. Create response channel
    let (tx, rx) = tokio::sync::oneshot::channel();
    
    // 3. Send command to Companion via internal bus
    let internal = InternalCompanionCommand {
        Companion: cmd.Companion.clone(),
        raw_args: cmd.raw_args,
        response_tx: tx,
    };
    
    state.Companion_command_bus.send(internal).await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::from_message(&format!(
                "Failed to route command: {}", e
            )))
        ))?;
    
    // 4. Wait for response (5s timeout)
    match tokio::time::timeout(Duration::from_secs(5), rx).await {
        Ok(Ok(response)) => Ok(Json(response)),
        Ok(Err(_)) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::from_message("Companion dropped response channel"))
        )),
        Err(_) => Err((
            StatusCode::GATEWAY_TIMEOUT,
            Json(ErrorResponse::from_message(&format!(
                "Companion '{}' did not respond within 5 seconds", 
                cmd.Companion
            )))
        )),
    }
}
```

### Internal Command Structure

```rust
pub struct InternalCompanionCommand {
    pub Companion: String,
    pub raw_args: Vec<String>,
    pub response_tx: oneshot::Sender<CompanionCommandResponse>,
}
```

---

## Companion Implementation

### Command Handler Interface

Each Companion implements its own command parsing:

```rust
/// Companion trait for command handling
pub trait CompanionCommandHandler {
    /// Process raw command args and return response
    fn handle_command(&self, raw_args: &[String]) -> CompanionCommandResponse;
}
```

### Example: Cricket Handler

```rust
impl CompanionCommandHandler for CricketCompanion {
    fn handle_command(&self, raw_args: &[String]) -> CompanionCommandResponse {
        if raw_args.is_empty() {
            return CompanionCommandResponse {
                status: ResponseStatus::Error,
                output: None,
                message: "No command provided".to_string(),
                suggestions: vec![
                    "See commands: garden-rake hey tell cricket?".to_string(),
                ],
            };
        }
        
        let command = &raw_args[0];
        let args = &raw_args[1..];
        
        match command.as_str() {
            "select" => self.cmd_select(args),
            "list" => self.cmd_list(),
            "volume" => self.cmd_volume(args),
            "pull" => self.cmd_pull(args),
            "remove" => self.cmd_remove(args),
            "status" => self.cmd_status(),
            _ => CompanionCommandResponse {
                status: ResponseStatus::Error,
                output: None,
                message: format!("Unknown command: {}", command),
                suggestions: vec![
                    "See commands: garden-rake hey tell cricket?".to_string(),
                ],
            },
        }
    }
}
```

---

## Rake Implementation

### Command Dispatch

```rust
pub async fn hey_tell_command(
    service: &str,
    args: Vec<String>,
    endpoint: &str,
) -> Result<()> {
    let url = format!("{}/api/v1/stone/presence/command", endpoint);
    
    let request = CompanionCommandRequest {
        Companion: service.to_string(),
        raw_args: args,
    };
    
    let response = reqwest::Client::new()
        .post(&url)
        .json(&request)
        .timeout(Duration::from_secs(10))
        .send()
        .await?;
    
    if response.status().is_success() {
        let result: CompanionCommandResponse = response.json().await?;
        format_Companion_response(&result);
    } else {
        let error: ErrorResponse = response.json().await?;
        eprintln!("✗ {}", error.error);
    }
    
    Ok(())
}
```

### Response Formatter

```rust
pub fn format_Companion_response(response: &CompanionCommandResponse) {
    // Status icon + message
    match response.status {
        ResponseStatus::Success => {
            println!("✓ {}", response.message);
        }
        ResponseStatus::Error => {
            eprintln!("✗ {}", response.message);
        }
        ResponseStatus::Warning => {
            println!("⚠ {}", response.message);
        }
    }
    
    // Output block (if present)
    if let Some(output) = &response.output {
        println!();
        println!("{}", output);
    }
    
    // Suggestions
    if !response.suggestions.is_empty() {
        println!();
        for suggestion in &response.suggestions {
            println!("  → {}", suggestion);
        }
    }
}
```

---

## Timeout Handling

| Scenario | Timeout | Behavior |
|----------|---------|----------|
| Moss → Companion | 5s | Return 504 Gateway Timeout |
| Rake → Moss | 10s | Return network error to user |
| Companion crashed | - | oneshot channel drops, return 500 |

**Philosophy:** Fast feedback. If something is broken, user knows quickly.

---

## Error Responses

### Companion Not Found

```
HTTP 404 Not Found
{
  "error": "Companion 'nonexistent' not registered"
}
```

### Companion Timeout

```
HTTP 504 Gateway Timeout
{
  "error": "Companion 'cricket' did not respond within 5 seconds"
}
```

### Internal Error

```
HTTP 500 Internal Server Error
{
  "error": "Failed to route command: channel closed"
}
```

---

## Security Considerations

1. **No authentication required** - Local/LAN only, same as Presence API
2. **Companion isolation** - Commands go only to specified Companion
3. **No shell execution** - Companions validate all input, no command injection
4. **Timeout protection** - Hung Companions don't block Moss

---

## Future Extensions

1. **Batch commands** - Send multiple commands in one request
2. **Async commands** - For long-running operations (pull large tune)
3. **Streaming responses** - For progress updates during downloads
4. **Command history** - Track recent commands per Companion

---

## Related Documents

- [Companion-SERVICE-REGISTRY.md](Companion-SERVICE-REGISTRY.md) - Service registration and lifecycle
- [HEY-TELL-SYNTAX.md](HEY-TELL-SYNTAX.md) - Rake syntax specification
- [CRICKET-SPEC.md](CRICKET-SPEC.md) - Cricket-specific implementation

---

**Document Status:** Draft  
**Last Updated:** 2026-01-26
