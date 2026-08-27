# Orientation — one datagram's life

The read-me-second: after the README's *why*, before the law's *must*. This
page walks one stone's traffic from boot to expiry, because the whole system —
discovery, cache, surfaces, storage — is visible in that single life. Law
lives in `CODE-RULES.md`; this page teaches the shape the law protects.

**Status note (L7: self-description must be true):** the walk below describes
the accepted design (ADR-0004 + ADR-0005 §8). Landed through S2 — dynamic
chirp source, lean heartbeats, songs and framer, fixtures. The rich ask's
*answer* (step 3's inventory), the candidates pool, the URI cut, and storage
are slices ahead; see the epic map in the repo's continuation notes.

## The cast (each word guessable from the metaphor)

| Word | Gloss |
|---|---|
| **stone** | a machine running moss — the substrate things grow on |
| **moss** | the daemon; the quiet green layer doing the actual work |
| **garden** | the fleet; the room the stones share |
| **offering** | a named unit of work placed on a stone (`mongodb::default` — FQN per ADR-0003) |
| **chirp** | a lean heartbeat: who I am, that I'm here (`stone_chirp`) |
| **song** | the full voice: presence plus inventory, sung on change (`stone_song`) |
| **bank** | storage — after *seed bank*: what outlives the season (ahead) |
| **rake** | the CLI; what a gardener walks the rows with |
| **the cache** | one hot topology map per stone; what the room last said |

## The walk

**1 · A stone wakes.** First boot mints a GUIDv7 and draws a poetical name
from the naming well (`stone-{adjective}-{noun}` — glossary::naming). The
identity attaches to the *stone record*, never to the box; the offering
directory lives under `~/.zen-garden/` (ADR-0001). Health reads `starting`.

**2 · It asks the room.** Before announcing, the newcomer multicasts the rich
ask — *"who are you guys, and what do you have?"*
(`DiscoveryRequest::for_moss_rich`). You cannot contact what you don't know
exists; the opening question is how a stranger seeds its map in one exchange.

**3 · The room answers.** Every willing respondent replies
`DiscoveryResponse{stone, inventory}` — its card plus what it grows. Answers
arrive unicast, out of order, some lost. It doesn't matter: merge is by
revision arithmetic, not coordination — a block present in one frame outranks
an older rev wherever both are heard. ADR-0004 §2: *"revisions settle
disputes between mouths, not votes."*

**4 · The cache remembers.** Everything heard lands in ONE hot topology cache
(L22) through one ingest door, stored as the contract's own `ChirpFrame` —
the same sections the wire spoke (R3.9: records are paths). Liveness is
soft-state, stamped `received.last_seen`, expired by sweep. Knowledge heard
only through middlemen lands as TTL'd *candidates* — rumors until the named
stone's own frame promotes them. And self is never ingested: this stone's own
card is a *projection* of its local truth, rebuilt, never stored.

**5 · Ordinary life chirps.** Heartbeats are lean — anchors only: who, where,
how alive, `meta{boot_id, seq}`. Presence must not amortize inventory; the
PoC paid ~50% payload for fat chirps (COMM-0005) so v1 keeps the two
registers. Domain events stay inside (L18): modules never poll each other;
the wire's only cadence is the protocol's own.

**6 · Something changes, the stone sings.** An offering planted, rested,
woken — `OfferingChanged` bumps `svc_rev`, the announcer's debounce collapses
the burst, and a *song* goes out: presence re-anchored plus the dirty
inventory domains, quantized whole against the datagram budget by the framer
(~3.5 KB; a block rides entire or waits; `meta.part` is informational —
consumers never reassemble, revs make order irrelevant). Every peer's cache
heals from the song, or from one rate-limited rich ask when a rev says
*"you're behind."*

**7 · A drive is plugged in — news, not machinery (ahead).** Storage rides
the same envelope (ADR-0005 §8): a plugged bank is announced like any
offering (`bank_rev` beside `svc_rev`), capacity rides along as telemetry
but never triggers frames, and a bank's liveness is inherited from its
stone's heartbeat — *"announce loudly what you know; expire quietly what you
can't prove."*

**8 · A stone dies.** No goodbye? The sweep dims it on expiry — silently, in
the bounded way soft presence has always worked. On return, the boot_id
changed, the revs speak, and the cache heals within a heartbeat. If goodbye
was graceful, absence is announced authoritatively.

**9 · Surfaces render the one truth.** Moss's HTTP faces are projections of
the same cache and the same composed self-view (URI grammar per ADR-0004 §4:
`/stone`, `/stone/{ref}`, `/garden/stones`, `/offerings[/{fqn}]`). Rake
attaches to a moss and renders what it reports — never its own view of the
world (L21). Wire, cache, HTTP, CLI: one record, many mouths.

**A day-one honesty note (L24):** after joining the room, a silent first
minute is often the switches' IGMP querier converging, not the garden
failing. Rooms take a breath before they carry.

## Where the code lives

| What | Where |
|---|---|
| The frame, inventory map, song, framer | `src/v1/crates/contract/src/{chirp,discovery,song}.rs` |
| The wire's pinned shape (fixtures) | `src/v1/crates/contract/tests/wire_fixtures.rs` |
| Announcer (heartbeats, change songs) | `src/v1/crates/kernel/src/announce.rs` |
| The cache | `src/v1/crates/kernel/src/topology.rs` |
| Ask/tell the room | `src/v1/crates/kernel/src/{probe,responder}.rs` |
| A stone's own view + registry | `src/v1/crates/moss/src/{source.rs,offerings/}` |
| The CLI | `src/v1/crates/rake/src/main.rs` |
| The vocabulary | `src/v1/crates/glossary/src/` |

## Where the law lives (read in this order)

1. `docs/MEMORY.md` — the pointer index; durable truth lives where it points
2. `docs/v1/lessons.md` — L1–L26, the PoC's tuition, normative
3. `docs/v1/CHARTER.md` — mission, jobs, bets B1–B11
4. `docs/v1/CODE-RULES.md` — engineering law P0–P5
5. `docs/v1/decisions/ADR-0001..0005` — the big decisions, amended never edited
6. `docs/v1/OFFERINGS.md` — the core domain's design law
