# Unified Interface Language

**Date**: 2026-03-17
**Status**: Proposal
**Supersedes**: [rake-dual-ergonomics.md](rake-dual-ergonomics.md)
**Migration**: Hard cut. No deprecation shims, no backwards compatibility, no leftover code.
Remove the dual syntax parser entirely. Push to test garden for validation.

---

## Design Thesis

Zen Garden communicates through two surfaces: a CLI (rake) and an HTTP API (moss).
Today they speak overlapping but inconsistent dialects — the CLI has dual grammars
(zen/normative), the API mixes domain verbs with REST conventions, and some concepts
have different names depending on which surface you're using.

This proposal unifies the language. One vocabulary. One grammar for the CLI, one
RESTful convention for the API. Both surfaces share the same nouns and — where
appropriate — the same verbs.

---

## Part 1 — Shared Language

### Domain Nouns

These nouns name the domain model. They appear in both CLI output and API paths.

| Noun | Meaning | Notes |
|------|---------|-------|
| **garden** | Cluster of stones | Cluster-wide scope in API (`/garden/`) |
| **stone** | Individual machine | Local scope in API (`/stone/`) |
| **offering** | Software available to install | Catalog item — becomes a service once running |
| **service** | Running instance of an offering | Container on a specific stone |
| **pond** | Trust network between stones | mTLS-based security domain |
| **companion** | Extension process (audio, LED) | Sidecar binaries managed by moss |
| **storage** / **bank** | Managed storage device or directory | "Bank" is a specific mounted unit |
| **seed bank** | Storage with the seed-bank role | Backup destination, cross-stone replication |
| **snapshot** | Point-in-time backup of service data | Was: "harvest" / "nurturing" |
| **update** | New version of software or firmware | Was: "nourishment" |

### Retired Terms

| Old Term | Replacement | Reason |
|----------|-------------|--------|
| nourish | **upgrade** | "Upgrade" is universally understood |
| nourishment | **update** | Standard term for available new versions |
| nurturing | **snapshot** | Describes the artifact, not the metaphor |
| harvest | **snapshot** | Same — "harvest" was a second metaphor for the same thing |
| touch | *(dropped)* | Unix collision (`touch` = create/update file timestamp) |
| place / lift | *(dropped)* | Aliases for `pond init` / `pond remove` — one name per operation |
| rouse / slumber / stir | `stone wake` / `stone shutdown` / `stone reboot` | Grouped under `stone`; standard verbs are clearer |

### Kept Domain Verbs

These domain verbs earn their place — they're clearer or more memorable than the generic alternative.

| Verb | Meaning | Why it stays |
|------|---------|-------------|
| **offer** | Browse/install from catalog | Encapsulates browse + install intent; distinctive |
| **rest** | Stop a service (reversible) | Natural pair with `wake`; communicates reversibility |
| **wake** | Start a resting service | Natural pair with `rest` |
| **uproot** | Destroy a service permanently | Carries the weight of irreversibility better than "destroy" |
| **adopt** | Claim an unmanaged container | Precise domain verb — no standard CLI equivalent |
| **release** | Stop managing an adopted service | Natural pair with `adopt` |
| **borrow** | Register an external service | Precise — communicates that the service lives elsewhere |
| **return** | Unregister a borrowed service | Natural pair with `borrow` |
| **tend** | Set the target stone context | Short, memorable, domain-native |
| **observe** | Garden-wide state snapshot | Richer than "list" — implies a holistic view |
| **pulse** | Live vitals dashboard | Evocative and unambiguous |
| **reconcile** | Force registry ↔ reality sync | Standard infra term, no better alternative |
| **explore** | Alias for `offer` in browse mode | Optional synonym — zero cost in Clap |

---

## Part 2 — CLI Reference (garden-rake)

### Grammar

Every command follows one pattern:

```
garden-rake <verb> [noun] [--flags]
```

No positional keywords. No alternate syntax. No style detection. The grammar a
beginner learns on day one is the same grammar used in scripts.

### Global Flags

| Flag | Short | Type | Default | Description |
|------|-------|------|---------|-------------|
| `--at <stone>` | | string | tended | Target a specific stone |
| `--json` | | flag | | JSON output; implies non-interactive (currently `--output json`) |
| `--quiet` | `-q` | flag | | Suppress hints and suggestions |
| `--verbose` | `-v` | count | 0 | Increase detail (`-v`, `-vv`, `-vvv`) |
| `--field <path>` | | string | | Dot-notation JSON extraction (implies `--json`) |
| `--yes` | `-y` | flag | | Skip all confirmation prompts |
| `--fresh` | | flag | | Bypass cache, fetch live data |

**`--yes` unifies** the currently per-command flags: `--force` on `remove`/`uproot`,
`--auto-confirm` on `nourish`, `--yes`/`-y` on `storage add`.

**Behavioral rules**:
- `--json` forces non-interactive mode. Missing required values produce errors, not prompts.
- `--field` implies `--json` and outputs only the extracted value (no wrapping).

### Interactive Principle

When a required value is missing and there's a genuinely ambiguous choice, the CLI
prompts. When the value is provided (via flag or tending context), it executes directly.

This applies **only where real ambiguity exists** — not as a general-purpose wrapper
around every command. Most commands already have clear defaults: `offer mongodb`
installs on the tended stone, `rest mongodb` stops it. No prompt needed.

```
$ garden-rake remove mongodb
  ⚠️  This will remove service 'mongodb' and stop its container.
  Volumes will be preserved. Continue? [y/N]: _

$ garden-rake remove mongodb --yes
  Removing mongodb...
```

Where prompts do exist, each question maps to exactly one flag (`--yes` skips
the confirmation above). Over time, users learn the flags by seeing which
questions they skip.

---

### Commands — Quick Reference (Proposed State)

