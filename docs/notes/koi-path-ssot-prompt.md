# Koi-agent prompt — single source of truth for koi's data path

> Authored 2026-06-14 from a 4-agent map of koi's path-resolution surface (zen-garden side).
> Decision: **Option A** (koi owns a single machine-scoped data root; greenfield, no migration).
> Pass the block below to the koi agent. The zen-side follow-on is at the bottom.

---

```markdown
# Single source of truth for koi's data path (DDD value object, injected)

## Mission
koi's data directory is an AMBIENT GLOBAL re-resolved ad hoc: `koi_common::paths::koi_data_dir()`
(env `KOI_DATA_DIR` -> platform default) and its bridge `impl Default for CertmeshPaths` are called
from ~30 operational sites across koi-certmesh / koi-embedded / the koi binary. Each consumer
independently re-answers "where is the data dir?", so a host that configures a custom `data_dir`
(koi-embedded `Builder::data_dir`) gets a SPLIT: the certmesh core uses the injected dir for some
operations while ambient `CertmeshPaths::default()` calls silently use the platform default.

Establish a SINGLE SOURCE OF TRUTH: the data root is resolved ONCE at each composition root, wrapped
as the existing `CertmeshPaths` value object, OWNED by `CertmeshCore`, and INJECTED everywhere. No
operational code calls `CertmeshPaths::default()` / `koi_data_dir()`.

Phase 1 (this prompt): certmesh + the two composition roots + the paths-drop bug. Sibling domains
(proxy/dns/health) follow the same pattern in Phase 2 (see Out of scope).

## Ground truth (re-verify each; cite file:line)
- `CertmeshPaths` is ALREADY a value object (`certmesh_paths.rs`, `with_data_dir(PathBuf)`). The single
  ambient leaf is `impl Default for CertmeshPaths` -> `koi_data_dir()` (certmesh_paths.rs:108-114).
- `CertmeshCore` ALREADY owns `CertmeshState.paths: CertmeshPaths` (lib.rs:67), set by `*_with_paths`
  ctors — but has NO public accessor. Add `pub fn paths(&self) -> &CertmeshPaths`.
- No-paths ctors are the leak: `CertmeshCore::new()` (181), `locked()` (209), `uninitialized()` (234)
  each fall back to `CertmeshPaths::default()`.
- THE BUG: `init_certmesh_core` computes `paths` then DROPS it in the uninitialized early-returns —
  koi-embedded lib.rs:708,715 and koi binary main.rs:896,904,909 call no-paths `uninitialized()`/
  `locked()`. Fix: `*_with_paths(paths.clone())` on EVERY branch.
- Cert-write leak: `certfiles::write_cert_files()` uses `koi_certs_dir()`; reached operationally from
  `enrollment::process_enrollment` (135) and `lifecycle::renew_and_update_member` (120) though the
  calling core methods (`enroll`/`renew_all_due`) have `self.state.paths`. `write_cert_files_to(&Path)`
  already exists.
- Audit leak: `audit::append_entry()` uses `koi_log_dir()`; reached from `lifecycle.rs:134`.
  `append_entry_to(&Path)` exists; http.rs already uses `state.paths.audit_log_path()`.
- Status leak: `build_status()` (lib.rs:1598) resolves `default()` though both callers (facade
  `status()` + http handler) have `state.paths`.
- Bridge leak: `CertmeshBridge::active_members` (integrations.rs:30) + `CertmeshBridgeEmbedded::
  active_members` (koi-embedded:971) re-read `default().roster_path()` while holding an UNUSED
  `Arc<CertmeshCore>`.
- Renewal-loop leak: main.rs:1149 `ca_fingerprint_from_disk(&default())` with a live `cm` in scope.
- Ceremony leak: `pond_ceremony.rs:904 eval_unlock` reads `default().slot_table_path()`;
  `PondCeremonyRules` is a unit struct — thread paths via a field set at the `CeremonyHost::new` site.
- Dead statics: `save_auto_unlock_key` (732), `delete_auto_unlock_key` (759),
  `configure_auto_unlock_for_profile` (809) have ZERO in-repo callers. ⚠️ BUT the embedding host
  (zen-garden/moss) DOES call `configure_auto_unlock_for_profile` (zen pond.rs). So do NOT just delete
  it — make it a `&self` (or `&CertmeshPaths`) method that honors injected paths, and coordinate the
  signature with zen. The other two are safe to delete.
- Already correct (the model to follow): `ca::create_ca/load_ca/load_ca_with_master_key/
  ca_fingerprint_from_disk` take `&CertmeshPaths`; `read_auto_unlock_key(&paths)` / `try_auto_unlock(&self)`
  use injected paths; `save_auto_unlock_key_at(&paths)` exists.
- Test isolation to PRESERVE: `koi_common::test::ensure_data_dir()` (OnceLock + `KOI_DATA_DIR` set_var +
  `KOI_NO_CREDENTIAL_STORE`) and `paths.rs` `ENV_LOCK: Mutex`. Tests reach `default()` only via ~6
  helpers (`make_test_ca` x4 files, `make_unlocked_core`, `make_locked_core`) that first call
  `ensure_data_dir`. The injected pattern already exists at lib.rs:1672
  (`auto_unlock_key_round_trips_through_vault`, `with_data_dir(base.join(...))`) — the migration target.

## Plan gate (OPERATOR)
1. `impl Default for CertmeshPaths` disposition: gate behind `#[cfg(test)]` (keeps the ~6 test helpers
   one-line) vs delete (migrate helpers to `with_data_dir(ensure_data_dir(..))`). Recommend cfg(test).
