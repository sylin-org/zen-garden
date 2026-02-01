# How to Create an Infrastructure Handler

**Version**: 0.1.0
**Last Updated**: 2026-01-31

---

## Overview

**Infrastructure handlers** are self-contained modules in Moss that react to garden topology changes and configure local system infrastructure. They enable distributed, autonomous configuration where each Stone manages its own systems based on what's happening across the garden.

### Key Principles

1. **Local Only**: Handlers only affect the LOCAL Stone's infrastructure
2. **Self-Contained**: Each handler knows what offerings it matches and what actions to take
3. **Distributed**: All Stones react to topology changes independently
4. **SoC Compliant**: Behavioral logic lives in Moss domain, not in offering manifests

### Built-in Handlers

| Handler | Purpose | Triggered By |
|---------|---------|--------------|
| `DockerRegistryHandler` | Configures Docker daemon's insecure-registries | Registry offerings (registry, zot, harbor) |

---

## Quick Start

**Goal**: Create a handler that reacts when specific offerings appear in the garden.

**Steps**:
1. Create handler struct implementing `InfrastructureHandler` trait
2. Define matching logic (what offerings trigger this handler)
3. Implement sync logic (what to configure locally)
4. Register handler in the registry
5. Test the handler

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                       Stone A                                │
│  ┌──────────────┐                                           │
│  │   Offering   │  ──chirp──►  UDP Broadcast                │
│  │   (Registry) │                    │                      │
│  └──────────────┘                    │                      │
└──────────────────────────────────────│──────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────┐
│                       Stone B                                │
│                          │                                   │
│                          ▼                                   │
│  ┌──────────────────────────────────────────────────────┐  │
│  │              Topology Cache                           │  │
│  │  (receives chirp, stores registry info)               │  │
│  └──────────────────────────────────────────────────────┘  │
│                          │                                   │
│                          ▼                                   │
│  ┌──────────────────────────────────────────────────────┐  │
│  │        Infrastructure Handler Registry                │  │
│  │                                                        │  │
│  │  ┌────────────────────────────────────┐               │  │
│  │  │    DockerRegistryHandler           │               │  │
│  │  │    matches("registry") → true      │               │  │
│  │  │    sync() → update daemon.json     │               │  │
│  │  └────────────────────────────────────┘               │  │
│  │                                                        │  │
│  │  ┌────────────────────────────────────┐               │  │
│  │  │    YourCustomHandler               │               │  │
│  │  │    matches("your-offering") → true │               │  │
│  │  │    sync() → configure something    │               │  │
│  │  └────────────────────────────────────┘               │  │
│  └──────────────────────────────────────────────────────┘  │
│                          │                                   │
│                          ▼                                   │
│  ┌──────────────────────────────────────────────────────┐  │
│  │         Local Infrastructure                          │  │
│  │    (Docker daemon.json, DNS config, etc.)             │  │
│  └──────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────┘
```

---

## Handler Trait

The core trait that all handlers must implement:

```rust
use async_trait::async_trait;

/// Instance of a matching offering in the garden
pub struct OfferingInstance {
    pub stone_name: String,
    pub stone_endpoint: String,
    pub offering: String,
    pub category: String,
    pub tags: Vec<String>,
}

/// Trait for infrastructure handlers
#[async_trait]
pub trait InfrastructureHandler: Send + Sync {
    /// Handler name for logging
    fn name(&self) -> &'static str;

    /// Check if this handler cares about an offering
    fn matches(&self, offering: &str, category: &str, tags: &[String]) -> bool;

    /// Synchronize local infrastructure with current instances
    /// Called whenever topology changes (new chirp received)
    async fn sync(&self, instances: &[OfferingInstance]) -> anyhow::Result<()>;
}
```

---

## Creating a Handler

### Step 1: Define the Handler Struct

```rust
// src/moss/src/domain/infrastructure/my_handler.rs

//! My Custom Handler - Manages local X when Y offerings are deployed
//!
//! Self-contained handler that:
//! - Matches: offerings named "my-offering" OR category="my-category"
//! - Action: Updates local configuration, restarts service if needed

use super::{InfrastructureHandler, OfferingInstance};
use async_trait::async_trait;
use anyhow::Result;

