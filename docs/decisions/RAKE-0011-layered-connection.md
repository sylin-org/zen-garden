---
audience: [developer, ai]
doc_type: decision
status: proposed
---

# RAKE-0011: Layered Connection Architecture

**Date**: 2026-04-06
**Status**: Proposed
**Depends on**: ARCH-0012 (StoneApi typed client)

## Context

Rake resolves a stone endpoint once (from `--at` flag, env var, `.tending` file,
or UDP discovery) and passes it as a bare `String` to commands.  Commands
construct their own `StoneApi` or use raw `reqwest::Client` with that string.

When the endpoint becomes stale (DHCP reassignment, stone offline), nothing
recovers.  The `.tending` file has no TTL by design -- it persists until the
stone is unreachable.  But "unreachable" was never detected: `resolve_endpoint`
returned the cached string optimistically, the command failed with a TCP error,
and the error bubbled up to the user with no re-resolution.

Concrete failure: a stone's IP changed from `.175` to `.171`.  The `.tending`
file still said `.175`.  Every `garden-rake` invocation resolved to `.175`,
failed to connect, and never fell through to discovery.  The stone was live and
discoverable -- rake just never tried.

Additional problems with the current design:

- **52 call sites** do `ctx.stone_api()?` -- the `?` exists because the context
  holds `Option<StoneApi>`, even though commands that require an endpoint always
  have one.  The `Option` is a type-system lie.
- **4 direct callers** in `route.rs` call `resolve_endpoint` outside of
  `Runtime::execute` and get no recovery at all.
- **Pulse** (SSE streaming) swallows connection errors in its reconnect loop,
  hammering a dead endpoint forever.
- **`client.rs`** contains `resolve_target_endpoint` which overlaps with
  `dispatch.rs::resolve_endpoint` -- two resolution functions in two files.

## Decision

Replace the bare-string endpoint flow with a three-layer connection module in
`garden_rake::connection`.  Each layer composes the one below.

### Layer 3: Resolution

Pure computation.  Answers "what endpoint should I talk to?" with provenance.

```rust
// connection/resolution.rs

pub struct Resolved {
    pub endpoint: String,
    pub origin:   Origin,
}

pub enum Origin {
    Flag,        // --at: user intent, never re-resolved
    Env,         // ZG_STONE: operator intent, never re-resolved
    Tending,     // .tending file: cached, flushable on TCP failure
    Discovered,  // UDP/mDNS: just found, flushable on TCP failure
}

impl Origin {
    pub fn is_soft(&self) -> bool {
        matches!(self, Self::Tending | Self::Discovered)
    }
}

pub async fn resolve(
    client: &reqwest::Client,
    at: Option<&str>,
    cache: Option<&dyn CachedStoneOps>,
) -> Result<Resolved>;
```

`Origin::is_soft()` encodes the policy: explicit user overrides (`Flag`, `Env`)
are never invalidated by automatic recovery.  Cached or discovered endpoints are.

### Layer 2: Stone (bound connection)

Binds a resolved endpoint to an HTTP client.  Provides typed API access, raw
HTTP, and endpoint metadata.  No recovery -- caller controls lifecycle.

```rust
// connection/stone.rs

pub struct Stone {
    resolved: Resolved,
    api:      StoneApi,
    bag:      StoneBag,
}

impl Stone {
    pub fn bind(client: reqwest::Client, resolved: Resolved) -> Self;
    pub fn api(&self) -> &StoneApi;
    pub fn http(&self) -> &reqwest::Client;
    pub fn endpoint(&self) -> &str;
    pub fn origin(&self) -> &Origin;
    pub fn is_reclaimable(&self) -> bool;
    pub async fn name(&self) -> Option<&str>;
}
```

This is what streaming commands (pulse, log tailing) use.  They manage their
own reconnect loops and need direct access to the HTTP client and endpoint.

### Layer 1: Resilient (bound connection + recovery)

Wraps `Stone`.  On TCP connection failure (refused, timeout -- not HTTP errors),
flushes stale tending if the origin is soft, re-resolves, and retries once.

