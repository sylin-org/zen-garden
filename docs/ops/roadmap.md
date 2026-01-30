# Roadmap

**Development timeline and priorities.**

---

## Current Status

**Version**: 0.1.0 (Initial Garden Phase)  
**Phase**: Core implementation complete, client libraries next

### What's Working (2026-01-26)

The Rust implementation is complete and functional:

✅ **Core Infrastructure**
- garden-moss daemon with 14-phase startup orchestration
- garden-rake CLI with full command taxonomy (zen verbs + lifecycle commands)
- garden-lantern registry for cross-subnet discovery
- HTTP API (Axum 0.7) with v1 endpoints

✅ **Discovery & Topology** (Completed 2026-01-25)
- Multicast-first UDP discovery (239.255.42.99:7184) with directed broadcast fallback
- Per-interface sender sockets for multi-homed Windows 11 support
- Virtual Companion detection/filtering (skips Docker/WSL/Hyper-V interfaces)
- Automatic topology maintenance (45s offline threshold, 30s cleanup interval)
- P2P stone chirping and announcement system

✅ **Service Management**
- 30+ service offering templates with taxonomy system
- Docker Compose integration and lifecycle management
- Service registry with persistence
- Intelligent placement with compatibility scoring
- Health monitoring and auto-adoption

✅ **Updates (Nourishment)** (Completed 2026-01)
- Software update checking (Docker registry integration)
- Firmware update detection (fwupd/LVFS integration)  
- Hardware constraint validation (CPU features, memory, architecture)
- Garden-wide update orchestration (parallel stone queries)
- Scoped execution (all/offerings/firmware)
- SSE streaming for real-time progress updates
- Automatic rollback on health check failures
- Reboot handling for firmware updates

✅ **Hardware Detection**
- CPU capabilities (AVX, SSE4.2, architecture validation)
- Memory and storage detection
- AI runtime detection (CUDA, OpenVINO, Ollama)
- Hardware manifest system (hw/ directory with YAML frontmatter)

### What's Next

Near-term priorities:

| Priority | Feature | Status |
|----------|---------|--------|
| P0 | First-boot experience polish | In progress |
| P0 | Error messages for common failures | In progress |
| P1 | Nourishment ceremony system (v2) | Planned (basic updates working) |
| P1 | Connection string driver libraries | Planned |
| P1 | Pond security layer (mTLS) | Planned |
| P2 | Web dashboard | Not started |
| P2 | Additional offering templates | Ongoing |

---

## Phase 1: Core Infrastructure (COMPLETE ✅)

**Goal**: Production-ready P2P discovery and service management

### Discovery & Topology ✅ Completed 2026-01-25

- ✅ Multicast-first UDP discovery with broadcast fallback
- ✅ Per-interface sender sockets for multi-homed systems
- ✅ Virtual Companion detection and filtering
- ✅ Topology maintenance (offline detection, cache eviction)
- ✅ Stone chirping and P2P announcement system

### Service Management ✅ Complete

- ✅ Offering deployment and lifecycle management
- ✅ Intelligent placement with compatibility scoring
- ✅ Health monitoring and auto-adoption
- ✅ Service registry with persistence

### Updates (Nourishment V0) ✅ Complete 2026-01

- ✅ Software update checking (Docker registry)
- ✅ Firmware update detection (fwupd/LVFS)
- ✅ Garden-wide update orchestration
- ✅ Scoped execution (all/offerings/firmware)
- ✅ SSE streaming for progress
- ✅ Hardware constraint validation
- ✅ Reboot handling for firmware

---

## Phase 2: Polish & Libraries (NEXT)

**Goal**: Production-ready for home lab use + client library ecosystem

### First Boot Experience (In Progress)

- [x] 14-phase startup orchestration
- [x] Hardware detection and capability discovery
- [ ] Clear feedback during startup phases
- [ ] Graceful handling of Docker not running
- [ ] Network interface auto-detection improvements

### Error Handling (In Progress)

- [ ] Human-readable error messages
- [ ] Troubleshooting hints in error output
- [x] Structured logging for debugging

### Documentation

- [x] Philosophy documentation
- [x] Architecture specifications
- [ ] Video walkthrough
- [ ] Offering template authoring guide
- [ ] Advanced nourishment ceremony guide

### Connection String Driver Libraries

Applications should resolve `zen-garden:mongodb/mydb` to actual connection strings without manual discovery.

**Planned libraries**:

| Language | Package | Status |
|----------|---------|--------|
| Node.js | `@zen-garden/resolver` | Planned |
| Python | `zen-garden-resolver` | Planned |
| Rust | `zen-garden-client` | Planned |
| C# | `ZenGarden.Client` | Planned |

**Driver Integration**:
- MongoDB5: Web Dashboard

**Goal**: Visual garden management for operators who prefer GUIs.

### Features

- Stone health overview
- Service catalog browser
- One-click offering deployment
- Log viewer
- Configuration editor
- Nourishment status and history

**Not planned**: Cloud-hosted dashboard. This runs on a Stone.

---

## Non-Goals

Things we're explicitly not building:

- **Kubernetes replacement**: Use K8s for clusters. Zen Garden is for 3-10 Stones.
- **Cloud integration**: No AWS/Azure/GCP connectors. This is for local hardware.
- **Multi-tenancy**: One garden, one operator. No user management.
- **Load balancing**: Services handle their own scaling.
- **Container registry**: Use Docker Hub or run your own registry Stone.
- **Managed services**: Zen Garden manages self-hosted services, not SaaS
- [ ] Harvest archival and restore
- [ ] Service migration between stones

---

## Phase 4: Pond Security

**Goal**: Optional mTLS for authenticated, encrypted communication.

### Features

- **Keystone**: Encrypted CA keypair storage
- **Cornerstone**: First Stone as certificate authority
- **Certificate binding**: mTLS for all Stone-to-Stone traffic
- **Admission control**: New Stones require approval

### Security

- **Encrypted UDP**: XChaCha20-Poly1305 for all pond traffic
- **TOTP invitation**: Bluetooth-pairing style device admission
- **Admission control**: New Stones require approval

---

## Phase 4: Web Dashboard

**Goal**: Visual garden management for operators who prefer GUIs.

### Features

- Stone health overview
- Service catalog browser
- One-click offering deployment
- Log viewer
- Configuration editor

**Not planned**: Cloud-hosted dashboard. This runs on a Stone.

---

## Non-Goals

Things we're explicitly not building:

- **Kubernetes replacement**: Use K8s for clusters. Zen Garden is for 3-10 Stones.
- **Cloud integration**: No AWS/Azure/GCP connectors. This is for local hardware.
- **Multi-tenancy**: One garden, one operator. No user management.
- **Load balancing**: Services handle their own scaling.
- **Container registry**: Use Docker Hub or run your own registry Stone.

---

## Contributing

The best way to help right now:

1. **Run Stones on old hardware** — Report what breaks
2. **Write offering templates** — See `manifests/` for examples
3. **Improve error messages** — Unclear errors are bugs
4. **Test on unusual hardware** — Thin clients, ARM boards, old laptops

See [maintainers.md](maintainers.md) for architecture invariants.
