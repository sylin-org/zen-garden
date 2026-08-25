# COMM-0002: P2P Transport Pipeline Specification

**Status**: Active  
**Date**: 2026-01-25  
**Related**: COMM-0001 (P2P Transport Singleton)  
**RFC Keywords**: MUST, SHOULD, MAY per [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt)

## Abstract

This document specifies the P2P transport pipeline architecture for Zen Garden. The pipeline provides a centralized, type-safe UDP communication layer that separates transport concerns from business logic. All UDP communication MUST flow through this pipeline using the UdpAnnouncement envelope format.

## Goals

1. **Single Socket**: Prevent port conflicts by using one UDP socket per process
2. **Separation of Concerns**: Transport layer has no knowledge of business logic
3. **Type Safety**: Structured announcement types prevent protocol errors
4. **Testability**: Domain handlers can be tested without network access
5. **Extensibility**: New announcement types can be added without modifying transport

## Architecture Overview

```
┌──────────────────────────────────────────────────────────────────┐
│                         Application Layer                         │
│  (Business Logic: discovery_handler, election_service, etc.)     │
│                                                                    │
│  Consumes: UdpEvent via subscribe_to_events()                    │
│  Produces: Calls send_announcement(type, payload)                │
└────────────────────────┬────────────────────────────────────────┘
                         │ tokio::sync::broadcast<UdpEvent>
┌────────────────────────▼────────────────────────────────────────┐
│                      Transport Layer (p2p.rs)                    │
│                                                                   │
│  Responsibilities:                                               │
│  - Bind UDP socket (0.0.0.0:7184)                               │
│  - Validate UdpAnnouncement envelope structure                  │
│  - Broadcast validated events to all subscribers                │
│  - Send announcements via socket.send_to()                      │
│                                                                   │
│  NO business logic - doesn't interpret announcement_type         │
└────────────────────────┬────────────────────────────────────────┘
                         │ UDP Port 7184
┌────────────────────────▼────────────────────────────────────────┐
│                        Network Layer                             │
│               (UDP Broadcast: 255.255.255.255:7184)              │
└──────────────────────────────────────────────────────────────────┘
```

## Core Concepts

### 1. UdpAnnouncement Envelope

ALL UDP messages MUST use this envelope format:

```rust
pub struct UdpAnnouncement {
    pub announcement_type: String,  // e.g., "discovery_request"
    pub data: serde_json::Value,    // Typed payload
}
```

**Rules**:
- `announcement_type` MUST be one of the constants from `garden_common::announcement_types`
- `data` MUST be a valid JSON value that deserializes to the expected type
- Envelope size MUST NOT exceed 65507 bytes (UDP limit)
- Invalid envelopes MUST be rejected by transport layer

### 2. Announcement Types

Defined in `garden_common::announcement_types`:

```rust
pub const DISCOVERY_REQUEST: &str = "discovery_request";
pub const DISCOVERY_RESPONSE: &str = "discovery_response";
pub const STONE_CHIRP: &str = "stone_chirp";
pub const STONE_GOODBYE: &str = "stone_goodbye";
pub const ELECTION_REQUEST: &str = "election_request";
pub const ELECTION_CANDIDATE: &str = "election_candidate";
pub const ELECTION_RESULT: &str = "election_result";
```

**Adding New Types**:
1. Add constant to `announcement_types` module
2. Define payload struct in appropriate domain module
3. Add `UdpEvent` variant if needed
4. Create domain handler to process the type

### 3. UdpEvent Stream

Transport broadcasts events to subscribers:

```rust
pub enum UdpEvent {
    Request { 
        request: UdpAnnouncement, 
        source: SocketAddr 
    },
    Response { 
        response: UdpAnnouncement, 
        source: SocketAddr 
    },
    Chirp { 
        chirp: StoneChirp, 
        source: SocketAddr 
    },
    Goodbye { 
        goodbye: StoneGoodbye, 
        source: SocketAddr 
    },
    ElectionRequest { 
        request: ElectionRequest, 
        source: SocketAddr 
    },
    ElectionCandidate { 
        candidate: ElectionCandidate, 
        source: SocketAddr 
    },
    ElectionResult { 
        result: ElectionResult, 
        source: SocketAddr 
    },
}
```

