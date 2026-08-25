Status: complete
Run ID: 20260719T034246Z
Project: zen-garden
Created: 2026-07-19T03:42:46Z
External publication authorized: no

# Publication and adoption plan

## Strategy and sequencing rationale

Zen Garden should earn adoption through proof, not volume. Its niche requires trust with local networks, persistent data, and old hardware; a broken install or inflated recovery/security claim is more damaging than low initial reach. The sequence therefore moves from owned truth to a small observed cohort, then durable ecosystem surfaces, one high-attention technical anchor, adapted communities, and finally earned curation.

The launch unit is not “the entire garden.” It is one crisp autonomy loop:

> install one supported Linux Stone → plant one verified offering → wait for real health → resolve it by intent → remove the container → see Moss reconstruct it on the same Stone.

The larger vision—heterogeneous placement, adopted/borrowed services, Tailscale/Consul exporters, Ollama routing, MongoDB choreography, companions, storage, and multiple execution backends—supports the roadmap and collaboration story. It must not blur what the release candidate proves.

Do not publish on multiple borrowed channels at once. Leave at least three working days between substantive appearances, longer after any P0/P1 failure, so the maintainer can repair the product and incorporate questions into the next adapter.

## Phase 0: repair readiness blockers

Objective: convert the repository from a substantial pre-release codebase into a narrow, reproducible, legally and operationally coherent release candidate.

- Choose license/ownership terms; add root license/notice; audit bundled assets; reconcile OCI labels.
- Select one Linux reference path, one architecture (plus any genuinely verified extras), one Docker range, one verified offering, and one supported Pond/security posture.
- Pin or publish Koi; make a fresh clone self-contained under documented prerequisites.
- Replace the fictional quickstart; drive success from application health; prove same-Stone reconstruction and state boundary.
- Rewrite README claims/counts/ports/security/platform support from the claim ledger.
- Add SECURITY, SUPPORT, CONTRIBUTING, CODE_OF_CONDUCT, governance/maintainer, issue/PR templates.
- Add release workflow, checksums, signing/provenance, SBOM, immutable OCI images, installer/uninstaller smoke, image/frontend/probe gates.
- Produce a clean transcript and uncut two-minute demo from the release candidate.

Exit only when every blocker in [readiness-audit.md](readiness-audit.md) is closed by evidence, not merely by a roadmap note.

## Phase 1: validate with a small cohort

Recruit five to ten high-fit design partners individually from existing relationships or opt-in project followers. Do not cold-message community member lists. Seek variation: a Pi/ARM user, an x86 thin-client user, a multi-NIC user, a GPU/Ollama user, and one operator with Portainer/Runtipi/Umbrel/Coolify/Komodo already in place.

Use a consented 30-minute observation or an asynchronous install worksheet. Collect no automatic telemetry. Record environment in coarse, non-identifying categories. Ask participants to narrate what they think “Stone,” “Offering,” and “garden” mean; time the first healthy endpoint; delete the reference container; ask who they think owns updates/data; then show the collaboration map.

Cohort gate:

- at least 70% complete first success unaided in 20 minutes;
- at least 80% of successful installs reconstruct the missing runtime as documented;
- no unresolved data-loss, remote-exposure, privilege, or supply-chain P0 issue;
- median maintainer assistance below 30 minutes per successful install;
- at least three participants can accurately distinguish same-Stone reconciliation from machine failover;
- owner affirms that observed weekly support load is sustainable.

If the gate fails, pause publication and fix the top shared failure. Do not increase cohort size to hide conversion problems.

## Phase 2: seed ecosystem surfaces

After cohort success, create durable canonical discovery in this order:

1. GitHub tagged release, release notes, verified assets, social preview, topics, community profile.
2. Public GHCR packages tied to source; Docker Hub mirror using the same digests.
3. selfh.st/apps directory submission and, if newsworthy, Self-Host Weekly release submission.
4. AlternativeTo entry with honest closest alternatives and pre-release/stable status.
5. One host-platform recipe or adapter—Runtipi/Portainer is a practical first candidate—and a documented Compose ownership contract.

Each surface must link back to one canonical support and install route. If registry or docs drift, halt further seeding until they agree.

## Phase 3: anchor launch

Prepare one Show HN launch only after the release is self-service and the maintainer can reserve launch day plus the following morning. Link directly to the repository/demo. Lead with the mixed-hardware problem and working recovery loop. Compare fairly with Uncloud, Komodo, Portainer, K3s, home-server platforms, and Tailscale Services. State pre-release maturity, trusted-LAN/security boundary, and non-claims near the top.

HN prohibits generated or AI-edited comments. The maintainer must personally author or completely rewrite the submission comment and every reply; the playbook draft can inform facts only. Never ask for votes or coordinate engagement.

Anchor stop condition: pause replies only for sleep/safety; if a reproducible security/data-loss defect appears, add a visible warning, stop promotion, and ship/communicate the fix before using another channel.

## Phase 4: adapt for high-fit communities

Wait until Show HN feedback has been triaged. Then choose at most one community per week:

- **r/selfhosted:** an affiliation-disclosed operational post covering installation, persistence, ownership modes, security, limits, and a specific feedback request. Verify the current project-age/megathread rule on posting day.
- **r/homelab:** a distinct hardware build report with screenshots, machine inventory, resource/power observations, placement decision, and failure exercise. Meet karma/history/flair/AI-disclosure rules. It should not read like launch copy.

