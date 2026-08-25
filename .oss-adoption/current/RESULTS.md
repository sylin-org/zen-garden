Status: complete
Run ID: 20260719T034246Z
Project: zen-garden
Created: 2026-07-19T03:42:46Z

# Results

Recommendation: **Prepare for a narrow 0.2 release candidate and design-partner cohort; do not broadly launch or publish yet.**

## Why

Zen Garden has a credible, differentiated implementation but an under-specified release contract. Its strongest niche is not “another self-hosted app store” or “simpler Kubernetes.” It is a local-first service-intent and continuity layer for a small fleet of mismatched, repurposed machines, especially where services may already be owned by several tools and hardware capability matters.

The implementation is substantial: 11 root-workspace crates, four standalone orchestrator crates, 51 checked-in software offerings across 18 categories, a Rust daemon/CLI/discovery stack, Lantern UI, storage/security domains, physical companions and firmware, and extensive tests. The preceding repository audit successfully ran `cargo check --workspace --all-targets` with four test-only warnings.

The public trust path is not ready: no root license, conflicting image license labels, no tagged/signed release or package, an unpinned sibling Koi dependency, a fictional quickstart, default observe-and-allow security, incomplete application readiness, overstated failover/update language, drifted counts/docs, incomplete CI coverage, and no community/support/security contract. Those are correctable launch blockers, not reasons to abandon the project.

## Market niche and collaboration posture

Position Zen Garden as:

> **Local-first service intent and continuity for a small fleet of mismatched self-hosting hardware.**

The closest direct peer is Uncloud, which currently has the clearer general multi-machine Compose/network/rolling-deploy story. Zen's defensible combination is explicit managed/adopted/borrowed ownership, curated capability/hardware intent, bounded reconciliation, physical companions, and service-specific Ollama/MongoDB orchestration.

Collaboration rule: **Zen coordinates the service's life across a garden; another tool may own the OS, deployment primitive, network path, file transfer, ingress, or inference engine.** Highest-value work:

1. bidirectional Compose plus `io.zen-garden.*` ownership/capability labels;
2. enforceable single-reconciler modes (`managed`, `adopted-read-only`, `borrowed`, external owner);
3. Tailscale Services readiness/drain exporter and standards-based DNS-SD, then Consul;
4. host recipes/importers for Runtipi/Umbrel/CasaOS/Cosmos/YunoHost and manager recipes for Portainer/Komodo/Coolify;
5. an Uncloud execution-backend prototype, followed only later by k3s if demanded;
6. optional Syncthing replication transport while preserving independent snapshot semantics;
7. harden Ollama around measured cross-Stone placement/warmup/stable gateway behavior.

See [ecosystem-map.md](ecosystem-map.md) for the comparable-project map and integration boundaries.

## Next three actions

1. **Close legal and release-chain blockers.** Owner chooses the license and supported matrix; add LICENSE/NOTICE/asset attribution; reconcile OCI labels; pin/publish Koi; create clean signed/checksummed/SBOM release artifacts and immutable images from a clean commit.
2. **Make one autonomy loop indisputable.** Replace the quickstart; require real app health; prove install → healthy endpoint → intent resolution → same-Stone missing-container reconstruction in CI/probe and a short uncut demo; rewrite README/security/failover/update/count claims from the ledger.
3. **Run a five-to-ten-person opt-in cohort before public promotion.** Measure unaided time-to-first-success, recovery success, conceptual understanding, integration demand, and maintainer minutes. Proceed only if the thresholds in [measurement-plan.md](measurement-plan.md) pass.

## Readiness blockers

- canonical root license/notice and third-party asset/image license reconciliation;
- tagged, checksummed/signed, reproducible release and canonical package/image coordinates;
- self-contained or immutable Koi dependency;
- real, CI-exercised first-success/install/uninstall path;
- application-level readiness and release-gated same-Stone reconciliation proof;
- accurate README/docs for 51/18 catalog, lifecycle, platform, ports, and security;
- explicit trusted-LAN/remote-access threat boundary and safe supported enforcement posture;
- expanded CI for format/warnings/frontends/orchestrators/images/installers/probe/release artifacts;
- SECURITY/SUPPORT/CONTRIBUTING/CODE_OF_CONDUCT/governance/templates;
- named maintainer/support capacity and release authority.

Detailed gates and exit conditions are in [readiness-audit.md](readiness-audit.md). Safe and excluded wording is in [claim-ledger.md](claim-ledger.md).

## Best-fit channels

1. Owned GitHub front door and topics.
2. Tagged GitHub Release plus GHCR; Docker Hub as an identical-digest discovery mirror.
3. selfh.st Apps Directory / Self-Host Weekly after the install-ready release.
4. One maintainer-authored Show HN anchor after cohort proof and with same-day support capacity.
5. A separately adapted r/selfhosted post, then an evidence-heavy r/homelab build report.
6. AlternativeTo for durable comparison intent; Changelog News/This Week in Rust for later technical proof.

Avoid awesome-selfhosted because its current scope rules redirect generic deployment/orchestration tools. Defer awesome-sysadmin until release age, real use, and an independent ecosystem qualify. Do not use Lobsters as a drive-by launch channel or spend effort on inactive niche podcasts.

See [channel-research.md](channel-research.md) and [publication-plan.md](publication-plan.md). Drafts are in [publication-pack](publication-pack/launch-brief.md).

## Owner decisions needed

1. License and redistribution authority for all source/assets/images.
2. Reference Linux distribution, architecture, Docker range, hardware, first verified offering, and exact supported security mode.
3. Canonical registry/package names and release/version policy.
4. Which surfaces are supported, preview, experimental, or removed from the first public release.
5. Maintainer/support owner, weekly budget, response window, and backup release reviewer.
6. First interoperability bet: recommended order is Compose ownership contract, Tailscale/DNS-SD exporter, then Uncloud adapter.
7. Cohort participants and consent process.
8. Separate authorization for any external release, registry push, submission, post, or outreach.

## Unresolved risks and evidence gaps

- No independent user/install/scale evidence, public benchmarks, RPO/RTO, or security audit was supplied.
- Direct peer Uncloud is crisp and fast-moving; Zen must demonstrate its richer intent/ownership/hardware distinction.
- Scope exceeds any stated maintainer capacity and could make support unsustainable.
- Two reconcilers can damage service state unless ownership is enforced, not merely documented.
- Backup/snapshot/sync/migration semantics can be conflated and create data-loss expectations.
- Windows/Android and cross-subnet claims need explicit matrices and exercises.
- The common project name creates searchability/trademark/namespace risk.
- The inspected tree was heavily dirty; releases must come from a clean reviewed commit, not this workspace state.

## Archived predecessor

No prior `.oss-adoption/current` run existed, so no predecessor was archived. The playbook created run `20260719T034246Z` under `.oss-adoption/current/` and preserved the repository's pre-existing dirty work.

## Authorization reminder

External publication remains unauthorized. This run made no external post, release, registry push, submission, or outreach. It wrote only the target repository's `.oss-adoption/` workspace. Product-source and public-doc repairs described here require a separate implementation request.
