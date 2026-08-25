# Zen Garden: A Video Script (v2)

**Target runtime:** 10-12 minutes  
**Format:** Hands + voice, no on-camera presenter  
**Location:** Snowy deck + indoor terminal shots  
**New in v2:** Cost story, circular economy, "garden gets stronger"

---

## COLD OPEN (0:00 - 1:15)

[VISUAL: Close-up of the Kangaroo MD2B lying flat on weathered wood. Snow visible in background. Silent.]

**VOICE:**
Forty dollars.

[Beat. Hold the shot.]

That's what this cost. A Kangaroo MD2B. It was supposed to be a portable PC you could plug into any TV. It wasn't very good at that.

[Hand enters frame, picks it up, turns it over]

Someone gave up on it. Listed it on eBay. I bought it.

[Hand sets it back down. Camera pulls back to reveal the full deck setup: five stones, three fireflies glowing softly, snow on the ground]

And right now, it's part of this.

[Hold on the wide shot. Cricket audio fades in - gentle bloops, soft ambient sounds. The fireflies pulse.]

A distributed compute cluster. Five machines. Total cost...

[DIAGRAM: Price breakdown animates in]

A hundred and ninety-two dollars. And fifty cents.

[TITLE CARD: "Zen Garden" - simple, clean]

---

## ACT 1: THE STONES (1:15 - 2:45)

[VISUAL: Closer shot of the three Wyse 5070s standing vertically]

**VOICE:**
These are Wyse 5070 thin clients. Thirty-five dollars each. They were built for call centers and bank branches - locked-down terminals where nobody trusted the user with a real computer.

[Hand touches one]

When those places upgraded, these became e-waste. Technically functional. Officially garbage.

[CUT TO: The dx0q]

This one's a Wyse dx0q. Twenty-five dollars. A little more powerful. Same story.

[CUT TO: The Kangaroo again]

And this. The forty-dollar impulse buy someone regretted.

[WIDE: All five together]

None of these were supposed to work together. Different product lines. Different years. Different purposes.

[CUT TO: Terminal screen]
[TYPED: garden-rake discover]

```
Discovering stones...

  stone-coral-prairie    5 offerings    thriving
  stone-amber-falls      2 offerings    thriving  
  stone-quiet-brook      1 offering     thriving
  stone-morning-mist     3 offerings    thriving
  stone-silver-leaf      idle           thriving
```

[VISUAL: Back to hardware]

**VOICE:**
But they found each other. No configuration. No central server. They just... noticed each other on the network and said hello.

[DIAGRAM: mDNS discovery - stones appearing, mesh forming]

---

## ACT 2: THE VOCABULARY (2:45 - 4:00)

[VISUAL: Close-up of a firefly, pulsing warm amber]

**VOICE:**
I call these "stones." Not nodes. Not servers. Stones.

[Hand gestures across the setup]

And that's not just poetry. The vocabulary shapes how you think about the system.

[DIAGRAM: Node vs Stone comparison]

A "node" is interchangeable. Abstract. When a node dies, you replace it. You don't mourn a node.

A stone has weight. History. That cracked laptop that ran your database for three years? That's not a node. That's a stone. You remember it.

[CUT TO: Terminal showing the status page]

The software running on each stone is called Moss. Because moss grows on stones.

The command-line tool is called Rake. Because you tend a garden with a rake.

[VISUAL: Back to fireflies]

These LED matrices are Fireflies. The nervous system made visible.

[VISUAL: Cricket audio becomes more prominent]

And the audio is called Cricket.

[Hold. Let the sound breathe.]

The metaphor isn't decoration. The metaphor is the architecture.

---

## ACT 3: THE LIVING SYSTEM (4:00 - 5:30)

[VISUAL: The fireflies, close enough to see individual pixels]

**VOICE:**
Here's what I didn't expect. I started checking on it. Not because anything was wrong. Just... to see how it was doing.

[The fireflies pulse - slow, meditative rhythm]

When the system is idle, the fireflies breathe slowly. One or two lit at a time. Warm white, like actual fireflies at dusk.

[DIAGRAM: Tempo breathing - idle vs busy side by side]

