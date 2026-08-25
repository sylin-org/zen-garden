# Zen Garden v1 — greenfield

The clean-code implementation. Governed, in order: `docs/v1/lessons.md` →
`docs/v1/CHARTER.md` → `docs/v1/CODE-RULES.md`. The PoC (`src/poc/`, branch
`poc`, tag `poc-final`) is the frozen oracle: on-media contracts are
forever-compatible (CODE-RULES R0.5); the network is v1's own design.

## Topology

v1 owns a declared room, separate from the PoC's (charter amendment
2026-08-25): the PoC proved the mechanisms; v1 chooses deliberately.

| Assignment | Value | Owner |
|---|---|---|
| Discovery multicast | UDP 7284, group 239.255.42.199 | `contract::consts` |
| Stone HTTP surface | TCP 7285 | `kernel::config::HttpConfig` |
| Reserved block | 7286–7299 | claim in `contract::consts` before first bind |

The PoC fleet keeps its room (7184 / 239.255.42.99) untouched; constants for
it exist as legacy reference only and are never defaults.

## Assets

One-word names for things you invoke; `garden-*` namespaces for libraries
you link (L16: delight is load-bearing; L20: names are decisions too).

| Asset | Kind | Status |
|---|---|---|
| **moss** | The stone's resident service (this daemon) | live |
| **rake** | The gardener's CLI — humans *and* agents walk the garden with it | reserved, next |
| pond | A storage cluster | inherited name, M3+ |
| firefly, cricket | Companions | inherited names, M5 |
| lantern | PoC registry endpoint discovery | retired unless missed |

## Layout

| Crate | Role |
|---|-----|
| `crates/glossary` | Domain nouns and verbs, defined once (R1.1). Zero dependencies. |
| `crates/contract` | The one wire truth: announcement envelope, chirp body, discovery ask/tell, per-domain constants (B1, R1.7). |
| `crates/kernel` | Presence, single-point ingestion, handler dispatch, responder, announcer, typed startup (R2.8, R2.9, R0.4). |
| `crates/moss` | The `moss` binary: config, pipeline, HTTP surface. |

## Running

```bash
cargo run -p garden-moss -- --stone-name proto-stone
```

Defaults are the v1 room; nothing to isolate from. Overrides
(`--discovery-port`, `--mcast-group`, env twins `MOSS_*`) exist for
experiments. All deployment config lives on moss's CLI; the kernel ships
pure defaults only.

## Ledger

- `DEBT.md` — borrowed shortcuts; RC0 gates on zero-open.
- `WITNESSES.md` — live fleet proof, PoC bar first.
