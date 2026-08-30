# Design — The domain realignment (ADR-0015 worklist)

Working notes for the rebuild. Law lives in
`docs/v1/decisions/ADR-0015-domain-realignment.md`; this document is
the audit that turns it into a worklist. Acceptance for every step:
the W15 story runs green, unchanged; the wire, faces, MCP, catalog,
and checkpoint formats do not move.

## Target module map (moss internal)

| Target context | Absorbs today | Aggregate |
|---|---|---|
| journal | offerings/events.rs EventLog, capture_run runs map, pulse Bus, jobs JobTracker, kernel Dispatcher subscriptions | the event log |
| room | kernel (topology, announce, ingress, responder), moss source.rs | the Room (peers, presence, expiry) |
| garden | offerings service.rs (lifecycle), registry.rs, evaluate.rs, compile.rs + ports.rs (as the placement pipeline), facts.rs | the Offering |
| will | offerings capture.rs, capture_run.rs (dissolved), rehearse.rs | the Run (saga), the Checkpoint |
| stores | offerings storage.rs, directory.rs | banks, files |
| ledger | offerings ports.rs (as-is, pure) | claims |
| channels | http.rs, mcp.rs (thin); rake (client) | — |
| adapters | offerings docker.rs, detect.rs, capabilities.rs, sysinfo, fs | — |

## Event vocabulary (the journal speaks these)

Room: `PeerSeen`, `PeerExpired`, `GoodbyeSpoken`.
Garden: `OfferingPlanted`, `OfferingRested`, `OfferingWoke`,
`OfferingUprooted`, `OfferingReplanted`.
Will: `RunStarted`, `ImprintDone`, `CheckpointCommitted`,
`CheckpointDelivered{sink}`, `RunAborted{reason}`.
Stores: `BankAdopted`, `BankEjected`, `RolesDeclared`, `FileWritten`.
Job: `JobStarted`, `JobStep`, `JobProgress`, `JobFinished`.

Each event: `seq`, `at`, `aggregate` (fqn | run id | bank | stone),
`kind`, `data`. One JSONL stream per stone; replay at boot; broadcast
for live projections. Pulse, audit, MCP observe, the wall, and
delivery retries all read the journal — none of them is written to
directly anymore.

## Worklist (strangle order; delete in the same commit)

1. **DONE — will/** extracted: `policy` / `run` (the Run aggregate,
   forward-only) / `checkpoint` (entity: open refuses partials,
   tar-walking verify, `.partial` rotation match fixed) / `saga`
   (executor; pack is a thin call to checkpoint::commit). Cross-stone
   ferry lives here as the delivery leg.
2. **DONE — the Incarnation law is executable**:
   `Offering::reincarnate_on(dir, claims, pool)`; service.replant is
   a coordinator.
3. **DONE — runs persist**: each run's fate rides the offering's own
   audit chain; `replay_runs` rebuilds at boot; in-flight-by-restart
   runs are marked interrupted (law 3).
4. **DONE — debts closed**: scheduler consumes interval's immediate
   tick; announcer re-asserts full voice every 10th heartbeat
   (law 6) — witnessed live in W16.
5. **DONE — rake has one router**: stone_op follows not-here once;
   offering_op deleted.
6. **DONE — the journal breathes**: run fates, lifecycle audits,
   room events (peers seen/expired), and the stone's own goodbye all
   land in stone.jsonl (typed, seq'd, replayed at boot). Witnessed
   live on .195. Pulse stays the live-rich feed (samplers, shapes);
   jobs stay per-job documents — settled, see 14.
7. **DONE — the room context is named and owned**: the wire plane is
   the `garden-room` crate (ingress, dispatch, announce, topology,
   responder, probe, pipeline, config); moss holds the room facade and
   the stone's voice (room/voice.rs). Farewell atomic; streams end on
   the token.
8. **DONE — the wish is room-level**: ensure walks every answering
   stone's own view before planting; a bystander can no longer silence
   the wish.
9. **DONE — the law of names**: replant refuses a name still sung by a
   living peer (best-effort over the room cache — gossip is eventual;
   a heard name is proof enough to refuse).
10. **DONE — the wall never haunts**: feed loss renders the strip as
    "last known", dimmed.
11. **DONE — the everyday verbs come home**: `offering.rest(&world)`,
    `.wake(&world)` (returns WakeOutcome: started / resurrected /
    already-running), `.uproot(&world)` (idempotent at the world's
    edge). The service slims to load-invoke-persist; faces unchanged.
12. **DONE — Provenance**: `garden/provenance.rs`. `plan_install` is
    the dry twin (same compile install will run, NOTHING placed,
    can/cannot + the decision trail); `install` runs the plan as a
    JOB (progress rides the pulse; the plant face answers with an
    additive job_id). Additive surfaces: the PlanInstall face
    (+ regenerated surface.json), the MCP plan-install tool, and
    `rake offer --plan` — witnessed live on .195: ollama --plan says
    "already planted"; mongodb --plan speaks the whole decision trail
    and places nothing.
13. **DONE — the Moss facade begins**: `state.provenance()` —
    `moss.provenance().install("ollama")` is now the shape.
14. **Settled by decision**: jobs.rs IS the Job aggregate's durable
    store (per-job documents, boot reconciliation) — complementary to
    the stone's fact stream, not a duplicate; the per-offering
    events.jsonl is the aggregate's own chain and rides the checkpoint.
    Both stay. Optional, low-value: merging glossary into contract.

## Named debts riding along

- Scheduler's immediate first tick runs every will at boot — a boot is
  not a calendar (witnessed 3× in W15).
- Wall renders feed-loss, not the goodbye datagram (rake-wall).
- Gossip: a booting stone whose rich answers are lost stays inventory-
  blind until a peer sings (law 6 fixes by re-assertion).