pub struct MyCustomHandler {
    // Optional: configuration or state
}

impl MyCustomHandler {
    pub fn new() -> Self {
        Self {}
    }
}
```

### Step 2: Implement Matching Logic

The `matches()` method determines which offerings trigger this handler:

```rust
#[async_trait]
impl InfrastructureHandler for MyCustomHandler {
    fn name(&self) -> &'static str {
        "my-custom"
    }

    fn matches(&self, offering: &str, category: &str, tags: &[String]) -> bool {
        // Option 1: Match by name
        matches!(offering, "my-offering" | "other-offering")

        // Option 2: Match by category
        || category == "my-category"

        // Option 3: Match by tag
        || tags.iter().any(|t| t == "my-special-tag")
    }

    // ...
}
```

**Matching Strategies**:

| Strategy | Example | Use When |
|----------|---------|----------|
| By Name | `offering == "registry"` | Specific, well-known offerings |
| By Category | `category == "devops"` | All offerings in a category |
| By Tag | `tags.contains("dns-server")` | Cross-category functionality |
| Combined | Name OR (category AND tag) | Complex matching requirements |

### Step 3: Implement Sync Logic

The `sync()` method configures local infrastructure:

```rust
    async fn sync(&self, instances: &[OfferingInstance]) -> Result<()> {
        if instances.is_empty() {
            // No matching offerings in garden - clean up local config
            self.cleanup_config().await?;
            return Ok(());
        }

        // Build configuration from instances
        let endpoints: Vec<String> = instances
            .iter()
            .map(|i| format!("{}:{}", extract_host(&i.stone_endpoint), 8080))
            .collect();

        // Read current local config
        let current = self.read_current_config().await?;

        // Compare and update if changed
        if current != endpoints {
            tracing::info!(
                handler = self.name(),
                endpoints = ?endpoints,
                "Updating local configuration"
            );

            self.write_config(&endpoints).await?;
            self.restart_service().await?;
        }

        Ok(())
    }
```

**Sync Principles**:

1. **Idempotent**: Running sync multiple times produces the same result
2. **Incremental**: Only change what's different
3. **Silent by default**: No user interaction required
4. **Resilient**: Handle errors gracefully (log, don't crash)

### Step 4: Add Infrastructure I/O Layer

Keep I/O separate from domain logic:

```rust
// src/moss/src/infra/my_config.rs

//! My configuration file management
//!
//! Handles reading/writing config files and restarting services.

use anyhow::Result;

/// Platform-specific config file path
pub fn config_path() -> &'static str {
    #[cfg(target_os = "linux")]
    { "/etc/my-service/config.json" }

    #[cfg(target_os = "windows")]
    { r"C:\ProgramData\MyService\config.json" }

    #[cfg(target_os = "macos")]
    { "/usr/local/etc/my-service/config.json" }
}

/// Read current configuration
pub async fn read_config() -> Result<MyConfig> {
    let path = config_path();
    if !std::path::Path::new(path).exists() {
        return Ok(MyConfig::default());
    }

    let content = tokio::fs::read_to_string(path).await?;
    let config: MyConfig = serde_json::from_str(&content)?;
    Ok(config)
}

/// Write configuration (returns true if changed)
pub async fn write_config(config: &MyConfig) -> Result<bool> {
    let path = config_path();
    let current = read_config().await.unwrap_or_default();

    if current == *config {
        return Ok(false); // No change needed
    }

    // Ensure directory exists
    if let Some(parent) = std::path::Path::new(path).parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // Write atomically (to temp file, then rename)
    let temp_path = format!("{}.tmp", path);
    let content = serde_json::to_string_pretty(config)?;
    tokio::fs::write(&temp_path, &content).await?;
    tokio::fs::rename(&temp_path, path).await?;

    Ok(true)
}

/// Restart service (platform-specific)
pub async fn restart_service() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        tokio::process::Command::new("systemctl")
            .args(["restart", "my-service"])
            .status()
            .await?;
    }

    #[cfg(target_os = "windows")]
    {
        tokio::process::Command::new("net")
            .args(["stop", "MyService"])
            .status()
            .await?;
        tokio::process::Command::new("net")
            .args(["start", "MyService"])
            .status()
            .await?;
    }

    Ok(())
}
```

### Step 5: Register the Handler

Add your handler to the registry in `src/moss/src/domain/infrastructure/mod.rs`:

```rust
pub mod my_handler;
pub use my_handler::MyCustomHandler;

