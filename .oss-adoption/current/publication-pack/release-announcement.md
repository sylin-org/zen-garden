Status: complete
Run ID: 20260719T034246Z
Project: zen-garden
Created: 2026-07-19T03:42:46Z

# Release announcement draft

Release: proposed Zen Garden 0.2 release candidate; no such release was published in the inspected state

Publication gate: closed. This is a release-scope draft, not a statement that artifacts shipped. Replace “candidate” with the exact signed tag/digests only after all referenced evidence exists and the owner authorizes publication.

## Proposed headline

**Zen Garden 0.2 release candidate: service intent and same-Stone continuity for mismatched homelab hardware**

## What shipped

The announcement may say the following only if the final release gate proves each item:

- a reproducible, documented Moss/Rake install for the declared Linux/Docker reference matrix;
- garden discovery, CLI/UI inspection, curated-offering deployment, stable local port allocation, real application readiness, endpoint resolution, and missing-container reconstruction on the same surviving Stone;
- explicit `managed`, `adopted-read-only`, and `borrowed` lifecycle ownership, with documented behavior when another tool owns deployment;
- a verified subset of the 51 checked-in offering templates across 18 categories, each labeled by maturity and tested platform/image digest;
- checksummed/signed release assets, SBOM/provenance, immutable GHCR images and matching Docker Hub mirrors;
- a root license/notice, SECURITY/SUPPORT/CONTRIBUTING/governance materials, supported matrix, threat/boundary summary, known issues, and uninstall/rollback instructions;
- release-gated CI/probe evidence and an uncut first-success/reconstruction demo.

Additional components—Lantern, companions, storage/snapshots, Ollama/MongoDB/AI orchestrators, Windows/Android paths, cross-subnet/security modes—must be individually labeled supported, preview, experimental, or excluded.

## Who benefits and how

This candidate is for self-hosters and homelab operators with a few genuinely different machines who want services to be described by intent and capability rather than permanently coupled to one host address. It is also for operators who want to keep Portainer, Komodo, Coolify, Runtipi, Umbrel, CasaOS, Cosmos, YunoHost, or another owner and let Zen adopt a service without a second reconciler mutating it.

Zen does not replace Docker Compose, a reverse proxy/SSO layer, Tailscale, Consul, Syncthing, Ollama, or Kubernetes. It coordinates service intent across the garden and provides adapters so those systems can continue owning their layer.

## Compatibility and migration impact

- State the exact supported OS, architecture, Docker version, filesystem/volume assumptions, open ports, multicast requirements, and minimum resources in the final announcement.
- State whether upgrading from an earlier untagged 0.2 development build is supported. If not proven, require a fresh reference install and document export/backup first.
- Pin the exact Koi dependency revision or packaged crates used to build the release.
- Publish configuration/schema changes, data migrations, rollback boundary, and tag/image digest policy.
- Treat external manager adoption as read-only unless the user explicitly transfers lifecycle ownership.
- Treat snapshots/sync/replication separately from runtime reconstruction; name what happens if the host or disk is lost.

## How to try it

The final release must point to one versioned quickstart generated or exercised by CI. It should:

1. verify the artifact signature/checksum;
2. install and start Moss/Rake on the supported reference host;
3. confirm network/firewall/security assumptions;
4. plant the verified reference offering;
5. wait for application-level health and perform one real request;
6. remove the container and verify documented reconstruction;
7. show logs/status, data location, backup boundary, uninstall, and support route.

Do not reuse the current root README's `zen-garden/stone:latest` / `ANNOUNCE_SERVICE` example; the preceding repository audit found no matching implementation. Do not publish an install command until it has passed on a clean machine from the exact public artifacts.

## Limitations and known issues

- Pre-release software; supported scope is the declared reference matrix and verified offering subset.
- Generic recovery is missing-container reconstruction on a surviving Stone, not automatic machine or state failover.
- Application-consistent backup, exact cross-Stone migration, and universal guarded update rollback are not general guarantees.
- Catalog inclusion does not certify every architecture, image version, or data migration.
- Platform features differ; Windows mDNS and several host operations are incomplete relative to Linux.
- Pond security semantics must be stated exactly: current default observe mode allows invalid/unsigned control mutations after warning; enforce mode uses signed envelopes over clear HTTP, not enforced mTLS transport.
- Remote internet exposure is outside the reference trusted-LAN path unless a documented authenticated network/ingress layer is used.
- AI/Pavilion and some CLI/companion surfaces are incomplete or experimental.

## Acknowledgements and support route

Thank upstream layers and communities by actual use: Rust, Docker/Compose, Koi, and every bundled offering/image/asset under its own license. Credit external contributors by their chosen public identity only.

Final support text must link SECURITY.md for private vulnerabilities, SUPPORT.md for expected response and safe diagnostic redaction, issue forms for reproducible bugs, and Discussions only if staffed. The named release owner and backup reviewer must be present during the release window.

No release or announcement is authorized by this draft.