```
CONTEXT       tend

DISCOVER      observe · list · find · pulse · watch · logs · config

LIFECYCLE     offer (explore) · status · rest · wake · upgrade · remove · uproot

ADOPTION      adopt · release · borrow · return

SECURITY      pond    init · join · invite · enroll · trust · status
                      unlock · remove/drain · untrust · promote · rename

STORAGE       storage add · list · status · release · pin · unpin
              store   ls · get · put · rm · head

ADMIN         stone   wake · shutdown · reboot · verbosity · install
                      reconcile · refresh

SNAPSHOTS     backup  status · list · trigger · trigger-all · restore

AUTHORING     manifest  init · validate · test · export · enrich
              cap (capabilities) <offering> add · remove · refresh · mirror

COMPANION     hey <companion> <args>

META          api · launch · commands · ceremony · template · election
```

### Migration Table (Current → Proposed)

| Current Command | Proposed | Change |
|----------------|----------|--------|
| `--wishful` | `--ensure` | Rename — describes action, not feeling |
| `--force` (remove/uproot) | `--yes` | Unify confirmation skip under global `--yes` |
| `--auto-confirm` (nourish) | `--yes` | Same |
| `--output json` | `--json` | Shorthand |
| `nourish` | `upgrade --garden` | Absorb into `upgrade` with scope flag |
| `adopted` | `list --adopted` | Consolidate into `list` filter |
| `borrowed` | `list --borrowed` | Consolidate into `list` filter |
| `locate strays` | `list --strays` | Consolidate into `list` filter |
| `presence` | `watch --events presence` | Consolidate into `watch` filter |
| `rouse <stone>` | `stone wake <stone>` | Group under `stone` |
| `slumber [<stone>]` | `stone shutdown [<stone>]` | Group under `stone` |
| `stir [<stone>]` | `stone reboot [<stone>]` | Group under `stone` |
| `make sing\|quiet\|silent` | `stone verbosity <level>` | Group under `stone` |
| `take-root` | `stone install` | Group under `stone` |
| `reconcile` | `stone reconcile` | Group under `stone` |
| `refresh <comp> --from` | `stone refresh <comp> --from` | Group under `stone` |
| `nurturing status\|list\|trigger` | `backup status\|list\|trigger` | Rename group |
| `restore <offering>` | `backup restore <offering>` | Move under `backup` group |
| `place keystone` | `pond init` | Drop alias — one name per operation |
| `place stone` | `pond join` | Drop alias |
| `lift stone` | `pond untrust` | Drop alias |
| `lift keystone` | `pond drain` | Drop alias |
| `invite` (top-level) | `pond invite` | Already under `pond`; drop top-level |
| `touch` | *(dropped)* | Unix collision; use `status` |
| `install-service` | *(dropped)* | Internal alias for `stone install` |
| `somewhere` (keyword) | `--placement-mode` flag | Zen keyword → standard flag |
| `wishfully` (keyword) | `--ensure` flag | Zen keyword → standard flag |
| `quietly` (keyword) | `--quiet` flag | Already exists |
| `on <stone>` (keyword) | `--at` flag | Already exists |
| `from <url>` (keyword) | `--from` flag | Already exists |
| `fresh` (keyword) | `--fresh` flag | Already exists |
| Zen/normative style detection | *(removed)* | One grammar; Clap only |
| Normative aliases (`services find`, etc.) | *(removed)* | Domain verbs are the canonical names |

---

### Commands — Full Reference

#### Context

##### `tend`

Set which stone rake communicates with. Persists for 90 seconds or until changed.

```
tend [<target>] [--clear]
```

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `target` | positional | no | Stone name, URL, `local`, or `auto` |
| `--clear` | flag | no | Reset to auto-discovery |

```
garden-rake tend stone-crystal-forest
garden-rake tend local
garden-rake tend --clear
```

---

#### Discovery & Monitoring

##### `observe`

Garden-wide snapshot. All stones, offerings, topology, cluster state.

```
observe [<stone>] [--offering <names>]
```

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `stone` | positional | no | Filter to one stone |
| `--offering` | option | no | Filter to specific offerings (comma-separated) |

```
garden-rake observe
garden-rake observe stone-crystal-forest
garden-rake observe --offering mongodb,ollama
```

##### `list`

List services on a stone. Compact tabular output.

```
list
```

Today, `adopted`, `borrowed`, and `locate strays` are separate commands.
**Proposed**: consolidate as filter flags on `list`:

| Flag | Currently | Proposed |
|------|-----------|----------|
| `--adopted` | `garden-rake adopted` | `garden-rake list --adopted` |
| `--borrowed` | `garden-rake borrowed` | `garden-rake list --borrowed` |
| `--strays` | `garden-rake locate strays` | `garden-rake list --strays` |

```
garden-rake list
garden-rake list --at stone-crystal-forest
garden-rake list --json --field "services[0].name"
```

##### `find`

Search for services by name, category, or tag across the entire garden.

```
find <query> [--ensure]
```

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `query` | positional | yes | Name, `c:category`, or `t:tag` |
| `--ensure` | flag | no | Provision if missing, then search again |

Currently `--wishful`. **Proposed rename**: `--ensure` — describes the action
("ensure this service exists") rather than the feeling.

Without `--ensure`, a miss shows a hint:

```
$ garden-rake find mongodb
  mongodb is not running on any stone.
  Try: garden-rake find mongodb --ensure
```

With `--ensure`, silently provisions on best-fit stone and retries the search.

```
garden-rake find mongodb
garden-rake find c:database
garden-rake find t:gpu --at stone-crystal-forest
garden-rake find mongodb --ensure
```

##### `pulse`

Live terminal dashboard for stone vitals (CPU, memory, disk, GPU, network). Ctrl+C to exit.

```
pulse
```

```
garden-rake pulse
garden-rake pulse --at stone-crystal-forest
```

##### `watch`

Stream real-time events or service logs.

```
watch [--until <condition>]
watch offering <name> logs [--timestamps]
watch stone <name> logs [--timestamps]
```

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `--until` | option | no | Stop condition for event watching |
| `--timestamps` | flag | no | Include timestamps in log output |

Subcommands select what to stream. Bare `watch` streams stone events.