```rust
// connection/resilient.rs

pub struct Resilient {
    stone:  Stone,
    client: reqwest::Client,
    at:     Option<String>,
    cache:  Option<Arc<dyn CachedStoneOps>>,
}

impl Resilient {
    pub fn stone(&self) -> &Stone;

    pub async fn execute<F, Fut, T>(&mut self, op: F) -> Result<T>
    where
        F: Fn(&Stone) -> Fut,
        Fut: Future<Output = Result<T>>;
}
```

Recovery logic lives in one place.  `Runtime::execute` builds a `Resilient`
for standard commands.  Commands never see the recovery -- they receive a
`Stone` reference inside the closure.

### Context split: Connected / Local

The current `context::Runtime` holds `Option<String>` endpoint and
`Option<StoneApi>`.  Commands that require an endpoint always have one, but
the type does not guarantee it.

Split into two context types:

- **`Connected`**: holds a `Stone` reference.  `api()` returns `&StoneApi`
  (not `Option`).  Used by all commands with `requires_endpoint() == true`.
- **`Local`**: no stone, no API.  Used by local-only commands (version, help,
  completions).

The `Command` trait splits accordingly.  Commands that need a stone receive
`Connected` -- the compiler enforces it.  No more `ctx.stone_api()?`.

### Module structure

```
src/rake/src/
  connection/
    mod.rs          -- re-exports Stone, Resilient, Resolved, Origin
    resolution.rs   -- Layer 3: resolve(), Resolved, Origin
    stone.rs        -- Layer 2: Stone
    resilient.rs    -- Layer 1: Resilient
  tending.rs        -- unchanged (persistence)
  discovery.rs      -- unchanged (UDP/mDNS)
  stone_bag.rs      -- unchanged (lazy capabilities)
  context.rs        -- Connected / Local (replaces Runtime)
  dispatch.rs       -- uses Resilient, builds Connected/Local
  client.rs         -- removed (absorbed into resolution.rs)
```

### What stays in garden_common

`StoneApi` and `StoneApiError` are unchanged.  The common crate remains
unaware of tending, discovery, or re-resolution.  `Stone` in rake composes
`StoneApi` -- it does not wrap or replace it.

## Consequences

### Positive

- TCP connection failure triggers automatic re-resolution (the original bug)
- `Origin::is_soft()` makes re-resolution policy explicit in the type system
- `Connected` context eliminates 52 instances of `ctx.stone_api()?`
- Resolution lifecycle is one module, not scattered across dispatch/client/route
- Streaming commands (pulse) use `Stone` directly -- no special escape hatches
- `StoneApi` in garden_common is untouched -- no cross-crate changes

### Negative

- Breaks all ~70 command implementations (context type change)
- Breaks 4 direct `resolve_endpoint` callers in route.rs
- Single large commit touching many files

### Risks

**Risk**: Large blast radius introduces regressions.
**Mitigation**: `cargo check --all` + `cargo test` after implementation.
All changes are mechanical (context type swap, `?` removal).

## Alternatives Considered

### Phased migration (keep old context, add new alongside)

Carry two context types, adapter shims, and `Option` wrappers while commands
migrate incrementally.  Phase 2/3 may never happen.  Rejected: the migration
tax exceeds the risk of a single coherent change.

### Health probe in resolve_endpoint

Probe the tended endpoint at resolution time.  Adds ~50ms latency to every
command on the happy path (99.9% of invocations).  Rejected: unnecessary
friction for a rare failure case.  The correct trigger is actual connection
failure, not speculative probing.

### reqwest middleware wrapper

Intercept all HTTP calls at the transport level.  Requires cloning/rebuilding
request builders (consumed on `.send()`).  Over-engineered for a rare failure
case.  Rejected: `Resilient::execute` is simpler and more explicit.

## References

- ARCH-0012: StoneApi typed client (composed into Stone)
- Code standards section 9: typestate for phased initialization
- Code standards section 14: one file per concept
