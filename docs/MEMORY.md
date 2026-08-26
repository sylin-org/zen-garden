# MEMORY — project-level, model-agnostic memory index

Per-agent caches are caches only. Durable truth lives here and in the docs
this file points at; state lives once, decisions live once.

## Pointer index

| Topic | Where |
|---|---|
| Standing preferences for v1 work | this file, below |
| Normative lessons (L1–L25) | `docs/v1/lessons.md` |
| Charter + amendments | `docs/v1/CHARTER.md` |
| Engineering law (P0–P5, R-rules) | `docs/v1/CODE-RULES.md` |
| Offerings design law | `docs/v1/OFFERINGS.md` |
| Offering Directory decision | `docs/v1/decisions/ADR-0001-offering-directory.md` |
| Open engineering debt | `src/v1/DEBT.md` |
| Witnessed milestones | `src/v1/WITNESSES.md` |
| Sequenced slices (O0→later) | `docs/v1/OFFERINGS.md` §7 |
| Session-local environment/handoff | `local/NOTES.md` (gitignored), see `local/README.md` |

## Standing preferences

- Trust files over summaries: re-verify against the tree at every session start
  (`git log --oneline`, workspace clippy/tests) before continuing prior work.
- One task = one commit; commits follow existing style (`feat(v1): …`) and the
  pipeline must be clippy `-D warnings` clean at every commit (R4.1).
- Keep ceremonies (multi-step shell work) short and discrete — long compound
  PowerShell commands get killed on this machine (see `local/NOTES.md`).

## Durable learnings (cross-session gotchas)

- `gen` is a RESERVED keyword in Rust edition 2024 — never use as identifier.
- A tokio `interval` fires immediately on first tick — consume once before loops.
- rg/PowerShell quoting breaks through two hops of tooling — prefer
  `include_str!` or the Write tool over piped regex extraction.
- Windows permits same-host port sharing via SO_REUSEADDR; Unix needs
  SO_REUSEPORT (tracked as D8). One moss per host by discipline meanwhile.
- Compatibility-rule operands actually used by the corpus (census 2026-08-26):
  ram.total.mb(77), architecture(29), ai.runtime(16), cpu.pattern(5),
  gpu.vram.total.mb(5), cpu.features(4), gpu(1), os.family(1).

## Open threads not already owned elsewhere

Owned elsewhere: O3 adoption/borrow (`OFFERINGS.md` §7), runtime events stream
(D10), ceremonies (D11), orchestration roles (D12), borrow vaulting (D13),
wake/start port stability (D14). The pointers below have no other home yet:

- **Audit fan-out surfacing**: EventLog writes but nothing reads outside
  validate() tests; surface recent events per offering in
  `GET /api/v1/stone/offerings/{name}` before designing feed/stream posture.
- **Graceful-goodbye witness** needs a console ctrl_c harness (Start-Process
  cannot send CTRL_C; GenerateConsoleCtrlEvent approach exists but untested).
- **M1 release pipeline** ("stranger installs from public artifact") blocks on
  a remote-push DECISION first — no remote exists yet; tag→build→sign→release
  chain comes after that call.
- **Hardware manifests (`hw/`)**: port dell wyse-5070 profile from PoC
  (identity/firmware/profile/bios sections); inverse compatibility lists;
  same grammar with `kind: hardware`.
- **Catalog enrichment**: migrated manifests dropped PoC post-install healthcheck
  log patterns — needs a `post_install:` section once §6.5 failure signatures
  land; taxonomy.dictionary.yaml (user-token → canonical mapping) should power
  `rake find`; well-known-ports.yaml remediation catalog (e.g. DNS auto-fix)
  pending.
