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
| [Hardware Selection](guides/hardware.md) | Choose appropriate hardware for different workloads |
| [Managing Offerings](guides/offering-services.md) | Plant, upgrade, rest, wake, and take away services |
| [Using Adapters](guides/adapters.md) | Control Cricket, Firefly, and OLED adapters for physical presence |
| [Creating Tunes](guides/how-to-create-a-tune.md) | Write YAML configurations for Cricket audio adapter |
| [Adapter Development](guides/adapter-development.md) | Build custom adapters in any language |
| [Troubleshooting](guides/troubleshooting.md) | Common problems and solutions |

---

## Specifications

Technical specifications for implementers:

| Spec | Description |
|------|-------------|
| [Moss Daemon Lifecycle](specs/moss-daemon-lifecycle.md) | 14-phase startup, HTTP API, Docker Compose integration |
| [Rake Commands](specs/rake-commands.md) | CLI tool design, hot cache, command taxonomy, adapter control |
| [Discovery Protocol](specs/discovery.md) | mDNS announcement, TXT records, connection strings |
| [Service Offerings](specs/offerings.md) | Template format, taxonomy, query system |
| [Security](specs/security.md) | Pond mTLS, certificate management, threat model |
| [HTTP API](specs/api-v1.md) | REST endpoints, adapter management, request/response formats |
| [Adapter Command Protocol](specs/ADAPTER-COMMAND-PROTOCOL.md) | Synchronous command flow, port assignment, timeout handling |
| [Adapter Service Registry](specs/ADAPTER-SERVICE-REGISTRY.md) | Discovery protocol, manifest format, lifecycle management |
| [Hey-Tell Syntax](specs/HEY-TELL-SYNTAX.md) | Rake command grammar for adapter control |
| [Cricket Specification](specs/CRICKET-SPEC.md) | Audio adapter implementation, 4-channel mixer, tune system |

---

## Reference

Quick lookup for operators and developers:

| Reference | Description |
|-----------|-------------|
| [Components](reference/components.md) | System architecture, communication flows |
| [Connection Strings](reference/connection-strings.md) | `zen-garden:` URI scheme details |
| [Port Allocation](reference/ports.md) | Reserved ports (7184-7199) |
| [Service Catalog](reference/offerings.md) | Available service templates |
| [Configuration](reference/config.md) | moss.toml and garden-moss.toml settings |
| [Driver Specification](reference/driver-specification.md) | Client library implementation guide |

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

---

## Glossary

**[glossary.md](glossary.md)** — Essential terms: Stone, Moss, Rake, Lantern, Pond, Offering
