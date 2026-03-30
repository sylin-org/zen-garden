---
audience: [developer, ai]
doc_type: decision
status: proposed
last_verified: 2026-03-30
---

# DETECT-0001: Process-Based Service Detection

**Date**: 2026-03-30
**Status**: Proposed
**Applies to**: `garden-common`, `garden-moss` (auto-adoption subsystem)
**Depends on**: Process detection catalog (08), manifest analysis (09)

---

## Context

### Current State

The adopted service detection uses shell commands (`powershell -Command`,
`pgrep`, `pip show`, `where`, `which`) and HTTP probes. Testing every
manifest against a live Windows machine running 6 AI services revealed:

**5 of 7 command-based detection rules fail:**

| Service | Command | Failure |
|---------|---------|---------|
| ComfyUI | `comfy --help` | CLI not installed (portable build) |
| whisper.cpp | `where whisper-server.exe` | Binary not in PATH |
| OpenedAI Speech | PowerShell `Get-Process` pipeline | Exit 0 with empty output (false positive) |
| Infinity | `pip show infinity-emb` | System pip, package in venv only |
| LibreTranslate | `pip show libretranslate` | System pip, package in venv only |

**Root causes (systemic, not per-manifest):**
1. PowerShell returns exit 0 for empty pipelines, missing commands, and failed filters
2. `Get-Process` doesn't populate CommandLine (need WMI `Win32_Process`)
3. `cmd /C` adds version banner to stdout (digits match `\d+` patterns)
4. `pip show` checks system Python, not virtualenvs
5. `where`/`which` only check PATH, not actual install locations
6. Per-platform shell syntax creates maintenance burden with no reliability gain

**HTTP probes work** but depend on knowing the correct port. When a
service runs on a non-default port (e.g., OpenedAI Speech on 8001
instead of 8000 due to conflict with whisper.cpp), the probe fails.

### What Actually Works (From Live Investigation)

**WMI/procfs** reliably provides process inventory:
- PID, executable name, full command line, executable path
- Available via `sysinfo` crate (already a dependency), cross-platform
- No shell commands, no exit code ambiguity

**TCP table** maps PIDs to actual listening ports:
- Windows: `GetExtendedTcpTable` or `Get-NetTCPConnection`
- Linux: `/proc/net/tcp`
- No assumed ports — the OS knows what each process is bound to

**Each service has a unique fingerprint** observable in the process
inventory, even when the executable is generic (`python.exe`):

| Service | Executable | Unique Signal in CommandLine |
|---------|-----------|----------------------------|
| Ollama | `ollama` | `serve` |
| ComfyUI | `python` | `main.py` (generic — needs health verify) |
| whisper.cpp | `whisper-server` | unique name |
| OpenedAI Speech | `python` | `speech.py` |
| Infinity | `python` | `start.py` (generic — needs health verify) |
| LibreTranslate | `libretranslate` | unique name |

### Parent-Child Process Chains

Python venv services spawn child processes. The venv python launches
system python, and the **child** holds the listening port:

```
venv/python.exe (PID 32760, no port)
  └→ C:\Python312\python.exe speech.py --port 8001 (PID 32100, port 8001)
```

Both parent and child have `speech.py` in their command line.
Detection must follow the process tree to find the port holder.

### ollama vs ollama-cpu

These are the SAME binary (`ollama.exe serve`) on the SAME port
(11434). The difference is runtime environment
(`CUDA_VISIBLE_DEVICES=""`). Process detection cannot distinguish
them. This is handled post-adoption via capabilities classification.

---

## Decision

Replace shell-command-based detection with a **process inventory +
TCP port mapping + health verification** pipeline.

### Core Principle

Detection is **data matching against a system snapshot**, not command
execution. The system snapshot is captured once per scan cycle via
native APIs. Every offering's detection reads from the same cached
snapshot.

### Detection Pipeline

```
1. PROCESS SNAPSHOT (once per scan cycle, cached)
   sysinfo crate → Vec<ProcessInfo> { pid, name, cmdline, exe_path }
   Cross-platform. Same interface on Windows, Linux, macOS.

2. TCP PORT MAP (once per scan cycle, cached)
   Platform-native API → HashMap<PID, Vec<u16>>
   Windows: GetExtendedTcpTable (iphlpapi)
   Linux: /proc/net/tcp + /proc/{pid}/fd
   Maps every process to its listening ports.

3. SERVICE MATCHING (per offering manifest)
   Match process snapshot against manifest signature:
   - executable name (case-insensitive substring)
   - cmdline_contains (substring match)
   → List of candidate processes with their ports

4. PARENT-CHILD RESOLUTION (for venv services)
   If matched process has no listening port:
   - Check child processes (same cmdline pattern)
   - Use the child's port
   → Matched process with discovered port

5. HEALTH VERIFICATION (per matched candidate)
   HTTP probe on discovered port:
   - path + expected_status + response_contains
   → Confirmed: service identity + operational status + actual port

6. PORT MEMORY (persistence)
   Store discovered port in adopted offering config.
   Next boot: fast-path probe on remembered port before full scan.
```

