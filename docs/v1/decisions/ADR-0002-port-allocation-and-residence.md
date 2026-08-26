# ADR-0002 — Port addresses: stable allocation, honest residence

**Status:** Accepted · 2026-08-26
**Supersedes:** nothing whole; **amends** [ADR-0001](ADR-0001-offering-directory.md)
law #2 — the `spec.preferred_ports` mechanism is kept exactly, but its meaning
is promoted from "last-observed placement constraint" to "carrier of the
offering's port allocations"
**Retires on implementation:** DEBT D14
**Referenced by:** OFFERINGS.md §4/§5.3 (amended), WITNESSES W4 finding, lessons L26

## Provenance

Raised in the post-W4 evaluation cycle (2026-08-26) by the operator: the PoC's
address story served garden-native clients ("ask where Ollama is" → a fresh
URL every time) but broke for a real and permanent class of consumer —
services that query rake once and burn the URL into a config file forever.
A NON-managed neighbour squatting on an offering's port exposed the deeper
design gap: the system conflated *who owns an address* with *where a workload
currently answers*. Three rulings were made in that discussion and are binding
for this ADR:

1. Service pool defaults to 7300–7449 (`MOSS_SERVICE_PORT_POOL` overrides),
   clear of the reserved 7284–7299 infra block.
2. Requiredness of a specific host port is DECLARED IN THE MANIFEST —
   namespaced per role — because pihole can only ever be :53 while Mongo does
   not care at all. The manifest knows which world it lives in; moss must not
   guess.
3. Homecoming is OPPORTUNISTIC ONLY: never recreate a healthy running
   workload merely to return it home. Additionally, pool assignment consults
   the claim ledger FIRST — "no need to check if 8080 is in use, just that
   it's already taken by a valid offering, even if it's offline."

## Context

W4 witnessed wake remapping memcached from one random host port to another:
containers were created with dynamic bindings (`""` HostPort), so Docker
reassigns on every `docker start`. Moss recorded the change honestly — but
honesty about breakage is still breakage for any client holding the old URL.

The naive remedies are both wrong:

- **Fixed ports** collide — with other offerings, with operators' own
  hand-installed services, across multi-service manifests sharing default
  container ports like 8080.
- **Floating ports** betray URL-burning clients at every relocation, forever,
  with no healing.

The dichotomy is false because it answers two different questions with one
mechanism: an address's OWNER is a matter of identity; its CURRENT USE is a
matter of runtime fact.

## Decision

Separate the two questions into three mechanisms:

### 1. Allocation = identity (stable)

Every managed offering holds named host-port **allocations**, persisted inside
its offering directory (the rehydration contract absorbs them). Allocation is
assigned at first plant by scanning the stone's claim ledger — active,
candidate, rested, and adopted claims all count — and taking the next free
pool slot. Reality (sockets) is never probed to arbitrate BETWEEN GARDEN
MEMBERS: a rested offering's claim is as good as a running one's. Allocations
are disjoint by construction, which makes intra-garden port disputes
unrepresentable rather than merely detected.

Manifest grammar (OFFERINGS.md §5.1):

```yaml
managed:
  ports: { default: 11211 }          # container side — unchanged
  host_ports:                        # allocation intent; absent role = flexible pool
    dns: { port: 53, strict: true }  # identity-critical: refuse plant if taken
    ui:  { port: 8080 }              # preferred home; squatting tolerated
```

Three tiers: **strict-pinned** (validation rule: `strict` requires explicit
`port`; occupied at plant → loud Conflict with a suggestion naming the
squatter), **soft-pinned** (home preference), **flexible** (pure pool).

### 2. Residence = fact (volatile)

Where the workload actually answers now. Adapters, chirps, GET, and rake
report it truthfully — today's `port_map` behaviour unchanged. At any CREATE
moment (first plant, wipe-healing, resurrect-after-loss), the adapter binds
the best available address: home when free, else a relocated choice.
**Every create emits explicit HostPort bindings — never `""`** — so Docker
can no longer reshuffle addresses behind our backs on `start`.

If home is squatted by a non-garden process at create time, the offering is
created relocated and the audit trail carries the reason
(`Relocated{because}` — first-class events, never log-line archaeology).

### 3. Arbitration = pure policy

The decision function over `{claims, observations, tier}` — relocate/return/
hold — is PURE (fixture-testable without Docker), keeping live socket probing
at the edges. One invariant governs transitions:

> **No recreation solely for addressing.** Residence changes ride existing
> re-creations only. The Converger OBSERVES residence≠allocation divergence
> and records it; it NEVER acts on it. An offering whose squash time ended
> returns home at its next natural rebuild — heal, wake-after-loss, replant —
> and not before.

## Law encoded

> An address has an owner before it has a value. Ownership lives in the
> ledger and survives rest; reality is reported, never rewritten backwards;
> no citizen of the garden is recreated for the convenience of its own
> bookkeeping.

Supporting rules:

1. **Ledger before sockets.** Between garden members, claims decide. Probing
   is only for outsiders.
2. **Requiredness is declared, not inferred** — the strict tier exists
   precisely because some services ARE their port and others are not.
3. **Explicit bindings always** — adapters receive concrete ports on every
   create; empty/dynamic HostPort is a removed failure mode (D14).
4. **Adoption registers too** (O3 forward-compatibility): when adoption
   detects a foreign service listening on a port, it records a claim, so
   future managed plants politely route around adopted neighbours.
   Borrowed entries point outward via connection_url and hold no claim.

## Alternatives considered

### A. Preserve-last-port forever (D14's original framing)

Persist whatever port was last used; converge/wake fight to restore it.

**Cons:** that value comes from observation, so the FIRST arbitrary ephemeral
assignment hardens into identity; under neighbour pressure it produces flapping
(start fails → healed elsewhere → heals back thrash); no vocabulary for
"temporarily elsewhere"; conflicts between two preserved claims need new
arbitration anyway.

### B. Deterministic hash-derived ports (FQN hash → fixed slot)

Allocation derived from name + role.

**Pros:** zero storage, no ledger. **Cons:** collisions need resolution logic
anyway (which reintroduces a ledger); manifest soft-pins override it; renaming
would migrate addresses; looks elegant, hides the same decisions behind math.

### C. Only report residence, allocate nothing (status quo generalised)

Rely entirely on garden-module clients re-querying.

**Cons:** abandons URL-burners permanently; breaks address-stability promises
inside compose-style config files; discards the discovered requirement mid-W4.

### D. Fixed well-known ports per catalog entry

Catalogs ship canonical host ports (redis=6379).

**Cons:** collides across stones sharing hosts, against operator services,
and across the many manifests sharing 8080; grants catalogs authority over
host reality they cannot see. Soft-pinning preserves the useful subset
("prefer my usual") without granting supremacy.

## Consequences

### Positive

- URL-burners survive: burned URLs are correct except during neighbour-driven
  windows, and self-heal at next natural rebuild.
- Noisy neighbours degrade gracefully instead of catastrophically — the
  offering answers somewhere while its identity remains claimed.
- D14 closes structurally: explicit bindings end dynamic reshuffling.
- Adoption/borrow land on a finished address story instead of building one.
- The audit trail narrates address history as first-class events.
- Posture/explain can show `resident ≠ allocated` honestly.

### Negative / costs

- Directory schema gains an allocations section — one-time auto-migration
  derives each existing offering's initial allocation from its current
  `port_map` (mirroring the legacy-JSON migration pattern).
- Relocated periods mean clients see two URLs over an offering's life; the
  room-wide answer stays truthful, but integrators must be told the
  convention (allocated = long-run stable).
- Pool sizing becomes operational knowledge (7300–7449 default ≈ 150 roles
  per stone before env tuning).
- Strict pins place refuse-power in manifests — surfaces operator mistakes
  loudly at plant time (intended).

### Neutral

- Chirp wire shape unchanged (residence was already what we published);
  exposing allocations on the wire is deferred until a consumer exists.
- Non-container worlds inherit the same semantics when they arrive — the seam
  is WorkloadSpec fields, unchanged in shape, promoted in meaning.

## References

- Discussion: post-W4 evaluation cycle, 2026-08-26 (operator rulings quoted
  in Provenance)
- Implementation slices follow this ADR: pure arbiter+allocator → directory
  schema v2 + migration → adapter explicit-bind mechanics → Converger
  observation-only stage → witness W5 (the noisy-neighbour choreography)
- Amends: [ADR-0001](ADR-0001-offering-directory.md) law #2 (mechanism kept,
  meaning promoted)
- Retires on implementation: `src/v1/DEBT.md` D14
