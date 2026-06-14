# Zen Garden

<p align="center">
  <img src="res/zg-256.png" alt="Zen Garden" />
</p>

Service discovery and orchestration for self-hosted infrastructure on repurposed hardware.

---

## The Situation

Every year, humanity generates 62 million tonnes of electronic waste.

Much of it works. Laptops discarded because they can't run Windows 11. Servers decommissioned because they're "out of support." Thin clients abandoned because a vendor stopped making drivers. The machines function. They simply don't function for what the _market_ wants.

Meanwhile, self-hosting remains hard — not because the software is complex, but because **machines fail**. When your database laptop dies, you face a choice: rename the replacement to match the old hostname, or update every application's configuration. Most people give up and pay for managed databases.

Zen Garden exists because both of these problems have the same solution.

---

## How It Works

```bash
# Traditional: tightly coupled to machines
MONGODB_URI=mongodb://old-laptop-01.local:27017

# Zen Garden: coupled to services
MONGODB_URI=zen-garden:mongodb/mydb
```

Stones (devices running Moss) provide service discovery, orchestration, and failure recovery using mDNS multicast, Docker, and curated offering templates. Security and multi-subnet scaling are opt-in.

| Layer            | What happens                                                       |
| ---------------- | ------------------------------------------------------------------ |
| Discovery        | mDNS multicast (same protocol as AirPlay, Chromecast)              |
| Orchestration    | Docker containers from curated offering templates                  |
| Failure recovery | Clients retry mDNS — new Stone, same service type, auto-reconnect |
| Security         | Optional mTLS via Pond (explicit trust boundary)                   |
| Scaling          | mDNS for up to 30 Stones; Lantern registry recommended beyond 20  |

- **Set up real hardware** — [First Stone Guide](docs/guides/first-stone.md)
- **Understand the protocol** — [Discovery Spec](docs/specs/discovery.md)

---

## Features

Zen Garden runs on Linux and Windows, detects GPU hardware automatically, and ships intelligent orchestrators for distributed workloads.

| Category                | What's included                                                          | Docs                                                                                    |
| ----------------------- | ------------------------------------------------------------------------ | --------------------------------------------------------------------------------------- |
| Platform                | Linux (managed containers), Windows (adopted native services)            | [Offering Modes](docs/decisions/OFFER-0005-offering-modes.md)                           |
| GPU acceleration        | NVIDIA CUDA, AMD ROCm, Intel OpenVINO, Windows DirectML — auto-detected | [Stone Hardware](docs/guides/stone-hardware.md)                                         |
| Ollama orchestrator     | VRAM-aware routing, demand-weighted model placement, auto-tiering        | [AI Capability Router](docs/proposals/offering-orchestration/ORCH-0002-ai-capability-router.md) |
| MongoDB orchestrator    | Automatic replica sets, dynamic membership, placement scoring            | [Database Choreographer](docs/proposals/offering-orchestration/ORCH-0003-database-choreographer.md) |
| Storage                 | Seed banks with replication, S3-compatible gateway                       | [Seed Bank Spec](docs/specs/STORAGE-0001-seed-bank-onboarding.md)                      |
| Updates                 | Multi-phase safe updates for software and firmware (fwupd/LVFS)          | [Nourishment Spec](docs/specs/nourishment-v0-spec.md)                                  |
| Security                | Pond mTLS with TOTP enrollment and certificate rotation                  | [Pond Setup](docs/security/pond-setup.md)                                               |
| Monitoring              | `rake pulse` live terminal, SSE event streams, garden-wide topology      | [Pulse ADR](docs/decisions/PULSE-0001-terminal-monitor.md)                              |
| Companions              | Audio (Cricket) and LED (Firefly) feedback via companion SDK             | [Companion Overview](docs/guides/companion-overview.md)                                 |
| State transfer          | Replant offerings between stones (harvest → collect → plant)             | [Replant Ceremony](docs/decisions/ORCH-0001-replant-ceremony.md)                        |
| Maintenance             | Automated sweeps for staging, images, and stale binaries                 | [Caretaking Spec](docs/specs/caretaking-maintenance-sweeps.md)                          |
| Hardware profiling      | CPU flags, VRAM, disk type, GPU utilization — per-offering compatibility | [Fitness Profiler](docs/decisions/ORCH-0003-fitness-profiler.md)                        |

