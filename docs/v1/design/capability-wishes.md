# Capability wishes — the slice proposal (walked 2026-08-29, awaiting nod)

*The Slice Mandate walk for the capability wishes / orchestrator slice.
Status: PROPOSAL — nothing implemented. On a nod, the design section is
the contract.*

---

## Gate 1 — Prior art

- **MCP (Model Context Protocol)**: consumers enumerate a server's
  resources/tools before use; discovery is a first-class operation, not a
  config file. Promise: agents never hard-code what a peer can answer.
  Cost: servers must keep listings honest and fresh.
- **Ollama `/api/tags`** (the concrete case that proves the pattern):
  every Ollama answers "which models do I hold". The PoC simply wrapped
  this; the generalization is the wrap, not the endpoint.
- **Kubernetes label selectors**: wishes as selectors evaluated over
  inventory; the scheduler answers by matching. Promise: intent separate
  from location. Cost: a selector grammar — kept small or it eats the
  product.
- **Home Assistant entity registry**: services publish typed entities
  (`domain.object_id`); automations wish by shape, not by address.
- **Cautionary — Consul service tags**: flat tag strings, rarely queried;
  the lesson is that wishes need TYPE + ITEM structure, not bag-of-words.

## Gate 2 — The PoC (objective, then mechanism)

**Objective:** the connection promise reaches past the offering to its
content — "is there an LLM here **with llama3**?" is a room question with
a room answer, and if the capability is missing but the offering can
grow it, `ensure` grows it and answers when ready.

Mechanism inventory (poc):
- **Wish grammar** (`common/src/tools/types.rs:347+`): canonical
  `offering[:instance][cap_type:item,...]` (brackets, `|` allowed),
  shorthand `offering:item` with a manifest-declared default type.
  Parsed in common; consumed by rake find/ensure.
- **Capability manifests** (`common/src/manifests/capabilities.rs`):
  per offering, per type (`model`...): display, mutability
  (hot/warm/cold), and operations list/add/remove/check_updates/upgrade —
  each a command templated per mode (managed = docker exec,
  adopted = host http/exec) and per platform (linux/windows/macos), with
  timeouts (add: 2h) and output transform specs.
- **Executor** (`domain/capabilities/executor.rs`, 1072 lines): builds
  context, runs the templated command, transforms output into
  `CapabilityCollection`s; add/remove report via jobs with progress.
- **Wire**: `sub_capabilities` on Offering; garden capabilities faces
  (moss-surfaces.yaml:92-96, 223): list/refresh/add/delete/mirror.
- **rake**: `find` matches wishes against the room; `ensure` calls
  add_capability when the wish is unmet, waits, answers.

Gaps: executor is 1k lines of string templating with no test seams to
speak of; per-platform command tables multiply every manifest; mirror
(cross-stone, signed) is security-dependent; nothing inherited by v1.

## Gate 3 — The house

- **J1 bar** (charter): service-type resolution with shipped resolvers;
  the connection promise. Wishes are J1's deep end.
- **L25**: detection → candidacy → remembered binding →
  **capability-scoped control**. Capabilities are the fourth station.
- **D14 / OFFERINGS.md §5.1**: `capabilities:` is a RESERVED, claimed
  grammar name in the manifest format — "design parked". This slice is
  the unparking. (v1 DEBT D14 is closed separately — ports; the manifest
  comment predates that closure and should be renumbered on landing.)
- **R2.1/R2.4**: command execution is seam work (adapters); matching and
  grammar are boring domain.
