---
audience: [developer, ai]
doc_type: decision
status: accepted
date: 2026-05-10
supersedes: [DNS-0001]
---

# DNS-0002: Remove `.zengarden` Zone — mDNS Is the Discovery Layer

**Date**: 2026-05-10
**Status**: Accepted
**Supersedes**: [DNS-0001](DNS-0001-local-zone-from-mdns-cache.md) (`.local` from mDNS cache — moot once Koi DNS is removed)

## Context

Each stone runs an embedded Koi DNS server (port 5642) that serves an authoritative `.zengarden` zone for application-level service names. systemd-resolved is configured at moss startup to forward `~zengarden` queries on the Docker bridge link to that server. Containers receive `dns_search: ["zengarden"]` so bare names resolve under the zone.

This architecture was inherited from a presentation-layer goal: let workloads reference services using familiar `host.zone` syntax — `mongo.zengarden:27017` rather than `_mongodb._tcp.local` browsing.

### Discovery findings

A grep over the entire workspace shows:

- **No code constructs a `.zengarden` URL.** `OfferingFqn::announcement_name` returns `"searxng.prod"` with no zone suffix. The doctable showing `searxng.prod.zengarden` is aspirational.
- **No code calls `lookup("…zengarden")`.** Stone resolution in [rake/connection/resolution.rs:282](src/rake/src/connection/resolution.rs#L282) uses mDNS via `format!("{}.local", …)`. The example URI in [rake/context.rs:246](src/rake/src/context.rs#L246) is `mongodb://stone-01.local:27017`.
- **No code publishes records into Koi DNS.** No `add_entry()` call exists outside tests. The authoritative `.zengarden` zone is empty.
- The `.zengarden` zone is **fully built infrastructure serving zero records to zero consumers.**

Meanwhile, the same architecture caused a production failure on `stone-golden-summit`: Koi DNS at `127.0.0.1:5642` became the system's global resolver via the docker0 link configuration, but its upstream forwarder inherited a broken `/etc/resolv.conf` chain. Every public DNS lookup returned SERVFAIL, every Docker image pull failed, and `garden-rake offer` reported `[pending create]` while the install died silently in the background. The root cause was structural: moss owns the host's DNS chain, so any breakage in that chain breaks moss.

### What problem `.zengarden` was solving

DNS-based service discovery for unmodified workloads — the same problem Kubernetes solves with `cluster.local`. The intent was that a containerized app could `mongo.connect("mongodb")` and have it resolve to whichever stone currently runs MongoDB.

### Why that problem doesn't need `.zengarden`

mDNS already provides:

| Capability | Provided by |
|---|---|
| Stone hostname resolution (`stone-01.local`) | OS resolver + mDNS responder (avahi/Bonjour/mDNSResponder) |
| Service-instance discovery (`_mongodb._tcp.local`) | mDNS service browse via DNS-SD (RFC 6763) |
| Cross-process address book | The mDNS browse cache (which Koi DNS was already wrapping) |
| Per-instance metadata | mDNS TXT records |

Modern OS resolvers translate mDNS into the libc `gethostbyname` path natively. Linux: `systemd-resolved` with `MulticastDNS=resolve` (a one-line drop-in). macOS and Windows: built-in. **The translation-from-mDNS-to-DNS layer that Koi DNS provides was already solved by the OS in every supported environment.**

For containers — which historically lack mDNS in their netns — pointing the container's resolver at the host bridge (already done at [docker/mod.rs:157](src/moss/src/docker/mod.rs#L157)) makes the host's resolver, with its mDNS support, perform the lookup on the container's behalf. No additional layer is needed.

### What `.zengarden` was *not* solving

- **Cross-subnet discovery.** Each stone runs its own Koi DNS, populated from its own mDNS browse cache. mDNS is link-local; the browse cache only contains stones reachable by multicast. So `.zengarden` lookups fail across subnets exactly the same way mDNS lookups do. The "zone" name promised something the implementation could not deliver.
- **Replicated authoritative records.** No mechanism exists to gossip Koi DNS state across stones.

## Decision

Remove the `.zengarden` zone and the Koi DNS subsystem from moss. Use mDNS as the sole stone-and-service discovery layer.

### Architecture after

| Concern | Mechanism |
|---|---|
| Stone reaches stone | mDNS `.local` via OS resolver (`stone-X.local`) |
| Container reaches stone | Container's DNS points at Docker bridge gw → host resolved → mDNS `.local` |
| Host resolves public names | OS-managed `/etc/resolv.conf` (DHCP, NetworkManager, dhcpcd, whatever the distro provides) |
| Service discovery | mDNS service browse (`_mongodb._tcp.local`, `_certmesh._tcp`, etc.) — already published by koi-mdns |
| Service-name "URL" semantics | Deferred; not implemented in any form today |

moss writes nothing into `/etc/systemd/resolved.conf.d/`, runs no DNS server, and does not configure `resolvectl`. The host's network stack is not its responsibility.

### Future: if DNS-based service-name resolution is wanted later

It can be reintroduced incrementally without resurrecting `.zengarden`:

- Lazy: start a DNS server on first `add_entry()` call rather than at boot.
- Mechanism-agnostic: container env-var injection (`MONGO_HOSTS=stone-a.local:27017,stone-b.local:27017`) is simpler and works without touching host DNS at all.
- Whichever path is chosen at that point will be a clean greenfield decision rather than an extension of an unused production-time DNS subsystem.

## Consequences

### Positive

- moss no longer touches the host's DNS chain. The class of "stone DNS broken on boot" bugs becomes structurally impossible.
- ~300 LOC removed from moss + koi (Koi DNS startup, `configure_resolved_for_containers`, the `discover_dns_upstreams` probe ladder, the `upstream_servers` API, container `dns_search`).
- No more conflicts with system network managers (dhcpcd, NetworkManager, openresolv).
- One fewer port listener (no `:5642`).
- Stones survive any host-DNS configuration drift without moss intervention.

### Negative

- Loses the conceptual "service-as-URL" presentation (`mongodb.zengarden:27017`). No code consumed it, but the convention is gone from documentation and naming intent.
- Future service-name DNS resolution must be re-designed from scratch rather than extending an existing subsystem.
- Containers that *did* rely on the `dns_search: ["zengarden"]` shortcut (none in the current codebase, possibly some user-deployed offerings) lose it. Those workloads should use full hostnames or env-injected addresses.

### Neutral

- mDNS limitations are unchanged: link-local, no cross-subnet without a reflector. `.zengarden` had the same limit, just less obviously.
- DNS-0001's `.local` zone work (serving `.local` from Koi's mDNS cache for Windows-host containers) becomes moot; the platform-specific Windows issue it addressed re-surfaces. Mitigation: the affected containers (Linux containers on Windows Docker Desktop hosts) should use the host gateway IP rather than the host's `.local` name. Documented as an operational caveat, not a moss responsibility.

