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

- **W4 — rake speaks stone ops: one ceremony, whole lifecycle** (2026-08-26,
  Windows workstation, debug builds at `4e4d483b`, Docker Desktop world,
  50-manifest corpus via `MOSS_CATALOG_DIR`, offered against
  `127.0.0.1:7285`).
  - Full verb cycle witnessed in one sitting: `offer memcached` → planted,
    running, manifest image bound, port ledger filled; `explain` rendered
    the placed record with its decision log; `rest` → stopped-stays-stopped;
    `wake` → running again, remap recorded honestly (`D14`); `uproot` →
    container removed, directory unregistered, volumes preserved.
  - Refusals bind to their stone: a pinned `--stone` that answered HTTP 409
    (`'memcached' is a catalog offering; its manifest defines the image…`)
    aborted loudly instead of redirecting; a pinned unreachable endpoint
    refused to guess; a mid-pull plant that outlived a stale 3s client
    timeout surfaced as `read timed out` + honest local abort (mutations now
    carry their own 120s budget).
  - §5.2/§6.4 fidelity fix landed from this witness: plans record EVERY
    compatibility outcome (a healthy memcached logs its `low-memory-warning`
    rule as `no_match`), and catalog category is manifest truth, not the
    client's default.
  - Corpus recovery en route: `ollama-cpu.offering.yaml` carried a
    PoC-shaped `op: present`; migrated to the v1 grammar
    (`gpu.present … eq true`) — catalog back to 50/50 loaded.

