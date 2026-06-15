---
audience: [maintainer, contributor]
doc_type: assessment
status: current
date: 2026-06-11
---

> Part of the [June 2026 project assessment](README.md). All evidence independently verified against code.

# Maturity Model Placement

## The scale

A pragmatic six-level scale for an open-source infrastructure project, defined by *who can safely depend on it*:

| Level | Name | Defining test |
|---|---|---|
| **L0** | Experiment | Exists to answer a question. No expectation anyone else can build or run it. |
| **L1** | Feasibility prototype | Works for the author, end to end, on the author's hardware. Verification is manual; the repo doubles as a lab notebook. |
| **L2** | Alpha / usable by friends | A motivated person with author access can build, install, and run it. Tests exist; defaults may be unsafe; docs partially true. |
| **L3** | Beta / usable by strangers | Clean clone builds; releases exist and the documented install path works unassisted; defaults are safe on a hostile LAN; docs validated against the shipped surface; CI gates regressions. |
| **L4** | Production-trusted | Versioned, signed, upgrade-and-rollback-tested releases; security model enforced by default; operational incidents are rare and post-mortemed; deprecation policy exists. |
| **L5** | Sustainable community project | Multiple maintainers, governance, contributor pipeline, release cadence that survives any single person. |

## Placement per dimension

Zen Garden is intensely uneven — some dimensions are two full levels ahead of others, which is itself the
signature of a 4.5-month, single-author, AI-amplified feasibility sprint (1,272 commits by one person,
84 active days, zero tags, zero CI).

| Dimension | Level | Evidence | Single gating item to next level |
|---|---|---|---|
| Conceptual / vocabulary design | **L3** | Stone/Moss/Rake/Lantern/Pond stable since commit one; zen verbs are real commands (`src/rake/src/command_manifest.rs`); metaphor drove actual renames. Frays at the edge: "nourishment" simultaneously deprecated (glossary) and shipped (`rake nourish`); "weather" UX claimed Implemented, absent from code | Vocabulary integrity sweep: finish nourishment→update across README/introduction/rake; implement or retract weather |
| Architecture | **L2** | Post-ARCH-0017 core is genuinely disciplined: 286-line thin `Moss` container, bollard sealed 100% inside `docker/`, DAG-validated task supervisor. But two full orchestrator generations coexist (57k lines; ai dormant operationally since Apr 12), three backup generations (~7.4k lines), 2,475 orphaned uncompiled lines (`infra/cloud_filter/`), and garden-common is "moss's second half" with ~13.6k verified moss-only lines | One implementation per concern — start with the ai-vs-ollama succession decision |
| Code quality | **L2** | Domain layer effectively unwrap-free (verified: heaviest files have zero production unwraps); typed errors; SAFETY comments. But api/ carries ~98 unwraps incl. 18 in `s3_gateway.rs` beside its own safe `build_response()` helper; garden-discovery's background Lantern path broken by construction (`src/discovery/src/lib.rs:96-101`); 3 unawaited `events.publish` calls shipping in ai's flow executor | Enforce the lint policy already written in the workspace `Cargo.toml` (`unwrap_used=warn` in api/, `-D warnings`) and fix what it flags |
| Testing | **L2** | 2,483 test functions; in-process axum harness; cross-language URI conformance corpus; a 9.6k-line dedicated probe crate. But ~4 integration files against 213 unique HTTP endpoints; zero tests in lantern/discovery/companion-usb; probe untouched since Mar 22; **no test has ever run automatically** | Run the existing suite in CI on every push (blocked by the koi dep, below) |
| CI / release engineering | **L0** | No workflow has ever existed (`git log --all -- .github/workflows` is empty); zero tags; zero GitHub releases; a clean single-repo clone cannot `cargo check` — five path deps on sibling `../koi` (`Cargo.toml:98-102`). Nuance: all five koi crates ARE published on crates.io, so this is a one-line-per-dep fix, not a vendoring project. Versions are wall-clock timestamps with no SHA | Switch koi deps to published versions (keep a `[patch]` for local dev) so a clean clone compiles — it blocks every other item in this row |
| Security posture | **L1** | Pond mTLS is real (crypto delegated to koi-certmesh); threat model honest and proportionate. But with pond inactive, :7185 serves unauthenticated reboot/shutdown/offering-delete; `POST /api/v1/stone/deploy` is registered in **both** route sets (`router.rs` configure and configure_public), so root code-push is unauthenticated even with pond active; `NoAuth` exists but is wired into zero middleware; stone/stone + NOPASSWD sudo baked into images; `changeme` default passphrase | Gate /deploy and mutating routes behind pond identity or a token |
| Docs accuracy | **L1** | Volume and governance are L4-grade (610 md files, 182 ADRs, DOCUMENTATION.md voice rules); accuracy is L1: README quickstart references a nonexistent image and env var; first-stone.md/troubleshooting.md use commands that never existed (15+ verified mismatches); ADR index claims 96 of 182 files; implemented ADRs still "proposed"; 25 of rake's 130 help examples fail against the actual parser | Regenerate the front door from the real command manifest, with a CI test that parses every manifest example |
| Onboarding reproducibility | **L0** | GitHub releases API returns `[]`; install.sh/install.ps1 fetch `releases/latest` → fail for everyone; public repo ~2 months behind the local tree (origin/dev pushed Apr 18); imaging is Windows-only; no Linux or Pi path despite docs recommending Pi hardware | Push current work + koi fix so `git clone && cargo build` succeeds for a stranger |
| Operational hardening | **L2** | Runs daily on a real 12-stone heterogeneous fleet incl. a rooted Android phone; incident→fix loop is fast (3 disk-full incidents May 27–29 → capacity governor ADR May 29, implemented). But the snapshot-runaway incident originated in a task that bypassed the supervisor (≥8 long-lived tasks run outside it; 77 raw `tokio::spawn` sites in moss), and fleet versions are untraceable to commits | Bring all spawns under the supervisor (the `fix/snapshot-scheduler-disposal` branch starts this) and bind versions to git SHAs |
| Community / governance readiness | **L0** | Single author, 100% of commits; dev frozen since May 10 with 46 commits of the most production-relevant work on a misnamed fix branch; 13 stale local branches; tests/README falsely claims GitHub Actions; no contributor can compile from one clone, let alone contribute | A public repo that is current, buildable, and honest about its own status |

