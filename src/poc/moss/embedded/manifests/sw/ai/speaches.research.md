# Speaches — Research Notes

## Overview

| Property | Value |
|----------|-------|
| Official Name | Speaches (formerly faster-whisper-server) |
| License | MIT |
| Repository | https://github.com/speaches-ai/speaches |
| Documentation | https://speaches.ai/ |
| Docker Registry | ghcr.io/speaches-ai/speaches |
| Runtime | Python 3.12 (FastAPI + Uvicorn) |
| Default Port | 8000 |

## What is Speaches?

Speaches is an OpenAI API-compatible server for speech-to-text (STT),
text-to-speech (TTS), and real-time voice processing. It describes itself
as "Ollama, but for TTS/STT models" — dynamic model loading/unloading on
demand, multi-model registry, and a Gradio web UI.

Previously named `faster-whisper-server` (repo `fedirz/faster-whisper-server`),
renamed to `speaches-ai/speaches` in 2025.

## Key Differentiator

Speaches is the only local offering that provides **both STT and TTS** in a
single service with **OpenAI API compatibility**. This makes it a drop-in
replacement for OpenAI's audio endpoints while running fully on-premise.

## Docker Image Analysis

### Image Selection

| Image | Tag | Purpose | Size |
|-------|-----|---------|------|
| `ghcr.io/speaches-ai/speaches` | `latest-cuda` | NVIDIA GPU (CUDA 12.6.3) | ~3.5 GB |
| `ghcr.io/speaches-ai/speaches` | `latest-cuda-12.6.3` | Pinned CUDA version | ~3.5 GB |
| `ghcr.io/speaches-ai/speaches` | `latest-cpu` | CPU-only | ~1.5 GB |

### Architecture Support

| Architecture | CUDA | CPU |
|-------------|------|-----|
| amd64 | Yes | Yes |
| arm64 | No | Untested |

ARM support is limited — the underlying CTranslate2 library has
x86-focused optimizations.

## GPU Support

**NVIDIA CUDA**: Fully supported. Docker images ship with CUDA 12.6.3
on Ubuntu 24.04. Use `--gpus=all` or CDI.

**AMD ROCm**: Not supported. No ROCm Docker images published. The
underlying CTranslate2 has limited ROCm support but Speaches doesn't
package it.

**Intel**: Not supported.

**Apple Metal/CoreML**: Not applicable (Docker-only deployment, macOS
uses CPU mode).

## Resource Requirements

### STT (Whisper) Models

| Model | Parameters | VRAM (float16) | RAM (int8) |
|-------|-----------|---------------|-----------|
| tiny | 39M | ~200 MB | ~100 MB |
| base | 74M | ~300 MB | ~200 MB |
| small | 244M | ~600 MB | ~400 MB |
| medium | 769M | ~1.5 GB | ~800 MB |
| large-v3 | 1.5B | ~3 GB | ~1.5 GB |
| distil-large-v3 | 756M | ~1.5 GB | ~800 MB |

### TTS (Kokoro) Models

| Model | Parameters | RAM |
|-------|-----------|-----|
| Kokoro-82M-v1.0-ONNX | 82M | ~200 MB |

Kokoro is lightweight — ONNX runtime, runs well on CPU.

## API Endpoints

### OpenAI-Compatible

| Method | Endpoint | Purpose |
|--------|----------|---------|
| POST | `/v1/audio/transcriptions` | Speech-to-text |
| POST | `/v1/audio/speech` | Text-to-speech |
| GET | `/v1/models` | List downloaded models |

### Speaches-Native

| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/v1/registry` | Browse all available models |
| GET | `/v1/registry?task=automatic-speech-recognition` | STT models |
| GET | `/v1/registry?task=text-to-speech` | TTS models |
| POST | `/v1/models/{model_id}` | Download a model |
| POST | `/v1/audio/speech/timestamps` | Voice activity detection |
| WS | `/v1/realtime` | WebSocket realtime voice |
| GET | `/health` | Health check |
| GET | `/docs` | Swagger UI |

### STT Request

```bash
curl -X POST http://localhost:8000/v1/audio/transcriptions \
  -F "file=@audio.wav" \
  -F "model=Systran/faster-distil-whisper-small.en"
```

Response: `{"text": "transcribed text here"}`

### TTS Request

```bash
curl -X POST http://localhost:8000/v1/audio/speech \
  -H "Content-Type: application/json" \
  -d '{
    "input": "Hello world",
    "model": "speaches-ai/Kokoro-82M-v1.0-ONNX",
    "voice": "af_heart",
    "response_format": "mp3"
  }' -o output.mp3
