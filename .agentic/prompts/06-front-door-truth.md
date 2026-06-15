# 06 — Make the Front Door True

> A first-time builder following README → install → first service succeeds or is honestly told what's not
> ready. Zero fictional commands anywhere a newcomer walks. Phase: Truth. Depends on: 02 (releases exist),
> 04 (parked things aren't advertised), 05 (defaults are defensible). Blocks: any launch.

## Mission

The README's Getting Started block is fictional (image and env var that exist nowhere), the headline
connection string is unwired, and the two front-door guides (`first-stone.md`, `troubleshooting.md`)
contain 15+ verified mismatches with the shipped CLI — including commands that never existed. The
assessment's "first 30 minutes" audit found a newcomer hits five consecutive walls before reaching the
parts that genuinely delight. Rewrite the front door against the *actual* shipped surface, and make the
walk from "I have an old laptop" to "rake observe shows my stone" real on at least one platform.

This is a writing-against-ground-truth task. Every command you publish, you first RUN (or parse-validate
against the rake manifest). Every claim you publish, you verify in code.

## Ground truth (verified 2026-06-11 — re-verify each)

| Fact | Re-verify |
|---|---|
| README.md ~83-92 Getting Started: `zen-garden/stone:latest` image + `ANNOUNCE_SERVICE` env — both appear nowhere else in the repo; no Dockerfile builds that image | `grep -rn "ANNOUNCE_SERVICE\|zen-garden/stone" . --include="*" \| grep -v target \| grep -v README` |
| README ~25-31 headline `MONGODB_URI=zen-garden:mongodb/mydb`: parser exists (`src/common/src/uri/`), consumed only by its own test corpus; no resolver | `grep -rn "ZenGardenUri" src --include="*.rs" \| grep -v "src/common"` |
| `installer/install.sh` / `install.ps1` fetch `releases/latest` — functional only AFTER prompt 02's first tagged release is published | `grep -n "releases" installer/install.sh` |
| first-stone.md + troubleshooting.md: nonexistent commands (`discover`, `describe`, `take-away`, `renew-certificate`), wrong pond forms, ≥6 invalid flags (`--to`, `--via-lantern`, `--tail`, …), wrong API paths, wrong GitHub org, nonexistent NewStone parameters (`-StoneName`, `-Offering`) | spot-check 5: `grep -n "garden-rake discover\|take-away\|--via-lantern" docs/guides/first-stone.md docs/guides/troubleshooting.md` |
| The CLI's real surface: 36 commands defined in `src/rake/src/command_manifest.rs` (the single source of truth); per-command help via `garden-rake <cmd>?` | `grep -c "CommandDef" src/rake/src/command_manifest.rs` |
| The real working loop (verified live 2026-06-11 against a 12-stone garden): discovery → `garden-rake observe` → `garden-rake find <service>` → `garden-rake offer <offering>` → `list`, `config --field`, tending cache auto-recovery | — |
| 51 offering snippets across 18 categories embedded at `src/moss/embedded/manifests/sw/`; three docs disagree on the count (9 / 31 / 51) | `ls src/moss/embedded/manifests/sw \| wc -l` |
| The Rust self-installer (BUILD-0003) lives inside the moss binary (`src/moss/src/infra/installer/`, ~3.6k lines): fresh install/update/repair, Docker+avahi provisioning, systemd/SCM registration | `ls src/moss/src/infra/installer/` |

## Research first (~60 min)

1. Run the binary: build rake (`cargo build -p garden-rake`) and capture real output of `--help`,
   `observe`, `find mongodb`, `offer --help`, `pond --help`. Real output is your style guide.
2. Read `src/rake/src/command_manifest.rs` — command names, args, examples (note: ~25 of its examples are
   themselves stale; prompt 07 fixes them — do NOT copy manifest examples blindly, parse-check each).
3. Read the self-installer's actual flags/flows (`src/moss/src/infra/installer/`) and `installer/install.sh`
   to write a truthful install section.
4. Read `docs/introduction.md` and two `docs/journeys/` files for the project's narrative voice — the
   front door should keep that warmth while becoming factual.
5. Check prompt 02's release artifacts list (RELEASING.md / release.yml) for what is actually downloadable.

## Plan gate — OPERATOR decisions

1. **The headline.** The `zen-garden:` URI is the best pitch and is unwired (prompt 14 wires it). Options:
   (a) keep it as the vision statement clearly marked *(roadmap — see prompt 14's issue link)*, (b) replace
   the headline with the real working loop (`find mongodb` returning a live endpoint). Recommend (b) for
   the code block + one vision sentence retained.
2. **Quickstart platform**: which single path gets the tested, golden quickstart? Recommend: existing
   Linux box + `install.sh` (post-release) since it needs no imaging; NewStone USB imaging stays the
   "dedicate a machine" section.
3. Whether troubleshooting.md is rewritten now or deleted-and-stubbed ("see `garden-rake <cmd>?` and
   docs/guides/") until the CLI contract (07) settles. Recommend rewrite-thin: 10 real failure modes max.

## Target shape

README Getting Started (shape — verify each command parses before publishing):

```markdown
## Getting Started

# On a Linux machine you want to become a Stone (after first release):
curl -fsSL https://github.com/sylin-org/zen-garden/releases/latest/download/install.sh | sh
# (installs garden-moss, Docker + avahi if missing, registers the service)

# From your laptop:
garden-rake observe                 # discovers stones on the LAN — no config
garden-rake offer mongodb           # plants MongoDB on the best-fit stone
garden-rake find mongodb            # → mongodb on stone-quiet-pond (192.168.1.42:27017)
```

first-stone.md skeleton: 1) what you need (one old machine, one network), 2) path A — turn an existing
Linux install into a stone (install.sh), 3) path B — dedicate a machine with NewStone USB imaging
(Windows-authored, stated honestly), 4) first contact (`observe`), 5) first offering (`offer mongodb` →
`find`), 6) where next (pond when ready, storage, companions). Every command copy-pasteable; every output
block taken from a real run.

