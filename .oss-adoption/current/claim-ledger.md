Status: complete
Run ID: 20260719T034246Z
Project: zen-garden
Created: 2026-07-19T03:42:46Z

# Claim ledger

Confidence reflects repository evidence at commit `1fb8205a8b204ef34f29c1d464e8009168bb1870`, not an independent production certification.

| Claim | Scope | Evidence | Version or date | Confidence | Safe wording | Excluded wording | Next proof |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Zen Garden targets self-hosted services on repurposed hardware. | Product purpose | [README](../../README.md); host/profile and offering code | 2026-06-24 commit | High | “A local-first service garden for useful, uneven hardware on your LAN.” | “Turns any device into a production cluster.” | Interview five target operators and verify the trigger problem. |
| The root product line is in the 0.2 development phase. | Version identity | [version.json](../../version.json); core crate manifests use 0.2.0 | Current tree | High | “Pre-release 0.2 development line.” | “Stable 0.2 release” or “production ready.” | Cut a signed 0.2.0 release candidate and tag it. |
| The checked-in software catalog has 51 snippets in 18 category directories. | Repository catalog | `src/moss/embedded/manifests/sw`; [lifecycle assessment](../../docs/notes/application-lifecycle-assessment-2026-07.md) | 2026-07 audit | High | “51 checked-in offering templates across 18 categories; support level varies.” | “51 production-certified apps.” | Generate counts and per-offering maturity from the catalog in CI. |
| Moss supports managed, adopted, and borrowed offering concepts. | Lifecycle modes | Common manifest types, adoption APIs, [offering-mode decision](../../docs/decisions/OFFER-0005-offering-modes.md) | Current tree | High for model; Medium for breadth | “Manage containers, register selected native services, or reference external services through explicit modes.” | “Seamlessly manages every service regardless of origin.” | End-to-end tests and docs for one offering in each mode. |
| Garden discovery uses mDNS plus UDP multicast/broadcast paths. | Same-LAN discovery | `src/discovery`; Moss discovery aggregate; constants | Current tree | High | “Discovers Stones and services on supported same-LAN networks using mDNS and UDP mechanisms.” | “Zero-config discovery on every OS and network.” | Multi-NIC Linux/macOS/Windows test matrix; resolve Windows mDNS stub. |
| Moss persists offering intent and can reconstruct a missing managed container on the same surviving Stone. | Reconciliation | Health monitor/reconciliation code; [lifecycle assessment](../../docs/notes/application-lifecycle-assessment-2026-07.md) | 2026-07 audit | Medium-High | “If a managed container disappears while its Stone survives, Moss can recreate the runtime from persisted offering intent.” | “Automatic failover when a machine dies.” | Release-gated black-box deletion/reconstruction test with data-boundary disclosure. |
| Managed offering ports are persisted and reused across recreation. | Local endpoint stability | Port ledger code and [changelog](../../docs/CHANGELOG.md) | Current tree | High in code | “Moss persists remapped host-port assignments for managed offerings.” | “Endpoints never change under migration or network change.” | Black-box restart/update/recreate test. |
| Offering placement can consider detected hardware and compatibility rules. | Placement and catalog | Host profile, compatibility evaluation, placement domain | Current tree | High for mechanism; Medium for outcomes | “Zen Garden evaluates host capabilities and offering compatibility before placement.” | “Always chooses the optimal machine.” | Publish deterministic reference cases and real-garden benchmark. |
| Lantern gives a garden-wide view and action proxy. | Operator UI | Lantern Rust router/state and React routes | Current tree | High | “Lantern aggregates discovered Stones in memory and exposes garden views, events, and proxied actions.” | “Durable, highly available central control plane.” | Frontend build/test gate and demo on a multi-Stone garden. |
| Rake provides CLI access to discovery, lifecycle, storage, pond, companion, and admin APIs. | CLI | Central command manifest and dispatch implementation | Current tree | High for implemented commands | “Rake is the operator CLI for the implemented Moss APIs.” | “Every displayed/scaffolded command is complete.” | Command-manifest contract test and remove/label scaffolds. |
| Ollama orchestration includes discovery, model reconciliation, metrics, placement, and compatible proxy surfaces. | Specialized AI integration | Ollama orchestrator router/domain; standalone crate check | Current tree, orchestrator 0.1.0 | Medium | “An experimental Ollama orchestrator implements VRAM-aware placement and Ollama/OpenAI-compatible routing.” | “Proven optimal AI scheduling” or “drop-in for every OpenAI workload.” | Multi-GPU benchmark, failure tests, API compatibility suite. |
| MongoDB orchestration implements replica-set lifecycle and topology monitoring. | Specialized database integration | MongoDB conductor/gateway/router; standalone crate check | Current tree, orchestrator 0.1.0 | Medium | “An experimental MongoDB orchestrator implements replica-set membership and health/lag guidance.” | “Automatic, lossless MongoDB HA.” | Destructive multi-node failover and recovery exercise with RPO/RTO results. |
| Snapshot, harvest, storage, S3, and WebDAV surfaces exist. | Data lifecycle | Moss storage/snapshot domains and APIs; tests | Current tree | Medium | “Zen Garden includes local snapshot/storage mechanisms and S3/WebDAV access surfaces, with important consistency limits.” | “Application-consistent backup and exact cross-Stone restore for all offerings.” | Select one snapshot model; quiesce, checksum, corruption, and exact-restore tests. |
| Pond supports enrollment/certificate material and signed control-plane envelopes. | Security mechanism | Pond lifecycle, Koi certmesh integration, signing/enforcement/replay code | Current tree | Medium | “Pond includes enrollment and signed-envelope verification; the current default observes invalid control mutations rather than rejecting them.” | “Secure by default,” “mTLS-enforced,” or “safe to expose directly to the internet.” | Threat-model review and enforced-mode adversarial test suite. |
| Companions extend the garden with USB, audio, display, and embedded-device behaviors. | Extension platform | Companion SDK/USB, Cricket, Firefly, firmware | Current tree | High for code presence; Medium for field support | “The companion SDK and included Cricket/Firefly implementations connect selected peripherals to garden events.” | “Universal hardware integration.” | Supported-device list, firmware build checks, unplug/replug soak tests. |
| `zen-garden:` URIs encode offering intent. | Naming/protocol surface | `src/common/src/uri`; URI corpus tests | Current tree | High for parser | “A typed URI grammar represents actions, capabilities, targets, tags, and protocols.” | “A universal service resolver adopted by other tools.” | End-to-end resolver contract and interoperability proposal. |
| The repository has substantial automated test investment. | Engineering maturity | 30 tracked test files; roughly 2,300 static Rust test attributes; probe registry | 2026-07 audit | High for presence; Medium for release coverage | “The codebase contains extensive unit/integration scenarios, though shipped surfaces are not uniformly gated.” | “Fully tested” or “battle tested.” | Publish test matrix, run results, coverage by supported surface, and flaky-test policy. |

