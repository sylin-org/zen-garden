Status: complete
Run ID: 20260719T034246Z
Project: zen-garden
Created: 2026-07-19T03:42:46Z

# Launch brief

Publication gate: closed. This brief is an internal fact source until the readiness audit closes and the owner authorizes a specific external use.

## Headline and one-sentence positioning

**Zen Garden — service intent and continuity for mismatched self-hosting hardware**

Zen Garden is a local-first control layer for a small fleet of old laptops, desktops, thin clients, Pis, and GPU boxes: it can deploy or adopt services, match workloads to hardware, discover them by intent instead of one host address, and reconstruct a missing managed container from persisted intent on the same surviving machine.

Short GitHub subtitle:

> Local-first service intent and continuity for a small fleet of mismatched self-hosting hardware.

## Audience, trigger problem, and fit

Primary audience: technically comfortable self-hosters/homelab operators with two to ten heterogeneous machines who use Docker but do not want the Kubernetes operating model.

Trigger problem: their services are coupled to mutable hostnames/IPs/ports; useful machines have uneven architecture, memory, disks, and GPUs; several managers may already own containers; and a missing runtime becomes a manual reconstruction exercise.

Good fit:

- same-LAN, local-first operation and no required hosted account;
- repurposed/mixed hardware where placement and capability matter;
- a desire to keep existing Portainer, Komodo, Coolify, Runtipi, Umbrel, CasaOS, Cosmos, or YunoHost ownership while adding semantic discovery through adoption/borrowing;
- operators who want a curated, inspectable lifecycle contract rather than a general-purpose production scheduler;
- Ollama/MongoDB/physical-companion experiments after the reference core works.

Poor fit:

- turnkey single-box app-store users who do not need a fleet;
- internet-exposed enterprise multi-tenancy/compliance;
- arbitrary stateful high availability with a guaranteed RPO/RTO;
- teams needing a mature GitOps/PaaS/Kubernetes control plane today.

## Proof and evidence links

- Source and current project status: https://github.com/sylin-org/zen-garden
- Repository implementation map: [project profile](../project-profile.md)
- Claim-by-claim evidence and excluded wording: [claim ledger](../claim-ledger.md)
- Lifecycle boundaries: [application lifecycle assessment](../../../docs/notes/application-lifecycle-assessment-2026-07.md)
- Market alternatives and collaboration paths: [ecosystem map](../ecosystem-map.md)
- Release gates and operational trust: [readiness audit](../readiness-audit.md)

Proof assets required before this brief can support publication:

1. signed/checksummed tagged release and immutable multi-arch image(s) for the declared matrix;
2. clean-machine install/uninstall transcript;
3. uncut two-minute install → health → resolve → missing-container reconstruction demo;
4. CI/probe result tied to the release commit;
5. license/notice, security boundary, supported matrix, known issues, and support route.

## Maturity, limitations, and non-claims

Current inspected state is a substantive 0.2 pre-release with 11 root workspace crates, four standalone orchestrator crates, 51 checked-in offering snippets across 18 categories, REST/SSE/CLI/UI surfaces, companion firmware, and extensive tests. The root workspace check passed in the preceding audit, but the repository has no tagged release, root license file, or reproducible public quickstart.

Required launch language:

- Catalog entries have individual maturity; catalog inclusion is not certification.
- The strong generic recovery proof is same-Stone reconstruction of a missing managed container, not machine failover.
- Generic stateful cross-Stone failover, exact migration, and universally guarded update rollback are not shipped guarantees.
- “Container created” is not always yet equivalent to “application ready”; the reference release must close this gap.
- The current Pond default observes invalid control-plane mutations and allows them; enforce mode uses signed envelopes over clear HTTP rather than enforced mTLS.
- Start with one supported Linux/Docker reference path. Other OSes/hardware/integrations need explicit preview/experimental labels.

Never use: “production ready,” “Kubernetes killer,” “automatic failover for any app,” “secure by default,” “zero config everywhere,” “battle tested,” or “51 supported apps.”

## Demo and first-success path

The canonical launch demo should use a small multi-architecture offering with a deterministic readiness check and disposable sample data:

1. Verify artifact checksum/signature and install Moss/Rake on the supported Linux host.
2. Start the Stone; use Rake and Lantern to show identity, hardware, and no hidden hosted control plane.
3. Plant the verified offering; narrate manifest, compatibility decision, stable port, and lifecycle owner.
4. Wait for real application health; resolve the endpoint and perform one application-level request.
5. Delete the managed container with the standard Docker CLI.
6. Show the reconciler event, bounded backoff, recreated container, reused endpoint, and successful request.
7. Show what did not happen: the machine did not fail, no generic state transfer occurred, and external backup/HA requires a separate policy/integration.
8. In a second optional scene, adopt a Portainer/Runtipi-managed container read-only to demonstrate collaboration without double reconciliation.

## Likely questions and response owner

| Question | Concise answer | Evidence/owner role |
| --- | --- | --- |
| Why not Uncloud? | Uncloud is the closest peer and currently stronger for general multi-machine Compose/networking. Zen focuses on mixed-hardware service intent, ownership modes, curated capabilities, companions, and service-specific orchestration; an Uncloud backend is a natural collaboration. | Ecosystem map; project owner |
| Why not k3s? | K3s is excellent lightweight Kubernetes. Zen targets operators who want a smaller service-intent model and explicit adoption of existing non-Kubernetes services. A backend/bridge is possible later. | Ecosystem map; architecture owner |
| Is it high availability? | Only bounded behaviors may be claimed. The reference generic proof is reconstruction on a surviving Stone; service-specific HA needs separate evidence and RPO/RTO. | Claim ledger; release owner |
| What happens to data? | The offering's persistence boundary and chosen snapshot/replication policy must be explicit. Runtime reconstruction alone does not recreate lost host storage. | Lifecycle assessment; storage owner |
| Can I keep Portainer/Runtipi/Coolify? | Yes: use adopted-read-only or borrowed ownership and standard labels; only one tool may mutate lifecycle. | Ecosystem map; integration owner |
| Does it need the cloud? | The intended reference LAN workflow does not require a hosted account. Remote/cross-subnet use should integrate an authenticated network layer such as Tailscale and follow the documented boundary. | Security docs; security owner |
| Is Windows/Android supported? | Only the published release matrix should answer “supported.” Current code has platform-specific paths and incomplete parity; other paths should be preview/experimental until exercised. | Release matrix; release owner |
| How much maintainer support exists? | The owner must publish response expectations and support budget before launch. | SUPPORT/governance; project owner |

Roles must be assigned to named people before launch day. No external response is authorized by this brief.
