---
audience: [developer, ai]
doc_type: reference
status: canonical
last_verified: 2026-04-12
---

# Scaffolding Tracker

**Purpose:** Every piece of intermediate-state code introduced during [ARCH-0017](decisions/ARCH-0017-ddd-monolith-epic.md) and [COMPANION-0001](decisions/COMPANION-0001-companion-integration-epic.md) is tracked here with an explicit removal trigger and action.
**Audience:** Developers adding or removing temporary scaffolds; reviewers verifying a book's cleanup is complete.

> **The scaffolding contract is part of the epic's shippability rule.** A scaffold whose removal trigger has landed but is still present in the tree is a bug, enforced by [scripts/check-scaffolding.sh](../scripts/check-scaffolding.sh). A scaffold that lives longer than its documented trigger is a bug. A `TODO: migrate later` comment with no entry here is a bug.

---

## Contents

- [The contract](#the-contract)
- [Entry schema](#entry-schema)
- [How enforcement works](#how-enforcement-works)
- [Adding a new scaffold](#adding-a-new-scaffold)
- [Removing a scaffold](#removing-a-scaffold)
- [Active scaffolds](#active-scaffolds)
- [Removed scaffolds](#removed-scaffolds)

---

## The contract

A **scaffold** is any code that:

- Exists only to keep the build green during an in-progress refactor, OR
- Preserves back-compat for a caller surface that will be migrated in a later book, OR
- Stubs out a dependency that does not yet exist, OR
- Is deliberately left in place with the knowledge that it will be deleted.

Every scaffold must be entered in this document **in the same commit** that introduces it. No exceptions. The rationale: scaffolds that are not tracked become forgotten, and forgotten scaffolds become the normal shape of the codebase.

A scaffold entry has:

- A **stable ID** (e.g., `arch-0016-active-guard`) used to reference it in code comments, commit messages, and the validator script.
- A **status**: `active` or `removed`.
- **What** the scaffold is, in prose.
- **Where** the scaffold lives (files, modules, call-site patterns).
- **Introduced in**: the commit or book that added it.
- **Removal trigger**: the book whose completion obsoletes the scaffold.
- **Removal action**: the concrete deletions needed.
- **Check commands**: grep patterns that must return zero matches when the scaffold is declared removed. [scripts/check-scaffolding.sh](../scripts/check-scaffolding.sh) runs these.

Intra-book scaffolds (introduced and removed within a single book) are **not** tracked here — they are ephemeral by definition and never leave the book's commit range. Only inter-book scaffolds belong in this document.

---

## Entry schema

Every entry uses the following fenced code block for machine-readable metadata, followed by a prose section.

````markdown
### <id>: <short title>

```yaml
id: <id>
status: active | removed
introduced_in: <book or commit>
removal_trigger: <book that obsoletes this>
removal_commit: <commit hash once removed>
check:
  - pattern: "<ripgrep pattern>"
    paths:
      - <path glob>
  - pattern: "<another ripgrep pattern>"
    paths:
      - <path glob>
```

**What:** <one-paragraph description>

**Where:**

- `<file:line>` — <what lives here>
- `<file:line>` — <what lives here>

**Removal action:**

1. <concrete step>
2. <concrete step>
3. <concrete step>
````

The `check` field is consumed by [scripts/check-scaffolding.sh](../scripts/check-scaffolding.sh). When an entry's `status` is `removed`, the script runs every `ripgrep` pattern against every listed path; any match fails the check.

When `status` is `active`, the script is informational — it logs that the scaffold exists and reports its removal trigger.

---

## How enforcement works

The validator at `scripts/check-scaffolding.sh`:

1. Parses this file, extracting every entry's YAML metadata.
2. For each entry with `status: removed`, runs its `check` commands. Any match is a hard failure.
3. For each entry with `status: active`, logs the scaffold ID and its removal trigger (informational).
4. Emits a summary: number of active scaffolds, number of removed, any failures.
5. Exits non-zero if any removed scaffold still matches its check patterns.

The script is intended to run in three contexts during the epic:

- **Locally**, on demand (`./scripts/check-scaffolding.sh`).
- **As a pre-commit hook**, if a developer opts in (see script header for installation).
- **In CI**, as part of Book XX (Epilogue) when GitHub Actions infrastructure is introduced. Until Book XX, enforcement is advisory.

---

## Adding a new scaffold

When a book introduces a scaffold, the commit that introduces it must also:

1. Add an entry under **Active scaffolds** below with `status: active`.
2. Reference the scaffold ID in any code comments that point at it: `// SCAFFOLD(arch-0016-active-guard): see docs/scaffolding.md`.
3. Document the removal action in enough detail that a future contributor (including a future AI agent with no context) can execute it mechanically.

Avoid scaffolds when possible. Prefer:

- Landing the full migration inside the book (even if the diff is large).
- Splitting the book into two if the migration cannot land atomically.
- Pulling a dependent book forward so the scaffold is never needed.

---

## Removing a scaffold

When a book's completion obsoletes a scaffold:

1. Execute every step in the scaffold's removal action.
2. Change the scaffold's `status` from `active` to `removed`.
3. Set `removal_commit` to the commit hash (filled in after the commit lands).
4. **Do not delete the entry** — it moves from **Active scaffolds** to **Removed scaffolds** below.
5. Run `./scripts/check-scaffolding.sh` locally; all `check` patterns must return zero matches.
6. Commit the removal as part of the triggering book's final chapter.

The permanent record of removed scaffolds is how the epic's postmortem (Book XX) measures that the cleanup actually happened.

---

## ID namespaces

Scaffold entries use a prefix that identifies the owning epic:

- `arch-*` — ARCH-0017 (moss DDD monolith epic)
- `companion-*` — COMPANION-0001 (companion integration platform epic)

New epics that produce scaffolds allocate a new prefix and document it here.

---

## Active scaffolds

*(Empty. All scaffolds have been removed.)*

COMPANION-0001 operates under the **break-and-rebuild** tenet (see [COMPANION-0001 §Tenets](decisions/COMPANION-0001-companion-integration-epic.md#the-tenets)). Long-lived coexistence scaffolding is not expected for companion-epic books; Book VIII replaces the firefly and cricket crates wholesale. Any `companion-*` scaffold that is introduced here is a signal that the book author could not find a break-and-rebuild path and should be scrutinized accordingly.

---

## Removed scaffolds

### arch-0016-active-guard: Offerings back-compat read shim

```yaml
id: arch-0016-active-guard
status: removed
introduced_in: ARCH-0016 (commit 426f020c)
removal_trigger: Book XVIII — Offerings Strangler Removal
removal_commit: 182a5a95
check:
  - pattern: "state\\.offerings\\.read\\(\\)"
    paths:
      - src/moss/src
  - pattern: "\\bActiveGuard\\b"
    paths:
      - src/moss/src
  - pattern: "\\bCandidatesGuard\\b"
    paths:
      - src/moss/src
  - pattern: "pub async fn read\\(&self\\) -> ActiveGuard"
    paths:
      - src/moss/src/domain/offerings
  - pattern: "pub async fn read_candidates\\(&self\\) -> CandidatesGuard"
    paths:
      - src/moss/src/domain/offerings
```

**What:** `Offerings::read()` and `Offerings::read_candidates()` returned guard types (`ActiveGuard`, `CandidatesGuard`) that deref'd to `&Vec<Offering>`. Kept the 82 `state.offerings.read().await.iter()...` call sites from [ARCH-0016](decisions/ARCH-0016-offerings-aggregate-domain.md) compiling unchanged during the migration period.

**Removal:** ARCH-0036 Book XVIII migrated all 81 remaining `.read().await` sites to typed aggregate queries (`snapshot`, `find_by_id`, `find_by_name`, `with_active`, `count_active`), deleted `guard.rs`, removed the `read()` and `read_candidates()` methods, and deleted 6 `AppState` delegate methods.

---

## Deferred renames — resolved

All deferred renames were resolved in Book XX (Epilogue) of ARCH-0017. No entries remain.

### deferred-registry-loader-task-rename: RESOLVED

**Resolution (Book XX):** Renamed in full. `RegistryLoaderTask` -> `OfferingsReconcilerTask`, task name `"registry-loader"` -> `"offerings-reconciler"`. All supervisor dependency strings updated (`catalog-builder`, `initial-service-sync`). The task name is informational on the `/api/v1/stone/tasks` endpoint — no external consumer matches on it by name (rake does not reference it). Safe to rename without wire-format concerns.

### deferred-placement-metrics: RESOLVED (closed without rename)

**Resolution (Book XX):** Closed without rename. `PlacementMetrics` is semantically accurate — the struct holds hardware resource *metrics* used for placement scoring (memory free/total, CPU load, storage free/total). The proposed rename to `PlacementResources` offers marginal clarity at the cost of a wire-format break across moss, rake (`PlacementRecommendation.metrics`), and the typed `StoneApi` client. The current name is acceptable and the rename is not warranted.

### deferred-job-offerings-field: RESOLVED

**Resolution (Book XX):** Rust field renamed from `offerings` to `targets` with `#[serde(rename = "offerings")]` to preserve wire compatibility. The JSON response still serializes as `{"offerings": [...]}` so rake and dashboard consumers are unaffected. The Rust-side name now accurately reflects the field's universal semantics (targets of a job — service names for install jobs, capability names for refresh jobs). All internal code references updated.

---

## References

- [ARCH-0017](decisions/ARCH-0017-ddd-monolith-epic.md) — the epic this tracker serves
- [ARCH-0016](decisions/ARCH-0016-offerings-aggregate-domain.md) — the refactor that introduced the first tracked scaffold
- [scripts/check-scaffolding.sh](../scripts/check-scaffolding.sh) — the validator script
- [domain-aggregates.md](specs/domain-aggregates.md) — the pattern spec (forbids untracked scaffolds in its anti-patterns section)
