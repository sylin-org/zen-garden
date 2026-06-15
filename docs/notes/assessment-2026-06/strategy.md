---
audience: [maintainer]
doc_type: assessment
status: current
date: 2026-06-11
---

> Part of the [June 2026 project assessment](README.md). Landscape data compiled 2026-06-11: GitHub
> figures pulled from the GitHub API that day; web sources ranged from first-party blogs (high confidence)
> to comparison sites (directional). Competitor star counts and dates below are point-in-time snapshots.

# Strategic Positioning and Opportunities

## The 2026 landscape in brief

| Signal | Detail |
|---|---|
| **CasaOS dormant** | 34k stars, no pushes since Aug 2025; vendor (IceWhale) pivoted to ZimaOS hardware. The largest single-node home-server UI is a maintenance vacuum — its audience is shopping for a home |
| **Nomad orphaned** | BSL since 2023, IBM-absorbed 2025, no community fork (unlike Terraform→OpenTofu). "Real orchestration without k8s" has no credible open champion |
| **TrueNAS dropped k3s** | 24.10 replaced Kubernetes with plain Docker for apps — the biggest storage-OS vendor empirically validating the no-k8s thesis |
| **Tailscale Services GA (Feb 2026)** | `svc:name` + virtual IPs with multiple backends — service-identity abstraction, but overlay-scoped, account-bound, proprietary control plane. Validation and threat in one |
| **Uncloud rising** | 0→5.2k stars in ~18 months: P2P no-control-plane multi-machine Docker. The closest architectural cousin; proof the decentralized-orchestration window is open and contested |
| **GPUStack** | 5.1k stars, Apache-2.0, Seal Inc. — heterogeneous GPU cluster manager with placement strategies. The one credible rival for AI placement; enterprise-aimed, Python-heavy |
| **exo** | 45k stars — shards one model across devices. Solves a different problem (sharding, not fleet placement); Petals (its predecessor concept) is dead |
| **Avahi decaying** | Linux's mDNS workhorse: last stable release Feb 2020, CVE backlog. Owning the discovery stack in Rust is both necessary and a differentiator |
| **Windows 10 ESU ends Oct 13, 2026** | ~400M capable x86 machines exit support; right-to-repair laws effective 2026 in four US states; repair-café campaigns active. The one dated adoption hook in the landscape |
| **Survivor patterns** | Syncthing (narrow scope + willingness to amputate), Home Assistant (foundation + revenue layer), Coolify (build-in-public sponsorship, solo→profitable team), Jellyfin (ruthless scope focus). The dead — arkOS, Sandstorm, Booklore, TrueCharts — died of maintenance obligation outpacing motivation, usually triggered by platform churn |

## Positioning statement

Zen Garden is, as of mid-2026, the only project attempting to productize the scavenged-hardware fleet —
old laptops, thin clients, Pis, and Android phones treated as a designed-for substrate rather than a DIY
blog stunt — and it occupies that position with real assets, not slideware: a working multi-stone garden
with mDNS discovery and election, a shipped Android stone (native moss on a LineageOS phone,
reboot-surviving, mDNS-correct), a Rust-owned discovery stack at the exact moment the category's incumbent
plumbing (Avahi) is rotting, and a vocabulary system that is genuinely the most production-ready layer of
the project. The landscape research validates four gaps that intersect precisely here:
**scavenged-heterogeneous hardware as product** (empty), **LAN-native service identity** (empty —
Tailscale Services proved demand at the overlay/account layer), **autonomous choreography rather than
management UI** (vacated by Nomad; Komodo/Dokploy/Uncloud all stop at human-decides-placement), and
**VRAM-aware AI placement integrated with general orchestration** (GPUStack the lone credible rival). No
competitor sits at the intersection of even two of these. The catch is equally clear: the position is
currently held by a private feasibility build — zero tags, zero releases, zero CI, public remote stale
since 2026-04-18, a README quickstart that cannot be executed, and a headline connection string whose
parser is consumed only by its own test corpus.

## Ranked strategic opportunities

**1. The Windows-10 exit wave — repurposed-hardware onboarding as the launch vehicle.**
*Why now:* consumer ESU ends October 13, 2026; ~400M capable machines exit support within months, with
right-to-repair laws and repair-café campaigns supplying the narrative. This is the single dated, concrete
adoption hook in the entire landscape, and it expires — that audience decides what to do with those
laptops in Q3/Q4 2026, once. *What it requires:* an honest front door. The fictional quickstart must
become a real, tested "wipe this laptop, become a stone" path; DEPLOY-0001 self-update must merge; a first
tagged release with CI must exist. Nothing new needs inventing — the installer, NewStone provisioning, and
self-update with crash-loop rollback are the project's most battle-hardened code (June's 0.82 fix:feat
ratio is deployment-hardening against real devices). *Who must be beaten:* nobody, directly — CasaOS, the
obvious destination for this audience, is dormant; Umbrel/Runtipi are single-node. The competition is
inertia and Linux Mint.

