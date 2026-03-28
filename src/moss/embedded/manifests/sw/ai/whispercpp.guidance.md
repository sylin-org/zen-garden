---
version: "1"
trigger: post_install
---

# whisper.cpp — Speech-to-Text

**API:** http://{{server-name}}:{{port}}

## Endpoint

```bash
# Transcribe an audio file
curl -X POST http://{{server-name}}:{{port}}/inference \
  -F "file=@audio.wav" \
  -F "response_format=json"
```

## Models

whisper.cpp uses GGML-format Whisper models. Download from HuggingFace:

| Model | Size | Speed | Accuracy |
|-------|------|-------|----------|
| `ggml-tiny.en` | 75MB | Fastest | Lower |
| `ggml-base.en` | 142MB | Fast | Good |
| `ggml-small.en` | 466MB | Moderate | Better |
| `ggml-medium.en` | 1.5GB | Slower | High |
| `ggml-large-v3` | 3.1GB | Slowest | Best (multilingual) |

English-only (`.en`) models are faster and more accurate for English.

## AI Orchestrator Integration

When the AI Orchestrator is running, speech-to-text is available via:
- `POST /api/transcribe` — automatic routing to the best whisper instance
