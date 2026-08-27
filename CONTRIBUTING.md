# Contributing to Zen Garden

Zen Garden is self-hosted service orchestration on repurposed hardware —
services outlive machines. The mission and its evidence base live in
[`docs/v1/CHARTER.md`](docs/v1/CHARTER.md); the why is in
[the README](README.md). This file is the lightweight path for contributors:
everything required from a PR, and nothing that isn't.

## Read in this order

1. [`docs/v1/orientation.md`](docs/v1/orientation.md) — the whole system in
   one walk; start here
2. [`docs/MEMORY.md`](docs/MEMORY.md) — the pointer index; durable truth
   lives where it points
3. [`docs/v1/lessons.md`](docs/v1/lessons.md) — L1–L26, normative lessons
   distilled from the PoC (the "why" behind every rule)
4. [`docs/v1/CODE-RULES.md`](docs/v1/CODE-RULES.md) — the engineering law
5. [`src/v1/crates/glossary/`](src/v1/crates/glossary/) — the vocabulary;
   every garden word carries its standard-term gloss

## The lightweight path

A contribution is welcome when it is:

- **One task, one commit.** Commits follow existing style (`feat(v1): …`).
- **Green at every commit.** From `src/v1/`: `cargo test` and
  `cargo clippy --all-targets -- -D warnings` clean (R4.1). Tests run where
  CI runs (R4.4) — no test is optional because it is inconvenient.
- **Law-respecting.** A few rules carry most of the weight:
  - New domain nouns and verbs go in the **glossary crate first** (R1.1);
    records are paths — sections hold facts (R3.9).
  - No `TODO` — borrowings go in `src/v1/DEBT.md` with a named milestone
    (R4.2).
  - Errors answer three questions: what happened, what it means, what to try
    (R3.3).
  - Events inside, polling only at the edge (L18, R2.8).
- **Argued, not snuck.** Disagree with a rule? The escalation path is the
  same for everyone: argue in writing → amend the rule → then code. Never
  code past a rule silently. Open a PR description or issue with the
  argument; the operator rules, and the ruling is recorded.

**What you do NOT carry:** epic ceremony is maintainer-side. Contributors
write no ADRs, run no fleet witnesses, and manage no slice gates — a clean
PR with green gates and an honest description is the whole ticket. Anything
that needs an ADR amendment will be co-authored; bring the argument, not the
paperwork.

## Environment

- The live tree is `src/v1/` (Rust, workspace of five crates). The PoC under
  `src/poc/` is a frozen oracle — read it, never modify it.
- Per-crate checks are fast (`cargo test -p garden-contract`); run the full
  workspace before pushing.
- One moss per host while developing locally — a rebuilt binary cannot
  replace a running one on Windows (file lock).

## Where PRs go

Branch `dev` is the working trunk. Keep PRs single-purpose; if you touched
the wire shape, the fixtures in
`src/v1/crates/contract/tests/wire_fixtures.rs` are the contract — update
them and say why in the PR description.
