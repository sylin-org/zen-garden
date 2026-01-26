# Ollama Detection States Reference

**Date**: 2026-01-25  
**Applies to**: Ollama adopted mode on Windows

---

## Command Output Variations

### When Ollama is NOT Running
```
C:\>ollama --version
Warning: could not connect to a running Ollama instance
Warning: client version is 0.15.0
```
**Exit Code**: 0  
**Indicates**: Binary installed, service not running

### When Ollama IS Running
```
C:\>ollama --version
ollama version is 0.15.0
```
**Exit Code**: 0  
**Indicates**: Service operational

---

## Detection Rule Interpretation

| Rule 1 | Rule 2 | Rule 3 | Vitality | Meaning | Action Available |
|--------|--------|--------|----------|---------|------------------|
| ✅ | ❌ | ❌ | **Dormant** | Installed but not running | Can awaken (if `control=full`) |
| ✅ | ✅ | ❌ | **Needs Attention** | Running but API unresponsive | Check network/port |
| ✅ | ✅ | ✅ | **Thriving** | Fully operational | None needed |
| ❌ | ❌ | ❌ | - | Not installed | Not adopted |

**Rule 1**: Pattern `version is ([0-9]+\.[0-9]+\.[0-9]+)` - Matches both outputs  
**Rule 2**: Pattern `^ollama version is` - Only matches when running (no "Warning:")  
**Rule 3**: HTTP probe to `http://localhost:11434/api/tags`

---

## Control Level Behavior

### With `control: monitor` (default)

```yaml
control:
  level: monitor
  health_check_url: http://localhost:11434/api/tags
```

**When Dormant** (Rule 1 only passes):
- ✅ Adopted into registry
- 💤 Reported as "Dormant" (sleeping, not an error)
- ❌ Moss will NOT awaken it
- 💡 User must run: `ollama serve`

**When Thriving** (All rules pass):
- ✅ Reported as "Thriving"
- ✅ Discoverable by other services
- 📊 Vitality monitored continuously

### With `control: full`

```yaml
control:
  level: full
  start_command: powershell -Command "Start-Process ollama -ArgumentList 'serve' -WindowStyle Hidden"
  stop_command: powershell -Command "Stop-Process -Name 'ollama' -Force"
  health_check_url: http://localhost:11434/api/tags
```

**When Dormant** (Rule 1 only passes):
- ✅ Adopted into registry
- 💤 Detected as "Dormant" (ready to awaken)
- ✅ Moss CAN awaken it automatically
- 🌱 Auto-start on next vitality check cycle

**When Thriving** (All rules pass):
- ✅ Fully managed by Moss
- 🔄 Can restart on failure (returns to Thriving)
- 🛑 Can put to sleep on demand (becomes Dormant)

---

## Typical Adoption Flow

### Scenario 1: Ollama Installed, Not Running

```bash
# User installs Ollama but doesn't start it
PS> ollama --version
Warning: could not connect to a running Ollama instance
Warning: client version is 0.15.0

# Moss detects it (auto-adoption or manual)
PS> garden-rake adopt ollama

# Result: Adopted with status "Offline"
# With control=monitor: User must start manually
# With control=full: Moss will start it on next check
```

### Scenario 2: Ollama Already Running

```bash
# User starts Ollama
PS> ollama serve
# (service running in background)

# Check version
PS> ollama --version
ollama version is 0.15.0

# Moss detects it
PS> garden-rake adopt ollama

# Result: Adopted with status "Healthy"
```

### Scenario 3: Ollama Crashes After Adoption

```bash
# Ollama was healthy, then crashes
# Next health check cycle:

Rule 1: ✅ (binary still installed)
Rule 2: ❌ (no "ollama version is" output)
Rule 3: ❌ (HTTP API down)

# Status changes to "Offline"

# With control=monitor: Stays offline until user restarts
# With control=full: Moss executes start_command automatically
```

---

## Version Extraction

Both detection patterns capture the version number:

**Pattern**: `version is ([0-9]+\.[0-9]+\.[0-9]+)`

