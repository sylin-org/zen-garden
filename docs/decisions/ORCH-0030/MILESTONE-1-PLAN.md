---
audience: developer
doc_type: plan
status: approved
---

# ORCH-0030 Milestone 1 — Implementation Plan

**Date**: 2026-04-08
**Status**: Approved
**Parent ADR**: [ORCH-0030](../ORCH-0030-orchestrator-architecture-realignment.md)
**Companion**: [ORCH-0030 R2](../ORCH-0030-orchestrator-architecture-realignment.md#revision-2--capability-events-and-adapter-owned-resolution)

This document is the contract for Milestone 1 of the ORCH-0030 R2
implementation. Every milestone listed below must land green —
`cargo check`, `cargo test --lib`, and the live integration test
suite all passing — before the next milestone starts. No
improvisation: if the implementation discovers a need to deviate
from this plan, the deviation must be documented as an addendum to
this file and re-approved before code lands.

---

## Why this plan exists

Earlier attempts to execute the R2 architecture switchover in a
single mega-commit failed: the change touches ~30 files and ~6000
lines simultaneously, and intermediate states do not compile. Each
attempt ended with a half-rewritten tree that had to be reverted to
the last green commit, losing the work.

This plan breaks the migration into 9 milestones (M0 through M8).
Every milestone is small enough to land in a single focused session
and ends in a green tree. M3 is the one unavoidable atomic commit
(the trait switch); the rest are additive or surgical.

---

## Scope split: Milestone 1 vs Milestone 2

ORCH-0030 R2 lists 13 adapters across the inventory. They split
cleanly into two waves:

### Milestone 1 — local-first + one cloud (9 adapters)

These adapters cover 9 of the 10 locked primitives and represent
the operator's primary workflow (a self-hosted garden with one
cloud chat fallback). They land first, in full, with all features
the ADR requires (capability events, adapter-local model resolution,
skill loading, provisioning, import pipeline for ComfyUI).

| Adapter        | Primitives                                                  | Form factor  |
|----------------|-------------------------------------------------------------|--------------|
| Ollama         | text.chat, text.embed, image.analyze                        | Local pool   |
| Gemini (Google)| text.chat, text.embed, image.analyze                        | Cloud        |
| ComfyUI        | image.generate, image.edit, image.upscale, image.analyze    | Local pool + skills |
| Docling        | image.analyze (via `ocr` skill)                             | Local pool   |
| LibreTranslate | text.translate                                              | Local pool   |
| WhisperCpp     | audio.transcribe                                            | Local pool   |
| Speaches       | audio.transcribe                                            | Local pool   |
| Kokoro         | audio.generate                                              | Local pool   |
| OpenedaiSpeech | audio.generate                                              | Local pool   |

**9 adapters total.** All 9 ship in M1 with their full ADR-aligned
implementations — no stubs, no skimping.

### Milestone 2 — additional cloud adapters (2 adapters)

These come later, after M1 patterns are proven and live-tested:

| Adapter   | Primitives                                                                                   | Form factor |
|-----------|----------------------------------------------------------------------------------------------|-------------|
| Anthropic | text.chat, image.analyze                                                                     | Cloud       |
| OpenAI    | text.chat, text.embed, image.analyze, image.generate, audio.generate, audio.transcribe      | Cloud       |

These are **deleted from the source tree in M1**, not stubbed.
M7 recreates each file from scratch using the M1 patterns
(`cloud_common::resolve_cloud_model`, capability event publishing,
etc.) as templates. This avoids stub-rot and forces M7 to write
the files honestly.

### Removed entirely

| Adapter | Reason |
|---------|--------|
| Infinity | `text.embed` is covered by Ollama/Gemini; `text.rerank` deferred (see below). |

**Infinity is deleted in M1 and never returns** unless a future ADR
amendment introduces a deliberate need for it.

### `text.rerank` decision

The locked Primitive enum includes `TextRerank`. With Infinity
deleted, no M1 adapter serves it. Two options were considered:

1. **Remove the enum variant + vocabulary entry now** (ADR amendment
   + locked-enum modification + test churn).
