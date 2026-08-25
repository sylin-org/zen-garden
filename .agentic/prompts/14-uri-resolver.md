# 14 — Wire the Headline: `zen-garden:` URI Resolution End-to-End

> `MONGODB_URI=zen-garden:mongodb/mydb` stops being the README's largest fiction: one resolver, consumed
> by rake and a demonstrable client path, working against a live garden. Phase: Product. Depends on: 06
> (front door), 07 (CLI contract). Strategy opportunity #3 (LAN-native service identity — validated empty
> in the 2026 landscape; Tailscale Services proved demand at the wrong layer).

## Mission

The project's single best pitch — *couple to the service, not the machine* — has a parser
(`src/common/src/uri/`, 838 lines, URI-0003, with a cross-language conformance corpus) consumed by
nothing. Build the resolution layer: given `zen-garden:mongodb/mydb`, return live endpoint(s) via the
existing discovery cascade, with failover semantics when a stone dies. Ship it in three consumable forms,
cheapest first: (1) `garden-rake resolve` for scripts and humans, (2) a resolver in `garden-discovery`
for Rust consumers, (3) one end-to-end demo path for a real application (MongoDB via connection-string
rewriting). Do NOT build client drivers for every language — the strategy is to prove the semantics and
publish the resolution contract so ecosystems can follow.

## Ground truth (verified 2026-06-11 — re-verify each)

