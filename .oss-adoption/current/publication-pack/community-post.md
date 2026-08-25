Status: complete
Run ID: 20260719T034246Z
Project: zen-garden
Created: 2026-07-19T03:42:46Z

# Community post draft

Target community: r/selfhosted after a verified accessible release

Current rules verified: 2026-07-19 UTC via https://www.reddit.com/r/selfhosted/about/rules.json and https://support.reddithelp.com/hc/en-us/articles/360043504051/Spam

Publication gate: closed. Recheck the live rules, current New Project Megathread/project-age treatment, release URL, and factual release scope immediately before any owner-authorized submission.

## Draft

**Proposed title:** I built Zen Garden to keep services understandable across mismatched homelab machines

Hi r/selfhosted — I maintain Zen Garden, so this is an affiliated project post.

My homelab problem was not installing another app on one server. It was keeping track of services across old laptops, thin clients, Pis, and one machine with a useful GPU. The hardware changes, ports collide, and some containers are already owned by another tool. I wanted the service intent to survive those details without operating Kubernetes or depending on a hosted control plane.

Zen Garden runs a Moss daemon on each “Stone.” It discovers the garden on the LAN, evaluates host capabilities against curated offering definitions, and exposes services through Rake/Lantern. A service can be:

- **managed** by Zen, which owns its container lifecycle;
- **adopted read-only** from an existing host/tool;
- **borrowed** as an external dependency.

That ownership distinction matters: Zen should not fight Portainer, Komodo, Coolify, Runtipi, Umbrel, CasaOS, Cosmos, or YunoHost for control of the same service.

The narrow recovery behavior I am asking people to test is deliberately less dramatic than “high availability”: if a managed container disappears while its Stone and persistent data survive, Moss can reconstruct the runtime from its stored offering intent and reuse its port assignment. This is **not** generic machine failover, not a substitute for backup, and not a promise of lossless state migration.

The inspected 0.2 codebase includes 51 checked-in offering templates across 18 categories, a Rust CLI/daemon/discovery stack, a garden dashboard, storage and snapshot work, USB/audio/display companions, and experimental Ollama and MongoDB orchestrators. Catalog presence does not mean every app/platform combination is certified; the release's supported matrix and verified subset are the contract.

Repository and tested install path: https://github.com/sylin-org/zen-garden

The first test takes one supported Linux/Docker Stone through install → healthy reference offering → endpoint resolution → manual container deletion → same-Stone reconstruction. The release notes should include checksums, image digests, persistence boundary, uninstall, security assumptions, and known limitations.

I would especially value feedback from people who:

1. run two or more genuinely mismatched machines;
2. already use a container manager or home-server app platform and want Zen to adopt rather than replace it;
3. can tell me whether explicit lifecycle ownership plus Compose labels would solve a real interoperability problem;
4. are willing to report the first command or concept that becomes confusing.

I will not ask you to test cross-Stone stateful failover or expose Moss directly to the internet. Those claims are not part of the reference release. If you try it, please redact IPs, service names, secrets, and full logs before opening an issue.

## Rule and tone check

- Affiliation is disclosed in the first line.
- The post leads with a self-hosting problem and useful architecture/ownership context before the link.
- It states the exact recovery boundary and catalog maturity instead of promising production readiness.
- It contains no vote/comment solicitation, tracking link, copied cross-post language, or attack on alternatives.
- It asks for specific technical/usability feedback and names privacy-safe reporting.
- It must not be submitted until a release satisfies the subreddit's live released/production-ready/documentation rule and any project-age placement rule.
- Any AI-assistance disclosure required by the live community or owner policy must be added clearly; do not conceal this playbook's role.

## Follow-up plan

- Maintainer reserves two response windows daily for three days and answers every reproducible install/security question.
- Label reports by install, readiness, recovery, ownership/integration, docs, security, or out-of-scope expectation.
- Add a visible warning and pause promotion for a reproducible P0/P1 supported-path defect.
- At 72 hours, post one concise reply summarizing confirmed issues and links to fixes; do not bump for attention.
- Convert repeated confusion into docs/claim-ledger changes before considering a distinct r/homelab showcase.
- Record aggregate attempts, first successes, recovery proofs, support hours, and safety events in the measurement ledger.