- **W2 — the room crosses the LAN** (2026-08-25, v1 room `239.255.42.199:7284`,  three physical Debian stones + the Windows workstation; release binaries
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

---

## W5 — the ledger wins over sockets (2026-08-27, stone-crystalline-dune)

- `redis::ports` planted ad hoc with one named port (`default: 6379`); the
  arbiter drew home **7300** (tier flexible), the allocation rode the
  stored spec, and Docker showed the explicit binding
  (`0.0.0.0:7300->6379`) — never a dynamic assignment.
- `rest` -> `wake`: the ledgered home **survived the lifecycle**
  (`port_map {default: 7300}` re-emitted, binding re-created explicitly).
  A rested offering's claim beat anyone's socket probe (L26).
- D14 closes on this witness (arbiter + directory + adapter all in the
  path, neighbour choreography observed end-to-end).

---

## W6 — the garden knows itself, witnessed live (2026-08-27)

Deployed to the fleet and witnessed end-to-end, from this workstation
(fleet: 192.168.1.111 + 192.168.1.195 live; 192.168.1.82 powered off —
absent from this witness, joinable later by the same procedure).

- **Fleet upgrade (S8)**: `installer/v1` perennial Docker builder produced
  linux-x64 release binaries (moss 6.9 MB, rake 1.9 MB); staged via pscp to
  `~/zen-v1/`, swapped atomically (`moss.new` -> `moss`), restarted under
  `MOSS_RUNTIME=docker` (appliance modality — see finding below). Both
  stones healthy on `proto: zg/1` in seconds.
- **Finding — self-ingest**: the first convergence check showed each stone
  TWICE in its own view (spliced self + an ingested peer row): multicast
  loop-back was seating self among the peers. Fixed at the ingest door
  (topology `set_self_id`; `6566fad3`), redeployed, and the fix witnessed:
  each view then showed exactly self (splice) + peer. Regression test
  `self_frames_never_seat_self_among_peers` pins it. This is why witnesses
  are the definition of done.
- **Mutual presence**: within one heartbeat of boot, each stone's
  `/garden/stones` showed the other (`crystalline-dune` <-> `tranquil-pass`),
  both thriving, chirp counts rising. L24 honored: convergence budgeted.
- **Plant on A visible from B**: `redis::witness` (ad-hoc, redis:7-alpine)
  planted on crystalline-dune through the grammar face
  (`POST /api/v1/offerings/redis::witness`); within one interval
  tranquil-pass's cache carried it — `svc_rev=2`, items include
  `redis::witness`. The sectioned v3 record rendered over HTTP on the way.
- **Rev-heal drill**: killed and restarted crystalline-dune's moss. Its
  cache refilled from the room (boot rich ask -> peer row present) and its
  own offering REHYDRATED from its directory (registry rev 1, item present;
  S5.5's first field test). Stale-rev arithmetic held on the survivor.
- **USB adopt ceremony, garden-wide (ADR-0005 §8)**: a removable volume was
  present on tranquil-pass (`/mnt/gposingway-seed`, 256 GB NTFS, the
  operator's fstab-declared point-of-restore). The scan listed it
  **adoptable**; the ceremony's write refused as `stone` (fail-closed
  against a root-owned mount — correct; noted below), so the manifest was
  staged by sudo with a minted GUIDv7 and the DAEMON did the recognizing:
  the watcher registered `seed-vault::default` **mounted** within one
  watcher tick (news -> bank_rev -> song); crystalline-dune heard it
  garden-wide (`/garden/storage` showed the bank with live telemetry);
  the eject verb sang authoritative absence and crystalline-dune showed
  `ejected` within one interval.
- **Finding — appliance default**: with no `MOSS_RUNTIME`, the stone
  adopts docker but defaults to companion-grade `null` (L17/L23 working as
  designed). Fleet stones start under `MOSS_RUNTIME=docker`; a future
  appliance-modality declaration (D9) makes this first-class.
- **Finding — adopt vs point-of-restore**: a root-owned, uid=0 fstab mount
  cannot be adopted by a non-root daemon. To let moss adopt natively, the
  operator may remount uid-mapped (`uid=1000,gid=1000` in the fstab line);
  left untouched deliberately — weakening a point-of-restore's posture is
  the operator's call, not the deployment's.
- **State left behind**: `redis::witness` runs on crystalline-dune as
  living evidence (`rake offerings redis::witness uproot` to clear);
  `seed-vault::default` is ejected in tranquil-pass's boot ledger (remount
  the drive's slot or reboot to re-mount); binaries + identities live at
  `~/zen-v1/` on both live stones; 192.168.1.82 awaits the same swap.

---

## W7 — the night the drive died, replayed honestly (2026-08-28)

The living will, witnessed live on the fleet. The demo the charter calls
the niche-defining moment: kill the stone, watch the garden regrow the
service, connection string unchanged.

- **The stage**: `witness-db::garden` (redis:7-alpine) planted on
  stone-crystalline-dune (192.168.1.111) from a catalog manifest that
  DECLARES a will: `lock-and-copy`, quiesce `redis-cli SAVE`, resume
  `redis-cli PING`, `max_locked_s: 60`. Ledgered home **:7301** (address
  drawn by the arbiter, ADR-0002). Data written into the volume:
  `will.txt` and the redis key `will = "survives"`.
- **The will is read and executed**: POST capture -> quiesce (SAVE) ->
  imprint (raw copy of the volume) -> resume (finally-style) -> pack
  (tar.zst + SHA-256 manifest) -> **ferried to the seed bank**
  (`seed-gentle-valley::default` on the SanDisk, roles: sink) ->
  committed atomically. `phase: done`.
- **Findings during the run** (the fleet teaches):
  - redis's SAVE recreates `dump.rdb` as the container-internal user,
    mode 0600 - the imprint correctly REFUSED to copy what it cannot
    read (torn copies are lies). The manifest's quiesce now opens the
    files (`chmod -R a+rX /data` inside the lock). Correct behavior,
    witnessed as designed. Debt recorded: imprint via `docker cp` for
    internal-user-owned files (D16).
  - bank roles live in memory per boot; roles re-declared after a moss
    restart (roles persistence + declaration-on-adopt land later).
  - emerald vale (192.168.1.82) had returned to the network carrying its
    PoC-era name; the SanDisk moved onto it post-replant, and v1
    RECOGNIZED the PoC-era manifest (version 4) across generations -
    `seed-gentle-valley::default`, original GUIDv7 device id, mounted
    (R0.5 + L5 in the field).
- **THE MURDER**: stone-crystalline-dune powered off, no goodbye.
  Verified: ping 100% loss, HTTP silent, soft presence holding (online
  until the threshold - honest: the garden believes until silence proves
  otherwise).
- **THE REPLANT** on stone-tranquil-pass (192.168.1.195), one command:
  select -> verify (archive + per-file SHA-256) -> restore the directory
  -> place FROM THE STORED SPEC. The source checkpoint rode the
  gposingway-seed bank on the survivor - the dead stone's own copy died
  with it, which is exactly why sinks must be elsewhere.
- **The proofs**:
  - same FQN: `witness-db::garden` - status **running**;
  - same ledgered home: port_map `{"client": 7301}` - the connection
    string unchanged (allocations claimed ledger-first, ADR-0002);
  - same identity: offering_id `01a045b7-0372-7522-a21d-cb5e85c940b2`
    - the incarnation, not a copy;
  - same data: redis answers `GET will` -> **"survives"**;
  - the audit chain opens with `Replanted{predecessor_offering_id,
    final_hash: 1fe9e406...}` - lineage in the tamper-evident ledger,
    not tribal memory.
- **Finding — rich replies carry services only**: emerald vale's newcomer
  cache saw tranquil-pass's card but not its banks (the rich reply's
  inventory block carries services; the inventory MAP belongs in the
  reply per A2.1). Fix queued: reply carries the full InventoryMap;
  on_response merges by rev into existing peers.
- **State left behind**: witness-db::garden lives on stone-tranquil-pass;
  crystalline-dune awaits its power button (rejoins as a peer); the
  SanDisk rests on emerald vale with its PoC lineage intact; all three
  live stones now run the living-will build.
