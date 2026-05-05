---
audience: [contributor, maintainer, ai]
doc_type: adr
status: superseded
last_verified: 2026-05-04
canonical: true
---

# URI-0002: Protocol-Prefix Deprecation and Capability/Action Extensions

**Status**: Superseded by [URI-0003](URI-0003-zen-garden-urn-form-scheme.md)
**Date**: 2026-05-04
**Deciders**: Architecture
**Tags**: uri, deprecation, capabilities, categories, actions, replica-pinning
**Extends**: [URI-0001](URI-0001-zen-garden-uri-scheme.md)

---

## Context

URI-0001 specified the canonical `zen-garden://` URI scheme as a cascade
intent resolver. Several existing specs and proposals describe an *older*
grammar that URI-0001 does not directly cover:

```
zen-garden:[<protocol>//]<offering>[:<instance>][/<partition>][?options]
```

This older form appears in:

- [specs/discovery.md](../specs/discovery.md) — connection-string format and resolution steps
- [specs/offerings.md](../specs/offerings.md) — protocol/offering/category requests
- [reference/driver-specification.md](../reference/driver-specification.md) — parser pseudocode for client libraries
- [proposals/storage-api-design.md](../proposals/storage-api-design.md) — `zen-garden:s3//`, `zen-garden:storage//` storage protocols
- [proposals/garden-aws-bridge.md](../proposals/garden-aws-bridge.md) — `zen-garden:s3//`, `zen-garden:sqs//`, `zen-garden:dynamodb//` AWS-style protocol handlers
- [proposals/rake-find-command.md](../proposals/rake-find-command.md) — `zen-garden:wish//<offering>` find-or-provision semantic
- [proposals/ongoing/storage-capability-model.md](../proposals/ongoing/storage-capability-model.md), [discovery-service-resolution.md](../proposals/ongoing/discovery-service-resolution.md), [offering-alignment-checklist.md](../proposals/ongoing/offering-alignment-checklist.md) — capability and instance grammar

The older grammar tried to express five distinct intents in a single
syntactic form:

1. **Named lookup** — "give me MongoDB" (`zen-garden:mongodb`)
2. **Capability selection** — "give me anything speaking S3"
   (`zen-garden:s3//`)
3. **Replica pinning** — "use this specific physical location"
   (`zen-garden:s3//minio@seed-usb-01`)
4. **Wish / find-or-provision** — "find it, or create it if missing"
   (`zen-garden:wish//mongodb`)
5. **Categorical lookup** — "any document database" (`zen-garden:document-database`)

URI-0001's cascade handles intent (1) directly, and provides query
parameters (`?cap=`, `?action=`, `?protocol=`) and reserved keywords as
building blocks for the others — but the mapping from each older intent
to the URI-0001 grammar is not yet specified. Worse, the older grammar's
single-colon prefix (`zen-garden:`) does not parse under URI-0001 at all
(URI-0001 requires `://`), so URIs in those forms are silently broken.

We need a single decision that:

- Specifies the canonical URI-0001 expression for each older intent
- Closes the gaps in URI-0001 (capability-only queries, categories,
  wish action, replica pinning) with minimal extensions
- Marks the protocol-prefix grammar as deprecated, with a translation
  table for downstream specs
- Lets the older proposals be updated incrementally without grammar
  re-litigation

---

## Decision

The protocol-prefix grammar `zen-garden:[<protocol>//]<offering>...` is
**deprecated**. The five intents it expressed are reachable through
URI-0001's existing mechanisms plus four small extensions:

1. **Capability-only queries** — URI-0001's `<target>` may be empty when
   the URI carries a `cap=` query. Empty-target URIs are *not* cascaded;
   they are resolved by capability matching alone.

2. **Categories as a virtual cascade layer** — taxonomic groupings
   (`database`, `document-database`, `vector`, `relational-database`,
   etc.) resolve through the existing offering cascade by way of a
   *category index*: when no offering matches a bare name, the resolver
   consults a category table and returns offerings whose taxonomy
   includes the requested term.

3. **`wish` action** — find-or-provision is expressed as
   `?action=wish`. The resolver attempts cascade resolution; on miss,
   it triggers provisioning (subject to authorisation) and returns the
   provisioned endpoint.

4. **Replica pinning via `at=`** — pin to a specific seed-bank or stone
   with `?at=<name>`. URI-0001 already permits arbitrary query
   parameters; this ADR elevates `at=` to a documented standard
   parameter.

The cascade order, reserved keywords, equality rules, and round-trip
semantics from URI-0001 are unchanged.

