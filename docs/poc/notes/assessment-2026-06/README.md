---
audience: [maintainer, contributor]
doc_type: assessment
status: current
date: 2026-06-11
---

> Source: staged multi-agent assessment (2026-06-11) — 11 parallel deep-readers across code/docs/git history,
> a 2026 self-hosting landscape research pass, 16 adversarial verifiers re-deriving every load-bearing claim
> against the code, and 4 dimension assessors. Every quantitative claim in these documents was independently
> verified with file:line evidence; verification corrections are already folded in.

# Zen Garden — Project Assessment, June 2026

A point-in-time assessment of pillars, philosophy, architecture, DX, UX, maturity, and strategy —
commissioned to answer: *where is this project in its maturity model, and what does maturation toward
"less but more meaningful parts" look like?*

## Documents

| File | Contents |
|---|---|
| [maturity.md](maturity.md) | Six-level maturity scale, per-dimension placement, honest status banner, critical path to "usable by strangers" |
| [architecture.md](architecture.md) | Verified codebase findings; target lean-core architecture; tiering (core / extension / park / delete); sequencing |
| [dx-ux.md](dx-ux.md) | Contributor and user journey audits; the "first 30 minutes" narrative; 15 quick wins; 6 structural fixes |
| [strategy.md](strategy.md) | 2026 landscape, positioning, 4 ranked opportunities, strategic sheds, sustainability risks, 12-month sequence |
| [shed-register.md](shed-register.md) | The consolidated, verification-backed register of everything to delete, park, consolidate, or decide |
| [landscape-data.md](landscape-data.md) | Appendix: the raw 2026 self-hosting landscape research (sources, GitHub data, prior art) |

## Executive summary

**Overall placement: L1 — feasibility prototype**, with L2–L3 internals in the core daemon, an L3
conceptual/vocabulary layer, and L0 release engineering, onboarding, and governance. The unevenness is
the signature of what this project has been: a 4.5-month, single-author, AI-amplified feasibility sprint
(1,272 commits, 84 active days, ~477k Rust lines added / ~190k deleted, zero tags, zero releases, zero CI).