2. **Keep the variant as an unserved primitive** (the catalog
   renders `text.rerank` with zero providers; a request to
   `POST /v1/text/rerank` returns 503 `no_provider_for_primitive`).

**M1 chooses option 2.** The variant stays. M6 (the milestone-1
retrospective) revisits the decision once we have data on whether
anyone asks for it. If the answer is "no demand," M6 ships an ADR
amendment removing the variant, the vocabulary, and the canonical
keys (`text::QUERY`, `text::DOCUMENTS`, `text::RESULTS_TOP_K`,
`text::SEGMENTS`).

This deferral costs almost nothing (a single empty primitive entry
in the catalog) and gains us a real decision moment instead of a
premature deletion.

---

## Primitive coverage matrix (post-M1)

| Primitive          | M1 providers                            |
|--------------------|-----------------------------------------|
| text.chat          | Ollama, Gemini                          |
| text.embed         | Ollama, Gemini                          |
| text.translate     | LibreTranslate                          |
| text.rerank        | *(none — deferred to M6)*               |
| image.generate     | ComfyUI (via skills)                    |
| image.edit         | ComfyUI (via skills)                    |
| image.upscale      | ComfyUI (via skills)                    |
| image.analyze      | Ollama (vision), Gemini (vision), Docling (`ocr` skill) |
| audio.generate     | Kokoro, OpenedaiSpeech                  |
| audio.transcribe   | WhisperCpp, Speaches                    |

**9 of 10 primitives covered. 1 deliberately empty.**

---

## Files: keep, delete, add, rewrite

### Files **deleted** in M1

**Domain layer:**
- `src/domain/directory.rs` — the `Directory` aggregate
- `src/domain/recommendation_types.rs` — central recommendation value objects

**Services layer:**
- `src/services/recommendation.rs` — the central `RecommendationEngine`
- `src/services/directory_maintenance.rs` — Directory rebuild loop
- `src/services/skills/registry.rs` — the `Skills` aggregate (only this one file from `skills/`)

**HTTP layer:**
- `src/http/recommendations.rs` — `/v1/recommendations/*` handlers

**Provider layer:**
- `src/providers/anthropic.rs` — recreated from scratch in M7
- `src/providers/openai.rs` — recreated from scratch in M7
- `src/providers/infinity.rs` — gone permanently
- `src/providers/openai_compat_stt.rs` — helper, not an adapter; functionality folded into WhisperCpp + Speaches
- `src/providers/openai_compat_tts.rs` — helper, not an adapter; functionality folded into Kokoro + OpenedaiSpeech

### Files **added** in M1

**Services layer:**
- `src/services/provider_registry.rs` — process-internal `Arc<dyn Provider>` lookup

**Provider layer:**
- `src/providers/cloud_common.rs` — shared `CloudModel` + `resolve_cloud_model` helper for cloud adapters (Gemini in M1; Anthropic + OpenAI in M7)

### Files **retained** in M1 (no changes from `985b0c56`)

- `src/domain/capability_announcement.rs` — already has `CapabilityMediaInput` from commit 7c step 1
- `src/domain/events.rs` — the unified event bus
- `src/domain/resources.rs` — the Resources domain
- `src/services/directory_subscriber.rs` — the `CapabilityDirectory` + subscriber
- `src/services/instance_manager.rs` — `InstancePool` primitives
- `src/services/skills/{loader, cache, provisioner, queue, moss_volume, types, import/}` — every file except `registry.rs`
- `src/providers/ollama_matrix.rs` — already correct
- `src/http/{introspect, events, resources, ...}` — most HTTP modules unchanged

### Files **rewritten** in M1

**Domain layer:**
- `src/domain/provider.rs` — lean trait (`name + onboard + flush_caches`); delete `ProviderState`, `ProviderStatePublisher`, `Registration`, `RegistrationStrategy`, `HonoredField`, `MediaInputSpec`, `MediaOutputSpec`, `Model`, `ModelDescriptor`, `ProviderHealth`, `PerformanceHint`, `PerformanceVerdict`, `FieldRange`, `FieldConstraint`, `AutoKind`, `ParamOption`. Keep `ProviderError`, `ProviderOutcome`, `FlushReport`. Add `PinNotServable` variant.
- `src/domain/request.rs` — drop `ModelRef`, drop `resolved_model` from `OrchestratorRequest`, keep `resolved_provider`
- `src/domain/ids.rs` — drop `RegistrationId`, drop `ModelFqn`
- `src/domain/mod.rs` — drop `directory`, drop `recommendation_types`

