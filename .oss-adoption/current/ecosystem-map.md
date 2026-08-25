Status: complete
Run ID: 20260719T034246Z
Project: zen-garden
Created: 2026-07-19T03:42:46Z
Verified at (UTC): 2026-07-19T05:10:00Z

# Ecosystem map

## Market niche

Zen Garden belongs between polished single-machine home-server platforms and production cluster orchestrators:

> **A local-first service-intent and continuity layer for a small, heterogeneous fleet of repurposed machines—able to deploy, adopt, or reference services; select hardware; attach lifecycle/storage policy; and preserve a device-independent service identity.**

The buyer/user is not looking for another Docker GUI. They have several imperfect boxes and a mix of services already managed by different tools. They want the garden to answer: what can run here, where is the service now, who owns its lifecycle, is it actually healthy, and what should happen when its runtime disappears?

The closest direct peer is [Uncloud](https://uncloud.run/docs/). The closest overlap with Zen's device-independent naming story is [Tailscale Services](https://tailscale.com/docs/features/tailscale-services). The category is crowded, so Zen should not claim novelty for multi-machine Docker, health-aware discovery, stable names, placement, or app catalogs individually. Its defensible combination is mixed-fleet intent, explicit managed/adopted/borrowed ownership, hardware-aware domain orchestration, physical companions, and bounded continuity without requiring Kubernetes or a hosted account.

## User workflow and adjacent systems

The ideal workflow is collaboration-first:

1. An OS/home-server layer owns the machine, packages, accounts, and perhaps ingress.
2. Docker/Compose or another execution backend owns the container primitives.
3. An existing UI/GitOps tool may deploy or inspect the container.
4. Zen records one explicit lifecycle owner, discovers/adopts the service, attaches semantic capability and hardware/storage intent, and exposes it by stable garden identity.
5. A network layer such as Tailscale carries remote traffic; a resolver/exporter publishes only healthy Zen endpoints.
6. A replication transport such as Syncthing may move file-shaped state; Zen's separately tested snapshot contract determines recoverability.
7. A workload engine such as Ollama serves requests; Zen adds cross-Stone placement, warmup, model distribution, and a stable gateway.

The hard invariant is **one mutating reconciler per service**. Portainer, Komodo, Coolify, Uncloud, Cosmos, or a home-server app manager may own deployment while Zen uses `adopted-read-only` or `borrowed`. Zen uses `managed` only when it owns create/update/restart/delete.

## Comparable projects

| Project | What the official product currently does | Relationship to Zen Garden | Honest differentiation/collaboration boundary |
| --- | --- | --- | --- |
| [Uncloud](https://uncloud.run/docs/) | Decentralized multi-machine Docker cluster with WireGuard, internal DNS names, Compose, rolling deployments, health/restarts, scaling, load balancing, Caddy HTTPS, and persistent storage. | **Closest direct peer.** | Uncloud has the clearer general multi-node Compose/network story. Zen can differentiate through explicit service ownership modes, curated capabilities, hardware semantics, companions, and service-specific orchestrators. Prototype an Offering→Uncloud Compose backend and adopt Uncloud services. |
| [Komodo](https://komo.do/docs/intro) | Core plus Periphery agents manage servers, containers, Compose/Swarm, builds, procedures, secrets, monitoring, RBAC/audit, API, and declarative resource sync. | Near-direct operator-plane peer. | Komodo is DevOps/GitOps-oriented; Zen is LAN identity/continuity and service-intent-oriented. Let Komodo deploy Moss or call Moss APIs; map Komodo tags to Stone capabilities. Do not claim Komodo lacks orchestration. |
| [Portainer](https://docs.portainer.io/) | Manages Docker, Swarm, Kubernetes, and multiple environments through server/agent architecture, APIs, and stack sources. | Adjacent infrastructure control plane. | Use standard Docker labels so Portainer can inspect/install Moss while Zen supplies offering semantics and discovery. Some Portainer governance features are edition-dependent. |
| [Coolify](https://coolify.io/docs) | Self-hosted PaaS for Git/image/Compose deployment over SSH, services, TLS, backups, webhooks, APIs, and multi-server/Swarm paths. | Adjacent developer delivery plane. | Coolify can build/deploy; Zen can adopt and name the service and add hardware/lifecycle intent. Declare one owner. Coolify's same-app multi-server feature is documented as experimental; do not reduce it to a single-server app store. |
| [Cosmos Cloud](https://cosmos-cloud.io/) | App store/container management plus reverse proxy, HTTPS, SSO/MFA, monitoring, backups, and Constellation multi-server mesh/private DNS. | Adjacent home-server/gateway platform. | Cosmos can own north-south proxy/auth/VPN while Zen adopts ServApps and supplies garden intent. Note Cosmos's Apache-2.0 plus Commons Clause licensing accurately. |
| [K3s](https://docs.k3s.io/) | Fully conformant, lightweight Kubernetes for edge, homelab, IoT, SBC, and air-gapped use; bundles runtime, CoreDNS, Traefik, ServiceLB, and storage components. | Alternate execution substrate. | A future backend could translate Offerings to Kubernetes resources and Stone capabilities to node labels. Do not call K3s inherently heavyweight; Zen's distinction is avoiding the Kubernetes operating model for a narrower small-fleet job. |
| [Consul](https://developer.hashicorp.com/consul/docs/discover) | Replicated catalog, health checks, DNS, prepared queries, and healthy-instance routing across runtimes. | Mature discovery complement/alternative. | Export healthy Offerings with tags/checks or use Consul DNS as a resolver. Health-aware service identity is not unique to Zen. |
| [Tailscale Services](https://tailscale.com/docs/features/tailscale-services) | MagicDNS service names/TailVIPs route to one or more approved backend hosts with ACLs, steering, approval, and draining. | Closest identity-layer overlap and strong complement. | Zen should advertise/drain ready Offerings while retaining lifecycle/data ownership. Tailscale owns authenticated network reachability across subnets; Zen owns local intent, placement, and recovery. |

## Home-server platforms

| Platform | Current strength | Collaboration surface | Positioning discipline |
| --- | --- | --- | --- |
| [Runtipi](https://runtipi.io/docs/learn/apps-and-app-store) | Single-server Docker manager, curated/custom app stores, Traefik routing, persistent data, backups, and updates. | Its Compose-like app definitions are a high-value import/export format; package Moss as a host recipe and adopt Runtipi-managed apps. | Do not call it only a dashboard. Zen starts when more than one box/service owner matters. |
| [CasaOS](https://casaos.zimaspace.com/) | Docker-centered personal-cloud UI with app store, custom app import, and a product path that now points users toward ZimaOS. | Import Compose/AppFile metadata or adopt containers without stealing lifecycle ownership. | Describe the official transition; do not call CasaOS dead. |
| [Umbrel](https://umbrel.com/support/getting-started/what-is-umbrel) | Polished home-server OS with files, shares, backups, hundreds of apps, and a Compose-based [App Framework](https://github.com/getumbrel/umbrel-apps). | Offer a Moss/Portainer recipe, consume exported connection variables, and adopt Umbrel apps. | Umbrel's repository uses PolyForm Noncommercial; do not call it permissively open source. |
| [YunoHost](https://doc.yunohost.org/admin/what_is_yunohost/) | Debian server OS with 500-plus apps, LDAP/SSO, domains, mail, and backup/restore; its app packaging intentionally does not use Docker. | Adopt YunoHost-managed services read-only; optionally expose a redirect tile. | Treat non-container lifecycle ownership as deliberate, not obsolete. |
| [Cosmos Cloud](https://cosmos-cloud.io/docs/servapps/) | Secure app/gateway experience with ServApps, proxy, and operational controls. | Exchange container labels/routes and let Cosmos own public ingress/auth. | Do not claim multi-server connectivity is absent. |

These products compete for beginner mindshare but are also the best hosts and catalog sources. Zen's useful message is: **keep the UI you like; connect and preserve services across several boxes.**

## Complements and integrations

- **[Docker Compose](https://docs.docker.com/compose/): workload lingua franca.** Make import/export deterministic, preserve services/networks/volumes/health checks, support GPU/architecture requirements, and define `io.zen-garden.*` ownership/capability labels. Zen adds garden-wide semantics; it does not reinvent multi-container YAML or Compose-local DNS.
- **[mDNS/DNS-SD](https://www.rfc-editor.org/info/rfc6763/) and [Avahi](https://avahi.org/): standards substrate.** Publish standards-compliant records and document unicast-DNS bridging. `.local` discovery is link-local by specification; custom UDP should not be the only integration route.
- **[Tailscale Services](https://tailscale.com/docs/features/tailscale-services): remote reachability exporter.** Register only application-ready endpoints, drain before restart/update/migration, remove stale endpoints, and reflect ACL/tag requirements without requiring Tailscale for LAN use.
- **[Consul](https://developer.hashicorp.com/consul/docs/discover): optional service catalog backend.** Export capability/Stone tags and health checks; optionally resolve `zen-garden:` intent through Consul DNS/prepared queries.
- **[Syncthing](https://syncthing.net/): replication transport.** A Seed Bank driver can use its [REST/events API](https://docs.syncthing.net/dev/rest.html) and receive-only folders for file-shaped data. Keep independent snapshots because Syncthing's own FAQ warns that synchronization is not backup.
- **[Ollama](https://docs.ollama.com/api/introduction): workload engine.** Ollama owns inference, model APIs, and intra-host scheduling. Zen should prove cross-Stone capability placement, model warmup/replication, a stable OpenAI-compatible endpoint, explicit failover semantics, and local-only policy. Modern Ollama also offers cloud models, so avoid calling it universally local-only.
- **Reverse proxies and SSO:** Traefik, Caddy, Cosmos, Authelia, or the host platform should own north-south TLS/auth unless Zen adopts that responsibility explicitly. Export routes and readiness; do not silently create competing ingress.

## Registries, catalogs, and host ecosystems

The integration priority is contract compatibility, not a proprietary mega-store:

1. Compose Specification plus documented Zen labels.
2. OCI images in GHCR as source of truth and Docker Hub as a discovery mirror, using identical immutable digests.
3. Runtipi/Umbrel/CasaOS importers or generated host recipes.
4. Uncloud execution adapter.
5. Tailscale Services and DNS-SD exporters.
6. Optional Consul registry exporter and Syncthing replication driver.
7. A Kubernetes/k3s backend only after the Docker ownership model and readiness contract are stable.

Every imported catalog item should retain upstream source, license, image digest, architecture, health contract, persistence boundary, last verification date, and Zen maturity classification. Do not fork app definitions invisibly.

## Communities and trusted curators

The highest-fit discovery community is the practical self-hosting/homelab ecosystem: GitHub users, selfh.st/Self-Host Weekly readers, r/selfhosted, r/homelab, and technically curious Show HN readers. Rust communities are secondary and should receive architecture/contributor material, not generic product copy. Docker/Compose, Ollama, Runtipi/Umbrel/CasaOS, Uncloud, Tailscale, and Syncthing maintainers are potential interoperability reviewers—not targets for unsolicited mass promotion.

The best curator story is a tested release plus a two-minute heterogeneous-hardware recovery demo and a precise “works with” matrix. Curators should not be asked to repeat unproven HA/security claims.

## Strategic openings

1. **Own the heterogeneous-garden wedge.** Show a thin client, an old laptop, and a GPU desktop doing different jobs based on capability—not a row of identical cloud VMs.
2. **Make adoption/borrowing first-class.** Most homelabs already contain services. A read-only semantic layer across Portainer/Coolify/Runtipi/Umbrel/YunoHost is more credible than demanding migration.
3. **Turn service identity into an interoperable layer.** Put `zen-garden:` intent above DNS-SD, Tailscale Services, and Consul rather than beside them.
4. **Prove one bounded continuity loop.** Intent → real health → endpoint → missing-container reconstruction is a stronger launch than generic “self-healing” language.
5. **Use service-specific orchestration where generic schedulers stop.** Ollama model/VRAM behavior and MongoDB topology can be defensible after published failure/benchmark evidence.
6. **Connect software to place.** Cricket/Firefly/presence can make a physical homelab legible, a genuinely distinctive but secondary extension story.
7. **Make lifecycle ownership inspectable.** A user should always see whether Zen, another tool, or a human owns mutations and backups.

## Risks and dependencies

- **Direct-peer risk:** Uncloud already presents a crisp decentralized multi-machine Compose/network/rolling-deploy story. Zen must not launch with a vaguer, broader promise.
- **Trust risk:** missing legal/release/security contracts can overwhelm the differentiation.
- **Scope risk:** 11 workspace crates, four orchestrators, UIs, firmware, storage, security, and several platforms exceed an unstated support budget.
- **Double-reconciliation risk:** integrations can cause destructive races unless lifecycle ownership is explicit and enforceable.
- **Data-loss risk:** sync, snapshot, migration, and service-specific HA must retain separate semantics and proof.
- **Dependency risk:** a clean build currently requires an unpinned sibling Koi checkout.
- **Catalog risk:** copied app definitions drift from upstream and can obscure licenses/secrets/persistence requirements.
- **Network-model risk:** mDNS is link-local, while remote meshes and routed networks require explicit security and discovery bridges.
- **Searchability risk:** “Zen Garden” is a common phrase; package/image namespaces, repository description, topics, and a stable subtitle must carry discovery.
- **Positioning risk:** calling the project an OS, PaaS, Docker dashboard, VPN, sync engine, or Kubernetes replacement invites comparisons it should not try to win.

The central discipline is: **Zen Garden coordinates the service's life across a garden; another tool may own the OS, deployment primitive, network path, file transfer, ingress, or inference engine.**
