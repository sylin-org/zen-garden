# Zen Garden Hardware Vision: From Stones to Shards

> **Status:** Vision / Exploration  
> **Origin:** Conversation between Leon and Claude, February 2026  
> **Purpose:** Capture the full vision for future reference and development

---

## The Spark

A friend said: *"It would be so cool if you could do it cyberpunk/manga style, each compute module a shard that you slot and it lights up like a cyberdeck."*

That offhand remark crystallized something that had been forming quietly across the entire Zen Garden project — the idea that infrastructure can be beautiful, that computation can be physical and legible, and that there's a product buried inside this philosophy that could sustain the project without betraying its values.

---

## Design Philosophy: The Modder's Platform

Zen Garden is itself a mod. The entire project began by taking discarded hardware — machines someone else decided were done — and making them do something they were never designed to do. The modder mentality isn't optional in Zen Garden. It's essential. It's the founding act.

This means the hardware vision must embody the same principle. The dock is not a finished product you use as-is. It's a starting point. The Socket Specification defines the minimum contract — power, network, airflow, slots — and everything beyond that is an invitation.

**Design for modification, not against it:**

- **Open frame options** — chassis designs that expose internals rather than hiding them, so owners can see, reach, and change things
- **Documented mounting points** — standardized screw holes, rail positions, and cavity dimensions published as part of the spec, so modders can design add-ons that fit precisely
- **Accessible power taps** — clearly marked, safely fused points where modders can draw power for add-ons (fog generators, underglow LEDs, speakers, secondary displays)
- **Removable panels** — the base compartment, side panels, and top cover should be designed for easy removal and replacement with custom alternatives
- **Published CAD files** — full mechanical drawings of the dock and Shard envelope released openly, so the community can 3D print custom enclosures, brackets, and accessories

The fog mod, the underglow, the Cricket speaker in the base, the custom cyberdeck enclosure — these aren't deviations from the product vision. They're the product vision working as intended. If someone buys a dock and never modifies it, that's fine. But if someone turns it into a fog-breathing neon shrine, that's not a hack — that's Zen Garden fulfilling its purpose.

**The modder's ecosystem becomes a community flywheel:**

People share mods. Mods attract new users who want that experience. New users bring new ideas. Some of those ideas feed back into the official spec. The platform evolves through its community rather than despite it.

This is what separates Zen Garden hardware from every other compute platform on the market. A Turing Pi is a product. A Zen Garden dock is a canvas.

---

**A Shard is a slab-format single-board computer that slides into a dock. The dock provides power and network. That's it.**

The dock is dumb infrastructure — a power supply, a small Ethernet switch, and a set of slots with guide rails. Every slot presents two connectors at the back wall: a standard RJ45 Ethernet jack and a 3-pin power connector. The Shard slides in along rails that force correct orientation and alignment, and at the end of travel both connectors seat simultaneously. The user never touches a cable or a connector directly.

Within seconds of insertion, the Moss daemon on the Shard discovers the garden, announces its capabilities, and the garden rebalances. The Firefly LEDs on the Shard bloom to life. Its OLED display wakes and shows its name.

You just made your system more powerful with a physical gesture.

---

## Naming: Stones and Shards

The vocabulary splits naturally into two tiers that coexist within the same garden:

**Stone** — the base concept. Any machine running Moss. A Wyse thin client is a Stone. An old laptop is a Stone. A Raspberry Pi duct-taped to a shelf is a Stone. Free, open, everyone's entry point. This never changes.

**Shard** — the premium physical product. A purpose-built board conforming to the Zen Garden Socket Specification, with integrated RGB Firefly edge lighting, an OLED display, and the standard rail-mount connector interface. You slide it into a dock and it *comes alive*.

Every Shard is a Stone, but not every Stone is a Shard. Moss treats them identically — they're all just machines on the network with capabilities. The distinction is purely physical: a Shard is a Stone that was *designed* to be part of this ecosystem, with the form factor, the lighting, the display, and the ritual of slotting in.

The word itself works on every level. A shard is a fragment of something larger — which is literally what distributed computing is. Each one is a piece of the whole garden's capability. It carries cyberpunk and anime resonance. And it sounds like something you'd *collect*: "I just added a new shard" hits completely differently than "I provisioned an additional node."

---

## Vocabulary Update

The Shard concept extends the existing Zen Garden vocabulary:

