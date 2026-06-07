# Documentation

**Navigate Zen Garden documentation by what you need.**

---

## What Is This?

**Zen Garden is automatic service discovery for self-hosted infrastructure.**

You have old hardware—laptops, thin clients, Raspberry Pis. You want to run databases, caches, file servers. The problem: when hardware fails, you have to update every application's connection string.

Zen Garden solves this. Services announce themselves. Apps discover them. When you replace failed hardware, apps reconnect automatically. No configuration changes.

```bash
# Your app asks for MongoDB. A Stone answers.
MONGODB_URI=zen-garden:mongodb/mydb
```

**Time investment**: 30 minutes to first Stone. 5 minutes per additional Stone.  
**Hardware required**: Any 64-bit machine with 2GB+ RAM.  
**Skills required**: Basic terminal comfort. No DevOps expertise.

→ **Ready to start?** [First Stone Guide](guides/first-stone.md)  
→ **Want to understand why?** Keep reading.

---

## Philosophy

Why we build infrastructure this way. [Full index with reading order →](philosophy/README.md)

- [Humanist Infrastructure](philosophy/humanist-infrastructure.md) — The moral case for accessible, understandable systems
- [Metaphor as Architecture](philosophy/metaphor-as-architecture.md) — How naming shapes design decisions
- [Stone Against the Clouds](philosophy/stone-against-the-clouds.md) — Choosing physical presence over abstraction
- [Discovery Over Configuration](philosophy/discovery-over-configuration.md) — Let services announce themselves
- [Failure as Weather](philosophy/failure-as-weather.md) — Treating hardware failure as normal
- [Joy of Understanding](philosophy/joy-of-understanding.md) — Why comprehensibility matters
- [Curated Offerings](philosophy/curated-offerings.md) — Opinionated defaults over infinite options
- [Pond Security Model](philosophy/pond-security-model.md) — Optional boundaries, explicit trust

---

## Guides

Step-by-step instructions for operators:

| Guide | Description |
|-------|-------------|
| [First Stone](guides/first-stone.md) | Set up your first Stone from hardware to running service |
| [Hardware Selection](guides/stone-hardware.md) | Choose appropriate hardware for different workloads |
| [Managing Offerings](guides/offering-lifecycle.md) | Plant, upgrade, rest, wake, and take away services |
| [Authoring an Offering](guides/authoring-an-offering.md) | Add a new managed offering end to end (snippet, frontmatter, compatibility, guidance) |
| [Tools Domain User Guide](guides/tools-domain.md) | Build adapter/client automation on the normative tools snapshot + stream APIs |
| [Using Companions](guides/companion-overview.md) | Control Cricket, Firefly, and OLED Companions for physical presence |
| [Creating Tunes](guides/cricket-tune-authoring.md) | Write YAML configurations for Cricket audio Companion |
| [Companion Development](guides/companion-development.md) | Build custom Companions in any language |
| [Troubleshooting](guides/troubleshooting.md) | Common problems and solutions |

---

## Specifications

Technical specifications for implementers:

| Spec | Description |
|------|-------------|
| [Moss Daemon Lifecycle](specs/moss-daemon-lifecycle.md) | 14-phase startup, HTTP API, Docker Compose integration |
| [Rake Commands](specs/rake-commands.md) | CLI tool design, hot cache, command taxonomy, Companion control |
| [Discovery Protocol](specs/discovery.md) | mDNS announcement, TXT records, connection strings |
| [Service Offerings](specs/offerings.md) | Template format, taxonomy, query system |
| [Security](specs/security.md) | Pond mTLS, certificate management, threat model |
| [HTTP API](specs/api-v1.md) | REST endpoints, Companion management, request/response formats |
| [Companion Command Protocol](specs/companion-command-protocol.md) | Synchronous command flow, port assignment, timeout handling |
| [Companion Service Registry](specs/companion-service-registry.md) | Discovery protocol, manifest format, lifecycle management |
| [Hey-Tell Syntax](specs/hey-tell-syntax.md) | Rake command grammar for Companion control |
| [Cricket Specification](specs/cricket-spec.md) | Audio Companion implementation, 4-channel mixer, tune system |
| [Discovery Transport](specs/discovery-transport.md) | Multicast-first transport with directed broadcast fallback |
| [Topology Cache](specs/topology-cache.md) | Stone liveness tracking, offline detection, cache eviction |
| [Nourishment V0](specs/nourishment-v0-spec.md) | Software and firmware update checking and execution |
| [Seed Bank Onboarding](specs/STORAGE-0001-seed-bank-onboarding.md) | Storage device preparation and lifecycle |
| [Distributed Election](specs/ELECTION-0001-distributed-election.md) | Tended stone election protocol |

---

## Reference

Quick lookup for operators and developers:

| Reference | Description |
|-----------|-------------|
| [Components](reference/components.md) | System architecture, communication flows |
| [Connection Strings](reference/connection-strings.md) | `zen-garden:` URI scheme details |
| [Port Allocation](reference/ports.md) | Reserved ports (7184-7199) |
| [Service Catalog](reference/offerings.md) | Available service templates |
| [Configuration](reference/config.md) | moss.toml configuration settings |
| [Driver Specification](reference/driver-specification.md) | Client library implementation guide |
| [S3 API Reference](reference/s3-api-reference.md) | S3-compatible object storage gateway |
| [Cost Analysis](reference/cost-analysis.md) | Self-hosted vs cloud cost comparison |

---

## Security

Security posture and optional hardening:

| Document | Description |
|----------|-------------|
| [Overview](security/overview.md) | Default plaintext, when to add Pond |
| [Pond Setup](security/pond-setup.md) | Certificate management, Stone admission |
| [Threat Analysis](security/threat-analysis.md) | Attack vectors and mitigations |

---

## Operations

For maintainers and contributors:

| Document | Description |
|----------|-------------|
| [Release Notes](ops/release-notes.md) | Version history, breaking changes |
| [Roadmap](ops/roadmap.md) | Development timeline and priorities |
| [Maintainers](ops/maintainers.md) | Architecture invariants, contribution guidelines |
| [Build Distribution](ops/build-distribution.md) | Build artifacts and packaging |

---

## Decisions

Architecture Decision Records (ADRs) documenting design choices:

- [Decision Index](decisions/README.md) — Browse all ADRs by category
- Key decisions:
  - [Dual-Layer API](decisions/API-0001-dual-layer-api.md) — Why admin/service split
  - [Stateless Moss](decisions/STATE-0001-stateless-moss.md) — No persistent state
  - [Single mDNS Service Type](decisions/MDNS-0001-single-service-type.md) — Discovery simplicity
  - [SoC/DDD Architecture](decisions/ARCH-0001-soc-ddd-architecture.md) — Domain/infra/API separation
  - [Offering Modes](decisions/OFFER-0005-offering-modes.md) — Managed, Adopted, Borrowed
  - [Multicast-First Discovery](decisions/COMM-0004-multicast-first-discovery.md) — Why multicast over broadcast

---

## Glossary

**[glossary.md](glossary.md)** — Essential terms: Stone, Moss, Rake, Lantern, Pond, Offering

---

## Contributing to Docs

**[DOCUMENTATION.md](DOCUMENTATION.md)** — Style guide, naming conventions, and templates

Every document has a voice: guides are instructional, specs are declarative, ADRs are historical. Read the style guide before writing or editing documentation.