---

## Translation Table

Every obsolete form has a single canonical URI-0001 + URI-0002
equivalent. Implementations MUST NOT accept the obsolete syntax (it
does not parse as a valid URI under URI-0001 anyway), and downstream
specs MUST be migrated.

### Named lookup (already covered by URI-0001)

| Obsolete | Canonical | Notes |
|---|---|---|
| `zen-garden:mongodb` | `zen-garden://mongodb` | Cascade hits offering |
| `zen-garden:mongodb/mydb` | `zen-garden://mongodb/mydb` | Sub-path = database/partition |
| `zen-garden:mongodb:staging` | `zen-garden://mongodb:staging` | Instance qualifier |
| `zen-garden:mongodb::staging` | `zen-garden://mongodb:staging` | Single colon canonical (specs/offerings.md drift fixed) |
| `zen-garden:mongodb:staging/myapp` | `zen-garden://mongodb:staging/myapp` | Instance + sub-path |

### Capability selection (URI-0002 §1)

| Obsolete | Canonical | Notes |
|---|---|---|
| `zen-garden:s3//` | `zen-garden://?cap=s3` | Empty target + cap query |
| `zen-garden:storage//` | `zen-garden://?cap=storage` | |
| `zen-garden:mongodb//` | `zen-garden://?cap=mongodb` | "Anything MongoDB-protocol" — different from `zen-garden://mongodb` (named lookup of the MongoDB offering) |
| `zen-garden:s3//minio` | `zen-garden://minio?cap=s3` | Named offering + capability constraint |
| `zen-garden:s3//minio:backup` | `zen-garden://minio:backup?cap=s3` | Named instance + capability constraint |
| `zen-garden:s3//minio/myapp` | `zen-garden://minio/myapp?cap=s3` | Named offering + sub-path + capability |

The semantic distinction between `zen-garden://mongodb` (cascade hits the
*offering* called "mongodb") and `zen-garden://?cap=mongodb` (any offering
exposing the MongoDB wire protocol) is preserved cleanly: target identity
versus capability constraint.

### Replica pinning (URI-0002 §4)

| Obsolete | Canonical | Notes |
|---|---|---|
| `zen-garden:s3//@seed-usb-01` | `zen-garden://?cap=s3&at=seed-usb-01` | Capability query + pinned location |
| `zen-garden:s3//minio@seed-usb-01` | `zen-garden://minio?cap=s3&at=seed-usb-01` | |
| `zen-garden:s3//{bucket}@{seed-bank}` | `zen-garden://{bucket}?cap=s3&at={seed-bank}` | Storage-API form |

`at=` accepts a stone name, bank name, or replica identifier. Resolvers
treat it as a hard constraint — if the named target cannot be resolved,
resolution fails (rather than silently falling back).

### Wish / find-or-provision (URI-0002 §3)

| Obsolete | Canonical | Notes |
|---|---|---|
| `zen-garden:wish//mongodb` | `zen-garden://mongodb?action=wish` | Cascade + wish action |
| `zen-garden:wish//mongodb/mydb` | `zen-garden://mongodb/mydb?action=wish` | |
| `zen-garden:wish//s3//{bucket}` | `zen-garden://{bucket}?cap=s3&action=wish` | Combined wish + capability |

Wish behaviour: resolver attempts cascade; on miss, requests provisioning
through the host Moss and returns the resulting endpoint. Provisioning
is subject to local authorisation policy. Read-only callers should not
use `?action=wish`.

### Categories (URI-0002 §2)

| Obsolete | Canonical | Notes |
|---|---|---|
| `zen-garden:database` | `zen-garden://database` | Cascade falls through offerings, hits category index |
| `zen-garden:document-database` | `zen-garden://document-database` | |
| `zen-garden:vector` | `zen-garden://vector` | |
| `zen-garden:database?tags=document` | `zen-garden://database?tags=document` | Existing tag filter preserved |

Category names are *not* reserved keywords; they live in a category index
(`garden-common::constants::categories`) that the cascade consults after
offering, stone, bank, service, companion, pond, and garden have been
attempted. Category lookups return the set of offerings whose taxonomy
contains the requested term, then re-run resolver selection (priority,
health) over that set.

A bare name's cascade order is therefore extended:

1. offering
2. stone
3. bank
4. service
5. companion
6. pond
7. garden
8. **category** (new — added by this ADR)

Categories ranking last preserves the principle that explicit named
matches always win.

---

## Capability-only URI shape

