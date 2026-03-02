# Zen Garden

<p align="center">
  <img src="res/zg-256.png" alt="Zen Garden" />
  <br/>
  <br/>
  **Automatic service discovery for self-hosted infrastructure**

## </p>

## The Situation

Every year, humanity generates 62 million tonnes of electronic waste.

Much of it works. Laptops discarded because they can't run Windows 11. Servers decommissioned because they're "out of support." Thin clients abandoned because a vendor stopped making drivers. The machines function. They simply don't function for what the _market_ wants.

Meanwhile, self-hosting remains hard—not because the software is complex, but because **machines fail**. When your database laptop dies, you face a choice: rename the replacement to match the old hostname (complex networking), or update every application's configuration (error-prone, downtime). Most people give up and pay $100/month for managed databases.

Zen Garden exists because both of these problems have the same solution.

---

## The Idea

```bash
# Traditional: tightly coupled to machines
MONGODB_URI=mongodb://old-laptop-01.local:27017

# Zen Garden: coupled to services
MONGODB_URI=zen-garden:mongodb/mydb
```

Your app asks "Where's MongoDB?" A Stone answers. Connection established.

When hardware fails, swap in a replacement. The new Stone announces the same service. Apps reconnect automatically. No configuration changes. No coordination. No expertise required.

**That's the entire idea.** The old laptop becomes useful again. The hardware swap becomes trivial. You understand what's running because you can see it, touch it, hear the fan spin.

---

## The Vocabulary

The names are the architecture. When you understand the words, you understand the system.

| Term        | What It Is                                                              | Why This Name                                                                                     |
| ----------- | ----------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| **Stone**   | A device offering services (laptop, desktop, Raspberry Pi, thin client) | Stones have weight. They sit where you put them. You can touch them. They don't float in a cloud. |
| **Moss**    | The daemon running on each Stone (port 7185)                            | Moss grows on stones. It can't exist without them. It lives where the stone lives.                |
| **Rake**    | The CLI tool for tending the garden                                     | You shape a garden with a rake. You don't command it—you tend it.                                 |
| **Lantern** | Optional registry for larger gardens                                    | When you can't see all your Stones from where you stand, you light a lantern.                     |
| **Pond**    | Optional security layer (mTLS)                                          | Water creates boundaries. Inside the pond, different rules apply.                                 |

You already understood most of this. The vocabulary carries meaning you didn't have to learn.

---

## Quickstart

```bash
# Terminal 1: Start a MongoDB Stone
docker run -d -p 27017:27017 --name mongo-stone \
  -e ANNOUNCE_SERVICE=mongodb \
  zen-garden/stone:latest

# Terminal 2: Discover what's running
garden-rake find mongodb

# Output:
# Found: mongodb on stone-01 (192.168.1.42:27017)

# Terminal 3: Control Companions (audio, LEDs, displays)
garden-rake hey tell cricket play stone-online
```

The Stone announced itself via mDNS (the same protocol as AirPlay and Chromecast—20+ years proven). The Rake heard the announcement. Companions extend Stone capabilities with audio, visual, and display feedback. No configuration. No registry. Just presence.

**Want real hardware?** → [First Stone Guide](docs/guides/first-stone.md)  
**Want to understand the protocol?** → [Discovery Spec](docs/specs/discovery.md)

---

## What You Can Build

A small garden. Three to ten Stones.

- A 2015 laptop running MongoDB (handles 1,000+ req/sec—more than most apps need)
- A Dell Wyse thin client running Redis (2GB RAM, draws 5 watts, silent)
- An old desktop serving files via MinIO
- A Raspberry Pi running Prometheus

All discovered automatically. All managed through `garden-rake`. All _yours_.

You can see each Stone from where you stand. When one fails, you replace the hardware—not the configuration. When you outgrow the garden, you light lanterns and fill ponds. But you probably won't outgrow it. Most applications don't need infinite scale. They need _appropriate_ scale.

---

## Permission

If you've read this far, you may be looking for permission to do something unfashionable.

To use old hardware. To build small. To run infrastructure you can understand. To care about things that don't scale to millions of users.

**This is that permission.**

Zen Garden is for people who want their feet on the ground. Not because the cloud is wrong, but because this is where they choose to stand.

---

## How It Works

**Discovery**: Stones announce services via mDNS multicast. Apps query for service types. No central registry required for small gardens.

**Orchestration**: Each Stone runs Docker Compose. Services are defined as "offerings"—curated templates with sensible defaults. You don't write YAML; you select from a catalog.

**Failure**: When a Stone disappears, apps retry discovery. When a new Stone offers the same service, apps reconnect. Hardware becomes interchangeable.

**Security** (optional): Fill a Pond to add mTLS authentication. Stones inside the pond trust each other. The boundary is explicit.

**Scale**: 3-10 Stones work with mDNS alone. Beyond that, light a Lantern (registry service). This isn't a limitation—it's a design choice. Zen Garden is for home labs, not data centers.

---

## Documentation

| If you want to...               | Start here                                                            |
| ------------------------------- | --------------------------------------------------------------------- |
| Set up your first Stone         | [First Stone Guide](docs/guides/first-stone.md)                       |
| Understand the philosophy       | [Humanist Infrastructure](docs/philosophy/humanist-infrastructure.md) |
| See what services are available | [Offerings Catalog](docs/reference/offerings.md)                      |
| Learn the CLI                   | [Rake Commands](docs/specs/rake-commands.md)                          |
| Understand security options     | [Security Overview](docs/security/overview.md)                        |
| Read the specifications         | [Moss Daemon Lifecycle](docs/specs/moss-daemon-lifecycle.md)          |
| See architecture decisions      | [Decision Records](docs/decisions/)                                   |

**Complete navigation**: [Documentation Hub](docs/README.md)

---

## Project Status

**Version**: 0.1.0 (Initial Garden Phase)  
**Status**: Active development, post-refactoring

The Rust implementation is complete and functional:

- `garden-moss` daemon with 14-phase startup orchestration
- `garden-rake` CLI with full command taxonomy
- `lantern` registry for cross-subnet discovery
- 30+ service offering templates

See [Release Notes](docs/ops/release-notes.md) for details.

---

## Contributing

The best way to help:

- Run Stones on your old hardware and tell us what breaks
- Write offering templates for services you use
- Improve documentation where it confused you

See [Maintainer Docs](docs/ops/maintainers.md) for architecture invariants.

---

## The Name

A zen garden is a space for contemplation. Stones placed deliberately. Moss growing slowly. Patterns raked into gravel. Nothing wasted. Nothing rushed.

Infrastructure can be like this. Not the frantic scaling of cloud dashboards, but the quiet satisfaction of systems you understand. Hardware you can touch. Services you can see.

That's what we're building.

---

**License**: Apache 2.0  
**Maintainer**: Sylin.org  
**Repository**: [https://github.com/sylin-org/zen-garden](https://github.com/sylin-org/zen-garden)
