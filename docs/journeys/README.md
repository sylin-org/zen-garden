# Journeys

*Documentation that tells stories.*

---

## What Are Journeys?

Journeys are narrative documentation. Each one follows a user through a scenario, showing what they see and do, then explaining what happened behind the scenes.

**Structure:**
- **The Story** — Pure experience. Commands, outputs, time passing. No explanations.
- **What Just Happened** — Rewind and reveal the machinery.

This format lets readers choose their depth: skim the story for the overview, or dive into the explanations for understanding.

---

## The Journey Catalog

### Foundation

These journeys cover the essential experiences every gardener will have.

| Journey | Story Hook | Illuminates |
|---------|------------|-------------|
| [The First Stone](./01-the-first-stone.md) | "You have an old laptop and twenty minutes." | Boot sequence, hardware detection, first offering, the moment it becomes a Stone |
| [The Life of an Offering](./02-the-life-of-an-offering.md) | "You typed `plant mongodb` and pressed Enter." | Discovery, planting, announcements, updates, backup, migration |
| [When Stones Meet](./03-when-stones-meet.md) | "You plug in the second machine." | mDNS protocol, topology cache, chirp mechanism, how Stones find each other |
| [Finding Things](./04-finding-things.md) | "You can't remember which Stone has Redis." | Discovery cascade, connection strings, the `find` command, application integration |

### Resilience

These journeys explore what happens when things go wrong—and how the garden recovers.

| Journey | Story Hook | Illuminates |
|---------|------------|-------------|
| [The Night the Drive Died](./05-the-night-the-drive-died.md) | "You wake up to a clicking sound." | Seed banks, restore flow, identity preservation, the garden reconverging |
| [The Failed Update](./06-the-failed-update.md) | "The health check didn't pass." | Ceremonies, harvest/rollback, the safety net that caught you |
| [The Stone That Vanished](./07-the-stone-that-vanished.md) | "Stone-coral-peak just... stopped responding." | Topology expiration, service rediscovery, what applications see during failure |
| [Vacating Before the Storm](./08-vacating-before-the-storm.md) | "The forecast says power outage. You have two hours." | Vacate ceremony, service migration, graceful evacuation |

### Growth

These journeys cover expanding the garden and managing complexity.

| Journey | Story Hook | Illuminates |
|---------|------------|-------------|
| [The Third Stone](./09-the-third-stone.md) | "Two Stones felt cozy. Three feels like infrastructure." | Placement decisions, load distribution, when to spread vs. concentrate |
| [Crossing the Subnet](./10-crossing-the-subnet.md) | "Your office Stone can't see your basement Stone." | Lantern registry, when mDNS isn't enough, lighting the way |
| [The Stray Container](./11-the-stray-container.md) | "You already have Postgres running. Can the garden see it?" | Adoption, `locate strays`, bringing existing services into the fold |
| [Borrowing from Outside](./12-borrowing-from-outside.md) | "Your NAS has an S3 endpoint. Your apps need to find it." | Borrowed services, external registration, discovery for things you don't control |

### Presence

These journeys explore the physical and sensory aspects of the garden.

| Journey | Story Hook | Illuminates |
|---------|------------|-------------|
| [The Sound of the Garden](./13-the-sound-of-the-garden.md) | "A chime plays. Something just happened." | Cricket Companion, event-to-sound mapping, tunes, ambient awareness |
| [The Lights on the Shelf](./14-the-lights-on-the-shelf.md) | "A soft green glow means everything is healthy." | Firefly Companion, LED status, visual presence, the 5x5 matrix |
| [The Portrait on the Wall](./15-the-portrait-on-the-wall.md) | "You want a screen showing what this Stone is doing." | Portrait page, living dashboard, identity and state at a glance |
| [When the Garden Speaks](./16-when-the-garden-speaks.md) | "You're in another room when the tone changes." | Event system, SSE streaming, how Companions subscribe, presence without screens |

### Maintenance

These journeys cover the ongoing care of a healthy garden.

| Journey | Story Hook | Illuminates |
|---------|------------|-------------|
| [The Morning Check](./17-the-morning-check.md) | "Coffee in hand, you run `nourish`." | Update detection, firmware checks, the daily rhythm |
| [The Careful Update](./18-the-careful-update.md) | "There's a new MongoDB. Time to update—carefully." | Nourishment ceremony, quiesce/resume, zero-downtime updates |
| [Preparing for the Worst](./19-preparing-for-the-worst.md) | "You buy a USB drive. It's time to make a seed bank." | Seed bank preparation, nurturing scheduler, snapshot strategy |
| [The Quarterly Prune](./20-the-quarterly-prune.md) | "Old harvests are piling up. Time to clean." | Retention policies, storage management, what to keep |

### Security

These journeys explore the optional security layer.

| Journey | Story Hook | Illuminates |
|---------|------------|-------------|
| [Filling the Pond](./21-filling-the-pond.md) | "You want encrypted communication between Stones." | Pond initialization, keystone, the trust boundary |
| [The Invitation](./22-the-invitation.md) | "A new Stone needs to join the pond." | TOTP codes, pairing ceremony, time-limited trust |
| [Draining the Pond](./23-draining-the-pond.md) | "You're decommissioning the security layer." | Pond removal, certificate cleanup, returning to open |

### Advanced

These journeys cover power-user scenarios and edge cases.

| Journey | Story Hook | Illuminates |
|---------|------------|-------------|
| [The Election](./24-the-election.md) | "Three Stones. One update source. Who decides?" | Distributed election, hash-based ordering, consensus without coordination |
| [The Custom Offering](./25-the-custom-offering.md) | "Your app isn't in the catalog. Make it discoverable anyway." | Custom templates, manifest structure, extending the catalog |
| [The Reconciliation](./26-the-reconciliation.md) | "Something's out of sync. Containers don't match the registry." | Reconcile command, state drift, bringing truth back together |
| [The Overnight Job](./27-the-overnight-job.md) | "You started a large restore and went to bed." | Job tracking, background operations, checking progress |

---

## Reading Order

**New to Zen Garden?** Start here:
1. The First Stone
2. The Life of an Offering
3. When Stones Meet
4. The Morning Check

**Setting up backup?** Read these:
1. Preparing for the Worst
2. The Night the Drive Died

**Adding physical feedback?** Try:
1. The Sound of the Garden
2. The Lights on the Shelf

**Scaling up?** Consider:
1. The Third Stone
2. Crossing the Subnet

---

## Contributing

Each journey follows the same structure:

```markdown
# Journey Title

*A one-line hook that draws the reader in.*

---

## The Story

[Pure narrative - what you do, see, experience]
[No technical explanations]
[Time jumps encouraged: "Three days later..."]

---

## What Just Happened

[Rewind to the beginning]
[Explain each moment from the story]
[Reference the story: "When you saw..."]

---

## Commands From This Journey

[Quick reference of commands used]

---

*Zen Garden Documentation — Journeys*
```

The story should feel like a short film. The explanation should satisfy curiosity the story created.

---

*Zen Garden Documentation — Journeys*
