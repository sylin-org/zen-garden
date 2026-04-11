---
audience: [developer, ai]
doc_type: reference
status: canonical
last_verified: 2026-04-11
---

# Scaffolding Tracker

**Purpose:** Every piece of intermediate-state code introduced during [ARCH-0017](decisions/ARCH-0017-ddd-monolith-epic.md) is tracked here with an explicit removal trigger and action.
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

## Active scaffolds

### arch-0016-active-guard: Offerings back-compat read shim

```yaml
id: arch-0016-active-guard
status: active
introduced_in: ARCH-0016 (commit 426f020c)
removal_trigger: Book XVIII — Offerings Strangler Removal
removal_commit: ~
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

**What:** `Offerings::read()` and `Offerings::read_candidates()` return guard types (`ActiveGuard`, `CandidatesGuard`) that deref to `&Vec<Offering>`. This keeps the 82 `state.offerings.read().await.iter()...` call sites from [ARCH-0016](decisions/ARCH-0016-offerings-aggregate-domain.md) compiling unchanged. The guards are a strangler-vine — the aggregate's real read API (`snapshot`, `find_by_*`, `with_active`) is in place from day one, but callers migrate to it opportunistically as they are touched by other books. This pattern avoids a flag-day rewrite of 82 read sites in a single PR.

**Where:**

- `src/moss/src/domain/offerings/guard.rs` — the `ActiveGuard` and `CandidatesGuard` types with their `Deref<Target = Vec<Offering>>` impls
- `src/moss/src/domain/offerings/aggregate.rs` — the `Offerings::read()` and `Offerings::read_candidates()` methods
- ~82 call sites across `src/moss/src/` that use `state.offerings.read().await`
- `src/moss/src/app_state.rs` — the thin delegate methods `get_offerings`, `get_managed_offerings`, `get_adopted_offerings`, `get_borrowed_offerings`, `find_offering`, `find_offering_by_id` that wrap the aggregate

**Removal action:**

1. Migrate every remaining `state.offerings.read().await` call site to a typed query method: `snapshot().await` for full-clone callers, `find_by_id(id).await` / `find_by_name(name).await` for single-item lookups, `with_active(|o| ...).await` for scoped iteration.
2. Delete `src/moss/src/domain/offerings/guard.rs`.
3. Delete the `read()` and `read_candidates()` methods from `src/moss/src/domain/offerings/aggregate.rs`.
4. Delete the `pub mod guard;` line and the `pub use guard::{ActiveGuard, CandidatesGuard};` re-export from `src/moss/src/domain/offerings/mod.rs`.
5. Delete the `ActiveGuard`/`CandidatesGuard` re-export from `src/moss/src/domain/mod.rs`.
6. Delete `get_offerings`, `get_managed_offerings`, `get_adopted_offerings`, `get_borrowed_offerings`, `find_offering`, `find_offering_by_id` from `src/moss/src/app_state.rs`.
7. Run `./scripts/check-scaffolding.sh` — all patterns must return zero matches.
8. Change this entry's `status` to `removed` and set `removal_commit`.

---

## Removed scaffolds

*(Empty. Entries move here from Active scaffolds as their trigger books complete.)*

---

## Deferred renames — wire-format preserved

A third category exists for renames that **were not performed** during the epic because the affected surface is part of an external wire contract (JSON response body, UDP payload, SSE event shape) consumed by another crate or another process. Renaming these would break Rake, orchestrators, or external dashboards.

Each entry here is a **deliberate non-rename**. It is not a scaffold (no temporary code exists) and not a scaffold removal (nothing was deleted). It is a note that a better internal name was left alone, and a commitment to revisit when a coordinated API realignment can be done across moss, rake, and consumers.

**Rule:** entries here do not block the epic. They do not run through `check-scaffolding.sh`. They exist as a searchable, durable record that survives the epic and feeds into a future "API realignment" effort.

### deferred-placement-metrics: `PlacementMetrics` struct and `.metrics` field

```yaml
id: deferred-placement-metrics
kind: deferred-rename
introduced_in: ARCH-0018 Book I Chapter 2
revisit_when: Post-moss-epic API realignment
```

**What would have been renamed:**

- `moss::domain::placement::PlacementMetrics` (struct) → `PlacementResources`
- `moss::domain::placement::PlacementRecommendation.metrics` (field) → `resources`

**Why it was not renamed:**

Both symbols are part of the JSON response body of `POST /api/v1/offerings/place` (and equivalent placement endpoints). `garden-rake` has its own `PlacementRecommendation` struct at [src/rake/src/commands/offering/mod.rs:61](../src/rake/src/commands/offering/mod.rs) with a matching `metrics: PlacementMetrics` field that deserializes the response. Renaming moss's Rust field to `resources` would change the wire format from `{"metrics": {...}}` to `{"resources": {...}}` and break Rake deserialization.

Per [ARCH-0017](decisions/ARCH-0017-ddd-monolith-epic.md), external API contracts (REST endpoints, UDP wire format, JSON schemas) stay stable throughout the epic. Internal Rust APIs are rewritten freely, but the serialized names on public HTTP responses are not internal.

An alternative — `#[serde(rename = "metrics")]` on a renamed Rust field — is technically available. It was rejected for Chapter 2 because it introduces a mild Rust/JSON naming asymmetry that is only worth paying for as part of a coordinated cross-crate realignment, not inside a rename chapter of a rename book.

**What the revisit looks like:**

A future "API realignment" project (post-moss-epic) renames the wire shape in lockstep across moss, rake, the typed `StoneApi` client, any orchestrator consumers of the placement endpoint, and any external dashboards. That project:

1. Audits every consumer of `/api/v1/offerings/place` (and related endpoints).
2. Chooses a versioning strategy: either a new endpoint path, an `Accept: application/vnd.garden.v2+json` header, or a coordinated breaking change with a deprecation window.
3. Renames moss's Rust symbols (`PlacementMetrics` → `PlacementResources`, `.metrics` → `.resources`) and updates the serde-facing names in lockstep with every consumer.
4. Removes this entry from the deferred-renames section.

The revisit is **not in scope** for any ARCH-0017 book. It is a separate effort that depends on the moss refactor being complete (so it has a clean baseline to realign against).

**Other searchable markers:**

- `rg 'pub struct PlacementMetrics' src/moss/src/`
- `rg 'pub metrics: PlacementMetrics' src/moss/src/`

---

## References

- [ARCH-0017](decisions/ARCH-0017-ddd-monolith-epic.md) — the epic this tracker serves
- [ARCH-0016](decisions/ARCH-0016-offerings-aggregate-domain.md) — the refactor that introduced the first tracked scaffold
- [scripts/check-scaffolding.sh](../scripts/check-scaffolding.sh) — the validator script
- [domain-aggregates.md](specs/domain-aggregates.md) — the pattern spec (forbids untracked scaffolds in its anti-patterns section)