### Manifest Format

Detection rules become declarative process signatures:

```yaml
detection:
  # Process matching (required)
  process:
    executable: python           # match process name (cross-platform)
    cmdline_contains: speech.py  # match in command line args
    # Platform-specific executable override (optional)
    windows_executable: python.exe
    linux_executable: python3

  # Health verification (required for generic executables, optional for unique ones)
  health:
    path: /health                # HTTP endpoint to probe
    expected_status: 200
    response_contains: '"status"' # body must contain this string

  # Port configuration
  ports:
    default: 8000                # try this first if no port in TCP table
    range: [8000, 8010]          # scan range as last resort
    remember: true               # persist discovered port across restarts
```

No platform-specific sections for detection. No shell commands. No
exit code parsing. The `sysinfo` crate handles platform differences.

### Per-Service Rules (Updated)

**Ollama:**
```yaml
detection:
  process:
    executable: ollama
    cmdline_contains: serve
  health:
    path: /
    response_contains: "Ollama is running"
  ports:
    default: 11434
```
Risk: Low. Unique binary.

**ComfyUI:**
```yaml
detection:
  process:
    executable: python
    cmdline_contains: main.py
  health:
    path: /system_stats
    expected_status: 200
    response_contains: '"system"'
  ports:
    default: 8188
```
Risk: Medium. `main.py` is generic. Health verification mandatory
to confirm identity. `response_contains: '"system"'` matches the
ComfyUI-specific `/system_stats` JSON shape.

**whisper.cpp:**
```yaml
detection:
  process:
    windows_executable: whisper-server.exe
    linux_executable: whisper-server
  health:
    path: /health
    response_contains: '"status"'
  ports:
    default: 8000
    range: [8000, 8010]
    remember: true
```
Risk: Low. Unique binary name.

**OpenedAI Speech:**
```yaml
detection:
  process:
    executable: python
    cmdline_contains: speech.py
  health:
    path: /health
    response_contains: '"status"'
  ports:
    default: 8000
    range: [8000, 8010]
    remember: true
```
Risk: Low. `speech.py` is unique. Parent-child resolution needed
(venv python → system python holds the port).

**Infinity:**
```yaml
detection:
  process:
    executable: python
    cmdline_contains: start.py
  health:
    path: /health
    response_contains: '"unix"'
  ports:
    default: 7997
    range: [7990, 8000]
    remember: true
```
Risk: Medium. `start.py` is generic. Health verification mandatory.
Infinity's `/health` returns `{"unix": timestamp}` — unique signature.

**LibreTranslate:**
```yaml
detection:
  process:
    windows_executable: libretranslate.exe
    linux_executable: libretranslate
  health:
    path: /health
    response_contains: '"status"'
  ports:
    default: 5000
    range: [5000, 5010]
    remember: true
```
Risk: Low. Unique entry point name.

**ollama-cpu:**

Not a separate detection target. Ollama is detected once as `ollama`.
Post-adoption, the capabilities system determines GPU availability.
If no GPU → classified as ollama-cpu equivalent. If GPU → classified
as ollama (GPU). The manifest for ollama-cpu becomes a configuration
variant, not a separate adopted offering.

### Scan Timing

- **Process snapshot + port map**: once per cycle (10-30s configurable)
- **Service matching**: every cycle, against cached snapshot
- **Health verification**: on first detection and when process reappears
  after absence. Not every cycle for confirmed services.
- **Re-verification**: confirmed services re-probed at longer interval
  (5 minutes) or when process disappears from snapshot.
- **Manually launched services**: detected within one scan cycle.

### Two-Collection Integration

Works with the adopted offerings two-collection architecture:
- **candidates pool**: persisted adopted configs (cold storage)
- **active pool**: detection-confirmed services (visible in topology)

Pipeline result feeds into `promote_adopted()` / `demote_adopted()`:
- Detection succeeds → promote candidate to active pool
- Detection fails for active offering → demote to candidates
- New service detected (not in candidates) → create and adopt

---

## Implementation Plan

### Phase 1: Process Inventory (`garden-common`)

New module: `detection::inventory`

