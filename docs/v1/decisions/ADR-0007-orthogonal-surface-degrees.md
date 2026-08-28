# ADR-0007 — Surface degrees: encoding, projection, extraction are orthogonal

**Status:** Accepted · 2026-08-28
**Supersedes:** nothing whole; **sharpens** the "three-degree machine output"
note (`--json` / `--field` / `--format uri`) into a structural law
**Retires on implementation:** the per-verb `if cli.json` branches in rake
(~15 copies), the ad-hoc `--format` growth, the absence of a
machine-readable rake command catalog
**Referenced by:** CODE-RULES R4.8 (the coding standard), L9 (manifest-driven
surfaces), L7 (self-description is true), L21 (rake is a thin client)

## Provenance

Raised by the operator mid-session (2026-08-28), while watching the watch
slice land: *"avoid bespoke processors per commands."* The smell was correct
and the codebase half-knew it. The moss had solved this class of problem at
birth — the `Face` manifest declares every surface once; help, routing, and
advertisement are generated from it (L9), and an unadvertised emission is
structurally impossible. The rake side had the *flags* of a solution (`--json`
and `--field` are global) without the *structure*: every verb hand-rolled the
same `if cli.json { return emit_output(..) }` branch (~15 copies and growing
with each slice), `--format` grew per-verb where a particular agent wanted it,
and help existed only in human clap rendering — an agent could discover a moss
with one `GET /api/v1` but could not discover rake at all.

The PoC had reached toward this too: its rake carried a `command_manifest`
module (`commands/…/watch.rs` imports it), uncompleted as a law. Its tuition
adds the warning that justifies the structural form: the PoC's moss advertised
27 paths of which 7 were ghosts and ~195 routed pairs were unadvertised —
bespoke per-command anything drifts from its own description.

## Decision

A surface answer has three **orthogonal degrees**, never to be conflated and
never to be decided per command:

1. **Encoding** — *how the answer is rendered*: `human` (default) or `json`.
   One policy, applied at ONE dispatch point. Flag `--output json|human`
   (alias `--json`, the shipped shorthand); env `RAKE_OUTPUT` (rake's
   established env prefix, matching `RAKE_STONE`/`RAKE_DISCOVERY_PORT`).
2. **Projection** — *which view of the answer*: `default`, `uri`, …
   Verb-declared where a view earns its keep (`--format uri` on observe,
   list, find). Projections compose with encoding: a `uri` projection under
   `--output json` is a JSON array of URIs.
3. **Extraction** — *which slice of the answer*: `--field dot.notation`,
   implies encoding=json, and composes with both above.

The verbs compute and return **answers** (the envelope value plus, where one
exists, a human projection); the dispatcher — one place — applies encoding,
then projection, then extraction. A verb never asks "am I json?".

And the mirror of the moss's front door: **the CLI is self-describing in
machine form.** `rake manifest` (alias of `rake help --json`) walks the clap
command tree and emits every verb, argument, type, and help text as JSON —
generated, never hand-maintained, so an agent discovers rake the same way it
discovers a moss. Help text lives exactly once, in doc comments; drift
between help and behavior is structurally impossible for the same reason the
moss cannot emit an unadvertised route.

## Law encoded

CODE-RULES **R4.8** — the coding standard this ADR carries: verbs return
answers; encoding, projection, and extraction are applied at the dispatch
point; per-verb rendering branches are bespoke processors and do not get
written; the CLI self-describes in machine form.

## Alternatives considered

- **Per-verb `--json` flags with hand-rolled branches** (today's shape) —
  rejected: the branch is copied per verb, forgotten by new verbs, and the
  storage files slice shipped without `--format` as fresh proof.
- **A responses-as env var only, no flag** — rejected: environments set the
  default, but humans need per-invocation override without mutating their
  shell (`gh`/kubectl both carry the flag alongside the env).
- **`zen-garden-responses-as` naming** — rejected: `RAKE_` is this CLI's
  established env prefix and "output" is the standard term the ecosystem
  already reads (gh, kubectl, docker).
- **Generated wrapper scripts / separate `rake-json` binary** — rejected:
  two front doors is the PoC's two-router drift with new syntax.

## Consequences

- Rake verbs shed their `if cli.json` branches; the human renderers remain as
  pure projections the dispatcher selects. Net: ~15 bespoke branches deleted,
  one dispatcher added.
- New verbs get encoding, extraction, and catalog presence for free — a slice
  cannot forget them, which is the point (the storage files verbs shipped
  without `--format`, and each hand-added `--format` since has been a fresh
  reminder).
- `RAKE_OUTPUT` makes MCP/tool harnesses and CI set the policy once, per
  environment, instead of threading a flag through every call.
- The catalog's truth is clap's truth: a flag renamed in code renames in the
  manifest. L7 applied to the CLI.
- Nothing here touches the wire contract (R0.5): these degrees are rake's
  rendering law. The moss's HTTP faces already live it via the Face manifest
  and their `?format`-style parameters where they exist.
- Implementation lands as a queued slice under the Slice Mandate (the PoC's
  `command_manifest` module is the gate-2 artifact), retiring the per-verb
  branches it replaces.
