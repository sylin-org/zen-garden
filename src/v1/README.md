# Zen Garden v1 — greenfield

The clean-code implementation. Governed, in order: `docs/v1/lessons.md` →
`docs/v1/CHARTER.md` → `docs/v1/CODE-RULES.md`. The PoC (`src/poc/`, branch
`poc`, tag `poc-final`) is the frozen oracle; wire and on-media contracts are
forever-compatible (CODE-RULES R0.5).

## Layout

| Crate | Role |
|---|-----|
| `crates/glossary` | Domain nouns and verbs, defined once (R1.1). Zero dependencies. |
| `crates/contract` | The one wire truth: announcement envelope, chirp body, beacons, per-domain constants (B1, R1.7). |
| `crates/kernel` | Presence, single-point ingestion, handler dispatch, announcer, typed startup (R2.8, R2.9, R0.4). |
| `crates/daemon` | The stone binary: config, pipeline, HTTP surface. |

## Running

```bash
cargo run -p garden -- --stone-name proto-stone --isolate
```

`--isolate` switches discovery to the v1 experiment group/port so the
production PoC garden is unbothered (DEBT D1). Do not run without it on a LAN
that hosts PoC stones until interop is proven.

## Ledger

- `DEBT.md` — borrowed shortcuts; RC0 gates on zero-open.
- `WITNESSES.md` — live fleet proof, PoC bar first.
