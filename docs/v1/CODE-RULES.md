# v1 Code Rules — the engineering law of src/v1

**Status:** Law · 2026-08-25. Subordinate to `lessons.md` and `CHARTER.md`.
Every rule exists because the PoC paid for it. When a rule blocks you, the
escalation path is: argue in writing → amend the rule → then code. Never code
past a rule silently.

---

## THE SLICE MANDATE — how every piece of work begins

*"Let's work on this" is a mandate to research first.* Before a line of
design or code, every slice — new feature, port, or a fix that touches a
surface — walks five gates, in order, and records the walk (a verdict sheet
in the slice's design note, PR, or commit message):

1. **Prior art.** How do other solutions tackle this? Name the standard, the
   successful product, the failed one. What do they promise the user, and
   what does the approach cost them?
2. **The PoC.** How did *we* already try this? Read the inventoried surfaces
   (`../inventory/poc-*.yaml`) and the PoC source itself, file:line. State
   the **objective** — the promise the feature made its user — separately
   from the mechanism. The PoC is a prior experiment, not a spec: its wins
   are evidence, its stubs are warnings, its fiction is a debt log.
3. **The house.** Why did we do it this way? Check the lessons, ADRs, DEBT,
   witnesses, and prior verdicts before overruling anything. A lesson is
   bypassed only in writing (header rule).
4. **Design.** Only now shape v1 — fulfill the *objective* with a leaner,
   streamlined architecture. DDD to the letter: aggregates own their state
   and invariants (R0.2), dependencies run one way (R0.3), and complexity
   lives at the seams — wire, media, device, the edge — never spread
   through the domain. Complexity that cannot find a seam must justify
   itself in the design note or be cut.
5. **Verdicts.** Every PoC element the slice touches is **brought**,
   **brought reshaped**, **deferred**, or **left dead** — with a name on
   the decision. Silently absent is a defect, not a decision.

The walk ends in a **proposal**: homework notes, the verdicts, and the
recommended design — presented for a nod before implementation begins.
Small fixes may walk the gates lightly, but the walk is never skipped.

Then witness: the objective from gate 2 becomes an assertion, tested in
deployment reality — including the mid-cycle joiner (a restart, a deploy, a
newcomer stone), because that is how features actually arrive in the field.

*(Born 2026-08-28: the storage data-plane slice was built stone-local,
dropped the PoC manifest's roles field, and missed the room's convergence
wound — all three recorded in the inventories and the bring-assessment,
none consulted before the code.)*

---

## P0 — Shape: a modular monolith

**R0.1** One deploy unit per stone. Processes exist only at *machine* boundaries:
companions, orchestrators, the koi sidecar (B7). Everything else is modules in
one binary. No internal HTTP between modules — ever.

**R0.2** Modules are bounded contexts; aggregates are DDD. An aggregate owns its
state, exposes commands/queries, emits events. No anemic models, no service-layer
soup: behavior lives *with* its data (the PoC's offerings/ceremony aggregates were
right — keep that instinct).

**R0.3** Dependency direction is one-way and enforced:
`contract ← glossary ← domain ← adapters ← surfaces`.
Domain imports nothing that does I/O. Adapters implement domain ports. Surfaces
(domain verbs made wire-shaped) never contain domain logic.

**R0.4** Startup is a typed pipeline, not numbered phases. Each step declares what
it needs and produces; the pipeline is readable top-to-bottom by a newcomer in one
sitting. (PoC scar: phases 5/14/17 missing, boot-time exits mid-sequence.)
**Build-up is sacred (L17):** every step's outputs are the next step's inputs —
capabilities accumulate, nothing tears down, nothing skips. A step that cannot
run aborts startup loudly with its name; the garden is never half-built.

**R0.5** Two kinds of contracts, two lifetimes. *On-media contracts* —
`.zen-garden/manifest.json`, stored state formats, anything read off a drive
pulled from a drawer — are forever-compatible: v1 reads what the PoC wrote,
in the field. *Network protocol* is v1's own design: v1 runs a declared,
separate topology (charter amendment 2026-08-25) and may evolve its wire
freely between its own versions, using the `proto` chirp marker to
distinguish speakers. The chirp/envelope *shapes* stay PoC-compatible for
now — not because coexistence is required, but because the format fixtures
already guard them and they cost nothing. When unsure which kind a format
is, it's the on-media kind. *(Amended 2026-08-25: originally bound network
wire to PoC compatibility under an assumed shared room.)*

## P1 — Fewest *meaningful* moving parts

"Meaningful" = must be held in working memory to change code safely. We minimize
that count, not the line count.

**R1.1** One concept, one name, one home. The `glossary` crate defines every domain
noun and verb once. No synonyms (`storage`/`bank`/`volume` for one thing), no
overloaded terms (the three-lease sin). The glossary is load-bearing: names in
code, API, CLI, and docs derive from it.
*(Amended 2026-08-27 — registers.)* The glossary's jurisdiction is code, wire,
and operator surfaces. Household surfaces (family portal, companions) speak a
declared plain register: one rendering per noun, chosen so the metaphor teaches
where it can and household words carry where it cannot (`bank` renders
*storage*, never untranslated). Registers bridge through help and error text —
the standard-term nearest neighbor plus how the garden word differs — never
through second names (R1.3). Provenance:
`docs/v1/design/dx-delight-research.md` (F4) + operator ruling, 2026-08-27.