**2. Claim Nomad's vacated "real orchestration without Kubernetes" position via the autonomy demo.**
*Why now:* Nomad's BSL/IBM orphaning left the position empty; TrueNAS dropping k3s validated the thesis;
the adjacent space (Komodo v2, Dokploy, Uncloud) is crowding fast at the *management-UI* level but nobody
ships autonomous choreography — failure recovery, replica election, automatic placement, zero-config
discovery. Uncloud's trajectory says the window stays open roughly 18 months. *What it requires:*
hardening what exists rather than building: the mongodb orchestrator's check()/reconcile()
single-authority design is correctly layered; the missing pieces are supervision discipline, closing the
unauthenticated-HTTP hole (disqualifying for this positioning), and making orchestrators deployable
through the offering lifecycle instead of Windows .bat scripts. *Who must be beaten:* Komodo and Uncloud —
by demonstrating the thing they architecturally cannot: pull a stone's power cord mid-demo and watch the
replica set heal.

**3. LAN-native service identity — make the headline true.**
*Why now:* Tailscale Services GA is both validation and threat: the adjacent $1.2B player now ships
service abstraction, but scoped to its proprietary overlay and team accounts — not LAN, not zero-config,
not sovereign. Twenty years of prior art (skydock/registrator absorbed, ServiceWeaver archived, Wide-Area
Bonjour dead, Consul gone enterprise-BSL) shows discovery survives only as an integral property of an
orchestration system — which is exactly what Zen Garden has and standalone rivals don't. *What it
requires:* wiring the existing `zen-garden:mongodb/mydb` parser (URI-0003) into an actual resolver
consumed by moss/rake and at least one client path, end-to-end, for one database. This is the README's
first promise and currently its largest fiction. *Who must be beaten:* Tailscale's mindshare, not its
product — the pitch is "service identity without an account, a control plane, or a WAN."

**4. VRAM-aware AI placement at homelab scale — after the succession decision.**
*Why now:* the local-AI tailwind is strong (r/LocalLLaMA ~744k members; rising VRAM floors paradoxically
favor pooling scavenged GPUs); Ollama explicitly lacks multi-node utilization-aware scheduling; exo solves
sharding-one-model, not fleet placement; GPUStack is credible but enterprise-oriented and heavier. The
ollama orchestrator is operationally proven — benchmark runner, fitness matrix, recommendation engine,
gateway self-registration. *What it requires:* the single hardest shed decision in the repo: two full
generations (ollama 14.8k lines, deployed; ai 41.9k lines + dashboard, operationally dormant since
2026-04-12) coexist with no succession statement — 57k lines, ~20% of all project Rust, carried in
indecision. The evidence favors ollama as the present (built, published, documented, deployed) even though
ai is the newer design; whichever is chosen, archive the other and benchmark publicly against GPUStack on
a mixed-GPU fleet. *Who must be beaten:* GPUStack — on footprint, zero-config, and integration with
general service orchestration, not on enterprise features.

## Strategic sheds — what the landscape says not to fight for

| Territory | Owner / graveyard | Zen Garden artifact to shed or cap |
|---|---|---|
| Deploy-from-git PaaS | Coolify (57k), Dokploy (35k) | Never build it; resist template-catalog sprawl |
| Single-node app-store polish | Umbrel, Runtipi; CasaOS's dormancy proves even 34k stars doesn't survive vendor drift | Keep the 51-snippet catalog curated, not a 300-app store |
| Model sharding / disaggregated inference | exo (45k); Petals is the cautionary corpse | The ai crate's ambition; the claim is *placement*, not tensor parallelism |
| General-purpose storage/NAS | MinIO/Garage/SeaweedFS; TrueNAS below | The ~25k-line storage plane (>20% of moss) with non-SigV4 presign and fake WebDAV locks — feature-gate or extract; it anchors quality perception downward |
| Desktop sync client | Syncthing (85k) — whose own maintainer amputated its Android app | Pavilion (6.8k lines, built May 5–6, idle since) — park explicitly |
| Multi-provider AI capability routing | LiteLLM-shaped commodity space | The dormant ai crate's nine-provider matrix |
| WAN-extended discovery | Back to My Mac died of it; Tailscale owns the overlay | Keep pond/mTLS LAN-scoped; do not chase federation |

