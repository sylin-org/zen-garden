# Zen Garden 1.0 — Greenfield Charter

**Status:** Accepted 2026-08-25 · governs src/v1
**Authority:** This charter governs the clean-code migration. The inventory
(`inventory/*.yaml`) is its evidence base; the lessons ledger (`lessons.md`) is
its constraint set; `CODE-RULES.md` is its engineering law. Conflicts resolve
in order: lessons > charter > code rules > inventory verdicts > PoC precedent.

---

## Mission

Services outlive machines, on hardware nobody wants.

One sentence, three claims the PoC proved and 1.0 must make ordinary:

1. **Machines are disposable; services are not.** Identity attaches to
   services and media, never to boxes.
2. **Discarded hardware deserves a second career.** Weak, heterogeneous,
   aging machines are the *normal* deployment target, not a charity case.
3. **Sovereignty without sysadmin burden.** Managed-cloud reliability
   semantics on hardware you own, operable by one person.

## The four jobs (what households hire it for)

1.0 is organized around jobs, not mechanisms. Discovery is plumbing beneath
all four.

| # | Job | 1.0 bar |
|---|-----|---------|
| J1 | **Reach it** — family access without a VPN lecture | Service-type resolution with shipped resolvers (Node first); family-facing portal from portraits |
| J2 | **Back it up** — the safety net that proves itself | First-run asks where safety lives; snapshots to seed banks; automated restore rehearsal, reported green/red |
| J3 | **Update it safely** — never the watchtower story | Nourish with canary rings (1 stone → soak → fleet), automatic mark-good revert |
| J4 | **Trust it while ignoring it** — calm, honest surfaces | Pulse + companions + heal-moment events; posture (security/degradation) always observable, never silent |

## Pillars (promoted from PoC evidence)

Each pillar cites its proof in the inventory. A pillar without live proof by
RC does not ship as a pillar.

| Pillar | Proof |
|--------|-------|
| Presence → discovery → transparent failover | `discovery-plane.yaml` — witnessed unprompted on fleet |
| Service-type resolution + resolvers | `find` live; resolvers = new work (J1) |
| Offering catalog + desired-state lifecycle | `service-lifecycle.yaml` — live recovery: wipe → pull → recreate, ports preserved, dormant/desired-state honored |
| Hardware fitness placement | `inspect` fleet census live |
| Fleet self-deployment + nourish | `delivery-ops.yaml` — garden deploys itself via own discovery |
| Intermittent stones | Discovered live 2026-08-25: part-time workstation stone; reconcile-on-wake already works |
| Plug-and-play garden storage → Windows cloud drive | `storage-estate.yaml` — cfapi chain proven by code + live registry artifact; client parked mid-repair |
| Pond security as ceremony | TOTP pairing live; enrollment UX proven |
| Ambient legibility (pulse/companions/portraits) | `clients-companions.yaml` — probe-tested end-to-end |

## Non-goals (inherited and sharpened)

Not Kubernetes; no multi-tenancy; 3–30 stones; local-first; no cloud control
plane. **New:** no capability ships without its crash-recovery path (L11) and
its self-description true (L7). Breadth is earned by soak, not by curiosity.

## Architecture bets (ADR skeletons)

Each bet names the lessons it satisfies. Full ADRs written at implementation
of each.

