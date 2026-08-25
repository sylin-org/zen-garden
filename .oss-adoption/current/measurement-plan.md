Status: complete
Run ID: 20260719T034246Z
Project: zen-garden
Created: 2026-07-19T03:42:46Z

# Measurement plan

## Decisions this measurement should support

Measurement exists to answer five operational decisions, not to maximize vanity metrics:

1. Can a qualified self-hoster reach a healthy endpoint and understand the recovery boundary without maintainer rescue?
2. Does the heterogeneous-garden/service-intent positioning attract the intended user rather than people seeking a generic Docker UI or production Kubernetes replacement?
3. Is adoption/borrowing existing services more valuable than expanding the built-in catalog?
4. Which collaboration surface should be built first: Compose/manager ownership labels, Tailscale Services, Consul/DNS-SD, Runtipi/Umbrel/CasaOS, Uncloud, Syncthing, or Ollama hardening?
5. Can the maintainer support the resulting users within the declared weekly budget without degrading engineering or safety?

No product telemetry is proposed. Use aggregate public platform data, release/download counts, issue labels, opt-in cohort worksheets, and short voluntary surveys. Do not collect LAN topology, service names, IPs, hardware serials, model prompts, file paths, logs, or usage events by default.

## Baseline

Public baseline checked on 2026-07-19 UTC:

- GitHub repository is public on branch `dev`, with 1 star, 0 forks, 0 open issues, 2 pull requests, and 1,135 commits visible.
- GitHub reports no published releases and no packages.
- Local `git tag --list` returned no tags.
- No canonical Docker Hub/GHCR artifacts, adoption campaign, public cohort, release funnel, or verified external listing was identified.
- Repository inventory contains 1,985 tracked/seen files, 909 Rust files, 510 documentation files, 30 test files, and one CI workflow.
- Existing real-user deployments, unique cloners, support time, install success, retention, and failure-recovery success were not supplied. Record these only prospectively; do not invent a historical baseline.

GitHub traffic graphs retain limited windows and clone counts are noisy; export aggregate owned metrics on the same weekday after each campaign while respecting GitHub's terms. Stars are context, never a success gate.

## Funnel signals

| Stage | Signal | Source | Baseline | Review window | Decision threshold | Privacy note |
| --- | --- | --- | --- | --- | --- | --- |
| Reach | Qualified repository/referral visits | GitHub Insights and channel referral data where available | Not recorded prospectively | 7 days per activity | Diagnostic only; no target | Aggregate platform data only |
| Evaluate | README→install/demo engagement and substantive architecture questions | Release asset views/downloads, demo host aggregate views, issue/discussion labels | No public release/demo | 7 and 30 days | At least 10 qualified evaluators before judging positioning | No cross-site fingerprinting or user-level tracking |
| Install | Verified artifact pull/download plus explicit install attempt | GHCR/Docker Hub/GitHub aggregates; opt-in “attempted” issue/survey | No package/release | Cohort session; 7 days launch | Cohort: 5+ attempts; public: report count, not quota | Registry/platform aggregates; deduplicate only when platform supplies it |
| First success | Healthy reference offering resolved within 20 minutes unaided | Cohort worksheet; optional success issue/discussion reaction | No measured users | Each cohort; monthly public | At least 70% of cohort attempts | Record coarse OS/architecture only with consent |
| Recovery proof | User removes managed container and observes documented same-Stone reconstruction | Cohort worksheet; opt-in probe output | No measured users | Each cohort/release | At least 80% of successful installs | Never request secrets/full logs; provide redaction tool/instructions |
| Correct understanding | User distinguishes reconstruction from machine/state failover and identifies lifecycle owner | Two-question cohort interview/survey | Not measured | Cohort and 30-day survey | At least 80% answer both correctly | Voluntary, anonymous response allowed |
| Activate integration | User adopts an existing service or exports one endpoint through an adjacent tool | Issue/discussion poll; integration-specific opt-in report | No adapters released | 30 days | Three independent requests or two successful uses before prioritizing an adapter | No automatic inventory upload |
| Retain | Garden still running and useful after 30 days | Voluntary 30-day check-in | Not measured | 30 days | At least 50% of successful cohort users respond “still useful,” with reasons | Do not chase nonresponders or infer failure |
| Contribute | Reproducible issue, docs correction, offering/integration PR, independent review | GitHub issues/PRs/contributors | 0 open issues; 2 PRs at check | 30 and 90 days | Quality matters; one independent accepted contribution is meaningful | Public contribution data only |
| Support cost | Maintainer minutes per successful first install and weekly triage hours | Private time ledger using categories, not user content | Not measured | Weekly | Median <30 min/install; total within owner budget | Store duration/category, no message transcripts |
| Safety | Security, data-loss, privilege, corrupt restore, or misleading-readiness incident | SECURITY intake and labeled issues | Intake absent | Continuous | Zero unresolved P0; one P0 pauses promotion | Keep disclosure private until coordinated |
| Channel fit | Qualified installs/issues per maintainer hour, plus tone/relevance | Campaign ledger | No campaign | 7 and 30 days after channel | Continue only if learning or adoption justifies support | Aggregate outcomes, no profiling commenters |

