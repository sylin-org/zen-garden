---
audience: [maintainer]
doc_type: assessment
status: current
date: 2026-06-11
---

> Appendix to the [June 2026 project assessment](README.md): the raw landscape research that
> [strategy.md](strategy.md) synthesizes. Point-in-time data — GitHub figures from the GitHub API on
> 2026-06-11; treat star counts and project statuses as a snapshot.

# Zen Garden Strategic Landscape Research (compiled 2026-06-11)

**Methodology note:** GitHub figures pulled from the GitHub API on 2026-06-11 (stars / last push / archived status). Web sources range from first-party blogs (high confidence) to SEO-farm comparison sites (directional claims flagged).

## 0. Raw GitHub data (API, 2026-06-11)

| Repo | Stars | Last push | Notes |
|---|---|---|---|
| home-assistant/core | 87,650 | 2026-06-11 | healthiest project in the space |
| syncthing/syncthing | 85,209 | 2026-06-09 | |
| coollabsio/coolify | 56,771 | 2026-06-09 | |
| jellyfin/jellyfin | 53,123 | 2026-06-10 | |
| exo-explore/exo | 45,284 | 2026-06-11 | distributed AI inference |
| juanfont/headscale | 39,902 | 2026-06-10 | |
| portainer/portainer | 37,692 | 2026-06-10 | |
| Dokploy/dokploy | 34,752 | 2026-06-09 | |
| **IceWhaleTech/CasaOS** | **34,054** | **2025-08-06** | **dormant ~10 months** |
| k3s-io/k3s | 33,223 | 2026-06-10 | |
| dokku/dokku | 31,926 | 2026-06-10 | |
| louislam/dockge | 23,462 | 2026-04-25 | single-maintainer (Uptime Kuma author) |
| hashicorp/nomad | 16,591 | 2026-06-11 | BSL, IBM-owned, 1,644 open issues |
| caprover/caprover | 15,054 | 2026-05-18 | |
| getumbrel/umbrel | 11,394 | 2026-05-12 | |
| moghtech/komodo | 11,357 | 2026-05-12 | |
| bigscience-workshop/petals | 10,186 | **2024-09-07** | **dead** — distributed inference cautionary tale |
| siderolabs/talos | 10,575 | 2026-06-10 | |
| runtipi/runtipi | 9,466 | 2026-06-11 | |
| sandstorm-io/sandstorm | 7,032 | 2026-06-05 | revival attempt, "sponsored by TestMu AI" |
| k0sproject/k0s | 6,229 | 2026-06-11 | |
| azukaar/Cosmos-Server | 5,955 | 2026-05-26 | effectively single-maintainer |
| psviderski/uncloud | 5,216 | 2026-06-10 | fast riser |
| gpustack/gpustack | 5,140 | 2026-06-11 | closest LLM-orchestration competitor |
| ServiceWeaver/weaver | 4,834 | — | **archived** (Google killed it) |
| beclab/Olares | 4,603 | 2026-06-11 | "sovereign cloud OS" |
| YunoHost/yunohost | 2,920 | 2026-06-08 | (stars split across many org repos) |
| IceWhaleTech/ZimaOS | 2,701 | 2026-04-24 | CasaOS's vendor successor |
| b4rtaz/distributed-llama | 2,946 | 2026-04-14 | |
| kalavai-net/kalavai-client | 217 | 2026-06-09 | P2P GPU pooling, tiny |

---

## 1. Competitive / adjacent landscape

### CasaOS — dormant, vendor pivoted away
- 34k stars but **no pushes to the main repo since August 2025** (GitHub API). IceWhale (ZimaBoard/ZimaCube hardware vendor) has shifted messaging to **ZimaOS** (a CasaOS fork pre-installed on their hardware, 2.7k stars), positioning CasaOS as "community-driven" — i.e., de-prioritized.
- What it did well: prettiest single-node "app store on a NAS" UX, Go-based, lightweight, one-line install. What it never did: multi-node anything, no service abstraction, no AI awareness. License: Apache-2.0.
- **Strategic read:** the largest single-node home-server UI is now a maintenance vacuum — a cautionary tale of vendor-owned OSS, and an audience now shopping for a home.

