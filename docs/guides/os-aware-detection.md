# OS-Aware Detection Commands

**Related**: [Offering Modes Refactoring Plan](../archive/proposals/offering-modes-refactoring-plan.md)

---

## Overview

Detection commands in adopted offerings now support **OS-specific variants** to handle platform differences in:
- Shell syntax (PowerShell vs bash vs zsh)
- Service management (systemd vs launchd vs Windows services)
- File paths and conventions
- Command availability

---

## Manifest Schema

### Command Detection with OS Variants

```yaml
detection:
  - method: command
    config:
      # OS-specific commands (checked first)
      command_windows: powershell -Command "Get-Service -Name 'MyService'"
      command_linux: systemctl is-active myservice
      command_macos: launchctl list | grep myservice
      
      # Fallback command (used if no OS-specific match)
      command: myservice --version
      
      # Common validation
      expected_pattern: "version \\d+"
      expected_exit_code: 0
    
    stability_threshold: 2
    cache_ttl_secs: 300
```

### Field Priority

At runtime, Moss selects the command in this order:

1. **`command_windows`** - Used on Windows (if present)
2. **`command_linux`** - Used on Linux (if present)
3. **`command_macos`** - Used on macOS (if present)
4. **`command`** - Fallback for any OS

---

## Common Patterns

### Pattern 1: Service Status Check

**Use Case**: Detect if a system service is running

```yaml
detection:
  - method: command
    config:
      command_windows: |
        powershell -Command "Get-Service -Name 'Ollama' -ErrorAction SilentlyContinue | 
        Select-Object -ExpandProperty Status"
      command_linux: systemctl is-active ollama
      command_macos: launchctl list | grep com.ollama.service
      expected_pattern: "running|active"
```

### Pattern 2: Config File Location

**Use Case**: Read configuration from OS-specific paths

```yaml
detection:
  - method: command
    config:
      command_windows: |
        powershell -Command "Get-Content $env:USERPROFILE\\.ollama\\config.json | 
        ConvertFrom-Json | Select-Object -ExpandProperty port"
      command_linux: cat ~/.ollama/config.json | jq -r .port
      command_macos: cat ~/.ollama/config.json | jq -r .port
      expected_pattern: "\\d+"
```

### Pattern 3: Version Detection

**Use Case**: Extract version from different output formats

```yaml
detection:
  - method: command
    config:
      command_windows: powershell -Command "ollama --version"
      command_linux: ollama --version
      command_macos: ollama --version
      # All OSes produce similar output, so same pattern works
      expected_pattern: "version ([0-9]+\\.[0-9]+\\.[0-9]+)"
```

### Pattern 4: Process Detection

**Use Case**: Check if process is running (when no service manager)

```yaml
detection:
  - method: command
    config:
      command_windows: |
        powershell -Command "Get-Process -Name 'ollama' -ErrorAction SilentlyContinue | 
        Select-Object -First 1 -ExpandProperty Name"
      command_linux: pgrep -x ollama
      command_macos: pgrep -x ollama
      expected_exit_code: 0
```

---

## OS-Specific Considerations

### Windows

**Shell**: PowerShell (default), not CMD
- Use `powershell -Command "..."` for inline commands
- Escape quotes: `\"`
- Multi-line: Use `|` in YAML for readability
- Error suppression: `-ErrorAction SilentlyContinue`

**Paths**:
- User home: `$env:USERPROFILE` (e.g., `C:\Users\username`)
- AppData: `$env:LOCALAPPDATA` or `$env:APPDATA`
- Program Files: `$env:PROGRAMFILES`

**Services**:
```powershell
# Check service status
Get-Service -Name 'ServiceName' -ErrorAction SilentlyContinue

# Get service properties
Get-Service -Name 'ServiceName' | Select-Object Status, DisplayName
```

**Common Commands**:
- List processes: `Get-Process -Name 'name'`
- Read file: `Get-Content path\to\file.txt`
- Parse JSON: `ConvertFrom-Json`
- Check path: `Test-Path path\to\file`

### Linux

**Shell**: bash (most common), sh (POSIX)
- Use POSIX-compliant commands when possible
- Avoid bashisms if targeting minimal containers

**Paths**:
- User home: `~` or `$HOME` (e.g., `/home/username`)
- Config: `/etc/servicename/` or `~/.config/servicename/`
- System services: `/etc/systemd/system/`

**Services** (systemd):
```bash
# Check if service is active
systemctl is-active servicename

# Get service status
systemctl status servicename

# Check if service exists
systemctl list-units | grep servicename
```

**Common Commands**:
- List processes: `pgrep -x name` or `ps aux | grep name`
- Read file: `cat /path/to/file`
- Parse JSON: `jq` (if installed)
- Check path: `test -f /path/to/file`

### macOS

**Shell**: zsh (default on modern macOS), bash (older)
- Most bash commands work
- Use POSIX-compliant when possible

**Paths**:
- User home: `~` or `$HOME` (e.g., `/Users/username`)
- Config: `~/Library/Application Support/ServiceName/`
- System services: `/Library/LaunchDaemons/` or `~/Library/LaunchAgents/`

**Services** (launchd):
```bash
# List services
launchctl list | grep servicename

# Check service status
launchctl list com.company.servicename

# Get service info
launchctl print gui/$(id -u)/com.company.servicename
```