---

## Vocabulary

| Term        | What It Is                                                              |
| ----------- | ----------------------------------------------------------------------- |
| **Stone**   | A device offering services (laptop, desktop, Raspberry Pi, thin client) |
| **Moss**    | The daemon running on each Stone (port 7185)                            |
| **Rake**    | The CLI tool for managing the garden                                    |
| **Lantern** | Optional registry for multi-subnet gardens                              |
| **Pond**    | Optional security boundary (mTLS)                                       |

---

## Getting Started

```bash
# Start a Stone with MongoDB
docker run -d -p 27017:27017 --name mongo-stone \
  -e ANNOUNCE_SERVICE=mongodb \
  zen-garden/stone:latest

# Discover what's running
garden-rake find mongodb
# => Found: mongodb on stone-01 (192.168.1.42:27017)
```

---

## Building from source

```bash
git clone https://github.com/sylin-org/zen-garden && cd zen-garden
cargo build --workspace                       # moss, rake, lantern, companions
cd src/orchestrators/ollama && cargo build    # orchestrators build standalone
```

Developing against a local [koi](https://github.com/sylin-org/koi) checkout? Copy
`.cargo/config.local.toml.example` to `.cargo/config.local.toml` (gitignored).

---

## Examples

A typical garden runs on whatever hardware you have. 31 offering templates ship across 17 categories including databases, AI, networking, observability, storage, and search.

| Hardware              | Service    | Notes                  |
| --------------------- | ---------- | ---------------------- |
| 2015 laptop           | MongoDB    | Handles 1,000+ req/sec |
| Dell Wyse thin client | Redis      | 2 GB RAM, 5 watts      |
| Old desktop           | Ollama     | Local LLM inference    |
| Raspberry Pi          | Prometheus | Monitoring             |

- [Full offerings catalog](docs/reference/offerings.md)

---

## Documentation

| If you want to...               | Start here                                                            |
| ------------------------------- | --------------------------------------------------------------------- |
| Install Moss on hardware        | [Installing Moss](docs/guides/installing-moss.md)                     |
| Set up your first Stone         | [First Stone Guide](docs/guides/first-stone.md)                       |
| See what services are available | [Offerings Catalog](docs/reference/offerings.md)                      |
| Learn the CLI                   | [Rake Commands](docs/specs/rake-commands.md)                          |
| Use companions (audio, LEDs)    | [Companion Overview](docs/guides/companion-overview.md)               |
| Understand security options     | [Security Overview](docs/security/overview.md)                        |
| Troubleshoot issues             | [Troubleshooting](docs/guides/troubleshooting.md)                     |
| See architecture decisions      | [Decision Records](docs/decisions/)                                   |

[Documentation Hub](docs/README.md)

---

## Project Status

Version 0.2.0 — active development.

| Component        | Description                                    |
| ---------------- | ---------------------------------------------- |
| `garden-moss`    | Stone daemon — discovery, orchestration, API   |
| `garden-rake`    | CLI with `pulse` live terminal monitor         |
| `garden-lantern` | Registry for cross-subnet discovery            |
| `garden-cricket` | Audio companion (sound feedback)               |
| `garden-firefly` | LED companion (visual status on RP2040-Matrix) |
| Offerings        | 31 curated service templates across 17 categories |

- [Release Notes](docs/ops/release-notes.md)
- [Roadmap](docs/ops/roadmap.md)

---

## Contributing

- Run Stones on your old hardware and tell us what breaks
- Write offering templates for services you use
- Improve documentation where it confused you
- [Maintainer Docs](docs/ops/maintainers.md)

---

**License**: Apache 2.0
**Maintainer**: Sylin.org
**Repository**: [github.com/sylin-org/zen-garden](https://github.com/sylin-org/zen-garden)
