# WITNESSES — live fleet proof

The bar. A v1 milestone isn't done until it matches or beats the PoC's
opening score. Recorded in the inventory tradition: what happened, what
proved it.

## PoC bar (2026-08-25, stone-leaded-sparkle workstation + 5-stone fleet)

- **Unprompted failover**: rake query hit sleeping stone, auto re-resolved,
  answered by another. (inventory: clients-companions)
- **Wipe recovery**: containers vanished → boot reconcile pulled 6 images,
  recreated with ports preserved, dormant/running desired-state honored.
- **Goodbye observed**: emerald-vale goodbye ×3 in pulse feed, then return —
  graceful restart seen garden-wide in seconds.
- **Adopted-service loop**: ollama::adopted flipped failing → thriving live.
- **Wall monitor**: pulse rendering 6/6 garden with per-peer freshness at
  32 evt/min.

## v1

- **W1 — two stones meet, and ask** (2026-08-25, the v1 room
  `239.255.42.199:7284` by default, Windows workstation, `garden` debug
  build).
  `stone-smoke-alpha` boots, chirps, and *asks the room who's here*;
  ten seconds later `stone-smoke-beta` does the same. Both `/api/v1/observe`
  views list both stones within ~2s of beta's boot — no heartbeat waited:
  - beta heard alpha's `discovery_response` instantly (alpha as
    `health=starting` hint — honest: capabilities unknown until alpha's own
    chirp arrives) while already holding beta's full truth from beta's chirp;
  - alpha answered beta's request via multicast (`answered discovery request`
    in the log), matching PoC moss behavior where the whole room hears every
    answer;
  - chirps carry v1 markers (`proto: zg/1`, monotonic `seq`, per-boot
    `boot_id`) over the v0-compatible core.
  - Also witnessed en route: L17 abort fired exactly as designed on a port
    collision (`startup step 'ingress-bind' failed: os error 10048`) — loud,
    named, no half-garden.
  - Wire correction caught before first contact: discriminators are lowercase
    on the v0 wire; v1's constants were fixed and re-pinned by fixture (L19).
  - Not yet witnessed: graceful-goodbye path (force-killed here; needs a
    console ctrl_c harness), expiry, cross-machine delivery in the v1 room.
    (PoC interop is not on the bar: v1 owns its topology by design.)

- **W3 — wipe recovery: the offering directory survives Docker's death**
  (2026-08-26, Windows workstation, release build). mongodb planted from the
  catalog manifest via the compile path; then **total destruction** —
  `docker rm -f`, `docker rmi mongo:7`, moss killed. Only
  `~/.zen-garden/offerings/mongodb/` remained: record.json, plan.json,
  events.jsonl, configs/, volumes/.
  - Moss restarted → registry index rebuilt from directories → boot
    convergence detected missing+Running → re-placed from stored spec →
    image pulled fresh → **same host port bound (51279)** via preferred-
    ports-as-placement-constraint → config file materialized and mounted →
    status Running.
  - The events ledger narrates the entire life: `1 Placed, 2 Healed,
    3 Healed` — hash-chained, tamper-evident.
  - En route, three real bugs caught and fixed: wake/converge registered
    stale clones undoing their own marks; preferred-port lookup compared
    host-ports against container-ports (namespace confusion); Docker's
    bind-placeholder habit left directory-shaped corpses where config files
    belonged — now defensively cleared.
  - The rehydration contract held: **everything needed to resurrect the
    offering lived outside Docker.**

- **W2 — the room crosses the LAN** (2026-08-25, v1 room `239.255.42.199:7284`,
  three physical Debian stones + the Windows workstation; release binaries
  from `installer/v1` perennial builder, deployed to `~/zen-v1/`, PoC fleet
  untouched by construction).
  - Field identities minted on first boot, one per stone, each drawn from
    the well and collision-checked against the live room:
    **stone-translucent-clearing** (192.168.1.82),
    **stone-crystalline-dune** (192.168.1.111),
    **stone-tranquil-pass** (192.168.1.195) — companion modality (L23).
  - Cross-machine ask/tell witnessed in both directions: stone-thrown raw
    datagrams and moss heartbeats cross hosts; a rake running ON a stone
    attached across the LAN and rendered all three.
  - Cross-machine attachment from the workstation: `rake observe` rendered
    the full room; `rake find dune --json` returned the standard garden
    view with v1 markers (`proto: zg/1`, `seq`, per-boot `boot_id`) intact.
  - Scar harvested (**L24**): the first minutes were silent — IGMP snooping
    needed a querier cycle before forwarding our group. Convergence, not
    failure; witnesses must budget for it.
  - Diagnosis trail worth keeping: posture counters (`bad_json` delta)
    turned "is it the network?" into a measurable question; unicast health
    fetch isolated the failure layer without leaving any machine.
  - Deployment footprint left deliberately: binaries + minted identities
    remain at `~/zen-v1/` on the three stones (processes stopped after the
    witness). D8 (same-host port sharing) remains open — one moss per box
    here.
  - Not yet witnessed: graceful-goodbye, expiry, appliance modality (D9),
    offerings.