Do not cross-post identical text. Do not post merely because a calendar says so. A community adapter proceeds only when there is new proof or a question that community is uniquely positioned to answer.

## Phase 5: enable earned discovery

- Submit a concise, factual item to Changelog News after a real release or noteworthy integration.
- Submit a Rust architecture article or scoped contributor CFP to This Week in Rust only after the license and contributor path are complete.
- Make curator assets easy to adapt: release facts, demo, supported matrix, screenshots, license, maintainer contact, comparison boundaries.
- Watch for organic coverage by Linux Unplugged or similar active outlets; do not depend on cold podcast pitching.
- Let a real operator eventually propose awesome-sysadmin with actual scale/pros/cons after the project's release age and ecosystem qualify. Do not submit to awesome-selfhosted against its scope rule.

## Phase 6: maintain and learn

- Reserve two support windows after every channel appearance and publish response expectations.
- Review install/recovery failures weekly for the first month, then monthly.
- Rebuild images for material CVEs and keep registry tags/digests synchronized.
- Re-run the first-success and recovery scenario for every release.
- Refresh market/channel rules before each submission; record the date and artifact version.
- Update the claim ledger when behavior, platform support, or evidence changes.
- Publish a short 30-day learning note: who succeeded, where they failed, which integrations were requested, what will not be built, and the next narrow proof.

## Activity ledger

| Activity | Purpose | Audience | Artifact | Owner | Prerequisite | Success signal | Stop condition | Follow-up |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Legal/release contract | Establish redistribution and artifact trust. | All evaluators | LICENSE/NOTICE, asset ledger, release workflow, SBOM/signatures | Maintainer + legal owner | License decision | Clean automated audit; artifacts verify on fresh host | Any ownership conflict | Resolve before distribution |
| Truthful front door | Make the first five minutes match reality. | New evaluators | README, support matrix, security/limitations pages | Maintainer | Claim-ledger approval | A new reader describes the bounded promise correctly | Any fictional command or overclaim remains | Run docs/command contract checks |
| Reference proof | Demonstrate the launch unit. | Operators/curators | Transcript, probe result, two-minute uncut demo | Release owner | Healthy reference offering and release candidate | Repeatable install/readiness/resolve/reconstruct | State loss, false readiness, or undocumented privilege | Fix and rerun from clean VM |
| Design-partner cohort | Validate usability and support cost. | 5–10 opt-in self-hosters | Worksheet, consent note, anonymized findings | Adoption owner | Phase 0 gates | Cohort thresholds met | P0 defect or unsustainable help time | Repair shared bottleneck |
| GitHub release | Create canonical versioned source/artifacts. | Technical evaluators | Tag, release notes, checksums, packages | Release owner | Cohort gate | Verified installs from public assets | Artifact mismatch/CVE/P0 bug | Yank/warn/fix transparently |
| GHCR + Docker Hub | Meet native container install expectations. | Docker operators | Mirrored multi-arch OCI images | Release owner | Canonical namespace/digest policy | Successful anonymous pulls and smoke runs | Digests/tags diverge | Repair mirror before outreach |
| selfh.st submission | Reach high-intent self-hosters through a trusted curator. | Selfh.st readers | Directory metadata + optional release item | Adoption owner | Public release + screenshot/docs | Accepted listing and qualified installs | Curator flags readiness mismatch | Correct source facts |
| AlternativeTo listing | Capture durable comparison intent. | Tool evaluators | Structured listing | Adoption owner | Accessible release | Accurate approved listing; referral installs | Wrong category/claims | Amend or withdraw |
| Show HN | Gather deep technical feedback and establish narrative. | Builders/operators | Maintainer-authored post + direct demo | Maintainer | Self-service release; support day reserved | Qualified installs/questions and useful issue reports | Security/data-loss/reproducible install break | Warn, pause, fix |
| r/selfhosted post | Validate operational fit. | Self-hosters | Native affiliation-disclosed post | Maintainer | Live rule check + feedback fixes | Install/recovery reports and integration requests | Moderation concern or support overload | Respond, summarize, update docs |
| r/homelab showcase | Validate hardware story. | Homelab builders | Evidence-heavy build report | Maintainer | Real multi-device proof + eligibility | Hardware-specific feedback/reproductions | Flair/rule/AI-disclosure mismatch | Correct before reposting |
| Technical/curator brief | Enable earned discovery and contributors. | Changelog/TWiR/curators | Fact sheet + technical article/CFP | Maintainer/editor | Newsworthy proof + contribution path | Editorial pickup or qualified contributor | Generic promotional framing | Rework around useful technical content |
| 30-day review | Decide continuation and scope. | Maintainers/contributors | Measurement review + public learning note | Project owner | One complete campaign window | Clear keep/adapt/stop decisions | Metrics cannot answer a decision | Simplify measurement |

## Decisions requiring owner approval

1. Chosen license and authority to distribute every bundled asset/image.
2. Supported reference Linux distribution, architecture, Docker range, hardware, and first verified offering.
3. Release security posture and trusted-network/remote-access boundary.
4. Canonical version/tag policy and GitHub/Docker registry namespaces.
5. Maintainer/support owner, weekly support budget, response expectations, and launch-day availability.
6. Whether a 0.2.0 release candidate waits for all blocker repairs or uses a later version.
7. Which external manager receives the first formal adapter and whether Tailscale Services is the first network exporter.
8. Cohort invitation list and consent/feedback process.
9. Approval for each external submission. This plan grants none; external publication remains unauthorized.