## Scope

### moss (zen-garden)

- [src/moss/src/bootstrap/run.rs](src/moss/src/bootstrap/run.rs):
  - Remove `discover_dns_upstreams` and `parse_resolv_conf_upstreams` helpers.
  - Remove `let dns_upstreams = …` and the closure capture.
  - Remove `.dns_enabled(true)`, `.dns_auto_start(true)`, and the `.dns(|cfg| …)` block on the koi-embedded builder.
  - Remove the Phase 7.1 call to `configure_resolved_for_containers`.
  - Remove `configure_resolved_for_containers` (Linux + non-Linux variants) entirely.
  - Remove the Phase 7.2 reconciliation block that patches existing managed containers' `/etc/resolv.conf`.
- [src/moss/src/docker/mod.rs](src/moss/src/docker/mod.rs): drop `dns_search: vec!["zengarden".to_string()]` from `container_networking`. Update the docstring.
- [src/moss/src/docker/spec.rs](src/moss/src/docker/spec.rs): the `dns_search` field on `ContainerNetworking` becomes vestigial; remove if no other consumers, otherwise leave as `Vec::new()`-only.
- [src/moss/src/domain/announcement.rs](src/moss/src/domain/announcement.rs): drop `.zengarden` from the docstring table; the function itself stays — mDNS announcement names are still useful.
- [src/common/src/constants/mod.rs](src/common/src/constants/mod.rs): remove `KOI_DNS` constant.
- [src/common/src/offerings.rs](src/common/src/offerings.rs): drop the `.zengarden` column from the `announcement_name()` doctable; show only the FQN→announcement mapping.

### koi (sibling repo)

- Revert `feat(koi-dns): expose explicit upstream nameservers in DnsConfig` (commit `4add29a`). The `upstream_servers` field had a single consumer in zen-garden, which is going away. Other koi consumers can re-add it if needed.

### Operational rollout

- Existing `/etc/systemd/resolved.conf.d/zen-garden.conf` files left behind by prior moss versions are harmless (they just leave a `DNS=` upstream configured) but are now orphaned. Cleanup is optional and not done by moss — manual `rm` if a stone operator wants tidiness.
- Existing managed containers built with the old `dns_search: ["zengarden"]` configuration continue to function; the search domain just resolves to nothing, identical to the actual production behaviour today.

## References

- [DNS-0001](DNS-0001-local-zone-from-mdns-cache.md) — the `.local` zone work being superseded.
- [LANTERN-0003](LANTERN-0003-mdns-service-discovery.md) — mDNS service discovery (now the only discovery layer).
- [MDNS-0001](MDNS-0001-single-service-type.md) — single-service-type announcements.
- [garden-naming-assessment](../proposals/garden-naming-assessment.md) — the proposal whose presentation-layer goal motivated `.zengarden`. Service-name resolution is deferred from this design; that proposal will need to choose between env-injection and a fresh DNS subsystem if it advances.