**Classification Rules**:
- `announcement_type` ending in `_request` → `UdpEvent::Request`
- `announcement_type` ending in `_response` → `UdpEvent::Response`
- Specific types (chirp, goodbye, election_*) → Typed variants
- Unknown types → Ignored with warning

## Transport Layer API

### For Moss (Server-Side)

#### Initialization

```rust
use crate::infra::communications::p2p;

// Called once during bootstrap
p2p::initialize_transport().await?;
```

**MUST** be called before any other p2p operations.  
**MUST** only be called once per process.

#### Subscribing to Events

**Filtered Subscription (Recommended):**
```rust
// In domain handler (e.g., discovery_handler.rs)
use crate::infra::communications::p2p;
use garden_common::announcement_types;

let mut udp_rx = p2p::subscribe_to_announcement(announcement_types::DISCOVERY_REQUEST).await?;

loop {
    match udp_rx.recv().await {
        Ok((payload, source)) => {
            // Automatically filtered - only DISCOVERY_REQUEST events
            let request: DiscoveryRequest = serde_json::from_value(payload)?;
            handle_request(request, source).await?;
        },
        Err(e) => {
            tracing::error!(error = ?e, "Event receive error");
            break;
        }
    }
}
```

**Multi-Type Subscription (Advanced):**
```rust
// For handlers that need multiple announcement types
let mut udp_rx = p2p::subscribe_to_all_events().await?;

loop {
    match udp_rx.recv().await {
        Ok(UdpEvent::Chirp { chirp, source }) => {
            // Handle chirp
        },
        Ok(UdpEvent::Goodbye { goodbye, source }) => {
            // Handle goodbye
        },
        Ok(_) => { /* Ignore other types */ },
        Err(e) => break,
    }
}
```

**Rules**:
- Use filtered subscriptions for single-purpose handlers (discovery, election)
- Use multi-type subscriptions only when handling multiple related types (coordinator)
- Each subscriber gets an independent channel
- Slow subscribers MAY miss events (channel overflow)
- Subscribers SHOULD run in separate tokio tasks

#### Sending Announcements

```rust
use garden_common::announcement_types;

let payload = DiscoveryResponse {
    stone_name: "stone-crystal-forest".to_string(),
    stone_endpoint: "http://192.168.1.100:7185".to_string(),
    moss_version: "0.1.0".to_string(),
    request_id: request.request_id,
};

p2p::send_announcement(
    announcement_types::DISCOVERY_RESPONSE,
    &payload
).await?;
```

**Rules**:
- Payload MUST implement `Serialize`
- Sends to broadcast address (255.255.255.255:7184)
- Non-blocking - does not wait for delivery
- Failures are logged but not propagated

### For Rake (Client-Side)

Rake does NOT have access to moss p2p infrastructure. It MUST create its own UDP socket and speak the protocol directly.

#### Sending Discovery Request

```rust
use garden_common::{UdpAnnouncement, announcement_types, DiscoveryRequest};
use tokio::net::UdpSocket;

// Create ephemeral socket
let socket = UdpSocket::bind("0.0.0.0:0").await?;
socket.set_broadcast(true)?;

// Wrap request in envelope
let request = DiscoveryRequest {
    discover: "moss".to_string(),
    request_id: uuid::Uuid::now_v7().to_string(),
    requester: "rake-cli".to_string(),
};

let announcement = UdpAnnouncement {
    announcement_type: announcement_types::DISCOVERY_REQUEST.to_string(),
    data: serde_json::to_value(&request)?,
};

// Send broadcast
let bytes = serde_json::to_vec(&announcement)?;
socket.send_to(&bytes, "255.255.255.255:7184").await?;
```

#### Receiving Discovery Response

```rust
let mut buf = [0u8; 2048];
socket.set_read_timeout(Some(Duration::from_secs(3)))?;

let (len, addr) = socket.recv_from(&mut buf).await?;

// Parse envelope
let envelope = serde_json::from_slice::<UdpAnnouncement>(&buf[..len])?;

if envelope.announcement_type == announcement_types::DISCOVERY_RESPONSE {
    let response: DiscoveryResponse = serde_json::from_value(envelope.data)?;
    println!("Found: {} at {}", response.stone_name, response.stone_endpoint);
}
```