## Overall placement

**L1 — feasibility prototype**, with L2–L3 internals in the core daemon and an L3 conceptual layer, dragged
down by L0 release engineering, onboarding, and governance. The project's own philosophy documents describe
this phase accurately ("the spec describes what survived contact with reality"); the README does not — its
product narrative is written at L3 while the distribution reality is L0, and that gap is the single most
damaging property for the stated audience of first-time builders.

A maintainer's honest status banner:

> **Status: feasibility prototype (pre-alpha).** Zen Garden runs daily on the maintainer's own 12-stone
> fleet, but there are no releases, no CI, and a clean clone does not yet build (sibling koi repo must be
> cloned alongside). Do not run it on a network you don't trust — several admin endpoints are
> unauthenticated until Pond security is enabled, and the deploy endpoint is unauthenticated regardless.
> The vocabulary and architecture are stable; the install story, docs, and security defaults are not yet
> truthful for anyone but the author.

## Critical path to L3 (usable by strangers)

Ordered; each item unblocks the next. Nothing on this list is new engineering — it is publishing, wiring,
gating, and subtracting what already exists.

1. **Make a clean clone compile.** Switch the five `koi-*` path deps (`Cargo.toml:98-102`) to their
   published crates.io versions, keeping a `[patch.crates-io]` section or workspace overlay for local
   sibling development. Every subsequent item — CI, contributors, releases — is hard-blocked behind this,
   and the fix is hours, not days.
2. **Make the public repo current.** Merge `fix/snapshot-scheduler-disposal` (46 commits incl.
   DEPLOY-0001/HOST-0001/STONE-0001) into dev, push, delete the 13 stale local branches. Today a stranger
   clones a two-month-old codebase the docs no longer describe.
3. **Stand up minimal CI.** One workflow: `cargo check --workspace` + `cargo test --workspace` +
   `cargo deny check`. The 2,483-test suite and deny.toml are already paid for; they have simply never run
   automatically. Add a lane for the orchestrator crates (currently outside all quality gates — that is
   where the unawaited-future bug ships).
4. **Cut a tag-driven release.** Reuse the existing containerized cross-compile pipeline; bind versions to
   git SHAs. This single step makes install.sh/install.ps1 functional and gives the Rust self-installer
   (the project's most product-shaped asset, BUILD-0003) a real artifact source — a working Linux path
   falls out for free.
5. **Close the unauthenticated-LAN holes.** Gate `/api/v1/stone/deploy` and mutating admin routes behind
   pond identity or a deploy token; remove `changeme`; reconcile the "SSH disabled by default" doc claim
   with the preseed that installs ssh-server with stone/stone + NOPASSWD sudo. Unauthenticated root
   code-push is disqualifying for a data-sovereignty audience.
6. **Make the front door true.** Rewrite or delete README's quickstart, first-stone.md, and
   troubleshooting.md against the actual command manifest; add the manifest-example parse test to CI so
   help text can never teach a rejected grammar again (25 of 130 examples currently fail).
7. **Subtract the misleading mass.** Delete the dead weight a stranger will trip on (see the
   [shed-register](shed-register.md)); decide the ai-vs-ollama succession; park pavilion and the stub
   orchestrators with explicit status notes. At L3, the repo a stranger reads must be the project that exists.

Items 1–4 move the project to a solid L2 in roughly the effort of one of its documented refactoring
"books"; items 5–7 are the L3 gate. The project has already demonstrated every capability this path
requires — incident-driven hardening (capacity governor), honest supersession (SECURITY-0004), and
measured subtraction (DNS-0002, the ARCH-0017 dissolutions). The critical path is not learning to do
these things; it is pointing them at distribution instead of features.
