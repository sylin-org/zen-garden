# DX & Delight Research — the vocabulary, the tutorial gap, and the household register

**Status:** Research, DRAFT for argument — not law. External evidence gathered
2026-08-27 at the operator's request ("prior art investigation, user delight
research"), after the operator flagged a tension in review. This document
exists to be argued with; anything adopted amends the law it touches and then
this document records the disposition.

## The question

The operator-flagged tension (2026-08-27 session): the glossary-first idiom
raises the price of entry — a newcomer must absorb the constitution (stones,
moss, chirp/song, banks, rake) before the code reads like operator sentences —
and the process weight (amendments, witnesses, per-slice gates) assumes a
single disciplined operator. What does prior art and DX research say a project
like this should do?

## Prior-art findings

### F1 — Vocabulary walls form from untiered vocabulary, not from naming

The two canonical cautionary tales both kept their concepts and struggled on
entry:

- **Nix**: "steep due to the documentation and odd terminology. But after you
  get going it solves so many problems" — the community wrote its own
  [terminology primer](https://news.ycombinator.com/item?id=23377292) because
  the official docs did not tier the vocabulary. [Tweag's Nix Book
  report](https://tweag.io/blog/2022-09-29-the-nix-book-report/): "the
  learning curve is perceived as extremely steep." Community critique
  ["Nix is built for its own developers"](https://www.reddit.com/r/NixOS/comments/1fx1653/nix_is_built_for_its_own_developers/)
  targets the unwelcomed entry, not the model. Defenders' counterpoint: the
  same terminology creates a cohesive, precise domain language once learned —
  which is exactly R1.1's ubiquitous-language bet.
- **Urbit**: the extreme case — ships, galaxies, hoon, nock. HN: "in an
  attempt to seem more sophisticated they invented new words so that nobody
  could figure out what it was"
  ([thread](https://news.ycombinator.com/item?id=15299972)). Its own blog
  conceded the barrier ([Urbit for Normies](https://urbit.org/blog/urbit-for-normies));
  postmortems ([Compact](https://compactmag.com/article/the-rise-and-fall-of-urbit/))
  record adoption failure despite real technical ambition. Urbit's words
  cannot be inferred from anything; that is the property to avoid.
- **The counter-example is Git**: a ferociously obscure plumbing language
  (blobs, refspecs, reflogs) coexisting with total adoption, because everyday
  porcelain spoke plain and the deep vocabulary stayed opt-in. Tiered
  registers succeed; flat walls do not.

### F2 — Diátaxis: the project has explanation and reference, and no tutorial

The [Diátaxis framework](https://diataxis.fr/) (tutorials / how-to / reference
/ explanation; adopted by [Python](https://discuss.python.org/t/adopting-the-diataxis-framework-for-python-documentation/15072)
and [Canonical](https://ubuntu.com/blog/diataxis-a-new-foundation-for-canonical-documentation))
maps this repository cleanly: charter, lessons, and ADRs are *explanation*
(excellent); glossary and CODE-RULES are *reference*. There is no *tutorial* —
no artifact walks a newcomer through one successful contact. The ephemeral
version exists (continuation.md, delete-me-first by design); the stable
version does not. This is L7's law ("self-description must be true") with the
audience widened: the project describes itself truthfully to machines
(route-manifest gates) but not yet teachably to people.

### F3 — Delight research: time-to-first-success and error messages dominate

- **TTFS < ~3 minutes** is the benchmark onboarding metric; documentation that
  shortens it is the highest-leverage DX investment
  ([DX research](https://getdx.com/blog/developer-documentation/),
  [first-time DX slides](https://pt.slideshare.net/slideshow/improving-first-time-developer-experience-dx/287061214)).
- **Error messages are a CLI's primary delight surface** — a great error "can
  be more helpful to new developers than having an expert sitting next to
  them" ([Yoz Grahame](https://developerrelations.com/talks/your-error-messages-can-be-beautiful/));
  [clig.dev](https://clig.dev/) and
  [Thoughtworks' CLI guidelines](https://www.thoughtworks.com/en-us/insights/blog/engineering-effectiveness/elevate-developer-experiences-cli-design-guidelines)
  converge on the same: when there is no GUI, the error IS the guidance.
  **R3.3 already encodes this law** (what happened / what it means / what to
  try) — independently derived, empirically confirmed.
- Beloved CLIs (fzf, ripgrep, bat, zoxide) share traits, not whimsy: instant
  feedback, sensible defaults, fast time-to-value; plus "create a reaction for
  every action" ([Atlassian](https://medium.com/designing-atlassian/10-design-principles-for-delightful-clis-522f363bac87))
  — which B11's pulse/companions/heal-moments already aim at.

### F4 — The ubiquitous language was never meant for end users

R1.1 executes DDD's ubiquitous language faithfully — and Evans' concept scopes
the language to team + code within the bounded context; presentation layers
translate. The glossary is not the wrong move; the open question is only the
*boundaries of its jurisdiction*: where entry (newcomers, future sessions) and
exit (household surfaces) need a rendering, not the raw noun.

## What the project already does right (keep; cite as precedent)

- Moniker suppression is already "a rendering concern" (ADR-0003; chirp.rs).
- Connection strings already speak household: `zen-garden:mongodb/mydb`.
- R3.3 is the error-delight law; the route-manifest gate is the L7 gate.
- M5's own gate is the delight bar: "J1–J4 demonstrable to a non-technical
  household member."
- Metaphor-derivability is already high: most nouns (offerings, stone, moss,
  rake, chirp/song, replant, rest/wake, nourish) can be *guessed* from the
  garden metaphor — the property Urbit lacked.

## Proposal areas (menu of options — pending operator ruling)

### A. Audience registers — the porcelain/plumbing amendment

- **A-minimal**: one-paragraph R1.1 amendment: the glossary speaks moss;
  surfaces translate; the household register never reuses operator nouns
  (family surfaces say storage, never "banks"). ~15 min; prevents
  leak-by-drift.
- **A-structural**: glossary entries gain a declared household rendering,
  consumed by future portal/companion surfaces; fold the wiring into the J1
  slice (M5) when that surface exists.
- **A-defer**: record intent in DEBT.md against J1; no amendment now.

### B. The tutorial — one datagram's life

- **B-internal**: `docs/v1/orientation.md` — a stone boots, asks the room
  rich, answers populate the cache, a USB plug sings, a stone dies and its
  chirp expires. The S1→S7 arc as a ~2-page stable walk. Serves future-you,
  resumed sessions, and any future contributor.
- **B-public**: the short version (3 paragraphs) lives in the root README
  beside the e-waste story — front-door delight for the project's users.
- **B-both**: B-public abridged + B-internal full; written once, rendered
  twice (B1 rendering discipline applied to prose).

### C. Glossary metaphor glosses

- **C-code**: one-line metaphor gloss per glossary-crate noun doc comment
  ("bank — after *seed bank*: storage that outlives the season"). The lawful
  home per R1.1; rustdoc renders it. ~1 hour. Seals the weak nouns (bank,
  pond, Lantern) so the metaphor teaches the system.
- **C-docs**: a human-facing glossary page under docs/ with a metaphor column.
  A rendering — add when a human-facing glossary is wanted.

### D. How-to-work-here

- **D-memory**: a short "How to work here" section atop docs/MEMORY.md (the
  existing pointer index): ordered reading path, what a slice looks like
  (gates green at every commit), where law lives. Minutes; it is also the
  AI-session entry point, matching how work actually happens here.
- **D-contributing**: repo-root CONTRIBUTING.md — the conventional location
  for humans arriving from outside; justified only when the repo is pushed
  public (decision pending).

### E. (Note, not a decision) The S5 delight showcase

When S5's `/stone/{ref}` not-here answer lands, treat it as the delight
showcase per F3: the body teaches — names the stone that knows, offers the
`Location:` (ADR-0004 §4 already specifies the mechanics). No new scope; an
acceptance note on the S5 slice.

## Dispositions

- **2026-08-27, operator ruling:** A-minimal (R1.1 register amendment —
  landed); B-internal (`docs/v1/orientation.md` — landed); C-code (glossary
  metaphor glosses — landed); D-contributing (`CONTRIBUTING.md` with a
  **lightweight contributor process** — epic ceremony stays maintainer-side);
  E accepted, applies when S5 opens (the not-here answer becomes an
  acceptance criterion on the slice).
- **2026-08-27, operator agreement (settled):** the CLI keeps garden
  verbs; bridges in help/error text, never aliases. Codified as the R3.1
  registers amendment the same day.
- **Open ruling — the CLI register.** Operator proposal: detach rake onto
  technically standard terms (stop/deploy/remove) to minimize cognitive load
  for advanced operators; poetic semantic language reserved for UX. Session
  recommendation (awaiting ruling): **keep garden verbs in the CLI** — the
  verbs encode nonstandard lifecycle semantics (`rest` is a converged
  desired state the Converger maintains, not a momentary `stop`; `wake`
  resurrects from stored spec; `uproot` removes *and* forgets), so standard
  terms would import standard expectations and lie. A translating CLI also
  recreates the L12 synonym sin across CLI/wire/audit in one command's
  lifetime (rake renders moss's truth — L21 — verbs included). Serve the
  expert's first-contact cost with help-text bridges (rake already does
  this: "Rest a managed offering — stopped, and converge will keep it so")
  and R3.3 error cross-references — bridges, never aliases (R1.3). Note the
  proposal contains an inversion: the standard terms ARE the jargon (docker-
  distributed, but jargon); the garden verbs are plain English drawn from
  everyday life, which is why the household register (A-minimal) converges
  with them rather than away from them.

### Landing record

- R1.1 register amendment: CODE-RULES.md, 2026-08-27.
- `docs/v1/orientation.md`: 2026-08-27.
- Glossary metaphor glosses: `crates/glossary/src/lib.rs`, 2026-08-27.
- `CONTRIBUTING.md` (lightweight process): 2026-08-27.