| Term | Meaning |
|---|---|
| **Stone** | Any machine running Moss — the universal base unit. Includes surplus thin clients, old laptops, SBCs, anything. |
| **Shard** | A premium, purpose-built Stone conforming to the Socket Specification. Features RGB Firefly edge lighting, OLED display, and rail-mount form factor. Every Shard is a Stone; not every Stone is a Shard. |
| **Dock** | A passive chassis providing power, Ethernet, and forced airflow to 4–8 Shard slots via guide rails, centralized fans, and the Socket Specification connectors. Slot beds tilt 0–30° for cooling and aesthetics. |
| **Firefly** | Ambient LED feedback — on a Shard, this becomes the RGB perimeter lighting driven by Moss state data. |
| **Cricket** | Spatial audio monitoring — can map to physical Shard positions within a dock. |

---

## The Socket Specification

The Zen Garden Socket Spec is intentionally minimal — small enough to fit on one page.

### The Slot (Dock Side)

Each slot consists of:

- **Two guide rail channels** — parallel tracks that accept the Shard's edge profile, constraining movement to a single axis (slide in / pull out)
- **One RJ45 female jack** — standard gigabit Ethernet, mounted at the back wall of the slot
- **One 3-pin power D-type female connector** — mounted at the back wall adjacent to the RJ45, at a defined offset

The rail channels handle mechanical alignment. The connectors engage at the end of travel. Both connectors are recessed behind the back wall plane so they're protected when the slot is empty.

### The Shard (Module Side)

Each Shard module conforms to:

- **A defined board envelope** — width, height, depth (dimensions TBD, to be determined through prototyping)
- **Rail edges** — matching profiles along two long edges of the board that engage with the dock's guide channels
- **An air duct channel** — a shaped cutout or channel along the underside of the board, conforming to a defined profile, allowing dock-supplied airflow to pass across hot components
- **One RJ45 male connector** — at the rear edge, position defined relative to rail edges
- **One 3-pin power D-type male connector** — at the rear edge, adjacent to RJ45 at the defined offset

### The Electrical and Mechanical Contract

- **Power:** DC voltage (likely 12V, TBD), maximum wattage per slot (TBD, likely 30–65W range)
- **Network:** Standard gigabit Ethernet, no PoE required
- **The three power pins:** Positive, negative, and a sense/ID pin (the third pin could carry a simple identification signal or remain reserved)
- **Airflow:** Dock provides forced air through the slot bed; Shard's duct channel profile is part of the spec to ensure compatible airflow path

### Why Separate Power from PoE

PoE tops out at approximately 25W standard (802.3af) or 71W for PoE++ (802.3bt), and requires PoE-capable switch hardware which adds significant cost. A dedicated power connector fed by a commodity DC power supply in the dock can deliver whatever wattage the spec defines, cheaply and reliably. The Ethernet side stays plain gigabit with no PoE negotiation overhead.

### The Power D-Type Connector

The current candidate is a **3-pin power D-type connector**. Advantages:

- Robust, rated for decent current
- Standardized, cheaply sourced components
- The D-type shell provides mechanical keying — cannot be inserted wrong
- Panel-mountable, sits flush on the dock's back wall
- Physically sturdy enough for repeated insertion cycles
- Satisfying positive-lock tactile feel

**Design consideration:** D-type connectors are designed for cable-to-panel mating. For a slide-in dock, the alignment and insertion force need to work smoothly alongside the RJ45's latch mechanism. The rail system handles this — the mechanical guides ensure both connectors engage at the correct angle and depth simultaneously. This needs prototyping to validate.

---

## The Dock

A dock is a simple appliance:

- **4 to 8 slots** (configurable by model)
- **An internal gigabit Ethernet switch** connecting all slots, with one or more uplink ports for connecting the dock to a broader network
- **A DC power supply** (internal or external brick) feeding all slots
- **No compute on the dock itself** — the dock is purely passive infrastructure
- **Physical status indicators** per slot — at minimum, Firefly LED routing from each Shard to visible positions on the dock exterior
- **Centralized airflow system** — bottom-intake fan(s) pushing air through per-slot ducts
- **Adjustable slot angle** — slot beds rotate 0–30° from horizontal

The dock's job is to present power, network, and airflow to each slot. All intelligence lives in Moss on each Shard.

### Airflow and Tilt

Thermal management is centralized in the dock rather than on individual Shards. The design:

**Bottom-intake fan(s)** pull fresh air from underneath the dock chassis. Air is pushed upward through **integrated duct channels** — each slot has a duct path molded into its bed that directs airflow across the Shard's hot components (CPU, RAM, voltage regulators). Exhaust exits from the top or rear of the dock.

This means **Shards don't need their own fans.** The board profile includes a duct channel as part of the Socket Specification — a shaped cutout or channel along the underside of the Shard that the dock's airflow passes through. Simpler boards, quieter operation, centralized airflow management.

**The slot beds are adjustable from 0° to 30°.** Each slot can tilt, angling the Shard from flat to a 30-degree lean. This serves multiple purposes:

- **Thermal:** Angled surfaces increase exposed area for convective cooling and reduce turbulent wake effects between tightly packed adjacent boards. At 30°, each Shard has clear air separation from its neighbors.
- **Structural:** A fanned arrangement distributes weight more evenly and gives each Shard's OLED display and Firefly edge lighting better viewing angles.
- **Aesthetic:** A row of angled Shards looks like crystal formations growing from the chassis, or cards fanned out in a hand. At full tilt with RGB edges glowing, this is the cyberdeck visual your friend was imagining.

The tilt mechanism could be as simple as a notched hinge at the base of each slot bed, with discrete angle positions (0°, 10°, 20°, 30°) that click into place. Or a unified adjustment where all slots tilt together via a single dial or lever on the dock's side.

**Visual narrative:** Someone running heavy workloads cranks the angle up — the dock physically *opens* when it's working hard. That's a legible signal even from across the room.

### Dock Variants (Potential)

- **Desktop dock** — 4 slots, compact form factor, designed to sit on a desk as a display piece
- **Rack dock** — 8 slots, 1U or 2U form factor for server rack mounting
- **Portable dock** — 4 slots, battery-capable, for field deployment or demonstration use

---

## The Shard as Art Object

This is where the vision transcends infrastructure.

### Firefly-Embedded RGB Lighting

A Shard is a dark PCB — matte black or smoke translucent — with an embedded RGB LED strip running the entire perimeter behind a diffuser edge. The lighting is driven by data already available from Moss:

| State | Visual Effect |
|---|---|
| Idle | Slow breathing animation in the Shard's identity color |
| Under load | Color shifts from cool blue toward warm amber proportional to utilization |
| Service arriving | Ripple effect as a service migrates onto the Shard |
| Being drained | Slow fade-out as services migrate away |
| Donated compute | Distinct color (e.g., soft violet) indicating cycles are being shared |
| Error / attention needed | Gentle pulse in a warning color |

Because each Shard carries its own color identity, you read your garden at a glance. "The blue one's running Ollama. The green pair are handling vector database replicas. The amber one is hot because it's transcoding." No dashboard. No screen. Just light.

### OLED Display

Each Shard carries a small OLED screen on its exposed face — visible when slotted into the dock. This display is driven by Moss and shows contextual information:

- **Shard identity** — its name, assigned by the owner
- **Current state** — running services, CPU/memory utilization as a tiny real-time graph
- **Donation stats** — hours contributed, current project benefiting from donated cycles
- **Personality** — a pixel art avatar, icon, or visual theme assigned by the owner

The OLED transforms each Shard from anonymous hardware into a character. People will name their Shards. "Koji is running my Ollama instance. Suki handles vector search. Hana's been donating to the genomics project all week." That's not a server rack — that's a *crew*.

Community-designed OLED themes become another customization vector. Someone will make a Shard face that shows an anime character whose expression changes with CPU load — calm at idle, fierce under heavy compute, sleepy during low-priority donation work. Screen themes can be shared, remixed, and collected.

### The Ritual of Adding Compute

The experience of expanding your garden is physical and immediate:

1. You take a new Shard out of its packaging
2. You slide it into an open slot on the dock — the rails guide it
3. It clicks into place
4. The edge LEDs bloom to life
5. The OLED wakes — displays the Shard's name and a greeting
6. Within seconds, Moss discovers it and the garden rebalances
7. You just made your system more powerful with a gesture

This is the cartridge-loading pattern — Game Boy, Neo Geo, server blades. Humans intuitively understand "slot and activate." It makes distributed computing tangible and joyful.

### Community Customization

The form factor invites personalization:

- **Custom PCB art** on the Shard face — anime characters, circuit patterns, garden motifs, geometric designs
- **Limited edition Shards** with different diffuser tints, edge colors, or OLED bezels
- **Custom OLED themes** — shared and traded within the community, from minimalist system monitors to expressive character animations
- **Clear acrylic docks** that expose the internal wiring and switch as part of the aesthetic
- **3D-printed custom enclosures** — someone will build a dock that looks like a sword hilt with Shards as blade segments, or a wall-mounted shrine, or a cyberdeck console
- **Named and personalized Shards** — owners assign names, avatars, and identities to each one

None of this changes the software. Moss doesn't care if a Shard is a matte black RGB slab with an anime OLED face or a Wyse thin client from 2015. They're all just Stones to the garden.

---

## The Sustainability Model

### The Principle

**Software is always open and given freely.** This is non-negotiable. Anyone can build a garden from e-waste, surplus hardware, old laptops — that's the core mission and it stays untouched.

### Revenue Through Hardware

The commercial product is the curated, beautiful hardware experience:

- **Official Zen Garden Docks** — manufactured and sold with the project's branding
- **Official Zen Garden Shards** — reference-design compute modules with integrated Firefly RGB lighting and OLED displays
- **Trademark licensing** — "Works with Zen Garden" or "Official Zen Garden Shard" certification for third-party hardware manufacturers (not software licensing, which stays open)

The margin on hardware sales funds ongoing development of the open-source software that benefits everyone.

### Why This Doesn't Conflict

The model is additive, not extractive. The commercial hardware is a premium experience layered on top of a free foundation:

- Free tier: Build a garden from whatever hardware you have (Stones), guided by community docs and the compatibility list
- Premium tier: Buy a dock and Shards for a polished, beautiful, plug-and-play experience

This is the same model used successfully by Pine64, System76, Turris, and others — open-source software funded by purpose-built hardware.

### Phased Approach

1. **Phase 1 (Now):** Publish a hardware compatibility list and build community around tested surplus hardware setups (Stones)
2. **Phase 2:** Offer a "Zen Garden Kit" — curated off-the-shelf components bundled with setup guides and branding
3. **Phase 3:** Design and sell the custom dock with integrated Firefly lighting and purpose-built Shard modules with OLED displays
4. **Phase 4:** Open the hardware spec for third-party Shard manufacturers, offer certification program

Each phase validates demand before committing capital to the next.

---

## The Social Impact Flywheel

This is where the vision reaches its full scope.

### Donated Compute

Someone buys a dock and a few Shards because they look incredible and they want a homelab. They run their own services — AI experiments, media server, development environments. But when their garden is idle at 3am, those Shards are glowing softly in a distinct color, donating cycles through a Meadow bridge to:

- A university genomics lab sequencing data on a shoestring budget
- A classroom in a country where cloud compute is unaffordable
- A distributed protein folding or climate modeling project
- Students learning distributed systems on real infrastructure

And the donor can *see* it happening. The Fireflies shift color when running donated workloads. The OLED switches to show the project name and progress. You wake up, glance at your dock, and the Shards are shimmering in violet — their little screens showing "Genomics Lab — 12.4 hrs contributed." That's not a notification. Not a badge. It's a living, ambient signal that your hardware is part of something bigger.

### The Flywheel

```
People buy Shards because they're beautiful and functional
    → Revenue keeps the project alive
        → Software stays free, anyone can build a garden from e-waste (Stones)
            → Shard owners donate idle compute through federation (Meadows/Bridges)
                → Donated compute goes to students and researchers who can't afford cloud
                    → Some students build their own gardens from old laptops (Stones)
                        → Some eventually buy Shards
                            → The garden grows
```

### Social Proof as Marketing

The social proof is physical and shareable. Someone posts a photo of their glowing dock — each Shard's OLED showing its name and stats: *"My garden donated 400 GPU-hours to protein folding this month."* That sells more Shards than any ad campaign ever could. The community markets itself because the product is inherently visible and meaningful.

---

## Compatibility with Existing Zen Garden Architecture

The hardware vision requires **zero changes** to the existing software architecture:

| Component | How It Relates |
|---|---|
| **Moss daemon** | Runs on each Shard exactly as it runs on a Wyse thin client — discovers, announces, serves |
| **Wishful discovery** | Applications on Shards still declare wishes; the garden still fulfills them |
| **Fitness scoring** | Shard capabilities feed into the same scoring system as any other Stone |
| **Service migration** | Moving ceremonies work identically — a Shard is just another Stone |
| **Fireflies** | The existing Firefly protocol drives the RGB edge lighting — same data, different output device |
| **Cricket** | Spatial audio can map to physical dock positions |
| **Meadows / Bridges** | Federation for donated compute uses existing garden-to-garden protocols |
| **Offerings** | Service templates deploy to Shards through the same offering system |

