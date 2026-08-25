# Surface Ledger

> **Rotation contract.** Before the lane leaves this repo or surface: tag; CI green;
> a tripwire exists for every surface the departing work was exercising; status
> endpoints tell the truth; this ledger is updated. Leave a guard at the door when
> you leave the room.

This is the mechanical memory behind the maintainer's serial-lane model (Epic E02).
Each row records a pillar/plane-level **surface**, **who exercises it**, **when it was
last exercised**, and **what guard protects it**. When the focus lane rotates away,
unguarded surfaces rot silently — the ledger turns that from a discovered fiction into
a known risk.

**Columns.** `Exercised by` ∈ {a named in-repo test/sample suite, `koi`, `koan`,
`private downstream solution`, `none`}. `Guard` ∈ {test/CI job name, `none`}.
`Last exercised` is a real date (`YYYY-MM-DD`) or `unknown since <date>` — never a
guessed "works".

**Honesty notes for this repo.**
- **Dates** are seeded from `git log -1 --format=%as <paths>`.
- **`tests, unwired`** is the truthful guard for tested surfaces here: the repo has a
  large unit corpus (~2,483 tests) but **no CI** — there is no `.github/workflows/`
  directory, so nothing runs the suites on push/PR. They guard locally only. Wiring CI
  is workstream #1 of the architecture assessment, not this card.

| Surface | Exercised by | Last exercised | Guard | Notes |
|---|---|---|---|---|
| Offerings lifecycle | moss unit suites (part of ~2,483) | 2026-06-07 | tests, unwired | Tier 0 core value; battle-tested |
| mongodb orchestrator | orchestrator suites + private downstream solution | 2026-06-10 | tests, unwired | Tier 1; check()/reconcile() single-authority; delegates deploy to Moss |
| ollama orchestrator | orchestrator suites + private downstream solution | 2026-06-10 | tests, unwired | Tier 1; deployed/published/documented generation; the AI contract target |
| ai orchestrator | none | unknown since 2026-04-12 | none | Dormant; pending succession ADR; never registers with the gateway (Tier 2 park) |
| pond (security bootstrap) | moss suites | 2026-04-12 | tests, unwired | koi-certmesh delegation; crypto not hand-rolled |
| discovery (browse/topology/UDP) | discovery + moss suites | 2026-05-06 | tests, unwired | UDP-7184 garden mesh is ZG-internal (STACK-0001), never a cross-project contract |
| self-update / updates | moss suites | 2026-04-12 | tests, unwired | Update-transaction aggregate |
| storage durability (banks/replication/snapshots) | moss suites | 2026-06-07 | tests, unwired | Tier 0 durability core; replication has ~0 e2e tests |
| storage gateways (S3/WebDAV/sets/garden-FS) | none | unknown since 2026-06-07 | none | Tier 2 parked / feature-gate; 18 prod unwraps in s3_gateway; unauth writes on :7185 when pond inactive |
| moss daemon (router/supervisor/tasks) | moss suites (~2,483) | 2026-06-10 | tests, unwired | Router duplication; >=8 unsupervised spawns outside the registry |
| rake CLI | rake suites | 2026-05-10 | tests, unwired | Manifest-driven; StoneApi adopted across ~15 files |
| garden-contracts (common, reborn) | shared-contract round-trip tests | 2026-06-10 | tests, unwired | 21 moss/rake duplicate structs is the open debt |
| lantern dashboard | lantern suites | 2026-05-05 | tests, unwired | Registry pretense retired; resolve + SSE + heartbeats |
| companions SDK (cricket/firefly) | companion-sdk suites (2 in-tree binaries) | 2026-06-10 | tests, unwired | Frozen, feature-complete (COMPANION-0011) |
| pavilion (Windows Tauri client) | none | unknown since 2026-05-06 | none | Parked; flagship feature broken end-to-end (PAVILION-0002) |
| probe (integration harness) | none (not scheduled) | unknown since 2026-03-22 | none | The only real integration surface; stale since Mar 22, unwired |

---

*Seeded by Epic E02 (2026-06-13). Every lane that touches a surface above updates its
row — `Last exercised` to today, `Guard` to the tripwire it left — before it leaves.*
