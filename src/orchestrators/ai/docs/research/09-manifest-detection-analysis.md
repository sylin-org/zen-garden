# Manifest Detection Analysis

> Cross-reference of current detection rules against actual process
> behavior. Identifies every failure point and maps to new process-based
> detection approach.

---

## Per-Service Analysis

### 1. Ollama

**Current detection**: `ollama --version` (command) + HTTP probe :11434
**Actual process**: `ollama.exe serve` (PID 7932), listening on :11434

| Rule | Works? | Issue |
|------|--------|-------|
| `ollama --version` | Yes on this machine | Relies on ollama being in PATH. Works because Ollama installer adds to PATH. |
| HTTP probe :11434 | Yes | Reliable — unique port, unique response "Ollama is running" |

**New approach**:
- Process: `executable: ollama`, `cmdline_contains: serve`
- Port: TCP table lookup for matched PID → 11434
- Health: `GET /` → "Ollama is running"

**Risk**: Low. Ollama has a unique binary name.

**Note**: `ollama-cpu` uses the SAME binary and port. The only
difference is environment variables (`CUDA_VISIBLE_DEVICES=""`).
Process detection cannot distinguish ollama from ollama-cpu.
This needs a different approach — maybe check env vars, or detect
via capabilities after adoption (GPU vs CPU inference speed).

---

### 2. ComfyUI

**Current detection**: `comfy --help` (command) + HTTP probe :8188
**Actual process**: `python.exe -s main.py --windows-standalone-build --listen 0.0.0.0` (PID 31108), listening on :8188

| Rule | Works? | Issue |
|------|--------|-------|
| `comfy --help` | No on this machine | `comfy` CLI not installed (portable build, no pip install). The ComfyUI CLI is a separate pip package, not included in the portable distribution. |
| HTTP probe :8188 | Yes | Reliable — `/system_stats` returns JSON with ComfyUI-specific fields |

**New approach**:
- Process: `executable: python`, `cmdline_contains: main.py`
- Disambiguation: need additional signal. `main.py` is too generic.
  Options: (a) check that `--windows-standalone-build` is in args,
  (b) check executable path contains "ComfyUI" (path-dependent, bad),
  (c) rely on health probe at discovered port
- Port: TCP table lookup → 8188
- Health: `GET /system_stats` → response contains `"system"` and `"devices"`

**Risk**: Medium. `main.py` is generic. Multiple Python apps use it.
Must combine with port health verification to confirm identity.

---

### 3. whisper.cpp

**Current detection**: `where whisper-server.exe` (command) + HTTP probe :{{port}}
**Actual process**: `whisper-server.exe --model ... --port 8000` (PID 27624), listening on :8000

| Rule | Works? | Issue |
|------|--------|-------|
| `where whisper-server.exe` | Fails | whisper-server.exe is NOT in PATH. It's at `E:\AI\WhisperCpp\Release\`. `where` only searches PATH. |
| HTTP probe :{{port}} | Depends | Works if `{{port}}` resolves correctly from frontmatter. But the port template resolution may not work for adopted offerings. |

