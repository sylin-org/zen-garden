# LibreTranslate — Research Notes

## Source

- **Repository:** https://github.com/LibreTranslate/LibreTranslate
- **PyPI:** `libretranslate`
- **License:** AGPL-3.0
- **Translation engine:** Argos Translate (CTranslate2 backend)

## Installation

### pip (native)
```
pip install libretranslate
libretranslate --host 0.0.0.0 --port 5000
```

On Windows: `libretranslate.exe` (pip-installed script wrapper).

### Docker
- `libretranslate/libretranslate:latest` (CPU, multi-arch: amd64 + arm64)
- `libretranslate/libretranslate:latest-cuda` (GPU, amd64 only)

## First-Run Behavior

On first startup, LibreTranslate auto-downloads language models from the
Argos Translate package index. This can take 5-15 minutes depending on
the number of language pairs and network speed.

- `--load-only en,fr,de,es` restricts to specific languages
- `--update-models` updates to latest model versions
- Models stored in platform app data dir (`~/.local/share/argos-translate/` on Linux)
- `--force-update-models` reinstalls all packages
- Also downloads MiniSBD sentence segmentation models

## API Surface

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/translate` | Translate text |
| POST | `/translate_file` | Translate uploaded file |
| POST | `/detect` | Detect language |
| GET | `/languages` | List supported languages |
| GET | `/health` | Health check |
| POST | `/suggest` | Submit translation suggestion |
| GET | `/spec` | OpenAPI spec |
| GET | `/metrics` | Prometheus metrics |
| GET | `/` | Web UI |

### GET /health

Returns `{"status": "ok"}`.

### GET /languages

```json
[
  {"code": "en", "name": "English", "targets": ["es", "fr", "de", ...]},
  {"code": "es", "name": "Spanish", "targets": ["en", "fr", ...]}
]
```

### POST /translate

Request:
```json
{
  "q": "Hello world",
  "source": "en",
  "target": "es",
  "format": "text",
  "alternatives": 0
}
```

`q` can be a string or array of strings. `source` can be `"auto"` for detection.

Response:
```json
{
  "translatedText": "Hola mundo",
  "detectedLanguage": {"confidence": 98.5, "language": "en"}
}
```

### POST /detect

Request: `{"q": "Bonjour"}`
Response: `[{"confidence": 95.2, "language": "fr"}]`

## Default Port

**5000** (configurable via `--port`). Default host: `127.0.0.1`.

## CLI Entry Points

- `libretranslate` → `libretranslate.main:main` (server)
- `ltmanage` → `libretranslate.manage:manage` (API key management)

On Windows: `libretranslate.exe` (NOT `python -m libretranslate`).

## Process Detection

pip-installed entry point: process name is `libretranslate` (or `libretranslate.exe`).

## Key CLI Flags

- `--host` — bind address (default: 127.0.0.1)
- `--port` — listen port (default: 5000)
- `--load-only` — comma-separated language codes to load
- `--threads` — number of threads (default: 4)
- `--char-limit` — max characters per request
- `--req-limit` — request rate limit
- `--api-keys` — enable API key auth
- `--disable-web-ui` — headless mode

## Key Environment Variables (LT_ prefix)

`LT_HOST`, `LT_PORT`, `LT_LOAD_ONLY`, `LT_THREADS`, `LT_CHAR_LIMIT`,
`LT_REQ_LIMIT`, `LT_API_KEYS`, `LT_DEBUG`, `LT_DISABLE_WEB_UI`,
`LT_UPDATE_MODELS`, `LT_SUGGESTIONS`

## WSGI Server

Production: `waitress.serve()`. Debug: Flask `run_simple()`.

## Orchestrator Adapter Requirements

1. **Probe:** `GET /health` — check for `{"status":"ok"}`
2. **Enumerate:** `GET /languages` — returns available language pairs
3. **Proxy:** Forward `POST /translate` (custom format, NOT OpenAI-compatible)
4. **Health:** `GET /health`
5. **No model sync** — Argos Translate manages its own model downloads
6. **Benchmark:** Translate N reference sentences, measure latency per pair
