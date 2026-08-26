# Offerings in v1 — Understanding & Proposed Behavior

**Status:** DRAFT for refinement — not law. This document exists to be argued
with. Sections 1–2 are harvested fact and stated rationale (citations point
into `docs/poc` and `src/poc`); sections 3–6 are the v1 proposal built on
top; section 7 lists what needs the gardener's decision before any brick is
laid.

---

## 1 · What the PoC taught us

### 1.1 The soul (stated rationales worth keeping verbatim)

- **Templates encode correctness.** "This works. It's also a minefield: no
  healthcheck… hardcoded password… configuration drift is not a risk. It's a
  certainty. Offerings exist to encode what 'correctly deployed' means."
  (`philosophy/curated-offerings.md`)
- **Non-intrusion.** "Direct access is always available. The abstraction is
  *offered*, not imposed… The system adds options. It doesn't remove them."
- **Detect generously, control conservatively.** Adoption defaults to
  Monitor-level control because shared systems, production databases, and
  externally-managed daemons must never be auto-restarted by a garden that
  merely noticed them. (`guides/adoption-detection-vs-control.md`)
- **Containers are disposable; the registry is truth.** "Ephemeral shells
  around a manifest spec + config patches" — this premise is what makes
  self-healing reconciliation safe rather than reckless. (`OFFER-0008`)
- **The offering is the unit; the machine is incidental.** "MongoDB doesn't
  live 'on stone-amber-ridge'—it lives 'in the garden'. When you backup, you
  backup offerings." (`journeys/02-the-life-of-an-offering.md`)
- **Language shapes the system.** "You don't deploy and terminate. You plant,
  and you take away." (`curated-offerings.md`)

### 1.2 The mechanism map (facts we build on)

| Mechanism | How it worked | Why |
|---|---|---|
| **Modes** | Managed / Adopted / Borrowed (`OFFER-0005`) | Containers aren't always right: GPU overhead, existing native installs, NASes/printers can't be containerized. "Garden represents the full home infrastructure." Admitted cost: adopted/borrowed have weaker guarantees. |
| **FQN** | `offering[::instance]`, e.g. `mongodb::prod`; adopted instances get `::adopted` (`OFFER-0006`) | Multi-instance without ambiguity; volume trees isolated per FQN (`OFFER-0007` scar: two instances sharing one volume dir). |
| **Namespace law** | Only `zen-offering-*` containers are claimable (`OFFER-0002`) | Prevents claiming strangers; makes self-heal adoption safe by construction. |
| **Desired-state reconcile** | 30 s health loop; missing container ⇒ rebuild from manifest+patches; stored ports win; backoff 30→480 s ×5 then Degraded | Scar: post-Docker-wipe log-spam with no recovery. Priority ordering: "**service running > perfect port preservation**". Rested offerings stay rested (reconcile respects Stopped). |
| **Ceremony** | Journaled multi-phase ops with rollback: nourish = Collect→Nourish→Water; quiesce hooks (e.g., mongo fsyncLock) let DBs snapshot live | "A deliberate, multi-phase operation with safety guarantees"; crash-recovery via journal. |
| **Adoption paths** | (a) zen-stray containers auto-claimed if template known; (b) native detection: process signature + HTTP probe, tiered scan (10 s×10 min then 30 s), compatibility gate, default control=Monitor | Self-heal after wipes (a); bring hand-installed AI/native stacks into discovery without taking them over (b). Detection ≠ control. |
| **Borrow/lend** | Register external endpoint under a garden name; return = unregister; service untouched | Connection-string stability across vendor migrations; NAS/printers/DBA-owned clusters join discovery without management. |
| **Health** | Probe→compare→mutate→emit; only *transitions* emit events; docker-events stream complements polling | Quiet-by-default event streams (L18 ancestor). |
| **Chirp payload** | `services[]`: `{offering_id, name(FQN), category, status, role?, ports}`; status strings running/stopped/installing/cordoned/degraded/maintenance/unknown | Already carried forward into our `contract::chirp::ServiceEntry`. |

### 1.3 The rigidity we are breaking

`ARCH-0030` sealed all Bollard/Docker types behind a concrete
`docker::ContainerRuntime` and **explicitly declined a trait**, reasoning
that a 1:1 forwarding interface wouldn't help podman/containerd because their
APIs differ fundamentally. Consequence: every runtime feature (exec, logs,
events, stats, port-scan, daemon-config editing) grew *inside* the Docker
module, and ~15 call-site families assume containers exist.

That decision was half right: verb-mirroring traits ARE useless across
runtimes. The conclusion drawn from it (no seam at all) is what v1 rejects.

---

## 2 · The v1 proposal: intents, not verbs

### 2.1 The seam — `Runtime` as a capability-bounded intent contract

Moss never asks a runtime "how do you create a container?" It declares
**intents** and receives **observations**:

```
trait Runtime {
    fn kind(&self) -> RuntimeKind;              // Docker | Podman | Systemd | Process | External…
    fn capabilities(&self) -> Capabilities;     // exec? logs? events? healthchecks? gpu?

    async fn plant(&self, spec: WorkloadSpec) -> Result<Handle>;     // create+start, idempotent by name
    async fn observe(&self, h: &Handle) -> Result<Observation>;      // state, health, RESOLVED ports
    async fn take_away(&self, h: &Handle, keep_data: bool) -> Result<()>;
    async fn rest(&self, h: &Handle) -> Result<()>;
    async fn wake(&self, h: &Handle) -> Result<()>;
    async fn exec(&self, h: &Handle, cmd: &[String]) -> Result<ExecOutput>;
    fn logs(&self, h: &Handle) -> LogStream;
    fn events(&self) -> EventStream;            // runtime-native when available, else synthesized
}
```

Rules that make this honest where ARCH-0030 feared to tread:

- **Capability honesty over fake universality.** An adapter declares what it
  truly supports; moss degrades visibly (posture!) when a verb isn't real.
  A systemd adapter says `exec: false`; nothing pretends.
- **Observation carries resolved truth** (actual bound ports, actual health)
  — the reconcile loop's input, exactly the PoC's hard-won lesson about port
  drift.
- **Events optional but typed.** Runtimes without event streams get
  poll-synthesized ones; consumers never know (R2.8 holds: events inside,
  polling at the protocol edge).
- The Docker adapter is a *port* of today's `docker::ContainerRuntime`
  knowledge (port scan, remediation, naming law) — nothing is relearned.

### 2.2 Modes become capability profiles, not special cases

The PoC's three modes were three codepaths. In v1 they are three points on
one spectrum of *who holds the intent*:

| Mode | Intent holder | Runtime role | Control levels |
|---|---|---|---|
| Managed | Moss (from a manifest) | plants/observes via full `Runtime` | full |
| Adopted | The outside world | observe-mostly adapter (process/systemd/container watcher); moss never mutates unless control=Full | Monitor (default) / Full / Announce |
| Borrowed | Nobody local | pure declaration; no runtime at all — a `ConnectionTemplate` in the topology | announce-only |

Adoption keeps its PoC soul: generous detection, conservative control,
namespace law for strays, `::adopted` FQN suffix, compatibility gates.

### 2.3 Vocabulary (proposed additions to glossary)

- **Workload** — the agnostic "run-time container": whatever a Runtime
  places and observes. A Docker container, a systemd unit, a detected native
  process, even a borrowed endpoint seen from afar.
- **Plant / Take away** — create / remove (keep-data flag decides volume
  fate). Rest/Wake unchanged.
- **RuntimeKind** — Docker, Podman, Systemd, Process, External.
- Everything else inherited: FQN grammar, namespace law, ceremony names,
  reconcile, cordon.

### 2.4 Lifecycle (proposed states — superset-compatible with chirp wire)

```
minting ──> planting ──> running <──> stopped(rest)
                            │  \__> degraded (reconcile exhausted)
                            └────> gone (boot found nothing; registry decides)
cordoned ⟂ (unschedulable flag, orthogonal)
adopted/borrowed carry presence states, not planting states.
```

Wire mapping stays byte-compatible: these collapse into the seven v0 status
strings for chirps.

### 2.5 Reconcile (inherited nearly whole)

Registry = truth; runtimes report actuals; drift triggers re-plant with
stored ports winning; backoff ladder 30/60/120/240/480 ×5 then Degraded;
rested stays rested; boot pass marks vanished offerings gone and adopts
strays. One v1 change: reconcile issues **intents** through whichever
Runtime adapter owns the workload — it never knows Docker.

### 2.6 Ceremony (deferred shape, kept name)

Nourish/Vacate/Replant/Store remain *ceremonies*: journaled phases with
rollback. v1 M-offerings does NOT build the framework; it only ensures the
plant/observe intents are rich enough that ceremonies can be written later
without re-touching runtimes.

---

## 3 · Proposed verb behaviors (thin-client shaped)

All ride the L21 cascade; endpoints land under `/api/v1/stone/*` (ops on
this stone) and `/api/v1/garden/*` (room-wide), per L22.

| Verb | Behavior |
|---|---|
| `rake offer [name]` | browse catalog → pick placement (compatibility-aware) → minting/planting ceremony with job progress |
| `rake rest|wake [fqn]` | stop/start preserving data; reconcile respects rest |
| `rake remove|uproot [fqn]` | soft delete (stray-able) vs destroy (data dies; confirmed) |
| `rake adopt|release [target]` | claim/detach existing workload; release leaves it running |
| `rake borrow [name] from <url>` / `return` | register/unregister external endpoint |
| `rake reconcile` | manual sync registry↔runtimes; prints adopted/dropped/left |
| `rake watch [fqn]` | SSE logs/events |

---

## 4 · Open questions (block bricks, not thought)

1. **First adapter target**: Docker only (parity with PoC), or Docker+Podman
   together to force-test the seam's honesty early?
2. **Where do adapters live?** New crate `crates/runtime` (kernel sibling)
   vs module inside kernel. (P0 modular-monolith leans module-with-port.)
3. **Manifest format v1**: keep PoC's four-file set (snippet.yaml +
   frontmatter + compatibility + guidance) or simplify to one file with
   sections? Wire/catalog implications either way.
4. **Adopted detection engine**: port DETECT-0001 signatures now, or start
   manual-adopt only and automate later?
5. **Naming**: is `Workload` the right noun, or does the garden already own
   a better word?
6. **Ceremony timing**: confirm deferment of the journaled framework until
   nourish/vacate are actually needed.