### For External Scripts (PowerShell)

```powershell
# Create envelope
$requestData = @{
    discover = "moss"
    request_id = [guid]::NewGuid().ToString()
    requester = "deploy"
}

$announcement = @{
    announcement_type = "discovery_request"
    data = $requestData
} | ConvertTo-Json -Compress

# Send broadcast
$udpClient = New-Object System.Net.Sockets.UdpClient
$udpClient.EnableBroadcast = $true
$requestBytes = [System.Text.Encoding]::UTF8.GetBytes($announcement)
$udpClient.Send($requestBytes, $requestBytes.Length, "255.255.255.255", 7184)

# Receive response
$remoteEP = New-Object System.Net.IPEndPoint([System.Net.IPAddress]::Any, 0)
$responseBytes = $udpClient.Receive([ref]$remoteEP)
$responseJson = [System.Text.Encoding]::UTF8.GetString($responseBytes)
$envelope = $responseJson | ConvertFrom-Json

if ($envelope.announcement_type -eq "discovery_response") {
    $response = $envelope.data
    Write-Host "Found: $($response.stone_name) at $($response.stone_endpoint)"
}
```

## Domain Handler Pattern

### Creating a Handler

```rust
// moss/src/tasks/my_handler.rs

use crate::infra::communications::p2p;
use garden_common::announcement_types;

pub async fn start_my_handler() -> anyhow::Result<()> {
    let mut udp_rx = p2p::subscribe_to_announcement(announcement_types::MY_REQUEST).await?;
    
    tracing::info!("My handler started");
    
    loop {
        match udp_rx.recv().await {
            Ok((payload, source)) => {
                handle_request(payload, source).await?;
            },
            Err(e) => {
                tracing::error!(error = ?e, "Event receive error");
                break;
            }
        }
    }
    
    Ok(())
}

async fn handle_request(
    payload: serde_json::Value,
    source: SocketAddr
) -> anyhow::Result<()> {
    // 1. Parse request payload
    let request: MyRequest = serde_json::from_value(payload)?;
    
    // 2. Execute business logic
    let result = process_request(&request)?;
    
    // 3. Send response via p2p
    let response = MyResponse { /* ... */ };
    p2p::send_announcement(
        announcement_types::MY_RESPONSE,
        &response
    ).await?;
    
    Ok(())
}
```

### Spawning Handler in Bootstrap

```rust
// moss/src/bootstrap/run.rs

tokio::spawn(async move {
    if let Err(e) = my_handler::start_my_handler().await {
        tracing::error!(error = ?e, "My handler failed");
    }
});
```

## Protocol Rules

### MUST Requirements

1. **Envelope Format**: All UDP messages MUST use UdpAnnouncement envelope
2. **Port 7184**: All moss-related UDP MUST use port 7184
3. **Single Socket**: Moss MUST NOT create additional UDP sockets
4. **Type Constants**: announcement_type MUST use constants from announcement_types module
5. **JSON Payload**: data field MUST be valid JSON
6. **Broadcast Address**: Announcements MUST be sent to 255.255.255.255:7184

### MUST NOT Requirements

1. **No Bespoke Sockets**: Domain handlers MUST NOT call `UdpSocket::bind()`
2. **No Direct Send**: Domain handlers MUST NOT use `socket.send_to()` directly
3. **No Business Logic in Transport**: p2p.rs MUST NOT interpret announcement content
4. **No Blocking**: p2p operations MUST be async, never blocking

### SHOULD Recommendations

1. **Idempotency**: Handlers SHOULD handle duplicate messages gracefully
2. **Timeouts**: Clients SHOULD implement timeouts (recommended: 3 seconds)
3. **Buffer Size**: Clients SHOULD use 2048-byte buffers for envelopes
4. **Error Logging**: Handlers SHOULD log parse errors but continue running

## Example: Discovery Protocol

## Example: Discovery Protocol

Demonstrating the pipeline with discovery as a concrete use case.

