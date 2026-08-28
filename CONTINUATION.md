# CONTINUATION — read me first, then delete me

Written 2026-08-28 at end of a marathon session. This replaces all prior
continuation content. Everything below reflects the current tree.

---

## What happened today (one paragraph for orientation)

The living-will epic (S3–S8 + W7) was completed, witnessed on the fleet, and
the companion ecosystem (Suzu) was spun out. Then the bring-assessment was
written, the visibility slice (list/URIs/portrait/pulse) landed, the agentic
baseline (errors-as-JSON/exit-codes/--field) landed, and the jobs registry was
built. 134 tests, clippy `-D warnings` clean, everything pushed.

---

## Git state

Branch `dev`, pushed to `origin/dev` (`git@github.com:sylin-org/zen-garden.git`, SSH).
Tree is clean. Latest: `1e46b3a7`.

Branch law: only `dev` (trunk) and — from RC — `main`.
PoC lives as tag `poc-final`. Pavilion parked at tag `pavilion-parked`.

The fleet: stone-crystalline-dune (.111) and stone-tranquil-pass (.195) run the
latest v1 build. stone-emerald-vale (.82) runs v1 (joined at W7). stone-limpid-dune
and stone-quartz-fen are chirping but HTTP-unreachable. The PoC fleet is ALSO
still alive on 7184/7185 — 5 stones thriving (mongodb x3, qdrant, searxng).
One stone has 65-day uptime. Live-migration target confirmed.

## v1 capabilities — what exists NOW

| Surface | Routes/verbs |
|---|---|
| Discovery | chirp/song (lean + full), rich ask/reply, candidates pool, topology cache |
| HTTP faces | 28 faces: /health, /api/v1 (manifest), /stone{,/this,/{ref},/posture}, /garden/stones, /garden/storage, /catalog, /storage{,/adopt,/roles,/eject}, /pulse{,/stream}, /offerings[/{fqn}][/capture[/last]|[/replant]|[/rest|/wake]] , /jobs[/{id}], /portrait via / |
| rake verbs | observe, find (--format uri), list, offer, explain, rest, wake, uproot, capture (--last), replant, storage (list/adopt/roles/garden/eject) |
| Living will | capture grammar (3 modes), two-phase pipeline, checkpoints (SHA-256, atomic rename, rotation), sink banks, replant with Replanted audit |
| Jobs | JobTracker (in-memory), GET /api/v1/jobs + /{id}, capture runs tracked |
| Agentic | --json, --field dot.notation, exit codes, errors-as-JSON |
| Suzu | spun out to sylin-org/suzu (ADR-0006); companion-contract, SDK, USB layer, firefly, cricket, firmware |

## What to build next (priority order)

**Standing rule — every slice below opens with THE SLICE MANDATE**
(`docs/v1/CODE-RULES.md`, first section): prior art → the PoC's objective →
the house's own history → design (DDD monolith, complexity at the seams) →
verdicts on every PoC element. "Let's work on this" is a mandate to
research first.

### 0 · The rake surface law (ADR-0007 / R4.8) — implement the decision

Encoding/projection/extraction decided at ONE dispatch point (delete the ~15
per-verb `if cli.json` branches); `--output json|human` + `RAKE_OUTPUT` env;
`rake manifest` — the machine-readable command catalog generated from the
clap tree. The decision is law (ADR-0007, CODE-RULES R4.8); this slice is
its implementation.

### 1 · Bank file operations
CRUD on mounted bank filesystems. Makes a seed bank a real storage destination.
- GET `/api/v1/storage/{fqn}/files` — list files on the bank
- GET `/api/v1/storage/{fqn}/files/{path}` — read/download a file
- PUT `/api/v1/storage/{fqn}/files/{path}` — write a file
- DELETE `/api/v1/storage/{fqn}/files/{path}` — delete a file
Implementation: resolve bank mount_point, join with traversal-checked path,
read/write via std::fs. The Storage.banks() already tracks mount_point.

### 2 · Agentic baseline completion
- errors currently exit(1) with no code distinction — wire NOT_FOUND/CONFLICT/
  UNAVAILABLE from the typed error refactor
- `--format uri` on find exists; add it to observe and list

