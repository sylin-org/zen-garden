---
audience: operator
doc_type: reference
status: current
last_verified: 2026-02-07
---

# Roadmap

**Development timeline and priorities.**

---

## Current Capabilities (0.1.0)

### Core Infrastructure

- garden-moss daemon with 14-phase startup orchestration
- garden-rake CLI with full command taxonomy (zen verbs + lifecycle commands)
- garden-lantern registry for cross-subnet discovery
- HTTP API (Axum 0.7) with v1 endpoints

### Discovery & Topology

- Multicast-first UDP discovery (239.255.42.99:7184) with directed broadcast fallback
- Per-interface sender sockets for multi-homed Windows 11 support
- Virtual adapter detection and filtering (skips Docker/WSL/Hyper-V interfaces)
- Automatic topology maintenance (45s offline threshold, 30s cleanup interval)
- P2P stone chirping and announcement system

### Service Management

- 30+ service offering templates with taxonomy system
- Docker Compose integration and lifecycle management
- Service registry with persistence
- Intelligent placement with compatibility scoring
- Health monitoring and auto-adoption

### Updates (Nourishment V0)

- Software update checking (Docker registry integration)
- Firmware update detection (fwupd/LVFS integration)
- Hardware constraint validation (CPU features, memory, architecture)
- Garden-wide update orchestration (parallel stone queries)
- Scoped execution (all/offerings/firmware)
- SSE streaming for real-time progress updates
- Automatic rollback on health check failures
- Reboot handling for firmware updates

### Hardware Detection

- CPU capabilities (AVX, SSE4.2, architecture validation)
- Memory and storage detection
- AI runtime detection (CUDA, OpenVINO, Ollama)
- Hardware manifest system (hw/ directory with YAML frontmatter)

---

## Phase 2: Polish & Libraries

**Goal**: Production-ready for home lab use + client library ecosystem

### First Boot Experience

- Clear feedback during startup phases
- Graceful handling of Docker not running
- Network interface auto-detection improvements

### Error Handling

- Human-readable error messages
- Troubleshooting hints in error output

### Documentation

- Video walkthrough
- Offering template authoring guide
- Advanced nourishment ceremony guide

### Connection String Driver Libraries

Applications resolve `zen-garden:mongodb/mydb` to actual connection strings without manual discovery.

| Language | Package |
|----------|---------|
| Node.js | `@zen-garden/resolver` |
| Python | `zen-garden-resolver` |
| Rust | `zen-garden-client` |
| C# | `ZenGarden.Client` |

### Nourishment Ceremonies (V1)

- Orchestrated update sequences with pre/post checks
- Dependency-aware ordering
- Harvest archival and restore
- Service migration between stones

---

## Phase 3: Pond Security

**Goal**: Optional mTLS for authenticated, encrypted communication.

- **Keystone**: Encrypted CA keypair storage
- **Cornerstone**: First Stone as certificate authority
- **Certificate binding**: mTLS for all Stone-to-Stone traffic
- **Admission control**: New Stones require approval
- **Encrypted UDP**: XChaCha20-Poly1305 for all pond traffic
- **TOTP invitation**: Bluetooth-pairing style device admission

---

## Phase 4: Web Dashboard

**Goal**: Visual garden management for operators who prefer GUIs.

- Stone health overview
- Service catalog browser
- One-click offering deployment
- Log viewer
- Configuration editor
- Nourishment status and history

Not planned: Cloud-hosted dashboard. The dashboard runs on a Stone.

---

## Non-Goals

Things Zen Garden is explicitly not building:

- **Kubernetes replacement**: Use K8s for clusters. Zen Garden is for 3-10 Stones.
- **Cloud integration**: No AWS/Azure/GCP connectors. This is for local hardware.
- **Multi-tenancy**: One garden, one operator. No user management.
- **Load balancing**: Services handle their own scaling.
- **Container registry**: Use Docker Hub or run your own registry Stone.
- **Managed services**: Zen Garden manages self-hosted services, not SaaS.

---

## Contributing

The best way to help right now:

1. **Run Stones on old hardware** — Report what breaks
2. **Write offering templates** — See `manifests/` for examples
3. **Improve error messages** — Unclear errors are bugs
4. **Test on unusual hardware** — Thin clients, ARM boards, old laptops

See [maintainers.md](maintainers.md) for architecture invariants.
