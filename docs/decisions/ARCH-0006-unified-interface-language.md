---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-03-17
---

# ARCH-0006: Unified Interface Language

**Date**: 2026-03-17
**Status**: Accepted
**Depends on**: None (standalone — touches rake CLI, moss API, and shared vocabulary)

## Context

Zen Garden communicates through two surfaces: garden-rake (CLI) and garden-moss
(HTTP API). Both evolved organically, producing three problems:

### 1. Dual CLI grammars

Rake supports two interchangeable syntaxes for every command:

- **Zen**: natural-language-inspired positional keywords (`find mongodb wishfully`,
  `offer mongodb somewhere`, `status mongodb on stone-01`)
- **Normative**: traditional CLI flags (`services find --name mongodb --wishful`,
  `offer mongodb --placement-mode interactive`, `services status mongodb --at stone-01`)

This requires a three-layer parsing pipeline: a pre-Clap style detector, a keyword
extractor, and a normalizer that converts zen tokens into Clap-compatible flags.
Every new command or flag must be declared in both grammars. A "no mixing" rule
rejects commands that combine zen keywords with normative flags, creating a third
failure mode users must learn.

The zen syntax was designed for approachability — letting non-CLI users interact
with the system in near-natural language. In practice it teaches a dialect nobody
else speaks, creates a ceiling when users outgrow the fixed keyword vocabulary, and
breaks the moment someone tries to speak actual natural English (the parser only
knows a finite set of keywords).

### 2. Inconsistent vocabulary between CLI and API

The same concept has different names depending on which surface you ask:

| Concept | CLI (zen) | CLI (normative) | API endpoint |
|---------|-----------|-----------------|--------------|
| Upgrade a service | `nourish` | `services upgrade` | `/services/{s}/nourish` |
| Backup management | `nurturing` | — | `/stone/nurturing/` |
| Backup artifact | — | — | "harvest" (ID field), "nurturing" (path) |
| Available updates | — | — | `/stone/nourishment` |
| Power off stone | `slumber` | `admin stone shutdown` | `/admin/stone/shutdown` |
| Reboot stone | `stir` | `admin stone reboot` | `/admin/stone/reboot` |

A developer or integration author must learn multiple names for the same operation.

### 3. 48 commands with unclear organization

Commands are a mix of top-level verbs (`rouse`, `slumber`, `stir`, `nourish`,
`adopted`, `borrowed`, `locate`, `presence`, `place`, `lift`, `invite`, `touch`)
and grouped subcommands (`pond init`, `storage add`). Some concepts have both a
top-level alias and a grouped form. The command surface is wide and inconsistent.

## Decision

**Unify the interface language across both surfaces.** One vocabulary, one CLI
grammar, one set of API naming conventions.

**Full design**: [../proposals/unified-interface-language.md](../proposals/unified-interface-language.md)

### CLI: one grammar

Remove the dual zen/normative syntax. Every command follows:

```
garden-rake <verb> [noun] [--flags]
```

- Delete the style detection layer, keyword extraction layer, and normalization layer.
- Delete all normative aliases (`services find`, `services status`, `adoption claim`, etc.).
- Parse with Clap directly. No pre-processing.

### CLI: domain verbs stay, zen vocabulary that obscures meaning goes

**Keep** verbs that are clearer than the generic alternative: `offer`, `rest`/`wake`,
`uproot`, `adopt`/`release`, `borrow`/`return`, `tend`, `observe`, `pulse`, `reconcile`.

**Retire** vocabulary that requires knowing the metaphor:

| Retired | Replacement | Reason |
|---------|-------------|--------|
| `nourish` | `upgrade` | Universally understood |
| `nourishment` | `update` | Standard term |
| `nurturing` / `harvest` | `snapshot` | Describes the artifact |
| `--wishful` | `--ensure` | Describes the action, not the feeling |
| `rouse` / `slumber` / `stir` | `stone wake` / `shutdown` / `reboot` | Standard verbs, grouped |
| `touch` | *(dropped)* | Unix collision |
| `place` / `lift` | *(dropped)* | One name per operation |
| `somewhere` keyword | `--placement-mode` flag | Standard flag |
| All zen positional keywords | Corresponding `--flags` | Unified grammar |
| All normative aliases | *(dropped)* | Domain verbs are canonical |

