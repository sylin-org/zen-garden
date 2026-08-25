---
audience: [contributor, ai]
doc_type: reference
status: current
last_verified: 2026-02-07
---

# Documentation Guidelines

How to write, name, and organize Zen Garden documentation.

---

## The Litmus Test

> *If I deleted all the ADRs tomorrow, would every guide, spec, and reference still make complete sense?*

If yes, the separation is clean. If a guide says "we moved from X to Y" and you can't understand it without historical context, that sentence belongs in an ADR, not the guide.

---

## Voice Rules

Every document type has one voice. A document never mixes voices.

| Type | Voice | Tense | Writes like | Example |
|------|-------|-------|-------------|---------|
| **Guide** | Instructional | Present | "Here's how to do X" | "To configure discovery, set `DISCOVERY_PORT`..." |
| **Spec** | Declarative | Present | "The system does X" | "Discovery uses multicast on 239.255.42.99" |
| **Reference** | Factual | Present | Tables, lists, lookup | Port table, configuration options |
| **ADR** | Historical | Past | "We decided X because Y" | "We adopted multicast because broadcast failed on multi-homed hosts" |
| **Journey** | Narrative | Second person | "You type... you see..." | "You hear a chime. Something changed." |
| **Philosophy** | Reflective | Present, timeless | "We value X over Y" | "We prioritize comprehensibility over completeness" |
| **Proposal** | Speculative | Future | "This proposes X" | "We could add ceremonies for coordinated updates" |
| **Changelog** | Terse | Past | "What shipped" | "Added multicast discovery transport" |

### Red-flag phrases

These should **never** appear in guides, specs, or reference docs:

- "What Changed", "Before / After", "Original behavior / New behavior"
- "We decided", "We switched", "We migrated from"
- "Status: COMPLETE", embedded timelines, commit-by-commit diffs
- "Previously", "In the old system", "This replaces"

If you need to explain *why* something works a certain way, link to the ADR:

```markdown
> Design rationale: [COMM-0004](decisions/COMM-0004-multicast-first-discovery.md)
```

---

## Naming Conventions

Names are the first DX touchpoint. A developer should know what a file contains before opening it.

### General rules

- **Default**: `lowercase-kebab-case.md`
- **UPPERCASE only for**: `README.md`, `CHANGELOG.md`, `CONTRIBUTING.md`
- **Never in filenames**: status words (`COMPLETE`, `ACTIVE`, `TODO`), phase markers (`PHASE-2`), implementation stages

### Per directory

| Directory | Pattern | Examples |
|-----------|---------|----------|
| `decisions/` | `PREFIX-NNNN-slug.md` | `COMM-0001-p2p-transport-singleton.md` |
| `specs/` | `PREFIX-NNNN-slug.md` or `slug.md` | `STORAGE-0001-seed-bank-onboarding.md`, `discovery.md` |
| `guides/` | `slug.md` | `first-stone.md`, `companion-development.md` |
| `reference/` | `slug.md` | `ports.md`, `connection-strings.md` |
| `journeys/` | `NN-slug.md` | `01-the-first-stone.md` |
| `philosophy/` | `slug.md` | `humanist-infrastructure.md` |
| `proposals/` | `slug.md` | `ceremonies.md`, `nourishment.md` |

### ADR prefixes

`API-`, `BUILD-`, `COMM-`, `COMPAT-`, `CRICKET-`, `FIREFLY-`, `GUIDANCE-`, `LANTERN-`, `MDNS-`, `METRICS-`, `MOSS-`, `OFFER-`, `POND-`, `PORTRAIT-`, `PRESENCE-`, `RAKE-`, `SECURITY-`, `STATE-`, `STORAGE-`, `TOPO-`

Numbers are unique within prefix. Leading zeros: `0001`, `0010`.

---

## Directory Taxonomy

