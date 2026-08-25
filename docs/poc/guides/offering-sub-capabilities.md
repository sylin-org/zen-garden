# Sub-Capabilities Guide

Sub-capabilities are runtime-discovered features within offerings. For example, Ollama exposes its downloaded models, PostgreSQL might expose installed extensions, and Redis could expose loaded modules.

## Current Status

| Operation | Status | CLI | API |
|-----------|--------|-----|-----|
| **List** | Implemented | `rake capabilities <offering>` | `GET .../capabilities` |
| **Add** | Implemented | `rake capabilities add <offering> <name>` | `POST .../capabilities` |
| **Remove** | Implemented | `rake capabilities remove <offering> <name>` | `DELETE .../capabilities/:name` |
| **Mirror** | Implemented | `rake capabilities <offering> mirror from <stone> to <stone>` | `POST .../capabilities/mirror` |

## Quick Start

```bash
# List capabilities for an offering
rake capabilities ollama

# Add a capability (e.g., pull a model)
rake capabilities add ollama llama3

# Remove a capability
rake capabilities remove ollama phi

# Mirror capabilities between stones (same offering instance)
rake capabilities ollama mirror from stone-01 to stone-02

# Via API
curl http://localhost:7185/api/v1/stone/offerings/ollama/capabilities
```

## Concepts

### Naming Rule

`capability` is the protocol-level term. Offering-specific labels such as `model`, `extension`, or `module` are manifest aliases for display and UX only.

### What are Sub-Capabilities?

Sub-capabilities represent dynamic, runtime features of an offering:

| Offering | Capability Type | Examples |
|----------|----------------|----------|
| Ollama | models | llama2:7b, mistral:latest |
| PostgreSQL | extensions | pgvector, postgis |
| Redis | modules | RediSearch, RedisJSON |
| Elasticsearch | plugins | analysis-icu, mapper-size |

### How Discovery Works

1. **Manifest-Driven**: Each offering has a `*.capabilities.yaml` manifest defining how to discover capabilities
2. **Mode-Aware**: Different commands for managed (container) vs adopted (native) installations
3. **Platform-Specific**: Commands can vary by OS (Linux, Windows, macOS)
4. **Automatic Transformation**: Raw output is transformed to a consistent format

## CLI Usage

### List Capabilities

```bash
# Basic usage
rake capabilities <offering[::instance]>

# Examples
rake capabilities ollama
rake capabilities ollama::dev
rake capabilities postgresql
rake capabilities redis

# Target a specific stone
rake capabilities ollama --at my-stone
```

**Example: List Ollama models**

```bash
$ rake capabilities ollama

OLLAMA CAPABILITIES (adopted)

  MODELS (4)

    llama3.1:8b-instruct-q6_K                  6.1 GB
    deepseek-r1:32b                           18.5 GB
    mistral:latest                             4.1 GB
    llama2:latest                              3.6 GB
```

**Example: List PostgreSQL extensions**

```bash
$ rake capabilities postgresql

POSTGRESQL CAPABILITIES (managed)

  EXTENSIONS (3)

    pgvector                                   v0.7.0   public
    uuid-ossp                                  v1.1     public
    pg_stat_statements                         v1.10    public
```

### Add Capability

```bash
# Add a capability (e.g., pull a model)
rake capabilities add <offering[::instance]> <name>

# Examples
rake capabilities add ollama llama3
rake capabilities add ollama deepseek-r1:8b
rake capabilities add ollama mistral:7b-instruct

# With capability type (if offering has multiple types)
rake capabilities add ollama llama3 --type model

# Target a specific stone
rake capabilities add ollama llama3 --at my-stone
rake capabilities add ollama::dev llama3 --at my-stone
```

### Remove Capability

```bash
# Remove a capability
rake capabilities remove <offering[::instance]> <name>

# Examples
rake capabilities remove ollama phi
rake capabilities remove ollama llama2:7b

# Target a specific stone
rake capabilities remove ollama phi --at my-stone
rake capabilities remove ollama::dev phi --at my-stone
```

### Find by Capability

Use `rake find` to discover services that have specific capabilities:

```bash
# Find ollama instances that have the llama2 model
rake find ollama[llama2]

# Find ollama instances that have BOTH models
rake find ollama[llama2,mistral]

# Find any service with a specific model (garden-wide)
rake find model:llama2

# Find any service with a capability (any type)
rake find cap:llama2

# Find with connection string output
rake find ollama[mistral] --format uri
```

**Supported syntaxes:**

| Syntax | Example | Description |
|--------|---------|-------------|
| `name[item]` | `ollama[llama2]` | Find offering with specific capability |
| `name[item1,item2]` | `ollama[llama2,mistral]` | Find offering with all listed capabilities (AND) |
| `model:item` | `model:llama2` | Find any service with model |
| `cap:item` | `cap:embeddings` | Generic capability search (any type) |

## API Reference

### List Capabilities

**Endpoint**: `GET /api/v1/stone/offerings/:name/capabilities`

**Parameters**:
- `name` (path): Offering FQN (e.g., "ollama", "ollama::dev")
- `refresh` (query, optional): Force fresh discovery, bypass cache

**Example Request**:
```bash
curl -s http://localhost:7185/api/v1/stone/offerings/ollama/capabilities
```

**Example Response**:
```json
{
  "data": {
    "offering": "ollama",
    "mode": "adopted",
    "capabilities": [
      {
        "type": "model",
        "display": {
          "singular": "model",
          "plural": "models"
        },
        "items": [
          {
            "name": "llama2:latest",
            "size": "3.6 GB",
            "size_bytes": 3826793677,
            "metadata": {
              "family": "llama",
              "format": "gguf",
              "parameter_size": "7B",
              "quantization": "Q4_0"
            }
          },
          {
            "name": "mistral:latest",
            "size": "4.1 GB",
            "size_bytes": 4405252096,
            "metadata": {
              "family": "mistral",
              "format": "gguf",
              "parameter_size": "7B",
              "quantization": "Q4_0"
            }
          }
        ],
        "discovered_at": "2026-02-03T04:39:01.347Z"
      }
    ]
  }
}
```

### Add Capability

**Endpoint**: `POST /api/v1/stone/offerings/:name/capabilities`

**Parameters**:
- `name` (path): Offering FQN (e.g., "ollama", "ollama::dev")

**Request Body**:
```json
{
  "name": "llama3",
  "type": "model"  // optional, defaults to first capability type
}
```

**Example Request**:
```bash
curl -X POST http://localhost:7185/api/v1/stone/offerings/ollama/capabilities \
  -H "Content-Type: application/json" \
  -d '{"name": "llama3"}'
```

**Example Response**:
```json
{
  "data": {
    "success": true,
    "capability": "llama3",
    "operation": "add"
  }
}
```

### Remove Capability

**Endpoint**: `DELETE /api/v1/stone/offerings/:name/capabilities/:capability`

**Parameters**:
- `name` (path): Offering FQN (e.g., "ollama", "ollama::dev")
- `capability` (path): Capability name to remove (e.g., "llama3")
- `type` (query, optional): Capability type

**Example Request**:
```bash
curl -X DELETE http://localhost:7185/api/v1/stone/offerings/ollama/capabilities/phi
```

**Example Response**:
```json
{
  "data": {
    "success": true,
    "capability": "phi",
    "operation": "remove"
  }
}
```

### Related Endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /api/v1/stone/offerings` | List all offerings on stone |
| `GET /api/v1/stone/offerings/adopted` | List adopted offerings |
| `GET /api/v1/stone/offerings/adoptable` | List offerings available for adoption |
| `POST /api/v1/stone/offerings/:name/adopt` | Manually adopt an offering |

### Mirror Capabilities

**Endpoint**: `POST /api/v1/stone/offerings/:name/capabilities/mirror`

**Request Body**:
```json
{
  "from": "stone-01",
  "to": "stone-02",
  "dry_run": false
}
```

**Example CLI**:
```bash
garden-rake capabilities ollama mirror from stone-01 to stone-02
garden-rake capabilities ollama::dev mirror from stone-01 to stone-02
```

**Notes**:
- `name` is the offering FQN (instance identity)
- `::` must be URL-encoded when used directly in URLs (`ollama%3A%3Adev`)

## Offering Modes

Capabilities work with all offering modes:

