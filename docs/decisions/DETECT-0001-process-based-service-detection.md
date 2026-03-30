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

---

## Context

The current adopted service detection uses shell commands (`powershell
-Command "..."`, `pgrep`, `pip show`) and HTTP probes to determine if a
service is installed and running. This approach has systemic reliability
issues:

1. **PowerShell returns exit code 0 for everything** — empty pipelines,
   missing commands, failed filters all exit 0. The detection engine
   checks exit codes, so false positives are endemic on Windows.

2. **`Get-Process` doesn't populate CommandLine** — requires WMI
   (`Get-CimInstance Win32_Process`) instead, which the detection
   never used.

3. **`cmd /C` adds banner text to stdout** — version string contains
   digits that match `\d+` patterns, causing false positive detection.

4. **`pip show` uses system Python, not venvs** — services installed
   in virtual environments aren't found by the system pip.

5. **Platform-specific shell syntax** — each manifest needs separate
   Windows/Linux/macOS detection sections with different commands that
   do the same thing (find a process).

These issues caused false positive adoption of OpenedAI Speech on
Windows stones where the service wasn't installed, because Python
processes existed (for other services) and the PowerShell command
returned exit 0.

### What Actually Works

Investigation on a live Windows machine running 6 AI services revealed:

- **WMI (`Win32_Process`)** reliably provides PID, executable name,
  full command line, and executable path for every process.
- **`GetNetTCPConnection`** maps PIDs to listening TCP ports.
- **Each service has a unique fingerprint** in its command line args,
  even when the executable name is generic (`python.exe`).
- **The `sysinfo` crate** (already a dependency) provides cross-platform
  process enumeration with command lines.
- **Port mapping** is available via platform APIs (Windows:
  `GetExtendedTcpTable`, Linux: `/proc/net/tcp`).

---

## Decision

Replace shell-command-based detection with a **process inventory +
port mapping + health verification** pipeline. Detection is data
matching against a system snapshot, not command execution.

### Detection Pipeline

```
┌─────────────────────────────────────────────────────┐
│ 1. PROCESS SNAPSHOT (runs once per scan cycle)      │
│    sysinfo::System::refresh_processes()             │
│    → Vec<ProcessInfo> { pid, name, cmdline, exe }   │
│    Cached. Every offering reads from the same       │
│    snapshot. No per-offering process enumeration.    │
├─────────────────────────────────────────────────────┤
│ 2. PORT MAP (runs once per scan cycle)              │
│    Platform-native TCP table enumeration:            │
│    Windows: GetExtendedTcpTable (IP Helper API)     │
│    Linux: parse /proc/net/tcp                       │
│    → HashMap<PID, Vec<u16>> (PID → listening ports) │
│    Combined with step 1: each process now has its   │
│    listening ports attached.                         │
├─────────────────────────────────────────────────────┤
│ 3. SERVICE MATCHING (per offering manifest)         │
│    Match process snapshot against manifest rules:    │
│    - executable name (exact or pattern)             │
│    - cmdline contains (substring or regex)          │
│    - optional: port extraction from cmdline         │
│    → Matched processes with discovered ports        │
├─────────────────────────────────────────────────────┤
│ 4. HEALTH VERIFICATION (per matched process)        │
│    Optional HTTP probe on discovered port:           │
│    - path: /health (or service-specific)            │
│    - expected_status: 200                           │
│    - response_match: unique string for this service │
│    → Confirmed operational service with known port  │
├─────────────────────────────────────────────────────┤
│ 5. PORT MEMORY (persistence)                        │
│    Store discovered port for fast re-detection:      │
│    - Next boot: probe remembered port first         │
│    - If remembered port fails: run full pipeline    │
└─────────────────────────────────────────────────────┘
```

### Manifest Format

```yaml
detection:
  process:
    # Required: executable name to match (case-insensitive)
    executable: python
    # Optional: platform-specific executable name
    windows_executable: python.exe
    linux_executable: python3
    # Optional: command line must contain this substring
    cmdline_contains: "speech.py"
    # Optional: regex for more complex matching
    cmdline_pattern: "speech\\.py"
    # Optional: extract port from command line
    port_extract: "--port\\s+(\\d+)"

  # Optional: verify the service is operational
  health:
    path: /health
    expected_status: 200
    # Optional: response body must contain this string
    response_contains: '"status"'

  # Port configuration
  ports:
    default: 8000
    # Scan range if default and cmdline extraction both fail
    range: [8000, 8010]
    # Remember discovered port across restarts
    remember: true

  # Scan timing (inherits from global config if not specified)
  scan:
    interval_secs: 30
    stability_threshold: 2   # consecutive successful detections
```