## Implementation

1. README: rewrite How-It-Works + Getting Started + fix the features table rows that reference parked
   items (pavilion/ai per prompt 04) and the wrong offering count (51); keep the e-waste mission framing
   — it is the project's best asset.
2. Rewrite `docs/guides/first-stone.md` from the skeleton; run every command against a local/dev stone
   where possible; mark anything release-dependent with the literal tag `(requires v0.1+)`.
3. Rewrite or thin `docs/guides/troubleshooting.md` per the OPERATOR decision — every entry: symptom →
   diagnosis command (real) → fix (real). Kill all 15+ fictional references.
4. Sweep the remaining newcomer path for the same fictions: `docs/introduction.md`,
   `docs/guides/installing-moss.md` (`grep -n "discover\|take-away\|ANNOUNCE_SERVICE" docs/ -r`).
5. Add a CI guard (extend prompt 02's workflow): a script that extracts fenced `garden-rake ...` commands
   from README + guides and runs each through `garden-rake <args> --help`-level parse validation (rake
   exposes clap parsing; a tiny `--validate-args` hidden flag or a test binary both work — pick the
   smallest; coordinate with prompt 07's example-parse test, same mechanism).
6. Commits: `docs(front-door): truthful README quickstart`, `docs(guides): rewrite first-stone against
   shipped CLI`, `docs(guides): factual troubleshooting`, `ci: parse-validate doc commands`.

## Definition of done

- [ ] `grep -rn "ANNOUNCE_SERVICE\|zen-garden/stone:latest" README.md docs/` → empty.
- [ ] Every fenced rake command in README/first-stone/troubleshooting parses against the built binary
      (paste the validation run).
- [ ] The 15+ verified mismatches are gone: re-run the Ground-truth spot-check greps → empty.
- [ ] Offering count consistent (51) across README/introduction/curated-offerings (or counts removed in
      favor of "50+").
- [ ] A newcomer-walk dry run is documented in the session report: each quickstart step with its actual
      result on your test environment.
- [ ] CI doc-command validation wired and green locally.

## Out of scope

Fixing the manifest's own 25 stale examples and exit codes (prompt 07). Wiring the URI (14). The deep
docs/ADR sweep (08). Philosophy essays (08). Building new install tooling (15 owns Linux/ARM artifacts).
