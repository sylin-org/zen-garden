# ADR-0008 — Catalog sourcing: the approved set is embedded, operators layer on top

**Status:** Accepted · 2026-08-28
**Supersedes:** the bare `MOSS_CATALOG_DIR` sourcing rule (the directory
remains, demoted from "the whole catalog" to "an operator layer")
**Referenced by:** CODE-RULES R2.10 (the standard), OFFERINGS.md §5.1
(layered catalogs), B9 (wrapped distribution), L10 (distribution wraps the
garden), L25 (adoption is the house style)

## Provenance

Raised by the operator (2026-08-28), immediately after the `ensure` slice
demonstrated the failure mode live: a freshly deployed moss on `.195` booted
with no catalog directory at all, and `ensure memcached` — the growth
promise — had nothing to grow from. The PoC had solved this at birth
(`infra/embedded.rs`, `rust_embed`): the approved manifest set compiled into
the binary, filesystem layers overriding on top. The question "should we do
the same?" was the walk's opening gate.

## Decision

The moss binary **embeds the approved catalog** (compile-time, `rust_embed`
over `src/v1/catalog/`) and sources its offering catalog from three layers,
later layers overriding earlier entries **by name** (identity is the stem):

0. **Embedded approved catalog** — the floor, always present. What the
   release tagged is what the binary knows how to place: manifests and the
   placement code ship as one tested pair (B9; a manifest field the code
   cannot honor never rides in its own binary).
1. **Operator catalog** — `MOSS_CATALOG_DIR` / `~/.zen-garden/catalog`:
   additions and by-name corrections.
2. **Manifests overlay** — `MOSS_CATALOG_OVERLAY_DIR` /
   `{data_dir}/manifests`: highest, per the twin that already existed.

The loader reports each layer honestly (offerings found, overrides applied)
— L3: a silent merge is a lie about which truth is serving.

## Law encoded

CODE-RULES **R2.10** — a moss boots knowing how to place the approved set;
filesystem layers adjust by name, never by absence.

## Alternatives considered

- **Keep directory-only sourcing** — rejected: first light is broken (the
  `.195` witness), fleet deployment carries a second artifact, and the
  catalog can silently drift from the placement code that must honor it.
- **Embed a generated index file** (build.rs emits one JSON) — rejected:
  two parsers (tree + index) is the PoC's manifest-vs-reality drift with a
  build step attached; `rust_embed` walks the same tree the tests walk.
- **No overrides, binary is the only truth** — rejected: operators legitimately
  correct image tags and add private offerings without forking the binary;
  the PoC's overlay pattern is the proven release valve.

## Consequences

- First light: a fresh stone boots with a working garden — `ensure` and
  `offer` function with zero configuration.
- Catalog updates ride binary releases until M1 grows an update channel —
  which is the definition of "approved".
- Binary size grows by the catalog's bytes (tens of KB; negligible).
- Operators cannot wholesale-delete embedded entries (by-name override
  only) — accepted: wholesale absence is a release decision, not a runtime
  one.
- Companion binaries (the PoC embedded those too) are out of scope here;
  the Suzu slice owns them.
