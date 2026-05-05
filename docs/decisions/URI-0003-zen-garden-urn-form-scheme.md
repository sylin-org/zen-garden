---
audience: [contributor, maintainer, ai]
doc_type: adr
status: accepted
last_verified: 2026-05-04
canonical: true
---

# URI-0003: `zen-garden:` URI Scheme — URN-Form Cascade Intent Resolution

**Status**: Accepted
**Date**: 2026-05-04
**Deciders**: Architecture
**Tags**: uri, urn, intent, scheme, dx, supersession
**Supersedes**: [URI-0001](URI-0001-zen-garden-uri-scheme.md), [URI-0002](URI-0002-protocol-prefix-deprecation-and-extensions.md)

---

## Context

URI-0001 specified the canonical scheme as `zen-garden://` with a
URL-like authority slot, on the strength of preserving compatibility
with the existing Koan framework C# parser. URI-0002 extended that
scheme with capability queries, replica pinning, a `wish` action, and a
category cascade layer.

Both decisions were correct in their cascade-resolution model — naming
versus capability versus action versus replica are the right axes — but
both got the surface syntax wrong on a more fundamental level.

`://` after a scheme is an **authority delimiter** in RFC 3986 hierarchy
URIs. It introduces a host (with optional port and userinfo) that
*locates* a resource:

- `https://example.com/path` — fetch from this host
- `mongodb://stone-01:27017/db` — connect to this host
- `ssh://user@host` — SSH to this host

Schemes that do not address a host — schemes that express *intent*,
*identity*, or *opaque content* — conventionally do not use `://`:

- `urn:isbn:0451450523` — book identity
- `tel:+15551234` — phone number to call
- `mailto:foo@bar` — address to message
- `geo:37.78,-122.40` — coordinate
- `data:text/plain,Hello` — inline content

Zen Garden URIs are **intent**, not location. `zen-garden:mongodb` says
"give me MongoDB"; the resolver decides who provides it. There is no
authority. There is no host. The URN-form syntax (`scheme:` without
`://`) reflects this honestly.

Two further pressures point the same direction:

1. **Consistency with existing prior art.** Proposals across the
   codebase already use the `zen-garden:<protocol>//<rest>` shape
   (notably `zen-garden:s3//`, `zen-garden:storage//`,
   `zen-garden:wish//`). A URN-form generalisation —
   `zen-garden:<kind>//<name>` — preserves that shape rather than
   deprecating it.

2. **Greenfield window.** The implementation surface is small: one
   working C# parser, no Rust parser yet, no Pavilion code, no test
   corpus, ~27 docs that just got their first pass. The cost of
   correcting the syntax now is bounded; the cost of correcting it
   after Pavilion ships, after the Rust parser exists, after the
   shared test vectors solidify, after every client library has been
   written, is not.

URI-0001's argument for `://` rested heavily on preserving the C#
parser's existing behaviour. That argument was tactically sound but
strategically wrong: developer experience for the entire scheme's
lifetime trades a one-time C# rewrite. Greenfield correctness wins.

URI-0001 and URI-0002 are therefore superseded.

---

## Decision

The canonical scheme is **`zen-garden:`** (URN-form). The URL-form
`zen-garden://` is **accepted as a tolerant alias** — parsers treat
the leading `//` as optional whitespace and process the remainder under
URN grammar — but it is not the canonical form. Round-trip and equality
operations always produce URN-form output.

Grammar (canonical):

```
zen-garden:[<target>][/<sub-path>][?<query>][#<fragment>]

<target>      := <bare-name>            # cascade resolution
               | <kind>//<name>          # explicit kind
               | (empty, if <query> contains cap=)

<bare-name>   := <name>[:<instance>]
<kind>        := "offering" | "stone" | "bank" | "service"
               | "companion" | "pond" | "garden"

<name>        := lowercase identifier; not in reserved-keywords set
<instance>    := lowercase identifier
<sub-path>    := kind-specific (see "Sub-path semantics" below)
<query>       := standard URI query
<fragment>    := standard URI fragment
```