**R1.2** No parallel implementations of anything. Two mDNS stacks, two enabled-flag
formats, two canonical-layout validators — each was a PoC wound. If two exist at
review, one dies in the same PR — or the PR writes the first's death into
`DEBT.md` with a named milestone. Strangler sequences are legal; unsequenced
duplication is not.

**R1.3** One way to do a thing. No competing idioms for the same operation
(two error styles, two config systems, two JSON envelopes — the envelope drift is
the founding scar; B1 makes it unrepresentable).

**R1.4** Background work is declared in one registry with validated dependencies
(keep the supervisor pattern — it worked). A task that cannot state its
dependencies doesn't spawn.

**R1.5** Dormant code is deleted code. No "keep it around, might be useful"
(announce-if-changed machinery, MAC-OUI tables, limited-broadcast tier). Git
remembers; the tree shouldn't.

**R1.6** Crates are API boundaries, not folders. A new crate requires a
one-paragraph justification of its seam — what it isolates, who depends on it.
Within a crate, `pub(crate)` by default; `pub` is a commitment with a
maintenance price.

**R1.7** No magic values. Every literal that participates in logic, comparison,
wire format, or timing lives as a named constant in *its own domain's*
constants module — never a global grab-bag (PoC scar: `common/constants` grew
into a junk drawer holding two different constants for the same `STONE_CHIRP`
string). Names carry units and glossary nouns (`OFFLINE_THRESHOLD_SECS`, not
`OFFLINE_THRESHOLD`). Wire-format literals — mDNS TXT keys, announcement types,
header names, vault key templates — are declared beside their protocol
definition and pinned by fixture tests: changing one is a breaking change
(R0.5) and must fail CI, never slip past review. The test for "magic": would a
reviewer ask *"what is this?"* If yes, it's magic.

## P2 — Complexity lives at the seams

**R2.1** Every external boundary — disk, network, Docker, OS, koi, clock, randomness
— sits behind a port defined *by the domain*, implemented in adapters. The domain
reads like the charter; adapters absorb the ugly. The domain is *given* time and
randomness; it never asks the wall clock itself.

**R2.2** The `KoiGateway` pattern is the template: one port per seam, embedded
implementation + test double, degrade-to-Unavailable semantics, never crash.
Generalize it; don't reinvent it per seam.

**R2.3** No port without a second implementation or a test double. A trait with one
production impl and no double is speculative complexity — delete it (R1.5).

**R2.4** Complexity budget by layer: adapters may be intricate; domain must be
boring. If domain code needs a diagram to review, the complexity is in the wrong
layer — move it outward.

**R2.5** Failure semantics are uniform: operations return typed results; degraded
capability is `Unavailable`, never a crash, never a silent empty. *And* (B3) every
degradation is observable — posture appears in self-description.

**R2.6** Logging is structured tracing with glossary nouns. No secrets or personal
data in fields; every domain event name lives in the glossary. Log lines are for
operators mid-incident — write them the way R3.3 writes errors.

**R2.7** `cfg` gates live at the adapter layer only (platform reality like udev).
Domain behavior has no compile-time feature flags — a behavior toggle is runtime
configuration (R3.7). Compile-time branching on domain semantics was how the
Windows mDNS split grew two stacks.

**R2.8** Events inside, polling at the edge (L18). Domain internals communicate
by events — broadcast/watch channels — never by polling each other. Timers exist
only where the *protocol* is periodic (chirp heartbeat) or the outside world
offers no push; even then prefer the external event stream (Docker events,
filesystem watchers, sockets). A `loop { sleep; ask; }` over domain state is a
design defect.

**R2.9** Single ingestion, registered handlers. All inbound messages (UDP/unicast
included) enter through ONE ingestion point — parse, dedup, dispatch — fed by
whatever listeners booted. Handlers register by message type ("I handle this"),
pull from their bounded queue, or ignore what isn't theirs. No module owns a
private socket tapping the wire; no handler is invoked by surprise. Ingress never
blocks on a slow handler: bounded queues, counted drops, visible in posture (B3).

**R2.10** A moss boots knowing the approved set (ADR-0008): the catalog is
sourced in LAYERS — the embedded approved catalog is the floor; operator
directories (`MOSS_CATALOG_DIR`, the manifests overlay) add and override BY
NAME. Filesystem absence never means an empty garden, and every layer merge
is logged with honest counts (found / overridden) — a silent merge is a lie
about which truth is serving.

## P3 — Semantics and ergonomics: low cognitive load

