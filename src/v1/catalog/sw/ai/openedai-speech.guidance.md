---
version: "1"
trigger: post_install
---

# OpenedAI Speech — Text-to-Speech

**API:** http://{{server-name}}:{{port}}

## OpenAI-Compatible Endpoint

```bash
# Generate speech
curl -X POST http://{{server-name}}:{{port}}/v1/audio/speech \
  -H "Content-Type: application/json" \
  -d '{"model": "tts-1", "input": "Hello from Zen Garden", "voice": "alloy"}' \
  --output speech.mp3
```

## Voice Backends

| Backend | GPU | Quality | Speed | Image |
|---------|-----|---------|-------|-------|
| XTTS v2 | Required (~4GB VRAM) | High (cloneable) | Moderate | `openedai-speech` |
| Piper | CPU only | Good | Fast | `openedai-speech-min` |

## Custom Voices

Place WAV samples (6-30 seconds, clear speech) in the voices volume to create custom cloned voices with XTTS v2.

## AI Orchestrator Integration

When the AI Orchestrator is running, text-to-speech is available via:
- `POST /api/speak` — text-to-speech with automatic instance routing