The pattern across the graveyard (arkOS, Sandstorm, TrueCharts, Booklore) is uniform: death by maintenance
obligation outpacing motivation, usually triggered by platform churn — and Zen Garden's churn exposure
(Docker API, Magisk, Windows services, fwupd, ComfyUI/CivitAI APIs) scales linearly with the surface it
refuses to shed.

## Sustainability risks specific to this project

The structural risk profile currently matches the cautionary tales more than the survivors.

**Solo + AI-amplified volume.** 1,272 commits by one author in 84 active days, 380 Claude-co-authored,
~477k Rust lines added. Booklore (2026) established that "solo maintainer + visible AI-generated volume"
is now a community trust *tax* users actively screen for, and Jellyfin's May 2026 burnout statement names
AI submissions explicitly. Zen Garden's disclosed 46% AI co-authorship (docs/zen-garden-archaeology.md) is
the right instinct — transparency preempts the Booklore failure mode — but disclosure does not neutralize
the volume problem: 284k lines is more than one person can warrant, and reviewers will ask.

**No community surface.** Zero tags, zero releases, zero CI ever; public remote stale since 2026-04-18;
the project's most production-relevant work unpushed on a misnamed branch. There is currently no way for
an outsider to adopt, evaluate, or contribute — the project has positioning but no funnel.

**Scope.** The trait list (orchestration + discovery + storage/S3/WebDAV + replication + AI placement +
companions + desktop app) is wider than arkOS or Sandstorm were at death. The philosophy corpus already
contains the shedding criteria ("add features when real users ask"); the repo stopped applying them.

The survivor patterns prescribe the realistic path: Syncthing's narrow scope plus willingness to amputate;
Coolify's transparent build-in-public sponsorship (solo→profitable small team, no VC); Home Assistant's
foundation model is a later-stage answer, not a year-one one. For Zen Garden specifically: shed to a
legible core, ship public releases, make the AI-assisted methodology a documented, reviewable practice
rather than a discovered liability, and treat the first outside contributor as a strategic milestone, not
an interruption.

## 12-month strategic sequence (Jun 2026 → Jun 2027)

- **Jun–Jul 2026 — Shed before showing.** Archive the ai orchestrator; delete the
  postgresql/valkey/weaviate scaffolds, cloud_filter dead code, and stale branches; consolidate the three
  backup generations onto ORCH-0039 snapshots; park Pavilion with a status note; feature-gate the
  storage/S3/WebDAV plane.
- **Jul 2026 — Security and supervision baseline.** Close the unauthenticated-HTTP hole on :7185 (token
  check or pond-by-default; the deploy endpoint first); move all unsupervised spawns under the task
  supervisor. Non-negotiable before any public adoption push.
- **Jul–Aug 2026 — Become a real open-source project.** Merge the June branch into dev, push, add minimal
  CI (the verification commands in .agentic/CONTEXT.md, plus an orchestrator-crate lane), cut the first
  tagged v0.x release with binaries.
- **Aug–Sep 2026 — Make the front door true.** Rewrite the README quickstart against a real install path,
  tested on an actual retired Windows-10 laptop; wire `zen-garden:mongodb/mydb` end-to-end for MongoDB so
  the headline demo works as written.
- **Sep–Oct 2026 — Launch on the Windows-10 ESU date.** "Your Windows 10 laptop becomes a stone" guide and
  build-in-public posts timed to Oct 13; target r/selfhosted's newcomer influx and the XDA phone-server
  audience (where the Android stone is ahead of everything shipping).
- **Q4 2026 — Convert attention into community.** Release cadence, issue templates, a written
  AI-coauthorship/review policy as a trust feature, contribution norms; success metric: 2–3 recurring
  outside contributors, not stars.
- **Q4 2026–Q1 2027 — The autonomy demo as flagship.** Publish the pull-the-plug demo: 3-stone MongoDB
  replica set self-healing on scavenged hardware; benchmark VRAM-aware model placement against GPUStack on
  a mixed-GPU fleet. These two artifacts are the claims no competitor can copy quickly.
- **Q1–Q2 2027 — Sustainability decision, from evidence.** With adoption data in hand, choose the
  funding/governance posture (Sponsors/donations à la Coolify's early phase; defer foundation talk); hold
  the line on LAN-only scope — the overlay/WAN layer is Tailscale's, and Back to My Mac already died there.