When the system is busy, the rhythm picks up. More fireflies. Faster pulses. You can see it thinking.

[SPLIT SCREEN: Terminal deploying a service, firefly blooming green]

[TYPED: garden-rake offer redis]

**VOICE:**
Deploy a service... it blooms.

[Firefly settles back with occasional blue glints]

Then back to baseline. But now with blue mixed in. The garden knows something is running.

[VISUAL: Wide shot of the stones with fireflies]

This isn't a dashboard. Dashboards demand attention. This is presence. You notice it the way you notice sunlight moving across a room.

---

## ACT 4: THE SEED-BANK (5:30 - 7:00)

[VISUAL: Close-up of the SanDisk drive on the deck]

**VOICE:**
This is a terabyte SanDisk. I found it in a drawer. Cost me nothing.

[Hand touches it]

In Zen Garden, it's called a seed-bank. Portable storage that any stone can use.

[DIAGRAM: Seed-bank migration - showing app on leaf saving file]

Watch what happens. An application on this stone saves a file. The file goes to the seed-bank.

[Hand reaches in, unplugs the SanDisk]

[Fireflies react - brief state change. Cricket audio shifts.]

Now I physically move it.

[Hand carries the SanDisk to a different stone, plugs it in]

[DIAGRAM: File being retrieved from new location]

The application asks for the file again. The garden finds it. Different stone. Same file. The app didn't notice.

[CUT TO: Terminal]
[TYPED: garden-rake status seed-bank]

```
seed-bank-zen-garden
  Location: stone-amber-falls (was: stone-coral-prairie)
  Status: online
```

**VOICE:**
Storage is singular. Access is distributed. You can pick up the brain of the system and move it.

---

## ACT 5: THE ECONOMICS (7:00 - 8:30)

[VISUAL: The full garden on the deck, breathing]

**VOICE:**
Let's talk about money.

[DIAGRAM: Cost comparison - Zen Garden vs Cloud, animated over 5 years]

This setup - five stones, a terabyte of storage, physical feedback - would cost maybe two hundred dollars a month in the cloud. Conservatively.

[Watch the lines diverge on the graph]

The break-even happens... here.

[The marker appears almost immediately]

About three weeks.

[Hold on the diverging lines]

After that, every month is savings. By year five, the cloud option would have cost over twelve thousand dollars. This? Still just sitting here. Still the same hundred and ninety-two fifty.

[VISUAL: Back to hardware on the deck]

**VOICE:**
But it's not just the money that stays. The hardware stays.

[Hand touches a stone]

The money I send to AWS every month buys... nothing. Access. Rental. When I stop paying, it vanishes.

This? This is still here. Still working.

[Beat]

And when one of these finally dies...

---

## ACT 6: THE GARDEN GETS STRONGER (8:30 - 9:45)

[VISUAL: Close-up of RAM module, SSD, spare parts on a shelf - if available. Otherwise, hands holding the Kangaroo.]

**VOICE:**
Cloud instances don't leave anything behind. When you stop paying, they just... disappear. There's nothing to bury. Nothing to harvest.

But a stone that finally dies?

[Hand mimes removing a component]

The RAM goes into another stone. The one that was limping along on four gigs is now running eight. It can handle workloads it couldn't before.

[Beat]

The SSD becomes a seed-bank. The chassis becomes spare parts.

[VISUAL: Back to the living garden]

**VOICE:**
The garden doesn't shrink when a stone dies. It *accumulates*. The surviving stones get stronger. The parts pool. The knowledge of what works and what fails concentrates.

[DIAGRAM: Could show "Year 1" vs "Year 5" capability - same cost, more RAM, better parts]

Year five of cloud computing? Same capability you're renting. Same invoice.

Year five of a garden? The stones are *upgraded*. From salvage. From their own dead.

[Beat]

The garden composts its own.

---

## ACT 7: THE POINT (9:45 - 11:00)

[VISUAL: Wide shot - the garden breathing on the deck. Snow. Dusk light if possible.]

**VOICE:**
I keep thinking about a classroom somewhere. Maybe it's in Nairobi. Maybe it's in rural Ohio. Somewhere with old laptops and zero budget.

