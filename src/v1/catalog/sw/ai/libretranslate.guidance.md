---
version: "1"
trigger: post_install
---

# LibreTranslate — Machine Translation

**Web UI:** http://{{server-name}}:{{port}}
**API:** http://{{server-name}}:{{port}}

## Endpoints

| Endpoint | Purpose |
|----------|---------|
| `POST /translate` | Translate text |
| `GET /languages` | List available languages |
| `GET /health` | Health check |
| `POST /detect` | Detect language |

## Quick Test

```bash
# Translate English to French
curl -X POST http://{{server-name}}:{{port}}/translate \
  -H "Content-Type: application/json" \
  -d '{"q": "Hello, how are you?", "source": "en", "target": "fr"}'
```

## Language Configuration

By default, 10 languages are loaded. Change `LT_LOAD_ONLY` to adjust:

```
# Minimal (fast startup, low memory)
LT_LOAD_ONLY=en,fr,de

# Full European set
LT_LOAD_ONLY=en,fr,de,es,pt,it,nl,pl,ru

# Asian languages
LT_LOAD_ONLY=en,ja,zh,ko
```

Each language pair uses ~100-200MB RAM. Fewer languages = faster startup.

## AI Orchestrator Integration

When the AI Orchestrator is running, translation is available via:
- `POST /api/translate` — automatic routing to the best LibreTranslate instance
