---
audience: [contributor, maintainer, ai]
doc_type: adr
status: superseded
last_verified: 2026-05-04
canonical: true
---

# URI-0001: `zen-garden://` URI Scheme — Cascade Intent Resolution

**Status**: Superseded by [URI-0003](URI-0003-zen-garden-urn-form-scheme.md)
**Date**: 2026-05-04
**Deciders**: Architecture
**Tags**: uri, resolution, intent, contract, cross-language

---

## Context

The `zen-garden://` URI scheme exists as documented prior art and as a working
C# implementation in the Koan framework
([`Koan.ZenGarden.Core/ZenGardenConnectionIntent.cs`](https://github.com/sylin-org/koan-framework)),
but several gaps and inconsistencies have accumulated:

1. **Documentation–implementation drift.** The Rust-side docs at
   [docs/reference/connection-strings.md](../reference/connection-strings.md)
   and [docs/guides/offering-lifecycle.md](../guides/offering-lifecycle.md)
   specify single-colon form (`zen-garden:mongodb`). The C# parser requires
   double-slash form (`zen-garden://mongodb`). The implementation is
   authoritative; the docs are wrong.

2. **Narrow grammar.** The C# parser handles exactly one shape:
   `zen-garden://<offering>[:<instance>][?cap=...]`. Anything after the
   first slash in the path is silently discarded. This is adequate for
   offering connection intents (the original use case) but inadequate for:
   - Stone, bank, service, companion, pond, and garden references
   - File-path navigation under a bank
   - Deep links from external systems (chat, email, Pavilion's
     "share this view" feature)
   - URI fragments for in-view anchors

3. **No Rust parser exists.** The scheme has been one-sided in
   implementation. Pavilion ([PAVILION-0001](PAVILION-0001-windows-client-separation.md))
   needs to register as the Windows OS handler for `zen-garden:` and parse
   incoming URIs in Rust.

4. **No cross-language test parity.** The C# and (future) Rust parsers must
   agree on every input. Today there is no shared specification or shared
   test corpus.

5. **No design philosophy stated.** The scheme is implicitly an intent
   resolver — `zen-garden://mongodb` says "I want a MongoDB" without
   committing to *where* it comes from. This philosophy is documented in
   [docs/proposals/patent-analysis.md](../proposals/patent-analysis.md) as
   "intent-based infrastructure resolution" but not surfaced in the URI
   scheme spec itself.

The scheme needs a single canonical specification that subsumes the existing
C# behaviour, opens the navigation surface Pavilion needs, and binds Rust
and C# implementations to identical semantics.

---

## Decision

`zen-garden://` is a **cascade intent resolution scheme**: the user
expresses *what they want*, the resolver decides *what it is*.

We will:

1. Specify a single canonical grammar (below) covering both connection
   intents and resource navigation.
2. Resolve bare names by **cascading** through resource kinds in a fixed
   priority order; first match wins.
3. Allow callers to **bypass cascade** by prefixing with a reserved
   keyword that names the resource kind explicitly.
4. **Reserve resource-kind keywords at resource-allocation time** — no
   stone/bank/service/etc. may be named `offering`, `stone`, `bank`,
   `service`, `companion`, `pond`, or `garden` (or future-reserved
   variants). Enforcement is at create time in Moss, not at parse time.
5. Implement the parser in Rust (`garden-common`) and update the C# parser
   in Koan framework to match. Both bind to a shared test-vector corpus.

The existing C# behaviour is **the offering-first cascade match for bare
names**. No URI that worked under the old parser stops working under the
new one — backward compatibility is a property of the cascade, not a
compatibility shim.

---

## Grammar

```
zen-garden://<target>[/<sub-path>][?<query>][#<fragment>]

<target>      := <bare-name>            # cascade resolution
               | <kind>/<name>           # explicit kind

<bare-name>   := <name>[:<instance>]    # cascade form; legacy shape preserved
<kind>        := "offering" | "stone" | "bank" | "service"
               | "companion" | "pond" | "garden"

<name>        := lowercase identifier; no reserved keywords
<instance>    := lowercase identifier
<sub-path>    := kind-specific (see "Sub-path semantics" below)
<query>       := standard URI query (see "Query parameters")
<fragment>    := standard URI fragment (kind-specific anchor)
```

Parsers MUST delegate URI splitting to a real URI library
(Rust `url` crate; C# `System.Uri`) and apply zen-garden semantics to the
resulting structured components. String surgery on the raw URI is forbidden.

### Cascade order

When `<target>` is a bare name, resolvers attempt resource kinds in this
order; the first match wins:

1. **offering** — connection intents (dominant case, preserves legacy
   semantics)
2. **stone** — named compute nodes
3. **bank** — named storage replicas
4. **service** — named running services
5. **companion** — Cricket / Firefly / etc.
6. **pond** — security tiers
7. **garden** — whole gardens (least specific)

A resolver that cannot discover any of these (e.g., offline) MAY return
a structured "unresolved" result rather than failing parse. URIs always
parse; resolution may fail.

### Reserved keywords

The following names are **reserved** and MAY NOT be used as resource names
for any kind:

- `offering`, `stone`, `bank`, `service`, `companion`, `pond`, `garden`
- Historical aliases: `seed-bank`, `tool`
- Prophylactic reservations: `gateway`, `orchestrator`, `keystone`,
  `cornerstone`, `lantern`, `moss`, `pavilion`, `rake`

Reservation enforcement: Moss MUST reject resource creation requests that
collide with this list, returning a clear error. The reserved set lives in
`garden-common::constants::reserved_names` with an `is_reserved(&str) -> bool`
helper. Adding new kinds in future ADRs MUST extend this list.

### Sub-path semantics

After cascade resolution lands on a resource kind, the sub-path is
**kind-specific**:

| Kind | Sub-path shape | Examples |
|---|---|---|
| offering | (none) | `zen-garden://mongodb` |
| stone | `/<resource-kind>/<name>[/...]` | `zen-garden://crystal-forest/storage/personal` |
| bank | `/<file-path>` | `zen-garden://bank/personal/Documents/file.txt` |
| service | (none typically; use `?action=`) | `zen-garden://service/postgres-prod` |
| companion | (none typically; use `?action=` and `?cmd=`) | `zen-garden://companion/firefly` |
| pond | `/<ceremony>` | `zen-garden://pond/home-garden/enroll` |
| garden | `/<topology-section>` | `zen-garden://garden/home-garden/topology` |

Each kind's sub-path schema MAY be specified in a follow-up ADR or spec.
Parsers SHOULD expose the sub-path as a typed structured value when the
kind's schema is known, and as a raw string when it is not.

### Query parameters

Query parameters are **generic** and MAY apply to any cascade match,
subject to a kind-specific interpretation:

| Parameter | Meaning | Notes |
|---|---|---|
| `cap=X[,Y]` or `cap=X&cap=Y` | Capability constraints | Generic; primary use is offering, but valid on any kind. Future ADRs may restrict per kind. |
| `action=<verb>` | Action to perform | Kind-specific verbs (e.g., `logs`, `restart`, `enroll`) |
| `protocol=<proto>` | Preferred protocol | Resolver hint, not a hard constraint |
| `v=<n>` | Scheme version | Default 1; future grammar revisions bump this. Parsers reject unknown versions. |

Capability comparison MUST be **order-independent**: `cap=tools,vision` and
`cap=vision,tools` describe the same intent. Equality on parsed
intent values MUST treat capabilities as a set.

### Fragments

Fragments (`#anchor`) are **kind-specific anchors** within a view:

- `bank`: line numbers in text files (`#L42`), ranges (`#L42-L51`)
- `service`: log timestamps, log line ids
- `garden`: dashboard tab ids
- Other kinds: reserved for future use

Fragments MUST round-trip through the parser unchanged.

---

## Round-trip and equality

Every parsed `ZenGardenUri` MUST round-trip back to a canonical string form
via a `to_string()` / `ToString()` method. Two URIs that parse to
equivalent structured values MUST have equal canonical strings.

Canonicalisation rules:
- Lowercase the scheme, host, and reserved keywords
- Sort query parameters by key
- Sort capability values within `cap=`
- Strip trailing slashes on empty sub-paths
- Percent-encoding follows RFC 3986

---

## Backward compatibility

The current C# parser handles `zen-garden://<offering>[:<instance>][?cap=...]`
as connection intent. Under cascade semantics, this shape:

1. Parses to bare-name target `<offering>[:<instance>]`
2. Cascades to **offering** (first kind in the order)
3. Resolves identically to the legacy result

**No existing URI changes meaning.** No migration is required. The C#
implementation is widened to handle additional shapes; its existing
behaviour is preserved as the offering-cascade case.

---

## Implementation Requirements

### Rust parser — `garden-common`

- Module: `garden_common::uri` with a `ZenGardenUri` typed struct
- Cascade dispatch implemented via a `ResourceKind` enum and a
  `resolve_cascade(name) -> Option<(ResourceKind, Resolved)>` resolver
  trait that callers implement (Pavilion, Moss, Lantern each have their
  own resolver context)
- Use the `url` crate for raw URI parsing; zen-garden semantics layered on
  top of `Url`'s structured fields
- Public surface includes `parse(&str) -> Result<ZenGardenUri, UriError>`,
  `to_string(&self) -> String`, and `kind_explicit(&self) -> bool`

### C# parser — `Koan.ZenGarden.Core`

- Existing `ZenGardenConnectionIntent` is preserved as the offering-case
  result type; a new `ZenGardenUri` record is the parsed-URI representation
- New `ZenGardenUri.TryParse` handles all shapes; `ZenGardenConnectionIntent.TryParse`
  remains as a thin wrapper that succeeds only when the cascade resolves to
  offering
- Use `System.Uri` for raw parsing
- Reserved-keyword set lives in `Koan.ZenGarden.Core.ReservedNames`

### Shared test vectors

A JSON file at `docs/specs/zen-garden-uri-test-vectors.json` enumerates
every parse case both implementations must agree on. Each entry:

```json
{
  "uri": "zen-garden://mongodb:prod?cap=tools,vision",
  "parses": true,
  "kind_explicit": false,
  "target_name": "mongodb",
  "target_instance": "prod",
  "sub_path": null,
  "capabilities": ["tools", "vision"],
  "canonical": "zen-garden://mongodb:prod?cap=tools,vision"
}
```

Both parser implementations MUST run this corpus as part of their test
suites. New URI shapes added to the spec MUST add entries here in the
same change.

### Reserved-keyword enforcement

Moss enforces reserved-keyword rejection at resource creation:

- Stone naming (boot-time auto-name + manual rename)
- Bank creation
- Service planting
- Companion registration
- Pond initialisation
- Garden naming

The constant list lives in `garden-common::constants::reserved_names` and
is imported by both the URI parser (for explicit-form keyword matching)
and the resource validators (for rejection at create time). Single source
of truth.

---

## Rationale

- **Intent-first, addressing-second.** Users say "I want a MongoDB"; the
  scheme should not require them to know whether MongoDB is an offering, a
  borrowed service, or an adopted external system. Cascade resolution
  matches user intent.
- **No grammar inflation.** A naive design forces the user to classify the
  resource kind in the URI (`zen-garden://offering/mongodb`). The cascade
  model lets the simple case stay simple while preserving the explicit
  escape for ambiguity.
- **Backward compatibility falls out naturally.** The existing C# behaviour
  is the offering-cascade case. v1 URIs work unchanged under v2 semantics.
- **Cross-language parity is enforceable.** Shared test vectors make
  drift detectable in CI.
- **Reserved-keyword enforcement at allocation time** keeps the cascade
  unambiguous without requiring runtime ambiguity-resolution logic. The
  user can never accidentally create a stone named "offering" — the system
  refuses at allocation.

---

## Consequences

### Positive

- Single canonical spec replacing implicit, drifted prior art
- Pavilion can ship deep links from M2 without scheme-design rework
- C# implementation is widened, not replaced — existing callers unaffected
- Cross-language parser parity is testable
- New resource kinds extend the cascade and the reserved set with no
  grammar change
- Doc/implementation drift on the colon question is permanently resolved

### Negative

- A small set of names becomes globally reserved across all resource kinds
- Resource validators across Moss must add the reserved-keyword check
  (small mechanical change)
- The shared test corpus is a new artifact to maintain in two languages

### Neutral

- Capability semantics are presently generic across all cascade kinds. A
  future ADR may restrict or refine this if practice shows it should be
  kind-specific.
- Sub-path schemas per kind are deferred to follow-up specs. Parsers
  surface sub-paths as raw strings until kind-specific schemas land.

---

## Alternatives Considered

### Alternative 1: Required explicit kind in path

- **Description**: `zen-garden:///<kind>/<name>` always; no cascade.
- **Pros**: No ambiguity; no reserved keywords needed.
- **Cons**: Forces every caller to classify the resource kind; breaks
  every existing URI; user-hostile for the dominant case (offering
  resolution).
- **Rejected because**: Would require migration of every existing
  `zen-garden://<offering>` use site and pushes classification onto users
  who don't care.

### Alternative 2: Two sibling schemes

- **Description**: Keep `zen-garden://` for connection intents, add
  `pavilion://` for navigation.
- **Pros**: Clean separation of concerns.
- **Cons**: Doubles the OS handler footprint; loses brand cohesion;
  external systems must learn two schemes; cross-references between
  navigation and resolution become awkward.
- **Rejected because**: The two use cases are facets of "address something
  in the garden, then act on it" and benefit from a single addressing
  vocabulary.

### Alternative 3: Versioned grammar with hard cutover

- **Description**: Bump to `zen-garden://v2/...` for new shapes; keep
  existing as v1.
- **Pros**: Explicit version dispatch.
- **Cons**: Pollutes every URI with a version segment; existing URIs
  continue to need legacy-compatible parser; v2 surface area must include
  the v1 case anyway.
- **Rejected because**: The cascade model achieves the same goal —
  preserving v1 behaviour while extending — without polluting URIs.

---

## References

- [Koan framework `ZenGardenConnectionIntent`](https://github.com/sylin-org/koan-framework) — prior art parser preserved as offering-cascade case
- [docs/proposals/patent-analysis.md](../proposals/patent-analysis.md) — Intent-based infrastructure resolution philosophy
- [docs/reference/connection-strings.md](../reference/connection-strings.md) — Doc fix required (single-colon → double-slash)
- [docs/guides/offering-lifecycle.md](../guides/offering-lifecycle.md) — Doc fix required
- [LANTERN-0003](LANTERN-0003-mdns-service-discovery.md) — Speculation about future `zen-garden://` browser handler (now in scope via Pavilion)
- [PAVILION-0001](PAVILION-0001-windows-client-separation.md) — Pavilion ships the first OS handler for the scheme
- [OFFER-0003](OFFER-0003-offering-fqn.md) — `OfferingFqn` parser referenced by `ToolFqid` C# port
