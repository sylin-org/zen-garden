# Companion Development Guide

**Purpose:** Build custom Companions that extend Moss capabilities  
**Audience:** Developers

---

## Overview

Companions are standalone executables that:
1. Receive a port assignment from Moss via CLI arguments
2. Expose an HTTP command server on that port
3. Subscribe to Stone presence events via SSE (optional)
4. Execute commands and return results
5. Support graceful shutdown via `/shutdown` endpoint

**Language:** Rust (recommended, uses `garden-companion-sdk`) or any language supporting HTTP servers

---

## Quick Start with Rust SDK

The `garden-companion-sdk` crate provides all common Companion infrastructure:

```rust
use garden_companion_sdk::prelude::*;
use std::sync::Arc;

// 1. Define your manifest
fn build_manifest() -> CommandManifest {
    CommandManifest::new("my-Companion", "My Companion", "0.1.0", "Does cool things")
        .command(CommandDef::new("hello", "Say hello"))
        .command(
            CommandDef::new("greet", "Greet someone")
                .arg(CommandArg::required_string("name", "Name to greet"))
        )
}

// 2. Implement command handler
struct MyHandler;

#[async_trait]
impl CommandHandler for MyHandler {
    async fn handle(&self, args: &[String]) -> CommandResult {
        let cmd = args.first().map(|s| s.as_str()).unwrap_or("");
        let cmd_args = if args.len() > 1 { &args[1..] } else { &[] };
        
        match cmd {
            "hello" => CommandResult::success("Hello, World!"),
            "greet" => {
                let name = cmd_args.first().map(|s| s.as_str()).unwrap_or("stranger");
                CommandResult::success(format!("Hello, {}!", name))
            }
            "" => CommandResult::error("No command provided")
                .with_suggestions(vec!["hello".into(), "greet <name>".into()]),
            _ => CommandResult::error(format!("Unknown command: {}", cmd)),
        }
    }
    
    async fn on_shutdown(&self) {
        // Optional: cleanup resources
        tracing::info!("Cleaning up...");
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Handle --dump-commands before anything else
    check_dump_commands(&build_manifest());
    
    // Initialize logging
    garden_companion_sdk::runtime::init_tracing();
    
    // Parse CLI
    let config = CompanionConfig::from_cli();
    
    // Run Companion (handles HTTP server, shutdown, etc.)
    CompanionRuntime::new(config, "my-Companion")
        .command_handler(MyHandler)
        .run()
        .await
}
```

**Cargo.toml:**
```toml
[dependencies]
garden-companion-sdk = { path = "../companion-sdk" }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

---

## SDK Features

The `garden-companion-sdk` provides:

| Module | Purpose |
|--------|---------|
| `cli` | Standard CLI args (`--stone`, `--port`, `--dump-commands`) |
| `handler` | `CommandHandler` trait and `CommandResult` type |
| `server` | HTTP server with `/command`, `/shutdown`, `/health` |
| `sse` | SSE client for presence events |
| `runtime` | Main loop, shutdown coordination, signal handling |

### Command Handler Trait

```rust
#[async_trait]
pub trait CommandHandler: Send + Sync + 'static {
    /// Handle a command from Moss
    async fn handle(&self, args: &[String]) -> CommandResult;
    
    /// Called before shutdown (optional override)
    async fn on_shutdown(&self) {}
}
```

### SSE Event Handler

```rust
#[async_trait]
pub trait EventHandler: Send + Sync + 'static {
    async fn on_event(&self, event: SseEvent);
}

// Usage:
struct MyEventHandler { mixer: Arc<Mixer> }

#[async_trait]
impl EventHandler for MyEventHandler {
    async fn on_event(&self, event: SseEvent) {
        if event.event_type == "stone-online" {
            self.mixer.play("welcome.wav").await;
        }
    }
}

CompanionRuntime::new(config, "my-Companion")
    .command_handler(cmd_handler)
    .event_handler(event_handler)  // Optional
    .run()
    .await
```

---

## Protocol Requirements

### 1. CLI Arguments

**Required flags:**
- `--stone <url>` - Moss HTTP endpoint (e.g., `http://localhost:7185`)
- `--port <port>` - Assigned port from Moss (e.g., `7187`)
- `--dump-commands` - Output JSON manifest to stdout and exit

---

