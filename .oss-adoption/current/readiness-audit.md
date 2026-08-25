Status: complete
Run ID: 20260719T034246Z
Project: zen-garden
Created: 2026-07-19T03:42:46Z

# Readiness audit

Severity vocabulary: `blocker` prevents a credible public launch; `important` should close before broad promotion; `helpful` improves adoption after the core promise is safe.

## Executive finding

**Recommendation: prepare, do not broadly launch yet.** Zen Garden has a real and unusually broad implementation, but its release trust chain and first-use story lag behind the code. The fastest A+ path is not more breadth. It is a narrow release contract: one supported Linux install, one healthy offering, one honest same-Stone reconciliation demo, a coherent security statement, and evidence that a clean user can reproduce it.

The project can begin a private five-to-ten-person design-partner cohort after blockers 1–5 below close. Public community launch should wait for the entire blocker set plus the highest-priority CI and support work.

## Legal and ownership

| Severity | Finding | Evidence | Required exit condition |
| --- | --- | --- | --- |
| **blocker** | No root license file supports the README's “Apache 2.0” claim. | [repository inventory](repository-inventory.json) reports `license: null`; [README](../../README.md) asserts Apache 2.0. | Owner selects the license; add canonical `LICENSE` and any required `NOTICE`; verify source, docs, firmware, fonts, audio, icons, and generated assets are distributable. |
| **blocker** | Container metadata conflicts with the public license claim. | Ollama, MongoDB, and AI Dockerfiles label images MIT while README says Apache 2.0. | Reconcile all package/image labels with the chosen license and document third-party exceptions. |
| **important** | Contribution ownership and project governance are undefined. | No CONTRIBUTING, governance, DCO/CLA policy, or maintainer roster was found. | State contribution license mechanism, decision authority, release authority, and maintainer list. |

## First success and installation

| Severity | Finding | Evidence | Required exit condition |
| --- | --- | --- | --- |
| **blocker** | The root quickstart is not an executable contract. | [README](../../README.md) uses `zen-garden/stone:latest` and `ANNOUNCE_SERVICE=mongodb`; the preceding code audit found no matching implementation. | Replace it with a clean-machine-tested path using artifacts that actually exist. Run it verbatim in CI or a release smoke job. |
| **blocker** | No tagged, checksummed, signed release or canonical package path exists. | `git tag --list` returned no tags; repository contains build/install assets but no public release workflow. | Publish a release candidate for the reference platform with immutable version, checksums, provenance/signature, SBOM, rollback/uninstall, and artifact retention policy. |
| **blocker** | A clone does not build without an unpinned sibling repository. | [Cargo.toml](../../Cargo.toml) uses four `../koi` path dependencies; CI's `KOI_REF` is empty. | Pin Koi to a public immutable revision or publish consumable crates; verify a clean clone without undocumented local state. |
| **blocker** | “Created” is not consistently “ready.” | [lifecycle assessment](../../docs/notes/application-lifecycle-assessment-2026-07.md) shows jobs can succeed before real app health and plant health timeout may be only a warning. | Make the reference journey fail unless the application health contract succeeds; test install → readiness → resolve. |
| **helpful** | There is no compact visual proof for a new evaluator. | Documentation is extensive but no canonical uncut first-success asset was identified. | Add a two-minute terminal/UI recording and transcript generated from the tested release. |

## Product evidence and limitations

| Severity | Finding | Evidence | Required exit condition |
| --- | --- | --- | --- |
| **blocker** | Front-door claims exceed exercised lifecycle behavior. | README promises automatic reconnect/failure recovery and implies safe updates; lifecycle assessment limits the strong proof to missing-container reconstruction on the same surviving Stone. | Rewrite README and launch copy around intent, discovery, stable ports, hardware fit, and same-Stone reconstruction. State that generic cross-Stone stateful failover and guarded rollback are not shipped guarantees. |
| **important** | Product counts and terminology drift. | README says 31 offerings/17 categories; filesystem contains 51 snippets/18 category directories. Some docs retain old ports/security models. | Generate volatile facts from source where feasible; classify current, historical, proposal, and experimental docs; add link/contract checks. |
| **important** | Catalog inclusion can be mistaken for certification. | 51 manifests exist, but no release evidence proves all combinations on all advertised architectures. | Add per-offering maturity (`verified`, `community-tested`, `experimental`) and last-tested platform/image digest. Market only the verified subset. |
| **important** | Update, backup, restore, and migration models overlap. | Lifecycle assessment describes basic recreation, stronger but non-default update ceremony, two backup/snapshot models, and incomplete exact migration. | Pick one public lifecycle contract and exercise healthy update rollback plus checksum-verified restore before claiming them. |
| **important** | Platform breadth exceeds current uniformity. | Windows mDNS is stubbed; several host operations are platform-limited; Android proof is internal/specific. | Publish a capability matrix and use “supported,” “preview,” and “not supported” rather than a single broad platform claim. |
| **helpful** | Pavilion is an artifact-only residual surface. | The audit found generated schemas/build output but no buildable Pavilion source/package. | Remove, archive, or label it so users do not infer a supported product. |

## Repository and community health