The dock's internal Ethernet switch is transparent to Moss — each Shard appears as a normal network peer. The dock is invisible to the software layer.

---

## Open Questions for Prototyping

### Physical

- Exact Shard board dimensions — what compute module form factor to target? (CM4? Custom? Multiple?)
- Rail profile design — material, tolerance, retention mechanism
- Connector engagement depth — ensuring RJ45 latch and power D-type seat simultaneously at end of travel
- Duct channel profile — shape, dimensions, and position within the Shard board spec; molded into PCB substrate or a separate structural layer?
- Fan selection — single large fan vs. multiple small fans; noise profile; CFM requirements per slot
- Tilt mechanism — individual per-slot adjustment or unified gang tilt? Notched detent positions or continuous?
- Connector reliability at angle — RJ45 and power D-type must maintain solid contact across the full 0–30° tilt range
- Firefly LED strip integration — power draw, diffuser material, control interface (addressable LEDs via SPI/I2C from the Shard's GPIO?)
- OLED display selection — size, resolution, interface (I2C/SPI), viewing angle from dock-mounted position at various tilt angles
- OLED bezel and protection — flush mount, recessed, or under a transparent window?

### Electrical

- DC voltage selection — 12V is common but some modules prefer 5V; a dock-side regulator per slot adds cost
- Maximum wattage per slot — determines power supply sizing and thermal budget
- Sense/ID pin protocol — simple resistor ID? I2C device identification? Or just reserved for future use?
- OLED power budget — included in the Shard's power envelope or separately supplied?

### Product

- Bill of materials and target price point for dock + Shards
- Manufacturing partner or self-manufacturing (low volume initially)
- Certification process and costs for "Works with Zen Garden" trademark
- Community feedback and demand validation before committing to hardware
- OLED theme SDK — how do community members create and distribute custom Shard display themes?

### Modding and Community

- Chassis design — open frame, removable panels, or both as separate product variants?
- Power tap specification — voltage, max current, fuse rating, connector type for mod power points
- CAD file release format — STEP, STL, or both? What license?
- Mod gallery / community showcase — platform for sharing designs, print files, wiring diagrams?
- Safety guidelines — how to document safe mod practices without being restrictive?
- Dock base compartment dimensions — how much space to leave for user add-ons (fog generators, speakers, underglow, etc.)?

### Social

- Governance model for donated compute — who decides where cycles go?
- Privacy and security for donated workloads
- Metrics and transparency — how donors see their impact
- Partnership model with universities and research institutions

---

## Inspirations and Reference Points

- **Turing Pi 2** — 4x CM4 slots on a mini-ITX board with shared Ethernet switch and power; closest existing product to the dock concept
- **Compute Blade by Uptime Lab** — 1U rack system with CM4 blades and PoE; demonstrates the blade-in-rack approach
- **DeskPi Super6C** — 6x CM4 on one board; shows density possibilities
- **Neo Geo cartridge system** — the aesthetic and ritual of slot-and-play
- **PC modding culture** — decades of community-driven hardware customization proving that people will invest enormous creativity into making their machines beautiful and personal
- **Mechanical keyboard community** — a direct parallel: functional computing hardware elevated to collectible art through customization, community designs, and group buys
- **Pine64, System76, Turris** — open-source projects funded by hardware sales
- **BOINC / Folding@Home** — distributed donated compute for science, as a model for the social impact layer

---

## Summary

Zen Garden's hardware vision is an extension of its founding philosophy: **joy in infrastructure, physicality over theater, democratized computing.** And at the deepest level: **the modder's mentality is not optional — it's essential.**

The software remains free and runs on anything — every machine is a Stone. The Shard is the premium expression: a purpose-built module with RGB Firefly lighting, an OLED display with personality, and a form factor designed for the ritual of slotting in. The dock is not a finished product — it's a canvas, designed to be modified, personalized, and made your own.

The revenue from Shards sustains the project. The donated compute creates social impact. The OLED shows you what your hardware is doing — for you and for others. And the community turns every dock into something no one at a factory could have imagined.

*Art that computes. A canvas that invites.*