URI-0001 grammar requires `<target>`. URI-0002 relaxes that requirement
when (and only when) the URI carries a `cap=` query parameter:

```
zen-garden://[<target>][/<sub-path>][?<query>][#<fragment>]

<target> := <bare-name>
          | <kind>/<name>
          | (empty, if <query> contains cap=)
```

Empty-target URIs **bypass cascade**. Resolution proceeds entirely
through capability matching against the offering catalogue and seed-bank
gateways. This is exactly the intent the old `zen-garden:<protocol>//`
form expressed.

A URI with neither target nor `cap=` is a parse error.

---

## Equality and canonical form

Capability sets are compared as unordered sets. Two URIs are equal under
canonical form iff:

- Their parsed targets agree (or both empty)
- Their capability sets agree (`cap=tools,vision` ≡ `cap=vision,tools`)
- Their action values agree
- Their `at=` values agree
- Their remaining query parameters agree
- Their fragments agree

Canonical string form sorts query parameters alphabetically by key, with
multi-valued parameters (`cap`, `tags`) sorted within their value list.

---

## Implementation Requirements

### Parser changes

- `garden-common::uri::ZenGardenUri` adds `Option<String>` target
  (currently required) and validates that empty-target URIs include
  `cap=`
- `Koan.ZenGarden.Core` mirrors the change; existing
  `ZenGardenConnectionIntent` continues to require non-empty offering
  (it represents only the offering-cascade case)
