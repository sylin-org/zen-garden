Status: complete
Run ID: 20260719T034246Z
Project: zen-garden
Target: F:\Replica\NAS\Files\repo\github\sylin-org\zen-garden
Created: 2026-07-19T03:42:46Z
Playbook version: 0.1.0

# Project profile

## Owner goal and non-goals

The owner wants Zen Garden polished into an A+ solution for a defensible market niche, with an explicit understanding of adjacent tools and collaboration surfaces. “A+” should mean unusually trustworthy and complete for its chosen job, not feature parity with Kubernetes, a 300-app home-server store, or a cloud PaaS.

The working goal is to make a heterogeneous collection of spare LAN-connected machines behave like one understandable service garden: declare or adopt an offering, find it by intent rather than host address, keep its runtime tended, and use each machine according to its actual hardware.

Non-goals for the launch candidate:

- universal high availability or transparent failover for arbitrary stateful software;
- replacing Docker, Compose, existing app stores, remote-access meshes, or mature fleet-control products;
- enterprise multi-tenancy, compliance, or an always-on hosted control plane;
- claiming every checked-in offering is production-certified;
- broad promotion before install, license, security, and first-success gates close;
- any external publication from this run. External publication is not authorized.

## Primary user and triggering problem

The highest-fit initial user is a technically comfortable self-hoster, homelab operator, or very small local team with roughly two to ten mixed machines: old laptops, desktops, thin clients, Raspberry Pis, and perhaps a GPU box or Android/Linux oddity. They already accept Docker as a substrate but do not want to become a Kubernetes operator or memorize which service moved to which IP and port.

Their triggering problem is not “install one app on one server.” Existing tools already do that well. It is: “I have useful but uneven hardware on my LAN; I want to offer services from it by name, see the garden as a whole, recover a missing runtime from declared intent, and route specialized workloads to suitable hardware without a cloud account.”

Secondary users are offering authors, companion/embedded-device builders, and developers of service-specific orchestrators such as Ollama or MongoDB integrations.

## First successful outcome

The target first-success journey should take no more than 15 minutes on one supported Linux reference machine:

1. Install signed Moss and Rake artifacts with a documented, reversible command.
2. Start one Stone and see it through `garden-rake find` and Lantern.
3. Plant a small, multi-architecture reference offering with a real health check.
4. Wait until application readiness—not merely container creation—is reported.
5. resolve the service to a usable endpoint and connect to it;
6. delete the managed container and watch Moss reconstruct it from the persisted offering intent;
7. inspect the event, logs, stable port, and limitation statement.

Today this journey is a launch target, not a safely reproducible public promise. The root quickstart uses an image/environment contract not implemented by the repository, no tagged release exists, and lifecycle readiness can report success before application health is proven.

## Primary and secondary archetypes

- **Primary archetype — local infrastructure product:** a per-machine daemon plus garden-wide discovery, reconciliation, and operation for self-hosted services.
- **Secondary archetype — CLI/developer tool:** Rake exposes discovery and lifecycle actions over typed REST/SSE interfaces.
- **Secondary archetype — curated integration catalog:** 51 checked-in software offering snippets across 18 categories provide intent and compatibility metadata.
- **Secondary archetype — extension platform:** companions connect physical/audio/display hardware; standalone orchestrators add domain-specific placement and lifecycle logic.
- **Emerging archetype — protocol/service-identity layer:** `zen-garden:` intent URIs and discovery records describe services independently of one host, but the URI surface must not yet be presented as a universal resolver.

## Maturity, support surface, and maintainer capacity

The repository is a substantive pre-release implementation, not a concept demo. The tracked inventory contains 1,985 files, 909 Rust files, 510 documentation files, 30 test files, 11 root-workspace crates, four standalone orchestrator crates, React dashboards, firmware, installers, and a probe harness. `cargo check --workspace --all-targets` passed during the preceding repository audit with four test-only warnings.

Release maturity is lower than implementation breadth: version metadata is 0.2/0.2.0, there are no Git tags, no root license file, no packaged public release path, and no evidence in this run of a clean-machine end-to-end install. Maintainer count, weekly support capacity, supported hardware matrix, response expectations, and long-term governance are not stated and require owner confirmation before launch planning is scheduled.

The sensible support promise for the first cohort is narrow: one Linux reference path, Docker-managed offerings, same-LAN discovery, best-effort issue support, and explicitly named experimental surfaces. Windows adoption, Android Stones, cross-subnet operation, storage migration, security enforcement, physical companions, and service-specific failover should graduate separately when each has an exercised proof.

## Installation, use, and evidence

Current evidence is strongest in source and internal tests:

- [README.md](../../README.md) states the intended product story but contains count and behavior drift.
- [version.json](../../version.json) names the current Garden phase as 0.2.
- [Cargo.toml](../../Cargo.toml) defines the 11-crate root workspace and Rust 1.95 requirement; four Koi dependencies resolve through a sibling checkout.
- [CI](../../.github/workflows/ci.yml) checks, lints, tests, and audits the root workspace and checks three orchestrators.
- [application lifecycle assessment](../../docs/notes/application-lifecycle-assessment-2026-07.md) distinguishes implemented same-Stone reconstruction from incomplete readiness, rollback, migration, and generic failover.
- [repository inventory](repository-inventory.json) records files, languages, tests, workflows, community files, VCS state, and the fact that sensitive file contents were not read.

Before promotion, evidence should move from “code and design exist” to a versioned proof bundle: clean install transcript, checksum/signature verification, short uncut first-success demo, probe output, supported-platform matrix, and documented failure/recovery exercises.

## Constraints and open questions

- The working tree was already heavily dirty; this playbook wrote only `.oss-adoption/` and did not reinterpret user-owned changes as product evidence.
- A clean clone currently requires a sibling `../koi` checkout whose CI ref is not pinned.
- The public license claim is not backed by a root `LICENSE`; orchestrator image labels say MIT while the README says Apache 2.0.
- The default Pond enforcement posture observes and allows; enforce mode retires HTTPS and uses signed envelopes over clear HTTP. Security language must describe that exact model.
- The catalog has 51 templates, but catalog presence is not operational certification.
- Support capacity, target release date, canonical package/registry names, trademark/name availability, and which Linux distribution/hardware becomes the reference path remain owner decisions.
- The first launch should prove one narrow autonomy loop. Cross-Stone state recovery and service-specific HA should remain roadmap items until independently exercised.
