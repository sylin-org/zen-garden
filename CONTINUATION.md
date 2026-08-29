# CONTINUATION — read me first, then delete me

Written 2026-08-28, second continuation of the marathon (replaces the earlier
one). Everything below reflects the current tree. The immediate task — the
**adoption slice** — has a full mandate-walk proposal at §NEXT that was just
delivered to the user and is **awaiting the nod**. Do not re-derive it; if the
user says "proceed/accepted", implement §NEXT-Design as written.

---

## Orientation (one paragraph)

Zen Garden v1 is the lean rewrite of the PoC: Rust workspace under `src/v1/`
(garden-contract, garden-glossary, garden-kernel, garden-moss, garden-rake),
DDD monolith, complexity at the seams. This session landed: the convergence
law fix, bank roles persistence, the complete storage data plane
(?depth, Range, mv, DELETE, HEAD, cross-stone redirect routing — witnessed),
THE SLICE MANDATE as governing law, contract-first faces (37 faces from a
data table) + staleness-gated surface.json, the rehearse slice (witnessed
green on .195), the nourish slice (check/apply with auto-revert, witnessed),
logs/watch SSE streaming, and a Node resolver. 37-face build is deployed on
.195. The adoption slice proposal (detect → adopt) is on the table.

---

## Git state

Branch `dev`, pushed to `origin/dev` (`git@github.com:sylin-org/zen-garden.git`, SSH).
Latest at handoff: `7d542eea` (docs: continuation rewritten) + the commit that
lands this file. Tree law: only `dev` (trunk) and — from RC — `main`.
PoC preserved at tag `poc-final`. Pavilion parked at tag `pavilion-parked`.

## What landed this session (since the last continuation)

- **Convergence law fix**: room seated peers bankless because rich discovery
  answers were destroyed at promotion. Fixed: `kernel/src/topology.rs`
  `InventoryMap::merge_frame` richer-block-fills-thinner (incl. equal-rev fix)
  + `fill_rumor` at promotion. Old promotion-law test preserved.
- **Bank roles persistence**: `.zen-garden` manifest roles survive restart —
  `BankManifest.roles` write-through in `set_roles` (canonicalizes bare stems)
  + reconcile read-back.
- **Storage data plane complete** (`moss/src/offerings/storage.rs`,
  `moss/src/http.rs`): FilesError{UnknownBank,NotMounted,BadPath,NotThatKind,
  Missing,Exists,Io}, safe_join traversal guard, list_tree with `?depth`,
  read_file with RFC 7233 Range (206/416), file_size/HEAD, write_file
  (octet-stream, content_type threaded through redirect-follow), PATCH move,
  DELETE. **Cross-stone routing witnessed**: GET on entry stone redirects to
  home stone; sha256-verified put/get through the redirect; MOSes reverted.
- **THE SLICE MANDATE** is now the first section of `docs/v1/CODE-RULES.md`:
  5 gates (prior art → PoC objective → house history → lean DDD design →
  verdicts on every PoC element), proposal before code, witness in deployment
  reality. Ritual: "do the homework, take notes, elaborate on the best way,
  propose." Every slice opens with it.
- **Contract-first faces** (ADR-0009): `contract/src/faces.rs` — Face enum
  (37 variants) + `FACES: &[FaceDef]` + `def()` (position-based, total); moss
  front door and router render from it; bijection tests. `contract/src/surface.rs`
  + `surface.json` — schemars-derived wire-type schemas, staleness-gated
  (regenerate: `ZG_REGEN_SURFACE=1 cargo test -p garden-contract`).
- **ADRs**: 0007 (rake surface degrees / R4.8), 0008 (embedded catalog layers /
  R2.10), 0009 (contract-first codegen). All indexed in `docs/v1/decisions/README`.
- **Rehearse slice**: `moss/src/offerings/rehearse.rs` — `RehearsalDeps` (owned,
  Send-bounded boxes), `rehearse()` → RehearsalReport{green,checkpoint,files,
  bytes,hash,container_ran_secs,container_state,...}; docker `rehearse_run`
  (`zen-rehearsal-` prefix, ro config mounts, no ports, force-remove around);
  face `/api/v1/offerings/{fqn}/rehearse`, `rake rehearse`. **Witnessed green
  on .195** (checkpoint from seed bank → scratch boot → verdict).
- **Nourish slice**: `moss` `update_check`/`update_offering` (docker
  `refresh_image` pull; place fail → auto-revert to pre-pull image ID);
  `rake nourish` conductor — per-stone per-offering check/apply, canary filter,
  halt on red. **Witnessed honest "current" on .195.**
- **Logs/watch**: docker `logs_stream` (bollard) → SSE → `rake watch`.
- **Node resolver** (`src/v1/resolvers/node/`): discover (multicast with
  addMembership + setMulticastInterface pinned to the LAN iface — this was the
  silence bug), resolve garden-walk, serviceMatches (bare-stem vs named
  instance law), connectionUri; 4 vitest tests passing. ask envelope must be
  `{msg_id,type,data}` — untyped asks count as undecodable.