```
garden-rake watch                                # stone events
garden-rake watch offering mongodb logs          # mongodb container logs
garden-rake watch offering mongodb logs --timestamps
garden-rake watch --at stone-01 --until ready
```

> `presence` is currently a separate command. **Proposed**: consolidate as `watch --events presence`.

##### `logs`

**Proposed alias** for `watch offering <name> logs`. The most common streaming
use case deserves a short path.

```
logs <service> [--timestamps]
```

```
garden-rake logs mongodb                         # same as: watch offering mongodb logs
garden-rake logs mongodb --timestamps
garden-rake logs mongodb --at stone-01
```

---

#### Service Lifecycle

##### `offer`

Browse the offering catalog or install an offering.

```
offer [<name>] [--prefer <hardware>] [--anywhere-on-fail]
offer info <name>
offer image <image-ref>
```

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `name` | positional | no | Offering to install (omit to browse) |
| `--prefer` | option | no | Hardware preference (repeatable: `--prefer gpu --prefer ssd`) |
| `--anywhere-on-fail` | flag | no | Fall back to any stone on failure |
| `--placement-mode` | option | no | `interactive` (ranked stone picker) or `auto` (pick best). Currently: zen keyword `somewhere` |

Installs on the tended stone. Use `--at` to target a different one.

Alias: `explore` (synonym for `offer` with no arguments — browse mode).

```
garden-rake offer                            # browse catalog
garden-rake explore                          # same
garden-rake offer mongodb                    # install on tended stone
garden-rake offer mongodb --at stone-01      # install on specific stone
garden-rake offer info mongodb               # offering details
garden-rake offer image nginx:latest         # deploy Docker image directly
garden-rake offer mongodb --prefer gpu,ssd
```

##### `status`

Context-sensitive detail view with contextually recommended commands.

```
status [<service>]
```

| Invocation | Scope | Content |
|------------|-------|---------|
| `garden-rake` (bare) | Tended stone | Quick identity + command directory |
| `garden-rake status` | Tended stone | Full stone detail (hardware, health, services) + suggested commands |
| `garden-rake status <service>` | Service | Full service detail (health, ports, resources, events) + suggested commands |

The current handler shows stone-level info only. **Proposed**: extend to
accept an optional `<service>` arg for service-level drill-down.

```
garden-rake status
garden-rake status mongodb
garden-rake status mongodb --at stone-crystal-forest
garden-rake status --json
garden-rake status mongodb --field "connection.uris[0]"
```

##### `rest`

Stop a service. It enters rest mode and can be woken later with `wake`.

```
rest <service>
```

```
garden-rake rest mongodb
garden-rake rest mongodb --at stone-crystal-forest
```

##### `wake`

Start a service from rest mode.

```
wake <service>
```

```
garden-rake wake mongodb
garden-rake wake mongodb --at stone-crystal-forest
```

##### `upgrade`

Upgrade services to their latest version. Two scopes exist today:

- `upgrade [<service>] [--all]` — single stone, non-interactive.
- `nourish` — garden-wide aggregator with interactive `[A/O/F/Q]` menu.

**Proposed**: unify under `upgrade` with `--garden` flag for cross-stone scope.

```
upgrade [<service>] [--all]
```

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `service` | positional | no | Specific service to upgrade |
| `--all` | flag | no | Upgrade all services on the stone |

Currently `nourish` (garden-wide) prompts when not auto-confirmed:

```
$ garden-rake nourish
  Apply updates:
    [A] All updates
    [O] Offerings only
    [F] Firmware only
    [ESC/Q] Cancel
  Choice: _

$ garden-rake nourish --auto-confirm
  Applying all updates...
```

```
garden-rake upgrade mongodb
garden-rake upgrade --all
garden-rake upgrade --at stone-crystal-forest
```

##### `remove`

Soft-remove a service. Container is preserved as a stray and can be re-adopted.

```
remove <service>
```

Prompts for confirmation. `--yes` skips (currently `--force`).

```
garden-rake remove mongodb
garden-rake remove mongodb --yes
```

##### `uproot`

Permanently destroy a service, its container, and all volumes. **Irreversible.**

```
uproot <service>
```

Prompts for confirmation. `--yes` skips (currently `--force`).

```
$ garden-rake uproot mongodb
  ⚠️  WARNING: This will PERMANENTLY DESTROY service 'mongodb' and its container.
  This action cannot be undone. Continue? [y/N]: _
```

```
garden-rake uproot mongodb
garden-rake uproot mongodb --yes
```

---

#### Adoption

##### `adopt`

Adopt an unmanaged (stray) container into zen-garden management.

```
adopt <container>
```

Requires the explicit container name. Use `list --strays` (proposed) or
`locate strays` (current) to discover adoptable containers first.

```
garden-rake adopt my-mongo
garden-rake adopt my-mongo --at stone-crystal-forest
```

##### `release`

Release an adopted service from management. Container keeps running unmanaged.

```
release <service>
```

```
garden-rake release mongodb
```

##### `borrow`

Register an external (non-containerized) service for garden-wide discovery.

```
borrow <name> --from <url>
```

```
garden-rake borrow my-nas --from http://192.168.1.50:8080
```

##### `return`

Unregister a borrowed service.

```
return <name>
```

```
garden-rake return my-nas
```

---

#### Pond (Security & Trust)

Multi-stone trust network. All subcommands under `pond`.

```
pond init [--passphrase <pass>] [--profile <name>]
pond status
pond join <code>
pond invite [--passphrase <pass>]
pond enroll
pond trust
pond unlock [--passphrase <pass>] [--totp <code>]
pond remove | pond drain
pond untrust <stone>
pond promote [--passphrase <pass>]
pond rename [<name>]
```

Pond operations use a server-driven ceremony flow: moss sends prompt types
(text, secret, code, selection) and rake renders them interactively via the
ceremony render loop. Flags like `--passphrase` short-circuit specific ceremony
steps for non-interactive use.

