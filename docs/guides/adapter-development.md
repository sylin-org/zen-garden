# Adapter Development Guide

**Purpose:** Build custom adapters that extend Moss capabilities  
**Audience:** Developers

---

## Overview

Adapters are standalone executables that:
1. Receive a port assignment from Moss via CLI arguments
2. Expose an HTTP command server on that port
3. Subscribe to Stone presence events via SSE (optional)
4. Execute commands and return results

**Language:** Any language supporting HTTP servers (Rust, Python, Go, Node.js, etc.)

---

## Quick Start

### 1. Minimal Adapter (Python)

```python
#!/usr/bin/env python3
import json
import sys
from http.server import HTTPServer, BaseHTTPRequestHandler
from urllib.parse import urlparse

class AdapterHandler(BaseHTTPRequestHandler):
    def do_POST(self):
        if self.path == '/command':
            content_length = int(self.headers['Content-Length'])
            body = self.rfile.read(content_length)
            request = json.loads(body)
            
            # Execute command
            args = request.get('args', [])
            result = self.execute_command(args)
            
            # Return result
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.end_headers()
            self.wfile.write(json.dumps(result).encode())
        else:
            self.send_response(404)
            self.end_headers()
    
    def execute_command(self, args):
        if not args:
            return {'success': False, 'output': 'No command specified'}
        
        command = args[0]
        if command == 'hello':
            return {'success': True, 'output': 'Hello from my adapter!'}
        else:
            return {'success': False, 'output': f'Unknown command: {command}'}

def main():
    # Parse CLI arguments
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument('--stone', required=True, help='Moss endpoint')
    parser.add_argument('--port', type=int, required=True, help='Assigned port')
    parser.add_argument('--dump-commands', action='store_true', 
                        help='Output manifest and exit')
    args = parser.parse_args()
    
    # Handle --dump-commands
    if args.dump_commands:
        manifest = {
            'name': 'My Adapter',
            'version': '0.1.0',
            'description': 'Example adapter',
            'commands': [
                {
                    'name': 'hello',
                    'description': 'Say hello',
                    'parameters': [],
                    'examples': [
                        {
                            'command': 'hello',
                            'description': 'Basic greeting',
                            'expected_output': 'Hello from my adapter!'
                        }
                    ]
                }
            ]
        }
        print(json.dumps(manifest))
        sys.exit(0)
    
    # Start HTTP server
    server = HTTPServer(('127.0.0.1', args.port), AdapterHandler)
    print(f'Adapter running on port {args.port}', file=sys.stderr)
    server.serve_forever()

if __name__ == '__main__':
    main()
```

**Usage:**
```bash
# Test manifest output
python my-adapter.py --dump-commands --port 7187

# Start adapter
python my-adapter.py --stone http://localhost:7185 --port 7187

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
garden-my-adapter --stone http://localhost:7185 --port 7187
garden-my-adapter --dump-commands --port 7187
```

---

### 2. Command Manifest

Output JSON manifest when invoked with `--dump-commands`:

```json
{
  "name": "My Adapter",
  "version": "0.1.0",
  "description": "Short description of adapter purpose",
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

**Timeout:** Commands must respond within 5 seconds. Moss will timeout and return error if adapter takes longer.

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
            output: "Hello from Rust adapter!".to_string(),
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
            "name": "My Rust Adapter",
            "version": "0.1.0",
            "description": "Example Rust adapter",
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
    
    eprintln!("Adapter listening on {}", addr);
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
    name: 'My Node Adapter',
    version: '0.1.0',
    description: 'Example Node.js adapter',
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
  console.error(`Adapter listening on port ${opts.port}`);
});
```

---

## Testing Your Adapter

### 1. Test Manifest Output

```bash
./my-adapter --dump-commands --port 7187 | jq
```

Verify JSON is valid and contains all required fields.

---

### 2. Start Adapter Manually

```bash
./my-adapter --stone http://localhost:7185 --port 7187
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
# Copy to adapters directory
sudo cp my-adapter /var/lib/zen-garden/adapters/
sudo chmod +x /var/lib/zen-garden/adapters/my-adapter

# Restart Moss (or trigger refresh)
sudo systemctl restart garden-moss

# Verify registration
garden-rake hey list
```

---

### 5. Test via Rake

```bash
garden-rake hey my-adapter          # Show help
garden-rake hey tell my-adapter hello   # Send command
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
    print(f'[my-adapter] {message}', file=sys.stderr)

log('Starting adapter...')
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

Provide compiled binary/script for users to copy to adapters directory.

---

### Option 2: Package Manager (Future)

Planned: Adapter packages installable via Rake:
```bash
garden-rake adapter install my-adapter
```

---

## Reference

- [ADAPTER-COMMAND-PROTOCOL.md](../specs/ADAPTER-COMMAND-PROTOCOL.md) - Full protocol spec
- [ADAPTER-SERVICE-REGISTRY.md](../specs/ADAPTER-SERVICE-REGISTRY.md) - Registration details
- [HEY-TELL-SYNTAX.md](../specs/HEY-TELL-SYNTAX.md) - Command grammar
- [Cricket source](../../src/cricket/) - Reference implementation in Rust
- [ports.md](../reference/ports.md) - Port allocation (7187-7199)

---

## Examples

### Complete Examples

1. **Cricket** (`src/cricket/`) - Full-featured audio adapter with SSE, mixer, tune system
2. **Minimal Python** (above) - Bare minimum command server
3. **Rust with Axum** (above) - Async Rust implementation
4. **Node.js with Express** (above) - JavaScript implementation

---

## Getting Help

- Open issue on GitHub with `[adapter]` prefix
- Check existing adapters for patterns
- Review Cricket source for complex examples