[Hold on the hardware]

They're trying to learn distributed systems - one of the hardest and most important subjects in computing. But they can't *see* it. They can only read about it.

[VISUAL: The fireflies pulsing]

With this... they could see it. Watch services discover each other. Unplug a machine and watch failover happen. Feel the system breathe.

[Hand enters frame, hovers near a stone]

The feedback matters. When you can see the system, concepts stop being abstract.

[Hand touches the stone - the "tend" gesture]

[TYPED (via overlay): garden-rake tend stone-coral-prairie]

[Firefly sparkles. A soft boop from the speakers.]

You're not debugging a black box.

[Hold on the sparkle]

You're tending a garden.

---

## CLOSING (11:00 - 12:00)

[VISUAL: The wide shot. Snow. Stones. Fireflies pulsing. Cricket audio gentle underneath.]

**VOICE:**
A hundred and ninety-two dollars. Five machines nobody wanted. Some LEDs. Some code. And whatever was lying in a drawer.

[Long hold. Let the garden breathe.]

This isn't going to replace AWS. It's not trying to.

[Beat]

But it's mine. I can see it. I can hear it. When something goes wrong, I'll know - not because a pager went off, but because the rhythm changed.

[VISUAL: Close-up of a single firefly, pulsing slow]

And when a stone finally dies, the garden will grow stronger.

[TEXT ON SCREEN: "When you stand on stone, you can look up at the clouds and assess them honestly."]

[VISUAL: Back to wide shot]

**VOICE:**
Sometimes clouds bring rain you actually need. Sometimes they just drift past. Sometimes they block your view.

[Beat]

Stone doesn't fight clouds. Stone just... remains.

[The fireflies continue. Cricket audio continues. Hold for 5-10 seconds.]

[FADE TO BLACK - but the audio continues briefly, then fades]

---

## END CARD

[VISUAL: Simple title card]

**TEXT:**
Zen Garden  
[github URL]  

$192.50 · 5 stones · open source  
The garden gets stronger.

[Hold 5 seconds]

[FADE OUT]

---

# PRODUCTION NOTES

## What Changed from V1

1. **Added Act 5: The Economics** - the brutal break-even, cost comparison
2. **Added Act 6: The Garden Gets Stronger** - circular economy, RAM migration, composting
3. **Moved "The Point" (classroom) to Act 7** - now lands after the economics/sustainability argument
4. **New closing line** - "And when a stone finally dies, the garden will grow stronger."
5. **Runtime increased** - ~10-12 minutes to accommodate new material

## New Diagrams Needed

- [x] Cost comparison (animated, 5-year view)
- [ ] Optional: "Year 1 vs Year 5 capability" showing accumulated upgrades

## Shot List Additions

### Outdoor - Deck (add to existing list)
- Spare parts on shelf (RAM modules, SSDs) if available
- Hand miming component removal

### Diagrams (screen record)
- Cost comparison with scenario buttons (use "Small Team" for your setup)
- Let the break-even moment land - pause on it

## Key Beats to Nail

1. **"Forty dollars"** - the cold open hook
2. **"The metaphor is the architecture"** - the philosophy pivot
3. **"Three weeks"** - the break-even shock
4. **"The garden composts its own"** - the sustainability revelation
5. **Boop + sparkle** - the emotional payoff
6. **"The garden will grow stronger"** - the new closing promise

## Audio Notes

- Cricket audio should be present throughout, but subtle
- Let it come forward during the "Living System" section
- The boop at the tend moment should be clearly audible
- Consider letting garden audio run under the economics section too - reminder that this is *alive*, not just a cost spreadsheet

## Timing Targets (Revised)

| Section | Target Duration |
|---------|-----------------|
| Cold Open | 1:15 |
| The Stones | 1:30 |
| The Vocabulary | 1:15 |
| The Living System | 1:30 |
| The Seed-Bank | 1:30 |
| The Economics | 1:30 |
| The Garden Gets Stronger | 1:15 |
| The Point | 1:15 |
| Closing | 1:00 |
| **Total** | **~12:00** |