```
garden-rake pond init --passphrase "secret"
garden-rake pond status
garden-rake pond invite
garden-rake pond join ABC123
garden-rake pond remove stone-mossy-brook
garden-rake pond drain --yes
garden-rake pond promote stone-mossy-brook
garden-rake pond rename "home lab"
```

---

#### Storage

Block device and directory management. Subcommands under `storage`.

```
storage add <target> [--as <name>] [--role <role>] [--format] [--fs <type>] [--encrypted]
storage list
storage status
storage release <name>
storage pin <name>
storage unpin <name>
storage rename <name> <new-name>
storage visibility <name> <level>
storage roles <name> <roles>
```

When target is omitted, `storage add` fetches candidates from the stone and
presents an interactive device picker (if multiple eligible devices exist).
Name, role, and format are specified via flags — not a wizard flow.

```
$ garden-rake storage add
  Multiple devices found:
    [1] /dev/sdb  (2.0 TB)  Samsung 870 EVO
    [2] /dev/sdc  (500 GB)  WD Blue
  Select: _

$ garden-rake storage add /dev/sdb --as media --role seed-bank --format --yes
  Adding /dev/sdb as 'media'...
```

```
garden-rake storage add /dev/sdb --as media --role seed-bank --format --yes
garden-rake storage list
garden-rake storage status
garden-rake storage pin media
garden-rake storage release media
```

##### `store`

S3-compatible object storage operations on seed banks.

```
store ls <bucket> [--prefix <prefix>] [--delimiter <char>]
store get <bucket> <key>
store put <bucket> <key> <file>
store rm <bucket> <key>
store head <bucket> <key>
```

```
garden-rake store ls my-bucket
garden-rake store put my-bucket photos/cat.jpg ./cat.jpg
garden-rake store get my-bucket photos/cat.jpg
```

---

#### Stone Administration

Hardware-level operations. **Proposed**: group under `stone` subcommand.
Currently these are top-level commands (`rouse`, `slumber`, `stir`, `reconcile`,
`take-root`, `make`, `refresh`).

```
stone wake <name>                             # Wake-on-LAN
stone shutdown [<name>]                       # Power off
stone reboot [<name>]                         # Reboot
stone verbosity <level> [--forever]           # Console: sing, quiet, silent, minimal
stone install [--yes] [--dry-run]             # Install moss as system service
stone reconcile [--drop-invalid]              # Force registry sync
stone refresh <component> --from <path>       # Update binary (dev)
```

```
garden-rake stone wake stone-mossy-brook
garden-rake stone shutdown
garden-rake stone reboot --at stone-quiet-pond
garden-rake stone verbosity sing
garden-rake stone verbosity quiet
garden-rake stone install --yes
garden-rake stone reconcile
```

---

#### Snapshots & Backup

**Proposed**: rename to `backup`. Currently `nurturing` is the command name,
and `restore` is a separate top-level command.

```
backup status [<offering>]
backup list <offering> [--local] [--remote]
backup trigger <offering>
backup trigger-all
backup restore <offering> [<source>] [--dry-run] [--snapshot-id <id>]
```

| Arg | Type | Description |
|-----|------|-------------|
| `source` | positional | `from slot A\|B` or `from seed-bank <name>` |
| `--dry-run` | flag | Show what would be restored |

```
garden-rake backup status
garden-rake backup status mongodb
garden-rake backup list mongodb --remote
garden-rake backup trigger mongodb
garden-rake backup trigger-all
garden-rake backup restore mongodb
garden-rake backup restore mongodb --from seed-bank media --dry-run
```

---

#### Capabilities

Manage sub-capabilities of offerings (models, databases, plugins).
Alias: `cap` (14 characters is a lot for a frequently-typed command).

```
capabilities <offering> add <name> [--type <type>] [--dry-run]
capabilities <offering> remove <name> [--type <type>]
capabilities <offering> refresh [--type <type>] [--dry-run]
capabilities <offering> mirror --from <stone> --to <stone>
```

```
garden-rake capabilities ollama add llama3.2
garden-rake capabilities ollama remove llama3.2
garden-rake capabilities ollama refresh
garden-rake capabilities ollama mirror --from stone-01 --to stone-02
```

---

#### Manifests

Offering manifest authoring. Subcommands under `manifest`.

```
manifest init <image-ref> [--output <dir>] [--name <name>] [--category <cat>]
manifest validate [<path>]
manifest test [<path>]
manifest export <offering> [--output <dir>]
manifest enrich [<path>] [--auto]
```

```
garden-rake manifest init nginx:latest --name my-nginx
garden-rake manifest validate ./my-manifest.yaml
garden-rake manifest test ./my-manifest.yaml --at stone-crystal-forest
garden-rake manifest export mongodb
```

---

#### Meta

##### `api`

Display the live Moss HTTP API reference.

```
api [<endpoint>] [--category <name>] [--examples]
```

```
garden-rake api
garden-rake api /stone/offerings
garden-rake api --category storage --examples
```

##### `launch`

Open the stone portrait (web dashboard) in a browser.

```
launch
```

```
garden-rake launch
garden-rake launch --at stone-crystal-forest
```

##### `commands`

Browse the command directory.

```
commands [<name>] [--category <name>]
```

```
garden-rake commands
garden-rake commands find
garden-rake commands --category lifecycle
```

##### `config`

Show detailed service configuration for automation. Supports JSON output
and dot-notation field extraction.

```
config <service> [--field <path>]
```

```
garden-rake config mongodb
garden-rake config mongodb --json
garden-rake config mongodb --field "connection.port"
```

##### `hey`

Forward a command to a companion (Cricket, Firefly, etc.).

```
hey <companion> <args...>
```

```
garden-rake hey cricket play notification
garden-rake hey firefly pulse blue
```

##### `ceremony`

Run a guided workflow. Scaffolded — currently used internally by
pond operations.

```
ceremony [<workflow>]
```

##### `template`

Manage offering templates. Scaffolded.

```
template list
template show <name>
```

##### `election`

