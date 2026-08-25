# 10 — garden-common Split: A Contracts Crate That Tells the Truth

> garden-common becomes what its name claims: wire contracts, constants, utils, and the typed client —
> nothing that only one consumer compiles. ~13.6k moss-only lines go home to moss; the live UDP transport
> moves out; duplicate structs resolve. Phase: Structure. Depends on: 02, 03. Feeds: 13, and every
> musl/Android build.

## Mission

garden-common (38k lines pre-scrub) is "moss's second half" mislabeled as a shared crate: ~13.6k verified
moss-only lines, a process-global UDP multicast transport, and heavyweight deps (reqwest, sysinfo,
netstat2) that every companion and orchestrator image compiles for a handful of type imports. Meanwhile
the project's own MANDATORY rule — shared models via garden_common, NO duplicate structs — is violated by
21 struct names duplicated directly between moss and rake. Execute the split: move the moss-only mass into
moss, relocate the transport, resolve the duplicates, and feature-gate what remains so lean consumers
build lean. Greenfield rules: no re-export shims left behind "for compatibility" — every import path is
updated to its true home.

## Ground truth (verified 2026-06-11 — re-verify each; prompt 03 already deleted the dead modules)

| Fact | Re-verify |
|---|---|
| Moss-only mass ≈13,593 lines: `compatibility/` (2,154), `console/` (2,081 — includes first-boot tty art), `resources/` (1,357), `detection/` (1,323), `templates.rs` (384), `traits/` (239), `persistence/` (327), `platform_runtime.rs` (208), `infra/{timer 733, archive 267, debounce 255, process 218, platform 127}`, and `manifests/` MINUS its `generate`+`validation` submodules | per module: `grep -rln "garden_common::<module>" src tools --include="*.rs" \| grep -v "src/common"` → expect only `src/moss/...` paths |
| NOT moss-only (stays in common): `manifests::generate` (505 ln) + `manifests::validation` (729 ln) — consumed by rake's `manifest` command; `infra/network.rs` (447) — lantern + ollama/common orchestrators; `infra/koi_client.rs` (53) — lantern | `grep -rln "manifests::generate\|manifests::validation" src/rake; grep -rln "get_local_ip" src/lantern src/orchestrators` |
| Live transport in a contracts crate: `infra/communications/p2p.rs` (1,333 ln, process-global UDP socket singleton) + `mdns.rs`, `announcement_types`, `discovery.rs` — consumed by moss and garden-discovery only | `grep -rln "communications::p2p\|send_announcement\|subscribe_to_events" src --include="*.rs" \| grep -v "src/common"` |
| 21 struct names duplicated directly moss↔rake (the literal rule violation), several field-identical wire contracts: StorageOverview, GardenBankInfo, HarvestManifest, ListBucketResult, S3Object, PlacementRecommendation, StoneInfo… | for each: `grep -rn "pub struct StorageOverview" src/moss src/rake` |
| ~12 same-concept duplicates common↔(moss\|rake); verbatim copies: TransformSpec, FieldMappings, TemplateInfo | `grep -rn "pub struct TransformSpec" src --include="*.rs"` |
| NOT duplicates (leave alone): moss's `Current`/`Stone` namespace (prescribed by code-standards.md §5), name-collisions-of-different-concepts (Network, Runtime, PortConfig, rake's CLI CommandManifest/CommandDef vs the companion command-manifest contract — consider renaming rake's to `CliManifest` to end the collision) | read code-standards.md §5 before judging any Stone/Current hit |
| No `[features]` section in common's Cargo.toml; every consumer compiles reqwest/rustls/sysinfo/netstat2/socket2 | `grep -n "\[features\]" src/common/Cargo.toml` |
| The musl/Android targets (active investment: phone stones) pay this build cost on every cross-compile | — |

## Research first (~60 min)