**Services layer:**
- `src/services/contextualizer.rs` — consults `CapabilityDirectory` exclusively; delete `resolve_model` pass; delete `validate_provider_narrowing` pass; delete `RecommendationResolver` trait. Keep all alias/normalize/validate_input/extract_media helpers verbatim.
- `src/services/media_resolver.rs` — reads `media_inputs` from `CapabilityDirectory`
- `src/services/dispatcher.rs` — takes `Arc<ProviderRegistry>` + `Arc<CapabilityDirectory>`; drops `Directory`, drops `RecommendationEngine`, drops `DemandLedger`
- `src/services/catalog_builder.rs` — subscribes to `directory.provider.*` events on the bus; reads from `CapabilityDirectory`
- `src/services/skills/mod.rs` — drop `registry` from declarations

**HTTP layer:**
- `src/http/mod.rs` — drop `recommendations`
- `src/http/router.rs` — drop `/v1/recommendations/*` routes
- `src/http/skills.rs` — read from `CapabilityDirectory` filtered by `provider=comfyui`; the import endpoint dispatches to `ComfyUiProvider::import_from_url` via the `ProviderRegistry`
- `src/http/flush.rs` — query `ProviderRegistry::all()` instead of `Directory::providers`

**Provider layer (all 9 M1 adapters):**
- `src/providers/ollama.rs` — strip `state()`/`subscribe()` methods; matrix and selector unchanged
- `src/providers/google.rs` — full rewrite, uses `cloud_common::resolve_cloud_model`
- `src/providers/comfyui.rs` — owns its own `HashMap<Moniker, LoadedSkill>` (no `Skills` aggregate); imports happen through the existing `services::skills::import` pipeline; provisioning queue still wired
- `src/providers/docling.rs` — publishes `image.analyze` with the `ocr` skill as a `SkillDeclaration`
- `src/providers/libretranslate.rs` — full rewrite
- `src/providers/whispercpp.rs` — **full self-contained adapter** (~250 lines, no compat helper)
- `src/providers/speaches.rs` — **full self-contained adapter** (~250 lines, no compat helper)
- `src/providers/kokoro.rs` — **full self-contained adapter** (~250 lines, no compat helper)
- `src/providers/openedai_speech.rs` — **full self-contained adapter** (~250 lines, no compat helper)

**Wiring:**
- `src/app_state.rs` — drop `directory`, `recommendation`, `skills`, `provisioning`; add `capability_directory`, `provider_registry`. Keep `events`, `resources`, `media_store`, `job_store`, `idempotency_store`, `dispatcher`, `catalog`.
- `src/main.rs` — new construction order: events → capability_directory → directory_subscriber → provider_registry → adapters (each registers itself with both); drop directory_maintenance task; drop recommendation engine task

**Tests:**
- `tests/common/mod.rs` — `MockProvider` implements the new lean trait; fixture builds `CapabilityDirectory` + `ProviderRegistry` + `DirectorySubscriber`
- Every test file referencing deleted types (`Directory`, `Registration`, `ModelRef`, etc.) is updated to use the new shape, or deleted with a one-line rationale comment if its assertion no longer applies
- See M4 for new tests added in M1

---

## Milestone breakdown

Each milestone ends with: `cargo check` green + `cargo test --lib`
green + a single commit. Live integration tests are gated to M5
where Docker rebuild is part of the milestone.

### M0 — Plan document committed

**This file.** No code changes.

- Commit message: `docs(ORCH-0030): M0 — Milestone 1 implementation plan`
- Definition of done: this document is in tree, committed, reviewed.

### M1 — Add `ProviderRegistry` service (additive)

**Goal:** New service exists, has tests, is wired into `AppState`
and `main.rs`, but nothing reads from it yet. Tree compiles, all
existing tests still pass.