- **Agentic baseline**: exit codes from status, errors-as-JSON,
  `--format uri` everywhere, Answer/Projection/emit dispatcher (R4.8),
  `rake manifest`.

## Fleet state (leave as found; uproot test containers)

| Stone | Build | Notes |
|---|---|---|
| .195 tranquil-pass | **current 37-face build**, MOSS_RUNTIME=docker | SSH works: `ssh -o BatchMode=yes stone@192.168.1.195`. **root's gposingway production service must NOT be touched.** Has the USB seed bank. |
| .82 translucent-clearing | old build | **no SSH key from workstation — user must run console commands** |
| .111 crystalline-dune | down | user's console |
| workstation | stone-entry-glass 192.168.1.137 | runs entry moss (rebuilt before witnessing!) |

Fresh linux cross-compile (rust:latest docker ritual) finished 2026-08-28
13:11 → moss + rake binaries (that's the deployed 37-face build). Rebuild
after adoption lands.

**Deploy ritual**: `tar -cf - -C src/v1 --exclude=target . | docker run -i
rust:latest …cargo build --release` → scp to stone →
`pkill -u stone -f '^\./moss'` → swap binary →
`setsid nohup ./moss >> moss.log 2>&1 < /dev/null &`

---

## NEXT — the adoption slice (proposal delivered, AWAITING NOD)

User directive: *"harvest poc for the intent again. As usual — make it a
separate detection domain that adds to the service offering on detection.
Let's do some prior-art research, delight investigation for potential
audiences, the works."*

### Gate 1 — prior art
- **Azure Arc**: adopt existing servers into a management plane without
  touching the workload — management and workload are separate claims.
- **Home Assistant discovery**: constant scanning (mDNS/USB/UPnP) → visible
  "Discovered!" cards → one-click adopt; detected state is honest.
- **Portainer**: lists ALL containers incl. non-Portainer ones; "manage
  existing" is first-class.
- **Prometheus service discovery**: pluggable detector domains (file_sd,
  consul_sd…) feed one config; mechanism never leaks to consumers.
- **The trust rule** (failure mode everywhere): adoption that secretly means
  control. **Adoption observes, it never operates.**

### Gate 2 — PoC homework
`src/poc/moss/src/api/v1/adoption.rs`: `list_adoptable_v1`,
`ContainerDetector` + `DetectionOrchestrator`; three detection methods
(command, http_probe, container_inspect — container-name regex, image regex,
running-state verification); detection *stability* required before adopt;
`rake adopt` minted `name::adopted`. Gaps: docker-only, adoptable list not
surfaced in room view, detection rules buried in compatibility configs,
nothing inherited by v1.

### Gate 3 — house law
`Status::Adopted` + `name::adopted` FQN shaping already in the model;
NOURISH declared; L25 (claim only what answers); L11 (guest contract);
R2.4 (detection = adapter intricacy, domain boring); R2.10/ADR-0008 (rules
ride embedded catalog + operator overlays); R1.1 (one home per name).
User's directive makes detection **its own domain that adds to the service
offering on detection** — not a flag on place().

### Design (implement this when nodded)
1. **Catalog grammar**: optional `adopt:` section in offering manifests —
   `{ container_name_pattern, image_pattern }` (+ stem implicit). Parsed at
   catalog load; rides ADR-0008 layers.
2. **Runtime port**: `list_running(&self) -> Vec<ContainerFact{name, image,
   state}>` — ALL running containers, NOT just the `zen-offering-` prefix.
   Docker impl via bollard `list_containers`; default trait impl → empty.
3. **`moss/src/offerings/detect.rs` — the detection domain**: rules ×
   containers → matches; on match **mint an adopted record** (mode adopted,
   FQN `stem::adopted`, external container name remembered, status from
   observed state). Skip stems already placed/managed (R1.1). Runs on the
   converge sweep clock.
4. **Converge adopted law**: every sweep, observe adopted offerings, update
   status — **never heal, never restart, never remove**. Dies → reports
   stopped; returns → running. Lifecycle stays the host's.
5. **Surface**: free by construction — adopted records ride the registry
   snapshot into chirps/observe/find/list/URIs. `resolve("ollama")` answers
   whether planted or pre-existing.
6. **Tests** + witness: hand-run an ollama container on .195 → appears as
   `ollama::adopted` in `rake observe`; kill → stopped; restart → running;
   prove NO garden-initiated restart ever fired. Uproot test containers after.

### Gate 5 — verdicts
| Element | Verdict |
|---|---|
| Detection rules in catalog manifests | brought reshaped |
| Container inspect detection (name/image/state) | brought reshaped (docker port) |
| Adopt ceremony → `name::adopted` | brought, made automatic (safe because adopted is observe-only) |
| Converge observe-only law for adopted | new (L11: adopted without watch is fiction) |
| Detection stability caching | deferred (gate: first flapping detector) |
| Firmware/LVFS, moss self-update | deferred (LVFS availability / release channel) |
| Borrow/return across stones | deferred (M2+) |

### Delight / audiences
- **Agent (J1/J4)**: `resolve("ollama")` answers without caring how Ollama
  got there; "is there an LLM here?" is answered by the room.
- **Gardener**: `rake observe` shows `ollama::adopted` with true status —
  host services join the map without the garden claiming credit.
- **Household** (later portal): "found" markers, HA discovery-card dialect.
- **Skeptic demo**: hand-start container → appears adopted; kill → stopped;
  restart → running; no garden restart fired. The trust story in one minute.

---

## Backlog after adoption (each opens with the Slice Mandate)

- Capability wishes / orchestrator (PoC DetectionOrchestrator's deeper intent)
- Tools-SSE readiness stream (jobs-stream slice)
- Snapshot browse + access audit (capture surfacing)
- mkdir (empty-dir consumer), truncation caps, events `--until`
- uri-ip projection, firmware/moss-self updates (post-1.0 / release channel)
- **M1 release pipeline** (tag → build matrix → checksums/sign → GitHub
  release → install script; gate: a stranger installs). User explicitly
  deferred: "Nop, no release yet." Repo public = user's call.
- Fleet hygiene: .82 upgrade (needs SSH key via user's console), .111 down.

## Key file locations

| What | Where |
|---|---|
| Faces + surface.json | `src/v1/crates/contract/src/faces.rs`, `surface.rs`, `crates/contract/surface.json` |
| Wire/chirp/discovery/song | `src/v1/crates/contract/src/` |
| Kernel (ingress/dispatch/topology/responder/announce/probe) | `src/v1/crates/kernel/src/` |
| Moss offerings (storage, capture_run, rehearse, converge, docker, http) | `src/v1/crates/moss/src/offerings/`, `src/v1/crates/moss/src/http.rs` |
| rake (main.rs verbs, moss_http transport, tending) | `src/v1/crates/rake/src/` |
| Node resolver | `src/v1/resolvers/node/` |
| Detection domain (TO BUILD) | `src/v1/crates/moss/src/offerings/detect.rs` |
| PoC adoption reference | `src/poc/moss/src/api/v1/adoption.rs`, `src/poc/moss/src/infra/detection/container_inspect.rs` |
| PoC surface inventories (mandate gate 2) | `docs/v1/inventory/poc-rake-surfaces.yaml`, `poc-moss-surfaces.yaml` |

## Conventions & gotchas (hard-won this session)

- **Never run cargo from the repo root** — it hits the PoC workspace.
  `cd src/v1` or `--manifest-path`.
- **Bash heredocs mangle backslash-regex** — write .py/.js scripts with the
  Write tool; Edit on exact file text beats str.replace anchors.
- `cargo test` does NOT refresh bin artifacts — rebuild before live witnesses.
- Stale binaries produce phantom 404s (entry-moss included).
- `slice[..n]` needs `s.get(..n)`; CommandError variants need ApiError
  wrapping in handlers; `RehearsalDeps` taken by value (not Sync); boxed
  closures need `+ Send`; tuple arity is (status, resp).
- Enum close brace at column 0 when excising a local Face enum.
- `--json` / `--field` / `--format uri` — the three-degree machine output.
- `gen` reserved in Rust 2024; tokio interval first tick fires immediately;
  SO_REUSEADDR Windows / SO_REUSEPORT Unix (D8); one moss per host on Windows.
- Multicast group 239.255.42.199:7284; Node needs addMembership+interface pin.
- Push via SSH (HTTPS PAT lacks `workflow` scope).
- Fleet hygiene law: leave fleet as found; uproot test containers.

## Authority (read in order; conflicts resolve downward)

1. `docs/v1/lessons.md` — L1–L26 normative (L25 adoption, L11 guest contract)
2. `docs/v1/CHARTER.md` — bets B1–B11
3. `docs/v1/CODE-RULES.md` — **THE SLICE MANDATE first** (governs how every
   slice begins); R4.8 surface degrees; R2.10 catalog layers; R1.1 registers
4. `docs/v1/OFFERINGS.md` — offerings law (§5.1 layered catalogs, FQN)
5. `docs/v1/decisions/ADR-0001..0009` — directory, ports, FQN namespace,
   discovery envelope, living will, Suzu contract, surface degrees, catalog
   sourcing, contract-first codegen
6. `src/v1/DEBT.md` (D1–D15; D14 closed), `src/v1/WITNESSES.md` (W1–W7)
7. PoC inventories (gate 2), `docs/v1/design/poc-bring-assessment.md`,
   `dx-delight-research.md`, `suzu-bootstrap.md`
8. `docs/MEMORY.md`, `local/NOTES.md` — machine facts, SSH, deploy ritual
