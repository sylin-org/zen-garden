---
version: "1"
trigger: post_install
---

# Speaches — Speech-to-Text & Text-to-Speech

**API:** http://{{server-name}}:{{port}}
**Web UI:** http://{{server-name}}:{{port}}/docs (Swagger)

## OpenAI-Compatible Endpoints

| Endpoint | Purpose |
|----------|---------|
| `POST /v1/audio/transcriptions` | Speech-to-text (Whisper) |
| `POST /v1/audio/speech` | Text-to-speech (Kokoro/Piper) |
| `GET /health` | Health check |

## Quick Test

```bash
# Transcribe an audio file
curl -X POST http://{{server-name}}:{{port}}/v1/audio/transcriptions \
  -F "file=@audio.wav" \
  -F "model=Systran/faster-distil-whisper-small.en"
```

## Model Management

Models are downloaded automatically on first use from HuggingFace. To pre-load models at startup, set `PRELOAD_MODELS` in the environment.

Popular whisper models:
- `Systran/faster-whisper-tiny` (~150MB, fastest, lower accuracy)
- `Systran/faster-distil-whisper-small.en` (~500MB, good balance)
- `Systran/faster-distil-whisper-large-v3` (~1.5GB, best accuracy)

## AI Orchestrator Integration

When the AI Orchestrator is running, Speaches instances are available via:
- `POST /api/transcribe` — speech-to-text
- `POST /api/speak` — text-to-speech (if TTS models loaded)
