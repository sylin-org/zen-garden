# Adapter Guide

**Purpose:** Understand and use Moss adapters to extend Stone capabilities  
**Audience:** Operators and developers

---

## What Are Adapters?

Adapters are services that run on Stones to extend Moss capabilities beyond core service management. They provide:

- **Physical presence indicators** (audio, LEDs, displays)
- **Real-time monitoring dashboards** (OLED screens, web interfaces)
- **Custom automation** (webhooks, scripts, integrations)

Adapters subscribe to Stone presence events (service starts, container failures, firmware updates) and take action based on those events.

---

## Current Adapters

### Cricket (Audio Adapter)

**Port:** 7187  
**Purpose:** Sonify infrastructure with 4-channel audio mixer  
**Status:** ✅ Active

Cricket provides audio feedback for Stone operations using a tune system that maps events to sounds:

- **Foreground:** Alerts and critical events (service failures, errors)
- **Midground:** Notifications (service starts, restarts)
- **Ambient:** Background atmosphere (continuous loops, chimes)
- **Background:** Persistent ambient soundscapes

**Commands:**
```bash
# Select active tune
garden-rake hey tell cricket select zen-tech

# Adjust volume (0-100)
garden-rake hey tell cricket volume 75

# List available tunes
garden-rake hey tell cricket list

# Show tune configuration
garden-rake hey tell cricket show zen-tech

# Test play specific event
garden-rake hey tell cricket play stone-online

# Stop all channels
garden-rake hey tell cricket stop
```

**Tunes:**
- `zen-tech`: Digital infrastructure sounds (180 CC0 samples)
- `mr-robot`: Cyberpunk aesthetic (planned)
- `lo-fi-ops`: Chill ambient operations (planned)

**Reference:** [CRICKET-0001-audio-adapter-spec.md](../decisions/CRICKET-0001-audio-adapter-spec.md)

---

### Firefly (LED Adapter) - Planned

**Port:** 7188 (when implemented)  
**Purpose:** Visual presence indicators via programmable LEDs  
**Status:** 🔜 Planned

Firefly will control RGB LEDs to show Stone status:
- Green: Healthy
- Yellow: Degraded
- Red: Critical
- Blue pulse: Update available
- White flash: Service event

---

### OLED (Display Adapter) - Planned

**Port:** 7189 (when implemented)  
**Purpose:** Real-time metrics on small OLED screens  
**Status:** 🔜 Planned

OLED will render Stone metrics on 128x64 OLED displays:
- CPU/RAM/disk usage
- Active services
- Network activity
- Recent events

---

## Using Adapters

### List Available Adapters

```bash
garden-rake hey list
```

**Output:**
```
Available Adapters:
  cricket    Audio Adapter v0.1.0    ✅ Running (PID 12345)
  firefly    LED Control v0.1.0      ⏸ Stopped
```

---

### Get Adapter Help

```bash
garden-rake hey <adapter>
```

**Example:**
```bash
$ garden-rake hey cricket

Cricket Audio Adapter v0.1.0
Provides 4-channel audio mixer and tune system for Stone presence events.

Commands:
  select <tune>        Switch active tune
  volume <0-100>       Set master volume
  list                 List available tunes
  show <tune>          Show tune configuration
  play <event>         Test play specific event
  stop                 Stop all channels

Examples:
  garden-rake hey tell cricket play stone-online
  garden-rake hey tell cricket volume 75
  garden-rake hey tell cricket select zen-tech

Event Subscriptions:
  stone-online, stone-offline, service-started, service-stopped,
  service-restarted, container-failed, update-available, update-applied,
  firmware-update-available, firmware-updated, health-degraded,
  health-recovered, disk-warning, memory-warning, cpu-spike
```

---

### Send Commands

```bash
garden-rake hey tell <adapter> <command> [args...]
```

**Examples:**
```bash
# Cricket audio control
garden-rake hey tell cricket play stone-online
garden-rake hey tell cricket volume 50

# Firefly LED control (when available)
garden-rake hey tell firefly set presence green
garden-rake hey tell firefly blink alert

# OLED display control (when available)
garden-rake hey tell oled show metrics
```

---

## How Adapters Work

### Architecture

```
┌─────────────┐
│    Rake     │ garden-rake hey tell cricket play stone-online
└──────┬──────┘
       │ HTTP POST /api/v1/stone/adapters/cricket/command
       │ {"args": ["play", "stone-online"]}
       ▼
┌─────────────┐
│    Moss     │ Looks up Cricket's port (7187) from ledger
│   (7185)    │ Forwards to http://127.0.0.1:7187/command
└──────┬──────┘
       │ HTTP POST with 5s timeout
       ▼
┌─────────────┐
│   Cricket   │ Executes command
│   (7187)    │ Returns {"success": true, "output": "Playing..."}
└─────────────┘
```

### Port Assignment

Moss maintains a persistent port ledger at `{data_dir}/adapter-ports.json`:

```json
{
  "assignments": {
    "cricket": 7187,
    "firefly": 7188,
    "oled": 7189
  },
  "next_port": 7190
}
```

