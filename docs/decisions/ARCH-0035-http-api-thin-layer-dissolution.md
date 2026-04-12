---
audience: [developer, ai]
doc_type: decision
status: accepted
date: 2026-04-12
depends_on: [ARCH-0017]
completed: 2026-04-12
---

# ARCH-0035: HTTP API Thin Layer Dissolution

**Date**: 2026-04-12
**Status**: Accepted
**Book**: XVII of [ARCH-0017](ARCH-0017-ddd-monolith-epic.md)
**Bounded context**: HttpApi (dissolved)

## Context

ARCH-0017 Book XVII specifies: "Every HTTP handler becomes a thin
command/query dispatcher. API response types separated from domain types
via DTO mapping. DTOs in `api/dto/` separated from domain types.
`FromRef<AppState>` extraction used throughout; no handler takes
`State<AppState>` as a whole. Error types translate to
`(StatusCode, Json<ErrorResponse>)` via a shared `IntoResponse` impl
per domain error type."

Chapter 1's discovery mandate requires re-evaluating this against the
actual code after sixteen books of aggregate extraction.

### Discovery findings (7 findings)

1. **Handlers already dispatch to domain aggregates.** After Books I-XVI,
   handler functions call aggregate typed commands and queries:
   `service_lifecycle::install()`, `service_lifecycle::stop()`,
   `state.jobs.get()`, `state.jobs.snapshot()`, `state.tool.find_by_fqid()`,
   `state.security.pond_status()`, `state.catalog.get_compiled()`,
   `state.catalog.get_manifest()`, `state.topology.snapshot()`, and so on.
   85 aggregate method calls exist across 27 handler files. Handlers are
   thin dispatchers to domain logic — the Book XVII goal is already met
   organically through sixteen books of domain extraction.

2. **`State<AppState>` vs `FromRef` is cosmetic, not architectural.**
   161 handlers take `State<AppState>`, 23 use `FromRef` extraction
   (`State<Arc<T>>`). The plan anticipated `FromRef` when handlers reached
   across domain boundaries through raw `Arc<RwLock<>>` fields. After
   sixteen books, handlers reach into their correct aggregate. A handler
   that does `state.jobs.get(id)` has the same dependency surface as one
   extracting `State(jobs): State<Arc<Jobs>>` and calling `jobs.get(id)`.
   The 161-handler migration would be a massive mechanical change with no
   boundary enforcement benefit. The 16 existing `FromRef` impls serve
   handlers that genuinely need only one aggregate (e.g., `capabilities.rs`
   needs only `Current`, `election.rs` needs only `Presence`). Handlers
   that need 2-3 aggregates (most handlers) would need multi-extractor
   signatures, which is noisier than `State<AppState>` with no
   architectural gain.

3. **DTO separation into `api/dto/` would reduce cohesion.** The plan
   called for a separate `api/dto/` directory. Handler files already
   define their DTOs inline — `OfferingView`, `CompanionSummary`,
   `CapabilitiesResponse`, `ContainerEntry`, `ValidateRequest`, etc.
   These types are consumed by exactly one handler file. Moving them to a
   separate directory would scatter the handler's contract across two
   locations with no reuse benefit. The current pattern (DTO next to
   handler) is the higher-cohesion arrangement.

4. **Remaining `state.offerings.read().await` sites are Book XVIII scope.**
   30+ handler sites still access the strangler vine (`offerings.read()`
   returning `ActiveGuard`). These are explicitly deferred to Book XVIII
   (Offerings Strangler Removal), which replaces `.read().await` with
   typed query methods. Book XVII cannot clean these up without
   duplicating Book XVIII's work.

5. **View-layer composition is correctly in the API layer.** Several
   handlers perform view composition: `list_offerings_v1` merges catalog
   entries with installed offerings and Docker runtime state (image name,
   uptime); `portrait.rs` builds a multi-aggregate stone portrait;
   `greenhouse.rs` assembles a catalog view with manifest file CRUD. This
   is presentation logic, not domain logic. Moving it into aggregates
   would violate separation of concerns — aggregates should not know about
   view shapes. The handlers are thin where they should be thin (command
   dispatch) and appropriately thick where they should be (view
   composition).

6. **Error translation already exists per-domain.** The plan called for
   shared `IntoResponse` impls per domain error type. Storage handlers
   already translate `BankError` to HTTP status codes. Security handlers
   translate `SecurityError`. Service lifecycle handlers translate
   `LifecycleError`. The error translation pattern emerged naturally
   through the aggregate extraction books. A shared macro or trait impl
   could marginally reduce boilerplate, but would not change the
   architecture.

7. **One genuine code smell: `helpers.rs` duplicates `format_bytes()`.**
   `api/v1/helpers.rs` contains a `format_bytes()` function that
   duplicates `garden_common::format_bytes()`. This is a 15-line
   deletion, not a book-sized refactor.

## Decision

**Dissolve Book XVII.** The HttpApi bounded context does not warrant
extraction as a separate architectural concern. The API handlers are
already thin dispatchers to domain aggregates — a property that emerged
organically through sixteen books of domain extraction, not through a
dedicated API-layer refactor.

The plan anticipated handlers that contained business logic, reached
across domain boundaries through raw locks, and returned domain types
directly. After Books I-XVI:
- Business logic lives in aggregates (typed commands/queries)
- Handlers dispatch to their aggregates and compose view responses
- DTOs are defined with high cohesion next to their handlers
- Error translation exists per-domain at the handler boundary

The remaining work items are either:
- **Book XVIII scope** (strangler vine removal: `offerings.read()`)
- **Cosmetic** (`FromRef` migration with no boundary benefit)
- **Trivial** (`helpers.rs` format_bytes duplicate)

## Consequences

- No `api/dto/` directory created — DTOs stay inline with handlers
- No forced `FromRef` migration — handlers that need multiple aggregates
  keep `State<AppState>`
- `helpers.rs::format_bytes()` remains a minor duplicate (not worth a
  separate commit; can be cleaned in any future touch)
- Context map updated to mark HttpApi as dissolved
- Pattern spec unchanged (no new deviations)