1. Read `src/common/src/lib.rs` fully — the export map is your work order.
2. Read `.agentic/CONTEXT.md` rule 2 (shared models MANDATORY) and `docs/code-standards.md` §14 (one type
   per concept, enrich-don't-duplicate) — they decide each duplicate's resolution direction.
3. For each of the 21 moss↔rake duplicates: diff the two definitions. Field-identical → hoist ONE into
   common (it is a wire contract both sides speak). Diverged → understand why before choosing; the API
   response shape wins (moss serializes it; rake should deserialize the same type).
4. Map each moving module's moss import sites: `grep -rn "garden_common::console" src/moss | wc -l` etc.
   — sizing the mechanical edit.

## Plan gate

Post: the move list (module → destination path under `src/moss/src/`), the duplicate-resolution table
(keep-which / hoist-where for all 21+12), the transport destination, and the feature matrix. **OPERATOR**:
confirm the transport destination — recommend `src/discovery` absorbs `p2p.rs`+`mdns.rs`+announcement
types (it is the discovery crate; moss depends on it already or can), over a new `garden-transport` crate
(one more part = against the grain of this whole effort).

## Target shape

common's Cargo.toml after:

```toml
[features]
default = []                     # pure contracts: serde + thiserror + chrono, nothing heavier
client  = ["dep:reqwest"]        # StoneApi + http client factory
system  = ["dep:sysinfo", "dep:netstat2"]   # metrics collection (moss, probe)
```

Consumer lines after (the point of it all):

```toml
# cricket/Cargo.toml — a companion needs types only:
garden-common = { path = "../common" }                          # no features → no reqwest, no sysinfo
# rake:
garden-common = { path = "../common", features = ["client"] }
# moss:
garden-common = { path = "../common", features = ["client", "system"] }
```

Destination convention inside moss — modules land in their concept's home, not a `from_common/` dump:
`console/` → `src/moss/src/infra/console/`; `compatibility/` → `src/moss/src/domain/compatibility/`;
`detection/` → `src/moss/src/infra/detection/`; etc. File-per-concept per code-standards §14.

## Implementation

Strict order — each step compiles green before the next:

1. **Feature flags first** (no moves yet): introduce the `[features]` matrix, gate the existing heavy
   modules, fix every consumer's feature list. This alone delivers the musl build win and is safe.
2. **Transport move**: relocate p2p/mdns/announcements into `src/discovery`; update moss + discovery
   imports; the singleton stays a singleton (COMM-0001 rule: ALL UDP through it — re-read
   `.agentic/rules/networking.md` first).
3. **Moss-only mass**: move module-by-module (one commit each), updating imports
   (`garden_common::console::` → `crate::infra::console::`). Run `cargo check --workspace` between every
   module. Tests move with their modules.
4. **Duplicate resolution**: hoist the field-identical wire contracts into common (one commit per
   concept); delete the loser definitions; rename rake's CLI registry types to `CliManifest`/`CliCommand`
   if confirmed. Re-grep the rule at the end: the .agentic CONTEXT rule should finally be true.
5. Update `.agentic/reference/utilities.md` rows for everything that moved (it was made truthful in
   prompt 08 — keep it that way).
6. Orchestrator crates consume garden-common by path — verify their builds after every common-touching
   step (`cd src/orchestrators/{ollama,mongodb,common} && cargo check`).

## Definition of done

- [ ] `cargo tree -p garden-cricket | grep -c "reqwest\|sysinfo"` → 0 (paste; repeat for firefly).
- [ ] `wc -l` report: common ≤ ~18k lines; every moved module gone from `src/common/src`.
- [ ] Duplicate audit: for each of the 21 names, `grep -rn "pub struct <Name>" src --include="*.rs"`
      shows exactly one definition (or documented different-concept survivors with distinct names).
- [ ] `grep -rn "communications::p2p" src/common` → empty; UDP singleton rule still holds
      (`grep -rn "UdpSocket::bind" src --include="*.rs"` → only the relocated p2p.rs).
- [ ] Workspace + all three orchestrator crates green: check AND test. Probe (prompt 09) run against a
      local moss: 0 failed.
- [ ] No re-export shims: `grep -n "pub use" src/common/src/lib.rs` contains no paths into moss.

## Out of scope

Deleting anything not on the verified lists (FINDINGS.md suspicions instead). The router/supervision work
(11). Storage gateway extraction (13). Renaming garden-common itself (cosmetic; decide later). The uri/
module (14 wires it; it stays in common as a contract — it IS one).