```

### Model Aliases

Speaches maps OpenAI model names to HuggingFace IDs:
- `whisper-1` → configurable STT model
- `tts-1` → configurable TTS model
- `tts-1-hd` → configurable high-quality TTS model

## Environment Variables

### Core Server

| Variable | Default | Purpose |
|----------|---------|---------|
| `UVICORN_HOST` | `0.0.0.0` | Bind address |
| `UVICORN_PORT` | `8000` | Server port |
| `LOG_LEVEL` | `debug` | Log verbosity |
| `API_KEY` | (none) | Optional auth key |
| `ENABLE_UI` | `True` | Gradio web UI at `/` |

### Model Lifecycle (Ollama-style TTL)

| Variable | Default | Purpose |
|----------|---------|---------|
| `STT_MODEL_TTL` | `300` | Seconds before STT model unloads (-1 = never) |
| `TTS_MODEL_TTL` | `300` | Seconds before TTS model unloads (-1 = never) |
| `VAD_MODEL_TTL` | `-1` | Seconds before VAD model unloads |
| `PRELOAD_MODELS` | `[]` | Models to download at startup |

### Whisper Configuration

| Variable | Default | Purpose |
|----------|---------|---------|
| `WHISPER__INFERENCE_DEVICE` | `auto` | `cpu`, `cuda`, or `auto` |
| `WHISPER__DEVICE_INDEX` | `0` | GPU device ID |
| `WHISPER__COMPUTE_TYPE` | `default` | Quantization: `int8`, `float16`, `float32` |
| `WHISPER__CPU_THREADS` | `0` | CPU threads (0 = auto) |
| `WHISPER__NUM_WORKERS` | `1` | Concurrent inference workers |

### LLM Integration (for voice chat)

| Variable | Default | Purpose |
|----------|---------|---------|
| `CHAT_COMPLETION_BASE_URL` | `http://localhost:11434/v1` | LLM backend (Ollama) |
| `CHAT_COMPLETION_API_KEY` | `cant-be-empty` | LLM auth key |

## Model Discovery

Models are discovered through two endpoints:

1. `/v1/models` — shows downloaded/loaded models
2. `/v1/registry` — shows ALL available models from HuggingFace

The registry supports task filtering:
- `?task=automatic-speech-recognition` — Whisper models
- `?task=text-to-speech` — Kokoro and Piper models

Models are downloaded on first use or via explicit `POST /v1/models/{id}`.

## Health Check Strategy

`GET /health` — returns 200 when the server is ready.

Startup can take 30-120 seconds depending on whether models need
downloading. The health endpoint becomes available once the FastAPI
app is loaded, before model downloads complete.

## Comparison with Other STT/TTS Offerings

| Feature | Speaches | whisper.cpp | OpenedAI Speech |
|---------|----------|-------------|-----------------|
| STT | Yes (faster-whisper) | Yes (whisper.cpp) | No |
| TTS | Yes (Kokoro, Piper) | No | Yes (XTTS, Piper) |
| OpenAI API | Full compatibility | Custom `/inference` | TTS only |
| Model management | Dynamic (Ollama-style) | Single model | Fixed models |
| GPU | CUDA only | CUDA, Vulkan, Metal | CUDA only |
| CPU | Yes (int8 quantized) | Yes (optimized) | Yes |
| ARM | Limited | Excellent | Limited |
| WebSocket | Yes (`/v1/realtime`) | No | No |
| Streaming STT | Yes (SSE) | No | No |
| Web UI | Yes (Gradio) | No | No |
| Default port | 8000 | 8080 | 8001 |

**Recommendation**: Speaches is the preferred offering for environments
needing both STT and TTS with OpenAI compatibility. whisper.cpp is better
for ARM devices or minimal-resource environments needing STT only.

## Orchestrator Adapter Requirements

The AI Orchestrator's Speaches provider needs:

1. **Probe**: `GET /health` — check 200 status
2. **Enumerate**: `GET /v1/models` — list downloaded models with capabilities
   - STT models tagged with `Capability::Transcribe`
   - TTS models tagged with `Capability::Speech`
3. **Transcribe**: `POST /v1/audio/transcriptions` — OpenAI-compatible, pass-through
4. **Speak**: `POST /v1/audio/speech` — OpenAI-compatible, pass-through

Since Speaches speaks native OpenAI format for both endpoints, the adapter
is essentially pass-through — similar to the OpenedAI Speech and Infinity
providers.

## Security Considerations

- `API_KEY` env var enables bearer auth (optional)
- No HTTPS — runs behind reverse proxy in production
- Model downloads from HuggingFace require network access
- Gradio UI (`ENABLE_UI=True`) should be disabled in production

## Validation Checklist

- [x] Docker image exists on GHCR
- [x] CUDA and CPU variants available
- [x] OpenAI API compatible (STT + TTS)
- [x] Health check endpoint at `/health`
- [x] Model listing at `/v1/models`
- [x] Swagger docs at `/docs`
- [x] Dynamic model loading (Ollama-style)
- [x] Existing snippet.yaml and frontmatter.json in Moss
- [ ] adopted.yaml (native detection) — TODO
- [ ] Orchestrator Provider impl — TODO

## References

- https://github.com/speaches-ai/speaches
- https://speaches.ai/
- https://speaches.ai/installation/
- https://speaches.ai/configuration/
- https://speaches.ai/usage/speech-to-text/
- https://speaches.ai/usage/text-to-speech/
- https://speaches.ai/usage/model-discovery/
- https://speaches.ai/usage/realtime-api/