### Umbrel — hardware-funded, polished, single-node
- 11.4k stars, active. Debian-based umbrelOS, 300+ apps; monetizing via Umbrel Home (N150/16GB) and Umbrel Pro (i3-N300, up to 32TB) appliances (CNX Software, April 2026).
- Strengths: best consumer onboarding in the category, ISO install, app store. Gaps: single-node only, opinionated full-OS takeover, no orchestration, no GPU/AI placement. License: source-available with restrictions on commercial resale — historically a community complaint.

### Runtipi — healthy solo-maintainer project
- 9.5k stars, pushed today. ~200+ default apps; donation-funded solo developer, praised for code quality; one-line install. Single-node, no orchestration. License: GPL-3.0. Solo-maintainer risk applies.

### Coolify — category gravity well for "self-hosted PaaS"
- 56.8k stars; v4.0 (May 2026) added MCP server, 280+ one-click services.
- Sustainability model is the reference case: **no VC; Feb 2025 gross ~$15.7k/mo (Cloud ~$10.5k + donations ~$5.2k), net ~$12.9k/mo; hired a developer + support staff from sponsorships** (Andras Bacsai on X; coolify.io/sponsorships).
- Strengths: deploy-from-git PaaS, multi-server deploys, huge template catalog. Gaps: VPS/developer-deployment oriented, not appliance/zero-config; no discovery layer; heavy (1.2GB idle RAM, ~6% idle CPU per comparisons); no AI/GPU placement; not aimed at scavenged heterogeneous LANs. License: Apache-2.0.

### Dokploy — fastest riser in PaaS
- 34.8k stars (April 2024 launch → 34.8k in ~26 months — steeper curve than Coolify had). Lightweight (0.8% idle CPU), Docker-Swarm-based multi-node. Same audience/gaps as Coolify. License: Apache-2.0 (open core; paid cloud).

### YunoHost — mission twin, aging architecture
- The closest *philosophical* sibling (democratize self-hosting, sovereignty mission). Volunteer-run, donation campaigns active in 2026, NLnet grants. Debian-native packaging (not container-first), single-server, SSO+LDAP done well, ~12 years old. Gaps: no containers as first-class, no multi-node, dated UX. License: AGPL-3.0. Proof that mission-driven projects can persist for a decade on volunteers — but also that they plateau.

### TrueNAS SCALE — retreated from Kubernetes
- 24.10 "Electric Eel" (late 2024) **dropped k3s for plain Docker** for its app system; community verdict: Kubernetes "always felt like overkill for a single-node system"; Docker apps deploy ~3x faster with lower idle CPU (TrueNAS forums/docs). The TrueCharts third-party catalog ecosystem broke in the migration.
- **Strategic read:** the biggest storage-OS vendor concluded that even embedded Kubernetes is too much for home servers — direct empirical validation of Zen Garden's no-k8s thesis.

### Proxmox — the homelab hypervisor default
- Post-Broadcom VMware exodus: PeerSpot mindshare ~16.1% (up from ~10% in 2023); free ESXi discontinued; SMB VMware costs went from ~$15k/yr to $180k+/yr in extreme cases. AGPL-3.0, Vienna-based company, support-subscription model.
- Operates a layer below Zen Garden (VMs/LXC, not service orchestration) — complement, not competitor. Most multi-machine homelabs are "Proxmox cluster + manual everything above it."

### k3s / k0s — Kubernetes' lightweight beachhead
- k3s 33.2k stars, the default "homelab Kubernetes" (single binary <100MB, SQLite, bundled Traefik); k0s 6.2k, smaller community. Apache-2.0, CNCF (k3s).
- What they don't solve: YAML/Helm/cert/ingress cognitive load remains; heterogeneous scavenged hardware (mixed arch, flaky NICs, USB devices) is hostile territory; no app-store UX; GPU support is an ops project in itself. The persistent meme "I replaced my k8s homelab with docker compose and got my weekends back" is the market signal Zen Garden targets.

