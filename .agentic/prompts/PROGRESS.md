# Maturation Prompt Stash — Progress Ledger

The live execution record for the 16-prompt maturation stash described in
[README.md](README.md). One prompt per session; this file is how sessions hand
off to each other.

**Agents:** update your row the moment you start (status `in-progress`, fill
`Date` + `Agent/model`) and again when you stop (`done`, `blocked`,
`postponed`, or `obsolete` with a one-line note). Keep `Notes` to one or two
lines; link commits by short SHA. If repo reality contradicts a prompt's
Ground truth, record it in the [Divergence log](#divergence-log) and stop —
do not improvise. When a prompt's OPERATOR gate fires, record the decision
request in [Operator decisions](#operator-decisions) and stop.

## Status legend

| Status | Meaning |
|---|---|
| `pending` | Not started. |
| `in-progress` | A session is actively working this prompt. |
| `done` | Definition of done met; commits linked. |
| `blocked` | Cannot proceed — a dependency, a failing Re-verify, or an external factor. Note why. |
| `postponed` | Started or scoped, then deliberately deferred (premise questioned, priorities shifted). Note why. |
| `obsolete` | Prompt no longer applies — the tree moved past it. Note what superseded it. |

**Pick the next prompt** by the dependency graph in [README.md](README.md#order-and-composition):
a prompt is eligible only when every prompt its arrow points from is `done`.
Within an eligible set, prefer the lower number unless strategy says otherwise.

## Ledger

| # | Prompt | Phase | Moves maturity | Status | Date | Agent/model | Commits | Notes |
|---|--------|-------|----------------|--------|------|-------------|---------|-------|
| 01 | [clean-clone-build](01-clean-clone-build.md) | Gate | CI/release L0→ready | postponed | 2026-06-14 | Claude Opus 4.8 | 048ce1bd, 01f9587d, cf90baaf, 8954558a, baee86a5, 327231b7, b9fa3417 | **PIVOTED to local-code (path deps).** First did the full crates path (koi 0.4 lean version-deps, registry-verified, TOTP UX fix — commits 048ce1bd…8954558a). Then koi did a breaking data-path SSOT refactor (verified done via 4-agent audit + koi clippy/tests green); maintainer chose to **consume koi from local `../koi` path deps** (dogfooding) until koi stabilizes rather than wait on publishing. So: koi-* → path deps (lean features kept); migrated 13 sites to the koi SSOT API (`core.paths()`, `&self` auto-unlock, `PondCeremonyRules::new`, dropped `delete_auto_unlock_key`); kept `.data_dir()` (cross-platform data location). `cargo check --workspace` + 947 moss tests green vs local koi (commits baee86a5/327231b7/b9fa3417). **Clean-clone-from-crates DEFERRED** (path deps need sibling `../koi`); switch-back procedure documented in `docs/guides/koi-dependency.md`. Status postponed = the prompt's crates goal is intentionally deferred until koi stabilizes. Also still open (orthogonal): `lantern/frontend/dist` clean-clone gap (Divergence row 3). Details: FINDINGS.md (2026-06-14 entries). |
| 02 | [ci-and-first-release](02-ci-and-first-release.md) | Gate | CI/release L0→L2 | postponed | 2026-06-15 | Claude Opus 4.8 | c4c0c90d, db20ed39, a392ff7a, 83235b89, b9f108f8 | **CI quality gate committed (dormant); DEPLOYMENT POSTPONED pending a stable koi version surface (operator decision 2026-06-15).** Committed: GIT_SHA in moss/rake `--version` (c4c0c90d); installer `-SkipTests` → parameter (db20ed39); `.github/workflows/ci.yml` = workspace gate (check / clippy@`-W` / test / cargo-deny, koi as sibling `../koi`) + 6-crate koi-free orchestrator matrix (83235b89); releasing.md guide + honesty fix (a392ff7a, b9f108f8). Gate verified green locally: `cargo check --workspace --all-targets` exit 0; `cargo test --workspace` 37/37 test binaries pass — lone fail is Windows-UAC `os error 740` on `coalescing_load_updates` (cannot occur on ubuntu CI; FINDINGS 2026-06-15). 30-agent adversarial review of the artifacts → fixes folded in (orchestrator cache scoping, version-format, doc honesty); the `libmongocrypt` "blocker" was disproved (`cargo check` never links; mongocrypt-sys ships pregen bindings). **POSTPONED (operator):** deployment work — `release.yml` (tag→cross-compiled binaries), enriched `version.json` (`koi_commit`), and pushing to wire CI — is on hold **until koi guarantees a version surface** (published semver crates or stable tags zen can depend on; switch documented in `docs/guides/koi-dependency.md`). Rationale: a release binary statically embeds koi, so reproducible releases need a stable koi version — unachievable while koi is pre-1.0 and dogfooded from a moving `../koi`. Kept (stands alone, committed): SHA-in-`--version` (c4c0c90d), `-SkipTests` param (db20ed39), and the `ci.yml` gate (83235b89) — dormant/unpushed, unproven on Actions, tracks the moving koi default branch. Release plan preserved in `docs/notes/ci-rescope-plan.md`; releasing.md states the deferral. **Required mid-prompt:** koi 0.4.2 certmesh-diet migration (`aec0f024`) to keep the workspace compiling — see Divergence 2026-06-15 / [[project_koi_dogfooding_dependency]]. **Resume trigger:** koi offers a version surface. |
| 03 | [dead-code-deletion](03-dead-code-deletion.md) | Subtract | Architecture L2 hygiene | pending | | | | |
| 04 | [generation-decisions](04-generation-decisions.md) | Subtract | Architecture L2→L3 path | pending | | | | |
| 05 | [security-baseline](05-security-baseline.md) | Harden | Security L1→L2 | pending | | | | |
| 06 | [front-door-truth](06-front-door-truth.md) | Truth | Docs accuracy L1→L2, onboarding | pending | | | | |
| 07 | [rake-cli-contract](07-rake-cli-contract.md) | Truth | UX + automation contract | pending | | | | |
| 08 | [docs-adr-hygiene](08-docs-adr-hygiene.md) | Truth | Docs accuracy, contributor trust | pending | | | | |
| 09 | [probe-revival](09-probe-revival.md) | Verify | Testing L2→L3 | pending | | | | |
| 10 | [common-split](10-common-split.md) | Structure | Architecture L2→L3 | pending | | | | |
| 11 | [supervision-and-router](11-supervision-and-router.md) | Structure | Ops hardening L2→L3 | pending | | | | |
| 12 | [backup-consolidation](12-backup-consolidation.md) | Structure | Architecture, ops | pending | | | | |
| 13 | [storage-demarcation](13-storage-demarcation.md) | Structure | Architecture, security surface | pending | | | | |
| 14 | [uri-resolver](14-uri-resolver.md) | Product | Strategy opp. #3 (headline truth) | pending | | | | |
| 15 | [linux-arm-install](15-linux-arm-install.md) | Product | Onboarding L0→L2, strategy opp. #1 | pending | | | | |
| 16 | [autonomy-showcase](16-autonomy-showcase.md) | Product | Strategy opp. #2 (the demo) | pending | | | | |

**Rollup:** 0/16 done · 0 in-progress · 14 pending · 2 postponed.
Update this line whenever a status changes.

## Divergence log

When a Re-verify fails or repo reality contradicts a prompt, record it here:
date, prompt #, what was found, what was done instead. This is the trail that
keeps later sessions from re-tripping the same surprise.

| Date | # | Finding | Action |
|---|---|---|---|
| 2026-06-13 | 01 | Local `../koi` = **0.3.0**, unpublished (koi `main` 7 commits ahead of origin, no `0.3.0` tag, "manual publish" CI not run). crates.io `koi-embedded` max = `0.2.202602151054`, **lagging** its siblings (`0.2.202603241449`). Committed `Cargo.lock` pins `koi-embedded 0.2.202603241449`, a version **never published**. No published koi set builds a clean clone: exact `=0.2.202603241449` fails resolution (embedded missing); caret `0.2` resolves but `koi-embedded 0.2.202602151054` fails to compile against `koi-certmesh 0.2.202603241449` (8 errors — `is_ca_initialized`/`roster_path`/`auth_path` gone, `load_ca` arity changed, `emit` now private). Also: koi 0.3.0 advanced shared deps — moss's `bollard 0.20` conflicts with koi-runtime's `bollard 0.21` (incompatible exact `bollard-stubs` pins). | Stopped at OPERATOR gate before any edit. All koi tests run in a throwaway clone; real tree untouched. **RESOLVED 2026-06-13**: maintainer published koi 0.3.0 (all 14 crates); converted koi path deps → `koi-* = "0.3"`; clean resolve + `cargo check --workspace` green. |
| 2026-06-13 | 01 | Prompt's local-override mechanism ("add `.cargo/config.toml` to `.gitignore`; commit `.cargo/config.toml.example`") is **invalid**: `.cargo/config.toml` is already git-tracked and holds build-critical config (incremental builds + per-target linker flags) explicitly "mounted into all Docker containers". Gitignoring it would un-track required shared config. | **RESOLVED 2026-06-13**: kept the committed `.cargo/config.toml` (build config) and added `include = [{ path = "config.local.toml", optional = true }]`; committed `.cargo/config.local.toml.example`; gitignored `.cargo/config.local.toml`. Verified: optional include skips silently when the file is absent (clean clone check green), and copying the example redirects all koi crates to `../koi` (cargo metadata: koi-embedded resolves to path, no registry source). |
| 2026-06-13 | 01 | Clean-clone build (`git clone .`, no `../koi`) with koi 0.3.0 + bollard 0.21 **resolves and compiles all koi-dependent crates**, but `cargo check --workspace` still fails on **garden-lantern**: `#[derive(RustEmbed)] folder 'src/lantern/frontend/dist/' does not exist`. That dir is a gitignored build artifact (`.gitignore:21`), absent in clean clones — a pre-existing clean-clone blocker independent of koi. | **RESOLVED 2026-06-15** (commit `35883baa`): `src/lantern/build.rs` now ensures `frontend/dist/` exists, writing a placeholder `index.html` when the real SPA is absent (real builds embed the real assets untouched). Verified: with `frontend/dist` moved aside, `cargo check -p garden-lantern` exits 0 via the placeholder; real-dist build is a no-op. This closes the koi-independent clean-clone blocker that gated prompt 02 (CI). |
| 2026-06-15 | 02 | Mid-prompt-02, `../koi` advanced (branch `dev`, 0.4.2) past the API zen's SSOT migration targeted → garden-moss failed to compile (22 errors). koi's **P08 "certmesh diet"** breaking changes: trust profiles flattened to two booleans (`TrustProfile` gone), **FIDO2 removed**, automatic failover shed (`ca_announcement` gone), `configure_auto_unlock_for_profile`→`configure_auto_unlock`, `open_enrollment` deadline-less, `CreateCaRequest`/`CertmeshStatus` reshaped. zen compiled vs koi tags v0.4.0/v0.4.1 but not `dev` (0.4.2). | Operator chose **migrate zen to current `../koi` (0.4.2)** + keep consuming `../koi` directly at whatever version (dogfooding, no pinning). **RESOLVED 2026-06-15** (commit `aec0f024`): migrated pond/security/discovery + rake + pond.html; `cargo check --workspace --all-targets` green (0 warnings); `cargo test --workspace` 37/37 binaries pass; rust-reviewer pass. Details + open koi-ref-for-CI question in FINDINGS.md. See [[project_koi_dogfooding_dependency]]. |

## Operator decisions

Prompts mark some choices **OPERATOR** — decisions only the maintainer can
make (publishing keys, succession between two subsystem generations, anything
destructive or strategic). When a prompt's OPERATOR gate fires, the session
stops and records the decision request here; the maintainer answers inline,
and a follow-up session resumes the prompt.

Known OPERATOR gates embedded in the stash (surfaced when the prompt is reached):

- **01 clean-clone-build** — if the local `../koi` checkout has unpublished
  changes the workspace depends on, the fix must start with publishing koi
  (maintainer-only). Present the version diff and stop.
- **04 generation-decisions** — succession between the two coexisting
  AI-orchestrator generations (which lives, which is parked) is a strategic
  call, not an automated deletion.

| Date | # | Decision requested | Resolution |
|---|---|---|---|
| 2026-06-13 | 01 | **Publish koi `0.3.0` to crates.io** (all 5 deps + their transitive koi crates, as one consistent set — the version the working tree already targets), then resume prompt 01 to pin `koi-* = "0.3"`. Maintainer-only: crates.io owner is `lbotinelly`; koi `main` is 7 commits ahead of origin and its publish is manual. Tested: **no** published koi set yields a buildable clean clone today, so this is a hard prerequisite, not a preference. (Non-recommended alt: pin to an old self-consistent 0.2.x set and revert zen-garden source off 0.3.0 koi APIs.) | **RESOLVED 2026-06-13**: maintainer published koi 0.3.0 to crates.io (all 14 crates incl. transitive). Prompt 01 resumed; zen-garden pinned to `koi-* = "0.3"` and bollard aligned to 0.21. |
