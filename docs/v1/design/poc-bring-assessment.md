# What we bring from PoC to v1 — the full assessment

**Status:** DRAFT for refinement — input to roadmap, not yet law. Written
2026-08-28, after the read-everything inventories
(`../inventory/poc-rake-surfaces.yaml`, `../inventory/poc-moss-surfaces.yaml`,
2,200 lines of file:line evidence) and the W7 witness. Sources of truth: the
inventories, `lessons.md` L1–L26, the charter's bets and jobs, DEBT.md.

**Method:** every PoC surface was classified into four verdicts —
**Bring whole** (port near-verbatim), **Bring reshaped** (the capability is
law, the v1 form differs deliberately), **Bring later** (real, deferred with a
named gate), **Leave dead** (the lesson is kept, the thing is not). Every item
is tagged with the persona it serves (Gardener / Household / Agent / Developer /
Whisperer / Successor / Skeptic — see the delight-personas ideation) and the
job (J1–J4) it advances.

---

## v1 is already ahead — do not port backward

Before the gaps, the ledger of places v1 must **not** inherit PoC shape:

- **The route table and its fiction.** PoC: 281 registrations, ~215 unique
  pairs, 27 advertised (7 of them 404 ghosts), three error shapes,
  bare-vs-wrapped drift in ten clusters. v1: the `Face` enum — declared once,
  manifest = router, unadvertised emissions structurally impossible.
- **The CLI's manifest drift.** PoC: dead flags (`--snapshot-id` read as
  `harvest-id`), advertised syntaxes that fail parsing, suggestions pointing
  at nonexistent commands, two signing regimes, two recovery regimes. v1:
  verbs are the enum; the same discipline applies.
- **Envelope drift.** v1 has one envelope (B1); companions/hey/greenhouse
  drift must not be ported when those surfaces arrive.
- **What v1 has that the PoC never did:** capture/replant (the living will),
  sink banks with role declaration, rich replies carrying the full inventory
  map, the candidates pool, sectioned records + plan-hash exclusion,
  readiness honesty (untrusted volumes refuse to be silently tarred), the
  audit chain with `Replanted`, and the URI grammar faces.

---

## Tier 1 — Bring whole (port near-verbatim; high delight per line)

| # | Capability | Serves | Job | Notes |
|---|---|---|---|---|
| 1 | **Portrait** — per-stone landing page | Gardener, Skeptic | J4 | PoC `portrait.rs` + `portrait.html` embed the page from identity/resources/offerings/topology. v1's SelfView *is* the data source; the port is rewiring, not redesign. The Skeptic's landing "wow". |
| 2 | **Pulse (moss side)** — SSE stream + embedded panel | Gardener | J4 | PoC streamed domain + transport events; v1 has *better* events (OfferingChanged, topology Seen/Goodbye/Expired, capture runs, Replanted). One SSE endpoint + page. |
| 3 | **Pulse (rake side)** — the full-screen wall monitor | Gardener | J4 | The 1,453-line rendering module: regions, gauges with thresholds, event ring. Live-proven at 32 evt/min. The port is transport (v1 events) not craft. |
| 4 | **URIs on rake list/find** — the connection string as output | Agent, Developer | J1 | `--format uri` (+ `uri-ip`): the promise as a shell primitive. The ledgered homes exist; the rendering is the missing inch. |
| 5 | **`inspect`** — full-state JSON dump | Agent, Developer | J4 | PoC's `inspect --json` was the agent's x-ray. v1's Face enum + SelfView make it *more* truthful than the PoC's ever was. |
| 6 | **`hey` passthrough** — command relay to companions | Whisperer | J4 | The mechanism (auto-start, proxy, broadcast "all") is sound; the envelope drift is what we do NOT bring — the relay speaks the contract. |

## Tier 2 — Bring reshaped (capability is law; v1 form differs)