**Changes:**
- New file `src/services/provider_registry.rs` with the registry
  struct, `register`/`get`/`all`/`len`/`is_empty` methods, and 4
  unit tests (empty lookup, register+get, all-returns-everyone,
  duplicate overwrites).
- Add `pub mod provider_registry;` to `src/services/mod.rs`.
- Add `pub provider_registry: Arc<ProviderRegistry>` field to
  `AppState`.
- Construct `ProviderRegistry::new()` in `main.rs` and in
  `tests/common/mod.rs::fixture_with_provider`. Pass the empty
  registry through. **Nothing reads from it.**

**Test gate:**
- `cargo test --lib` green (existing 200+ tests pass + 4 new
  ProviderRegistry tests).
- `cargo check --tests` green.

**Commit:** `feat(ORCH-0030 M1): add ProviderRegistry service (additive)`

### M2 — ComfyUI exposes `SkillDeclaration`s via a new method (additive)

**Goal:** The ComfyUI adapter gains a method that produces
`Vec<SkillDeclaration>` from its current state (still backed by
the legacy `Skills` aggregate). The method is unused but compiles
and is unit-tested. This is preparation for M3 — when the trait
switches, M3 can wire ComfyUI's capability publication path with
no new risk.

**Changes:**
- Add `pub async fn skill_declarations(&self) -> Vec<SkillDeclaration>`
  to `ComfyUiProvider`. Implementation walks the existing `Skills`
  snapshot and constructs `SkillDeclaration`s with their parameter
  lists.
- Add 2 unit tests in the existing `comfyui` test module:
  - One with no skills loaded → empty vec
  - One with two synthetic skills loaded → two declarations with
    correct fields

**Test gate:**
- `cargo test --lib` green.
- The method is verifiably unused outside its tests (no other
  consumer in M2).

**Commit:** `feat(ORCH-0030 M2): ComfyUI exposes SkillDeclarations`

### M3 — Big-bang trait switch (the load-bearing milestone)

**Goal:** The architecture changes in one atomic commit. Before
this commit the legacy `Directory` is the source of truth; after
this commit the `CapabilityDirectory` is. Every adapter is on the
new lean `Provider` trait. All tests are updated to match.

**This is the only M1 milestone that cannot be split into smaller
commits because the trait change is atomic.** Internal
sub-checkpoints during development:

1. New lean `Provider` trait in `domain/provider.rs`; delete
   `ProviderState`, `Registration`, `HonoredField`,
   `MediaInputSpec`, `MediaOutputSpec`, `Model`, `ModelDescriptor`,
   `ProviderHealth`, `PerformanceHint`, `PerformanceVerdict`,
   `FieldRange`, `FieldConstraint`, `AutoKind`, `ParamOption`,
   `RegistrationStrategy`, `ProviderStatePublisher`. Add
   `PinNotServable` variant to `ProviderError`.
2. `domain/request.rs` — drop `ModelRef`, `resolved_model`.
3. `domain/ids.rs` — drop `RegistrationId`, `ModelFqn`.
4. `domain/mod.rs` — drop `directory`, `recommendation_types`.
5. Delete files: `domain/directory.rs`, `domain/recommendation_types.rs`,
   `services/recommendation.rs`, `services/directory_maintenance.rs`,
   `http/recommendations.rs`, `services/skills/registry.rs`.
6. Delete adapter files: `providers/anthropic.rs`,
   `providers/openai.rs`, `providers/infinity.rs`,
   `providers/openai_compat_stt.rs`, `providers/openai_compat_tts.rs`.
7. Add `providers/cloud_common.rs`.
8. Rewrite `services/contextualizer.rs` to consult
   `CapabilityDirectory` only.
9. Rewrite `services/media_resolver.rs` to read
   `CapabilityMediaInput` from `CapabilityDirectory`.
10. Rewrite `services/dispatcher.rs` to take
    `Arc<ProviderRegistry>` + `Arc<CapabilityDirectory>`.
11. Rewrite `services/catalog_builder.rs` to subscribe to
    `directory.provider.*` events.
12. Rewrite each of the 9 M1 adapters (Ollama needs only the
    `state()`/`subscribe()` strip; the other 8 are full rewrites).
13. Rewrite `services/skills/mod.rs` to drop `registry` from
    declarations.
