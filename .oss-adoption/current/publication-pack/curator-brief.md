Status: complete
Run ID: 20260719T034246Z
Project: zen-garden
Created: 2026-07-19T03:42:46Z

# Curator brief draft

Target curator: selfh.st Apps Directory / Self-Host Weekly editors

Publication gate: closed. The official route is https://selfh.st/submit/. Submit only after the license, install, release, security, and support gates close and the owner separately authorizes outreach.

## Why this fits the curator's audience

Zen Garden is self-hosted infrastructure for people using several mismatched machines rather than one polished appliance. It is not another application such as a photo gallery or notes server; it coordinates the identity and lifecycle of those applications across old laptops, desktops, thin clients, Pis, and GPU hosts. It is especially relevant to selfh.st readers who already run Docker/Compose or a home-server platform and need an adoption layer rather than a forced migration.

The useful editorial angle is bounded and demonstrable: a service is declared or adopted, matched to a host, reported ready through an application health contract, resolved independently of one hard-coded host address, and reconstructed if its managed container disappears on the same surviving Stone. The project explicitly distinguishes that from machine failover and backup.

## Verifiable project facts

- Public source: https://github.com/sylin-org/zen-garden
- Implementation language: primarily Rust, with React/TypeScript operator interfaces and embedded-device firmware.
- Current inspected maturity: 0.2 pre-release/active development; no GitHub release was published as of 2026-07-19.
- Architecture: Moss daemon per Stone, Rake CLI, Lantern aggregate UI, companions, and standalone Ollama/MongoDB/AI orchestrators.
- Catalog inventory: 51 checked-in software offering snippets across 18 categories; individual verification level varies.
- Discovery: supported same-LAN mDNS/UDP mechanisms; cross-network behavior should use an explicit bridge/mesh and security boundary.
- Generic recovery proof: persisted-intent reconstruction of a missing managed container on the same Stone; no generic stateful failover claim.
- Collaboration model: managed, adopted-read-only, and borrowed ownership; Compose/OCI/standard labels are preferred interfaces.
- Cloud account: not required for the intended same-LAN reference workflow.
- License: not safe to state until the owner adds a canonical root license and reconciles image labels.

## What changed and why it matters

The candidate launch should be pitched only when the following changes are actually shipped:

1. a legally coherent root license/notice and third-party asset ledger;
2. a clean, tagged, checksummed/signed release and immutable OCI images;
3. a reproducible Linux/Docker first-success path driven by real app readiness;
4. a release-gated missing-container reconstruction test and uncut demo;
5. a supported matrix, security/trusted-LAN boundary, known limits, and support policy;
6. corrected front-door language around catalog size, failover, updates, and Pond enforcement.

Why it matters: self-hosters can reuse uneven hardware and preserve a stable service intent while keeping the deployment, ingress, remote network, sync, or workload engines they already trust.

## Evidence, license, demo, and maintainer links

- Source/project page: https://github.com/sylin-org/zen-garden
- Internal evidence ledger: [claim ledger](../claim-ledger.md)
- Market/collaboration map: [ecosystem map](../ecosystem-map.md)
- Release-readiness gates: [readiness audit](../readiness-audit.md)
- Demo, release, immutable image, documentation, license, SECURITY, SUPPORT, and named maintainer links must be taken from the shipped release; none should be invented or inferred from this draft.

## Factual description available for adaptation

> Zen Garden is a local-first service-intent and continuity layer for a small fleet of mismatched self-hosting hardware. A Rust daemon on each machine discovers the garden, deploys or adopts services under an explicit lifecycle owner, evaluates hardware compatibility, and exposes them through a CLI and dashboard. Its first bounded recovery proof reconstructs a missing managed container from persisted intent on the same surviving machine; it does not claim generic stateful machine failover. Zen Garden is designed to collaborate with Docker Compose, existing home-server/container managers, remote-network layers such as Tailscale, and workload engines such as Ollama.

Suggested directory categories/tags, subject to the curator's taxonomy: self-hosting management, homelab, Docker, service discovery, orchestration, local-first, edge/repurposed hardware.

## Contact authority

The project owner/maintainer must supply the authorized contact and approve every fact tied to a release. A contributor, AI system, or playbook runner is not authorized to speak for the project or contact the curator.

No outreach is authorized by this draft.
