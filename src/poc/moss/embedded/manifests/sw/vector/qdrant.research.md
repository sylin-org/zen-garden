# Qdrant Research

## Overview

| Property | Value |
|----------|-------|
| **Official Name** | Qdrant |
| **Category** | Vector Database |
| **Primary Use** | Vector search, semantic similarity, RAG retrieval |
| **License** | Apache 2.0 |
| **Project URL** | https://qdrant.tech/ |
| **GitHub** | https://github.com/qdrant/qdrant |
| **Docker Hub** | https://hub.docker.com/r/qdrant/qdrant |
| **Implementation** | Rust |

## Docker Image Analysis

### Image Selection
**Selected**: `qdrant/qdrant:v1.18.0`

Pinned to a specific patch version (rather than `latest` or a floating `v1.18` tag) so that re-planting an offering yields a reproducible install. Qdrant follows semver and ships patch releases monthly; bump the tag here when validating a newer version on representative hardware.

### Tag Variants (verified on Docker Hub, 2026-05-11 release set)

| Tag | When to use |
|-----|-------------|
| `v1.18.0` | Default — Debian-slim base, runs as UID 0 |
| `v1.18.0-unprivileged` | Hardened deployments — runs as non-root UID |
| `v1.18.0-gpu-nvidia` | NVIDIA CUDA acceleration for HNSW build |
| `v1.18.0-gpu-amd` | AMD ROCm acceleration |

The default snippet ships the plain `v1.18.0` tag. GPU variants belong in a sibling offering (`qdrant-gpu`) if/when hardware-tiered placement is added — they pull large CUDA/ROCm runtime layers and would force GPU-less stones to download a multi-gigabyte image they cannot use.

### Architecture Support

| Architecture | Supported | Notes |
|--------------|-----------|-------|
| amd64 (x86_64) | ✅ | Primary platform; SIMD optimizations (SSE4.2 / AVX / AVX2 / AVX-512) when host advertises them |
| arm64 (aarch64) | ✅ | Official multi-arch image; Pi 5 verified by community; ~10–20% slower than amd64 per Qdrant's own benchmarks |
| armv7 (32-bit) | ❌ | No official image; no fallback |
| armv6 | ❌ | No official image; no fallback |

**Source**: Official Dockerfile uses `xx-cargo` cross-compilation with `TARGETPLATFORM` and is published as a multi-arch manifest for `linux/amd64` and `linux/arm64`.

### Base Image and Footprint

- Base: `debian:13-slim` (Trixie) for CPU variants
- WORKDIR: `/qdrant`
- Default USER: `0:0` (root) on the plain tag, non-root on `-unprivileged`
- Image size: ~140 MB compressed (CPU tag)

## CPU Feature Requirements

### Hard Requirements
**None.** Qdrant compiles with scalar fallbacks for distance kernels and does not hard-require any SIMD instruction set. The binary will start on a stock x86_64 host with only SSE2.

### SIMD Optimizations (used when available)

| Feature | Used for | Notes |
|---------|----------|-------|
| SSE4.2 | Baseline x86_64 distance kernels | Effectively universal post-2008 |
| AVX2 | Cosine / dot-product distance, scalar quantization | Major perf uplift |
| AVX-512 | Binary quantization, newer kernels (1.13+) | Best perf on modern Intel/AMD |
| NEON | ARM64 distance kernels | Used on all aarch64 hosts |

### Practical Risk Profile

| CPU class | Risk | Notes |
|-----------|------|-------|
| Modern Intel Core / Xeon | None | Full AVX2/AVX-512 |
| AMD Ryzen / EPYC | None | Full AVX2 |
| Apple M1/M2/M3 | None | NEON used; no x86 SIMD needed |
| Raspberry Pi 5 / Pi 4 | None | NEON used |
| Celeron J/N-series (no AVX) | Low | Scalar fallback works; expect slower vector ops |
| Pre-Nehalem (no SSE4.2) | Effectively zero | These hosts are >15 years old |

