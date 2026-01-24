# Roadmap

**Development timeline and priorities.**

---

## Current Status

**Version**: 0.1.0 (Initial Garden Phase)  
**Phase**: Core implementation complete, hardening in progress

### What's Working

The Rust implementation is complete and functional:

- **garden-moss** daemon with 14-phase startup orchestration
- **garden-rake** CLI with full command taxonomy
- **lantern** registry for cross-subnet discovery
- 30+ service offering templates
- mDNS discovery via mdns-sd
- Docker Compose integration
- HTTP API (Axum 0.7)

### What's Next

Near-term priorities:

| Priority | Feature | Status |
|----------|---------|--------|
| P0 | First-boot experience polish | In progress |
| P0 | Error messages for common failures | In progress |
| P1 | Connection string driver libraries | Planned |
| P1 | Pond security layer (mTLS) | Planned |
| P2 | Web dashboard | Not started |
| P2 | Additional offering templates | Ongoing |

---

## Phase 1: Hardening (Current)

**Goal**: Production-ready for home lab use.

### First Boot Experience

- [ ] Single-command Stone setup
- [ ] Clear feedback during 14-phase startup
- [ ] Graceful handling of Docker not running
- [ ] Network interface auto-detection

### Error Handling

- [ ] Human-readable error messages
- [ ] Troubleshooting hints in error output
- [ ] Structured logging for debugging

### Documentation

- [x] Philosophy documentation
- [x] Architecture specifications
- [ ] Video walkthrough
- [ ] Offering template authoring guide

---

## Phase 2: Client Libraries

**Goal**: Native connection string resolution for common languages.

### zen-garden:// Protocol

Applications should resolve `zen-garden:mongodb/mydb` to actual connection strings without manual discovery.

**Planned libraries**:

| Language | Package | Status |
|----------|---------|--------|
| Node.js | `@zen-garden/resolver` | Planned |
| Python | `zen-garden-resolver` | Planned |
| Rust | `zen-garden-client` | Planned |
| C# | `ZenGarden.Client` | Planned |

### Driver Integration

- MongoDB: Wrap official driver with zen-garden resolution
- Redis: Wrap official driver
- PostgreSQL: Connection string rewriting

---

## Phase 3: Pond Security

**Goal**: Optional mTLS for authenticated, encrypted communication.

### Features

- **Keystone**: Encrypted CA keypair storage
- **Cornerstone**: First Stone as certificate authority
- **Certificate binding**: mTLS for all Stone-to-Stone traffic
- **Admission control**: New Stones require approval

### Tiers

| Tier | Name | Use Case |
|------|------|----------|
| 0 | Open Garden | Default. Plaintext. Trusted LAN. |
| 1 | Garden Pond | Basic mTLS. Home lab with guests. |
| 2 | Deep Pond | Enterprise hardening. Audit logs. |

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