### 2. HTTP Endpoints (Required)

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/command` | Execute Companion commands |
| `POST` | `/shutdown` | Graceful shutdown (called by Moss before upgrades) |
| `GET` | `/health` | Health check |

**POST /command Request:**
```json
{ "args": ["command", "arg1", "arg2"] }
```

**POST /command Response:**
```json
{ "success": true, "output": "Command executed" }
{ "success": false, "output": "Error message", "suggestions": ["try this"] }
```

**POST /shutdown Response:**
```json
{ "status": "shutting_down", "Companion": "my-Companion" }
```

The `/shutdown` endpoint is critical for graceful upgrades. When Moss receives a deployment package, it calls `/shutdown` on all running Companions before installing new binaries.

---

### 3. Timeout

Commands must respond within **5 seconds**. Moss will timeout and return error if Companion takes longer.

---

## Quick Start (Non-Rust)

### Python Minimal Companion

```python
#!/usr/bin/env python3
import json
import sys
import signal
import threading
from http.server import HTTPServer, BaseHTTPRequestHandler

shutdown_flag = threading.Event()

class CompanionHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == '/health':
            self.send_json(200, {'status': 'healthy', 'Companion': 'my-Companion'})
        else:
            self.send_response(404)
            self.end_headers()
    
    def do_POST(self):
        if self.path == '/command':
            body = self.read_body()
            args = body.get('args', [])
            result = self.execute_command(args)
            self.send_json(200, result)
        elif self.path == '/shutdown':
            self.send_json(200, {
                'status': 'shutting_down',
                'Companion': 'my-Companion'
            })
            # Signal main thread to shutdown
            shutdown_flag.set()
        else:
            self.send_response(404)
            self.end_headers()
    
    def read_body(self):
        content_length = int(self.headers.get('Content-Length', 0))
        return json.loads(self.rfile.read(content_length))
    
    def send_json(self, status, data):
        self.send_response(status)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(json.dumps(data).encode())
    
    def execute_command(self, args):
        if not args:
            return {'success': False, 'output': 'No command specified'}
        
        command = args[0]
        if command == 'hello':
            return {'success': True, 'output': 'Hello from my Companion!'}
        else:
            return {'success': False, 'output': f'Unknown command: {command}'}

def main():
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument('--stone', help='Moss endpoint')
    parser.add_argument('--port', type=int, help='Assigned port')
    parser.add_argument('--dump-commands', action='store_true')
    args = parser.parse_args()
    
    if args.dump_commands:
        manifest = {
            'id': 'my-Companion',
            'name': 'My Companion',
            'version': '0.1.0',
            'description': 'Example Companion',
            'commands': [{'name': 'hello', 'description': 'Say hello'}]
        }
        print(json.dumps(manifest))
        sys.exit(0)
    
    if not args.stone or not args.port:
        print('--stone and --port required', file=sys.stderr)
        sys.exit(1)
    
    server = HTTPServer(('0.0.0.0', args.port), CompanionHandler)
    print(f'Companion running on port {args.port}', file=sys.stderr)
    
    # Run server in thread, check shutdown flag
    server_thread = threading.Thread(target=server.serve_forever)
    server_thread.start()
    
    shutdown_flag.wait()  # Block until /shutdown called
    server.shutdown()
    print('Companion stopped', file=sys.stderr)

if __name__ == '__main__':
    main()
```

**Usage:**
```bash
# Test manifest output
python my-Companion.py --dump-commands --port 7187

# Start Companion
python my-Companion.py --stone http://localhost:7185 --port 7187

# Send command
curl -X POST http://localhost:7187/command \
  -H 'Content-Type: application/json' \
  -d '{"args": ["hello"]}'
```

---

## Protocol Requirements

### 1. CLI Arguments

**Required flags:**
- `--stone <url>` - Moss HTTP endpoint (e.g., `http://localhost:7185`)
- `--port <port>` - Assigned port from Moss (e.g., `7187`)
- `--dump-commands` - Output JSON manifest to stdout and exit

**Example:**
```bash
garden-my-Companion --stone http://localhost:7185 --port 7187
garden-my-Companion --dump-commands --port 7187
```

---

### 2. Command Manifest

Output JSON manifest when invoked with `--dump-commands`:

```json
{
  "name": "My Companion",
  "version": "0.1.0",
  "description": "Short description of Companion purpose",
  "commands": [
    {
      "name": "command-name",
      "description": "What this command does",
      "parameters": [
        {
          "name": "param1",
          "type": "string",
          "required": true,
          "description": "Parameter description"
        }
      ],
      "examples": [
        {
          "command": "command-name arg1 arg2",
          "description": "Example usage",
          "expected_output": "What the command returns"
        }
      ]
    }
  ]
}
```

**Parameter types:** `string`, `number`, `boolean`, `choice` (with `choices` array)

---

### 3. HTTP Command Server

**Bind:** `127.0.0.1:{port}` (localhost only, never `0.0.0.0`)

**Endpoint:** `POST /command`

**Request:**
```json
{
  "args": ["command", "arg1", "arg2"]
}
```

**Response (success):**
```json
{
  "success": true,
  "output": "Command executed successfully"
}
```

**Response (failure):**
```json
{
  "success": false,
  "output": "Error: invalid parameter"
}
```

**Timeout:** Commands must respond within 5 seconds. Moss will timeout and return error if Companion takes longer.

---

### 4. SSE Event Subscription (Optional)

Subscribe to Stone presence events:

**Endpoint:** `GET {stone_endpoint}/api/v1/stone/presence/stream`

**Example (Python with sseclient):**
```python
import sseclient
import requests

def subscribe_to_events(stone_endpoint):
    url = f'{stone_endpoint}/api/v1/stone/presence/stream'
    response = requests.get(url, stream=True, headers={'Accept': 'text/event-stream'})
    client = sseclient.SSEClient(response)
    
    for event in client.events():
        if event.event == 'stone-online':
            handle_stone_online(json.loads(event.data))
        elif event.event == 'service-started':
            handle_service_started(json.loads(event.data))
```

**Event types:**
- `stone-online`, `stone-offline`
- `service-started`, `service-stopped`, `service-restarted`
- `container-failed`
- `update-available`, `update-applied`
- `firmware-update-available`, `firmware-updated`
- `health-degraded`, `health-recovered`
- `disk-warning`, `memory-warning`, `cpu-spike`

---

## Implementation Examples

### Rust (with Axum)

```rust
use axum::{routing::post, Json, Router};
use clap::Parser;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    stone: Option<String>,
    
    #[arg(long)]
    port: u16,
    
    #[arg(long)]
    dump_commands: bool,
}

#[derive(Deserialize)]
struct CommandRequest {
    args: Vec<String>,
}

#[derive(Serialize)]
struct CommandResponse {
    success: bool,
    output: String,
}

async fn handle_command(Json(req): Json<CommandRequest>) -> Json<CommandResponse> {
    let command = req.args.first().map(|s| s.as_str()).unwrap_or("");
    
    match command {
        "hello" => Json(CommandResponse {
            success: true,
            output: "Hello from Rust Companion!".to_string(),
        }),
        _ => Json(CommandResponse {
            success: false,
            output: format!("Unknown command: {}", command),
        }),
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    
    if args.dump_commands {
        // Output manifest
        let manifest = serde_json::json!({
            "name": "My Rust Companion",
            "version": "0.1.0",
            "description": "Example Rust Companion",
            "commands": [
                {
                    "name": "hello",
                    "description": "Say hello",
                    "parameters": [],
                    "examples": [{"command": "hello", "description": "Greet", "expected_output": "Hello!"}]
                }
            ]
        });
        println!("{}", serde_json::to_string_pretty(&manifest).unwrap());
        return;
    }
    
    // Start HTTP server
    let app = Router::new().route("/command", post(handle_command));
    let addr = format!("127.0.0.1:{}", args.port);
    let listener = TcpListener::bind(&addr).await.unwrap();
    
    eprintln!("Companion listening on {}", addr);
    axum::serve(listener, app).await.unwrap();
}
```

---

### Node.js (with Express)