### Managed Mode
Container-based offerings managed by Moss. Commands execute inside the container.

```yaml
# Example: managed mode command
managed:
  linux: "docker exec {{container_name}} curl -s http://localhost:11434/api/tags"
```

### Adopted Mode
Native installations detected and monitored by Moss. Commands execute on the host.

```yaml
# Example: adopted mode command
adopted:
  linux: "curl -s http://localhost:{{port}}/api/tags"
  windows: "curl.exe -s http://localhost:{{port}}/api/tags"
```

### Borrowed Mode
Remote services borrowed from other stones. Capability discovery not supported.

## Auto-Adoption

Moss automatically detects and adopts native installations:

1. **Fast Initial Detection**: Scans every 30 seconds for first 2 minutes
2. **Stability Threshold**: Requires 2 consecutive successful detections
3. **Normal Monitoring**: Switches to 5-minute intervals after adoption

Check adoptable offerings:
```bash
curl http://localhost:7185/api/v1/stone/offerings/adoptable
```

Manually trigger adoption:
```bash
curl -X POST http://localhost:7185/api/v1/stone/offerings/ollama/adopt \
  -H "Content-Type: application/json" \
  -d '{}'
```

## Writing Capability Manifests

Capability manifests define how to discover capabilities for an offering. Place them in `embedded/manifests/sw/<category>/<offering>.capabilities.yaml`.

### Manifest Structure

```yaml
version: "1"
offering: ollama

capabilities:
  - type: model
    display:
      singular: model
      plural: models
    mutability: hot  # Can change at runtime

    # List operation (implemented)
    list:
      commands:
        managed:
          linux: "docker exec {{container_name}} curl -s http://localhost:11434/api/tags"
        adopted:
          linux: "curl -s http://localhost:{{port}}/api/tags"
          windows: "curl.exe -s http://localhost:{{port}}/api/tags"
          macos: "curl -s http://localhost:{{port}}/api/tags"

      output_format: json
      transform:
        items_path: ".models"
        fields:
          name: ".name"
          size_bytes: ".size"
          metadata:
            family: ".details.family"
            format: ".details.format"
            parameter_size: ".details.parameter_size"
            quantization: ".details.quantization_level"

      timeout_secs: 30
      cache_ttl_secs: 300

    # Add operation
    add:
      available: true
      commands:
        managed:
          linux: "docker exec {{container_name}} ollama pull {{item}}"
        adopted:
          linux: "ollama pull {{item}}"
          windows: "ollama.exe pull {{item}}"
      timeout_secs: 7200  # Model downloads can take time (2 hours max)

    # Remove operation
    remove:
      available: true
      commands:
        managed:
          linux: "docker exec {{container_name}} ollama rm {{item}}"
        adopted:
          linux: "ollama rm {{item}}"
          windows: "ollama.exe rm {{item}}"
      timeout_secs: 60
```

### Template Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `{{container_name}}` | Container name for managed mode | `zen-offering-ollama` |
| `{{port}}` | Service port | `11434` |
| `{{host}}` | Service host | `localhost` |
| `{{item}}` | Capability name (for add/remove) | `llama3` |

### Output Formats

- `json`: Parse as JSON, use `items_path` and `fields` for transformation
- `lines`: One capability per line, use regex for parsing

## Troubleshooting

### "OFFERING_NOT_FOUND" Error

The offering isn't running or adopted. Check with:
```bash
rake list           # Managed offerings
rake adopted        # Adopted offerings
```

### "NO_CAPABILITY_MANIFEST" Error

No capability manifest exists for this offering. Not all offerings support capability discovery.

### Empty Capabilities

The offering is running but has no capabilities installed. For Ollama, pull a model:
```bash
ollama pull llama2
```

### Adopted Offering Not Detected

1. Verify the service is running: `ollama --version`
2. Check adoptable list: `curl http://localhost:7185/api/v1/stone/offerings/adoptable`
3. Wait for stability threshold (2 consecutive detections)
4. Or manually adopt: `curl -X POST .../adopt`

## See Also

- [Offering Modes](./offering-lifecycle.md) - Understanding managed, adopted, and borrowed modes
- [Tools Domain User Guide](./tools-domain.md) - Normative tools snapshot/stream and event-driven capability readiness