impl InfrastructureHandlerRegistry {
    /// Create registry with all built-in handlers
    pub fn with_defaults() -> Self {
        Self {
            handlers: vec![
                Box::new(DockerRegistryHandler::new()),
                Box::new(MyCustomHandler::new()),  // Add your handler
            ],
        }
    }
}
```

### Step 6: Export from Module

Update `src/moss/src/infra/mod.rs`:

```rust
pub mod my_config;
pub use my_config::{read_config, write_config, restart_service};
```

---

## Testing Handlers

### Unit Tests

Test matching logic:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_by_name() {
        let handler = MyCustomHandler::new();
        assert!(handler.matches("my-offering", "other", &[]));
        assert!(!handler.matches("unrelated", "other", &[]));
    }

    #[test]
    fn test_matches_by_category() {
        let handler = MyCustomHandler::new();
        assert!(handler.matches("anything", "my-category", &[]));
    }

    #[test]
    fn test_matches_by_tag() {
        let handler = MyCustomHandler::new();
        let tags = vec!["my-special-tag".to_string()];
        assert!(handler.matches("anything", "any", &tags));
    }
}
```

### Integration Tests

Test sync behavior:

```rust
#[tokio::test]
async fn test_sync_adds_entries() {
    let handler = MyCustomHandler::new();
    let instances = vec![
        OfferingInstance {
            stone_name: "stone-a".to_string(),
            stone_endpoint: "http://192.168.1.10:7185".to_string(),
            offering: "my-offering".to_string(),
            category: "my-category".to_string(),
            tags: vec![],
        },
    ];

    handler.sync(&instances).await.unwrap();

    // Verify config was updated
    let config = read_config().await.unwrap();
    assert!(config.endpoints.contains(&"192.168.1.10:8080".to_string()));
}

#[tokio::test]
async fn test_sync_removes_entries_when_empty() {
    let handler = MyCustomHandler::new();

    // First, add some entries
    handler.sync(&[/* ... */]).await.unwrap();

    // Then sync with empty list
    handler.sync(&[]).await.unwrap();

    // Verify config was cleaned up
    let config = read_config().await.unwrap();
    assert!(config.endpoints.is_empty());
}
```

### Manual Testing

```bash
# On Stone A - plant your offering
garden-rake plant my-offering

# On Stone B - check if handler ran
# (Check your config file)
cat /etc/my-service/config.json

# Check Moss logs for handler activity
journalctl -u garden-moss | grep "my-custom"
```

---

## Handler Examples

### Docker Registry Handler

The built-in handler for container registries:

```rust
impl InfrastructureHandler for DockerRegistryHandler {
    fn name(&self) -> &'static str {
        "docker-registry"
    }

    fn matches(&self, offering: &str, category: &str, tags: &[String]) -> bool {
        // Match by name
        matches!(offering, "registry" | "zot" | "harbor")
        // OR by category + tag
        || (category == "devops" && tags.iter().any(|t| t == "container-registry"))
    }

    async fn sync(&self, instances: &[OfferingInstance]) -> Result<()> {
        // Build registry endpoints
        let registries: Vec<String> = instances
            .iter()
            .map(|i| {
                let port = self.infer_registry_port(&i.offering);
                format!("{}:{}", extract_host(&i.stone_endpoint), port)
            })
            .collect();

        // Update daemon.json
        let changed = crate::infra::docker_config::write_insecure_registries(&registries).await?;

        if changed {
            crate::infra::docker_config::restart_docker_daemon().await?;
        }

        Ok(())
    }
}
```

### DNS Server Handler (Hypothetical)

A handler for local DNS configuration:

```rust
pub struct DnsServerHandler;

impl InfrastructureHandler for DnsServerHandler {
    fn name(&self) -> &'static str {
        "dns-server"
    }

    fn matches(&self, offering: &str, category: &str, tags: &[String]) -> bool {
        matches!(offering, "pihole" | "adguard" | "coredns")
        || (category == "networking" && tags.iter().any(|t| t == "dns-server"))
    }

    async fn sync(&self, instances: &[OfferingInstance]) -> Result<()> {
        // Update /etc/resolv.conf or systemd-resolved config
        // to use garden DNS servers

        let dns_servers: Vec<String> = instances
            .iter()
            .map(|i| extract_ip(&i.stone_endpoint))
            .collect();

        crate::infra::dns_config::update_nameservers(&dns_servers).await
    }
}
```

### Certificate Authority Handler (Hypothetical)

A handler for trusting garden CAs:

```rust
pub struct CertificateAuthorityHandler;

impl InfrastructureHandler for CertificateAuthorityHandler {
    fn name(&self) -> &'static str {
        "certificate-authority"
    }

    fn matches(&self, offering: &str, _category: &str, tags: &[String]) -> bool {
        matches!(offering, "step-ca" | "vault")
        || tags.iter().any(|t| t == "certificate-authority")
    }

    async fn sync(&self, instances: &[OfferingInstance]) -> Result<()> {
        // Download CA certificates from each instance
        // Install in system trust store

        for instance in instances {
            let ca_url = format!("{}/ca/root.crt", instance.stone_endpoint);
            let cert = fetch_certificate(&ca_url).await?;
            install_ca_certificate(&instance.stone_name, &cert).await?;
        }

        Ok(())
    }
}
```

---

## Best Practices

### 1. Match Conservatively

```rust
// GOOD: Specific matching
fn matches(&self, offering: &str, _: &str, tags: &[String]) -> bool {
    offering == "registry" || tags.contains(&"container-registry".to_string())
}

// BAD: Too broad
fn matches(&self, _: &str, category: &str, _: &[String]) -> bool {
    category == "devops"  // Matches too many offerings!
}
```

### 2. Handle Empty Instances

```rust
async fn sync(&self, instances: &[OfferingInstance]) -> Result<()> {
    if instances.is_empty() {
        // Important: clean up when all offerings are removed
        self.remove_all_entries().await?;
        return Ok(());
    }
    // ...
}
```

### 3. Be Idempotent

```rust
// GOOD: Check before modifying
if current_config != new_config {
    write_config(&new_config).await?;
    restart_service().await?;
}

// BAD: Always modify
write_config(&new_config).await?;
restart_service().await?;  // Unnecessary restarts!
```

### 4. Log at Appropriate Levels

```rust
// Info: significant actions
tracing::info!(handler = self.name(), "Updated local configuration");

// Debug: routine operations
tracing::debug!(handler = self.name(), instances = instances.len(), "Sync called");

// Warn: recoverable issues
tracing::warn!(handler = self.name(), error = ?e, "Failed to restart service");
```

### 5. Don't Block the Discovery Loop

Sync operations run in a spawned task to avoid blocking chirp processing:

```rust
// In coordinator.rs
tokio::spawn(async move {
    handlers.on_topology_changed(&cache, &manifests).await;
});
```

---

## Reference Files

| File | Purpose |
|------|---------|
| `src/moss/src/domain/infrastructure/mod.rs` | Handler trait and registry |
| `src/moss/src/domain/infrastructure/docker_registry.rs` | Docker registry handler |
| `src/moss/src/infra/docker_config.rs` | Docker daemon.json I/O |
| `src/moss/src/tasks/coordinator.rs` | Handler hook after topology update |
| `docs/decisions/MOSS-0002-infrastructure-handlers.md` | ADR documenting the design |

---

## Troubleshooting

### Handler Not Firing

1. Check matching logic with test
2. Verify offering has correct tags in frontmatter.json
3. Check Moss logs for handler sync calls

```bash
journalctl -u garden-moss | grep "infrastructure"
```

### Config Not Updating

1. Check file permissions
2. Verify config path for your platform
3. Check if service restart failed

### Service Restart Failed

1. Verify service name is correct
2. Check if Moss has required permissions (root/sudo for system services)
3. Check service logs directly

---

## Related Documentation

- [ADR: Infrastructure Handlers](../decisions/MOSS-0002-infrastructure-handlers.md)
- [Topology and Discovery](../specs/MOSS-SPEC.md)
- [Offerings and Manifests](./offering-services.md)