| # | Bet | Lessons |
|---|-----|---------|
| B1 | **Contract-first codegen.** One schema source generates moss API, StoneApi, rake, resolvers, web types. Envelope-vs-bare becomes unrepresentable. | L1, L7 |
| B2 | **Trust chains or no chains.** Every verification names its trust anchor; fails closed; "unsigned during transition" carries CI-enforced expiry. Presign secrets from vault, never identifiers. Pin on join. | L2, L13 |
| B3 | **Observable posture.** Enforcement stage, signing state, degraded capabilities advertised in /health and chirps. Degrade-don't-crash stays; silent-off goes. | L3 |
| B4 | **Soft presence, leased duties.** Membership stays heartbeat-soft; every duty (primary, update-source, coordinator) carries explicit lease + takeover. Storage primary election gains divergence safety. | L4, L5, L6 |
| B5 | **Declarative garden.** `garden.yaml` desired state (offerings, placement policy, backup targets) reconciled continuously — generalizing the reconcile loop the PoC already proved. | L4, J2, J3 |
| B6 | **Intermittent stones first-class.** Third presence state (intermittent), nourish-before-serve on wake, goodbye on sleep. | L10, live evidence |
| B7 | **Small kernel, guests at the edge.** Kernel = supervisor + registry + presence + routing. Orchestrators, companions, koi sidecar (via KoiGateway port) stay out-of-process guests. | L8 |
| B8 | **Manifest-driven surfaces.** CLI, companions, offerings declare; behavior wires to declarations. | L9 |
| B9 | **Wrapped distribution.** Tags → built artifacts → checksums/signing → GitHub releases feeding the existing fleet-native deploy. The fake front door becomes real before anything else ships. | L10, L15 |
| B10 | **Platform citizenship.** Each platform declares its supported surface; outside it, refuse loudly at startup. Windows: offering host (proven) + cloud-drive client (pillar) + storage scan-only (fenced). | L14 |
| B11 | **Delight budget.** Pulse philosophy, companion SDK contracts, portraits, named ponds carry forward as subsystems with contracts. New: heal-moment events, watts/e-waste counters. | L16 |

## Ports, redesigns, cuts (from inventory verdicts)

- **Port near-whole:** common types/discovery transport, companion SDK +
  cricket/firefly, rake connection layers, ceremony journaling concept,
  dist.json packaging model, perennial Docker builders, musl-QEMU trick,
  NewStone preseed+sentinel ideas, probe harness.
- **Redesign:** bootstrap phase machine, storage routing (authenticated proxy,
  divergence detection, streaming AEAD), security plane (single transport
  story; enforce-stage as config artifact), version scheme, jobs durability,
  Windows data-dir resolution, lantern registration (fix path bug class).
- **Cut:** S3 multipart breadth (until a user screams), SMB signpost,
  per-file AEAD (replaced by B4 storage design), dead pre-install manifests,
  announce-if-changed dormant machinery, MAC-OUI dead code, limited-broadcast
  tier, Pavilion-as-Tauri (decide fresh per B10 after recovering the branch).

## Migration story

The PoC fleet is the first migrator. New trunk ships behind the existing
deploy channel; stones migrate garden-by-garden; the old daemon's
DEPLOY-0001 machinery is the safety net for its own replacement. No flag-day.

*(Amended 2026-08-25)* Migration joins a stone to **v1's own topology**
(discovery UDP `7284`, multicast `239.255.42.199`, HTTP `7285`; declared
block 7284–7299) — cutover per stone is atomic: stop moss, start garden,
the stone changes rooms. The generations never share a discovery room;
coexistence was a requirement invented by assumption, not by need. The PoC
proved the mechanisms; v1 makes better, informed decisions about everything
it inherited — topology, ports, handshake, all of it. Wire format remains
fixture-pinned for on-media compat (CODE-RULES R0.5 as amended); network
design answers only to v1.

## Sequencing (stop-anywhere value)

| Milestone | Delivers | Gate |
|-----------|----------|------|
| **M0** | This charter accepted; baseline committed | ✅ |
| **M1** | Contract + codegen; Node resolver v0; release pipeline real (tag→sign→publish); `rake api` class of bugs unrepresentable | A stranger installs from a public artifact |
| **M2** | Security prerequisites: chirp CA-chaining, authenticated storage proxy, S3 auth-mode decision, TOFU pinning, observable posture | L2/L3 audits pass |
| **M3** | Declarative garden + desired-state reconcile + intermittent stones | Fleet runs on garden.yaml; laptop-stone wakes clean |
| **M4** | Cloud-drive pillar: recover/rewrite cfapi provider, uploader repaired | Family laptop sees garden storage in Explorer |
| **M5** | Delight layer: family portal, heal-moments, counters | J1–J4 demonstrable to a non-technical household member |

## Definition of 1.0

A stranger on an existing workstation or server goes from zero to a running
offering with one command, upgrades safely forever, backs up to proof, and
the connection-string promise is true in at least one shipped language —
while the fleet that proved the concept migrates without a flag day.