2. Daemon composition root: the koi binary has NO `data_dir` field (cli.rs Config 522-542) — it resolves
   ambiently per command. Decide: add a resolved-once `data_dir`/`CertmeshPaths` to the daemon Config and
   thread it (proper, bigger) vs keep each CLI subcommand resolving `koi_data_dir()` once at its own
   process entry. `koi status`, `koi factory-reset`, `koi install` are SEPARATE processes that
   legitimately must find the machine default with no running core — those resolutions stay.
3. KEEP these legitimate per-process composition-root resolutions (NOT operational leaks):
   `factory_reset.rs:17`, `commands/status.rs:104`, `platform/windows.rs:48-50`,
   `koi-config/dirs.rs:30`. Rule = "resolve once per process root + reuse", not "inject a core that
   doesn't exist there".

## Target shape
- `CertmeshPaths` value object unchanged; `impl Default` -> `#[cfg(test)]` only.
- `CertmeshCore` owns `state.paths` (unchanged) + `pub fn paths(&self) -> &CertmeshPaths`.
- No-paths ctors deleted/`#[cfg(test)]`; `*_with_paths` become the only operational ctors.
- Each composition root resolves the root ONCE and injects:
  - koi-embedded: `init_certmesh_core` builds `paths` once (Some->with_data_dir, None->default = the ONE
    embedded default) and passes it to EVERY branch (fix 708/715).
  - koi binary: `init_certmesh_core(paths)` receives a resolved `CertmeshPaths`; 896/904/909 use
    `*_with_paths`.
- FS-touching free fns take `&CertmeshPaths` (or become `&self` core methods using `state.paths`):
  `certfiles::write_cert_files`, `audit::append_entry`, `build_status`, `eval_unlock`,
  `configure_auto_unlock_for_profile`. Fix call sites (enroll/renew/status/ceremony/renewal-loop/
  bridges) to pass `state.paths`/`core.paths()`.
- Guard against regression: `#[cfg(test)]`-gate `Default` + a clippy `disallowed_methods` entry for
  `koi_data_dir`/`CertmeshPaths::default` outside tests.

## Definition of done
- Zero operational `CertmeshPaths::default()` / `koi_data_dir()` in koi-certmesh + koi-embedded
  (only `#[cfg(test)]` and the per-process CLI composition roots).
- A test proves a CUSTOM data_dir is honored END-TO-END on a FRESH machine (uninitialized -> /create ->
  CA under the injected dir -> reopen reads it back). This is the regression the 708/715 bug fails.
- `cargo test` green across koi; `cargo clippy -D warnings` clean.
- The embedding host's `configure_auto_unlock_for_profile` call still compiles + works (coordinate the
  signature change with zen-garden).

## Out of scope (Phase 2)
Sibling domains (koi-proxy already models the target via `with_data_dir`/`*_with_override`; koi-dns
`state_path` override; koi-health `state.rs`/`log.rs` ambient) — same SSOT pattern, separate pass.
```

---

## Zen-side follow-on (Option A)

1. **Immediate (one-liner, optional):** drop `.data_dir(data_dir()/koi)` at `src/moss/src/bootstrap/run.rs:567`.
   koi then owns the location (its machine default), so moss's six `CertmeshPaths::default()` sites
   (pond.rs 748/1003/1026/1247, run.rs 634/1661, pond_lifecycle.rs:166) coincide with the core — the
   split is gone. Works once the koi auto-unlock fix is published.
2. **After the koi SSOT refactor lands** (`core.paths()` + `configure_auto_unlock_for_profile` becomes a
   method): replace moss's six `CertmeshPaths::default()` calls with access through the injected koi core
   (via `state.security` -> koi handle -> `core.paths()`), and switch the `configure_auto_unlock_for_profile`
   call to the new instance method. Then moss has ONE source (the injected core), not ambient `default()`.