| Severity | Finding | Evidence | Required exit condition |
| --- | --- | --- | --- |
| **important** | CI omits several shipped surfaces and release contracts. | [CI](../../.github/workflows/ci.yml) covers root Rust and check-only for three orchestrators; no frontend, formatter, installer, firmware, probe, image, or release smoke gates are present. | Add `cargo fmt --check`, warning ratchet, frontend build/test, orchestrator tests where linkable, image builds, installer smoke, probe reference scenario, and release validation. |
| **important** | The AI orchestrator is excluded from CI's orchestrator matrix. | Matrix contains common, Ollama, and MongoDB only. | Explicitly classify AI as experimental/dormant or add its supported build/test job. |
| **important** | Standard community trust files are absent. | Inventory found no SECURITY, CONTRIBUTING, CODE_OF_CONDUCT, SUPPORT, governance, issue templates, PR template, or citation file. | Add concise, project-specific versions; include vulnerability reporting and logs/redaction guidance. |
| **important** | The branch/worktree is not release-audit clean. | Inventory saw branch `fix/snapshot-scheduler-disposal` with 5,532 dirty entries, 15 of them generated by this run. | Cut releases from a reviewed clean tree/commit; preserve unrelated user work separately. |
| **helpful** | Documentation volume makes the canonical path hard to distinguish. | Inventory found 510 documentation files, including archives/proposals. | Establish a small canonical docs index and search labels for current vs historical material. |

## Security and operational trust

| Severity | Finding | Evidence | Required exit condition |
| --- | --- | --- | --- |
| **blocker** | Security posture is transitional and easy to misdescribe. | Default `ZG_POND_ENFORCE` is observe; unsigned/mismatched mutations are logged and allowed. Enforce mode serves the full API on clear HTTP with signed envelopes; TLS uses no client authentication. | Publish a threat model and exact mode matrix. Decide the supported default for the reference release, add adversarial/replay tests, and remove “mTLS-secured control plane” wording unless restored and enforced. |
| **blocker** | LAN binding and data-plane exclusions need a safe deployment boundary. | Moss binds control/data surfaces on the network; enforcement code excludes data-plane routes and health. | Document trusted-LAN/firewall assumptions, authentication coverage, secret handling, remote-access pattern, and safe exposure rules. Add a security checklist to install docs. |
| **important** | Update/release supply-chain controls are absent from the visible workflow. | No release workflow, artifact signing, SBOM, provenance, or image scanning gate was found. | Add least-privilege release automation, immutable digests, dependency/image scanning, SBOM, checksums, signing, and documented key rotation. |
| **important** | Security incident intake is undefined. | No SECURITY.md or private disclosure route found. | Provide supported-version policy, disclosure contact, expected acknowledgement window, and advisory process. |

## Maintainer capacity

| Severity | Finding | Evidence | Required exit condition |
| --- | --- | --- | --- |
| **blocker** | Support capacity is unknown relative to a very broad surface. | Repository spans host daemons, web UIs, containers, storage, security, firmware, Windows/Linux/Android, and several orchestrators; no support policy is present. | Owner sets weekly support budget, response target, supported matrix, and named experimental areas before choosing launch scale. |
| **important** | Bus factor and release authority are unstated. | No governance or maintainer roster. | Name at least one backup release reviewer or explicitly publish single-maintainer risk and continuity policy. |
| **helpful** | Cohort feedback has no designated owner. | No adoption campaign exists yet. | Assign one person to onboarding calls/issues and reserve two post-launch response windows. |

## Ordered repairs

1. **Choose and apply the legal contract.** Add root license/notice, reconcile OCI labels, and complete third-party asset attribution. Gate: automated license scan plus owner sign-off.
2. **Define the narrow support contract.** Select one Linux distribution/architecture, Docker version, reference machine, verified offering, security mode, and support budget. Gate: compatibility table and maintainer approval.
3. **Make a clean release reproducible.** Eliminate or pin the sibling Koi dependency; build signed/checksummed artifacts and images from a clean commit. Gate: fresh VM can verify, install, uninstall, and reinstall.
4. **Make first success truthful.** Replace the fictional quickstart; drive completion from real health; prove find/resolve and same-Stone reconstruction. Gate: CI/probe transcript plus uncut demo.
5. **Align public claims.** Correct counts, ports, security, failover, update, and platform wording; separate current docs from proposals/history. Gate: claim-ledger review has no unsupported launch claim.
6. **Harden operational trust.** Threat model, SECURITY/SUPPORT docs, firewall assumptions, enforced reference posture, release provenance, image/dependency scanning. Gate: maintainer security checklist and adversarial tests pass.
7. **Expand CI around what ships.** Format, warnings, frontends, orchestrators, container builds, installer, firmware compile/static checks, and probe smoke. Gate: protected clean release branch is green.
8. **Run a five-to-ten-person design-partner cohort.** Measure time to first healthy endpoint, recovery success, abandonment, and support load. Gate: at least 70% succeed unaided within 20 minutes, no unresolved data-loss/security issue, median maintainer help under 30 minutes per successful install.
9. **Only then stage public discovery.** GitHub release and repository metadata first; ecosystem submissions second; one anchor technical launch after the maintainer has capacity for follow-up.
