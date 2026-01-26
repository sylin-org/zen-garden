# OS-Aware Detection for Adopted Offerings v2.0

**Status**: Implemented (2026-01-25)  
**Breaking Change**: Detection structure changed from flat list to OS-grouped  
**Migration**: See [Migration Guide](#migration-from-v10) below

---

## Overview

Adopted offerings use **OS-grouped detection rules** to locate and monitor native services. The new structure organizes detection by operating system, allowing Moss to efficiently load only the rules relevant to the current platform.

**Key Benefits**:
- ✅ **No redundancy**: Each OS has its own detection rules (no duplicate commands)
- ✅ **Fast matching**: Moss loads only current OS rules (no runtime conditionals)
- ✅ **Clear separation**: Different OSes can have completely different strategies
- ✅ **Maintainability**: Easy to add/modify OS-specific detection without affecting others

---

## Manifest Structure v2.0

### Schema

```yaml
detection:
  windows:    # Detection rules for Windows
    - method: command
      config:
        command: <command>
        expected_pattern: <regex>
        expected_exit_code: <code>
      stability_threshold: <count>
      cache_ttl_secs: <seconds>
    
    - method: http_probe
      config:
        url: <url>
        expected_status: <status>
        timeout_ms: <ms>
  
  linux:      # Detection rules for Linux
    - method: command
      config: { ... }
  
  macos:      # Detection rules for macOS
    - method: command
      config: { ... }
```

### Basic Example

```yaml
name: ollama
category: ai
modes:
  - adopted

detection:
  windows:
    - method: command
      config:
        command: ollama --version
        expected_pattern: "version is ([0-9]+\\.[0-9]+\\.[0-9]+)"
  
  linux:
    - method: command
      config:
        command: ollama --version
        expected_pattern: "version is ([0-9]+\\.[0-9]+\\.[0-9]+)"
  
  macos:
    - method: command
      config:
        command: ollama --version
        expected_pattern: "version is ([0-9]+\\.[0-9]+\\.[0-9]+)"
```

---

## Detection Methods

Each OS can define multiple detection rules using these methods:

### 1. Command Execution

Run a shell command and validate output:

```yaml
windows:
  - method: command
    config:
      command: ollama --version
      expected_pattern: "version ([0-9]+\\.[0-9]+\\.[0-9]+)"
      expected_exit_code: 0
    stability_threshold: 1
    cache_ttl_secs: 3600
```

**Fields**:
- `command` (required): Shell command to execute
- `expected_pattern` (optional): Regex pattern (extracts version if capture group present)
- `expected_exit_code` (optional): Expected exit code (default: 0)

### 2. HTTP Probe

Query a service API endpoint:

```yaml
linux:
  - method: http_probe
    config:
      url: http://localhost:11434/api/tags
      expected_status: 200
      timeout_ms: 2000
    stability_threshold: 2
    cache_ttl_secs: 60
```

**Fields**:
- `url` (required): HTTP endpoint to probe
- `expected_status` (optional): Expected HTTP status (default: 200)
- `timeout_ms` (optional): Request timeout (default: 2000)

### 3. Container Inspection

Check Docker container (for hybrid scenarios):

```yaml
windows:
  - method: container_inspect
    config:
      container_pattern: "zen-offering-mongodb"
      image_pattern: "mongo:.*"
    stability_threshold: 1
    cache_ttl_secs: 300
```

**Fields**:
- `container_pattern` (required): Container name regex
- `image_pattern` (optional): Expected image regex

---

## Platform-Specific Patterns

### Windows: PowerShell Commands

```yaml
detection:
  windows:
    # Check if process is running
    - method: command
      config:
        command: powershell -Command "Get-Process ollama -ErrorAction SilentlyContinue | Select-Object -First 1"
        expected_pattern: "ollama"
    
    # Check Windows service status
    - method: command
      config:
        command: powershell -Command "Get-Service -Name 'Ollama' -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Status"
        expected_pattern: "Running"
    
    # Check registry for install path
    - method: command
      config:
        command: powershell -Command "Get-ItemPropertyValue -Path 'HKLM:\\SOFTWARE\\Ollama' -Name 'InstallPath' -ErrorAction SilentlyContinue"
        expected_exit_code: 0
```

### Linux: systemd Services

```yaml
detection:
  linux:
    # Check systemd service
    - method: command
      config:
        command: systemctl is-active ollama
        expected_pattern: "^active$"
    
    # Check if binary exists in PATH
    - method: command
      config:
        command: which ollama
        expected_exit_code: 0
    
    # Check process via pidof
    - method: command
      config:
        command: pidof ollama
        expected_exit_code: 0
```

### macOS: launchd Services

```yaml
detection:
  macos:
    # Check launchd service
    - method: command
      config:
        command: launchctl list | grep ollama
        expected_exit_code: 0
    
    # Check Homebrew installation
    - method: command
      config:
        command: brew list ollama
        expected_exit_code: 0
    
    # Check application bundle
    - method: command
      config:
        command: test -d "/Applications/Ollama.app"
        expected_exit_code: 0
```

---

## Multi-Layered Detection Example

Ollama has different states: installed, running, operational. Use multiple rules to distinguish:

```yaml
detection:
  windows:
    # Rule 1: Detect INSTALLATION (passes whether running or not)
    - method: command
      config:
        command: ollama --version
        expected_pattern: "version is ([0-9]+\\.[0-9]+\\.[0-9]+)"
      stability_threshold: 1
      cache_ttl_secs: 3600  # Installation is stable
    
    # Rule 2: Detect RUNNING state (only passes when service is up)
    - method: command
      config:
        command: ollama --version
        expected_pattern: "^ollama version is"  # No "Warning:" prefix
      stability_threshold: 2
      cache_ttl_secs: 60  # Service can restart
    
    # Rule 3: Confirm API OPERATIONAL (extra validation)
    - method: http_probe
      config:
        url: http://localhost:11434/api/tags
        expected_status: 200
        timeout_ms: 2000
      stability_threshold: 2
      cache_ttl_secs: 60
```

**Interpretation**:
- Rule 1 PASS only → Installed but not running (vitality: DORMANT)
- Rule 1+2 PASS → Running (vitality: THRIVING)
- Rule 1+2+3 PASS → Fully operational (vitality: THRIVING, API confirmed)

---

## Code Implementation

### Rust Structs

```rust
/// OS-specific detection rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsDetectionRules {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub windows: Option<Vec<DetectionRule>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linux: Option<Vec<DetectionRule>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub macos: Option<Vec<DetectionRule>>,
}

impl OsDetectionRules {
    /// Get detection rules for current OS
    pub fn get_current_os_rules(&self) -> Vec<DetectionRule> {
        #[cfg(target_os = "windows")]
        return self.windows.clone().unwrap_or_default();
        
        #[cfg(target_os = "linux")]
        return self.linux.clone().unwrap_or_default();
        
        #[cfg(target_os = "macos")]
        return self.macos.clone().unwrap_or_default();
    }
}

/// Command detection (simplified - no more OS-specific fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDetection {
    pub command: String,  // Just one command per OS-grouped rule
    pub expected_pattern: Option<String>,
    pub expected_exit_code: Option<i32>,
}
```

### Usage

```rust
// Load manifest
let manifest: OfferingManifest = load_manifest("ollama.adopted.yaml")?;

// Get rules for current OS (compile-time selection)
let rules = match &manifest.detection {
    Some(os_rules) => os_rules.get_current_os_rules(),
    None => Vec::new(),
};

// Execute detection
for rule in rules {
    let result = detect_by_method(&rule).await?;
    if result.detected {
        return Ok(result);
    }
}
```

---

## Migration from v1.0

### Old Structure (v1.0)

```yaml
detection:
  - method: command
    config:
      command_windows: ollama --version
      command_linux: ollama --version
      command_macos: ollama --version
      expected_pattern: "version ([0-9]+\\.[0-9]+\\.[0-9]+)"
```

### New Structure (v2.0)

```yaml
detection:
  windows:
    - method: command
      config:
        command: ollama --version
        expected_pattern: "version ([0-9]+\\.[0-9]+\\.[0-9]+)"
  
  linux:
    - method: command
      config:
        command: ollama --version
        expected_pattern: "version ([0-9]+\\.[0-9]+\\.[0-9]+)"
  
  macos:
    - method: command
      config:
        command: ollama --version
        expected_pattern: "version ([0-9]+\\.[0-9]+\\.[0-9]+)"
```

### Migration Script

```python
# Convert old manifests to new structure
import yaml

def migrate_detection(old_detection):
    """Convert v1.0 detection to v2.0 OS-grouped structure"""
    new_detection = {}
    
    for rule in old_detection:
        config = rule.get('config', {})
        
        # Extract OS-specific commands
        for os_name in ['windows', 'linux', 'macos']:
            cmd_key = f'command_{os_name}'
            if cmd_key in config:
                if os_name not in new_detection:
                    new_detection[os_name] = []
                
                new_rule = {
                    'method': rule['method'],
                    'config': {
                        'command': config[cmd_key],
                        **{k: v for k, v in config.items() 
                           if not k.startswith('command_') and k != 'command'}
                    }
                }
                
                # Copy optional fields
                for field in ['stability_threshold', 'cache_ttl_secs']:
                    if field in rule:
                        new_rule[field] = rule[field]
                
                new_detection[os_name].append(new_rule)
        
        # Handle fallback command
        if 'command' in config and not any(f'command_{os}' in config for os in ['windows', 'linux', 'macos']):
            # Universal command - copy to all OSes
            for os_name in ['windows', 'linux', 'macos']:
                if os_name not in new_detection:
                    new_detection[os_name] = []
                
                new_rule = rule.copy()
                new_rule['config'] = {
                    'command': config['command'],
                    **{k: v for k, v in config.items() if k != 'command'}
                }
                new_detection[os_name].append(new_rule)
    
    return new_detection

# Example usage
with open('ollama.adopted.yaml.old', 'r') as f:
    old_manifest = yaml.safe_load(f)

old_manifest['detection'] = migrate_detection(old_manifest['detection'])

with open('ollama.adopted.yaml', 'w') as f:
    yaml.dump(old_manifest, f, default_flow_style=False, sort_keys=False)
```

---

## Best Practices

### 1. Avoid Duplication (If Commands Are Identical)

If all OSes use the same command, just copy it to all three:

```yaml
detection:
  windows:
    - method: command
      config:
        command: myservice --version
        expected_pattern: "version ([0-9.]+)"
  
  linux:
    - method: command
      config:
        command: myservice --version
        expected_pattern: "version ([0-9.]+)"
  
  macos:
    - method: command
      config:
        command: myservice --version
        expected_pattern: "version ([0-9.]+)"
```

> **Future**: We may add YAML anchors/aliases for shared configs, but explicit is better for clarity.

### 2. Layer Detection (Installation → Running → Operational)

Use multiple rules to distinguish service states:

```yaml
windows:
  # Layer 1: Is it installed?
  - method: command
    config:
      command: myservice --version
      expected_exit_code: 0
    cache_ttl_secs: 3600
  
  # Layer 2: Is it running?
  - method: command
    config:
      command: powershell -Command "Get-Process myservice"
      expected_exit_code: 0
    cache_ttl_secs: 60
  
  # Layer 3: Is API responding?
  - method: http_probe
    config:
      url: http://localhost:8080/health
      expected_status: 200
    cache_ttl_secs: 30
```

### 3. Use Appropriate Cache TTLs

- **Installation checks**: Long TTL (1 hour+) - binaries don't change often
- **Running checks**: Medium TTL (1 minute) - services can restart
- **API probes**: Short TTL (30 seconds) - real-time health

### 4. Handle Edge Cases

```yaml
windows:
  # Ollama may be installed but not running
  - method: command
    config:
      command: ollama --version
      # Matches BOTH:
      # - "ollama version is 0.15.0" (running)
      # - "client version is 0.15.0" (not running)
      expected_pattern: "version is ([0-9]+\\.[0-9]+\\.[0-9]+)"
```

---

## Troubleshooting

### Issue: "No detection rules configured"

**Cause**: Manifest has no detection rules for current OS

```yaml
detection:
  linux:  # Only Linux defined
    - method: command
      config:
        command: myservice --version
```

**Solution**: Add rules for all target OSes (Windows, Linux, macOS)

### Issue: Commands work manually but fail in Moss

**Cause**: PATH differences between user shell and Moss daemon

**Solution**: Use absolute paths or ensure Moss inherits correct PATH

```yaml
windows:
  - method: command
    config:
      # ❌ May fail if ollama not in Moss's PATH
      command: ollama --version
      
      # ✅ Use absolute path
      command: C:\\Program Files\\Ollama\\ollama.exe --version
      
      # ✅ Or use where.exe to find it
      command: powershell -Command "(Get-Command ollama).Source"
```

---

## Related Documentation

- [Offering Modes](../../decisions/OFFER-0001-taxonomy.md) - Managed/Adopted/Borrowed taxonomy
- [Ollama Detection States](ollama-detection-states.md) - Ollama-specific detection examples
- [Windows Docker Adoption](../specs/windows-docker-adoption-spec.md) - Platform-specific behaviors
- [ARCHITECTURE-REFERENCE.md](../ARCHITECTURE-REFERENCE.md) - Core architectural rules

---

## Summary

**v2.0 Changes**:
- ✅ Detection structure: `detection.{os}` instead of flat list
- ✅ CommandDetection: Single `command` field (no `command_windows` etc.)
- ✅ Runtime: Moss loads only current OS rules (compile-time selection)
- ✅ Maintainability: Easy to add OS-specific detection strategies

**Migration Effort**: Low (simple YAML restructuring, script provided)

**Benefits**:
- Cleaner manifests (no redundant OS-specific fields)
- Faster detection (no runtime OS checks)
- Better separation of concerns (each OS is independent)
