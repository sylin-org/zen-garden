# Parameter Matrix — PoC rake command variations

Every flag and parameter exercised against the live fleet. Findings marked
**F** = finding (delight or defect opportunity for v1).

## observe

| Flag | Effect | Finding |
|---|---|---|
| `--offering mongodb` | Filters to only stones running mongodb | F: works as a filter; topaz-butte drops from the table when it doesn't match — the table shrinks instead of highlighting the absence |
| `--offering searxng,redis` | Comma-separated multi-filter | F: works, but comma-separated args are undiscoverable from help |
| positional `[stone]` | Should scope to one stone | **BROKEN**: "Stone 'topaz-butte' not found in topology" — the same L1 class as the manifest bugs (stone exists but the scope filter rejects valid names) |
| `-o json` | Should emit machine-readable JSON | **BROKEN**: silently ignored, output is identical human table (L7 class: the flag is documented but not implemented) |
| `--field stones[0].name` | Should extract one value | **BROKEN**: empty output — --field and -o json not wired into observe's code path |

## find

| Flag | Effect | Finding |
|---|---|---|
| `--format human` (default) | Connection strings, suggestions, related commands | Good: the "Related commands" block is self-teaching |
| `--format json` | Full FoundService objects | Good: includes offering_id (GUIDv7), tags, connection.uris[], category |
| `--format uri` | One connection URI per line, nothing else | Good: the scripting primitive — exactly what an agent pipes |
| `--format uri-ip` | IP-based URI instead of hostname | F: distinction matters in LANs with split DNS; v1 should carry both forms |
| `--field services[0].name` | Extract one field via dot notation | Good: works; confirmed live (`mongodb`) |
| `--field services[0].connection.uris[0]` | Deep path with array indexing | Good: works; confirmed live (`mongodb://192.168.1.155:27017`) |
| `--ensure` | Provision if missing | F: advertised in manifest examples and in find's own error hints, but clap rejects it as a trailing positional — the self-teaching hints teach a broken syntax |

## config

| Flag | Effect | Finding |
|---|---|---|
| `--output json` | Full config JSON | Good: includes stone identity + structured offering config |
| `--field connection.uri` | Extract one value | Good: confirmed live (`mongodb://192.168.1.155:27017`) — this is the "one line for scripts" primitive |

## commands

| Flag | Effect | Finding |
|---|---|---|
| `--category discovery` | Filter commands by category | Good: works; categories are discovery/lifecycle/management/system/pond |
| `--category data` | **REJECTED**: "Unknown category" | F: adoption/storage/companion categories exist in the manifest but the filter doesn't know them |
| `--zen` | Zen-mode help | Works; minimal aesthetic |
| `--normative` | Normative reference | F: flag is hardcoded `false` at the route level — documented but dead |

## inspect

| Flag | Effect | Finding |
|---|---|---|
| (default) | Human-readable hardware panel | Good: stone name, GUIDv7, serial, BIOS, CPU model/features |
| `--json` | Machine-readable | Available but not captured (needs SSH to every stone) |
| `--save <file>` | Write to file | Good: confirmed live; writes to operator temp dir (Windows path noted) |

## nourish

| Flag | Effect | Finding |
|---|---|---|
| `--updates-only` | Check without applying | Good: reports "6 available, 2 blocked" garden-wide with per-stone breakdown |
| `--yes` | Auto-confirm | Available (not exercised: destructive) |
| (interactive) | Menu: [A]ll / [O]fferings / [F]irmware | Good: the three-way split is the J3 delight — calm, scoped, never surprises |

## watch

| Syntax | Effect | Finding |
|---|---|---|
| `watch` | General event stream | Good: raw JSON with full telemetry (CPU, MEM, DSK, GPU, net, offerings, health) |
| `watch offering mongodb` | **BROKEN**: "Usage: garden-rake watch offering \<name\> logs" — the advertised `logs` suffix is a required third positional, not a mode | F: the hint teaches a syntax that doesn't work |
| `watch stone <name>` | Requires `\<name\>` positional | F: help says `watch stone logs` but `logs` is parsed as `\<name\>`, then fails with "required arguments not provided" — the advertised example is self-contradictory |

## nourish (detailed)

| Flag | Effect | Finding |
|---|---|---|
| `--updates-only` | **Confirmed live**: "6 available, 2 blocked" garden-wide | The canary-ring concept (J3) needs this read-only check as its first half |
| `--yes` | Auto-confirm (not exercised: would apply updates) | Available |

## capabilities

| Syntax | Effect | Finding |
|---|---|---|
| `capabilities list` | **BROKEN**: OFFERING_NOT_FOUND — `list` is parsed as an offering name | F: `capabilities` requires a running offering; the list/discover mode isn't wired |

## The three-degree machine output (confirmed live)

1. `--json` / `-o json` — raw machine output
2. `--field dot.path` — one value extraction (dot notation + array indexing)
3. `--format uri` — the connection-string promise as a scripting primitive

All three confirmed working on `find` and `config`. Only `observe` and
`status` lack them — and those are the two where an agent needs them most.

## The parameter-matrix gap list for v1

Every finding above collapses to one design rule: **every flag documented
in help must work; every parameter must be exercised in CI.** The PoC's
manifest-vs-reality drift is the L7 lesson applied to CLI surfaces — v1's
Face/table discipline extends naturally to parameter matrices.