### CLI: regroup and consolidate

- **Group under `stone`**: `rouse`, `slumber`, `stir`, `make`, `take-root`, `reconcile`, `refresh`
- **Group under `backup`**: `nurturing` subcommands + standalone `restore`
- **Consolidate into `list`**: `adopted` → `--adopted`, `borrowed` → `--borrowed`, `locate strays` → `--strays`
- **Consolidate into `watch`**: `presence` → `watch --events presence`
- **Add aliases**: `logs <service>` (for `watch offering <name> logs`), `cap` (for `capabilities`), `explore` (for `offer`)
- **Extend `status`**: bare = stone detail, `status <service>` = service detail. Both include contextual command suggestions.
- **Absorb `nourish` into `upgrade`**: `upgrade --garden` for cross-stone scope.
- **Unify confirmation flags**: `--yes`/`-y` globally replaces per-command `--force`, `--auto-confirm`.

### API: align vocabulary

Rename API endpoints to match the unified vocabulary:

| Current | Proposed |
|---------|----------|
| `/services/{s}/nourish` | `/services/{s}/upgrade` |
| `/stone/nourishment` | `/stone/updates` |
| `/stone/nurturing/` | `/stone/snapshots/` |
| `/garden/nourishment` | `/garden/updates` |
| `/garden/storage/{name}/memories/` | `/garden/storage/{name}/snapshots/` |
| `POST /stone:upgrade` | `POST /stone/upgrade` (drop colon syntax) |

The `/stone` vs `/garden` scope split and domain action paths (`/rest`, `/wake`,
`/adopt`) are kept as-is — they're already good.

### Migration strategy

**Hard cut.** No deprecation shims, no backwards compatibility layer, no leftover
code. The three-layer parser is deleted entirely. Normative aliases are deleted.
Zen keywords are deleted. New versions are validated on the test garden.

The user base is home server operators, not enterprise CI pipelines. A clean break
now avoids carrying compatibility debt indefinitely.

### Three-tier `status` information architecture

| Command | Scope | Content |
|---------|-------|---------|
| `garden-rake` (bare) | Tended stone | Quick identity + command directory |
| `garden-rake status` | Tended stone | Full stone detail + contextual suggestions |
| `garden-rake status <service>` | Service | Full service detail + contextual suggestions |

## Consequences

### Positive

- **One vocabulary to learn.** Users, documentation, and integrations all use the
  same names for the same concepts.
- **One parser.** Clap only. No pre-processing layers. Every new command/flag is
  declared once.
- **Reduced command surface.** 48 commands consolidate to ~30 (including subcommands
  under `stone`, `backup`, `pond`, `storage`).
- **Transferable skills.** The `verb [noun] [--flags]` grammar matches every other
  CLI tool. Users learn patterns that work outside zen-garden.
- **Simpler code.** The command manifest no longer needs dual names, zen aliases,
  keyword specs, or normative mappings. `cli_build.rs` normalization and
  `parser.rs` style detection are deleted.

### Negative

- **Breaking change.** Every script or alias using normative syntax
  (`services find`, `adoption claim`) or zen keywords (`wishfully`, `somewhere`,
  `on stone-01`) breaks immediately.
- **Zen character is reduced.** The adverb keywords (`wishfully`, `quietly`,
  `somewhere`) gave the CLI a distinctive personality. That personality is traded
  for clarity and consistency.
- **API renames require client updates.** Any integration hitting `/nourishment`,
  `/nurturing`, or `/memories` endpoints must update paths.

### Files affected

- `src/rake/src/command_manifest.rs` — remove dual names, zen aliases, keyword specs
- `src/rake/src/cli_build.rs` — delete normalization layer
- `src/common/src/cli/parser.rs` — delete style detection
- `src/rake/src/route.rs` — update routing for consolidated commands
- `src/rake/src/commands/` — rename/move handlers for retired commands
- `src/moss/src/api/v1/` — rename endpoints (nourishment, nurturing, memories)
- `docs/reference/cli.md` — rewrite from unified-interface-language.md
- `docs/reference/api-endpoints.md` — update endpoint names
- `.agentic/reference/api-endpoints.md` — update endpoint names