Qdrant does **not** abort on missing AVX (unlike MongoDB 5.0+); the scalar path is always compiled in. This makes it a strong vector-DB choice for low-end stones (Celeron-class thin clients) where Milvus and Weaviate have soft requirements that bite.

**Sources**:
- [Qdrant — Capacity Planning](https://qdrant.tech/documentation/guides/capacity-planning/)
- [Qdrant ARM support announcement](https://qdrant.tech/blog/qdrant-supports-arm-architecture/)

## Resource Requirements

### Memory

| Workload | Memory |
|----------|--------|
| Minimum (to start) | 1 GB |
| Light (≤ 100k vectors, 768-dim) | 1–2 GB |
| Medium (1M vectors, 768-dim) | ~3 GB resident |
| Large (10M+ vectors) | 16 GB+ |

**Formula** (in-memory mode, per Qdrant docs):
```
RAM ≈ number_of_vectors × dimensions × 4 bytes × 1.5
```
The 50% margin covers index metadata, point versions, and transient segments during optimization. `on_disk: true` mode trades RAM for SSD I/O and is the standard knob for low-RAM stones.

**Sources**:
- [Qdrant — Minimal RAM article](https://qdrant.tech/articles/memory-consumption/)
- [GitHub discussion #3279 — maintainer reply: "0.5 CPU / 1 GB RAM" minimum](https://github.com/orgs/qdrant/discussions/3279)

### CPU

| Requirement | Value |
|-------------|-------|
| Minimum cores | 1 (0.5 acceptable for trivial workloads) |
| Recommended | 4+ for production query throughput |
| Scaling | Linear with concurrent query load; segment build is parallelized |

A dynamic CPU pool tunes search-worker concurrency under high I/O wait (1.18+).

### Disk

| Item | Notes |
|------|-------|
| Storage path | `/qdrant/storage` (collections, WAL, segments) |
| Snapshots | `/qdrant/snapshots` (compressed full-collection dumps) |
| Format | RocksDB-free as of 1.17; segments are mmap-backed flat files |

## Network Configuration

| Port | Protocol | Default | Purpose |
|------|----------|---------|---------|
| 6333 | TCP / HTTP | enabled | REST API + dashboard at `/dashboard` |
| 6334 | TCP / gRPC | enabled when `QDRANT__SERVICE__GRPC_PORT` is set | gRPC API (lower overhead for bulk ops) |

**Note on gRPC default**: per the Qdrant config schema, `service.grpc_port` defaults to `null` (gRPC disabled) on a bare config. The official Dockerfile exposes 6334 regardless, and the snippet sets `QDRANT__SERVICE__GRPC_PORT=6334` to actually bind it. This matches what every Qdrant SDK example assumes.

## Configuration

### Environment Variable Convention

Qdrant accepts every config key as `QDRANT__SECTION__KEY` (double-underscore separator). The env var takes precedence over the bundled `/qdrant/config/production.yaml`.

| Variable | Purpose | Default |
|----------|---------|---------|
| `QDRANT__SERVICE__HOST` | Bind address | `0.0.0.0` |
| `QDRANT__SERVICE__HTTP_PORT` | REST port | `6333` |
| `QDRANT__SERVICE__GRPC_PORT` | gRPC port (null = disabled) | `null` |
| `QDRANT__SERVICE__API_KEY` | Static bearer for full-access requests | unset |
| `QDRANT__SERVICE__READ_ONLY_API_KEY` | Static bearer for read-only requests | unset |
| `QDRANT__SERVICE__ENABLE_TLS` | TLS on REST/gRPC | `0` |
| `QDRANT__LOG_LEVEL` | `TRACE`/`DEBUG`/`INFO`/`WARN`/`ERROR` | `INFO` |
| `QDRANT__TELEMETRY_DISABLED` | Opt out of anonymous usage reporting | `false` |
| `QDRANT__STORAGE__STORAGE_PATH` | Override storage dir | `/qdrant/storage` |
| `QDRANT__STORAGE__SNAPSHOTS_PATH` | Override snapshots dir | `/qdrant/snapshots` |

**Sources**:
- [Qdrant — Configuration](https://qdrant.tech/documentation/guides/configuration/)
- [Qdrant — Security](https://qdrant.tech/documentation/guides/security/)

### Security defaults

**By default Qdrant accepts unauthenticated requests on both REST and gRPC.** This is acceptable inside the `zen-garden` Docker network (not exposed to the LAN), but any operator binding the published ports to a host interface MUST set `QDRANT__SERVICE__API_KEY`. The dashboard at `:6333/dashboard` is similarly unauthenticated by default and is a common foot-gun. Recommend pairing with `QDRANT__SERVICE__ENABLE_TLS=1` once a key is set, since the static key is sent in plaintext otherwise.

The `api-key` header carries the bearer; a separate `read_only_api_key` can be configured to permit search-only clients.

## Health Check Strategy

Qdrant exposes three Kubernetes-style probes that are **always accessible** (no API key required even when auth is configured):

| Endpoint | Purpose | Use for |
|----------|---------|---------|
| `/livez` | Process alive | Docker `liveness` |
| `/readyz` | Ready to serve queries | Docker `readiness` |
| `/healthz` | Aggregate (currently == `/livez`) | General health probe |

### Gotcha: image ships no HTTP client

This is the single most-cited Qdrant operational paper-cut. The Debian-slim base does **not** include `curl`, `wget`, or `netcat`. Issue [#3491](https://github.com/qdrant/qdrant/issues/3491) requested adding `curl`; Qdrant closed it as `not planned` (attack-surface argument). Issue [#4250](https://github.com/qdrant/qdrant/issues/4250) requested a built-in `qdrant healthcheck` subcommand — also unresolved.

**Workaround used in our snippet:** Debian's `bash` IS present (it's an `essential` package, kept even in `slim`), and bash supports the `/dev/tcp/host/port` pseudo-device. We use a TCP-open probe on 6333:

```yaml
healthcheck:
  test: ["CMD-SHELL", "bash -c '</dev/tcp/localhost/6333' || exit 1"]
```

This is a TCP-level probe, not an HTTP probe — it confirms the listener is up, not that `/readyz` is responding 200. That is good enough for "is the container alive" but does not catch a deadlocked server that holds the socket open but never responds. Moss's container-health watcher can hit `/readyz` over HTTP from outside the container for a stronger probe.

## Compatibility Rules Analysis

### Pre-flight Checks

| Rule | Condition | Action | Rationale |
|------|-----------|--------|-----------|
| `arm32-not-supported` | architecture IS armv7l/armv6l | deny | No official armv7/armv6 image |
| `insufficient-memory` | total RAM < 1024 MB | deny | Below Qdrant's own stated minimum |
| `marginal-memory` | total RAM < 2048 MB | warn | Works but expect swap pressure on real workloads |

No CPU-feature pre-flight is needed — Qdrant has scalar fallbacks for everything.

### Post-install Patterns

| Pattern | Issue | Action |
|---------|-------|--------|
| `Cannot allocate memory|OOM|out of memory` | OOM | suggestion: increase RAM or set `on_disk: true` per collection |
| `Address already in use.*633[34]` | Port collision | suggestion: free 6333/6334 or rely on well-known-ports remap |
| `Permission denied.*storage` | Volume permission | suggestion: chown storage dir to qdrant UID or use `-unprivileged` tag |
| `failed to load.*segment|corrupted` | Disk/index corruption | suggestion: restore from snapshot, check disk health |
| `Illegal instruction|SIGILL` | Theoretical only; defensive | Not expected on any supported arch |

## Cluster Mode (deferred)

Qdrant supports clustered, sharded deployments via Raft consensus (≥ v1.x). The default snippet runs a single node (`coordination: independent` in frontmatter) because:

1. Cluster mode requires explicit `cluster.enabled: true` plus peer-URI bootstrap.
2. Multi-stone Qdrant clusters would warrant a dedicated orchestrator (see `src/orchestrators/mongodb/` for the pattern), not just a manifest change.
3. Single-node Qdrant handles tens of millions of vectors comfortably on a single stone.

If/when a clustered offering is added, it would be a sibling `qdrant-cluster.snippet.yaml` with `coordination: elected` and a companion orchestrator binary.

## Comparison with Other Vector Stores in This Catalog

| Feature | Qdrant | Weaviate | Milvus | pgvector |
|---------|--------|----------|--------|----------|
| Language | Rust | Go | Go + C++ | C (extension) |
| Min RAM | 1 GB | 2 GB | 8 GB | < 1 GB |
| ARM64 | ✅ | ✅ | partial | ✅ |
| Low-end x86 (no AVX) | ✅ (scalar fallback) | risky | risky | ✅ |
| Built-in dashboard | ✅ `/dashboard` | ❌ | external | ❌ |
| gRPC | ✅ | ✅ (1.19+) | ✅ | n/a |
| Healthcheck out-of-box | ⚠️ no HTTP client | ✅ wget | ✅ curl | via psql |
| Clustering | Raft (built-in) | Raft | distributed | external |

**Recommendation**: Qdrant is the most homelab-friendly of the three dedicated vector DBs — smallest footprint, no CPU feature gotchas, multi-arch. pgvector remains the best fit when an existing Postgres is already on the stone.

## Validation Checklist

- [x] Official multi-arch image (amd64 + arm64) verified via Docker Hub tags
- [x] Pinned version (v1.18.0) verified as latest stable on Docker Hub 2026-05-11
- [x] No hard CPU feature requirement (scalar fallbacks confirmed)
- [x] Memory minimum researched (1 GB per maintainer)
- [x] Storage paths confirmed against official quickstart
- [x] Port numbers (6333 REST, 6334 gRPC) verified against Dockerfile EXPOSE
- [x] Healthcheck strategy accounts for missing curl/wget in image
- [x] Security defaults documented (unauthenticated by default)
- [x] Telemetry opt-out captured

## Files

| File | Purpose |
|------|---------|
| `qdrant.snippet.yaml` | Compose snippet (image, ports, env, volumes, healthcheck) |
| `qdrant.frontmatter.json` | UI metadata + connection profile |
| `qdrant.compatibility.yaml` | Pre-flight + post-install rules |
| `qdrant.research.md` | This document |

## References

1. [Qdrant — Installation](https://qdrant.tech/documentation/guides/installation/)
2. [Qdrant — Quickstart](https://qdrant.tech/documentation/quickstart/)
3. [Qdrant — Configuration](https://qdrant.tech/documentation/guides/configuration/)
4. [Qdrant — Security](https://qdrant.tech/documentation/guides/security/)
5. [Qdrant — Capacity Planning](https://qdrant.tech/documentation/guides/capacity-planning/)
6. [Qdrant — Minimal RAM for a million vectors](https://qdrant.tech/articles/memory-consumption/)
7. [Qdrant — ARM support announcement](https://qdrant.tech/blog/qdrant-supports-arm-architecture/)
8. [Qdrant Dockerfile (master)](https://github.com/qdrant/qdrant/blob/master/Dockerfile)
9. [Docker Hub — qdrant/qdrant tags](https://hub.docker.com/r/qdrant/qdrant/tags)
10. [Issue #3491 — Add curl to docker image (closed: not planned)](https://github.com/qdrant/qdrant/issues/3491)
11. [Issue #4250 — Add a healthcheck command for docker compose](https://github.com/qdrant/qdrant/issues/4250)
12. [Discussion #3279 — Hardware and software requirements](https://github.com/orgs/qdrant/discussions/3279)
