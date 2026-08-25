# Zot Registry Research

## Overview

| Property | Value |
|----------|-------|
| **Official Name** | Zot |
| **Category** | Container Registry |
| **Primary Use** | Lightweight OCI-native image hosting |
| **License** | Apache 2.0 |
| **Governance** | Linux Foundation |
| **Project URL** | https://zotregistry.dev/ |
| **GitHub** | https://github.com/project-zot/zot |
| **Runtime** | Go binary |

## Why Zot?

Zot is designed from the ground up for OCI compliance:
- Native OCI distribution-spec implementation
- No legacy Docker Registry v1 baggage
- Built-in pull-through cache (mirror hub.docker.com, ghcr.io, etc.)
- Minimal resource footprint
- Excellent ARM support including ARM32

## Docker Image Analysis

### Image Selection
**Selected**: `ghcr.io/project-zot/zot-linux-amd64:v2.1.1`

Architecture-specific images are published separately:
- `ghcr.io/project-zot/zot-linux-amd64:v2.1.1`
- `ghcr.io/project-zot/zot-linux-arm64:v2.1.1`
- `ghcr.io/project-zot/zot-linux-arm:v2.1.1`

### Architecture Support

| Architecture | Supported | Image Tag |
|--------------|-----------|-----------|
| amd64 | Yes | zot-linux-amd64 |
| arm64v8 | Yes | zot-linux-arm64 |
| arm32v7 | Yes | zot-linux-arm |
| arm32v6 | No | |
| ppc64le | No | |
| s390x | No | |

Good multi-architecture support covering common platforms.

## CPU Compatibility

### No Special Requirements

Zot has **no AVX, SSE, or other SIMD requirements**.

As a Go binary, it runs on any supported architecture.

## Resource Requirements

### Memory

Zot is extremely lightweight:

| Deployment | Memory | Notes |
|------------|--------|-------|
| Minimum | 64MB | Basic operation |
| Recommended | 128-256MB | Concurrent operations |
| Production | 512MB+ | High throughput |

Memory usage scales with concurrent pull/push operations and cache size.

### CPU

| Deployment | Cores |
|------------|-------|
| Minimum | 0.5 |
| Recommended | 1 |
| Production | 2+ |

CPU usage is minimal for serving images.

### Disk

| Use Case | Storage |
|----------|---------|
| Small | 10-50GB |
| Medium | 100GB-1TB |
| Large | Multi-TB |

Storage is the primary resource consideration.

## Network Configuration

| Port | Protocol | Purpose |
|------|----------|---------|
| 5000 | HTTP | Registry API |

Using 5050 external to avoid conflicts with Docker Registry.

## Health Check Strategy

### Selected Command
```yaml
healthcheck:
  test: ["CMD", "wget", "--no-verbose", "--tries=1", "--spider", "http://localhost:5000/v2/"]
```

The `/v2/` endpoint returns 200 OK when the registry is accessible.

## Configuration

Zot uses JSON configuration:

```json
{
  "distSpecVersion": "1.1.0",
  "storage": {
    "rootDirectory": "/var/lib/zot"
  },
  "http": {
    "address": "0.0.0.0",
    "port": "5000"
  },
  "log": {
    "level": "info"
  }
}
```

## Pull-Through Cache

Zot's killer feature - mirror external registries:

```json
{
  "extensions": {
    "sync": {
      "registries": [
        {
          "urls": ["https://docker.io"],
          "onDemand": true,
          "tlsVerify": true
        }
      ]
    }
  }
}
```

## Comparison with Docker Registry

| Feature | Zot | Docker Registry |
|---------|-----|-----------------|
| License | Apache 2.0 | Apache 2.0 |
| OCI Native | Yes | Partial |
| Pull-Through | Built-in | External |
| Memory | ~64MB | ~64MB |
| ARM32 | Yes | Yes |
| UI | Optional (zui) | No |
| Config | JSON | YAML |

## Raspberry Pi Compatibility

| Device | Support | Notes |
|--------|---------|-------|
| Pi 5 | Yes | Excellent |
| Pi 4 | Yes | Great |
| Pi 3 | Yes | Good |
| Pi 2 | Yes | ARM32v7 |
| Pi Zero 2 W | Yes | ARM64 |
| Pi Zero/1 | No | ARM32v6 not supported |

## Security Considerations

| Concern | Mitigation |
|---------|------------|
| No auth by default | Configure htpasswd or token auth |
| HTTP (insecure) | Use reverse proxy with TLS |

## Usage

### Push Image
```bash
docker tag myimage localhost:5050/myimage
docker push localhost:5050/myimage
```

### Pull Image
```bash
docker pull localhost:5050/myimage
```

### List Repositories
```bash
curl http://localhost:5050/v2/_catalog
```

## Validation Checklist

- [x] Docker image exists
- [x] Multi-architecture support verified (amd64, arm64, arm32)
- [x] No CPU feature requirements
- [x] Memory constraints documented (64MB minimum)
- [x] Health check endpoint verified (/v2/)
- [x] Apache 2.0 license confirmed

## Files

| File | Status |
|------|--------|
| `zot.snippet.yaml` | Created |
| `zot.compatibility.yaml` | Created |
| `zot.frontmatter.json` | Created |
| `zot.research.md` | Created |

## References

1. [Zot Documentation](https://zotregistry.dev/v2.1.1/)
2. [Zot GitHub](https://github.com/project-zot/zot)
3. [OCI Distribution Spec](https://github.com/opencontainers/distribution-spec)
4. [Zot Configuration](https://zotregistry.dev/v2.1.1/admin-guide/admin-configuration/)