```rust
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,          // executable name
    pub cmdline: String,       // full command line
    pub exe_path: String,      // executable path
    pub listening_ports: Vec<u16>,
}

pub struct SystemSnapshot {
    processes: Vec<ProcessInfo>,
    captured_at: Instant,
}

impl SystemSnapshot {
    /// Capture process list + TCP port map.
    /// Uses sysinfo crate for processes, platform API for ports.
    pub fn capture() -> Self;

    /// Find processes matching a signature.
    pub fn find(&self, sig: &ProcessSignature) -> Vec<&ProcessInfo>;
}
```

Platform-specific port mapping:
- `inventory::windows` — `GetExtendedTcpTable` via `windows` crate
- `inventory::linux` — parse `/proc/net/tcp`

### Phase 2: Service Matcher (`garden-common`)

New module: `detection::matcher`

```rust
pub struct ProcessSignature {
    pub executable: String,
    pub windows_executable: Option<String>,
    pub linux_executable: Option<String>,
    pub cmdline_contains: Option<String>,
}

pub struct HealthCheck {
    pub path: String,
    pub expected_status: u16,
    pub response_contains: Option<String>,
}

pub struct PortConfig {
    pub default: u16,
    pub range: Option<(u16, u16)>,
    pub remember: bool,
}

pub struct DetectionMatch {
    pub pid: u32,
    pub port: u16,
    pub health_verified: bool,
}
```

### Phase 3: Detection Pipeline (`garden-common`)

New module: `detection::pipeline`

```rust
pub struct DetectionPipeline {
    snapshot: Arc<RwLock<SystemSnapshot>>,
    refresh_interval: Duration,
}

impl DetectionPipeline {
    /// Refresh the system snapshot (call once per scan cycle).
    pub async fn refresh(&self);

    /// Detect a service using its manifest signature.
    pub async fn detect(
        &self,
        process_sig: &ProcessSignature,
        health: Option<&HealthCheck>,
        ports: &PortConfig,
        remembered_port: Option<u16>,
    ) -> DetectionResult;
}

pub struct DetectionResult {
    pub detected: bool,
    pub port: Option<u16>,
    pub pid: Option<u32>,
    pub details: String,
}
```

### Phase 4: Manifest Types (`garden-common`)

Extend `garden_common::manifests` with new detection format:

```rust
pub struct ProcessDetection {
    pub executable: String,
    pub windows_executable: Option<String>,
    pub linux_executable: Option<String>,
    pub cmdline_contains: Option<String>,
}

pub struct HealthVerification {
    pub path: String,
    pub expected_status: u16,
    pub response_contains: Option<String>,
}

pub struct PortDetectionConfig {
    pub default: u16,
    pub range: Option<(u16, u16)>,
    pub remember: bool,
}
```

Deserialized from the new YAML `detection.process` / `detection.health`
/ `detection.ports` sections. Coexists with existing `detection.windows`
/ `detection.linux` sections for backward compatibility.

### Phase 5: Moss Integration

- `DetectionOrchestrator::detect()` checks for new-format detection
  first. Falls back to old command-based detection if no process
  signature is defined.
- Auto-adoption task uses `DetectionPipeline::refresh()` once per
  cycle, then `detect()` per offering.
- Port memory integrated into persisted offering config
  (`offering.location.port`).

### Phase 6: Manifest Migration

Update all 7 adopted manifests:
- Replace `detection.windows/linux/macos` command sections with
  `detection.process` + `detection.health` + `detection.ports`
- Keep old sections commented out during transition
- Remove after validation on both Windows and Linux stones

### Phase 7: ollama-cpu Consolidation

- Remove `ollama-cpu.adopted.yaml` as separate detection target
- Ollama detection produces a single offering
- Post-adoption GPU capability check classifies as GPU or CPU-only
- Configuration differences (env vars, memory limits) applied as
  offering attributes, not separate manifests

---

## Consequences

### Positive

- No shell command execution for detection — deterministic behavior
- Cross-platform with single manifest section
- Process snapshot shared across all offerings — efficient
- Port discovered from TCP table — handles non-default ports
- Manually launched services detected within one scan cycle
- Parent-child process resolution handles venv services
- Eliminates all 5 known false positive/negative detection failures

### Negative

- Platform-specific code for TCP table enumeration (2 implementations)
- `sysinfo` refresh has overhead on large process tables (~5-10ms)
- Generic cmdline patterns (`start.py`, `main.py`) require health
  verification — detection is two-step, not instant
- ollama-cpu consolidation changes the offering model

### Neutral

- Old `command` and `http_probe` methods remain for backward compat
- Manifest format change is additive (new sections alongside existing)
- Stability threshold still applies (consecutive successful detections)