| Fact | Re-verify |
|---|---|
| Parser exists and is complete: `src/common/src/uri/` (canonical.rs, error.rs, kind.rs, mod.rs, parser.rs); sole consumer is `src/common/tests/uri_corpus.rs` (property-tested, shared with the C# Koan sibling) | `grep -rn "ZenGardenUri" src --include="*.rs" \| grep -v "src/common"` → empty |
| README headlines the URI (`zen-garden:mongodb/mydb`) — prompt 06 either kept it as roadmap or demoted it; your job makes it literal again | `grep -n "zen-garden:" README.md` |
| The discovery cascade rake uses (4 levels: `--at` > `ZG_STONE` > tending cache > UDP/mDNS) lives in `src/rake/src/connection/resolution.rs` (~71-195); garden-discovery (DISC-0001) is the client-side crate — post-prompt-10 it also owns the UDP transport | `ls src/discovery/src` |
| Moss already answers service-existence queries: `GET /api/v1/garden/services?q=` (garden-wide) and `GET /api/v1/stone/services` (local); lantern exposes `GET /api/v1/resolve` (verified shipped) | `grep -rn "resolve" src/lantern/src --include="*.rs" \| head` |
| URI grammar per URI-0003 / the spec (`docs/specs/` — find the URI spec; verify fields: service kind, instance, partition (`ZG_PARTITION`/`ZG_INSTANCE` env exist in common's EnvConfig)) | `ls docs/specs \| grep -i uri` |
| What a MongoDB client actually needs: a `mongodb://host:port/db` string — so the demo path is RESOLUTION + REWRITE, not a wire-protocol proxy | — |

## Research first (~60 min)

1. Read the parser + the URI spec + the corpus — the grammar is decided; you implement resolution, not
   syntax.
2. Read `src/rake/src/connection/resolution.rs` and garden-discovery's API: the resolver should be the
   same cascade generalized from "find a stone" to "find a service instance" — reuse, don't duplicate
   (check what `find` already does server-side via `/garden/services`).
3. Read how offerings expose ports (named ports guide: `docs/guides/named-ports.md`; the service records
   in topology) — resolution output needs host:port per service kind.
4. Decide where multi-result ordering/failover logic lives: recommend moss/garden answers "who serves
   X" (it already does), the client-side resolver adds caching + liveness re-check + re-resolve-on-failure.

## Plan gate — OPERATOR decisions

1. **Resolution semantics** (present as a table): what does `zen-garden:mongodb/mydb` resolve to when 3
   stones serve mongodb — primary-only (for replica-set kinds), all (SRV-style), or best-fit-first? Per
   service-kind defaults (mongodb → the choreographer's primary; stateless kinds → any) — recommend
   kind-aware with a `?any` override, but the operator owns the semantics.
2. Whether `garden-rake resolve` also emits **export formats** (`--format env|uri|json`) for shell-driven
   apps. Recommend yes (it is the cheapest adoption path: `MONGODB_URI=$(garden-rake resolve
   zen-garden:mongodb/mydb)`).
3. Scope of the demo app: a documented compose/example using the rake-export pattern (recommend) vs a
   tiny Rust example binary in `samples/`.

## Target shape

```
$ garden-rake resolve zen-garden:mongodb/mydb
mongodb://stone-quiet-pond.local:27017/mydb

$ garden-rake resolve zen-garden:mongodb/mydb -o json
{"uri":"zen-garden:mongodb/mydb","kind":"mongodb","endpoints":[
  {"stone":"quiet-pond","host":"stone-quiet-pond.local","port":27017,"role":"primary"}],
 "resolved":"mongodb://stone-quiet-pond.local:27017/mydb","ttl_hint_s":30}

$ MONGODB_URI=$(garden-rake resolve zen-garden:mongodb/mydb) my-app   # the README demo, literal
```

Rust resolver API (in garden-discovery; common keeps only the parsed type):

```rust
let resolved = garden_discovery::resolve(&ZenGardenUri::parse("zen-garden:mongodb/mydb")?).await?;
// resolved.primary() -> Endpoint { host, port, stone, role }
// resolved.connection_string(Kind::MongoDb) -> "mongodb://..."
// re-resolution on connection failure is the CALLER's loop; resolve() is cheap and cache-aware
```

Failover demo (the acceptance test and the README's proof): resolve → connect → kill the serving
container → re-resolve returns the survivor (with the mongodb choreographer, the new primary) → reconnect
succeeds. Document observed timings honestly (philosophy: physicality over theater).

## Implementation

1. Resolution contract first: write `docs/specs/uri-resolution.md` (semantics table from the plan gate,
   resolution algorithm, TTL/caching rules, failover guidance for client authors) — the spec IS the
   ecosystem deliverable.
2. `garden_discovery::resolve()` — cascade reuse + kind-aware selection + tending-style cache.
3. `garden-rake resolve` command (manifest entry + examples that parse — prompt 07's test enforces it),
   `-o json` via the unified output pipeline, export formats per OPERATOR.
4. Wire `find` and `resolve` to share the server-side query path (no duplicate client logic).
5. The demo: `samples/uri-resolution/` with a compose file + script running the failover scenario against
   a real garden; capture the transcript into the sample's README.
6. Probe suite `resolution` (single-stone resolvable; multi-stone failover tagged `requires: multi-stone`).
7. Restore the README headline to literal truth (the Getting Started block from prompt 06 gains the
   resolve line) + CHANGELOG entry.
8. Commits: `docs(spec): uri resolution contract`, `feat(discovery): zen-garden uri resolver`,
   `feat(rake): resolve command`, `test(probe): resolution suite`, `docs: headline is now true`.

## Definition of done

- [ ] `garden-rake resolve zen-garden:mongodb/mydb` returns a live, correct connection string against a
      real or local garden; paste transcript.
- [ ] The failover scenario executed and transcribed (kill → re-resolve → survivor); timings recorded.
- [ ] An actual mongo client connects using the resolved string (mongosh or driver one-liner; transcript).
- [ ] Manifest examples parse (prompt 07's test green); `-o json` valid; exit codes per the CLI contract.
- [ ] Conformance corpus still green (`cargo test -p garden-common uri`); resolver unit tests added.
- [ ] `docs/specs/uri-resolution.md` exists; README headline literal; probe suite green (single-stone).
- [ ] `cargo test --workspace` green.

## Out of scope

Language-specific client libraries (the spec enables them; FINDINGS.md a koan-framework/C# note — the
corpus is already shared). A DNS/SRV bridge (tempting; separate proposal). Proxying wire protocols.
WAN/overlay resolution (LAN-only by strategy — Back to My Mac died there).