### Discovery Handler (Moss)

## Example: Discovery Protocol

Demonstrating the pipeline with discovery as a concrete use case.

### Discovery Handler (Moss)

```rust
// moss/src/tasks/discovery_handler.rs
use crate::infra::communications::p2p;
use garden_common::{announcement_types, DiscoveryRequest, DiscoveryResponse};

pub async fn start_discovery_handler(self_entry: StoneTopologyEntry) -> anyhow::Result<()> {
    let mut udp_rx = p2p::subscribe_to_announcement(announcement_types::DISCOVERY_REQUEST).await?;
    
    loop {
        match udp_rx.recv().await {
            Ok((payload, source)) => {
                let request: DiscoveryRequest = serde_json::from_value(payload)?;
                
                let response = DiscoveryResponse {
                    stone_name: self_entry.stone_name.clone(),
                    stone_endpoint: self_entry.endpoint.clone(),
                    moss_version: env!("CARGO_PKG_VERSION").to_string(),
                    request_id: request.request_id,
                };
                
                p2p::send_announcement(
                    announcement_types::DISCOVERY_RESPONSE,
                    &response
                ).await?;
            },
            Err(e) => {
                tracing::error!(error = ?e, "Discovery handler error");
                break;
            }
        }
    }
    
    Ok(())
}
```

### Discovery Client (Rake)

```rust
// rake/src/discovery.rs
use garden_common::{UdpAnnouncement, announcement_types, DiscoveryRequest, DiscoveryResponse};
use tokio::net::UdpSocket;

pub fn discover_moss() -> Result<String> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_broadcast(true)?;
    socket.set_read_timeout(Some(Duration::from_secs(3)))?;

    let request = DiscoveryRequest {
        discover: "moss".to_string(),
        request_id: uuid::Uuid::now_v7().to_string(),
        requester: "rake-cli".to_string(),
    };

    // Wrap in envelope
    let announcement = UdpAnnouncement {
        announcement_type: announcement_types::DISCOVERY_REQUEST.to_string(),
        data: serde_json::to_value(&request)?,
    };

    let bytes = serde_json::to_vec(&announcement)?;
    socket.send_to(&bytes, "255.255.255.255:7184")?;

    // Receive envelope
    let mut buf = [0u8; 2048];
    let (len, _) = socket.recv_from(&mut buf)?;
    
    let envelope = serde_json::from_slice::<UdpAnnouncement>(&buf[..len])?;
    
    if envelope.announcement_type == announcement_types::DISCOVERY_RESPONSE {
        let response: DiscoveryResponse = serde_json::from_value(envelope.data)?;
        return Ok(response.stone_endpoint);
    }
    
    Err(anyhow::anyhow!("Invalid response type"))
}
```

### Message Flow

```
Client (rake)                  Transport (p2p)               Handler (discovery)
     |                              |                              |
     | UdpAnnouncement              |                              |
     | { type: "discovery_request"} |                              |
     |----------------------------->|                              |
     |                              |                              |
     |                              | UdpEvent::Request            |
     |                              |----------------------------->|
     |                              |                              |
     |                              |                              | (business logic)
     |                              |                              |
     |                              | send_announcement()          |
     |                              |<-----------------------------|
     |                              |                              |
     | UdpAnnouncement              |                              |
     | { type: "discovery_response"}|                              |
     |<-----------------------------|                              |
     |                              |                              |
```

## Testing

### Unit Testing Handlers

Handlers can be tested without network access by mocking the p2p layer:

```rust
#[cfg(test)]
mod tests {
    use tokio::sync::mpsc;
    
    #[tokio::test]
    async fn test_discovery_handler() {
        // Create mock filtered channel
        let (tx, mut rx) = mpsc::channel(10);
        
        // Send mock request payload
        let payload = serde_json::json!({
            "discover": "moss",
            "request_id": "test-123",
            "requester": "test"
        });
        let source = "127.0.0.1:12345".parse().unwrap();
        
        tx.send((payload, source)).await.unwrap();
        
        // Verify handler processes correctly
        if let Some((received_payload, received_source)) = rx.recv().await {
            assert_eq!(received_source.to_string(), "127.0.0.1:12345");
            let request: DiscoveryRequest = serde_json::from_value(received_payload).unwrap();
            assert_eq!(request.request_id, "test-123");
        }
    }
}
```