14. Rewrite `app_state.rs` and `main.rs` for the new shape.
15. Rewrite `http/mod.rs`, `http/router.rs`, `http/skills.rs`,
    `http/flush.rs` for the new wiring.
16. Rewrite `tests/common/mod.rs` fixture.
17. Update or delete every test file referencing deleted types.
18. `cargo check` green.
19. `cargo test --lib` green.
20. `cargo test --tests` green.

**Test gate:**
- All compile-time errors resolved.
- All lib unit tests pass.
- All in-process integration tests (acceptance, integration,
  capability_introspect, etc.) pass.
- Live tests (`tests/live.rs`, `tests/parallel_smoke.rs`,
  `tests/skill_events.rs`) are deferred to M5 where Docker rebuild
  happens.

**Commit:** `refactor(ORCH-0030 M3): big-bang trait switch + 9 adapters migrated`

### M4 — Per-adapter capability + onboard tests

**Goal:** Each M1 adapter has dedicated unit tests proving:

1. **Capability publication.** Construct the adapter with synthetic
   config; verify the `CapabilityAnnouncement` it publishes contains
   the right primitives, the right `media_inputs`, and (for ComfyUI)
   the right `SkillDeclaration`s.
2. **Model resolution.** Verify `selectors.model` handling:
   - Missing → adapter picks its default
   - `recommended:*` → adapter picks an appropriate model
   - Concrete known model → adapter uses it
   - Concrete unknown model → `PinNotServable` error
3. **Onboard happy path.** Synthetic instance pool / cloud config;
   verify the adapter's request shape construction is correct
   (without actually hitting the network).

**Changes:**
- New file `tests/adapter_capability_publishing.rs` with 9 test
  modules (one per M1 adapter), ~5 tests each = ~45 tests.
- New file `tests/adapter_onboard_smoke.rs` with synthetic onboard
  paths for each adapter where onboard logic is non-trivial
  (Ollama, Gemini, ComfyUI, Docling, ~12 tests).

**Test gate:**
- ~57 new tests pass.
- Per-adapter coverage demonstrated.

**Commit:** `test(ORCH-0030 M4): per-adapter capability + onboard tests`

### M5 — Live integration tests against rebuilt Docker image

**Goal:** Docker image rebuilds from M3's source, runs against the
test garden, and serves real requests through every M1 adapter.
The existing live integration tests are updated for the new
dispatch path.

**Changes:**
- Rebuild `zen-garden-ai-orchestrator:test` Docker image.
- Update `tests/live.rs` to exercise each M1 adapter end-to-end
  through the HTTP surface.
- Update `tests/parallel_smoke.rs` for the new dispatch path.
- Update `tests/skill_events.rs` for the ComfyUI capability event
  publication.
- Update `tests/skills_nouns.rs` for the new noun surface backed by
  `CapabilityDirectory`.
- Update `tests/capability_introspect.rs` for the integrated path.

**Test gate:**
- Docker image builds clean.
- `AI_ORCH_TEST_URL=http://localhost:27190 cargo test --tests` passes
  every test.
- Manual smoke: `curl POST /v1/text/chat` against Ollama returns a
  real chat response from the test garden.

**Commit:** `test(ORCH-0030 M5): live integration tests + Docker validation`

### M6 — Documentation update + rerank decision

**Goal:** ORCH-0030 R2 gets a Milestone 1 retrospective section.
The skill subsystem spec and operator guide are brought current.
The `text.rerank` decision is taken on the basis of M1 → M5 data.

**Changes:**
- Append `R2.9 — Milestone 1 retrospective` to ORCH-0030,
  documenting:
  - What landed
  - Test counts
  - Performance observations from M5
  - Known limitations carried into M2
- Decision section on `text.rerank`:
  - **Option A**: keep the variant; document why; close the
    question
  - **Option B**: ADR amendment removing the variant; submit as
    a separate commit referencing this milestone
- Update `docs/specs/skill-subsystem.md` to reflect the new
  shape (no `Skills` aggregate; ComfyUI owns its skill registry).
- Update `docs/guides/operating-skills.md` to point at the new
  ComfyUI-internal flow.