| # | Capability | Serves | Job | The reshaping |
|---|---|---|---|---|
| 7 | **Garden storage data plane** — fs CRUD, objects, snapshot browse on banks | Household, Successor | J2 | PoC: `/garden/storage/{name}/fs` + objects + snapshots. v1: banks exist; the data plane lands on them, constrained by the sink/replication role laws (ADR-0005). **Slice verdicts (2026-08-28, first mandate exercise):** BROUGHT — list/get/put/delete (shipped), routing via the garden's redirect + rake-follow instead of the PoC's proxy (leaner: no coordination registry, no loop guard), `?depth` tree listing, RFC 7233 Range on file GET (206/416, single range; PoC's 416-for-end<start corrected to the RFC's ignore), PATCH move-within-bank (never overwrites), HEAD (free via transport), safe_join escape gate at any depth. DEFERRED — explicit mkdir (gate: first empty-dir consumer), listing truncation caps (gate: first oversized bank), snapshot browse + its access-audit log (gate: jobs/logs slice, surfacing via capture faces), manifest `visibility`/`encrypted` fields (gates: election surfaces / M2 pond). LEFT DEAD — the bespoke objects API (superseded by #8's standard S3 dialect), Primary/replica proxy machinery (redirect replaces it), manifest `filesystem` field (derivable telemetry, not identity). |
| 8 | **S3 gateway + presign + WebDAV** | Developer, Agent | J1 | The garden speaks standard dialects so existing tools work *unmodified* — agent-delight: every S3 client becomes a garden client. Gateway leases per B7; pond-auth per B2 when M2 lands. |
| 9 | **Jobs registry + `/jobs/{id}/stream`** | Developer, Agent | J4 | Every mutation's async contract (capture, nourish, deploy all want it). PoC was memory-only with documented fiction; v1: persisted, event-streamed, in the Face router. |
| 10 | **Logs/watch streaming** | Developer | J4 | The PoC's open wound (advertised, stubbed). v1: docker adapter → SSE → `rake watch`. Ships with the jobs slice. |
| 11 | **Companions** — sdk + usb + registry + relay | Whisperer, Household | J4 | PoC architecture kept (guests at the edge, B7); the five integration rulings pending (device ownership, event transport, port pool 7286–7295, SDK home, hardware availability). Serial side (`companion-usb`, handshake) ports nearly as-is. |
| 12 | **Cricket** — audio companion | Household | J4 | No hardware dependency; YAML tunes port whole. The backup-completion chirp is the Household's heal-moment. |
| 13 | **Orchestrators / O3 adoption** — Ollama as first citizen | Developer, Gardener | J3 | PoC had ollama/mongodb/ai crates; v1 scrubbed them. The v1 form: L25 detect → adopt → expose, compatibility predicates (`ai.runtime`) already in the census grammar. The S3/gateway surfaces above give the "expose" half a home. |
| 14 | **Stone power ops** — shutdown/reboot/WoL/refresh | Gardener | J4 | PoC `stone` command cluster. Small; pairs with wake-on-LAN material already in the frame's network section. |
| 15 | **Agentic baseline** — errors-as-JSON, exit codes, `--field`, `--format` | Agent | J1 | The PoC's three-degree machine output (`--json`, `--output json`, `--field dot.notation`, `--format uri`) generalized; R3.3 gains its exit-code paragraph. Cheapest delight on the board. |

## Tier 3 — Bring later (real; named gate before it lands)

| # | Capability | Gate |
|---|---|---|
| 16 | **Nourish** — garden-wide updates with canary rings | J3's arc; after jobs registry exists (it needs async). PoC had it live. |
| 17 | **Pond security** — trust ceremony, signing, envelope enforcement, loopback sign oracle | M2 per charter (L2/L3 audits). The hidden loopback sign oracle is a genuinely good idea worth reviving there. |
| 18 | **Elections** — deterministic duty arbitration | When a second duty (storage Primary beyond first-online-wins) exists. BLAKE3-deterministic election philosophy already cited in ADR-0004. |
| 19 | **Greenhouse authoring UI** | After portraits/pulse prove the embedded-page pattern. |
| 20 | **Capabilities/mirror** (model + extension management) | With orchestrators (#13) — the two are one surface. |
| 21 | **Backup A/B slots** (nurturing) | Superseded by ADR-0005 checkpoints *except* the A/B slot concept — revisit when checkpoints gain restore-facing rotation semantics. |
| 22 | **Pavilion / Explorer projection** | M4 per charter. Recover the archive branch first (PAVILION-0002). |

## Leave dead — the lesson kept, the thing buried

- The **manifest fiction** (27 advertised / 7 ghosts / 95% unadvertised) —
  v1's Face router + manifest test kills the class.
- **Envelope drift** in all its forms — three error shapes, bare-vs-wrapped.
- **Dead flags and phantom suggestions** — the Face/table discipline makes
  them unwritable; suggestions derive from real verbs.
- **Two enabled-flag formats, two signing regimes, two recovery regimes** —
  R1.3's one-way law, now mechanically enforced.
- **Announce-if-changed dormant machinery, MAC-OUI tables, limited-broadcast
  tier** — already cut by the charter.
- **`template`'s "coming soon" lie** (it was fully built) — under the Face
  regime, a thing is either routed or it does not exist.

## Sequencing recommendation

1. **Visibility slice**: URIs + portrait + pulse-moss-side (Tier 1: 1–3).
   Three commits, immediate delight, all port-near-whole.
2. **Agentic baseline slice**: #15. One commit, the Agent persona unblocked.
3. **Companions epic**: #6, #11, #12 — after the integration rulings.
4. **Data plane epic**: #7, #8, #10, #9 (jobs first — the others stream
   through it).
5. **Orchestrators/O3**: #13, #20.
6. **M2 security**: #17 + B2 trust chains — before any public M1.
7. **Nourish**: #16. Then RC.

## Operator ruling (2026-08-29): the Integration epic

Zen Garden must be fully operational WITHOUT Koi and Suzu, and
integrate with them when available. Moved to the **Integration epic
(charter M6, gated on Koi/Suzu being baked)**: #6 hey passthrough,
#11 companions, #12 cricket, #17 pond security's signing/CA-chaining
and trust ceremony, authenticated storage proxy auth, borrow vaulting
(D13). Staying CORE from old #17: TOFU pinning, refuse-unsigned policy
hooks, S3 auth-mode decision, observable posture (new M2 scope).
Every integration enters as a KoiGateway-style port with a test
double — degrade cleanly, never hard-depend (B7).

## Open operator decisions

1. Companions integration (five questions from the ideation — device
   ownership, event transport, port pool, SDK home, hardware availability).
2. Repo visibility + what "a stranger installs" concretely is (M1's gate).
3. The SanDisk on emerald vale: adopt as a garden bank there (roles: sink?)
   or remain a roving vault.

---

*The PoC's capabilities were proven; their implementations were tuition. We
bring the capabilities, the evidence, and the scars — and none of the drift.*
