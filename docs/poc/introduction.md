# Zen Garden: A Gentle Introduction

## What is Zen Garden?

Imagine tending a real garden. You don't micromanage every root and leaf—you prepare the soil, plant seeds, water occasionally, and let nature do its work. When frost threatens, you cover the tender plants. When pests arrive, you respond. The garden grows, adapts, and thrives through your gentle stewardship rather than constant control.

**Zen Garden brings this philosophy to your home computers and servers.**

Instead of wrestling with IP addresses, configuration files, and terminal commands scattered across machines, Zen Garden lets you *tend* your digital infrastructure. Your computers become **Stones**—the foundation of your garden. The applications you run (photo libraries, media servers, databases) become **Offerings**—living things you plant, nurture, and harvest.

---

## The Garden Metaphor

| Traditional IT | Zen Garden |
|----------------|------------|
| Servers | **Stones** — the bedrock your garden grows upon |
| Applications | **Offerings** — living services you cultivate |
| Backups | **Seeds** stored in **Seed Banks** — preserved for future growth |
| Updates | **Nourishment** — feeding your offerings to help them thrive |
| Backup/restore cycles | **Nurturing ceremonies** — careful rituals that protect what matters |
| Installation images | **Seeds** — the potential for new life |

---

## Why Does This Matter?

Most home infrastructure tools assume you *want* to be a systems administrator. They expose every knob and dial, expecting you to understand networking, containers, storage drivers, and orchestration.

Zen Garden assumes you want to **run services that work**—a photo library for family memories, a media server for movie nights, a password manager for security. The complexity should fade into the background like good garden soil: essential, but not something you think about daily.

---

## How Stones Find Each Other

When you set up a second computer with Zen Garden, something magical happens: *they find each other automatically*. No configuration. No "enter the IP address of your main server." They simply announce themselves like neighbors waving across a fence, build a shared understanding of the garden's topology, and begin cooperating.

This works through a **discovery cascade**—a series of increasingly broad searches:

1. First, check local memory (instant)
2. Then, broadcast on the local network (milliseconds)
3. Then, use standard discovery protocols (under a second)
4. Finally, check a registry for devices on other networks

Your applications don't need to know *where* services live. They ask for "the database" or "the photo library," and Zen Garden handles the rest—even if you move services between Stones.

---

## Physical Presence

Unlike cloud services hidden in distant data centers, your Zen Garden has *presence*. Optional companions bring your infrastructure into the physical world:

- **Cricket** — A small speaker that chirps, chimes, and hums with garden activity. A soft melody when backups complete. A warning tone when something needs attention. Your garden has a voice.

- **Firefly** — A 5×5 LED matrix that glows like fireflies in a summer garden. White lights drift when all is well. Amber when something's degraded. Red when action is needed. One glance tells you the garden's health.

- **Portrait** — A simple web page showing a Stone's identity, what it's running, and its current state. Mount a cheap tablet on the wall, and your infrastructure becomes visible art.

---

## Resilience Through Ritual

Zen Garden protects your data through **ceremonies**—careful, reversible rituals rather than risky one-shot commands.

Every significant change follows three phases:

1. **Collect** — Create a safety backup first
2. **Apply** — Make the change
3. **Verify** — Confirm everything works; automatically undo if not

Failed update? The garden rolls back before you notice. Drive died? Restore from your seed bank to new hardware, and the Stone rises again with its identity intact. Your applications reconnect automatically because they never knew the IP address—only the *name*.

---

## The Narrative Journeys

The documentation includes **26 narrative stories** that follow users through real scenarios. Rather than dry reference material, these journeys show Zen Garden through storytelling:

- **The First Stone** — Setting up your first computer, watching it come alive
- **The Night the Drive Died** — Disaster strikes; the garden recovers
- **When Stones Meet** — Adding a second computer, watching them discover each other
- **The Failed Update** — An update goes wrong; automatic rollback saves the day
- **The Sound of the Garden** — Giving your infrastructure a voice
- **Preparing for the Worst** — Building habits that protect your data

Each journey has three parts: the *story* (pure narrative), *what just happened* (technical explanation), and a *command reference* for those who want to try it themselves.

---

## Next Steps

→ **Ready to start?** [First Stone Guide](guides/first-stone.md)
→ **Want the philosophy?** [Philosophy Index](philosophy/README.md)
→ **Prefer to explore?** [Journey Index](journeys/README.md)