- New `ZenGardenIntent` (Rust) / `ZenGardenIntent` (C#) is the broader
  parsed result type; `ZenGardenConnectionIntent` is one of its
  resolutions

### Resolver changes

- Empty-target URIs route through a `resolve_by_capability(caps, at)`
  path that does not touch the cascade
- Cascade gains a new final stage (category) consulting
  `garden-common::constants::categories`
- `wish` action requires the resolver to have a provisioning client
  (Moss API access); resolvers without provisioning capability return
  a typed error rather than silently failing

### Test vectors

The shared corpus at `docs/specs/zen-garden-uri-test-vectors.json` is
extended with cases for: empty target, `cap=`, `at=`, `action=wish`,
category resolution. Both Rust and C# parsers MUST pass.

### Categories index

A new constants module `garden-common::constants::categories` holds the
canonical category set. Initial entries derived from existing taxonomy:

```rust
pub const CATEGORIES: &[&str] = &[
    "database",
    "document-database",
    "relational-database",
    "key-value-store",
    "vector",
    "vector-database",
    "search-engine",
    "queue",
    "object-store",
    "storage",
    "cache",
    "stream",
    "ml-inference",
    "embedding-model",
];
```

Adding new categories is a `garden-common` change; URI grammar does not
need ADR amendment.

---

## Migration Plan

The protocol-prefix grammar is deprecated, not legacy-supported. This
ADR establishes:

1. **No parse-level backward compatibility.** Single-colon URIs do not
   parse and never did under URI-0001. Implementations MUST reject them.

2. **Documentation migration is incremental.** The translation table
   above is the canonical reference. Doc owners migrate their files
   on their own cadence using this table.

3. **Spec/proposal migration scope** (informational, not blocking this
   ADR's acceptance):
   - `specs/discovery.md` — rewrite §"Connection String Format" and
     resolution steps in URI-0001 + URI-0002 grammar
   - `specs/offerings.md` — same
   - `reference/driver-specification.md` — update parser pseudocode
   - `proposals/storage-api-design.md` — translate `zen-garden:s3//`,
     `zen-garden:storage//` forms throughout
   - `proposals/garden-aws-bridge.md` — translate AWS protocol handlers
   - `proposals/rake-find-command.md` — rewrite §"Wish Protocol" in
     `?action=wish` form
   - `proposals/ongoing/*` — translate as authors revise
   - Active proposals (`garden-naming-assessment.md`,
     `garden-federation-bridges.md`, `stone-lifecycle-operations.md`)
     update their simple-form occurrences

4. **CHANGELOG entries and historical archive content remain
   untouched.** They document state at a point in time.

---

## Rationale

- **Existing URI-0001 mechanisms are sufficient with small extensions.**
  Four small additions (empty target with `cap=`, category cascade
  layer, `wish` action, `at=` standard parameter) cover every intent the
  protocol-prefix grammar expressed. No grammar inflation.

- **Single canonical form per intent.** Each obsolete URI maps to
  exactly one URI-0001 + URI-0002 form, removing ambiguity and making
  the translation table mechanical.

- **Capability vs identity is preserved cleanly.** `zen-garden://mongodb`
  is "the offering named MongoDB"; `zen-garden://?cap=mongodb` is "any
  endpoint speaking the MongoDB protocol." Two distinct intents that
  the old grammar conflated under `mongodb//`.

- **Categories as last-resort cascade is honest.** Explicit named matches
  always win. Categories provide a fallback for "I don't care which
  database, just give me one." This matches user mental model.

- **Wish as an action verb scales.** Other resolution actions (`reset`,
  `health`, `metrics`) can join the same `?action=` slot without
  grammar revision.

- **`at=` formalises a query parameter that was already in informal
  use.** Documenting it standardises behaviour across resolvers.

- **No parse-level back-compat needed.** Old URIs were syntactically
  invalid under URI-0001 from day one; there is no installed base to
  preserve. The migration is a documentation exercise.

---

## Consequences

### Positive

- Five obsolete intents have one canonical expression each
- URI-0001 grammar gains one principled relaxation (empty target with
  `cap=`) and one cascade extension (categories)
- Doc migration becomes mechanical via the translation table
- Capability-vs-identity ambiguity in the old grammar is resolved
- New resolution kinds (categories, capability-only) extend without
  syntactic changes

### Negative

- `garden-common::constants::categories` becomes a maintenance surface
  — adding a category requires editing this constant
- `wish` action requires resolver authorisation policy (provisioning
  is privileged); not all callers can use it
- Empty-target URIs are syntactically valid only with `cap=`,
  introducing a context-sensitive parse rule

### Neutral

- The cascade gains an eighth stage (category) that runs on every miss
  through the first seven kinds. Performance impact is bounded by the
  category set size (small).
- Resolvers without provisioning capability must return typed errors
  for `?action=wish` rather than treating wish as a no-op

---

## Alternatives Considered

### Alternative 1: Preserve the protocol-prefix grammar as a legacy form

- **Description**: Make URI-0001/URI-0002 parsers accept
  `zen-garden:s3//foo` and translate internally to the canonical form.
- **Pros**: Old URIs that escape into the wild continue to work.
- **Cons**: Old URIs do not exist in the wild — the grammar was never
  implemented. The legacy code path adds parse complexity for zero
  installed-base benefit.
- **Rejected because**: There is no installed base to preserve.
  Single-colon URIs were never valid under any implementation.

### Alternative 2: Promote each obsolete intent to a reserved keyword

- **Description**: Add `wish`, `cap`, `at`, `category` as reserved
  keywords (e.g., `zen-garden://wish/mongodb`, `zen-garden://cap/s3`,
  `zen-garden://category/database`).
- **Pros**: Uniformity with existing reserved keywords; no query-param
  semantics needed.
- **Cons**: Inflates the reserved set; muddles "kind" (offering, stone,
  bank — these *exist*) with "modifier" (wish, capability — these
  *constrain*). Existing query parameters (`cap=`) already work.
- **Rejected because**: Modifiers belong in the query string, not the
  path. The cleaner separation matches RFC 3986 conventions.

### Alternative 3: Separate URI scheme for capability queries

- **Description**: Add `zen-garden-cap://s3` for capability queries,
  keep `zen-garden://` for named cascade.
- **Pros**: Each scheme has a single semantic.
- **Cons**: Doubles the OS handler footprint; loses unified branding;
  capability queries and named queries are facets of the same
  resolution intent.
- **Rejected because**: Same reasoning as URI-0001's rejection of a
  sibling navigation scheme. One vocabulary, multiple shapes.

---

## References

- [URI-0001](URI-0001-zen-garden-uri-scheme.md) — Cascade intent resolution scheme this ADR extends
- [PAVILION-0001](PAVILION-0001-windows-client-separation.md) — Pavilion ships the OS handler that consumes both URI-0001 and URI-0002 forms
- [DISC-0001](DISC-0001-discovery-as-first-class-crate.md) — `garden-discovery` is the resolver for these URIs
- [docs/specs/discovery.md](../specs/discovery.md) — Migration target (obsolete grammar removal)
- [docs/specs/offerings.md](../specs/offerings.md) — Migration target
- [docs/reference/driver-specification.md](../reference/driver-specification.md) — Migration target
- [docs/proposals/storage-api-design.md](../proposals/storage-api-design.md) — Migration target
- [docs/proposals/garden-aws-bridge.md](../proposals/garden-aws-bridge.md) — Migration target
- [docs/proposals/rake-find-command.md](../proposals/rake-find-command.md) — Migration target (`wish` translation)