### HashiCorp Nomad — orphaned mindshare
- BSL 1.1 since Aug 2023; IBM closed the HashiCorp acquisition Feb 27, 2025; full operational integration Sept 2025; product renamings underway.
- Unlike Terraform (→OpenTofu, 300% annual growth, Linux Foundation), **Nomad got no viable community fork**. Repo still active (16.6k stars) but 1,644 open issues and community trust damaged. BSL technically permits homelab use, but the homelab content ecosystem around Nomad+Consul has visibly decayed since 2023.
- **Strategic read:** Nomad was *the* "orchestration without k8s complexity" champion; its license change orphaned that positioning. The mindshare it vacated is exactly Zen Garden's claim.

### Docker Swarm — zombie incumbent
- Swarm mode alive but stagnant: "little meaningful innovation... stayed where it was"; Docker 29+ broke Swarm clusters; legacy volume plugin breakage (virtualizationhowto, March 2026; docker/roadmap#175). Still the fastest cluster bootstrap for compose users; Dokploy and Komodo v2 both build on it — but it's a foundation nobody trusts long-term.

### Tailscale — the adjacent giant, now with Services
- ~$1.2B valuation, 5M+ users early 2026; homelab adoption 41% (2025 survey, up from 18% in 2023 — moderate confidence). Headscale (self-hosted control plane) at 39.9k stars.
- **Critical development: "Tailscale Services" went GA late Feb 2026** — virtual IPs (TailVIPs) + MagicDNS names (`svc:staging-db`) assigned to logical *services* with multiple backend hosts, on all plans. This is service-identity abstraction — but scoped to the tailnet overlay, dependent on a proprietary coordination plane, and aimed at teams, not LAN-zero-config appliances.

### Others worth tracking
- **Komodo** (11.4k stars): Rust+TS multi-server Docker/Swarm management, v2 in 2026 with PKI auth, 2FA, Swarm management. The most direct "manage containers across many boxes without k8s" rival; no discovery abstraction, no AI placement, management UI rather than autonomous choreography.
- **Uncloud** (5.2k stars, fast-rising): "multi-machine Docker Compose for production" — WireGuard mesh, **no control plane, P2P-synced cluster state, automatic service discovery + ingress**; 2026 roadmap includes first-class databases and replicated volumes. **The closest architectural cousin to Zen Garden's decentralized model** — but VPS/production-web oriented, solo-maintained, no mDNS/LAN-zeroconf, no hardware heterogeneity story, no AI.
- **Olares** (4.6k stars, active): "sovereign cloud OS" — k8s-based, KubeSphere lineage; sovereignty marketing overlaps Zen Garden's audience but with full Kubernetes underneath.
- **Cosmos Cloud** (6k stars): security-first single-node server with built-in reverse proxy/auth; effectively one maintainer.
- **Dockge** (23.5k stars): compose-stack manager from the Uptime Kuma author; beloved, single-maintainer, slowing cadence (last push Apr 2026).

---

## 2. Gap verification — what nobody does well

### (a) Multi-node orchestration at homelab scale without k8s — **gap is real but actively closing**
The space between "docker compose on one box" and "k3s cluster" is the most contested frontier in self-hosting right now: Dokploy (Swarm-based), Komodo v2 (Swarm management), Uncloud (P2P mesh, no control plane), Portainer (Swarm GUI) all attack it. However: all are *deployment/management* tools where a human decides placement. **None do autonomous choreography (failure recovery, replica-set election, automatic placement) with zero-config discovery.** Nomad's abdication (BSL/IBM) left "real orchestration without k8s" without a credible open champion. Verdict: real gap at the *autonomy* level; crowded at the *management UI* level. The 18-month window matters — Uncloud's trajectory shows someone else sees it.

### (b) Heterogeneous / scavenged hardware as first-class concept — **genuinely unfilled**
No product treats "a 2014 laptop + a thin client + a Pi + an old Android phone" as a designed-for fleet. Evidence the *demand* exists: XDA running multiple 2025–2026 pieces on phones-as-servers, Hackaday covering phone clusters, postmarketOS k3s clusters running llama.cpp and Jellyfin. But it's all DIY blog culture — no platform productizes mixed-arch, mixed-age hardware with capability-aware placement. balena targets commercial IoT fleets; k3s tolerates ARM but doesn't *embrace* heterogeneity. Verdict: **real white space**, and Zen Garden's Android-stone work is ahead of anything shipping.

### (c) Distributed local-AI placement across mixed GPUs — **partially filled; placement-at-homelab-level still open**
- **exo** (45.3k stars, active; revived with DGX Spark + Mac Studio disaggregated-inference demos, 2.8x latency gains): shards *one model* across heterogeneous devices. About making big models fit, not fleet-level service placement, reliability, or VRAM-aware multi-model scheduling.
- **GPUStack** (5.1k stars, Apache-2.0, Seal Inc.): the closest direct competitor — GPU cluster manager with auto-discovery on heterogeneous hardware (NVIDIA/AMD/Apple), placement strategies (binpack/spread), orchestrates vLLM/SGLang/llama-box. Aimed at enterprise GPU clusters, Python-based, heavier; not integrated with general homelab service orchestration.
- **Petals dead** (no commits since Sept 2024); **llm-d** is Red Hat/Google Kubernetes-native datacenter tech; **llama.cpp RPC** is an active DIY corner but raw plumbing; **Ollama explicitly lacks utilization-aware multi-GPU/multi-node scheduling** (open issue #11810); kalavai (217 stars) is negligible.
- Verdict: model-sharding is solved-ish (exo, llama.cpp RPC); **VRAM-aware *placement* of models across a mixed fleet, integrated with general service orchestration and a recommendation layer, is not shipped by anyone at homelab scale.** GPUStack is the one to watch/benchmark against.

### (d) Service-identity abstraction ("ask for the service, not the machine") — **largely open at the LAN/zero-config level**
- mDNS today resolves *hosts* (`stone.local`), not capability-level services with failover. Docker's internal DNS is single-cluster-internal. Consul does true service catalogs but is BSL, IBM-owned, and enterprise-weight.
- **Tailscale Services (GA Feb 2026)** is the strongest prior art: `svc:name` + virtual IP + multiple backends — but requires the Tailscale control plane, accounts, and an overlay network; not LAN-native zero-config; control plane proprietary (Headscale lags new features).
- Historical attempts at host-transparent service connectivity died: skydock/skydns/registrator superseded by ~2016; Google's ServiceWeaver **archived**; Apple's Wide-Area Bonjour / Back to My Mac discontinued 2018-2019.
- Verdict: the specific `zen-garden:mongodb/mydb` URI-level abstraction on a LAN with autonomous failover **has no shipping equivalent**. Closest analogues operate at the wrong layer (overlay VPN) or wrong audience (enterprise service mesh).

---

## 3. Macro trends

### (a) Local-first AI / consumer-GPU LLM hosting — strong tailwind
r/LocalLLaMA ~744k members; ollama/ollama at 173.8k stars; open-weight models (Qwen 3.5, Llama 4 Scout, Kimi K2.5, Nemotron) now rival proprietary APIs on many tasks; self-hosting economics: RTX 4090 breakeven ~8 months at sustained usage, 5-10x cheaper over 2 years for steady load. Counterpoint: VRAM floor rising (16GB now "basically minimum usable") — which paradoxically *strengthens* the case for pooling multiple scavenged GPUs.

### (b) Data sovereignty / EU cloud repatriation — strong and accelerating
EU Commission weighing restrictions on US cloud platforms for sensitive government data (CNBC, May 2026); €180M sovereign-cloud procurement awarded (EC, April 2026); "Tech Sovereignty Package" expected; NIS2 audits due June 2026; CLOUD-Act "residency illusion" now mainstream compliance language; EU self-hosting boom explicitly attributed to residency law. Enterprise/government-centric, but sets cultural sentiment for prosumers and small teams — Zen Garden's stated audience.

### (c) E-waste / right-to-repair — strong narrative alignment with a hard date
Windows 10 EOL (Oct 14, 2025; consumer ESU bridge ends **Oct 13, 2026**) strands ~400M PCs that can't run Windows 11; estimated 1.06B pounds of e-waste (PIRG, 404media); Restart Project / repair-café campaigns explicitly pushing reuse; right-to-repair laws for consumer electronics effective Jan 1, 2026 in CO/NV/OR/WA. **A few hundred million capable x86 laptops are exiting Windows support in the next four months — the single best-timed adoption hook for a "repurposed hardware fleet" product.**

### (d) Small-web / self-hosting renaissance — real, broadening
r/selfhosted ~650k weekly visitors; 2024 survey: 97% container usage; market projections ~$85B by 2034 at 18.5% CAGR (low confidence, directionally consistent). 2026 commentary emphasizes *newcomer influx* (first Immich installs, beginner-friendliness) — the "first-time builder" persona is the growth segment.

### (e) AI-assisted development and the solo maintainer — double-edged
Upside: AI tooling measurably extends what one person can ship and maintain. Downside, now with case law: **Booklore** — 10k-star self-hosted ebook platform whose solo dev was found shipping ~20k-line AI-generated PRs, faced community backlash, and **deleted repo/Discord/website overnight in early 2026** (community forked as Grimmory) (XDA). **Jellyfin's May 2026 "State of the Fin" explicitly names "increased support requests, combined with the AI code submissions" as a cause of team burnout.** Implication: AI leverage is real but "single-maintainer + AI-generated code volume" is now a *trust liability* users actively screen for.

---

## 4. Cautionary tales and survivor patterns

### Died / stalled
- **Sandstorm**: technically brilliant (capability-security app platform), failed as startup 2017; only ~500 self-hosted servers ever on Sandcats; founder described maintenance as "a chore," stuck on MongoDB 2.6 because auto-update obligations made migrations impossible. Lessons: revenue model coupled to hosted service that died; *update obligations are a permanent tax*; ahead-of-market positioning didn't save it.
- **arkOS** (2012–2015): personal-server distro, discontinued — "more ambitious than there are developers contributing" (Phoronix). The canonical scope-sprawl death.
- **Booklore** (2026): solo maintainer rage-quit, zero deprecation path.
- **Syncthing-Android** (Dec 2024): discontinued by its (single) maintainer — Google Play friction plus "no active maintenance... not enough motivation"; community fork took over. Even healthy projects shed limbs where bus-factor = 1.
- **CasaOS** (2025–26): vendor attention moved to ZimaOS; 34k-star repo without pushes for 10 months. Vendor-owned OSS decays when the vendor's hardware roadmap moves.
- **Petals** (2024): distributed-inference BitTorrent-style network; activity ceased — volunteer compute networks without an operator economy don't self-sustain.
- **TrueCharts** (2024-25): third-party catalog destroyed by platform substrate change (TrueNAS k3s→Docker) — ecosystem risk when you build on someone else's app-platform layer.
- **ServiceWeaver** (Google, archived 2025): even well-funded service-abstraction frameworks die without adoption.
- **Avahi**: see §5 — the infrastructure the category depends on is itself under-maintained.

### Survived — and why
- **Home Assistant / Open Home Foundation**: 87.7k stars, 2M+ tracked installs; nonprofit foundation owns 250+ projects, commercial partners contractually contribute majority of profit from licensed products; ~50 full-time employees funded mainly by Nabu Casa cloud subscriptions. Pattern: *foundation governance + recurring-revenue cloud convenience layer + un-sellable structure*.
- **Jellyfin**: 53.1k stars; volunteer-run, 3,679 PRs merged in past year; 5-year donation surplus ("does not actually need financial contributions"); survives via fork-origin community legitimacy (Emby relicensing) and ruthless scope focus (media only). Currently strained by support load + AI submissions.
- **Syncthing**: 85.2k stars; survives via narrow scope (one protocol, one job), corporate steward (Kastelo) employing core devs, and willingness to amputate (Android app).
- **Coolify**: solo→small-team via transparent build-in-public sponsorship + cheap cloud offering; profitable without VC.
- **Proxmox / YunoHost**: support-subscription company and grant-funded volunteer collective respectively — both >12 years old, both unglamorous and alive.

**Survivor pattern summary:** (1) narrow, legible scope; (2) a revenue or foundation structure decoupled from one person's motivation; (3) community legitimacy (fork-resistance); (4) explicit governance before the crisis, not after. The killer of the dead ones was almost never code quality — it was *maintenance obligation outpacing motivation/funding*, frequently triggered by platform churn (Google Play, MongoDB, TrueNAS substrate).

---

## 5. mDNS / zeroconf prior art

- **Bonjour/DNS-SD (Stuart Cheshire, Apple)**: announced 2002 (Rendezvous), RFCs 6762/6763 published 2013; massive success *at the device/printer layer* — 250+ service types advertised, every network printer ships it. The lesson: zeroconf wins when it's invisible plumbing under a product experience, not a user-facing system.
- **Wide-Area Bonjour / Back to My Mac**: Apple's attempt to extend service discovery beyond the LAN; Back to My Mac killed in macOS Mojave (announced Aug 2018). NAT/router dependency (UPnP/NAT-PMP) made it unreliable; superseded internally by iCloud relays. Lesson: LAN-scope mDNS is robust; WAN-scope mDNS extension historically fails — pond/mTLS layering should not assume multicast beyond the LAN.
- **Avahi**: the Linux mDNS workhorse is **effectively unmaintained** — last stable 0.8 (Feb 2020), 0.9 stuck at rc-stage with unfixed-then-patched CVEs and an open "release to fix all CVEs since 2021?" issue (avahi#503, #325). systemd-resolved offers partial mDNS/DNS-SD overlap. Strategic note: owning the discovery stack in Rust (rather than depending on Avahi) is the defensible choice; also a documentation/compat hazard since most distro mDNS troubleshooting assumes Avahi.
- **Wi-Fi Aware (NAN)**: Android since 8.0; Apple added support in iOS 26 SDK (late 2025); Windows absent with no roadmap. Proximity-P2P, phone-centric; not a homelab substrate yet, but relevant to phone-stones long-term.
- **Docker-era service discovery wave (2014–2016)**: skydock, SkyDNS, registrator — all superseded when Docker shipped native DNS; none survived as independent projects. Lesson: discovery as a *standalone add-on* gets absorbed by the platform; discovery survives only as an integral property of an orchestration system (Consul survived because Nomad/service-mesh needed it).
- **Consul**: the only durable service-catalog product — now BSL + IBM, enterprise-weight, never penetrated homelab beyond Nomad enthusiasts.
- **No prior project found that shipped "service-oriented mDNS discovery for homelab" as a product.** The pieces existed for 20 years; the combination Zen Garden attempts (capability-level naming + autonomous placement + LAN zeroconf) has no direct failed predecessor — the nearest analogues failed at WAN extension (Apple) or got absorbed (Docker add-ons) or went enterprise (Consul).

---

## 6. Synthesis-ready observations

1. **Validated gaps:** scavenged-heterogeneous-hardware-as-product (empty), LAN-native service-identity abstraction (empty; Tailscale Services is the overlay-network analogue), autonomous homelab choreography (empty; management UIs crowd the adjacent space), VRAM-aware fleet AI placement (GPUStack is the credible competitor; exo solves a different problem).
2. **Closing windows:** Uncloud's trajectory (0→5.2k stars in ~18 months, P2P no-control-plane design) proves the decentralized-orchestration idea is in the air; Tailscale Services GA shows the giant adjacent player is moving toward service abstraction.
3. **Best-timed hook:** Windows 10 ESU expiry Oct 2026 + right-to-repair laws = a concrete, dated e-waste/reuse narrative.
4. **Biggest structural risk mirrors the cautionary tales:** the trait list (orchestration + discovery + storage/S3 + replication + LLM placement + MongoDB choreography + companions + Android stones) is wider in scope than arkOS or Sandstorm were — and they died of scope. Survivors won by narrow scope plus governance/revenue structure. The AI-assisted-solo-maintainer leverage argument is real but now carries a community-trust tax (Booklore, Jellyfin burnout).
5. **Dependency caution:** the category's foundational Linux mDNS layer (Avahi) is itself a maintenance hazard; owning the discovery stack in Rust is both necessary and a differentiator.