Trigger a distributed election (diagnostic/testing).

```
election start [--election-type <type>] [--timeout <seconds>]
```

```
garden-rake election start
garden-rake election start --election-type backup_source --timeout 20
```

---

### Scripting & Automation

```bash
# JSON output (currently: --output json)
garden-rake list --json

# Extract specific field
garden-rake find mongodb --field "services[0].connection.uris[0]"

# Pipe to jq
garden-rake list --json | jq '.services[] | select(.status == "running")'

# Non-interactive (all flags explicit, currently: --at, --force)
garden-rake offer mongodb --at stone-01
garden-rake remove mongodb --force

# JSON mode is always non-interactive
garden-rake upgrade --all --json
```

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Missing required value (non-interactive mode) |
| 3 | Resource not found |
| 4 | Confirmation declined |

---

## Part 3 — API Reference (garden-moss)

### Design Principles

1. **Nouns are resources, verbs are HTTP methods.** `GET /services` lists, `DELETE /services/{s}` removes.
   State transitions that don't map to CRUD use `POST /resource/action`.

2. **Same nouns as the CLI.** If the CLI says "offering", the API says `/offerings`.
   If the CLI says "snapshot", the API says `/snapshots`.

3. **Two scopes: `/stone` (local) and `/garden` (cluster-wide).** The scope is always
   the first path segment after `/api/v1/`. This is the API's best structural decision.

4. **SSE streams end in `/stream`.** Every streaming endpoint follows this convention.

5. **Response envelope.** All endpoints return `ApiResult<T>` — success produces the
   resource directly, failure produces `{ error: string, code: string }`.

### Vocabulary Alignment (Proposed Renames)

| Current API Path | Proposed Path | Reason |
|-----------------|---------------|--------|
| `POST /services/{s}/nourish` | `POST /services/{s}/upgrade` | Align with CLI verb |
| `GET /stone/nourishment` | `GET /stone/updates` | Standard term |
| `POST /stone/nourishment/execute` | `POST /stone/updates/execute` | Consistent |
| `GET /stone/nourishment/stream/{id}` | `GET /stone/updates/stream/{id}` | Consistent |
| `GET /stone/nurturing` | `GET /stone/snapshots` | Describes the artifact |
| `GET /stone/nurturing/{offering}` | `GET /stone/snapshots/{offering}` | Consistent |
| `POST /stone/nurturing/{offering}` | `POST /stone/snapshots/{offering}` | Consistent |
| `POST /stone/nurturing/{o}/restore` | `POST /stone/snapshots/{o}/restore` | Consistent |
| `POST /stone/nurturing/{o}/replicate` | `POST /stone/snapshots/{o}/replicate` | Consistent |
| `GET /stone/nurturing/remote/{bank}` | `GET /stone/snapshots/remote/{bank}` | Consistent |
| `POST /nurturing/{o}/trigger` | `POST /stone/snapshots/{o}/trigger` | Move under `/stone/` |
| `POST /nurturing/trigger-all` | `POST /stone/snapshots/trigger-all` | Move under `/stone/` |
| `GET /garden/nourishment` | `GET /garden/updates` | Consistent |
| `POST /garden/nourishment/execute` | `POST /garden/updates/execute` | Consistent |
| `POST /stone:upgrade` | `POST /stone/upgrade` | Drop colon syntax |
| `POST /stone:deploy` | `POST /stone/deploy` | Drop colon syntax |

### Structural Notes

**Keep as-is (good patterns):**
- `/stone/services/{s}/rest` and `/wake` — domain actions for state transitions; clear in context
- `/stone/offerings/{o}/adopt` — domain action; no REST equivalent
- `/stone/storage/banks/{name}/pin` and `/unpin` — domain actions
- `/stone/companions/{id}/command` — RPC-style is appropriate here
- `/pond/*` — security operations are inherently RPC-style
- `/admin/*` — administrative commands are RPC-style

**Consolidate:**
- `/api/v1/storage/s3/*` (legacy top-level) should nest under `/stone/storage/s3/*`
- Handler `_v1` suffixes are unnecessary — the version is in the URL path

---

### Endpoints — Stone (Local)

#### Info & Monitoring

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/stone` | Stone overview |
| GET | `/api/v1/stone/info` | Detailed stone info |
| GET | `/api/v1/stone/capabilities` | Hardware capabilities (GPU, CPU, RAM) |
| GET | `/api/v1/stone/metrics` | Prometheus-format metrics snapshot |
| GET | `/api/v1/stone/portrait` | Portrait dashboard data |
| GET | `/api/v1/stone/pulse/stream` | **SSE** — live vitals stream |
| GET | `/api/v1/stone/presence/stream` | **SSE** — presence event stream |
| POST | `/api/v1/stone/presence/notify` | Push presence notification |
| GET | `/api/v1/stone/logs` | Recent daemon log lines (`?lines=N&level=L`) |
| GET | `/api/v1/stone/logs/stream` | **SSE** — live daemon log stream |

#### Administration

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/v1/stone/upgrade` | Self-upgrade moss binary |
| POST | `/api/v1/stone/deploy` | Deploy new moss binary |
| GET | `/api/v1/stone/maintenance/history` | Sweep history |
| POST | `/api/v1/stone/maintenance/sweep` | Trigger maintenance sweep |