Ports are assigned incrementally starting from **7187** (base port). The ledger persists across restarts, ensuring adapters always get the same port.

### Discovery Protocol

When Moss starts:

1. Scans `{data_dir}/adapters/` for executables
2. For each adapter:
   - Gets/assigns port from ledger
   - Runs `{adapter} --dump-commands --port {port}` to get manifest
   - Caches manifest (commands, parameters, examples)
   - Starts adapter: `{adapter} --stone http://localhost:7185 --port {port}`
3. Adapter binds HTTP server on assigned port
4. Adapter subscribes to presence SSE: `GET http://localhost:7185/api/v1/stone/presence/stream`

### Event Streaming

Adapters receive Stone events via Server-Sent Events (SSE):

```
GET /api/v1/stone/presence/stream
Accept: text/event-stream

event: stone-online
data: {"timestamp": "2026-01-26T12:00:00Z", "stone": "stone-crystal-forest"}

event: service-started
data: {"timestamp": "2026-01-26T12:01:00Z", "service": "mongodb", "container": "zen-offering-mongodb"}

event: health-degraded
data: {"timestamp": "2026-01-26T12:05:00Z", "reason": "disk_usage_high", "value": 92}
```

Adapters filter for events they care about and take action.

---

## Installing Adapters

### Manual Installation

1. **Copy adapter executable** to `{data_dir}/adapters/`:
   ```bash
   # Linux
   sudo cp garden-cricket /var/lib/zen-garden/adapters/
   sudo chmod +x /var/lib/zen-garden/adapters/garden-cricket
   
   # Windows
   copy garden-cricket.exe .zen-garden\adapters\
   ```

2. **Restart Moss** to trigger discovery:
   ```bash
   sudo systemctl restart garden-moss
   ```

3. **Verify adapter registered**:
   ```bash
   garden-rake hey list
   ```

### Automatic Installation (Future)

Planned: Adapters distributed as packages with automatic installation via Rake:
```bash
garden-rake adapter install cricket
```

---

## Troubleshooting

### Adapter Not Showing

**Check adapter directory:**
```bash
# Linux
ls -l /var/lib/zen-garden/adapters/

# Windows
dir .zen-garden\adapters\
```

**Check Moss logs:**
```bash
sudo journalctl -u garden-moss -n 50 | grep adapter
```

**Force refresh:**
```bash
curl -X POST http://localhost:7185/api/v1/stone/adapters/refresh
```

---

### Adapter Not Starting

**Check executable permissions:**
```bash
# Linux - must be executable
chmod +x /var/lib/zen-garden/adapters/garden-cricket
```

**Check port conflicts:**
```bash
# View port ledger
cat /var/lib/zen-garden/adapter-ports.json

# Check if port is in use
netstat -tulpn | grep 7187
```

**Manual start for debugging:**
```bash
/var/lib/zen-garden/adapters/garden-cricket \
  --stone http://localhost:7185 \
  --port 7187
```

---

### Command Timeouts

Commands have a 5-second timeout. If an adapter takes longer:

1. **Check adapter health:**
   ```bash
   curl http://localhost:7187/health
   ```

2. **Check adapter logs** (if supported)

3. **Restart adapter:**
   ```bash
   curl -X POST http://localhost:7185/api/v1/stone/adapters/cricket/down
   curl -X POST http://localhost:7185/api/v1/stone/adapters/cricket/up
   ```

---

## Creating Custom Adapters

See [ADAPTER-COMMAND-PROTOCOL.md](../specs/ADAPTER-COMMAND-PROTOCOL.md) for implementation guide.

**Minimal adapter requirements:**

1. **Accept CLI flags:**
   - `--stone <moss-endpoint>` - Moss HTTP API URL
   - `--port <port>` - Assigned port from ledger
   - `--dump-commands` - Output JSON manifest and exit

2. **HTTP command server:**
   - Bind to `127.0.0.1:{port}` (localhost only)
   - POST `/command` endpoint accepting `{"args": ["cmd", "arg1", ...]}`
   - Return `{"success": bool, "output": string}`
   - Respond within 5 seconds

3. **SSE client (optional):**
   - Subscribe to `GET {stone_endpoint}/api/v1/stone/presence/stream`
   - Filter for relevant events
   - Take action based on events

4. **Command manifest:**
   - JSON schema describing commands, parameters, examples
   - Output when invoked with `--dump-commands --port {port}`

---

## Reference

- [ADAPTER-COMMAND-PROTOCOL.md](../specs/ADAPTER-COMMAND-PROTOCOL.md) - Technical protocol specification
- [ADAPTER-SERVICE-REGISTRY.md](../specs/ADAPTER-SERVICE-REGISTRY.md) - Registration and lifecycle
- [HEY-TELL-SYNTAX.md](../specs/HEY-TELL-SYNTAX.md) - Command grammar
- [CRICKET-0001-audio-adapter-spec.md](../decisions/CRICKET-0001-audio-adapter-spec.md) - Cricket design
- [how-to-create-a-tune.md](how-to-create-a-tune.md) - Cricket tune creation
- [ports.md](../reference/ports.md) - Port allocation (7187-7199)
