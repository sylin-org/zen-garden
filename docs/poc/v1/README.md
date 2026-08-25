# v1 Capability Inventory — PoC Codification

**Purpose:** The authoritative map of everything Zen Garden (PoC) does, how it does it,
and what becomes of it in the clean-code migration. This is the bridge document between
the proven PoC and the 1.0 rewrite: every capability is inventoried once, classified by
maturity, and given a verdict.

## Method

Four sources, cross-checked so nothing is invented twice:

| Source | Authority over |
|--------|---------------|
| Rake command manifest (`src/rake/src/command_manifest.rs`) | User-facing CLI surface |
| Moss route table + `/api/v1/manifest` | Machine-facing API surface |
| Code domains (aggregates, tasks, bootstrap, crates) | Mechanisms and behavior |
| Scripts & packaging (`installer/`, `dist.json`, deploy flow) | Supporting cast |

## Coverage rules

- Every command × route group × script × companion × orchestrator appears **exactly once**
- Every claim carries `file:line` evidence captured at survey time
- `maturity` is honest provenance, verified against the live fleet where possible:
  - `live-proven` — exercised and observed working on the real garden
  - `code-complete-unsoaked` — implemented, tested in CI or not, but never run in production
  - `scaffolded` — structure exists, behavior pending
  - `doc-fiction` — documented but not implemented (docs drift is a finding, not a shame)
- `v2_verdict` turns this inventory into the migration backlog: `keep | port | redesign | cut`

## Layout

- `inventory/*.yaml` — one file per domain, entries follow `_template.yaml`
- `lessons.md` — normative design constraints distilled from PoC experience;
  each phrased as a rule the greenfield must satisfy
- `CHARTER.md` — greenfield mission, pillars, architecture bets (B1–B11),
  ports/redesigns/cuts, migration story, milestone sequencing

## Status

**Survey COMPLETE (2026-08-24).** Seven domains inventoried from full-codebase passes,
cross-checked against live-fleet probes. See `inventory/` (7 files, ~85 capability entries)
and `lessons.md` (16 normative constraints). CHARTER.md pending — next artifact.