**New approach**:
- Process: `executable: whisper-server` (unique name, no ambiguity)
- Port: TCP table lookup → 8000 (or whatever it's actually bound to)
- Health: `GET /health` → `{"status":"ok"}`

**Risk**: Low. Unique binary name.

---

### 4. OpenedAI Speech

**Current detection**: PowerShell `Get-Process` + cmdline match (command) + HTTP probe :{{port}}
**Actual process**: TWO processes:
  - Parent: `venv\Scripts\python.exe speech.py --port 8001 --xtts_device none` (PID 32760, no port)
  - Child: `C:\Python312\python.exe speech.py --port 8001 --xtts_device none` (PID 32100, port 8001)

| Rule | Works? | Issue |
|------|--------|-------|
| PowerShell Get-Process + cmdline match | No | `Get-Process` doesn't populate CommandLine. Even with fix (direct PS call), the PowerShell pipeline returns exit 0 with empty output. Pattern `\d+` doesn't match empty string but the exit code check passes. Detection depends on pattern check, which is correct but fragile. |
| HTTP probe :{{port}} | Depends | If `{{port}}` = 8000 (default), fails because actual port is 8001. If template resolves to 8001, works. |

**Critical finding**: The child process (PID 32100) holds the port,
not the parent (PID 32760). Both have `speech.py` in their cmdline.

**New approach**:
- Process: `executable: python`, `cmdline_contains: speech.py`
- Match: finds PID 32760 AND PID 32100 (both match)
- Port: TCP table shows PID 32100 → port 8001 (PID 32760 has no port)
- Health: `GET :8001/health` → `{"status":"ok"}`
- Result: detected on port 8001 (from TCP table, not assumed)

**Risk**: Low with process-based approach. `speech.py` is unique enough.

---

### 5. Infinity

**Current detection**: `pip show infinity-emb` (command) + HTTP probe :7997
**Actual process**: TWO processes:
  - Parent: `Infinity\venv\Scripts\python.exe E:\AI\Infinity\start.py` (PID 31500, no port)
  - Child: `C:\Python312\python.exe E:\AI\Infinity\start.py` (PID 10364, port 7997)

| Rule | Works? | Issue |
|------|--------|-------|
| `pip show infinity-emb` | No | System pip doesn't find it — installed in venv only. `pip show` checks the system Python, not the venv. |
| HTTP probe :7997 | Yes | Reliable when running on default port |

**Critical finding**: Same parent-child pattern as OpenedAI Speech.
Child (system python) holds the port. `start.py` in cmdline of both.

**New approach**:
- Process: `executable: python`, `cmdline_contains: start.py`
- Problem: `start.py` is generic. Many apps use it.
- Disambiguation: TCP table shows the matched PID listens on a port.
  Health probe at that port: `GET /health` → response contains `"unix"`
  (Infinity returns `{"unix": timestamp}` — unique signature).
- Port: TCP table for child PID 10364 → 7997
- Health: `GET :7997/health` → `{"unix": ...}`

**Risk**: Medium. `start.py` is generic. Relies on health response
signature for disambiguation. If another service uses `start.py` AND
has a `/health` endpoint returning `"unix"`, false positive. In
practice, this combination is unlikely.

---

### 6. LibreTranslate

**Current detection**: `pip show libretranslate` (command) + HTTP probes :5000
**Actual process**: NOT RUNNING at time of data collection.
Expected: `libretranslate.exe --host 0.0.0.0 --port 5000`

| Rule | Works? | Issue |
|------|--------|-------|
| `pip show libretranslate` | No | System pip doesn't find it — installed in venv. Same issue as Infinity. |
| HTTP probe :5000 | Yes when running | Reliable |

**New approach**:
- Process: `executable: libretranslate` (unique entry point name)
- Port: TCP table → 5000 (or whatever bound)
- Health: `GET /health` → `{"status":"ok"}`

**Risk**: Low. Unique executable name.

---

### 7. ollama-cpu

**Current detection**: Same as Ollama (`ollama --version` + HTTP :11434)
**Actual process**: Same `ollama.exe serve` binary — cannot be
distinguished from GPU Ollama by process name or command line.

| Rule | Works? | Issue |
|------|--------|-------|
| `ollama --version` | Same as Ollama | Cannot distinguish CPU-only from GPU |
| HTTP probe :11434 | Same as Ollama | Same binary, same port |

**Fundamental issue**: ollama and ollama-cpu are the SAME process.
The difference is runtime environment (`CUDA_VISIBLE_DEVICES=""`).
Process-based detection cannot distinguish them.

**Options**:
1. Don't auto-adopt ollama-cpu separately. If ollama is detected,
   check GPU availability and categorize accordingly.
2. Check environment variables of the process (platform-specific,
   complex, fragile).
3. After adoption, classify based on observed behavior (GPU detection
   in capabilities vs CPU-only inference speed).

**Recommendation**: Option 3. Detect "ollama" once. After adoption,
the capabilities detection (which already runs) determines if GPU is
available. The offering category (ollama vs ollama-cpu) is an
attribute, not a separate detection target.

---

## Summary: Detection Method Reliability

| Service | Command Rule | HTTP Probe | Process-Based | Notes |
|---------|-------------|------------|---------------|-------|
| Ollama | Works | Works | Easy | Unique binary |
| ComfyUI | Fails | Works | Medium | `main.py` is generic, needs health verify |
| whisper.cpp | Fails | Works | Easy | Unique binary |
| OpenedAI Speech | Fails | Port-dependent | Easy | `speech.py` unique, parent-child handling |
| Infinity | Fails | Works | Medium | `start.py` generic, needs health verify |
| LibreTranslate | Fails | Works | Easy | Unique entry point |
| ollama-cpu | N/A | N/A | Impossible | Same binary as ollama |

**5 of 7 command-based rules fail.** HTTP probes work but depend on
knowing the correct port. Process-based detection with TCP port
lookup solves both problems.

---

## Manifest Changes Required

### Remove entirely:
- All `command` detection rules (5/7 fail on Windows)
- Platform-specific `windows:` / `linux:` / `macos:` sections for detection

### Replace with:
- `process:` section (cross-platform, one definition)
- `health:` section (HTTP verification with response signature)
- `ports:` section (default + range + remember)

### Special cases:
- **ollama-cpu**: merge with ollama detection. Post-adoption classification.
- **ComfyUI**: `main.py` too generic → mandatory health verification
- **Infinity**: `start.py` too generic → mandatory health verification