### Integration Testing

```bash
# Terminal 1: Start moss
cargo run --bin garden-moss

# Terminal 2: Test discovery
echo '{"announcement_type":"discovery_request","data":{"discover":"moss","request_id":"test","requester":"manual"}}' | \
  nc -u -w1 127.0.0.1 7184

# Expected response:
# {"announcement_type":"discovery_response","data":{"stone_name":"...","stone_endpoint":"...","moss_version":"...","request_id":"test"}}
```

## Migration Guide

### Removing Bespoke UDP Code

**Before (❌ Violates COMM-0002)**:
```rust
// domain/my_feature.rs
let socket = UdpSocket::bind("0.0.0.0:7184").await?;
socket.send_to(&bytes, "255.255.255.255:7184").await?;
```

**After (✅ Compliant)**:
```rust
// tasks/my_feature_handler.rs
use crate::infra::communications::p2p;
use garden_common::announcement_types;

let mut udp_rx = p2p::subscribe_to_announcement(announcement_types::MY_REQUEST).await?;

loop {
    match udp_rx.recv().await {
        Ok((payload, source)) => {
            let request: MyRequest = serde_json::from_value(payload)?;
            handle_request(request, source).await?;
        },
        Err(e) => break,
    }
}

// Send response
p2p::send_announcement(announcement_types::MY_RESPONSE, &response).await?;
```

### Adding New Announcement Type

1. **Define constant**:
```rust
// common/src/announcement_types.rs
pub const MY_NEW_REQUEST: &str = "my_new_request";
pub const MY_NEW_RESPONSE: &str = "my_new_response";
```

2. **Define payload types**:
```rust
// common/src/types/my_feature.rs
#[derive(Serialize, Deserialize)]
pub struct MyNewRequest {
    pub field1: String,
    pub field2: i32,
}

#[derive(Serialize, Deserialize)]
pub struct MyNewResponse {
    pub result: String,
}
```

3. **Create handler**:
```rust
// moss/src/tasks/my_feature_handler.rs
use crate::infra::communications::p2p;
use garden_common::announcement_types;

pub async fn start_my_feature_handler() -> anyhow::Result<()> {
    let mut udp_rx = p2p::subscribe_to_announcement(announcement_types::MY_NEW_REQUEST).await?;
    
    loop {
        match udp_rx.recv().await {
            Ok((payload, source)) => {
                let request: MyNewRequest = serde_json::from_value(payload)?;
                handle_request(request, source).await?;
            },
            Err(e) => break,
        }
    }
    
    Ok(())
}
```

4. **Spawn in bootstrap**:
```rust
// moss/src/bootstrap/run.rs
tokio::spawn(async move {
    if let Err(e) = my_feature_handler::start_my_feature_handler().await {
        tracing::error!(error = ?e, "My feature handler failed");
    }
});
```

## Compliance Checklist

- [ ] All UDP communication uses UdpAnnouncement envelope
- [ ] No `UdpSocket::bind()` outside `p2p.rs`
- [ ] announcement_type uses constants from announcement_types module
- [ ] Domain handlers subscribe to p2p events
- [ ] Domain handlers send via `p2p::send_announcement()`
- [ ] Rake clients implement envelope wrapping/unwrapping
- [ ] External scripts (PowerShell) use envelope format
- [ ] New announcement types documented in this spec
- [ ] Handlers spawned in bootstrap/run.rs
- [ ] Integration tests validate envelope format

## References

- [COMM-0001: P2P Transport Singleton](COMM-0001-p2p-transport-singleton.md)
- [Components](../reference/components.md) - P2P transport singleton
- RFC 2119: Key words for use in RFCs to Indicate Requirement Levels

---

**For Moss Developers**: All new UDP features MUST follow this specification.  
**For Rake Developers**: Client discovery MUST use envelope format.  
**For Script Authors**: PowerShell/Bash scripts MUST wrap requests/responses in envelopes.