### 3 · Logs/watch streaming
The PoC's open wound (advertised, stubbed). v1: docker adapter logs → SSE →
`rake watch <offering> logs`.

### 4 · Companions (Suzu)
Suzu is a separate project. The bootstrap, contract ADR, and harvest list are
committed in the zen-garden repo (`docs/v1/design/suzu-bootstrap.md`,
`docs/v1/decisions/ADR-0006-suzu-contract.md`). The suzu agent harvests from
`src/poc/companion-sdk/`, `companion-usb/`, `cricket/`, `firefly/`, and
`scripts/`. Integration: moss spawns companions, streams events via SSE,
proxies commands. Port pool: 7286–7295.

### 5 · Orchestrators / O3 adoption
Ollama as first citizen. Detect → adopt → expose (L25). Compatibility
predicates (`ai.runtime`) already in the census grammar.

### 6 · M1 release pipeline
main branch + tag → build → sign → publish. Gate: a stranger installs from a
public artifact. Requires: repo public, RC quality, install script.

## Fleet state

| Stone | v1 build | Moss name | Notes |
|---|---|---|---|
| .111 topaz-butte | latest (living-will) | crystalline-dune | witness-db::garden + redis::ports running |
| .195 obsidian-summit | latest (living-will) | tranquil-pass | seed-vault::default (ejected), redis::witness |
| .82 emerald-vale | latest (living-will) | translucent-clearing | joined at W7; seed-gentle-valley::default recognized |
| limpid-dune | — | — | powered off / unreachable |
| quartz-fen | — | — | powered off / unreachable |

PoC fleet: ALSO alive on 7184/7185 — 5 stones, mongodb x3, qdrant, searxng.
Live-migration target confirmed.

## Key file locations

| What | Where |
|---|---|
| Contract (wire shapes, BankEntry, consts) | `src/v1/crates/contract/src/` |
| Kernel (announce, topology, probe, responder, dispatch, ingress) | `src/v1/crates/kernel/src/` |
| Moss (offerings, storage, capture, jobs, http, source, identity) | `src/v1/crates/moss/src/` |
| rake (CLI) | `src/v1/crates/rake/src/` |
| Glossary (vocabulary with metaphor glosses) | `src/v1/crates/glossary/src/` |
| Suzu (companion ecosystem) | `sylin-org/suzu` (separate repo) |

## Conventions & gotchas

- `--json` / `--field` / `--format uri` — the three-degree machine output
- `gen` is a RESERVED keyword in Rust edition 2024
- tokio interval fires immediately on first tick — consume once before loops
- SO_REUSEADDR on Windows; SO_REUSEPORT on Unix (D8)
- one moss per host while developing (file lock on Windows)
- rg + PowerShell quoting breaks through two hops — use Write tool for scripts
- Push channel is SSH (HTTPS PAT lacks `workflow` scope)
- Companions repo: `sylin-org/suzu` — Suzu is generalized, Zen Garden is one
  consumer of many

## Authority (read in order; conflicts resolve downward)

1. `docs/v1/lessons.md` — L1–L26 normative
2. `docs/v1/CHARTER.md` — accepted, amended; bets B1–B11
3. `docs/v1/CODE-RULES.md` — THE SLICE MANDATE (first section, governs how
   every slice begins); then P0–P5; R3.9 records-are-paths; R1.1 registers
4. `docs/v1/OFFERINGS.md` — offerings law (§5.1 layered catalogs, FQN namespace)
5. `docs/v1/decisions/ADR-0001..0008` — directory, ports, FQN namespace,
   discovery envelope, living will, Suzu contract
6. `src/v1/DEBT.md` (D1–D15; D14 closed), `src/v1/WITNESSES.md` (W1–W7)
7. `docs/v1/inventory/poc-rake-surfaces.yaml` + `poc-moss-surfaces.yaml` —
   deep PoC capability inventories (gate 2 of the mandate)
8. `docs/v1/design/poc-bring-assessment.md` — what to bring/reshape/cut
9. `docs/v1/design/dx-delight-research.md` — vocabulary tiers, tutorial gap
10. `docs/v1/design/suzu-bootstrap.md` — the companion ecosystem brief
11. `docs/MEMORY.md` — durable memory index; `local/NOTES.md` — machine facts