**R3.1** Code speaks garden. Function names use glossary verbs (`offer`, `rest`,
`wake`, `replant`) so a stack trace reads like an operator sentence. If the CLI
says `wake`, no function is named `start_service_2`.
*(Amended 2026-08-27 — registers.)* The garden verbs STAY in the CLI:
standard terms (`stop`, `deploy`, `remove`) import standard semantics and
lie where the garden's lifecycle differs — `rest` is a converged desired
state, not a momentary `stop`; the unfamiliar word is the honesty marker.
The expert's first-contact cost is paid by BRIDGES, never second names
(R1.3): every non-derivable verb's help glosses its nearest standard term
plus the divergence, and R3.3's "what to try" carries the cross-reference.
Provenance: `docs/v1/design/dx-delight-research.md` (open ruling) +
operator agreement, 2026-08-27.

**R3.2** Signatures: arguments in domain order (what → where → how), errors as
types, no bare `bool` parameters (name them or split the function), builders over
telescoping argument lists.

**R3.3** Errors answer three questions: what happened, what it means, what to try.
(PoC roadmap goal, now law.) Error types are matched, not string-grepped.

**R3.4** A newcomer reads `kernel/` top-to-bottom in one hour and can name every
moving part afterward. If a file needs a table of contents, split it by concept.
If a module needs a diagram, complexity is in the wrong place (R2.4).

**R3.5** Comments explain *why*, never *what* — except where a lesson applies:
cite it (`// L6: divergence check before delete`). The lessons are part of the
code's vocabulary.

**R3.6** Async patterns are house patterns, reused verbatim: one root
`CancellationToken` tree, `select!` on cancellation in every loop, broadcast
channels for events with documented capacities, graceful drain with deadline.
No unbounded channels; no blocking calls in async contexts (`spawn_blocking`
at the seam). New concurrency idioms require a written pattern doc first.

**R3.7** Configuration is typed, declared once, precedence fixed
(`CLI > env > file > defaults`) and generated into self-description. Environment
variables exist only for platform/deployment concerns, and every env var has a
config twin. (PoC scar: ~50 env knobs, and `STONE_NAME` env silently losing to
cached hostname.)

**R3.8** Every crate and module opens with `//!` stating its responsibility in

**R3.9** Records are paths: rootspace holds sections; sections hold facts; every nesting level is a nameable noun. One canonical shape per record - wire, HTTP, cache, and disk render it identically (B1). Flat remains correct for same-kind maps (ports: {role: n}) and records too small to have sections. Shape authority lives in the contract crate; consumers render, never re-invent.

one to three sentences. If three sentences won't fit, it is more than one
module (R3.4).

## P4 — Mechanical enforcement (lessons as lint)

**R4.1** `clippy -D warnings` from the first commit. No ratchet debt at birth.
`undocumented_unsafe` = deny. `unwrap_used` = deny in domain, allow in adapters'
I/O edges and tests. Domain code never panics on external data — a panic is a
bug; `expect` in kernel wiring carries a justification comment.

**R4.2** `TODO` = deny. Use `DEBT.md`: one line per borrowed shortcut, with the
milestone that pays it. **RC gates on a zero-open ledger.** (The Pavilion rule:
nothing parks — code, docs, or debts — without a named settlement.)

**R4.3** The contract is generated truth (B1): handlers, clients, MCP tools, and
self-description all derive from one schema. Drift is a build error, not a review
catch.

**R4.4** Tests run where CI runs (L15) — every crate, every platform in the matrix,
from the first commit. The orchestrator sin (~162 tests CI never ran) is
constitutionally impossible.

**R4.5** Test doctrine: aggregate unit tests are the bulk (behavior at aggregate
boundaries, per the orchestrators' example); port doubles at seams; the probe
harness pattern for live fleet witnesses. No mock theater — testing internals is
a smell; testing *promises* is the point.

**R4.6** Security paths fail closed and carry a test that proves it. Every verify
names its trust anchor in code (B2). A security rule without a failing-case test
doesn't exist.

**R4.7** Self-description is generated (L7): the API manifest, capability lists,
and docs' load-bearing claims are built from the contract. Stale truth is a build
failure.

**R4.8** Surface degrees are orthogonal and decided once (ADR-0007). A command
computes an **answer**; it never asks how it will be rendered. **Encoding**
(`--output json|human`, env `RAKE_OUTPUT`, alias `--json`) is applied at the one
dispatch point — per-verb rendering branches are bespoke processors and do not
get written. **Projection** (`--format uri|…`) is a verb-declared view that
composes with encoding. **Extraction** (`--field dot.path`) implies json and
composes with both. The CLI self-describes in machine form (`rake manifest`,
generated from the clap tree): help lives once, in doc comments, and the
manifest cannot disagree with the behavior — the moss's Face manifest law (L9),
pointed at the CLI.

## P5 — The shape of done

A unit of work is done when: contract updated (if touched) → domain boring and
green → adapters at the seams → surfaces regenerated → tests where CI runs →
self-description true → DEBT.md settled or annotated → and, for milestone work,
a live fleet witness recorded in `WITNESSES.md`.

---

*These rules are the PoC's tuition, collected. They are meant to make v1 boring
in the right places so it can be delightful in all the others.*
