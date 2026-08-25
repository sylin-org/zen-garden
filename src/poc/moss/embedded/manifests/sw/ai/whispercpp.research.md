# whisper.cpp — Research Notes

## Source

- **Repository:** https://github.com/ggerganov/whisper.cpp
- **Server source:** `examples/server/server.cpp`
- **License:** MIT

## Server Binary

- **Name:** `whisper-server` (or `whisper-server.exe` on Windows)
- **Pre-built binaries:** GitHub releases (`whisper-bin-x64.zip` for Windows)
- **Default port:** 8080 (not 8000 — commonly overridden via `--port`)
- **Default bind:** 127.0.0.1 (must use `--host 0.0.0.0` for LAN access)

## API Surface

**NOT OpenAI-compatible.** Custom endpoints only:

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/inference` | Transcription (multipart/form-data) |
| GET | `/health` | Readiness check |
| POST | `/load` | Runtime model loading |
| GET | `/` | HTML test interface |

The `/inference` path is configurable via `--inference-path`.

### POST /inference

- **Content-Type:** multipart/form-data
- **Required field:** `file` (audio data)
- **Key optional fields:** `response_format` (json/verbose_json/text/srt/vtt), `language`, `translate` (bool)
- **Response (json):** `{"text": "..."}`
- **Response (verbose_json):** includes segments with timestamps, word-level data, language detection probabilities

### GET /health

- **200:** `{"status":"ok"}` — ready for inference
- **503:** `{"status":"loading model"}` — model still loading

## Audio Format Support

- **WAV:** Always supported (16kHz mono PCM 16-bit expected)
- **MP3, FLAC, OGG, etc.:** Only with `--convert` flag (requires ffmpeg)
- Docker images include ffmpeg; native installs need it separately

## Docker

- **Image:** `ghcr.io/ggml-org/whisper.cpp:main` (note: ggml-org, NOT ggerganov)
- **CUDA variant:** `ghcr.io/ggml-org/whisper.cpp:main-cuda`
- **Vulkan variant:** `ghcr.io/ggml-org/whisper.cpp:main-vulkan`
- **Platforms:** linux/amd64, linux/arm64

## GPU Support

- Pre-built binaries are CPU-only (AVX2/FMA optimized on x86_64)
- CUDA support requires building from source or using the `-cuda` Docker image
- **No ROCm in pre-built binaries** — AMD GPU acceleration requires source build with ROCm
- Vulkan support available via source build or `-vulkan` Docker image

## Compatibility Notes

- CPU inference is surprisingly fast for base/small models (5-10x realtime)
- Large-v3 model (~3GB) needs ~3GB RAM for CPU inference
- No specific CPU instruction requirements beyond basic x86_64
- ARM64 supported (native Apple Silicon + Linux ARM64)

## Key Differences from OpenAI Whisper API

1. Endpoint is `/inference` not `/v1/audio/transcriptions`
2. Multipart field names differ (whisper.cpp uses snake_case)
3. No `model` field in request — model is set at server startup or via `/load`
4. Response format options differ (adds `srt`, `vtt`)
5. VAD (Voice Activity Detection) is a server-side feature, not per-request

## Orchestrator Adapter Requirements

The `WhisperCpp` offering adapter must:
1. Translate between orchestrator's unified transcription request and whisper.cpp's `/inference` multipart format
2. Health check via `GET /health` — check for `{"status":"ok"}` specifically (503 means loading)
3. No model enumeration — whisper.cpp runs one model at a time, set at startup
4. No model pull/sync — model files must be pre-placed or loaded via `/load`
5. Benchmark via timed `/inference` with a reference audio clip