#### Services

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/stone/services` | List all services |
| POST | `/api/v1/stone/services` | Create a service |
| GET | `/api/v1/stone/services/{service}` | Service details |
| DELETE | `/api/v1/stone/services/{service}` | Remove service (soft — becomes stray) |
| POST | `/api/v1/stone/services/{service}/rest` | Stop service (rest mode) |
| POST | `/api/v1/stone/services/{service}/wake` | Start service from rest |
| POST | `/api/v1/stone/services/{service}/restart` | Restart service |
| POST | `/api/v1/stone/services/{service}/upgrade` | Upgrade to latest version |
| POST | `/api/v1/stone/services/{service}/destroy` | **Irreversible.** Destroy container + volumes |
| POST | `/api/v1/stone/services/{service}/reassign` | Reassign to different stone |
| POST | `/api/v1/stone/services/{service}/cordon` | Mark service as cordoned |
| GET | `/api/v1/stone/services/{service}/logs` | **SSE** — container log stream |
| GET | `/api/v1/stone/services/{service}/env` | Read environment variables |
| PATCH | `/api/v1/stone/services/{service}/env` | Set/delete environment variables |
| GET | `/api/v1/stone/services/{service}/config` | Read config overlay |
| PATCH | `/api/v1/stone/services/{service}/config` | Update config overlay |
| DELETE | `/api/v1/stone/services/{service}/config` | Reset config to defaults |
| GET | `/api/v1/stone/services/{service}/manifest` | Service manifest |
| GET | `/api/v1/stone/services/{service}/capabilities` | Sub-capabilities |
| POST | `/api/v1/stone/services/reconcile` | Force registry ↔ container sync |
| POST | `/api/v1/stone/services/refresh` | Reload manifests from disk |
| POST | `/api/v1/stone/services/refresh-capabilities` | Refresh all service capabilities |
| GET | `/api/v1/stone/services/manifests` | List available manifests |

#### Offerings (Catalog)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/stone/offerings` | List all offerings |
| GET | `/api/v1/stone/offerings/search` | Search with taxonomy (`?q=`) |
| GET | `/api/v1/stone/offerings/inspect` | Inspect Docker image (`?image=`) |
| POST | `/api/v1/stone/offerings/heal` | Adopt orphaned `zen-offering-*` containers |
| POST | `/api/v1/stone/offerings/refresh` | Refresh offering catalog |
| GET | `/api/v1/stone/offerings/{name}` | Offering details |
| DELETE | `/api/v1/stone/offerings/{name}` | Remove offering |
| GET | `/api/v1/stone/offerings/{name}/manifest` | Offering manifest (YAML) |
| GET | `/api/v1/stone/offerings/{name}/export` | Export manifest envelope |
| GET | `/api/v1/stone/offerings/adoptable` | List stray containers |
| GET | `/api/v1/stone/offerings/adopted` | List adopted services |
| GET | `/api/v1/stone/offerings/borrowed` | List borrowed services |
| POST | `/api/v1/stone/offerings/{offering}/adopt` | Adopt a stray |
| DELETE | `/api/v1/stone/offerings/{offering}/adopt` | Release (un-adopt) |
| POST | `/api/v1/stone/offerings/borrow` | Register external service |
| DELETE | `/api/v1/stone/offerings/borrow/{name}` | Unregister borrowed service |

##### Capabilities (Sub-offerings)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/stone/offerings/{name}/capabilities` | List capabilities |
| POST | `/api/v1/stone/offerings/{name}/capabilities` | Add capability |
| POST | `/api/v1/stone/offerings/{name}/capabilities/refresh` | Refresh capabilities |
| POST | `/api/v1/stone/offerings/{name}/capabilities/mirror` | Mirror from another stone |
| DELETE | `/api/v1/stone/offerings/{name}/capabilities/{cap}` | Remove capability |

#### Storage

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/stone/storage` | Storage overview |
| GET | `/api/v1/stone/storage/health` | Storage health |
| GET | `/api/v1/stone/storage/candidates` | Eligible devices |
| POST | `/api/v1/stone/storage/add` | Add storage device or directory |
| POST | `/api/v1/stone/storage/release-all` | Release all banks |
| GET | `/api/v1/stone/storage/banks` | List banks |
| GET | `/api/v1/stone/storage/banks/{name}` | Bank details |
| DELETE | `/api/v1/stone/storage/banks/{name}` | Remove bank |
| PATCH | `/api/v1/stone/storage/banks/{name}/visibility` | Set visibility |
| PATCH | `/api/v1/stone/storage/banks/{name}/rename` | Rename bank |
| PATCH | `/api/v1/stone/storage/banks/{name}/roles` | Set roles |
| POST | `/api/v1/stone/storage/banks/{name}/release` | Unmount bank |
| POST | `/api/v1/stone/storage/banks/{name}/pin` | Claim as primary |
| POST | `/api/v1/stone/storage/banks/{name}/unpin` | Release primary |
| GET | `/api/v1/stone/storage/banks/{name}/changes` | Replication changelog |
| GET | `/api/v1/stone/storage/stream` | **SSE** — storage event stream |

##### S3 Gateway

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/stone/storage/s3` | List buckets |
| GET | `/api/v1/stone/storage/s3/{bucket}` | List objects |
| GET | `/api/v1/stone/storage/s3/{bucket}/{*key}` | Get object |
| PUT | `/api/v1/stone/storage/s3/{bucket}/{*key}` | Put object |
| HEAD | `/api/v1/stone/storage/s3/{bucket}/{*key}` | Object metadata |
| DELETE | `/api/v1/stone/storage/s3/{bucket}/{*key}` | Delete object |

#### Snapshots (was: Nurturing)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/stone/snapshots` | List all snapshots |
| GET | `/api/v1/stone/snapshots/{offering}` | List slots for offering |
| POST | `/api/v1/stone/snapshots/{offering}` | Create snapshot |
| DELETE | `/api/v1/stone/snapshots/{offering}` | Delete all snapshots |
| POST | `/api/v1/stone/snapshots/{offering}/trigger` | Trigger scheduled snapshot |
| POST | `/api/v1/stone/snapshots/{offering}/restore` | Restore from local snapshot |
| POST | `/api/v1/stone/snapshots/{offering}/replicate` | Replicate to seed bank |
| POST | `/api/v1/stone/snapshots/{offering}/restore-remote` | Restore from seed bank |
| GET | `/api/v1/stone/snapshots/remote/{bank}` | List snapshots on remote bank |
| POST | `/api/v1/stone/snapshots/trigger-all` | Trigger all scheduled snapshots |