## Non-claims and known limits

- Zen Garden does not currently demonstrate generic cross-Stone failover for arbitrary stateful offerings.
- It does not prove application-consistent backup or exact snapshot-based migration for every offering.
- The stronger guarded update/rollback ceremony is not the normal universal update path.
- “Job succeeded,” “container created,” and “application ready” are not yet equivalent in every path.
- The 51-template catalog is inventory, not a promise that every image/platform combination works.
- Windows discovery and host management are not feature-equivalent to the Linux path.
- The default security mode is transitional observe-and-allow, and enforce mode is not mTLS transport.
- There is no tagged, signed, publicly verified release in the inspected repository state.
- Koi is a required sibling source dependency for a clean build in the current tree.
- Pavilion and some Rake/companion/AI surfaces are scaffolded, residual, or explicitly incomplete.
- The project has not supplied public user counts, uptime, scale, RPO/RTO, latency, or comparative benchmark evidence.

## Claims requiring owner confirmation

- The chosen open-source license and ownership/attribution of all bundled assets.
- The canonical project name, package/image coordinates, and whether the name has been checked for conflicts.
- The supported reference operating system, architectures, Docker versions, and hardware.
- Which offerings and orchestrators are `supported`, `preview`, or `experimental` for the first release.
- Maintainer count, weekly support capacity, response expectations, and release cadence.
- Any real external deployments, testimonials, user counts, or production history.
- Whether public security review or audit has occurred.
- Whether a tagged 0.2.0 release should preserve current semantics or wait for the readiness/security corrections.