### Per-Service Detection Rules

**Ollama:**
```yaml
detection:
  process:
    executable: ollama
    cmdline_contains: "serve"
  health:
    path: /
    response_contains: "Ollama is running"
  ports:
    default: 11434
```

**ComfyUI:**
```yaml
detection:
  process:
    executable: python
    cmdline_contains: "main.py"
  health:
    path: /system_stats
    expected_status: 200
  ports:
    default: 8188
```

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
    port_extract: "--port\\s+(\\d+)"
```

**OpenedAI Speech:**
```yaml
detection:
  process:
    executable: python
    cmdline_contains: "speech.py"
  health:
    path: /health
    response_contains: '"status"'
  ports:
    default: 8000
    range: [8000, 8010]
    port_extract: "--port\\s+(\\d+)"
```

**Infinity:**
```yaml
detection:
  process:
    executable: python
    cmdline_contains: "start.py"
  health:
    path: /health
    response_contains: '"unix"'
  ports:
    default: 7997
    range: [7990, 8000]
```

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
    port_extract: "--port\\s+(\\d+)"
```

### Parent-Child Process Handling

Python venv services spawn child processes. The venv python launches
the system python, and the CHILD holds the listening port:

```
venv/python.exe (PID 100) → C:\Python312\python.exe (PID 200, port 8001)
```

The detection engine must handle this:
1. Match the parent process by cmdline (`speech.py`)
2. If the parent has no listening port, check its children
3. A child with the same cmdline pattern inherits the match
4. The port comes from whichever process (parent or child) is listening

### Scan Cycle

The process snapshot + port map run **once per scan cycle** (every
10-30 seconds, configurable). All offering manifests match against
the same cached snapshot. This is O(manifests × processes) string
matching — fast, no I/O per offering.

The health verification runs only for newly matched processes (not
every cycle for already-confirmed services). Confirmed services are
re-verified at a longer interval (5 minutes) or when the process
disappears from the snapshot.

This means manually launched services are picked up within one scan
cycle (10-30 seconds).

---

## Implementation Plan

### Phase 1: Process Inventory Module (`garden-common`)

New module: `detection::process_inventory`

- `ProcessSnapshot` struct: cached process list with command lines
- `PortMap` struct: PID → listening ports
- `ProcessSnapshot::capture()` — uses `sysinfo` crate
- `PortMap::capture()` — platform-specific TCP table enumeration
- Cross-platform: same interface, different implementations

### Phase 2: Service Matcher (`garden-common`)

New module: `detection::service_matcher`

- `ProcessSignature` struct: parsed from manifest YAML
- `ServiceMatch` struct: matched process with discovered port
- `match_service(signature, snapshot, port_map) -> Option<ServiceMatch>`
- Parent-child resolution for venv processes

### Phase 3: Detection Pipeline (`garden-common`)

New module: `detection::pipeline`

- `DetectionPipeline` struct: orchestrates snapshot → match → verify
- Replaces `detect_by_command` as the primary detection method
- `detect_by_command` and `detect_by_http_probe` remain for backward
  compatibility but are deprecated for new manifests

### Phase 4: Moss Integration

- `DetectionOrchestrator` updated to use the pipeline
- Manifest types extended for new detection format
- Auto-adoption task uses process-based detection
- Port memory in persisted offering config

### Phase 5: Manifest Migration

- All adopted manifests updated to new format
- Old `command` sections removed (or kept as deprecated fallback)
- Testing on Windows and Linux stones

---

## Consequences

### Positive

- No shell command execution for detection (no exit code ambiguity)
- Cross-platform with single manifest section (no per-OS commands)
- Process snapshot is shared across all offerings (efficient)
- Port discovery from cmdline or TCP table (handles non-default ports)
- Manually launched services detected within seconds
- Parent-child process handling for venv services

### Negative

- Platform-specific code for TCP table enumeration (2 implementations)
- `sysinfo` crate refreshes may have overhead on large process tables
- Port range scanning adds latency for first-time detection
- Parent-child resolution adds complexity

### Neutral

- Old `command` and `http_probe` detection methods remain for backward
  compatibility (no breaking changes to existing manifests)
- Manifest format change is additive (new `process` section alongside
  existing `detection` sections)
