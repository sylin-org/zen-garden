# Infinity — Research Notes

## Source

- **Repository:** https://github.com/michaelfeil/infinity
- **PyPI:** `infinity-emb` (with extras: `[all]`, `[server]`, `[torch]`, `[optimum]`, `[onnxruntime]`)
- **License:** MIT
- **Docker Hub:** `michaelf34/infinity`

## Installation

### pip (native)
```
pip install "infinity-emb[server,torch]"
```

Full install: `pip install "infinity-emb[all]"` — includes torch + optimum + onnxruntime.

**Known issue:** `[all]` extras may cause `optimum.bettertransformer` conflicts with newer
transformers. Use `[server,torch]` and set `INFINITY_DISABLE_OPTIMUM=1` or
`--no-bettertransformer` flag.

### Docker
- `michaelf34/infinity:latest` (CUDA)
- `michaelf34/infinity:latest-cpu`
- `michaelf34/infinity:latest-rocm` (AMD MI200/MI300)
- Version-pinned: `michaelf34/infinity:x.x.x`

## CLI Entry Point

Console script: `infinity_emb`

```
infinity_emb v2 --model-id BAAI/bge-small-en-v1.5 --port 7997
```

Bare `infinity_emb` (no subcommand) defaults to `v2`. The `v1` subcommand is deprecated.

## API Surface

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/health` | Health check |
| GET | `/models` | List loaded models (OpenAI format) |
| POST | `/embeddings` | Compute embeddings (OpenAI-compatible, multimodal) |
| POST | `/rerank` | Rerank documents |
| POST | `/classify` | Text classification |

### GET /health

Returns `{"unix": 1711234567.89}` (timestamp float). HTTP 200 = healthy.

### GET /models

OpenAI-compatible:
```json
{
  "object": "list",
  "data": [{"id": "BAAI/bge-small-en-v1.5", "object": "model", ...}]
}
```

### POST /embeddings (OpenAI-compatible)

```json
{
  "input": "text to embed",
  "model": "BAAI/bge-small-en-v1.5",
  "encoding_format": "float",
  "modality": "text"
}
```

Response:
```json
{
  "object": "list",
  "data": [{"embedding": [0.1, 0.2, ...], "index": 0, "object": "embedding"}],
  "model": "BAAI/bge-small-en-v1.5",
  "usage": {"prompt_tokens": 5, "total_tokens": 5}
}
```

Multimodal: set `modality` to `"image"` or `"audio"` with appropriate input format.

### POST /rerank

```json
{
  "query": "search query",
  "documents": ["doc1", "doc2"],
  "return_documents": false,
  "raw_scores": false,
  "model": "model-name"
}
```

Response:
```json
{
  "object": "rerank",
  "results": [{"relevance_score": 0.95, "index": 0}],
  "model": "model-name",
  "usage": {"prompt_tokens": 10, "total_tokens": 10}
}
```

## Default Port

**7997** (configurable via `--port` or `INFINITY_PORT`)

## Key Environment Variables

All prefixed `INFINITY_`:
- `INFINITY_MODEL_ID` — semicolon-separated model IDs
- `INFINITY_PORT` — listen port
- `INFINITY_HOST` — bind address
- `INFINITY_BATCH_SIZE` — batch processing size
- `INFINITY_API_KEY` — authentication
- `INFINITY_ENGINE` — `torch`, `optimum`, `ctranslate2`
- `INFINITY_DEVICE` — `cuda`, `cpu`, `auto`
- `INFINITY_BETTERTRANSFORMER` — `true`/`false`
- `INFINITY_DISABLE_OPTIMUM` — set to `1` to skip optimum

## Process Detection

pip-installed entry point: process name is `infinity_emb`.

## Orchestrator Adapter Requirements

1. **Probe:** `GET /health` — check for 200 + JSON with `unix` key
2. **Enumerate:** `GET /models` — returns loaded model list
3. **Proxy (embed):** Forward `POST /embeddings` (already OpenAI-compatible)
4. **Proxy (rerank):** Forward `POST /rerank`
5. **No model sync** — models specified at startup via CLI/env, downloaded from HuggingFace on first load
6. **Benchmark:** Embed N texts, measure throughput (texts/sec). Rerank N queries, measure latency.