The feasibility question is answered — and the answer is yes. A 12-stone heterogeneous garden (including a
LineageOS Android phone) runs daily: mDNS discovery, offering lifecycle, pond mTLS, MongoDB replica-set
choreography, VRAM-aware model routing, fleet self-update with rollback. The philosophy corpus is unusually
crisp and *already contains the maturation criteria* ("add features when real users ask," the one-sentence
test, the don't-add table in `docs/philosophy/staying-focused.md`). The codebase stopped applying those
gates around February 2026; applying them retroactively **is** the maturation plan.

### Five headline findings

1. **The front door is fiction; everything behind it is real.** The README quickstart invokes a Docker
   image (`zen-garden/stone:latest`) and env var (`ANNOUNCE_SERVICE`) that exist nowhere else in the repo;
   the headline `zen-garden:mongodb/mydb` URI has a parser consumed only by its own test corpus; there are
   zero GitHub releases so both install scripts fail for everyone; `first-stone.md` documents commands that
   never shipped. Meanwhile the actual product — the discovery cascade, tending cache, `observe`, the
   five-fix error messages — verifiably delights against a real garden. The gap between narrative (written
   at L3) and distribution (L0) is the single most damaging property for the stated first-time-builder audience.

2. **Roughly 65–80k lines don't pull their weight.** Verified: ~2.8k uncompiled/orphaned lines
   (`infra/cloud_filter/` + `cloud_drive.rs`), ~3k zero-consumer modules in garden-common (jobs/, events/,
   uri/, stone.rs, GardenHttpClient, errors.rs), 1.3k stub orchestrators + 1.36k unused cluster primitives,
   a dead `tests/` directory, two coexisting AI-orchestrator generations (57k lines; the dormant one is 42k),
   three backup generations (~7.4k lines, three route families, two capture engines), and ~13.6k moss-only
   lines mislabeled "common." None of this is speculative — every item was adversarially re-verified.

3. **Quality is two-tiered: disciplined inner loop, absent outer loop.** 2,483 unit tests, an unwrap-free
   domain layer (verified in the heaviest files), typed errors, a sealed Docker boundary, a DAG-validated
   task supervisor — and not one test has ever run automatically. No CI has ever existed; the only scripted
   `cargo test` is hardcoded off (`installer/build.ps1:184` passes `-SkipTests`); a clean single-repo clone
   cannot `cargo check` (path deps on sibling `../koi` — though all five koi crates are published on
   crates.io, so the fix is small).

4. **Security posture contradicts the audience promise.** With pond inactive, port 7185 serves
   unauthenticated stone reboot/shutdown, offering deletion, and storage writes to anyone on the LAN — and
   `POST /api/v1/stone/deploy` (root code-push) is in *both* route sets, so it is unauthenticated even with
   pond active. Plus `changeme` default passphrase and stone/stone + NOPASSWD sudo baked into images. All
   fixable with plumbing that already exists; disqualifying for a data-sovereignty audience if shipped as-is.

5. **The strategic position is real, validated, and time-boxed.** Independent landscape research confirmed
   four white-space gaps that intersect exactly here: scavenged-hardware fleets as product (empty; the
   Android stone is ahead of anything shipping), LAN-native service identity (empty; Tailscale Services
   GA proves demand at the wrong layer), autonomous choreography without Kubernetes (vacated by Nomad's
   BSL/IBM orphaning; management UIs crowd the adjacent space), and VRAM-aware fleet AI placement (GPUStack
   is the lone credible rival, enterprise-weight). The dated hook: Windows 10 consumer ESU ends
   **October 13, 2026** — ~400M capable laptops exit support within months. CasaOS, the obvious destination
   for that audience, has been dormant since August 2025. The window is open and closing (Uncloud went
   0→5.2k stars in ~18 months on an adjacent thesis).

### Strengths to build on (verified)

- **The vocabulary system** — Stone/Moss/Rake/Lantern/Pond stable since commit one; zen verbs are real
  commands; the metaphor demonstrably drove design. The most production-ready layer of the project.
- **The offerings lifecycle** — embedded manifests with overlay, `zen-offering-*` namespace discipline,
  3-phase ceremony with journaled rollback. The core value, battle-tested on real stones.
- **The bollard seal** (ARCH-0030) — zero Docker-client references outside `docker/`. The strongest
  boundary in the codebase.
- **Rake's skeleton** — manifest-driven CLI generation, 4-level connection cascade with provenance,
  `Resilient` retry, best-in-class discovery-failure error message.
- **The task supervisor** — DAG validation, panic capture, per-task tokens (needs to become the *only* path).
- **mongodb's check()/reconcile() single-authority design** — the template for any future orchestrator.
- **The honest decision culture** — dissolution ADRs, reverted-attempt post-mortems, self-skipping
  integration tests, 46%-AI-coauthorship disclosure, "the spec describes what survived contact with reality."
- **Incident→fix velocity** — three disk-full incidents (May 27–29) became a capacity-governor ADR and
  implementation within 48 hours.

### The critical path (condensed)

Nothing on this list is new engineering — it is publishing, wiring, gating, and subtracting what exists:

1. Make a clean clone compile (switch koi path deps to the published crates.io versions; patch locally for dev).
2. Merge the June branch (46 commits of the most production-relevant work) into dev; push; prune 13 stale branches.
3. Stand up minimal CI (`cargo check` + `test` + `deny`), plus a lane for the orchestrator crates.
4. Cut a tag-driven v0.x release — this single step makes both install scripts functional.
5. Close the unauthenticated-LAN holes (deploy endpoint first).
6. Make the front door true (README quickstart, first-stone.md, the 25 broken help examples).
7. Subtract per the [shed-register](shed-register.md); decide ai-vs-ollama succession; park pavilion.

Full sequencing with effort/risk in [architecture.md](architecture.md) §4; the 12-month strategic version
in [strategy.md](strategy.md).

## Executing this assessment

The maturation plan is operationalized as a stash of self-contained agentic work orders at
[`.agentic/prompts/`](../../../.agentic/prompts/README.md) — 16 ordered prompts (gate → subtract →
harden/truth → structure → product), each embedding the verified ground truth above, re-verification
commands, target shapes, and definitions of done, written to be executable by lesser models in fresh
sessions.
