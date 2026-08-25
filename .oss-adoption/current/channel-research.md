Status: complete
Run ID: 20260719T034246Z
Project: zen-garden
Created: 2026-07-19T03:42:46Z
Verified at (UTC): 2026-07-19T05:10:00Z

# Channel research

All rules below were checked against current official channel documentation on 2026-07-19 UTC. “Prepare” means create/adapt the artifact but do not submit it; external publication remains unauthorized.

## Ranked channel portfolio

| Rank and channel | Audience and job | Official rules | Verified at (UTC) | Native format | Effort and support | Risk | Decision |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1. GitHub repository | Evaluators need source, trust files, install, issues, and canonical identity. | [Community profiles](https://docs.github.com/en/communities/setting-up-your-project-for-healthy-contributions/about-community-profiles-for-public-repositories); [topics](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/classifying-your-repository-with-topics) allow at most 20 lowercase/hyphen topics, 50 chars each. | 2026-07-19 | Tight README, supported matrix, demo, community files, topics, issue forms, optionally Discussions. | High one-time repair; continuing issue triage. | First impression exposes current license/quickstart/security drift. | **Do now on owned surface after product edits are separately authorized.** |
| 2. GitHub Releases + GHCR | Technical users need immutable artifacts and images tied to source. | [Releases](https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases) are tag-based deployable packages. [GHCR](https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry) supports anonymous public pulls, workflow token publishing, OCI source labels. | 2026-07-19 | Signed release notes, checksums/SBOM/provenance, multi-arch OCI images, immutable digests. | High release engineering; ongoing CVE/rebuild/tag work. | A weak artifact contract invalidates every downstream post. | **Prepare; release only after gates close.** |
| 3. Docker Hub mirror | Docker-native evaluators search by image and short description. | [Repositories](https://docs.docker.com/docker-hub/repos/) can be public/searchable; [creation rules](https://docs.docker.com/docker-hub/repos/create/) include a 100-character short description and names cannot be renamed. | 2026-07-19 | Discovery mirror of GHCR with identical tags/digests and source/docs links. | Medium; doubles registry hygiene. | Tag/digest divergence and abandoned images. | **Prepare after canonical namespace is owner-approved.** |
| 4. selfh.st directory / Self-Host Weekly | High-fit self-hosters seek maintained software and release news. | [Submission hub](https://selfh.st/submit/) separates newsletter and directory routes; [directory description](https://selfh.st/apps-about/) emphasizes self-hosted software/companions and active maintenance. | 2026-07-19 | Concise factual metadata, screenshot, install link, requirements, license, maturity, release change. | Medium; curator follow-up and incoming issues. | Premature listing magnifies install friction. | **Best curator; prepare until install-ready release.** |
| 5. Show HN | Builders/operators can provide deep technical feedback on a runnable project. | [Show HN rules](https://news.ycombinator.com/showhn.html): maker's own work, present to discuss, non-trivial and runnable, ideally no signup, title starts Show HN, no landing-page-only/fundraiser post. [HN guidelines](https://news.ycombinator.com/newsguidelines.html): no solicitation of votes/comments; generated or AI-edited comments are prohibited. | 2026-07-19 | Maintainer-written `Show HN: ...` post linking directly to runnable repo/demo, architecture note, precise limits. | Very high same-day response load. | Harsh response to broken install or inflated claims; maintainer must personally write/rewrite post and replies. | **Prepare as one anchor launch after cohort proof.** |
| 6. r/selfhosted | Practitioners test install, data ownership, maintenance, and integrations. | [Live subreddit rules](https://www.reddit.com/r/selfhosted/about/rules.json) require self-hosted relevance, production readiness/docs, useful description, and limited promotion; projects under three months belong in the current New Project Megathread. [Reddit spam policy](https://support.reddithelp.com/hc/en-us/articles/360043504051/Spam) requires authentic, relevant participation. | 2026-07-19 | Affiliation disclosure; problem/use case; tested install; persistence/security/limits; request for specific feedback. | High for several days. | Self-promotion sensitivity; “production-ready” bar; copied cross-posts. | **Prepare, then verify live rules again at posting.** |
| 7. r/homelab Project Showcase | Hardware experimenters are the best audience for the heterogeneous-fleet proof. | [Live rules](https://www.reddit.com/r/homelab/about/rules.json) plus [moderator process](https://www.reddit.com/r/homelab/comments/1ty58af/announcement_new_rules_processes_on_software/): personal non-commercial projects, software showcase flair, undisclosed minimum karma, repo with at least 30-day history and screenshots, problem/alternatives/homelab relevance, and AI-role disclosure. | 2026-07-19 | Real build log: three mismatched devices, hardware-aware placement, failure/recovery, power/resource numbers, limitations. | High response and proof burden. | Strict anti-advertising checks; flair/rules can change. | **Watch/prepare; participate genuinely before posting.** |
| 8. AlternativeTo | Searchers comparing Portainer, Uncloud, Coolify, CasaOS, and related tools. | [FAQ](https://alternativeto.net/faq/) allows self-suggestion; account must age one week; released/open-beta easy-access products only; English metadata; profile advertising prohibited; admin review follows. | 2026-07-19 | Exact category, platforms, license, maturity, description, and fair alternatives. | Low-medium, plus accuracy maintenance. | Wrong category or premature listing creates misleading comparisons. | **Prepare after first release.** |
| 9. Changelog News | Developers seek technically interesting open-source work. | [Submission form](https://changelog.com/news/submit) requires an account, URL/title/why-interesting; creators may submit their own work; tutorials/how-tos and commercial products are excluded; selection is editorial. | 2026-07-19 | Short editorial pitch around local-first service intent and a concrete architecture/recovery proof. | Medium. | Generic product launch has low editorial value. | **Prepare after release and a durable technical artifact.** |
| 10. This Week in Rust | Rust contributors may value the architecture and bounded-context implementation. | [Official repository](https://github.com/rust-lang/this-week-in-rust) accepts draft PRs for substantive project/tooling updates and contributor CFPs; a CFP needs OSI license, public tracker, scoped task/difficulty, and contribution guide. | 2026-07-19 | Architecture article, noteworthy Rust implementation update, or tightly scoped contributor call. | Medium; contributor onboarding required. | Raw product promotion is poor fit; current license/CONTRIBUTING gaps block a CFP. | **Prepare a technical story later.** |

## Channel adapters

### GitHub front door

- Subtitle: “Local-first service intent and continuity for a small fleet of mismatched self-hosting hardware.”
- Suggested topics after verification: `self-hosted`, `homelab`, `docker`, `service-discovery`, `orchestration`, `edge-computing`, `rust`, `raspberry-pi`, `local-first`, `ollama`.
- Above the fold: 90-second mental model, current maturity badge, supported reference path, install command, uncut demo, exact non-claims, security boundary, and `works with` matrix.
- Use issue forms for install failure, offering compatibility, security contact redirect, and integration proposal. Enable Discussions only if the maintainer can moderate it.

### Release/registry adapter

- GHCR is source of truth; Docker Hub is a discovery mirror. Publish identical immutable digest references.
- Release notes include supported OS/architecture, required Docker/Rust/Koi state, upgrade/rollback, data boundary, security changes, known issues, checksums, SBOM/provenance, and support route.
- Never use `latest` as the only documented install. Separate `stable`, release-candidate, and edge semantics.

### selfh.st adapter

- Submit only after a curator can complete the quickstart without private context.
- Provide name/subtitle, repository, documentation, license, current release, install methods, screenshot/demo, supported hardware, no-cloud-account/local-first distinction, and candid pre-release status.
- Pitch the heterogeneous small-fleet story, not “another home-server OS.”

### Show HN adapter

- Direct link to the repository or runnable demo; no marketing landing page required.
- Demonstrate three mismatched machines, one capability-aware placement, service resolution by intent, and same-Stone container reconstruction.
- State at the top: 0.2 pre-release, Linux reference path, no generic stateful failover, default trusted-LAN boundary.
- The maintainer must personally author/rewrite the submitted text and all HN comments; this draft is research material, not compliant copy for direct posting.

### Reddit adapters

- Write separate native posts. Never paste the same announcement into both communities.
- r/selfhosted: emphasize install, persistence, update expectations, external-tool ownership modes, license, and security. Ask for feedback on the one-Stone journey and Compose import.
- r/homelab: emphasize the physical build, hardware inventory, why existing tools did not answer the mixed-fleet intent problem, measured resource/power behavior, screenshots, and failure exercise. Disclose maintainer affiliation and AI assistance according to the live rule.
- Recheck the rules JSON, megathread age rule, flair name, and karma requirement on posting day.

## Rejected or deferred channels

- **awesome-selfhosted — avoid submission.** Its [contribution rules](https://github.com/awesome-selfhosted/awesome-selfhosted-data/blob/master/CONTRIBUTING.md) explicitly route generic container/deployment/virtualization tools toward awesome-sysadmin and require the first release to be older than four months. Zen is not currently a fit.
- **awesome-sysadmin — watch for organic eligibility.** Its [rules](https://raw.githubusercontent.com/awesome-foss/awesome-sysadmin-data/master/CONTRIBUTING.md) target professional use, require a working install and first release older than 12 months, and restrict self-submission absent a healthy independent ecosystem. A future user should submit it with actual use/pros/cons/scale.
- **Lobsters — do not drive-by launch.** [About/rules](https://lobste.rs/about) describe an invite-tree computing community, self-promotion below 25%, restrictions for new accounts/domains, and human-authored discussion. Participate genuinely and later share a durable technical article if appropriate.
- **Product Hunt — defer.** Broad launch traffic is poorly matched to current install/support friction and the technical self-hosting niche.
- **Console.dev — watch.** Its [selection criteria](https://console.dev/selection-criteria) emphasize developer-primary, self-service, secure, documented tools; Zen's first user is an operator and its trust gates are still open.
- **Podcasts as a launch dependency — avoid.** Self-Hosted's [official page](https://www.jupiterbroadcasting.com/show/self-hosted/) shows its last episode in 2025; other niche shows inspected were older. [Linux Unplugged](https://linuxunplugged.com/) is active and relevant, but no official project-pitch route was verified; treat coverage as earned after traction.
- **Mass directory submission, paid placements, cold influencer outreach, and identical cross-posts — reject.** They create support load without validating first success and conflict with community norms.

## Facts, inferences, and hypotheses

**Verified facts:** the official rule links and native-format constraints above; GitHub/Docker registry mechanics; selfh.st submission routes; Show HN authorship/tryability rules; current subreddit rules; awesome-list eligibility constraints.

**Inferences from repository and market evidence:** selfh.st is the highest-fit curator; Show HN is the strongest one-time technical anchor; r/homelab may deliver the best hardware-placement feedback; GitHub/GHCR must precede every borrowed audience; broad launch before the legal/install/security gates would create negative trust rather than useful adoption.

**Hypotheses to test in the cohort:** target users understand “service garden” without excessive new vocabulary; adoption/borrowing is more valuable than a larger built-in catalog; the same-Stone recovery demo is sufficiently differentiated; users prefer Tailscale/Consul exporters over a new cross-subnet control plane; support remains sustainable below ten early users.
