---
version: "1"
trigger: post_install
---

# ComfyUI — Image Generation

**Web UI:** http://{{server-name}}:{{port}}
**Image:** `yanwk/comfyui-boot` (auto-selects NVIDIA/AMD/CPU variant)

## Getting Started

ComfyUI starts with **no models**. Download at least one checkpoint to begin generating images.

### Download a Checkpoint

Place checkpoint files in the `comfyui-models` volume under `checkpoints/`:

**Popular models:**
- **Flux-dev** (12GB VRAM) — state-of-the-art, requires HuggingFace account
- **SDXL** (8GB VRAM) — high quality, broad community support
- **SD 1.5** (4GB VRAM) — lightweight, huge model ecosystem

Download from [CivitAI](https://civitai.com/) or [HuggingFace](https://huggingface.co/) and place `.safetensors` files in the models volume.

### Model Directory Structure

```
comfyui-models/
  checkpoints/    # Main model files (.safetensors, .ckpt)
  loras/          # LoRA fine-tune adapters
  vae/            # VAE models
  controlnet/     # ControlNet models
  clip/           # CLIP text encoders
  clip_vision/    # CLIP vision encoders
  text_encoders/  # Text encoder models (T5, etc.)
  upscale_models/ # Upscaling models (ESRGAN, etc.)
  embeddings/     # Textual inversion embeddings
```

### Install Custom Nodes

Open the web UI and use **ComfyUI-Manager** (pre-installed) to browse and install custom nodes. Nodes persist in the `comfyui-custom-nodes` volume across container updates.

## Hardware Variants

Zen Garden auto-selects the best image for your hardware:

| GPU | Image Tag | Performance |
|-----|-----------|-------------|
| NVIDIA (RTX 3060+) | `cu130-slim-v2` | Best — full CUDA acceleration |
| AMD (RX 6000/7000) | `rocm` | Good — ROCm acceleration |
| No GPU | `cpu` | Very slow — minutes per image |

### AMD ROCm Notes

For RX 7900 XTX/XT/GRE, set `HSA_OVERRIDE_GFX_VERSION=11.0.0` in CLI_ARGS or environment. For RX 6000 series, the `rocm6` tag may perform better.

**Performance tip:** Add `PYTORCH_TUNABLEOP_ENABLED=1` to environment — improves performance after first-run kernel compilation.

### AMD on Windows (DirectML)

Docker Desktop does not support AMD GPU passthrough. On Windows, AMD GPUs expose DirectML — but `/dev/kfd` (Linux ROCm) does not exist, and Docker Desktop does not expose `/dev/dxg` (WSL2 GPU device).

**To enable GPU acceleration on Windows with AMD:**

1. Install **Docker CE inside a WSL2 Ubuntu distro** (not Docker Desktop)
2. Install AMD's [ROCDXG library](https://github.com/ROCm/librocdxg) inside WSL2
3. Launch containers with `--device=/dev/dxg` and mount the ROCDXG libraries
4. Set `HSA_ENABLE_DXG_DETECTION=1` in the container environment

Without this setup, ComfyUI falls back to CPU mode. This is a Docker Desktop limitation, not a Zen Garden or ComfyUI issue.

## User Scripts

The `comfyui-scripts` volume contains a `pre-start.sh` hook that runs before ComfyUI launches. Use it to:
- Auto-install custom nodes
- Download models
- Set environment variables
- Run maintenance tasks

## API Usage

ComfyUI exposes a full REST API on port {{port}}:

| Purpose | Endpoint |
|---------|----------|
| System stats / health | `GET /system_stats` |
| Submit workflow | `POST /prompt` (JSON workflow graph) |
| Queue status | `GET /queue` |
| Job tracking | `GET /api/jobs` |
| Available models | `GET /models/checkpoints` |
| Available nodes | `GET /object_info` |
| Upload image | `POST /upload/image` |
| View output | `GET /view?filename=...&type=output` |
| Free VRAM | `POST /free {"unload_models": true}` |
| WebSocket events | `WS /ws?clientId=<uuid>` |

### AI Orchestrator Integration

When the AI Orchestrator is running, ComfyUI instances are automatically discovered and available for the **Image** capability. The orchestrator can submit workflows programmatically via the `/prompt` endpoint.

## Useful CLI_ARGS

Set via the `CLI_ARGS` environment variable:

| Flag | Effect |
|------|--------|
| `--lowvram` | Aggressive VRAM offloading (4-6GB GPUs) |
| `--novram` | Maximum offloading to system RAM |
| `--cpu` | Force CPU-only (no GPU) |
| `--fast` | Experimental optimizations |
| `--disable-smart-memory` | More predictable VRAM usage |
| `--enable-cors-header` | Enable CORS for cross-origin API access |

## Security

ComfyUI has **no built-in authentication**. Access is controlled at the network level. Do not expose port {{port}} to the public internet without a reverse proxy with authentication.
