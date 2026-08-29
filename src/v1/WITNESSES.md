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

## W8 — adoption observes, it never operates (2026-08-29, stone-tranquil-pass / 192.168.1.195, release build, MOSS_RUNTIME=docker)

The adopted mode (OFFERINGS.md §1, L25) made real: catalog detection rules
× live container facts → adopted offerings, riding the converge sweep
clock. The one-minute skeptic demo, run against the live stone:

- **Hand-run container → adopted, honestly**: `docker run --name ollama
  ollama/ollama:latest` at 03:13:44 — by 03:14:12 (one sweep) the garden
  answers `ollama::adopted` (stem `ollama`, category `ai`,
  `container_name: "ollama"`, `control_level: monitor`, status
  **running**). The garden claimed credit for nothing: the mint log line
  reads *"offering detected on the host - adopted (observe-only)"*.
- **Killed by hand → recorded, not operated**: `docker stop` at 03:14:54;
  next sweep the record says **stopped** and STAYS — no vanish, no
  restart ("adopted workload moved - recorded, not operated").
- **Returned by its owner → running again**, `RestartCount=0`,
  `StartedAt` = exactly the operator's `docker start`; zero
  garden-initiated operations in the moss log. Lifecycle stayed the
  host's throughout.
- **Mid-cycle joiner (the ghost law, OFFERINGS.md §2 "keep exactly")**:
  container removed + moss restarted → the record split to candidates at
  boot (*"adopted offerings await detection (ghost prevention)
  ghosts=1"*) and did NOT haunt the room — offerings list clean; the
  container hand-run again → confirmed back to **running** within one
  sweep.
- Free-by-construction surfacing held: the adopted record rode the
  registry snapshot into the offerings face and chirps with no new code;
  a bare `resolve("ollama")` wish matches it by the stem law.

En route, the detection tests exposed a LATENT DEADLOCK: `add_candidate`
called `persist()` while holding the registry write lock (parking_lot is
not reentrant). Unexercised since the registry was written; fixed in this
slice, covered by the ghost-law tests.

**State left behind**: none — the ollama container, image, and adopted
record were uprooted after the proof; .195 as found (witness-db::garden
running the living-will build, now with the adoption slice).

## W9 — the wish answers: content, not just services (2026-08-29, stone-tranquil-pass / 192.168.1.195, release build)

Capability wishes (W1, docs/v1/design/capability-wishes.md): the
connection promise reaches past the offering to its content, read-only,
room-wide.

- **Adopted content is addressable**: ollama hand-run with a published
  port → adopted within one sweep carrying the OBSERVED port
  (`location.port = 11434`); the port rides the chirp (`ports.default`),
  so `ollama://192.168.1.195:11434` is true on the wire, not a guess.
- **The miss teaches (F3 acceptance)**: `rake ensure
  'ollama[model:all-minilm]'` BEFORE the model existed → *"no stone
  holds model:all-minilm yet. It could grow on: ollama::adopted on
  stone-tranquil-pass — grow it there, then ask again."* A malformed
  wish teaches the grammar: *"selector 'model' needs a type — use
  type:item, e.g. model:llama3."*
- **The wish answers**: `ollama pull all-minilm` by hand → the stone's
  capability sweep re-observed within one tick (exec/http are reads;
  nothing operated on the workload) → `rake ensure` flipped to
  *"ollama::adopted holds model:all-minilm on stone-tranquil-pass —
  ollama://192.168.1.195:11434"*, exit 0; `--format uri` prints the
  connection string.
- **The shipped resolver agrees**: Node `resolve("ollama[model:all-minilm]")`
  against the live room returns the same answer JSON; a miss teaches.
- **`rake capabilities ollama::adopted`** reads what it holds, live
  (human: `model: all-minilm:latest`; machine: the full map).

En route, three real finds: the on-media record round-trip DROPPED
`sub_capabilities` (record.rs hardcoded the default — remembered caches
now persist honestly, serde-default keeps old records readable); a
registry tag needs the `:latest` normalization law (matched by exact or
tag-default spelling, both in rake and the Node resolver); adopted
offerings previously had no wire address at all (port 0 forever) — the
observed published port now rides, which is the connection promise
reaching adopted work.

**State left behind**: none — ollama container, image, and adopted
record uprooted; .195 as found.

## W10 — the wish grows it (2026-08-29, stone-tranquil-pass / 192.168.1.195, release build)

Capability mutations (W2): `ensure` grows managed content through the
manifest's add channel, journaled per L11, managed-only per the trust law.

- **The wish grows content**: managed ollama planted from the catalog
  (ledgered home 7300); `rake ensure 'ollama[model:all-minilm]'` → the
  stone's add command ran INSIDE the container as a journaled job →
  *"ollama now holds model:all-minilm — grown, not planted:
  ollama://192.168.1.195:7300"*, exit 0, port carried from the holder's
  observed record.
- **L11, crash and all**: a 270 MB model growth started via the add
  face; moss KILLED mid-pull. On reboot the journal spoke — *"jobs
  interrupted by the last restart interrupted=1"*, the job shows
  `interrupted` with *"interrupted by restart — ask again; what landed
  is re-observed"* — and the sweep then re-observed the TRUTH: the
  in-container pull had survived its client and landed. State on disk,
  facts from the world, nothing resumed blindly.
- **The trust law holds on the wire**: mode now rides ServiceState
  (`managed` | `adopted`); with only an adopted ollama in the room, the
  same wish answers *"It could grow on: ollama::adopted ... — the garden
  observes adopted work and never operates it (L25); grow it there
  yourself, then ask again."* The server refuses independently (the law
  lives in the domain, not the client).

