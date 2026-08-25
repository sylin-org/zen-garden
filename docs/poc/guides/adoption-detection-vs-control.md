# Adopted Offerings: Detection vs Control

**Updated**: 2026-01-25

---

## The Problem

When adopting native services (e.g., Ollama installed directly on the host), we face two questions:

1. **Detection**: How do we know it's installed?
2. **Control**: Should Moss be able to start/stop it?

These are separate concerns with different answers.

---

## Detection Philosophy

### Detect INSTALLATION, Not Just RUNNING

**Bad approach** (only detects running services):
```yaml
detection:
  - method: http_probe
    config:
      url: http://localhost:11434/api/tags
```
❌ Problem: Fails if service is installed but not running

**Good approach** (detects installation):
```yaml
detection:
  - method: command
    config:
      command: ollama --version
      expected_pattern: "version"
```
✅ Benefit: Works whether service is running or not

### Layered Detection

Use **multiple detection rules** for different scenarios:

```yaml
detection:
  # Layer 1: Is it installed?
  - method: command
    config:
      command: ollama --version
    cache_ttl_secs: 3600  # Long cache (binary doesn't change often)

  # Layer 2: Is it running?
  - method: http_probe
    config:
      url: http://localhost:11434/api/tags
    cache_ttl_secs: 60    # Short cache (service state changes)
```

**Moss behavior:**
- If **Layer 1 passes**: Service is adopted (but marked as "Offline" if Layer 2 fails)
- If **both pass**: Service is adopted and marked as "Healthy"
- If **both fail**: Service is not adopted

---

## Control Philosophy

### Three Control Levels

The `control.level` field determines Moss's authority:

#### 1. `announce` - Zero Control
```yaml
control:
  level: announce
```
- Moss only announces the service exists
- No health monitoring
- No start/stop commands
- **Use case**: Services managed by external tools

#### 2. `monitor` - Read-Only (DEFAULT)
```yaml
control:
  level: monitor
  health_check_url: http://localhost:11434/api/tags
```
- Moss checks if service is healthy
- Reports status to garden
- **Cannot** start/stop service
- **Use case**: User-managed services, production systems

#### 3. `full` - Complete Control
```yaml
control:
  level: full
  start_command: ollama serve &
  stop_command: pkill ollama
  restart_command: pkill ollama && ollama serve &
  health_check_url: http://localhost:11434/api/tags
```
- Moss can start/stop/restart service
- Monitors health continuously
- Can auto-restart on failure
- **Use case**: Dev environments, automated recovery

---

## When Should Moss Control Services?

### ✅ Give Moss FULL Control When:

1. **Development/Testing**: You want automatic service management
2. **Single-tenant systems**: No one else depends on the service
3. **Disposable environments**: Service can be restarted freely
4. **Delegated management**: You explicitly want Moss to handle lifecycle

**Example: Local dev machine running Ollama for experiments**
```yaml
control:
  level: full  # Moss can restart if it crashes
  start_command: ollama serve &
```

### ⚠️ Use MONITOR Mode When:

1. **Shared systems**: Multiple users/applications depend on service
2. **Production**: Service has specific startup procedures
3. **External management**: Service managed by systemd/Docker/Kubernetes
4. **Security concerns**: Don't want automated restarts

**Example: Ollama on a shared GPU server**
```yaml
control:
  level: monitor  # Just observe, don't touch
```

### 🚫 Use ANNOUNCE Mode When:

1. **Pure discovery**: You just want to know it exists
2. **External health monitoring**: Another tool handles health checks
3. **Network services**: Service on different host (borrowed offerings)

**Example: Ollama running on another machine**
```yaml
control:
  level: announce  # Just make it discoverable
```

---

## Practical Examples

### Example 1: Ollama on Windows Dev Machine

**Scenario**: Solo developer, local machine, wants automatic management

```yaml
name: ollama
modes: [adopted]
detection:
  - method: command
    config:
      command_windows: ollama --version
      expected_pattern: "version"

control:
  level: full
  start_command: powershell -Command "Start-Process ollama -ArgumentList 'serve' -WindowStyle Hidden"
  stop_command: powershell -Command "Stop-Process -Name 'ollama' -Force"
  health_check_url: http://localhost:11434/api/tags
```

