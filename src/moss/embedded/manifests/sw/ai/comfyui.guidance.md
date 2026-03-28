---
version: "1"
trigger: post_install
---

# ComfyUI — Image Generation

**Web UI:** http://{{server-name}}:{{port}}

## Getting Started

ComfyUI starts with no models. Download at least one checkpoint to begin generating images.

### Download a Checkpoint

Place checkpoint files in the data volume under `models/checkpoints/`:

```bash
# Example: download Flux-dev (requires HuggingFace account)
# Or download SD 1.5 / SDXL checkpoints from civitai.com
```

Models directory structure:
```
models/
  checkpoints/    # Main model files (.safetensors)
  loras/          # LoRA fine-tune adapters
  vae/            # VAE models
  controlnet/     # ControlNet models
  clip/           # CLIP text encoders
```

### AI Orchestrator Integration

When the AI Orchestrator is running, ComfyUI instances are automatically discovered and available via:
- `POST /api/imagine` — text-to-image generation
- `POST /api/edit` — image-to-image editing

The orchestrator routes requests to the best available ComfyUI instance based on VRAM availability and queue depth.

## Useful Commands

| Purpose | URL |
|---------|-----|
| System stats | `http://{{server-name}}:{{port}}/system_stats` |
| Queue status | `http://{{server-name}}:{{port}}/queue` |
| Available checkpoints | `http://{{server-name}}:{{port}}/models/checkpoints` |
| Free VRAM | `POST http://{{server-name}}:{{port}}/free {"unload_models": true}` |
