# 08 — Docs & ADR Corpus Hygiene

> The decision corpus tells the truth: statuses match reality, the index matches the files, proposals are
> triaged, the AI-bootstrap docs stop misleading, and the glossary serves its two audiences separately.
> Phase: Truth. Depends on: 04 (generation decisions are made — you record them, not make them).

## Mission

The project's documentation *governance* is excellent (DOCUMENTATION.md voice rules, ADR conventions) but
its *bookkeeping* lags ~2x behind the corpus: 182 ADR files where the index claims 96, implemented ADRs
still marked "proposed", 8+ unpropagated supersessions, 40 untriaged proposals inflating apparent scope,
and the `.agentic/` bootstrap docs — the files every AI session reads first — contain verified falsehoods
that corrupt future sessions. This is a librarian session: high volume, low risk, mechanical rules,
no code changes.

## Ground truth (verified 2026-06-11 — re-verify each)

| Fact | Re-verify |
|---|---|
| 182 ADR files; `docs/decisions/README.md` index claims "Total ADR files: 96", "Last Updated: 2026-03-22" | `ls docs/decisions/*.md \| wc -l; grep -n "Total ADR" docs/decisions/README.md` |
| Implemented-but-"proposed": ORCH-0039 (snapshots — implemented in `src/moss/src/domain/snapshot*`), STORAGE-0020 (capacity governor — implemented), ORCH-0013/0028/0029/0030 (resolved by prompt 04's ORCH-0042 — verify it exists) | `grep -ln "proposed" docs/decisions/ORCH-0039* docs/decisions/STORAGE-0020*` |
| 35 ADR prefixes incl. 12 singletons; one duplicate ID (COMM-0001 used twice); one unnumbered refactoring plan (`REFACTORING-PLAN-chirp-protocol.md`) | `ls docs/decisions \| sed 's/-[0-9].*//' \| sort -u \| wc -l` |
| 20 ARCH-0017 "epic book" ADRs (ARCH-0018..0037) are same-day completion records of a finished 2-day refactor | `ls docs/decisions/ARCH-00{18..37}* 2>/dev/null \| wc -l` |
| `docs/proposals/`: 40 entries, mostly imported at repo birth (aws-bridge, federation-bridges, patent-analysis…), 17 already duplicated in `docs/archive/proposals/` | `ls docs/proposals \| wc -l` |
| `.agentic/reference/utilities.md` documents `GardenHttpClient` (deleted in prompt 03) and locates TUI primitives at `common/src/ui/rendering.rs` (actual: `src/rake/src/ui/rendering.rs`) | `grep -n "GardenHttpClient\|ui/rendering" .agentic/reference/utilities.md` |
| `.agentic/CONTEXT.md` says `cargo test --package moss`; the package name is `garden-moss` (also re-check its module map post-prompt-04) | `grep -n "cargo test" .agentic/CONTEXT.md` |
| Glossary: user vocabulary up front, lines ~188-401 are a DDD contributor lexicon; contradicts itself on Lantern's port (~line 38 vs ~419) | `grep -n "7186\|port" docs/glossary.md \| head` |
| Philosophy facts drift: offering count 9 (curated-offerings.md) vs 31 (README — may be fixed by prompt 06) vs 51 (actual); a "State" pillar essay promised by three essays, never written; "weather vocabulary" listed as Implemented in joy-of-understanding.md but absent from src/; nine fictional "Workshop Panel" experts quoted in joy-in-infrastructure.md | `grep -rn "State" docs/philosophy/*.md \| grep -i "pillar\|essay" ; grep -n "Workshop Panel" docs/philosophy/joy-in-infrastructure.md` |
| `docs/guides/companion-overview.md` describes a planned port-7189 companion that shipped as firefly adapters | `grep -n "7189" docs/guides/companion-overview.md` |
| Spec staleness: `docs/specs/security.md` last_verified 2026-01-19 yet "canonical"; 18 specs unstamped | `grep -L "last_verified" docs/specs/*.md \| wc -l` |

## Research first (~45 min)

Read `docs/DOCUMENTATION.md` fully — it defines the rules you are enforcing (voice, taxonomy, what goes
to archive/). Read `docs/decisions/README.md` (the index you will replace). Skim 5 ADRs across ages
(COMM-0001 early, ORCH-0039 late) to calibrate status conventions.

## Plan gate — OPERATOR decisions

1. **Prefix consolidation**: present the mapping (12 singletons folded: DNS/MDNS/NET/DISC/TOPO/PRESENCE →
   COMM; DETECT/HOST/STATE → MOSS; STONE/PORT/GUIDANCE/METRICS → ARCH or MOSS) — renaming files breaks
   inbound links, so recommend: keep filenames, add a `domain:` frontmatter field instead. Confirm.
2. **ARCH-0017 books**: archive the 20 as a set (recommend) vs leave in place with a banner.
3. **Workshop Panel personas**: delete the quotes vs reframe as "perspectives" honestly authored.
   (Recommend delete — manufactured credibility in an honesty-differentiated project.)
4. Proposals triage list: you classify each of the 40 into promote/archive/delete; operator approves the
   *delete* sublist only.

## Target shape

Generated ADR index — replace the hand-maintained statistics in `docs/decisions/README.md` with a tiny
generator + its output (no build dependency; a script run by CI or by hand):

```bash
# scripts/gen-adr-index.sh — emits a table: ID | Title | Status | Domain | Date
# README.md keeps: naming convention, process, and the generated table between markers:
# <!-- adr-index:start --> ... <!-- adr-index:end -->
```

Status lifecycle note added to DOCUMENTATION.md (5 lines): `proposed → accepted → superseded(by) |
archived`; "accepted" is flipped when the implementing commit lands; living ADRs are labeled
`living: true` in frontmatter (pick the existing two: ARCH-0017, OFFER-0006).

Glossary split: `docs/glossary.md` keeps the garden vocabulary (~120 lines, the file a user opens);
`docs/reference/architecture-lexicon.md` receives the DDD back half with a header naming its audience.

## Implementation

1. **Status sweep** (biggest win first): walk all 182 ADRs; flip implemented-"proposed" to accepted
   (verify each against src/ with a quick grep — the two named above are confirmed; expect ~a dozen
   more); propagate `superseded-by:` onto OFFER-0003, ORCH-0001, COMPANION-0012/0015, ORCH-0028/0029
   partials and any ADR contradicted by a newer accepted one you encounter; fix the COMM-0001 duplicate
   ID (renumber the later one); give the chirp refactoring plan an ID or move it to archive/planning/.
2. **Index**: write the generator, regenerate, delete the stale hand stats.
3. **ARCH-0017 books**: per OPERATOR — move ARCH-0018..0037 to `docs/archive/implementation-reports/
   arch-0017-books/`, leave ARCH-0017 + postmortem + `docs/specs/domain-aggregates.md` as the living set;
   fix inbound links (`grep -rn "ARCH-002" docs --include="*.md" | grep -v archive`).
4. **Proposals triage**: classify 40 → promote (write the one-line ADR stub listing), archive
   (`docs/archive/proposals/`), delete (operator-approved). Delete the 17 duplicates outright.
5. **.agentic fixes**: utilities.md (remove deleted items per prompt 03's FINDINGS.md, fix the TUI path,
   re-verify every table against src/ — this file must be 100% true, it bootstraps every AI session);
   CONTEXT.md (`garden-moss` package name, module map already fixed in 04 — verify).
6. **Glossary split** + Lantern port fix (7186 — verify against `common/src/constants`).
7. **Philosophy pass** (lightest touch — voice is the maintainer's): fix the three factual drifts
   (offering count → "50+", cascade description → point to COMM-0004, scale claim); add the missing
   frontmatter; delete or reframe Workshop Panel per OPERATOR; for the missing "State" essay, remove the
   three dangling references (writing the essay is the maintainer's voice, not yours) and FINDINGS.md it;
   joy-of-understanding.md: move "Weather vocabulary" from Implemented to a "designed, not yet built"
   list (or note prompt 16 if the operator wants it built).
8. **companion-overview.md**: correct to shipped reality (firefly adapters).
9. **Spec stamps**: add `last_verified:` frontmatter to the 18 unstamped specs — set it ONLY where you
   actually verified the spec against code this session; others get `status: unverified` honestly.
10. Commits by step: `docs(adr): status + supersession sweep`, `docs(adr): generated index`, etc.

## Definition of done

- [ ] `bash scripts/gen-adr-index.sh` output matches the committed index; file count in index == `ls | wc -l`.
- [ ] Zero implemented-but-proposed among ADRs you checked; every superseded ADR carries `superseded-by:`.
- [ ] `docs/proposals/` ≤ a handful of genuinely-open items; triage table in the session report.
- [ ] `.agentic/reference/utilities.md` spot-audit: pick 10 random rows, verify each against src/ — 10/10
      (paste the audit).
- [ ] Glossary ≤ ~150 lines, garden-words only; lexicon file exists; Lantern port consistent.
- [ ] `grep -rn "Workshop Panel" docs/` per OPERATOR decision; no dangling "State essay" references.
- [ ] No code changed: `git diff --stat -- src/` is empty.

## Out of scope

README/first-stone/troubleshooting (prompt 06 owns the front door). Writing new philosophy. Renaming ADR
files. CHANGELOG restructuring. Anything in src/.