## Campaign ledger

Create one row per authorized external activity when it occurs. Keep source links and exact artifact commit so claims can be reconstructed.

| Activity ID | Date | Channel | Artifact/version | Purpose | Spend | Reach | Attempts | First successes | Recovery proofs | Support hours | Safety events | Decision |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 20260719-audit | 2026-07-19 | Internal playbook only | Commit `1fb8205`; run `20260719T034246Z` | Establish strategy and baseline | $0 | Not applicable | 0 | 0 | 0 | Audit time not used as launch support | 0 observed by campaign | Prepare; no publication |

For each future row, link a short qualitative note: top three questions, top three failures, unexpected audience, integration votes, changes shipped, and whether the channel rules were reverified that day.

## Continue, adapt, pause, and stop rules

**Continue** a release/channel sequence when first-success and recovery thresholds pass, no P0 is open, support remains within budget, and questions come from the intended user/problem.

**Adapt** positioning or docs when qualified users install successfully but confuse Zen with an app store/Kubernetes replacement, cannot name the lifecycle owner, or repeatedly ask for the same missing integration. Change one major variable per next activity so learning remains attributable.

**Pause** all promotion when unaided cohort first success falls below 70%, recovery below 80%, median support exceeds 30 minutes per successful install, the maintainer misses the stated response window, an artifact/tag/digest drifts, or a reproducible P1 install/security issue affects the supported path.

**Stop and warn** when a P0 security/data-loss/supply-chain defect appears, claims materially misrepresent recovery/security, or support demand exceeds the owner's hard weekly cap for two weeks. Publish a visible advisory on already-used owned surfaces once disclosure safety allows; do not quietly delete evidence.

**Deprioritize a channel** after two well-adapted attempts if it produces neither qualified learning nor successful intended-user installs, or if support/moderation cost is disproportionate. Do not optimize posts for raw traffic to rescue a low-fit channel.

**Graduate a capability claim** only after a versioned automated test plus at least two independent real-world confirmations on the declared matrix. Service-specific HA additionally needs published failure conditions and measured RPO/RTO.

## Review cadence and owner

- **Release day:** release owner watches install, registry, security, and digest integrity.
- **48 hours after any channel:** adoption owner triages failures/questions and decides whether the next activity remains scheduled.
- **Weekly for the first month:** project owner reviews funnel, support time, P0/P1 issues, and integration demand.
- **At day 30:** decide continue/adapt/pause/stop; publish an honest learning note if a public campaign occurred.
- **Monthly thereafter:** refresh image/security state, first-success proof, claim ledger, supported matrix, and channel backlog.
- **Each release:** rerun install → health → resolve → same-Stone reconstruction from a clean reference machine.

Named people are an owner decision. Until assigned, roles mean: project owner (scope), release owner (artifacts/security), adoption owner (cohort/channels), and backup reviewer (continuity). No external activity should start with an unstaffed role.
