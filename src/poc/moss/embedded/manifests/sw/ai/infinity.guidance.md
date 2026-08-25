---
version: "1"
trigger: post_install
---

# Infinity — Embedding & Reranking Server

**API:** http://{{server-name}}:{{port}}
**Docs:** http://{{server-name}}:{{port}}/docs (Swagger)

## Endpoints

| Endpoint | Purpose |
|----------|---------|
| `POST /embeddings` | Generate embeddings (OpenAI-compatible) |
| `POST /rerank` | Rerank documents by relevance |
| `GET /models` | List loaded models |
| `GET /health` | Health check |

## Quick Test

```bash
# Generate embeddings
curl -X POST http://{{server-name}}:{{port}}/embeddings \
  -H "Content-Type: application/json" \
  -d '{"model": "BAAI/bge-small-en-v1.5", "input": ["Hello world"]}'
```

## Loading Multiple Models

To load multiple models simultaneously, update the container command:

```
command: ["v2",
  "--model-id", "BAAI/bge-small-en-v1.5",
  "--model-id", "mixedbread-ai/mxbai-rerank-xsmall-v1",
  "--port", "7997"]
```

Models share GPU VRAM. Monitor usage via `GET /health`.

## AI Orchestrator Integration

When the AI Orchestrator is running, embedding and reranking are available via:
- `POST /api/embed` — multimodal embeddings
- `POST /api/rerank` — document reranking