```javascript
#!/usr/bin/env node
const express = require('express');
const { program } = require('commander');

program
  .option('--stone <url>', 'Moss endpoint')
  .option('--port <port>', 'Assigned port', parseInt)
  .option('--dump-commands', 'Output manifest and exit')
  .parse();

const opts = program.opts();

if (opts.dumpCommands) {
  const manifest = {
    name: 'My Node Companion',
    version: '0.1.0',
    description: 'Example Node.js Companion',
    commands: [
      {
        name: 'hello',
        description: 'Say hello',
        parameters: [],
        examples: [{
          command: 'hello',
          description: 'Basic greeting',
          expected_output: 'Hello from Node!'
        }]
      }
    ]
  };
  console.log(JSON.stringify(manifest, null, 2));
  process.exit(0);
}

const app = express();
app.use(express.json());

app.post('/command', (req, res) => {
  const { args = [] } = req.body;
  const command = args[0];
  
  if (command === 'hello') {
    res.json({ success: true, output: 'Hello from Node!' });
  } else {
    res.json({ success: false, output: `Unknown command: ${command}` });
  }
});

app.listen(opts.port, '127.0.0.1', () => {
  console.error(`Companion listening on port ${opts.port}`);
});
```

---

## Testing Your Companion

### 1. Test Manifest Output

```bash
./my-Companion --dump-commands --port 7187 | jq
```

Verify JSON is valid and contains all required fields.

---

### 2. Start Companion Manually

```bash
./my-Companion --stone http://localhost:7185 --port 7187
```

---

### 3. Send Test Commands

```bash
# Basic hello command
curl -X POST http://localhost:7187/command \
  -H 'Content-Type: application/json' \
  -d '{"args": ["hello"]}'

# Command with parameters
curl -X POST http://localhost:7187/command \
  -H 'Content-Type: application/json' \
  -d '{"args": ["volume", "75"]}'
```

---

### 4. Register with Moss

```bash
# Copy to Companions directory
sudo mkdir -p /usr/local/bin/companions/my-Companion
sudo cp my-Companion /usr/local/bin/companions/my-Companion/
sudo chmod +x /usr/local/bin/companions/my-Companion/my-Companion

# Restart Moss (or trigger refresh)
sudo systemctl restart garden-moss

# Verify registration
garden-rake hey list
```

---

### 5. Test via Rake

```bash
garden-rake hey my-Companion          # Show help
garden-rake hey tell my-Companion hello   # Send command
```

---

## Best Practices

### 1. Error Handling

Always return structured responses with `success` boolean:

```python
def execute_command(args):
    try:
        # Execute command
        result = do_something(args)
        return {'success': True, 'output': result}
    except Exception as e:
        return {'success': False, 'output': f'Error: {str(e)}'}
```

---

### 2. Timeout Awareness

Commands must complete within 5 seconds. For long operations:

```python
# Bad: synchronous long operation
def slow_command():
    time.sleep(10)  # Will timeout!
    return result

# Good: start background task, return immediately
def fast_command():
    threading.Thread(target=background_work).start()
    return {'success': True, 'output': 'Task started'}
```

---

### 3. Logging

Log to stderr, not stdout (stdout reserved for manifest):

```python
import sys

def log(message):
    print(f'[my-Companion] {message}', file=sys.stderr)

log('Starting Companion...')
```

---

### 4. Graceful Shutdown

Handle SIGTERM for clean shutdown:

```python
import signal
import sys

def shutdown_handler(signum, frame):
    log('Shutting down gracefully...')
    # Clean up resources
    sys.exit(0)

signal.signal(signal.SIGTERM, shutdown_handler)
```

---

### 5. Health Checks (Optional)

Provide health endpoint for monitoring:

```python
def do_GET(self):
    if self.path == '/health':
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(b'{"status": "healthy"}')
```

---

## Distribution

### Option 1: Manual Installation

Provide compiled binary/script for users to copy to Companions directory.

---

### Option 2: Package Manager (Future)

Planned: Companion packages installable via Rake:
```bash
garden-rake Companion install my-Companion
```

---

## Reference

- [companion-command-protocol.md](../specs/companion-command-protocol.md) - Full protocol spec
- [companion-service-registry.md](../specs/companion-service-registry.md) - Registration details
- [hey-tell-syntax.md](../specs/hey-tell-syntax.md) - Command grammar
- [Cricket source](../../src/cricket/) - Reference implementation in Rust
- [ports.md](../reference/ports.md) - Port allocation (7187-7199)

---

## Examples

### Complete Examples

1. **Cricket** (`src/cricket/`) - Full-featured audio Companion with SSE, mixer, tune system
2. **Minimal Python** (above) - Bare minimum command server
3. **Rust with Axum** (above) - Async Rust implementation
4. **Node.js with Express** (above) - JavaScript implementation

---

## Getting Help

- Open issue on GitHub with `[Companion]` prefix
- Check existing Companions for patterns
- Review Cricket source for complex examples
