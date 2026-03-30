# Process Detection Catalog

> Research artifact. Raw data gathered from a live Windows machine
> (stone-azure-pool) running all AI services. Data collected
> 2026-03-30 via Win32_Process + GetNetTCPConnection.

---

## Process Inventory (Win32_Process via WMI)

### Ollama

| Field | Value |
|-------|-------|
| PID | 7932 |
| Name | `ollama.exe` |
| ExecutablePath | `E:\AI\Ollama\ollama.exe` |
| CommandLine | `E:\AI\Ollama\ollama.exe serve` |
| ParentPID | 24976 (`ollama app.exe` — the tray app) |
| Listening Port | 11434 (on `::`) |

**Detection fingerprint**: Unique executable name `ollama.exe` + arg `serve`. No ambiguity.

**Parent**: `ollama app.exe` (tray app, PID 24976, listening on 56205/localhost only — internal comms).

---

### ComfyUI

| Field | Value |
|-------|-------|
| PID | 31108 |
| Name | `python.exe` |
| ExecutablePath | `E:\AI\ComfyUI\ComfyUI_windows_portable\python_embeded\python.exe` |
| CommandLine | `"...\python_embeded\python.exe" -s "...\ComfyUI\main.py" --windows-standalone-build --listen 0.0.0.0` |
| ParentPID | 31232 (exited — was the launcher) |
| Listening Port | 8188 (on `0.0.0.0`) |

**Detection fingerprint**: `python.exe` + cmdline contains `main.py` AND `--windows-standalone-build`. The `main.py` alone is too generic; the `--windows-standalone-build` flag is ComfyUI-specific.

**Note**: Uses embedded Python, NOT system Python. ExecutablePath contains `python_embeded`.

---

### whisper.cpp (STT)

| Field | Value |
|-------|-------|
| PID | 27624 |
| Name | `whisper-server.exe` |
| ExecutablePath | `E:\AI\WhisperCpp\Release\whisper-server.exe` |
| CommandLine | `"...\whisper-server.exe" --model ...\ggml-base.en.bin --host 0.0.0.0 --port 8000` |
| ParentPID | 31232 (exited) |
| Listening Port | 8000 (on `0.0.0.0`) |

**Detection fingerprint**: Unique executable name `whisper-server.exe`. No ambiguity.

**Port extraction**: `--port (\d+)` from cmdline → 8000.

---

### OpenedAI Speech (TTS)

| Field | Value |
|-------|-------|
| PID | 32760 (venv python) |
| Name | `python.exe` |
| ExecutablePath | `E:\AI\OpenedAI-Speech\venv\Scripts\python.exe` |
| CommandLine | `...\venv\Scripts\python.exe speech.py --port 8001 --xtts_device none` |
| ParentPID | 32684 (`cmd.exe /C start.bat`) |
| Listening Port | — (not the listener) |

**Child process**:
| Field | Value |
|-------|-------|
| PID | 32100 |
| Name | `python.exe` |
| ExecutablePath | `C:\Python312\python.exe` |
| CommandLine | `"C:\Python312\python.exe" speech.py --port 8001 --xtts_device none` |
| ParentPID | 32760 |
| Listening Port | 8001 (on `0.0.0.0`) |

**Detection fingerprint**: `python.exe` + cmdline contains `speech.py`. Unique enough (no other script named `speech.py`).

**Port extraction**: `--port (\d+)` from cmdline → 8001.

**Key observation**: TWO python processes. The venv python (32760) spawns system python (32100). The CHILD holds the listening port, not the parent. Detection must follow the process tree or match the child directly.

---

### Infinity (Embeddings + Rerank)

| Field | Value |
|-------|-------|
| PID | 31500 (venv python) |
| Name | `python.exe` |
| ExecutablePath | `E:\AI\Infinity\venv\Scripts\python.exe` |
| CommandLine | `"...\Infinity\venv\Scripts\python.exe" E:\AI\Infinity\start.py` |
| ParentPID | 31232 (exited) |
| Listening Port | — (not the listener) |

