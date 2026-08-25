# 11 — One Supervisor, One Route Table

> Every long-lived task runs under the supervisor (its own contract: "no second path"); the two
> hand-duplicated route sets become one declarative table with per-route policy. Phase: Structure.
> Depends on: 02, 09 (probe is the regression net). Feeds: 12, 13, 16.

## Mission

Two single-source-of-truth claims in moss are currently false, and both have bitten:

1. The task registry says "No second path, no duplication" while ≥8 long-lived tasks are raw-spawned
   outside it — including the snapshot scheduler whose JoinHandle is *discarded* at spawn despite its own
   doc saying the caller should keep it. The May snapshot-runaway incident lived in this bypass path.
2. The router maintains two hand-duplicated route sets (~266 verbatim lines) that have already drifted
   once (an endpoint registered only in the public set, contradicting the file's own doc comment).

Make both claims true, structurally: a route is declared once with a `public`/`privileged` tag; a
long-lived task exists only as a registry entry. After this prompt, adding a bypass spawn or a
single-listener route should be *harder* than doing it right.

## Ground truth (verified 2026-06-11 — re-verify each)

| Fact | Re-verify |
|---|---|
| Registry: 30 always-on unit-struct + 2 owned-state = 32 always-on, 4 conditional (`src/moss/src/tasks/task_registry.rs`, claim at line ~35) | read task_registry.rs:1-120 |
| ≥8 long-lived unsupervised: snapshot scheduler (`bootstrap/run.rs:183`, JoinHandle discarded), DockerMonitor, network monitor, discovery UDP listener (`tasks/discovery.rs:114`), 3 event-bus listeners + transport tap (`tasks/coordinator.rs:90-103`), plus HTTPS/S3 listeners | `grep -rn "tokio::spawn" src/moss/src/bootstrap/run.rs src/moss/src/tasks/coordinator.rs src/moss/src/tasks/discovery.rs` |
| 77 raw `tokio::spawn` sites in moss excluding tests (many are legit per-request/short-lived — only LONG-LIVED loops must move) | `grep -rn "tokio::spawn" src/moss/src --include="*.rs" \| grep -v "#\[cfg(test)\]" \| wc -l` |
| Finite one-shots that may legitimately stay raw (document, don't move): first-boot tasks (`run.rs` ~1354/1516), Windows DNS maintenance (~1551) | read those sites |
| Router: `configure_public()` lines ~62-377 (84 registrations), `configure()` ~384-1179 (196); 81/84 public registrations verbatim-identical in configure(); 280 total registrations, 213 unique method-path endpoints | `grep -c "\.route(" src/moss/src/bootstrap/router.rs` |
| The verified drift bug — `GET /api/v1/stone/banks/{moniker}/seeds` public-only — may already be fixed by prompt 05; check | `grep -n "seeds" src/moss/src/bootstrap/router.rs` |
| Prompt 05 added auth layers to mutating routes in BOTH sets — your declarative table must absorb that policy as a first-class column, not lose it | read prompt 05's diff (`git log --oneline --grep="auth"`) |
| The supervisor provides: DAG-validated startup order, panic capture, per-task cancellation tokens, `/tasks` status API — extend it, never fork it (ARCH-0015) | `ls src/moss/src/tasks/` |

## Research first (~60 min)

1. Read `task_registry.rs` + the supervisor implementation end to end: how a task declares dependencies,
   shutdown semantics, what an owned-state task looks like (the 2 existing ones are your templates).
2. Read `bootstrap/run.rs`'s spawn sites in context — each has a reason it bypassed (usually: needs state
   constructed later, or predates the registry). Note which need a new DAG node vs a conditional slot.
3. Read `bootstrap/router.rs` fully; catalog every divergence between the two sets (the 3 non-identical
   public registrations, the 2 deliberate method-subsets) — these become explicit flags in the table.
4. Read how the dual-listener split works (which listener serves which configure fn, pond-state switch).

## Plan gate

Post: (a) the task migration table — every long-lived bypass spawn → its registry entry name, DAG deps,
shutdown behavior; (b) the one-shot allowlist you are NOT moving, with one-line justifications; (c) the
route-table design (below) and the diff strategy. No OPERATOR items expected unless you find a spawn
whose shutdown semantics are unclear — then ask.

## Target shape

Declarative route table — one definition, listener membership and auth policy as data:

```rust
// bootstrap/routes.rs (new) — THE table. router.rs shrinks to interpretation.
route(GET,  "/api/v1/stone/services",          list_services,   Public,     Read),
route(POST, "/api/v1/stone/services/:s/wake",  wake_service,    Public,     Write),
route(POST, "/api/v1/stone/deploy",            deploy_stone_v1, Privileged, Write),  // HTTPS/full only
route(GET,  "/api/v1/stone/banks/:m/seeds",    bank_seeds,      Public,     Read),
// Public  → registered on both listeners (lobby + full)
// Privileged → full router only
// Write   → wrapped in require_write_auth() (prompt 05's middleware)
```

`configure_public()` = filter(Public); `configure()` = all. The drift class dies: a route CANNOT exist in
one set only, except by the explicit `Privileged`/`LobbyOnly` tag. Add a unit test that walks the table
and asserts: no duplicate method+path, every Write row has auth, lobby ⊆ full (modulo LobbyOnly).

Supervised task — the snapshot scheduler as the worked example:

```rust
// tasks/task_registry.rs — gains:
TaskDef::owned("snapshot-scheduler", deps: ["docker", "storage"], |state, token| {
    snapshot::scheduler_loop(state, token)   // loop selects on token.cancelled()
}),
// bootstrap/run.rs:183's raw spawn: deleted.
```

## Implementation

1. **Tasks first** (independent of router work): migrate the ≥8 long-lived spawns one per commit —
   snapshot scheduler, DockerMonitor, network monitor, discovery UDP listener, the 3 event-bus listeners
   + tap. Each: registry entry + cancellation-token-aware loop + delete the raw spawn. Verify after each:
   moss boots (`cargo run -p garden-moss` smoke), `/tasks` API lists the new entry, ordered shutdown
   clean (no "task did not stop" warnings in a Ctrl-C run).
2. Document the one-shot allowlist as a comment block in task_registry.rs — the registry's claim becomes
   "no second path for LONG-LIVED tasks; one-shots listed here".
3. **Route table**: build `routes.rs` + the interpreter; port all 280 registrations (mechanical;
   triple-check the 3 known divergences and prompt 05's auth wraps); delete both old configure bodies;
   add the table-invariant unit test.
4. Run probe (`--suite all`) against a local moss after each phase — route porting mistakes show up here
   first.
5. Commits: one per migrated task, then `refactor(moss): declarative route table (one source of truth)`.

## Definition of done

- [ ] `grep -n "tokio::spawn" src/moss/src/bootstrap/run.rs` → only sites on the documented one-shot
      allowlist (paste mapping).
- [ ] `/tasks` API (or supervisor debug output) lists the migrated tasks; paste a boot log showing them
      registered and a shutdown log showing them stopped in order.
- [ ] router.rs no longer contains two route bodies; the table test passes (no dupes, lobby ⊆ full, every
      Write row authed); endpoint count preserved: 213 unique method-paths before == after (script the
      count both ways and paste).
- [ ] Probe: 0 failed, before vs after identical pass counts.
- [ ] `cargo test --workspace` green; moss boots clean with pond inactive AND (if testable) active.

## Out of scope

Changing any route's path, handler, or behavior (pure reorganization — behavior diffs belong to other
prompts). The backup consolidation (12) even though its scheduler is now supervised. New tasks. Touching
rake.
