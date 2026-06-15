# Maturation Prompt Stash

Self-contained prompts for agentic coding sessions that execute the
[June 2026 assessment](../../docs/notes/assessment-2026-06/README.md) maturation plan. Each prompt is a
complete work order: mission, verified ground truth, research steps, plan gate, target shapes, and a
definition of done. They are written for **lesser models** — explicit scope, embedded facts, hard
guardrails, and verification commands with expected outputs.

## How to use

1. Start a fresh session in the repo root.
2. Paste [`_preamble.md`](_preamble.md), then the chosen prompt file, as the opening instruction.
3. One prompt per session. Do not combine prompts; do not let a session continue past its definition of done.
4. The agent must complete the prompt's **Re-verify** section before changing anything — the facts were
   verified 2026-06-11 and the tree moves.
5. Track execution in [`PROGRESS.md`](PROGRESS.md): set your row to `in-progress` when you start, then
   `done`/`blocked`/`postponed` with linked commits when you stop. Record contradictions in its divergence
   log and OPERATOR gates in its operator-decisions log.

## Posture (applies to every prompt)

- **Greenfield rules.** Zero external users. No compatibility shims, no deprecation cycles, no
  `#[deprecated]` bridges, no commented-out code. Break and rebuild cleanly; git history is the archive.
- **Subtract first.** When a prompt offers "fix" vs "delete," prefer delete. Less but more meaningful parts.
- **The standards are law.** `docs/code-standards.md` and `.agentic/CONTEXT.md` govern all Rust written.
  The philosophy gate applies to all new surface: if it doesn't serve "a small team running services on
  repurposed hardware," it doesn't go in.
- **Stop conditions.** If a Re-verify fact no longer holds, if work leaks outside the prompt's listed
  directories, or if a decision marked OPERATOR comes up — stop and ask, don't improvise.

## Order and composition

Recommended order. **Hard dependencies** are the arrows; prompts without an arrow between them can be
reordered if strategy demands (e.g. pull 15 forward before an October launch).

| # | Prompt | Phase | Moves maturity | Depends on |
|---|--------|-------|----------------|------------|
| 01 | [clean-clone-build](01-clean-clone-build.md) | Gate | CI/release L0→ready | — |
| 02 | [ci-and-first-release](02-ci-and-first-release.md) | Gate | CI/release L0→L2 | 01 |
| 03 | [dead-code-deletion](03-dead-code-deletion.md) | Subtract | Architecture L2 hygiene | 02 (CI safety net) |
| 04 | [generation-decisions](04-generation-decisions.md) | Subtract | Architecture L2→L3 path | 03 |
| 05 | [security-baseline](05-security-baseline.md) | Harden | Security L1→L2 | 02 |
| 06 | [front-door-truth](06-front-door-truth.md) | Truth | Docs accuracy L1→L2, onboarding | 02, 04, 05 |
| 07 | [rake-cli-contract](07-rake-cli-contract.md) | Truth | UX + automation contract | 02 |
| 08 | [docs-adr-hygiene](08-docs-adr-hygiene.md) | Truth | Docs accuracy, contributor trust | 04 |
| 09 | [probe-revival](09-probe-revival.md) | Verify | Testing L2→L3 | 02 |
| 10 | [common-split](10-common-split.md) | Structure | Architecture L2→L3 | 02, 03 |
| 11 | [supervision-and-router](11-supervision-and-router.md) | Structure | Ops hardening L2→L3 | 02, 09 |
| 12 | [backup-consolidation](12-backup-consolidation.md) | Structure | Architecture, ops | 09, 11 |
| 13 | [storage-demarcation](13-storage-demarcation.md) | Structure | Architecture, security surface | 09, 10, 11 |
| 14 | [uri-resolver](14-uri-resolver.md) | Product | Strategy opp. #3 (headline truth) | 06, 07 |
| 15 | [linux-arm-install](15-linux-arm-install.md) | Product | Onboarding L0→L2, strategy opp. #1 | 02 |
| 16 | [autonomy-showcase](16-autonomy-showcase.md) | Product | Strategy opp. #2 (the demo) | 09, 11, 12 |

```
01 → 02 → {03 → 04, 05, 07, 09, 15}
          04 → {06, 08}
          05 → 06
          09 → {11 → 12, 13}
          10 → 13
          {06, 07} → 14
          {09, 11, 12} → 16
```

## Why this order

It mirrors the assessment's critical path: **gate the tree** (a clean clone must build and CI must guard
every later change) → **subtract** (delete verified-dead weight and park dormant generations so every
later session reads a smaller, truer codebase) → **harden + tell the truth** (security defaults and the
documentation front door, so the project can be shown) → **structure** (the deep consolidations, protected
by CI and a revived probe) → **product** (the strategic bets: a true headline, an install path for the
audience, the demo nobody can copy).

## Authoring conventions (for adding prompts)

Every prompt follows the same skeleton — keep it:

```
# NN — Title
> Mission in one line. Phase. Dependencies.
## Mission            — what done looks like, in prose
## Ground truth       — verified facts with file:line, each with a Re-verify command
## Research first     — what to read before planning (bounded, ~30-60 min)
## Plan gate          — produce a plan; OPERATOR items that need a human decision
## Target shape       — concrete samples of the desired end state (code/CLI/YAML/output)
## Implementation     — ordered steps
## Definition of done — checklist with commands and expected outputs
## Out of scope       — explicit do-not-touch list
```