En route: the catalog refusal chain earned its keep (`rake offer ollama
--image ...` refused — catalog manifests define their own image), and
the ollama manifest grew its managed section (the ollama-cpu compat
rules already promised a GPU sibling).

**State left behind**: none — container, image, records, journals, and
the root-owned model files (cleared via a throwaway container; noted as
D17: in-container uid 0 writes make host files non-root cannot delete).

## W11 — the release pipeline is real (2026-08-29, tag v0.1.0, GitHub Actions)

M1's pipeline (charter): tag → build matrix → checksums → release →
install script.

- **One tag, three platforms**: pushing `v0.1.0` built moss + rake
  release binaries on ubuntu (linux-x86_64), windows (x86_64), and
  macos (aarch64) and published the release with `checksums.txt` and
  the two install scripts — no local machine in the loop.
- **Self-install, the witness run**: `installer/v1/install.ps1` fetched
  the release bundle, verified its sha256 against checksums.txt,
  installed to `~\.zen-garden\bin`, and the installed `rake 0.1.0`
  then walked the LIVE room — tending an unreachable entry stone,
  re-discovering .195 and .82 by multicast, answering `rake observe`.
  The connection promise, exercised by a binary that arrived only
  through the pipeline.
- **The R4.4 matrix earned its keep on night one** — six real finds,
  all fixed and green (ubuntu ✓ windows ✓ macos ✓ resolver ✓):
  the PoC-era root `.cargo/config.toml` forced lld on macOS (Apple
  clang refuses it); release packaging path; storage/source/http test
  fixtures faked mount points on an `E:` drive that exists only on the
  dev workstation (and passed on stale manifests there); the
  capability growth test needed the http channel's connect timeout;
  `surface.json` needed an eol=LF pin (CRLF checkouts broke the
  byte-equality gate); and the rich-ask wire test now composes its
  promise instead of racing CI multicast routes — which surfaced
  bind_ear hardening that is production-real: SO_REUSEPORT on unix
  (D8's note, load-bearing) and an explicitly chosen multicast
  interface for the send.

**Gate status (honest)**: M1's bar is "a stranger installs from a
public artifact". The pipeline and artifacts are real and verified by
self-install; the STRANGER part needs the repo's public flip — the
operator's call, recorded here so the gate is not silently waived.

DEBT settled en route: D18 records signing deferred to M2 (sha256 +
TLS is M1's trust anchor); D17 records the root-owned capability
volume files W10 discovered.