**Child process**:
| Field | Value |
|-------|-------|
| PID | 10364 |
| Name | `python.exe` |
| ExecutablePath | `C:\Python312\python.exe` |
| CommandLine | `"C:\Python312\python.exe" E:\AI\Infinity\start.py` |
| ParentPID | 31500 |
| Listening Port | 7997 (on `0.0.0.0`) |

**Detection fingerprint**: `python.exe` + cmdline contains `start.py`. BUT `start.py` is generic — many projects use it. Need additional signal.

**Disambiguation options**:
1. Cmdline contains `Infinity` (path-dependent — bad)
2. Verify port 7997 responds with Infinity's signature (`/health` returns `{"unix": ...}`)
3. Cmdline contains `infinity` (case-insensitive) — works if the directory name is stable

**Key observation**: Same parent-child pattern as OpenedAI Speech. Venv python spawns system python. The child holds the port.

---

### LibreTranslate

Not running at time of data collection.

Expected process:
| Field | Expected |
|-------|----------|
| Name | `libretranslate.exe` (pip entry point) |
| CommandLine | `libretranslate.exe --host 0.0.0.0 --port 5000` |
| Listening Port | 5000 |

**Detection fingerprint**: Unique executable name `libretranslate.exe`. No ambiguity.

**Port extraction**: `--port (\d+)` from cmdline → 5000.

---

## Key Findings

### 1. Parent-Child Process Chains

Python venv services spawn a child process. The parent (venv python)
launches the child (system python or entry point). The CHILD process
holds the listening port, not the parent.

```
start.bat → cmd.exe → venv/python.exe → C:\Python312\python.exe (LISTENER)
start-all.ps1 → venv/python.exe → C:\Python312\python.exe (LISTENER)
```

Detection must either:
- Match the CHILD process (which has the port)
- Or match the parent and follow the tree to find the port

### 2. CommandLine Is Populated via WMI

`Get-CimInstance Win32_Process` populates CommandLine correctly.
`Get-Process` does NOT. WMI (CIM) is the correct Windows API for
command line access.

On Linux: `/proc/{pid}/cmdline` — always available, no shell needed.

### 3. Process Name Uniqueness

| Service | Process Name | Unique? |
|---------|-------------|---------|
| Ollama | `ollama.exe` | Yes |
| whisper.cpp | `whisper-server.exe` | Yes |
| LibreTranslate | `libretranslate.exe` | Yes |
| ComfyUI | `python.exe` | No — need cmdline |
| OpenedAI Speech | `python.exe` | No — need cmdline |
| Infinity | `python.exe` | No — need cmdline |

### 4. Cmdline Patterns (Stable, Path-Independent)

| Service | Cmdline Pattern | Unique? |
|---------|----------------|---------|
| Ollama | `ollama.* serve` | Yes |
| whisper.cpp | `whisper-server` | Yes |
| LibreTranslate | `libretranslate` | Yes |
| ComfyUI | `main.py.*--windows-standalone-build` | Yes (Windows) |
| ComfyUI | `main.py.*--listen` | Likely unique (Linux) |
| OpenedAI Speech | `speech.py` | Yes |
| Infinity | `start.py` | No — needs port/health verify |

### 5. Port Extraction from CommandLine

| Service | Port Flag | Regex | Default |
|---------|-----------|-------|---------|
| Ollama | (none — always 11434) | — | 11434 |
| ComfyUI | `--port (\d+)` | `--port\s+(\d+)` | 8188 |
| whisper.cpp | `--port (\d+)` | `--port\s+(\d+)` | 8000 |
| OpenedAI Speech | `--port (\d+)` | `--port\s+(\d+)` | 8000 |
| Infinity | (in start.py, not cmdline) | — | 7997 |
| LibreTranslate | `--port (\d+)` | `--port\s+(\d+)` | 5000 |

### 6. PID-to-Port Mapping

Available via:
- **Windows**: `GetExtendedTcpTable` (C API) or `Get-NetTCPConnection` (PowerShell/WMI)
- **Linux**: `/proc/net/tcp` or `ss -tlnp`

This maps PID → listening port without parsing cmdline.
Combined with process matching: find PID by cmdline, then look up port.