**Extraction**:
```
Input:  "Warning: client version is 0.15.0"
Regex:  version is (0.15.0)
Output: "0.15.0"

Input:  "ollama version is 0.15.0"
Regex:  version is (0.15.0)
Output: "0.15.0"
```

This version is stored in `AdoptedOfferingInfo.version` and displayed in:
- `garden-rake adopted` output
- `/api/v1/offerings/adopted` response
- Garden topology views

---

## Manual Testing

### Test Installation Detection
```powershell
# Stop Ollama (if running)
Stop-Process -Name ollama -Force -ErrorAction SilentlyContinue

# Test pattern
ollama --version | Select-String "version is ([0-9]+\.[0-9]+\.[0-9]+)"
# Should match: "client version is 0.15.0"
```

### Test Running Detection
```powershell
# Start Ollama
Start-Process ollama -ArgumentList 'serve' -WindowStyle Hidden

# Wait 2 seconds
Start-Sleep -Seconds 2

# Test pattern
ollama --version | Select-String "^ollama version is"
# Should match: "ollama version is 0.15.0"
```

### Test HTTP Probe
```powershell
# With Ollama running
Invoke-RestMethod http://localhost:11434/api/tags
# Should return JSON with models list

# With Ollama stopped
Invoke-RestMethod http://localhost:11434/api/tags
# Should fail with connection error
```

---

## Troubleshooting

### Issue: Rule 1 fails (should always pass if installed)

**Symptom**: `ollama --version` returns non-zero exit code

**Check**:
```powershell
ollama --version
echo $LASTEXITCODE  # Should be 0
```

**Solution**: Reinstall Ollama

### Issue: Rule 2 passes but Rule 3 fails

**Symptom**: Command shows "ollama version is" but HTTP probe fails

**Possible causes**:
1. Service just started (still initializing) - Wait 5-10 seconds
2. Port conflict (something else on 11434) - Check with `netstat -ano | findstr 11434`
3. Firewall blocking localhost HTTP - Check Windows Firewall

**Check**:
```powershell
# Is process running?
Get-Process ollama

# Is port listening?
netstat -ano | findstr "11434"

# Can we connect?
Test-NetConnection -ComputerName localhost -Port 11434
```

### Issue: Moss adopts but shows "Offline" when it's running

**Symptom**: Service is running, API works, but Moss says "Offline"

**Check detection cache**:
- Detection results are cached (60s for Rule 2/3, 3600s for Rule 1)
- Wait for cache expiry or restart Moss

**Force re-detection**:
```powershell
# Unadopt and re-adopt
garden-rake release ollama
garden-rake adopt ollama
```

---

## Summary

**Key Insight**: `ollama --version` output changes based on service state, giving us TWO pieces of information from one command:

1. **Installation presence** - Matches `version is \d+\.\d+\.\d+` in both states
2. **Running state** - Only matches `^ollama version is` when service is up

This elegant detection allows Moss to:
- ✅ Adopt services even when not running
- ✅ Distinguish installed vs running
- ✅ Extract version regardless of state
- ✅ Support both monitor and full control modes

### Vitality Model for Adopted Offerings

Unlike managed offerings (containers), adopted offerings have a **dormancy cycle** - a natural state where the service is installed but not active. This is not an error condition!

**Vitality States**:
- 💚 **Thriving**: Service is running and operational (like a plant in full bloom)
- 💤 **Dormant**: Service is installed but sleeping (like a seed in soil, ready to sprout)
- ⚠️ **Needs Attention**: Service claims to run but API fails (something's wrong)

**Philosophy**:
- **Dormant ≠ Dead**: It's a valid, expected state
- Seeds can be awakened: `control=full` enables auto-start
- Natural cycle: Services can sleep and wake as needed
- No urgency: Dormant doesn't trigger alerts (unlike "Offline" or "Error")

This aligns with the Zen Garden metaphor:
- Not everything needs to be growing all the time
- Some offerings rest during seasons
- Moss tends the garden, not forces it
- Vitality is measured by potential, not just activity