**Result**: Moss can restart Ollama if it crashes

### Example 2: Ollama on Linux Server (Multi-User)

**Scenario**: Shared server, systemd-managed, don't want conflicts

```yaml
name: ollama
modes: [adopted]
detection:
  - method: command
    config:
      command_linux: systemctl is-active ollama  # Check systemd status
      expected_pattern: "active"
      expected_exit_code: 0

control:
  level: monitor  # Read-only
  health_check_url: http://localhost:11434/api/tags
```

**Result**: Moss announces service, reports health, but never touches it

### Example 3: Ollama with Auto-Start on Failure

**Scenario**: Dev machine, want automatic recovery

```yaml
name: ollama
modes: [adopted]
detection:
  - method: command
    config:
      command: ollama --version
  - method: http_probe
    config:
      url: http://localhost:11434/api/tags

control:
  level: full
  start_command: ollama serve &
  health_check_url: http://localhost:11434/api/tags
  # Optional: auto_restart_on_failure: true
```

**Result**: If HTTP probe fails but binary exists, Moss can start it

---

## OS-Specific Start Commands

### Windows

**Background process** (recommended):
```yaml
start_command: powershell -Command "Start-Process -FilePath 'ollama' -ArgumentList 'serve' -WindowStyle Hidden"
```

**Foreground process**:
```yaml
start_command: ollama serve
```

**Check if running**:
```yaml
command_windows: powershell -Command "Get-Process -Name 'ollama' -ErrorAction SilentlyContinue | Select-Object -First 1"
```

### Linux

**Systemd-managed** (best):
```yaml
start_command: systemctl start ollama
stop_command: systemctl stop ollama
restart_command: systemctl restart ollama
```

**Direct execution**:
```yaml
start_command: nohup ollama serve > /var/log/ollama.log 2>&1 &
stop_command: pkill -f "ollama serve"
```

### macOS

**LaunchDaemon-managed** (best):
```yaml
start_command: launchctl start com.ollama.service
stop_command: launchctl stop com.ollama.service
```

**Direct execution**:
```yaml
start_command: nohup ollama serve > ~/Library/Logs/ollama.log 2>&1 &
stop_command: pkill -f "ollama serve"
```

---

## Best Practices

### 1. Default to Monitor Mode

Unless you have a specific reason for `full` control, start with `monitor`:

```yaml
control:
  level: monitor  # Safe default
```

### 2. Test Commands Before Committing

Verify start/stop commands work manually:

```powershell
# Windows
Start-Process ollama -ArgumentList 'serve' -WindowStyle Hidden
Stop-Process -Name 'ollama' -Force

# Linux
systemctl start ollama
systemctl stop ollama
```

### 3. Use Health Checks

Always provide a health check URL, even for `monitor` mode:

```yaml
control:
  level: monitor
  health_check_url: http://localhost:11434/api/tags
```

### 4. Document Installation

Include installation instructions in manifest comments:

```yaml
# Installation:
#   Windows: winget install Ollama.Ollama
#   Linux:   curl https://ollama.com/install.sh | sh
```

### 5. Handle Ports Gracefully

If service might run on non-default port, use detection config:

```yaml
detection:
  - method: http_probe
    config:
      # Try default port first
      url: http://localhost:11434/api/tags
  - method: http_probe
    config:
      # Fallback to alternate port
      url: http://localhost:11435/api/tags
```

---

## Summary

| Question | Answer |
|----------|--------|
| **Should we detect installed-but-not-running services?** | YES - Use command detection (`ollama --version`) |
| **Should Moss start services automatically?** | DEPENDS - Use control levels |
| **What's the safe default?** | `monitor` mode (read-only) |
| **When should Moss have full control?** | Dev environments, single-user systems, explicit delegation |
| **When should Moss be read-only?** | Production, shared systems, systemd-managed services |

**Philosophy**: Detect presence generously, control conservatively.