Parsers MUST delegate URI splitting to a real URI library
(Rust `url` crate; C# `System.Uri`) and apply zen-garden semantics to
the resulting structured components. String surgery on the raw URI is
forbidden.

### Examples

| URI | Resolution |
|---|---|
| `zen-garden:mongodb` | Cascade: offering "mongodb" |
| `zen-garden:mongodb:prod` | Offering "mongodb" with instance "prod" |
| `zen-garden:mongodb/mydb` | Offering + sub-path (database) |
| `zen-garden:mongodb?cap=tools` | Capability constraint on cascade |
| `zen-garden:crystal-forest` | Cascade: not an offering, hits stone |
| `zen-garden:crystal-forest/storage/personal` | Stone + sub-path nav |
| `zen-garden:offering//crystal-forest` | Explicit: force offering kind |
| `zen-garden:stone//mongodb` | Explicit: stone named "mongodb" |
| `zen-garden:pond//home-garden/enroll` | Pond + ceremony |
| `zen-garden:bank//personal/Documents/file.txt#L42` | Bank + path + anchor |
| `zen-garden:service//postgres-prod?action=logs` | Service + action |
| `zen-garden:?cap=s3` | Capability-only (empty target) |
| `zen-garden:?cap=s3&at=seed-usb-01` | Capability + replica pin |
| `zen-garden:mongodb?action=wish` | Find-or-provision |
| `zen-garden:database` | Cascade falls through to category index |

### Cascade order

When `<target>` is a bare name, resolvers attempt resource kinds in
this order; first match wins:

1. **offering** — connection intents (dominant case)
2. **stone** — named compute nodes
3. **bank** — named storage replicas
4. **service** — named running services
5. **companion** — Cricket / Firefly / etc.
6. **pond** — security tiers
7. **garden** — whole gardens
8. **category** — taxonomic groupings (final fallback)

A resolver that cannot discover any of these MAY return a structured
"unresolved" result rather than failing parse. URIs always parse;
resolution may fail.

### Reserved keywords

These names are reserved and MAY NOT be used as resource names for any
kind:

- `offering`, `stone`, `bank`, `service`, `companion`, `pond`, `garden`
- Historical aliases: `seed-bank`, `tool`
- Prophylactic reservations: `gateway`, `orchestrator`, `keystone`,
  `cornerstone`, `lantern`, `moss`, `pavilion`, `rake`

Reservation enforcement: Moss MUST reject resource creation requests
that collide with this list, returning a clear error. The reserved
set lives in `garden-common::constants::reserved_names` with an
`is_reserved(&str) -> bool` helper.

### Sub-path semantics

After cascade resolution lands on a resource kind, the sub-path is
kind-specific:

| Kind | Sub-path shape | Examples |
|---|---|---|
| offering | optional partition/database | `zen-garden:mongodb/mydb` |
| stone | `/<resource-kind>/<name>[/...]` | `zen-garden:crystal-forest/storage/personal` |
| bank | `/<file-path>` | `zen-garden:bank//personal/Documents/file.txt` |
| service | typically empty; use `?action=` | `zen-garden:service//postgres-prod?action=logs` |
| companion | typically empty; use `?action=` and `?cmd=` | `zen-garden:companion//firefly?action=command&cmd=ping` |
| pond | `/<ceremony>` | `zen-garden:pond//home-garden/enroll` |
| garden | `/<topology-section>` | `zen-garden:garden//home-garden/topology` |
| category | (none — categories return offering set) | `zen-garden:database` |

Each kind's sub-path schema MAY be refined in follow-up specs.

### Query parameters

Query parameters are generic and MAY apply to any cascade match,
subject to kind-specific interpretation:

| Parameter | Meaning | Notes |
|---|---|---|
| `cap=X[,Y]` or `cap=X&cap=Y` | Capability constraints (also enables empty-target queries) | Order-independent for equality |
| `action=<verb>` | Action to perform | `wish` (find-or-provision), `logs`, `restart`, `enroll`, etc. |
| `at=<name>` | Pin to specific stone or bank | Hard constraint; resolution fails if target unreachable |
| `protocol=<proto>` | Preferred wire protocol | Resolver hint, not a hard constraint |
| `tags=<X>[,<Y>]` | Tag filter on cascade results | Order-independent for equality |
| `v=<n>` | Scheme version | Default 1; future grammar revisions bump this |

Capability and tag comparison MUST be order-independent:
`cap=tools,vision` and `cap=vision,tools` describe the same intent.
Equality on parsed intents MUST treat these as sets.

### Fragments

Fragments (`#anchor`) are kind-specific anchors within a view:

- `bank`: line numbers in text files (`#L42`), ranges (`#L42-L51`)
- `service`: log timestamps, log line ids
- `garden`: dashboard tab ids
- Other kinds: reserved for future use

Fragments MUST round-trip through the parser unchanged.

### Capability-only URIs

A URI with no target and no `cap=` query is a parse error. The empty
target is permitted *only* when the URI carries a `cap=` query
parameter. Empty-target URIs bypass cascade entirely; resolution
proceeds through capability matching against the offering catalogue
and seed-bank gateways:

- `zen-garden:?cap=s3` — any S3-speaking endpoint
- `zen-garden:?cap=mongodb` — any MongoDB-protocol endpoint
- `zen-garden:?cap=s3&at=seed-usb-01` — S3-speaking endpoint pinned to a bank

### Wish action

`?action=wish` triggers find-or-provision semantics: resolver attempts
cascade; on miss, requests provisioning through Moss and returns the
provisioned endpoint. Provisioning is subject to local authorisation.
Resolvers without provisioning capability return a typed error rather
than treating wish as a no-op.

Wish combines with capability and replica-pin queries:
`zen-garden:?cap=s3&action=wish` — provision an S3-speaking endpoint
if none exists.

### Categories

Category names (`database`, `document-database`, `vector`,
`relational-database`, `key-value-store`, etc.) are *not* reserved
keywords. They live in a category index
(`garden-common::constants::categories`) consulted as the final
cascade stage.

A category lookup returns the set of offerings whose taxonomy contains
the requested term, then re-runs resolver selection (priority,
health) over that set.

### Round-trip and equality

Every parsed `ZenGardenUri` MUST round-trip back to a canonical string
form via `to_string()` / `ToString()`. Two URIs that parse to
equivalent structured values MUST have equal canonical strings.

Canonicalisation rules:
- Lowercase the scheme, kind keywords, and target name
- Sort query parameters alphabetically by key
- Sort multi-valued parameters (`cap`, `tags`) within their value list
- Strip trailing slashes on empty sub-paths
- Percent-encoding follows RFC 3986
- **URL-form input** (`zen-garden://...`) parses but always
  canonicalises to URN-form output (`zen-garden:...`). Two URIs that
  differ only by leading `//` are equal under canonical form.

### URL-form tolerance — additional rules

Within the URL-form alias, the rules of the URN grammar apply uniformly
after the optional leading `//` is stripped:

- `zen-garden://mongodb` ≡ `zen-garden:mongodb` ✓ (cascade)
- `zen-garden://mongodb/db-a` ≡ `zen-garden:mongodb/db-a` ✓ (cascade + sub-path)
- `zen-garden://mongodb//db-a` ✗ (after stripping leading `//`, this is
  `zen-garden:mongodb//db-a` which parses as kind=mongodb — invalid kind)
- `zen-garden:offering//mongodb/db-a` ✓ (explicit kind + sub-path)
- `zen-garden:offering//mongodb//db-a` ✗ (extra `//` in path after the
  name — not a valid sub-path separator)

The general rule: `//` is the kind/name separator in explicit form and
appears at most once per URI. Anywhere else, sub-paths use single `/`.

---

## Implementation Requirements

### Rust parser — `garden-common`

- Module: `garden_common::uri` with a `ZenGardenUri` typed struct
- Use the `url` crate's `Url::parse` to get scheme + scheme-specific
  part, then apply zen-garden grammar to the latter
- Public surface: `parse(&str) -> Result<ZenGardenUri, UriError>`,
  `to_string(&self) -> String`, `kind_explicit(&self) -> bool`,
  `is_capability_query(&self) -> bool`
- Cascade dispatch via `ResourceKind` enum and a
  `resolve_cascade(name) -> Option<(ResourceKind, Resolved)>` resolver
  trait

### C# parser — `Koan.ZenGarden.Core`

- `ZenGardenConnectionIntent.TryParse` is **rewritten**, not extended.
  The previous `://`-requiring parser is replaced.
- Existing callers of `ZenGardenConnectionIntent` need only a parse
  call site update; the typed result remains the same shape (offering,
  instance, capabilities)
- A new `ZenGardenUri` record represents the broader parse result;
  `ZenGardenConnectionIntent` is one of its resolutions (the
  offering-cascade case)
- Use `System.Uri` with `UriKind.Absolute` for the initial parse;
  zen-garden semantics layered on the structured result

### Shared test vectors

`docs/specs/zen-garden-uri-test-vectors.json` is created as the
canonical corpus. Each entry:

```json
{
  "uri": "zen-garden:mongodb:prod?cap=tools,vision",
  "parses": true,
  "kind_explicit": false,
  "target_name": "mongodb",
  "target_instance": "prod",
  "sub_path": null,
  "capabilities": ["tools", "vision"],
  "action": null,
  "at": null,
  "fragment": null,
  "canonical": "zen-garden:mongodb:prod?cap=tools,vision"
}
```

Both Rust and C# parsers MUST run this corpus as part of their test
suites.

### Reserved-keyword and category enforcement

Moss enforces reserved-keyword rejection at resource creation:

- Stone naming
- Bank creation
- Service planting
- Companion registration
- Pond initialisation
- Garden naming

The reserved-name and category constants live in:

- `garden-common::constants::reserved_names`
- `garden-common::constants::categories`

Both have `is_*` helpers and are imported by the URI parser (for
explicit-form keyword matching) and by resource validators (for
rejection at create time). Single source of truth.

---

## Migration

### What changes

| Surface | Before (URI-0001/0002) | After (URI-0003) |
|---|---|---|
| Cascade form | `zen-garden://mongodb` | `zen-garden:mongodb` |
| Explicit kind | `zen-garden://offering/mongodb` | `zen-garden:offering//mongodb` |
| Sub-path | `zen-garden://crystal-forest/storage/personal` | `zen-garden:crystal-forest/storage/personal` |
| Capability-only | `zen-garden://?cap=s3` | `zen-garden:?cap=s3` |
| Wish | `zen-garden://mongodb?action=wish` | `zen-garden:mongodb?action=wish` (unchanged below scheme) |
| Replica pin | `zen-garden://?cap=s3&at=bank` | `zen-garden:?cap=s3&at=bank` |
| Category | `zen-garden://database` | `zen-garden:database` |

The cascade order, reserved keywords, query parameter set, equality
rules, and round-trip semantics are unchanged from URI-0002. Only the
scheme-to-target delimiter changes (`://` → `:`), and the
kind-to-name delimiter for explicit form (`/` → `//`).

### Doc migration

The doc fixes performed under URI-0001/URI-0002 (~27 files updated to
`zen-garden://`) need a second pass to revert to `zen-garden:`. This
is mechanical, file-by-file, using the slow careful approach
established earlier in the project.

The "specs with obsolete protocol-prefix grammar" deferred under
URI-0002 (specs/discovery.md, specs/offerings.md,
reference/driver-specification.md, proposals/*) now have a *cleaner*
migration target: their existing `zen-garden:s3//` form generalises
naturally to `zen-garden:s3//<rest>` (kind=s3, with s3 as a
capability-equivalent shorthand) or, more correctly, to
`zen-garden:?cap=s3` for capability-only queries.

### Code migration

- Koan framework `ZenGardenConnectionIntent.TryParse` rewritten
- Garden-common Rust parser written fresh (was not yet implemented)
- Shared test vectors authored
- Pavilion (PAVILION-0001) imports the URI-0003 parser from M0; no
  refactor needed since Pavilion code has not yet been written

### What does NOT need changing

- PAVILION-0001 architecture remains valid (Tauri client, Cloud Filter
  extraction, Lantern UI hosting, tray, etc.) — only the URI form
  inside it changes
- DISC-0001 architecture remains valid — the discovery crate consumes
  parsed URIs regardless of their surface form
- The cascade resolution model, reserved keywords, and four URI-0002
  extensions (capability queries, categories, wish, at) carry forward
  unchanged

---

## Rationale

- **URN-form matches the scheme's actual semantics.** This is intent,
  not location. The `://` form was syntactically misleading.
- **Existing prior art aligns.** Proposals across the codebase already
  use the `zen-garden:<x>//<rest>` shape; URI-0003 generalises rather
  than deprecates it.
- **Greenfield window justifies the cost.** No installed base, no
  parser code (Rust), one parser to rewrite (C#), one round of doc
  fixes to redo. The window for cheap revision closes when Pavilion
  code lands and the test corpus solidifies.
- **DX is paramount and lasts forever.** A scheme is a forever
  artifact; client libraries, deep links, documentation, and user
  muscle memory all anchor to it. Getting it right in greenfield
  trades short-term rework for long-term clarity.
- **The cascade model survives unchanged.** What was substantively
  decided in URI-0001 (cascade order, reserved keywords, intent-first
  resolution) and URI-0002 (capability queries, categories, wish, at)
  carries forward without revision. Only the surface syntax changes.

---

## Consequences

### Positive

- Scheme honestly reflects intent semantics
- Cleaner visual: `zen-garden:mongodb` reads as "scheme: mongodb-intent"
- Mirrors URN conventions (`urn:`, `tel:`, `mailto:`, `geo:`, `data:`)
- Existing protocol-prefix shape generalises naturally
- Shorter to type and reference (4 fewer chars per URI)
- Greenfield syntax stability: no further revisions expected

### Negative

- URI-0001 and URI-0002 marked superseded after one day of acceptance —
  paperwork churn
- Koan framework C# parser rewritten (not just extended)
- Doc files updated under URI-0001 (~27 files) need a second pass
- Until URI-0003 lands across all surfaces, mixed-form URIs may appear
  in transitional commits

### Neutral

- Empty-target URIs (`zen-garden:?cap=s3`) look unfamiliar but parse
  cleanly
- The `//` retains a role in the syntax (kind-name delimiter for
  explicit form), just at a different position than before

---

## Alternatives Considered

### Alternative 1: Keep `zen-garden://` (URI-0001/URI-0002 stand)

- **Description**: Preserve URI-0001 and URI-0002. Accept the URL-like
  surface as a familiarity tradeoff.
- **Pros**: No supersession churn; Koan framework C# parser unchanged;
  no second doc pass.
- **Cons**: Scheme syntactically misleading (URN intent expressed in
  URL form); doesn't match URN conventions; ignores the existing
  protocol-prefix shape across proposals.
- **Rejected because**: Greenfield window is open; DX matters more
  than tactical compatibility with one C# parser. The cost of
  correction grows over time; correcting now is cheap.

### Alternative 2: Both forms valid (URN and URL)

- **Description**: Accept both `zen-garden:mongodb` and
  `zen-garden://mongodb` as canonical, parsing to identical intents.
- **Pros**: No migration; parsers permissive.
- **Cons**: Two canonical forms = ambiguity. Equality, round-trip,
  and canonicalisation rules require choosing one form anyway.
- **Rejected because**: Two canonical forms is no canonical form.

### Alternative 3: New scheme name (`zg:`, `garden:`)

- **Description**: Switch scheme name entirely; abandon `zen-garden:`.
- **Pros**: Shorter; no association with the previous form.
- **Cons**: Loses brand cohesion; documentation across the project
  uses "zen-garden" extensively; OS handler registration would need to
  shift; no upside vs URN-form preservation of the existing name.
- **Rejected because**: The scheme name is correct; only the
  delimiter convention is wrong. Surgical fix beats reshuffle.

---

## References

- [URI-0001](URI-0001-zen-garden-uri-scheme.md) — Superseded; `zen-garden://` cascade scheme
- [URI-0002](URI-0002-protocol-prefix-deprecation-and-extensions.md) — Superseded; URI-0001 extensions
- [PAVILION-0001](PAVILION-0001-windows-client-separation.md) — Pavilion imports the URI-0003 parser from M0
- [DISC-0001](DISC-0001-discovery-as-first-class-crate.md) — `garden-discovery` resolves URI-0003 URIs
- [OFFER-0003](OFFER-0003-offering-fqn.md) — `OfferingFqn` parser; `ToolFqid` C# port
- [docs/proposals/patent-analysis.md](../proposals/patent-analysis.md) — Intent-based infrastructure resolution philosophy
- Koan framework `ZenGardenConnectionIntent` — to be rewritten under URI-0003
- RFC 3986 §3 — URI generic syntax (URN-form `scheme:` vs hierarchy `scheme://authority`)
