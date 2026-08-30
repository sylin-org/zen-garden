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

1. **journal** — new module: typed events, append, subscribe, replay.
   Migrate writers one at a time; delete EventLog/RunInfo/JobTracker/
   pulse-bus-as-source-of-truth as each moves. Pulse becomes a
   projection; jobs become Job aggregates over run/step events.
2. **will** — dissolve capture_run.rs (1.5k lines, four jobs) into:
   `Run` state machine (start → imprint → committed → delivered{},
   abort with compensation), `Checkpoint` entity (open = Committed |
   Partial; deliver_to(sink) → ack), the imprint mechanics adapter,
   and `Provenance` (plan_install / install). Scheduler becomes a
   due-date trigger on the journal. Absorb tonight's cross-stone
   ferry as the delivery adapter (delete the inline HTTP client).
3. **garden** — Offering gains behavior: `rest/wake/uproot/
   reincarnate_on(&dirs, &ledger)`; placement pipeline (compile +
   arbiter + path re-rooting) is THE only compiler of projections;
   replant = restore + recompile (deletes W15's special cases).
   service.rs shrinks to coordinators.
4. **room** — absorb kernel + source.rs; `Stone::depart()` speaks the
   goodbye at the instant of decision (done at P9, keep); songs re-
   assert full voice periodically (named debt); expiry stays.
5. **channels** — rake's three redirect followers collapse into one
   router; moss faces share the typed not-here reply; the wall stays a
   renderer (goodbye-as-wire-event debt lands here).

## Named debts riding along

- Scheduler's immediate first tick runs every will at boot — a boot is
  not a calendar (witnessed 3× in W15).
- Wall renders feed-loss, not the goodbye datagram (rake-wall).
- Gossip: a booting stone whose rich answers are lost stays inventory-
  blind until a peer sings (law 6 fixes by re-assertion).