**Common Commands**:
- List processes: `pgrep -x name` or `ps aux | grep name`
- Read file: `cat /path/to/file`
- Parse JSON: `jq` (if installed via Homebrew)
- Check path: `test -f /path/to/file`

---

## Best Practices

### 1. Provide Fallback Commands

Always include a generic `command` field that works across OSes when possible:

```yaml
config:
  command_windows: powershell -Command "ollama --version"
  command_linux: ollama --version
  command_macos: ollama --version
  # Fallback (works if ollama is in PATH on any OS)
  command: ollama --version
  expected_pattern: "version"
```

### 2. Use HTTP Probes When Possible

HTTP probes are OS-agnostic and more reliable:

```yaml
- method: http_probe
  config:
    url: http://localhost:11434/api/health
    expected_status: 200
```

### 3. Handle Missing Commands Gracefully

Detection failures are logged but don't crash Moss:
- Use `expected_exit_code: 0` to reject failures
- Use stability thresholds to avoid flapping
- Cache results to reduce detection overhead

### 4. Test on Multiple Platforms

Before committing a manifest:
- ✅ Test on Windows (if `command_windows` is used)
- ✅ Test on Linux (if `command_linux` is used)
- ✅ Test on macOS (if `command_macos` is used)
- ✅ Test fallback (remove OS-specific commands temporarily)

### 5. Document OS Requirements

In manifest comments, note:
- Which OSes are supported
- Required dependencies (e.g., `jq`, `curl`)
- Installation instructions per OS

---

## Implementation Details

### Runtime OS Detection

Moss uses Rust's built-in OS detection:

```rust
impl CommandDetection {
    pub fn get_command(&self) -> Option<String> {
        // Try OS-specific command first
        #[cfg(target_os = "windows")]
        if let Some(cmd) = &self.command_windows {
            return Some(cmd.clone());
        }

        #[cfg(target_os = "linux")]
        if let Some(cmd) = &self.command_linux {
            return Some(cmd.clone());
        }

        #[cfg(target_os = "macos")]
        if let Some(cmd) = &self.command_macos {
            return Some(cmd.clone());
        }

        // Fall back to generic command
        self.command.clone()
    }
}
```

### Compilation vs Runtime

- **Compile-time**: Moss binary is built for specific OS (`#[cfg(target_os)]`)
- **Runtime**: No additional OS checks needed (already baked in)
- **Cross-platform**: Single manifest works everywhere

---

## Examples

### Minimal (Single Command)

Works when command is identical across OSes:

```yaml
detection:
  - method: command
    config:
      command: ollama --version
      expected_pattern: "version"
```

### Simple (Two Variants)

Common pattern (Windows vs Unix-like):

```yaml
detection:
  - method: command
    config:
      command_windows: powershell -Command "ollama --version"
      command: ollama --version  # Works on Linux/macOS
      expected_pattern: "version"
```

### Complete (All OSes)

Maximum compatibility:

```yaml
detection:
  - method: command
    config:
      command_windows: powershell -Command "Get-Service -Name 'Ollama'"
      command_linux: systemctl is-active ollama
      command_macos: launchctl list | grep ollama
      command: ollama --version  # Fallback
      expected_pattern: "active|running|version"
      expected_exit_code: 0
    stability_threshold: 2
    cache_ttl_secs: 300
```

---

## Troubleshooting

### Detection Fails on Windows

**Symptom**: `No command specified for current OS`

**Solution**: Add `command_windows` field:
```yaml
command_windows: powershell -Command "your-command"
```

### PowerShell Syntax Errors

**Symptom**: `Failed to execute command: exit code 1`

**Solution**: Check PowerShell syntax:
```powershell
# Test directly in PowerShell first
powershell -Command "Get-Service -Name 'Ollama'"
```

### Command Not in PATH

**Symptom**: Detection fails with "command not found"

**Solution**: Use absolute paths:
```yaml
command_windows: C:\Program Files\Ollama\ollama.exe --version
command_linux: /usr/local/bin/ollama --version
```

### Pattern Not Matching

**Symptom**: Detection fails despite command succeeding

**Solution**: Debug with `expected_exit_code` only first:
```yaml
config:
  command: ollama --version
  # Temporarily remove pattern
  # expected_pattern: "..."
  expected_exit_code: 0
```

Then check logs to see actual output and refine pattern.

---

## Migration Guide

### Old Manifest (Pre OS-Aware)

```yaml
detection:
  - method: command
    config:
      command: ollama --version
```

### New Manifest (OS-Aware)

```yaml
detection:
  - method: command
    config:
      command_windows: powershell -Command "ollama --version"
      command_linux: ollama --version
      command_macos: ollama --version
      command: ollama --version  # Fallback (backward compatible)
```

**Backward Compatibility**: Old manifests still work! The `command` field alone is sufficient if the command works cross-platform.

---

## References

- [Offering Manifest Schema](../../src/common/src/manifests/offering.rs)
- [Detection Implementation](../../src/moss/src/infra/detection/mod.rs)
- [Ollama Adopted Example](../../src/moss/embedded/manifests/sw/ai/ollama.adopted.example.yaml)