#### Updates (was: Nourishment)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/stone/updates` | Check for pending updates |
| POST | `/api/v1/stone/updates/execute` | Execute pending updates |
| GET | `/api/v1/stone/updates/stream/{job_id}` | **SSE** — update progress stream |

#### Companions

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/stone/companions` | List companions |
| POST | `/api/v1/stone/companions/refresh` | Rescan companion directory |
| GET | `/api/v1/stone/companions/{id}` | Companion details |
| POST | `/api/v1/stone/companions/{id}/command` | Forward command |
| POST | `/api/v1/stone/companions/{id}/up` | Start companion |
| POST | `/api/v1/stone/companions/{id}/down` | Stop companion |

#### Greenhouse (Manifest Authoring)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/stone/greenhouse/containers` | List Docker containers |
| GET | `/api/v1/stone/greenhouse/catalog` | Manifest catalog |
| GET | `/api/v1/stone/greenhouse/file` | Read manifest file |
| PUT | `/api/v1/stone/greenhouse/file` | Write manifest file |
| DELETE | `/api/v1/stone/greenhouse/file` | Delete manifest file |
| GET | `/api/v1/stone/greenhouse/export` | Export offering |
| POST | `/api/v1/stone/greenhouse/validate` | Validate manifest |
| POST | `/api/v1/stone/greenhouse/generate` | Generate manifest from image |
| POST | `/api/v1/stone/manifests/test` | Test-deploy manifest |

#### Console

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/console/mode` | Current console verbosity |
| POST | `/api/v1/console/mode` | Set console verbosity |

---

### Endpoints — Garden (Cluster-Wide)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/garden` | Garden overview |
| GET | `/api/v1/garden/topology` | Full topology (all stones + states) |
| GET | `/api/v1/garden/stones/{name}` | Remote stone capabilities |
| GET | `/api/v1/garden/services` | Find services across garden (`?q=`) |
| POST | `/api/v1/garden/recommend` | Placement recommendation for offering |
| PUT | `/api/v1/garden/gateway/{offering}` | Mutate offering via garden gateway |
| DELETE | `/api/v1/garden/gateway/{offering}` | Remove via garden gateway |
| GET | `/api/v1/garden/tools` | Tools snapshot (aggregated) |
| GET | `/api/v1/garden/tools/stream` | **SSE** — tools event stream |
| GET | `/api/v1/garden/updates` | Aggregate pending updates |
| POST | `/api/v1/garden/updates/execute` | Dispatch updates to affected stones |

#### Garden Storage

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/garden/storage` | List all storage across garden |
| GET | `/api/v1/garden/storage/{name}` | Discover replicas |
| GET | `/api/v1/garden/storage/{name}/fs` | Directory listing (`?path=&depth=N`) |
| GET | `/api/v1/garden/storage/{name}/fs/{*path}` | Read file |
| PUT | `/api/v1/garden/storage/{name}/fs/{*path}` | Write file |
| DELETE | `/api/v1/garden/storage/{name}/fs/{*path}` | Delete file |
| HEAD | `/api/v1/garden/storage/{name}/fs/{*path}` | File metadata |
| GET | `/api/v1/garden/storage/{name}/objects/{*path}` | Read S3 object |
| PUT | `/api/v1/garden/storage/{name}/objects/{*path}` | Write S3 object |
| DELETE | `/api/v1/garden/storage/{name}/objects/{*path}` | Delete S3 object |
| HEAD | `/api/v1/garden/storage/{name}/objects/{*path}` | S3 object metadata |
| GET | `/api/v1/garden/storage/{name}/snapshots` | Offerings with snapshots (currently `/memories`) |
| GET | `/api/v1/garden/storage/{name}/snapshots/{offering}` | Offering snapshots (currently `/memories/{offering}`) |
| GET | `/api/v1/garden/storage/{name}/snapshots/{offering}/manifest` | Snapshot manifest |
| GET | `/api/v1/garden/storage/{name}/snapshots/{offering}/{id}` | Download snapshot (currently `/{harvest_id}`) |

#### WebDAV

| Method | Path | Description |
|--------|------|-------------|
| ANY | `/dav/{name}/{*path}` | RFC 4918 (PROPFIND, GET, PUT, DELETE, MKCOL, COPY, MOVE) |

---

### Endpoints — Pond (Security)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/pond/status` | Pond status and membership |
| GET | `/api/v1/pond/ca.pem` | Download CA certificate |
| POST | `/api/v1/pond/init` | Create pond (this stone becomes keystone) |
| POST | `/api/v1/pond/join` | Join pond with invitation code |
| POST | `/api/v1/pond/enroll-client` | Enroll a client (non-stone) |
| POST | `/api/v1/pond/invite` | Generate invitation |
| POST | `/api/v1/pond/unlock` | Unlock CA after restart |
| POST | `/api/v1/pond/ceremony` | Advance ceremony state |
| POST | `/api/v1/pond/promote` | Promote stone to standby CA |
| PUT | `/api/v1/pond/name` | Rename pond |
| DELETE | `/api/v1/pond` | Drain pond (destroy trust network) |
| DELETE | `/api/v1/pond/stones/{name}` | Untrust / revoke stone |

---

### Endpoints — Admin

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/v1/admin/moss/shutdown` | Graceful moss shutdown |
| POST | `/api/v1/admin/moss/take-root` | Install as system service |
| POST | `/api/v1/admin/stone/shutdown` | Power off stone |
| POST | `/api/v1/admin/stone/reboot` | Reboot stone |
| POST | `/api/v1/admin/stone/{name}/wake` | Wake-on-LAN |

---

### Endpoints — Jobs

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/jobs` | List active/recent jobs |
| GET | `/api/v1/jobs/{job_id}` | Job status |

---

### Endpoints — Utility

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Health check (200/503) |
| GET | `/api/v1/manifest` | API manifest (self-describing) |
| POST | `/api/v1/election/start` | Trigger election (testing) |
| POST | `/api/v1/helpers/json-transform` | JSON transformation utility |

---

## Part 4 — CLI ↔ API Mapping

