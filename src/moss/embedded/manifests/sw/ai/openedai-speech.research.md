# OpenedAI Speech — Research Notes

## Source

- **Repository:** https://github.com/matatonic/openedai-speech
- **License:** AGPL-3.0
- **Server source:** `speech.py` (application), `openedai.py` (OpenAI API stub base)

## Two Installation Modes

### Full (XTTS v2 + Piper)
- Requires NVIDIA GPU (~4GB VRAM for XTTS)
- `pip install -r requirements.txt`
- **Broken upstream:** `coqui-tts 0.27.5` has `transformers` import incompatibility
- Docker image: `ghcr.io/matatonic/openedai-speech` (~8GB)

### Minimal (Piper only)
- CPU-only, no GPU needed
- `pip install -r requirements-min.txt`
- Piper ONNX voices (~60-75MB each)
- Docker image: `ghcr.io/matatonic/openedai-speech-min` (~860MB)
- **This is the stable, working option** as of 2026-03

## API Surface

Fully OpenAI-compatible TTS API:

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/v1/audio/speech` | Generate speech (streaming audio) |
| GET | `/v1/models` | List registered models |
| GET | `/v1/models/{model}` | Model info |
| GET | `/health` | Health check |
| GET/HEAD | `/` | Returns 200 if models loaded, 503 otherwise |

### POST /v1/audio/speech

```json
{
  "model": "tts-1",
  "input": "Text to speak",
  "voice": "alloy",
  "response_format": "mp3",
  "speed": 1.0
}
```

Response: Streaming audio bytes. Content types: `audio/mpeg` (mp3), `audio/ogg;codec=opus`,
`audio/aac`, `audio/x-flac`, `audio/wav`, `audio/pcm;rate=22050`.

### GET /health

Returns `{"status": "ok"}` when models are registered, `{"status": "unk"}` otherwise.

### GET /v1/models

OpenAI-compatible format:
```json
{
  "object": "list",
  "data": [
    {"id": "tts-1", "object": "model", "created": 0, "owned_by": "user"},
    {"id": "tts-1-hd", "object": "model", "created": 0, "owned_by": "user"}
  ]
}
```

## Default Voices (Piper, tts-1)

From `voice_to_speaker.default.yaml`:
- `alloy` — `en_US-libritts_r-medium.onnx` speaker 79
- `echo` — `en_US-libritts_r-medium.onnx` speaker 134
- `fable` — `en_GB-northern_english_male-medium.onnx`
- `onyx` — `en_US-libritts_r-medium.onnx` speaker 159
- `nova` — `en_US-libritts_r-medium.onnx` speaker 107
- `shimmer` — `en_US-libritts_r-medium.onnx` speaker 163

## Model Selection Logic

- `model == "tts-1"` OR `xtts_device == "none"` → Piper (CPU, fast)
- `model == "tts-1-hd"` AND `xtts_device != "none"` → XTTS (GPU, high quality)
- `--xtts_device none` forces Piper for all requests

## Default Port

**8000** (configurable via `-P` / `--port`)
Note: User's deployment uses 8001 to avoid conflict with whisper.cpp on 8000.

## Process Name / Native Run

- `python speech.py` — process appears as `python` with `speech.py` in args
- No standalone binary, no pip-installable CLI
- Requires launcher script on Windows (`start.bat`) that sets PATH for `piper.exe` and `ffmpeg.exe`

## Windows PATH Issue

`subprocess.Popen` for `piper.exe` inherits the Windows process PATH, not bash PATH.
The `start.bat` launcher adds `venv\Scripts` to PATH before launching `speech.py` so
that `piper.exe` and `ffmpeg.exe` (which are pip-installed into the venv) can be found.

## Orchestrator Adapter Requirements

1. **Probe:** `GET /health` — check for `{"status":"ok"}`
2. **Enumerate:** `GET /v1/models` — returns tts-1, tts-1-hd
3. **Proxy:** Forward `POST /v1/audio/speech` (already OpenAI-compatible)
4. **Health:** `GET /health`
5. **No model sync** — voices are pre-installed, not dynamically downloadable
6. **Benchmark:** Generate a reference phrase, measure time-to-first-byte + total duration