- **The exec seam already exists**: `capture_run::HookRunner` (docker
  exec, timeout-bounded) — built for capture hooks; the capability
  executor is its second consumer (R2.3's second implementation, finally).
- **R4.8/ADR-0007**: answers never ask how they render; new faces enter
  the FACES table (ADR-0009), surface.json regen gates.
- **L11**: any add/upgrade mutation is a long job (model pulls: hours) —
  no state machine without crash recovery. This gates the mutation half.

## Gate 4 — Design (two stages, one grammar)

**Stage W1 — discover and answer (read-only). Proposed now.**

1. **Wish grammar in `contract`**: parse
   `stem[::instance][type:item,...]` + shorthand `stem:item` (default
   type from the offering's manifest). One machine-truth parser, fixture-
   pinned (R1.7). The FQN part rides glossary::fqn; the selector part is
   new, tested, boring.
2. **Manifest grammar** (unpark D14): a `capabilities:` section per
   offering — per type: `type`, `default: true|false`, `list:` (channel),
   and NOTHING else in W1. Two channels only:
   - `exec: [argv...]` — run inside the offering's container (HookRunner
     seam, stdout captured — one-line extension to return output);
   - `http: { path, item_path, value_path }` — GET, JSON transform.
   No per-platform tables (v1's stones speak one world at the seam), no
   add/remove/upgrade yet, no transform DSL — two JSON paths beat a
   language.
3. **Domain (`moss/src/offerings/capabilities.rs`)**: resolve manifest →
   channel → items; `discover(offering) -> Vec<(type, Vec<item>)>`.
   Results cached on the offering record (`sub_capabilities`), refreshed
   on demand and opportunistically on the sweep clock for offerings that
   declare capabilities. Boring: grammar in, items out, no retries.
4. **Wire**: `ServiceEntry` gains `capabilities: {type: [items]}`
   (capped, honest, omitted when undeclared — R0.5: v1 owns this wire);
   offerings face rides the snapshot; new faces in the FACES table:
   `OfferingCapabilities` (list/refresh). surface.json regen.
5. **Answering**: `rake find "ollama[model:llama3]"` and `rake ensure`
   match stem **and** capability across the room envelope before
   planting; Node resolver's `serviceMatches` gains the same check
   (chirp entries now carry the data). `resolve("ollama:model")` answers
   with a connection string only when the capability is really there.

**Stage W2 — assure (the orchestrator; its own mandate walk).** Add
(followed by remove/upgrade) with job semantics: L11 crash recovery,
progress events (feeds the jobs-stream slice), mutability honored
(hot/warm/cold), timeouts from the manifest. Gated on W1 + jobs-stream;
NOT in this slice.

## Gate 5 — Verdicts

| PoC element | Verdict |
|---|---|
| Wish grammar (typed selectors, shorthand) | brought (contract parser, fixtures) |
| Capability manifests | brought reshaped (exec/http channels; per-platform tables die) |
| Executor | brought reshaped (HookRunner seam + stdout; 1072 lines → ~200 boring ones) |
| `sub_capabilities` on the wire | brought (ServiceEntry, capped) |
| rake find/ensure wish matching | brought (room-wide, then resolver) |
| Node resolver capability check | new (v1's resolver didn't exist in PoC) |
| add / remove / upgrade mutations | deferred to W2 (L11: job semantics first) |
| Cross-stone signed capability mirror | deferred (M2 security prerequisites) |
| check_updates / upgrade of capabilities | deferred (nourish semantics for content; post-W2) |
| Garden-wide aggregate face | deferred (free later: chirps already carry entries) |

## Delight / audiences

- **Agent (J1)**: `resolve("ollama:llama3")` answers with a connection
  string only when that model truly answers somewhere in the room.
- **Gardener**: `rake capabilities ollama` reads the models on the stone;
  the room envelope shows which stone holds which model.
- **Household** (M5 portal, later): "found" markers — the garden speaks
  content, not just services.
- **Skeptic demo**: pull a model on one stone → `rake find` room-wide
  flips to answer; stop the stone → honest miss. The connection promise,
  one level deeper.

## Cost & size (honest estimate)

contract parser + fixtures (~150 lines), manifest section (~80), moss
domain + faces (~250), wire field + regen, rake matching (~80), Node
resolver check (~20) + tests throughout. One deploy + witness:
pull a model on .195's (adopted or planted) ollama, watch `rake find`
answer room-wide.