| CLI Command | HTTP Method | API Endpoint |
|-------------|-------------|--------------|
| `list` | GET | `/stone/services` |
| `list --strays` | GET | `/stone/offerings/adoptable` |
| `list --adopted` | GET | `/stone/offerings/adopted` |
| `list --borrowed` | GET | `/stone/offerings/borrowed` |
| `find <query>` | GET | `/garden/services?q=<query>` |
| `observe` | GET | `/garden` + `/garden/topology` |
| `pulse` | GET | `/stone/pulse/stream` (SSE) |
| `watch` | GET | `/stone/presence/stream` (SSE) |
| `watch <service>` | GET | `/stone/services/{service}/logs` (SSE) |
| `status <service>` | GET | `/stone/services/{service}` |
| `offer` | GET | `/stone/offerings` |
| `offer <name>` | PUT | `/garden/gateway/{offering}` |
| `offer info <name>` | GET | `/stone/offerings/{name}` |
| `offer image <ref>` | PUT | `/garden/gateway/{offering}` |
| `rest <service>` | POST | `/stone/services/{service}/rest` |
| `wake <service>` | POST | `/stone/services/{service}/wake` |
| `upgrade <service>` | POST | `/stone/services/{service}/upgrade` |
| `upgrade --all` | POST | `/stone/updates/execute` |
| `remove <service>` | DELETE | `/stone/services/{service}` |
| `uproot <service>` | POST | `/stone/services/{service}/destroy` |
| `adopt <target>` | POST | `/stone/offerings/{offering}/adopt` |
| `release <service>` | DELETE | `/stone/offerings/{offering}/adopt` |
| `borrow <name>` | POST | `/stone/offerings/borrow` |
| `return <name>` | DELETE | `/stone/offerings/borrow/{name}` |
| `pond init` | POST | `/pond/init` |
| `pond join <code>` | POST | `/pond/join` |
| `pond invite` | POST | `/pond/invite` |
| `pond status` | GET | `/pond/status` |
| `pond drain` | DELETE | `/pond` |
| `pond remove <stone>` | DELETE | `/pond/stones/{name}` |
| `pond unlock` | POST | `/pond/unlock` |
| `pond promote <stone>` | POST | `/pond/promote` |
| `pond rename <name>` | PUT | `/pond/name` |
| `storage list` | GET | `/stone/storage/banks` |
| `storage add <target>` | POST | `/stone/storage/add` |
| `storage pin <name>` | POST | `/stone/storage/banks/{name}/pin` |
| `storage unpin <name>` | POST | `/stone/storage/banks/{name}/unpin` |
| `storage release <name>` | POST | `/stone/storage/banks/{name}/release` |
| `store ls <bucket>` | GET | `/stone/storage/s3/{bucket}` |
| `store get <b> <k>` | GET | `/stone/storage/s3/{bucket}/{key}` |
| `store put <b> <k> <f>` | PUT | `/stone/storage/s3/{bucket}/{key}` |
| `store rm <b> <k>` | DELETE | `/stone/storage/s3/{bucket}/{key}` |
| `stone wake <name>` | POST | `/admin/stone/{name}/wake` |
| `stone shutdown` | POST | `/admin/stone/shutdown` |
| `stone reboot` | POST | `/admin/stone/reboot` |
| `stone install` | POST | `/admin/moss/take-root` |
| `stone reconcile` | POST | `/stone/services/reconcile` |
| `stone verbosity <lvl>` | POST | `/console/mode` |
| `backup status` | GET | `/stone/snapshots` |
| `backup list <offering>` | GET | `/stone/snapshots/{offering}` |
| `backup trigger <offering>` | POST | `/stone/snapshots/{offering}/trigger` |
| `backup trigger-all` | POST | `/stone/snapshots/trigger-all` |
| `backup restore <offering>` | POST | `/stone/snapshots/{offering}/restore` |
| `capabilities <o> add` | POST | `/stone/offerings/{o}/capabilities` |
| `capabilities <o> remove` | DELETE | `/stone/offerings/{o}/capabilities/{c}` |
| `capabilities <o> refresh` | POST | `/stone/offerings/{o}/capabilities/refresh` |
| `capabilities <o> mirror` | POST | `/stone/offerings/{o}/capabilities/mirror` |
| `launch` | GET | `/` (portrait HTML) |
| `api` | GET | `/api/v1/manifest` |

---

## Part 5 — Open Questions

### Resolved

| Question | Decision |
|----------|----------|
| `--on` vs `--at` | **`--at`** — no migration cost, both read fine |
| `capabilities` length | **`cap` alias** — precedent set by `explore` |
| `--wishful` naming | **`--ensure`** — describes action, not feeling |
| Snapshot terminology | **`snapshot` everywhere** — CLI group is `backup`, noun is `snapshot`, API is `/snapshots` |

### Remaining

1. **Top-level verb count.** Now ~25 with `logs`, `config`, `hey`, `ceremony`, `template`,
   `election`. If it grows further, consider grouping `adopt`/`release`/`borrow`/`return`
   under an `adoption` subcommand.

2. **`explore` alias.** Worth keeping? Zero cost in Clap, signals "browse only" intent.
   But adds a second name. Keep unless it confuses beginners.

3. **S3 gateway path.** Currently `/api/v1/storage/s3/` (top-level). Proposed:
   `/stone/storage/s3/`. Breaking change — worth it for consistency?

4. **`/greenhouse` vs `/manifests`** in the API. CLI uses `manifest`, API uses
   `/greenhouse`. Align to `/stone/manifests/`?

5. **`rest` in a REST API.** `POST /services/{s}/rest` collides with the REST acronym.
   Not actually confusing in context (it's a POST action path), but alternatives exist:
   `/pause`, `/stop`, `/sleep`. These lose the `rest`/`wake` domain pairing.

6. **`backup restore` source syntax.** Currently uses positional trailing args
   (`from slot A`, `from seed-bank media`). Should become pure flags
   (`--source slot-a`, `--source seed-bank:media`) to match the unified grammar.
