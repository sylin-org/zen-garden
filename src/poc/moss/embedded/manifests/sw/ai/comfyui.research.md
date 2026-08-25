# ComfyUI — Research Notes

## Source

- **Repository:** https://github.com/comfyanonymous/ComfyUI
- **Releases:** https://github.com/Comfy-Org/ComfyUI/releases
- **License:** GPL-3.0
- **Server source:** `server.py`, `comfy/model_management.py`

## Docker Images

**No official Docker image exists.** The ComfyUI project does not publish Docker images.
Community images exist (`yanwk/comfyui-boot`, `mmartial/comfyui-nvidia-docker`) but are
not officially maintained. The primary distribution is portable ZIP builds for Windows
and `comfy-cli` (`pip install comfy-cli`) for Linux/macOS.

The snippet.yaml uses `mmartial/comfyui-nvidia-docker:latest` as the best-maintained
community image. This is NVIDIA-only; there is no community AMD/ROCm Docker image.

## Portable Windows Build

Official releases provide self-contained portable builds:
- `ComfyUI_windows_portable_nvidia.7z` (~1.8GB, CUDA 12.4)
- `ComfyUI_windows_portable_nvidia_cu126.7z` (~1.8GB, CUDA 12.6)
- `ComfyUI_windows_portable_amd.7z` (~1.5GB, ROCm/DirectML)

Structure:
```
ComfyUI_windows_portable/
  python_embeded/python.exe    # Embedded CPython 3.12
  ComfyUI/main.py              # Application entry point
  ComfyUI/models/              # Model storage (checkpoints/, loras/, etc.)
  run_nvidia_gpu.bat
  run_amd_gpu.bat
```

Launch: `python_embeded\python.exe -s ComfyUI\main.py --windows-standalone-build`

Detection: `embedded_python: true` in `/system_stats` response when running
from a portable build (checks if parent of executable dir is `python_embeded`).

## API Surface

**No dedicated /health endpoint.** Use `GET /system_stats` (200 + JSON) for health.

### Key Endpoints

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/system_stats` | System info + GPU details (health check) |
| POST | `/prompt` | Submit workflow for execution |
| GET | `/queue` | Running + pending queue items |
| GET | `/models` | List model type folders |
| GET | `/models/{folder}` | List files in a model folder (e.g., `/models/checkpoints`) |
| GET | `/object_info` | All node class definitions |
| POST | `/upload/image` | Upload image for img2img |
| GET | `/view` | Retrieve generated images |
| POST | `/free` | Release VRAM (`{"unload_models": true, "free_memory": true}`) |
| WebSocket | `/ws?clientId=<uuid>` | Real-time execution events |

### GET /system_stats Response

```json
{
  "system": {
    "os": "nt",
    "ram_total": 34359738368,
    "ram_free": 28000000000,
    "comfyui_version": "0.18.2",
    "python_version": "3.12.0",
    "pytorch_version": "2.9.1+rocmsdk20260116",
    "embedded_python": true,
    "argv": ["main.py", "--windows-standalone-build", "--listen", "0.0.0.0"]
  },
  "devices": [
    {
      "name": "cuda:0 AMD Radeon RX 7900 XTX : ",
      "type": "cuda",
      "index": 0,
      "vram_total": 25753862144,
      "vram_free": 24000000000,
      "torch_vram_total": 1000000000,
      "torch_vram_free": 800000000
    }
  ]
}
```

**AMD ROCm note:** `device.type` is `"cuda"` even on AMD (ROCm's HIP layer presents
as CUDA). AMD is detected via `pytorch_version` containing `+rocm` or `+rocmsdk`.
The device name shows the actual AMD GPU name.

### WebSocket Events

Connection: `ws://host:8188/ws?clientId=<uuid>`

Key events:
- `execution_start` — workflow begins
- `progress` — `{value, max, prompt_id, node}` (throttled: 100ms min, 0.5% min change)
- `executed` — node completed with output
- `execution_success` — all nodes done
- `execution_error` — failure with traceback
- `execution_interrupted` — user cancelled

Binary messages: type byte 1=JPEG preview, 2=PNG preview.

### POST /prompt Request

```json
{
  "prompt": {"1": {"class_type": "CheckpointLoaderSimple", "inputs": {...}}, ...},
  "client_id": "uuid"
}
```

### GET /models/{folder}

Returns array of filenames: `["sd_xl_base_1.0.safetensors", "flux-dev.safetensors"]`

## GPU Detection

ComfyUI uses PyTorch for GPU detection:
- NVIDIA: `torch.version.cuda` is truthy
- AMD ROCm: `torch.version.hip` is truthy, device.type is still "cuda"
- Apple MPS: `torch.backends.mps.is_available()`
- CPU: fallback when no GPU available

## Orchestrator Adapter Requirements

1. **Probe:** `GET /system_stats` — extract version, GPU info, VRAM total/free
2. **Enumerate:** `GET /models/checkpoints` — list available checkpoints
3. **Proxy:** Submit parameterized workflow via `POST /prompt`, monitor via WebSocket, fetch output via `GET /view`
4. **Health:** `GET /system_stats` returning 200 with valid JSON
5. **VRAM tracking:** Real-time from `/system_stats` `devices[*].vram_free`
6. **Queue tracking:** `GET /prompt` → `exec_info.queue_remaining`
7. **VRAM release:** `POST /free {"unload_models": true, "free_memory": true}`

## Default Port

**8188** (configurable via `--port`)

## Default Bind

**127.0.0.1** (must use `--listen 0.0.0.0` for LAN access)