| Directory | Contains | Does NOT contain |
|-----------|----------|------------------|
| `guides/` | Step-by-step operator instructions | Decision rationale, implementation history |
| `specs/` | How the system works now (protocols, APIs, data formats) | Why it was designed this way (that's an ADR) |
| `reference/` | Quick-lookup tables and facts | Prose explanations (that's a guide) |
| `decisions/` | Architecture Decision Records (immutable after acceptance) | Current-state documentation |
| `journeys/` | Narrative stories teaching through experience | Dry technical reference |
| `philosophy/` | Design principles and values | Implementation details |
| `proposals/` | Design documents for future work | Implementation status reports |
| `security/` | Security model, threats, hardening guides | General operational procedures |
| `ops/` | Operational guides for maintainers (build, release, maintenance) | Analysis documents, planning artifacts |
| `archive/` | Historical artifacts preserved for context | Anything actively referenced |

### What goes to `archive/`

- Implementation reports (completed work summaries)
- Planning artifacts (refactoring analyses, migration plans)
- Superseded documents (replaced by newer versions)
- Completed proposals (after ADR extraction)

Archive subdirectories:
```
archive/
├── implementation-reports/   # Post-completion summaries
├── planning/                 # Analysis and migration plans
├── proposals/                # Implemented proposals (original text)
└── superseded/               # Docs replaced by newer versions
```

### What does NOT belong in `docs/` root

Only these files live in `docs/` root:
- `README.md` (navigation hub)
- `CHANGELOG.md` (what shipped)
- `DOCUMENTATION.md` (this file)
- `glossary.md` (terminology)
- `introduction.md` (project overview)

Everything else belongs in a subdirectory.

---

## Frontmatter

Required on all documents except `README.md` files.

```yaml
---
audience: [operator, developer, contributor, ai]
doc_type: guide | spec | reference | decision | journey | philosophy | proposal
status: current | draft | superseded | archived
last_verified: YYYY-MM-DD
---
```

- **audience**: Who reads this. Use multiple values if appropriate.
- **doc_type**: Must match the directory the file is in.
- **status**: `current` for living docs, `draft` for work in progress, `superseded` for replaced docs (link to replacement), `archived` for historical.
- **last_verified**: Date someone confirmed the content is still accurate.

---

## Templates

Templates live in `docs/templates/`. Copy the appropriate one when creating a new document.

- [guide-template.md](templates/guide-template.md)
- [spec-template.md](templates/spec-template.md)
- [proposal-template.md](templates/proposal-template.md)
- ADR template: see [decisions/README.md](decisions/README.md)

---

## Changelog Rules

`docs/CHANGELOG.md` is a **concise index of what shipped**, not a place for implementation narratives.

**Good entry** (3-5 lines):
```markdown
## 2026-01-26
- **Multicast-first discovery** — Primary transport now uses IPv4 multicast (239.255.42.99:7184) with directed broadcast fallback. Solves multi-homed Windows discovery failures.
  See: [COMM-0004](decisions/COMM-0004-multicast-first-discovery.md) | [Discovery Transport spec](specs/discovery-transport.md)
```

**Bad entry** (50+ lines of implementation details, before/after comparisons, file-by-file diffs): move the details to an ADR or spec and link to it.

### When to add an entry

- New features, breaking changes, architectural refactorings, user-visible bug fixes

### When to skip

- Typo fixes, formatting, internal refactoring, test-only changes

---

## Writing Checklist

Before submitting documentation:

- [ ] Voice matches document type (present-state for guides/specs, historical for ADRs)
- [ ] No red-flag phrases in guides/specs/reference
- [ ] Filename follows naming conventions for its directory
- [ ] File is in the correct directory
- [ ] Frontmatter present and accurate
- [ ] All internal links work
- [ ] If referencing *why* a design choice was made, links to ADR instead of inlining the rationale
- [ ] Examples and configuration values match current codebase

---

## Converting Existing Documents

When you find a document that mixes voices:

1. **Identify the current-state content** — configuration, behavior, how-to. This stays (or moves to the right directory).
2. **Identify the decision narrative** — "we changed", "we decided", rationale. Extract to a new ADR in `decisions/`.
3. **Identify implementation history** — "What Changed" sections, before/after, timelines. Archive to `archive/implementation-reports/`.
4. **Rewrite what remains** in the correct voice for its document type.
5. **Link** the rewritten doc to the ADR for readers who want historical context.

---

## Related

- [ADR process and template](decisions/README.md)
- [Journey writing guide](journeys/WRITING-GUIDE.md)
- [Agentic context](.agentic/CONTEXT.md) (AI assistant rules)
