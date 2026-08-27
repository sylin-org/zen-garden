# ADR-0003 — Offering identity: the FQN namespace, the reserved default, moniker surfaces

**Status:** Accepted · 2026-08-26
**Supersedes:** a single-session `{stem}:{instance}` spelling (never shipped
past one dev stone); complements [ADR-0001](ADR-0001-offering-directory.md)
(directory layout gains `{stem}/{instance}/` nesting) and
[ADR-0002](ADR-0002-port-allocation-and-residence.md) (allocations are
per-offering, so multi-instance addressing works unchanged)
**Referenced by:** OFFERINGS.md §1/§5.1, `glossary::fqn` (the grammar's only
home), directory/event/container path derivation

## Provenance

Post-W4 evaluation, mid-flight. The operator first requested named
installations ("memcached" and "memcache:prod" on one host). A single-colon
grammar was implemented and witnessed; on review the operator rejected it —
"bad resolutions coming out of the : and :: thing" — and prescribed the final
shape: `::` everywhere, mirrored by folder structure (`ollama/prod` IS
`ollama::prod`), with `"default"` RESERVED. The key additional ruling that
closed it: **"ollama" is a moniker for "ollama::default"** — users say,
plant, uproot, and list `ollama`; machines use the FQN always.

## Context

The PoC already used FQNs for adoption (`ollama::adopted`) but had no grammar:
names were ad-hoc strings whose separator collided conceptually with image
tags (`redis:7-alpine`). Offering directories slugged whole names flat
(`ollama_adopted`), which loses structure and risks underscore ambiguity.
When instance support was added, single-colon syntax made an image tag look
exactly like a valid name — two namespaces sharing a delimiter.

## Decision

One grammar, defined ONCE in `glossary::fqn`:

1. **Machine identity is the FQN** `{stem}::{instance}`. `:` exists ONLY as
   the doubled separator; segments match `[A-Za-z0-9_-]{1,64}`. More than
   two levels refuse.
2. **`default` is reserved.** Every stem implicitly owns `::{default}`;
   planting a bare stem plants its default instance. `redis::default` and
   `redis` are the same offering under two spellings (aliases collapse at
   canonicalization).
3. **Humans speak monikers.** CLI rendering strips `::default`
   (`memcached::default` displays as `memcached`); foreign instances render
   in full. JSON output stays verbatim machine truth.
4. **Directories mirror the namespace**: `{offerings-root}/{stem}/{instance}/`.
   Identity parsing becomes path traversal; no slug ambiguity between stems.
   Pre-namespace flat layouts migrate automatically to `{stem}/default/`,
   records rewritten to FQN spelling.
5. **Containers keep moniker-slugging** (`zen-offering-{slug(moniker)}`,
   PoC-compatible): with `:` banned inside names, slug() over valid names is
   injective — default instances keep their short form, foreign instances
   carry double underscores.

## Law encoded

> One namespace, one delimiter, one authority. If a human can type something
> ambiguous, the grammar refused it before the system saw it; if the system
> holds an address for it, the path on disk spells the same truth.

## Alternatives considered

### A. Single-colon instances (`{stem}:{instance}`) — implemented, then retired

Natural typing, but shares its delimiter with image tags; `rake offer
redis:7-alpine` would happily plant instance `7-alpine`. Ambiguity in the
identity space is unfixable downstream — refused here by construction.

### B. Flat slugs for everything (`{stem}_{instance}`)

No nesting; collision between literal underscores in stems and encoded
separators reappears forever; directory listing cannot reveal structure.
Nested dirs make identity structural instead of encoded.

### C. No instances (one installation per stem)

Loses genuine multi-instance needs (prod/shadow copies of one service per
stone); ADR-0002's ledger-first allocation already proved claims are
per-offering — instances are nearly free underneath.

## Consequences

### Positive

- Image-tag masquerade structurally impossible; validation lives in ONE leaf
  crate everything already depends on.
- Multi-instance hosts fall out naturally; each instance owns independent
  addresses (witnessed :7300/:7301 side-by-side).
- Directory tree = identity tree; rehydration contract absorbs namespacing
  without new artifacts.
- Adoption FQNs (`ollama::adopted`) stop being special-cased spellings.

### Negative / costs

- Wire shape changes for existing offerings (`mongodb` → `mongodb::default`);
  tolerated pre-deployment, plus automatic disk migration.
- Every name consumer must route through `glossary::fqn`; a second local
  parser would re-open the delimiter war.

### Neutral

- Catalog manifests keep bare stem keys; instances resolve via their stem.
- Audit events, plan hashes, chirps carry FQNs transparently.

## References

- Grammar implementation: `crates/glossary/src/fqn.rs`
- Layout + migration: `crates/moss/src/offerings/directory.rs`
  (flat → `{stem}/default/`, incl. same-pass visibility)
- Container naming: `crates/moss/src/offerings/docker.rs`
- Operator discussion 2026-08-26 (quoted rulings in Provenance)
