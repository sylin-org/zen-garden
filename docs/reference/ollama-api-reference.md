# Ollama API Reference (for Zen Garden)

**Verified against:** [Official Ollama API docs](https://github.com/ollama/ollama/blob/main/docs/api.md)  
**Date verified:** 2026-02-18  
**Referenced by:** [ORCH-0002: AI Capability Router](../proposals/offering-orchestration/ORCH-0002-ai-capability-router.md)

> This document captures the Ollama API surface relevant to the AI Capability Router
> and any other Zen Garden component that interacts with Ollama instances. It is the
> single source of truth for endpoint contracts, field names, and response schemas.

---

## Conventions

- **Base URL**: `http://<host>:11434` (default Ollama port)
- **All durations** are in **nanoseconds**
- **Streaming** uses **newline-delimited JSON (NDJSON)** — each line is a complete JSON object. NOT Server-Sent Events.
- **Model names** follow `model:tag` format (e.g., `llama3.2:latest`, `mistral:7b`). Tag defaults to `latest` if omitted.
- Streaming can be disabled on streaming endpoints by sending `{"stream": false}`

---

## Endpoints

### Inference

| Endpoint | Method | Model Field | Streaming | Description |
|----------|--------|-------------|-----------|-------------|
| `/api/generate` | POST | `model` | Yes (NDJSON) | Text completion |
| `/api/chat` | POST | `model` | Yes (NDJSON) | Chat completion (multi-turn, tools) |
| `/api/embed` | POST | `model` | No | Generate embeddings (current) |
| `/api/embeddings` | POST | `model` | No | Generate embeddings (**deprecated**, use `/api/embed`) |

### Model Management

| Endpoint | Method | Key Fields | Streaming | Description |
|----------|--------|------------|-----------|-------------|
| `/api/pull` | POST | `model` | Yes | Pull model from registry |
| `/api/delete` | DELETE | `model` | No | Delete model. Returns 200 or 404 |
| `/api/create` | POST | `model`, `from` | Yes | Create from existing model, GGUF, or safetensors |
| `/api/copy` | POST | `source`, `destination` | No | Copy/rename model. Returns 200 or 404 |
| `/api/show` | POST | `model` | No | Show model details, metadata, capabilities |

### Discovery

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/tags` | GET | List all locally available models |
| `/api/ps` | GET | List models currently loaded in memory |
| `/api/version` | GET | Ollama version string |

### Blob Management

| Endpoint | Method | Description |
|----------|--------|-------------|
| `HEAD /api/blobs/:digest` | HEAD | Check if blob exists. Returns 200 or 404 |
| `POST /api/blobs/:digest` | POST | Upload blob (for GGUF/safetensors model creation) |

---

## Request Schemas

### `POST /api/generate`

```json
{
  "model": "llama3.2",
  "prompt": "Why is the sky blue?",
  "suffix": "",
  "images": [],
  "think": false,
  "format": "json",
  "options": {
    "temperature": 0.8,
    "seed": 42,
    "num_predict": 100,
    "num_ctx": 1024,
    "top_k": 20,
    "top_p": 0.9,
    "stop": ["\n", "user:"]
  },
  "system": "You are a helpful assistant.",
  "template": "",
  "stream": true,
  "raw": false,
  "keep_alive": "5m"
}
```

All fields except `model` are optional.

**Special cases:**
- Empty `prompt` → loads the model into memory
- Empty `prompt` + `"keep_alive": 0` → unloads the model from memory

### `POST /api/chat`

```json
{
  "model": "llama3.2",
  "messages": [
    { "role": "system", "content": "You are helpful." },
    { "role": "user", "content": "Why is the sky blue?" },
    { "role": "assistant", "content": "Due to Rayleigh scattering." },
    { "role": "user", "content": "Explain more." }
  ],
  "tools": [
    {
      "type": "function",
      "function": {
        "name": "get_weather",
        "description": "Get the weather in a city",
        "parameters": {
          "type": "object",
          "properties": {
            "city": { "type": "string", "description": "The city name" }
          },
          "required": ["city"]
        }
      }
    }
  ],
  "think": false,
  "format": "json",
  "options": {},
  "stream": true,
  "keep_alive": "5m"
}
```

**Message roles:** `system`, `user`, `assistant`, `tool`

**Tool response messages** include `tool_name`:
```json
{ "role": "tool", "content": "11 degrees celsius", "tool_name": "get_weather" }
```

**Special cases:**
- Empty `messages` array → loads the model
- Empty `messages` + `"keep_alive": 0` → unloads the model

### `POST /api/embed`

```json
{
  "model": "all-minilm",
  "input": "Why is the sky blue?",
  "truncate": true,
  "options": {},
  "keep_alive": "5m",
  "dimensions": 384
}
```

`input` can be a string or an array of strings for batch embeddings.

### `POST /api/embeddings` (deprecated)

```json
{
  "model": "all-minilm",
  "prompt": "Here is an article about llamas...",
  "options": {},
  "keep_alive": "5m"
}
```

> Note: Superseded by `/api/embed`. The field is `prompt` (singular string), not `input`.

### `POST /api/pull`

```json
{
  "model": "llama3.2",
  "insecure": false,
  "stream": true
}
```

### `DELETE /api/delete`

```json
{
  "model": "llama3:13b"
}
```

### `POST /api/show`

```json
{
  "model": "llava",
  "verbose": false
}
```

### `POST /api/copy`

```json
{
  "source": "llama3.2",
  "destination": "llama3-backup"
}
```

### `POST /api/create`

```json
{
  "model": "mario",
  "from": "llama3.2",
  "system": "You are Mario from Super Mario Bros.",
  "stream": true
}
```

Can also create from GGUF (`files` dict) or safetensors (`files` dict). Optional `quantize` field for float16→quantized conversion (values: `q4_K_M`, `q4_K_S`, `q8_0`).

---

## Response Schemas

### Streaming Inference Response (generate / chat)

Each chunk is a JSON object on its own line:

**Generate — intermediate chunk:**
```json
{
  "model": "llama3.2",
  "created_at": "2023-08-04T08:52:19.385406455-07:00",
  "response": "The",
  "done": false
}
```

**Chat — intermediate chunk:**
```json
{
  "model": "llama3.2",
  "created_at": "2023-08-04T08:52:19.385406455-07:00",
  "message": {
    "role": "assistant",
    "content": "The"
  },
  "done": false
}
```

**Chat — tool call chunk:**
```json
{
  "model": "llama3.2",
  "created_at": "2025-07-07T20:22:19.184789Z",
  "message": {
    "role": "assistant",
    "content": "",
    "tool_calls": [
      {
        "function": {
          "name": "get_weather",
          "arguments": { "city": "Tokyo" }
        }
      }
    ]
  },
  "done": false
}
```

### Final Inference Object (both generate and chat)

```json
{
  "model": "llama3.2",
  "created_at": "2023-08-04T19:22:45.499127Z",
  "response": "",
  "done": true,
  "done_reason": "stop",
  "total_duration": 10706818083,
  "load_duration": 6338219291,
  "prompt_eval_count": 26,
  "prompt_eval_duration": 130079000,
  "eval_count": 259,
  "eval_duration": 4232710000
}
```

| Field | Type | Description |
|-------|------|-------------|
| `done` | bool | Always `true` on final object |
| `done_reason` | string | `"stop"`, `"load"`, `"unload"` |
| `total_duration` | u64 | Total time (ns) including load |
| `load_duration` | u64 | Time (ns) spent loading model |
| `prompt_eval_count` | u32 | Number of input tokens |
| `prompt_eval_duration` | u64 | Time (ns) evaluating prompt |
| `eval_count` | u32 | Number of output tokens generated |
| `eval_duration` | u64 | Time (ns) generating output tokens |

**Tokens/sec** = `eval_count / eval_duration × 10⁹`

### Non-streaming Inference Response

When `stream: false`, the full response is returned as a single JSON object with all fields from both the intermediate chunks and the final object combined.

For generate: `response` contains the full text.  
For chat: `message.content` contains the full text.

### Embed Response

```json
{
  "model": "all-minilm",
  "embeddings": [
    [0.010071029, -0.0017594862, 0.05007221, ...]
  ],
  "total_duration": 14143917,
  "load_duration": 1019500,
  "prompt_eval_count": 8
}
```

Multiple inputs return multiple embedding vectors in the `embeddings` array.

### `GET /api/tags` — List Local Models

```json
{
  "models": [
    {
      "name": "deepseek-r1:latest",
      "model": "deepseek-r1:latest",
      "modified_at": "2025-05-10T08:06:48.639712648-07:00",
      "size": 4683075271,
      "digest": "0a8c26691023...",
      "details": {
        "parent_model": "",
        "format": "gguf",
        "family": "qwen2",
        "families": ["qwen2"],
        "parameter_size": "7.6B",
        "quantization_level": "Q4_K_M"
      }
    }
  ]
}
```

| Field | Description |
|-------|-------------|
| `name` | Model name with tag |
| `size` | Size on disk in bytes (**not VRAM** — use as fallback only) |
| `digest` | SHA256 digest of model manifest |
| `details.parameter_size` | Human-readable parameter count ("7.6B") |
| `details.quantization_level` | Quantization type ("Q4_K_M", "Q4_0", etc.) |
| `details.family` | Model family ("llama", "qwen2", etc.) |
| `details.families` | Array of model families |

### `GET /api/ps` — List Running Models

```json
{
  "models": [
    {
      "name": "mistral:latest",
      "model": "mistral:latest",
      "size": 5137025024,
      "digest": "2ae6f6dd7a3d...",
      "details": {
        "parent_model": "",
        "format": "gguf",
        "family": "llama",
        "families": ["llama"],
        "parameter_size": "7.2B",
        "quantization_level": "Q4_0"
      },
      "expires_at": "2024-06-04T14:38:31.83753-07:00",
      "size_vram": 5137025024
    }
  ]
}
```

| Field | Description |
|-------|-------------|
| `size_vram` | **Exact VRAM consumption in bytes.** Authoritative source for VRAM-aware routing. |
| `expires_at` | When Ollama will auto-unload this model (based on `keep_alive`). Enables proactive routing. |
| All `details.*` fields | Same as `/api/tags` |

### `POST /api/show` — Model Information

```json
{
  "modelfile": "...",
  "parameters": "num_keep 24\nstop \"<|start_header_id|>\"...",
  "template": "{{ if .System }}...",
  "details": {
    "parent_model": "",
    "format": "gguf",
    "family": "llama",
    "families": ["llama"],
    "parameter_size": "8.0B",
    "quantization_level": "Q4_0"
  },
  "model_info": {
    "general.architecture": "llama",
    "general.file_type": 2,
    "general.parameter_count": 8030261248,
    "general.quantization_version": 2,
    "llama.attention.head_count": 32,
    "llama.attention.head_count_kv": 8,
    "llama.block_count": 32,
    "llama.context_length": 8192,
    "llama.embedding_length": 4096,
    "llama.feed_forward_length": 14336,
    "llama.vocab_size": 128256
  },
  "capabilities": ["completion", "vision"]
}
```

| Field | Description |
|-------|-------------|
| `model_info.general.parameter_count` | Exact parameter count (for VRAM estimation of unloaded models) |
| `model_info.*.context_length` | Max context length |
| `capabilities` | What the model can do: `"completion"`, `"vision"`, `"tools"`, etc. |

### Pull Progress Stream

```json
{"status": "pulling manifest"}
{"status": "pulling digestname", "digest": "digestname", "total": 2142590208, "completed": 241970}
{"status": "verifying sha256 digest"}
{"status": "writing manifest"}
{"status": "removing any unused layers"}
{"status": "success"}
```

Progress percentage = `completed / total`. The `completed` field may be absent before any data is downloaded. Cancelled pulls resume automatically on retry.

### Create Progress Stream

```json
{"status": "reading model metadata"}
{"status": "creating system layer"}
{"status": "using already created layer sha256:22f7f8ef5f4c..."}
{"status": "writing layer sha256:df30045fe90f..."}
{"status": "writing manifest"}
{"status": "success"}
```

When quantizing: `{"status": "quantizing F16 model to Q4_K_M", "digest": "0", "total": 6433687776, "completed": 12302}`

---

## Load / Unload Patterns

### Load a model into VRAM

```bash
# via generate
curl http://localhost:11434/api/generate -d '{"model": "llama3.2"}'
# Response: {"model":"llama3.2","done":true,"done_reason":"load"}

# via chat
curl http://localhost:11434/api/chat -d '{"model": "llama3.2", "messages": []}'
# Response: {"model":"llama3.2","done":true,"done_reason":"load"}
```

### Unload a model from VRAM

```bash
# via generate
curl http://localhost:11434/api/generate -d '{"model": "llama3.2", "keep_alive": 0}'
# Response: {"model":"llama3.2","done":true,"done_reason":"unload"}

# via chat
curl http://localhost:11434/api/chat -d '{"model": "llama3.2", "messages": [], "keep_alive": 0}'
# Response: {"model":"llama3.2","done":true,"done_reason":"unload"}
```

---

## Version

```bash
curl http://localhost:11434/api/version
# {"version": "0.5.1"}
```

---

## Error Behavior

- Model not found: HTTP 404 with `{"error": "model 'xyz' not found"}`
- Model deleted while loaded: subsequent requests will fail
- Pull interrupted: resumes automatically on retry (same model name)
- Blob missing for create: HTTP 400
