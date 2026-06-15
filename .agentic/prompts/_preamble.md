# Session Preamble — paste before any prompt in this stash

You are working in **Zen Garden** (repo root = current directory): an open-source Rust toolset for
self-hosted infrastructure on repurposed hardware — service discovery, orchestration, and failure recovery
for homelabbers, first-time builders, data-sovereignty users, and small teams. It is pre-alpha: zero
external users, one maintainer. The repo doubles as the maintainer's lab; treat anything not covered by
your prompt as someone else's bench.

## Vocabulary (stable — never rename these)

| Term | Meaning |
|---|---|
| **Stone** | A device offering services (laptop, thin client, Pi, Android phone) |
| **Moss** | The daemon on each stone — HTTP :7185, HTTPS :7183 when Pond is active |
| **Rake** | The CLI (`garden-rake`) |
| **Lantern** | Optional topology dashboard |
| **Pond** | Opt-in mTLS trust boundary (private CA) |
| **Offering** | A curated service template, deployed as Docker container `zen-offering-{name}` |

## Repo map (the parts that matter)

```
src/common/      garden-common — shared contracts + (currently) much more; workspace crate
src/moss/        the daemon: domain/ infra/ api/ tasks/ bootstrap/ docker/
src/rake/        the CLI: command_manifest.rs (declarative), commands/, connection/
src/discovery/   client-side stone discovery (DISC-0001)
src/lantern/ src/cricket/ src/firefly/ src/companion-sdk/ src/companion-usb/ src/probe/ src/pavilion/
src/orchestrators/   STANDALONE crates (excluded from root workspace): ollama, mongodb, ai, common, + stubs
installer/       PowerShell build/deploy pipeline + Rust self-installer lives in moss
docs/            decisions/ (ADRs), specs/, guides/, philosophy/, notes/
.agentic/        AI bootstrap: CONTEXT.md (read it), rules/, reference/
```

## Mandatory reading before you code

1. `.agentic/CONTEXT.md` — critical rules (shared models, layering, paths, async I/O, error handling).
2. `docs/code-standards.md` — naming, namespacing, channel conventions, unwrap discipline. This file is law.
3. Your prompt's **Ground truth** section — then run its Re-verify commands. The facts were verified
   2026-06-11 against commit-era state; if any fail, STOP and report instead of improvising.

## Working rules for this stash

- **Greenfield posture**: no compat shims, no deprecation bridges, no commented-out code. Delete cleanly;
  git history is the archive. Prefer rebuilding a small thing well over patching a wrong thing.
- **Scope discipline**: touch only the directories your prompt lists. If you notice adjacent problems,
  write them into a `FINDINGS.md` note at the repo root and keep moving — do not fix them.
- **Verification over confidence**: after every meaningful change run
  `cargo check --workspace` (root) and the prompt's own checks. Orchestrator crates build separately:
  `cd src/orchestrators/<name> && cargo check`.
- **Never** run `cargo publish`, push to remotes, force-push, or delete remote branches unless the prompt
  says so explicitly. Local commits are encouraged: small, one concern each, conventional format
  (`feat:`/`fix:`/`refactor:`/`docs:`/`chore:`), imperative mood.
- **OPERATOR items**: when your prompt marks a decision OPERATOR, present options and stop. Do not pick.
- **Tests**: keep the 2,400+ existing unit tests green. New behavior gets a test in the same file
  (`#[cfg(test)]` module), per existing convention.
- Ignore `target/` directories everywhere. Do not commit artifacts, scratch files, or logs.

## Definition-of-done etiquette

End your session by reporting: what changed (files + line deltas), every verification command you ran with
its actual output, what remains (if anything), and any FINDINGS.md entries you left. Honest partial > fake
complete.