**Test gate:**
- All docs render cleanly.
- Cross-references between ADR / spec / guide are consistent.
- The rerank decision is recorded with rationale.

**Commit:** `docs(ORCH-0030 M6): milestone 1 retrospective + rerank decision`

### M7 — Milestone 2: Anthropic + OpenAI cloud adapters

**Goal:** The two deferred cloud adapters land using the M1
patterns as templates.

**Changes:**
- New file `src/providers/anthropic.rs` (~450 lines) — uses
  `cloud_common::resolve_cloud_model`; supports `text.chat` and
  `image.analyze`.
- New file `src/providers/openai.rs` (~700 lines) — uses
  `cloud_common::resolve_cloud_model`; supports text.chat,
  text.embed, image.analyze, image.generate, audio.generate,
  audio.transcribe.
- Per-adapter capability + onboard unit tests (extensions to
  `tests/adapter_capability_publishing.rs` and
  `tests/adapter_onboard_smoke.rs`).
- Live tests against real Anthropic + OpenAI API keys (gated to
  the dev environment that has the keys).
- ADR retrospective entry `R2.10 — Milestone 2 retrospective`.

**Test gate:**
- All M1 tests still pass.
- New M2 tests pass.
- Live cloud tests pass against real APIs.

**Commit:** `feat(ORCH-0030 M7): milestone 2 cloud adapters`

### M8 — Final cleanup

**Goal:** No leftover M1 markers, no half-implemented features,
no stale documentation.

**Changes:**
- Grep for `// TODO milestone 2` and resolve every hit.
- Final pass on docs/decisions/ORCH-0030 — ensure R2.9 and R2.10
  retrospectives are referenced from the index.
- Update `docs/decisions/ORCH-0030/MILESTONE-1-PLAN.md` (this
  file) status to `archived` with a closing note pointing at the
  retrospectives.

**Test gate:**
- Full test suite green.
- No `TODO` / `FIXME` referencing milestones.

**Commit:** `chore(ORCH-0030 M8): final cleanup + milestone closure`

---

## Definition of done (per milestone)

A milestone is **done** when **all** of the following are true:

1. The source change matches what this plan describes for that
   milestone.
2. `cargo check` succeeds with zero errors and zero new warnings.
3. `cargo test --lib` passes every test (no `--ignored`, no
   `--filter`).
4. For M5 and M7: live integration tests pass against the rebuilt
   Docker image / real cloud APIs.
5. The commit message clearly identifies the milestone (e.g.
   `feat(ORCH-0030 M3): ...`).
6. The work is **not** combined with any other milestone or any
   unrelated change.
7. If the implementation discovered a need to deviate from this
   plan, the deviation is documented in this file as an addendum
   **before** the commit lands.

---

## Out of scope for Milestone 1

Anything not listed above. Specifically:

- Anthropic, OpenAI, Infinity, OpenedaiSpeech advanced features
  beyond the wire translation
- The reference garden test suite from ORCH-0030 §11 (lands
  alongside the relevant adapter milestones, not in M1)
- The Stone Resource Broker advanced features (commit 4 already
  shipped the basic version; no further work in M1)
- Soft claims for queued work (already in commit 4)
- The `/v1/do` flow composition (deferred per ORCH-0030 R2.5
  commit 11)
- Preferences as globals (deferred per ORCH-0030 R2.5 commit 12)
- Adapter-local benchmark systems (Ollama already has its
  capability matrix from commit 7a; no other adapter gets one
  in M1)
- Any vocabulary changes
- Any changes to the locked `Primitive` enum (subject to the
  `text.rerank` deferred decision in M6)

---

## How to use this document

1. **Before starting work on a milestone**: re-read the
   "Changes" section for that milestone. The list is exhaustive.
2. **During work on a milestone**: if anything outside the
   "Changes" list needs to change, stop, document the deviation
   as an addendum at the bottom of this file, get approval, then
   resume.
3. **At the end of a milestone**: verify the "Definition of done"
   checklist line by line. If any item is unchecked, the
   milestone is not done.
4. **When committing**: use the commit message template from the
   milestone definition. No improvisation.

---

## Addenda

*(Empty. Append `## Addendum N — date — title` blocks here when
plan deviations are approved.)*
