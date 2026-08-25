# Journey Writing Guide

*Reference for continuing this documentation effort across sessions.*

---

## The Task

Create narrative documentation ("Journeys") that explain Zen Garden's internal processes through storytelling. Each journey follows a user action through the system, showing what they experience, then explaining what happened behind the scenes.

## The Format

Every journey has two parts:

### Part 1: The Story
- Pure experience — commands, outputs, time passing
- No technical explanations
- Written in second person ("You type...", "You see...")
- Time jumps encouraged: "Three days later...", "Six months pass..."
- Should feel like a short film
- Builds tension and curiosity naturally

### Part 2: What Just Happened
- Rewinds to the beginning
- Explains each moment from the story
- References the story: "When you typed...", "That USB drive you prepared..."
- Technical depth appropriate to the concept
- Can include code snippets, protocol details, data structures
- Satisfies the curiosity the story created

### Ending: Commands From This Journey
- Quick reference of commands used
- Helps readers who want to try things themselves

## Voice and Tone

Match the existing Zen Garden documentation voice:
- Humanist, not corporate
- Technical precision without jargon overload
- The garden metaphor is real architecture, not decoration
- First person occasionally acceptable ("Here's what actually happened")
- Acknowledge that hardware fails, that's the point
- Permission to be small, comprehensible, touchable

## Technical Accuracy

When writing explanations, verify against:
- `docs/philosophy/` — For voice and concepts
- `docs/specs/` — For protocol details
- `docs/proposals/` — For feature designs
- `src/rake/src/commands/` — For CLI behavior
- `src/moss/src/` — For daemon internals
- `src/common/src/` — For shared types

Key technical details to get right:
- **Discovery**: UDP broadcast on 7184, Moss on 7185, mDNS service types
- **Topology cache**: 90-second TTL, hot cache on every Stone
- **Ceremonies**: Collect → Apply → Verify phases, harvest for rollback
- **Seed banks**: `.zen-garden/` structure, manifest.yaml, journal sync
- **Companions**: SSE subscription, event-to-action mapping
- **Offerings**: Templates with ceremony modes (stateless, quiesceable, unsafe)

## File Naming

```
docs/journeys/
├── README.md                              # Catalog and reading guide
├── WRITING-GUIDE.md                       # This file
├── EXAMPLE-the-life-of-an-offering.md     # Reference example
├── 01-the-first-stone.md
├── 02-the-life-of-an-offering.md          # Rename from EXAMPLE
├── 03-when-stones-meet.md
├── ...
```

## Progress Tracking

### Completed
- [x] README.md — Catalog with 27 journey candidates
- [x] WRITING-GUIDE.md — This reference document
- [x] 01-the-first-stone.md — Onboarding, hardware detection, becoming a Stone
- [x] 02-the-life-of-an-offering.md — Full offering lifecycle
- [x] 03-when-stones-meet.md — mDNS discovery, topology, the chirp
- [x] 04-finding-things.md — Discovery cascade, connection strings
- [x] 05-the-night-the-drive-died.md — Seed banks, restore, resilience payoff
- [x] 06-the-failed-update.md — Ceremonies, harvest/rollback
- [x] 07-the-stone-that-vanished.md — Topology expiration, offline detection, goodbye chirp
- [x] 08-vacating-before-the-storm.md — Graceful evacuation, service migration
- [x] 09-the-third-stone.md — Scaling up, placement decisions, scoring algorithm
- [x] 10-crossing-the-subnet.md — Lantern registry, cross-network discovery
- [x] 11-the-stray-container.md — Adopting existing Docker containers
- [x] 12-borrowing-from-outside.md — External service registration, shakkei concept
- [x] 13-the-sound-of-the-garden.md — Cricket Companion, events, ambient awareness
- [x] 14-the-lights-on-the-shelf.md — Firefly Companion, LED status
- [x] 15-the-portrait-on-the-wall.md — Portrait page, living dashboard
- [x] 16-when-the-garden-speaks.md — Event system, SSE streaming
- [x] 17-the-morning-check.md — Daily nourishment routine, update workflow
- [x] 18-the-careful-update.md — Manual update workflow, harvest/rollback
- [x] 19-preparing-for-the-worst.md — Disaster recovery planning, testing
- [x] 20-the-quarterly-prune.md — Maintenance routines, cleanup
- [x] 21-filling-the-pond.md — Security initialization, Keystone, encryption
- [x] 22-the-invitation.md — TOTP invitations, access control, revocation
- [x] 23-draining-the-pond.md — Removing access, key rotation, pond destruction
- [x] 24-the-election.md — Leadership transfer, emergency election, succession
- [x] 25-the-custom-offering.md — Custom offering templates, manifest structure
- [x] 26-the-reconciliation.md — Conflict resolution, state recovery, merge protocols

### Full Catalog Status

**Foundation**
- [x] 01 The First Stone
- [x] 02 The Life of an Offering
- [x] 03 When Stones Meet
- [x] 04 Finding Things

**Resilience**
- [x] 05 The Night the Drive Died
- [x] 06 The Failed Update
- [x] 07 The Stone That Vanished
- [x] 08 Vacating Before the Storm

**Growth**
- [x] 09 The Third Stone
- [x] 10 Crossing the Subnet
- [x] 11 The Stray Container
- [x] 12 Borrowing from Outside

**Presence**
- [x] 13 The Sound of the Garden
- [x] 14 The Lights on the Shelf
- [x] 15 The Portrait on the Wall
- [x] 16 When the Garden Speaks

**Maintenance**
- [x] 17 The Morning Check
- [x] 18 The Careful Update
- [x] 19 Preparing for the Worst
- [x] 20 The Quarterly Prune

**Security**
- [x] 21 Filling the Pond
- [x] 22 The Invitation
- [x] 23 Draining the Pond

**Advanced**
- [x] 24 The Election
- [x] 25 The Custom Offering
- [x] 26 The Reconciliation
- [ ] 27 The Overnight Job (optional)

## Key Resources

### CLI Commands Reference
- `src/rake/src/commands/` — All command implementations
- `src/rake/src/command_manifest.rs` — Command taxonomy

### Companions
- `src/cricket/` — Audio Companion, tune system
- `src/firefly/` — LED Companion, animations
- `docs/guides/companion-overview.md` — Companion documentation

### Storage & Backup
- `docs/specs/STORAGE-0001-seed-bank-onboarding.md` — Seed bank spec
- `src/moss/src/infra/storage/` — Storage implementation
- `src/moss/src/tasks/nourishment_scheduler.rs` — Backup scheduling

### Discovery & Topology
- `docs/philosophy/discovery-over-configuration.md` — Discovery concepts
- `src/moss/src/infra/topology/` — Topology cache
- `src/common/src/infra/communications/` — Protocol types

### Ceremonies & Updates
- `docs/specs/nourishment.md` — Full nourishment spec
- `src/moss/src/domain/ceremony/` — Ceremony engine (planned)

---

## Resuming Work

To continue this effort in a new session:

1. Read this guide for context and format
2. Check progress tracking above
3. Pick next journey from priority queue (or catalog)
4. Read the EXAMPLE file for tone reference
5. Explore relevant code/docs for technical accuracy
6. Write the story first, then the explanation
7. Update progress tracking when complete

The goal is comprehensive coverage of Zen Garden through stories that teach.

---

*Last updated: 2026-01-30*
*Session progress: 26 journeys complete (01-26), Journey 27 optional*
