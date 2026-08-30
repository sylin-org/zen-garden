# ADR-0015 — The domain realignment: Moss is a DDD monolith

**Status:** Accepted (2026-08-30)
**Trigger:** W15 (the full integration exercise). Five live failures,
one shape: each was a seam where two laws met and neither owned the
case. Operator direction: entities handled as entities, not data;
events and messages instead of sequences; atomic actions coordinated
internally; a small number of meaningful moving parts; days of
break-and-rebuild are acceptable; low-value bureaucracy is not.

## Context

Moss's modules each implement their ADR faithfully. The failures were
never inside a module — they were at the unnamed boundaries:

- The catalog validator knew hookless lock-and-copy is an honest
  copy-freely will; the capture executor still demanded hooks (W15
  P5). One business rule, two owners, divergence in the dark.
- The redirect law ("reads delegate, writes bind at their authority",
  ADR-0004 §4) is re-implemented per face and per verb — and each
  re-implementation covers a different subset (W15 P5).
- The stored workload spec — one stone's *infrastructure projection*
  (Windows host paths, ledgered ports) — was replayed as if it were
  the offering's *identity* (W15 P7: Docker "invalid mode", port
  collision).
- The replication lane of ADR-0005 §4 existed only for local banks;
  the cross-stone ferry was assumed and never built (W15 P5).
- The farewell was the last line of a shutdown sequence; any client
  holding a stream could silence it forever (W15 P9 — W12's note,
  resurfacing).

The common disease: **the domain entities — Stone, Offering, Run,
Checkpoint, Bank — are anemic records, so the behavior that belongs to
them lives in services, schedulers, and free functions, coordinated by
imperative sequences.** Sequences between contexts are exactly where
the seams tear.

## Decision

Moss stays ONE deliverable (with rake as the client). Internally it
reorganizes around a small number of contexts, entities with
behavior, and one event spine. Six laws, executable in code:

1. **The Incarnation law.** What survives a stone change: FQN,
   identity (offering_id), declared will, image reference, port roles
   and tiers, volume names. Everything else — host paths, concrete
   ports, container names — is a *projection*, recompiled by the
   placement pipeline on every stone. Replant is restore-the-aggregate
   then recompile; the address arbiter keeps free homes, redraws
   flexible ones, refuses strict disputes.
2. **One router.** Not-here is a typed reply naming the home; exactly
   one client-side router per channel follows it once. No verb, face,
   or tool implements routing itself.
3. **A will is a saga.** Every step idempotent; `resume` declared as
   the compensation beside `quiesce`; one place interprets a will.
   A failed run leaves a resumable state, never a husk.
4. **Delivery is at-least-once.** A committed checkpoint fans out
   per-sink, acked, re-asserted across restarts; local mounts and
   remote sinks differ only by adapter. The manifest is the commit
   marker everywhere.
5. **Farewell is atomic.** Goodbye is published at the instant of
   decision; draining waits on nothing; every stream face ends on the
   shutdown token.
6. **Gossip re-asserts.** Lean heartbeats plus slow periodic
   full-voice songs; the room converges from loss and depends on no
   single datagram.

### The shape

Six internal contexts, each owning an aggregate, coordinating ONLY
through the journal (a context may call itself; contexts talk in
events):

| Context | Aggregate | Emits (examples) |
|---|---|---|
| room | the Room: peers, presence, expiry | PeerSeen, PeerExpired, GoodbyeSpoken |
| garden | the Offering | Planted, Rested, Woke, Uprooted, Replanted |
| will | the Run (saga), the Checkpoint (immutable) | CaptureStarted, CheckpointCommitted, Delivered{sink}, RunAborted |
| ledger | the address ledger | claims read model |
| stores | banks, roles, files | BankAdopted, FileWritten |
| journal | the event log itself | subscriptions |

The spine is a consolidation: the four current event mechanisms
(per-offering audit, RunInfo map, pulse bus, kernel dispatcher) become
ONE typed journal. The pulse wall, MCP observe, audit trails, and
delivery retries are projections and reactions, never second brains.

### Entities behave

Entities own their stories; services shrink to coordinators (load,
invoke, append, persist):

- `offering.rest()`, `offering.uproot()` — whole transitions: validate,
  mutate, journal, delegate mechanics to adapters. Illegal states
  unrepresentable (enum phases, never strings).
- **Plan twins.** Every mutating verb has a report twin from the same
  decision path — `plan_install()` / `install()`. `explain` becomes
  the dry-run of everything, not a separate read model.
- **One Job.** Installation, capture, replant, nourish share one
  persisted, resumable Job (identity, ordered steps, progress as a
  projection of its events). Today's three partial job mechanisms
  merge into it.
- **Parse, don't validate.** A will is parsed into a policy type that
  yields an executable plan; an executor walks plans and cannot
  disagree with the validator.

### The freeze

The wire (chirps/songs/goodbyes), the face paths, the MCP tools, the
catalog YAML, and the checkpoint format on dumb storage DO NOT CHANGE.
The W15 story (P0–P10) is the acceptance test and must run green,
unchanged, against the rebuilt shape.

## Alternatives considered

- **Patch the seams as they fail.** Rejected: tonight proved the seams
  regenerate — each fix was honest, but the pattern is unbounded while
  the laws remain unnamed.
- **Rewrite from scratch.** Rejected: the wire, the pipeline, the
  arbiter, and the contract tests are correct and battle-worn; the
  realignment moves behavior home, it does not rediscover it.
- **Microservices-per-context.** Rejected: one stone is one deliverable;
  the contexts share a process and a journal, and split only if a
  network boundary ever earns its cost.

## Consequences

- Rebuild order (strangle by context): journal → will → garden → room
  → channels; delete each absorbed special case in the same commit
  that introduces its law.
- W15's five seams resolve as: 1 & 3 into the router and the policy
  type; 2 into will's delivery ledger; 4 into the Incarnation law;
  5 into room's farewell. The two witnessed protocol debts (scheduler
  boot-tick, gossip re-assertion) are named work, not surprises.
- Acceptance: W15 green unchanged; every public function in moss
  classifies as command, event, projection, or adapter; zero domain
  logic in channels; the entity method list is the documentation.

## References

W15 (the five seams, witnessed live) · ADR-0004 §4 (redirect law) ·
ADR-0005 §§2–6 (will, sinks, replant) · ADR-0002 (address law) ·
ADR-0014 (channels law) · DEBT entries opened with this ADR.
