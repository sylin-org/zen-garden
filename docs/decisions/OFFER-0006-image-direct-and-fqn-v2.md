---
audience: developer
doc_type: decision
status: accepted
---

# OFFER-0006: Image-Direct Deployment, Source Schemes, and FQN v2

**Date**: 2026-03-02
**Status**: Accepted — Phase 1 complete, Phases 2–4 pending
**Supersedes**: [OFFER-0003](OFFER-0003-offering-fqn.md) (FQN format only; multi-instance semantics preserved)

---

## Living Document

This ADR covers a multi-phase architectural change. Implementation will surface discoveries — edge cases in container encoding, unexpected interactions with orchestrators, compatibility gaps on specific hardware — that cannot be fully anticipated at design time. When such discoveries arise, they are incorporated back into this ADR (or spawn child ADRs) to maintain a single source of truth. The ADR evolves with implementation; its status reflects the current state of understanding, not just the initial proposal.

Each phase's implementation should begin with a review of this ADR against current codebase state, and conclude with an update pass to capture what was learned.

---

## Context

Zen Garden's offering system is manifest-driven and curated. Every installable offering requires a pre-authored set of YAML files (snippet, frontmatter, compatibility, guidance) embedded in the Moss binary or placed in a filesystem overlay directory. This produces a high-quality, hardware-aware deployment experience — but it has a hard ceiling: if an image doesn't have a manifest, it can't participate in the garden.

This creates three gaps:

1. **Image-direct gap**: Users discover Docker images (Docker Hub, GHCR, private registries) and want to run them on their stones with full garden integration (lifecycle, topology, metrics, logs, federation). Today they must either author a manifest or use `docker run` manually, losing all garden benefits.

2. **Sharing gap**: Power users who tune offerings (custom environment variables, volumes, health checks) have no way to export that work as a manifest and share it with others. There is no community contribution model.

3. **FQN collision**: The current FQN format `offering[:instance]` uses a single colon as the instance separator. This prevents using colons for source scheme denomination (e.g., `image:nginx:latest`) because the parser cannot distinguish source colons from instance colons from Docker tag colons.

These three gaps are interdependent. Solving image-direct deployment requires a source scheme in the FQN. The source scheme requires freeing the colon from instance separation. And community sharing requires manifest authoring tooling that bridges image-direct deployments back into the manifest ecosystem.

---

## Decision

### 1. FQN v2: Double-Colon Instance Separator

The FQN instance separator changes from `:` (single colon) to `::` (double colon), freeing the single colon for source scheme denomination.

**New grammar:**

```
fqn       = [source ":"] name ["::" instance]
source    = "image" | "repo" | "oci" | identifier
name      = offering_name | image_reference | repo_path
instance  = identifier
```

**Examples:**

```
mongodb                              curated offering, default instance
mongodb::prod                        curated offering, prod instance
mongodb::adopted                     curated offering, adopted instance
image:nginx:latest                   image-direct, default instance
image:nginx:latest::staging          image-direct, staging instance
image:mongo:7                        image-direct, mongo tag 7
image:ghcr.io/org/app:v2             image-direct from GHCR
image:ghcr.io/org/app:v2::prod       image-direct from GHCR, prod instance
repo:community/bookstack             from remote manifest repo
repo:community/bookstack::dev        from repo, dev instance
```

**Why `::` works:**

- **Shell-safe**: No special meaning in bash, zsh, or fish. No quoting needed.
- **URL-safe**: Colons are permitted in URL path segments (RFC 3986).
- **Unambiguous**: Docker image tags follow `[a-zA-Z0-9_.-]+` — no colons in tags, so `::` never appears inside a valid Docker image reference. The parser splits on `::` first (instance), then on the first `:` for source prefix.
- **Familiar**: Rust and C++ developers recognize `::` as a namespace qualifier.

**Why not other separators:**

| Candidate | Problem |
|-----------|---------|
| `#` | Bash comment character — `mongodb#prod` silently truncates to `mongodb` |
| `\|` | Bash pipe operator — `mongodb\|prod` pipes to a command called `prod` |
| `/` | Ambiguous with Docker image paths (`ghcr.io/org/app:v2/prod`) |
| `@` | Reserved for placement semantics in CLI context (`--at stone-01`) |

**Validation rules** (unchanged from OFFER-0003 except separator):

- Each segment (offering and instance) must be lowercase after normalization
- Allowed characters: `[a-z0-9_-]`
- Must start with a letter
- Max 128 characters per segment
- `--` remains reserved (container encoding)
- Only one `::` separator allowed in an FQN

**Canonicalization:**

```
mongodb::mongodb  →  mongodb     (default instance reduction, unchanged)
ollama::adopted   →  ollama::adopted
```

### 2. Container Name Encoding

#### Critical backward-compatibility property

Container names are derived from the **parsed** `OfferingFqn` struct, not from the raw FQN string. The method `encoded_for_container()` concatenates `offering` + `OFFERING_FQN_CONTAINER_SEPARATOR` (`"--"`) + `instance`. Since both V1 (`mongodb:prod`) and V2 (`mongodb::prod`) parse to the identical struct `OfferingFqn { offering: "mongodb", instance: Some("prod") }`, they produce the identical container name: `zen-offering-mongodb--prod`.

**Existing curated offering containers require zero changes.** No renaming, no restart, no Docker operations. The `OFFERING_FQN_CONTAINER_SEPARATOR` constant (`"--"`) is unchanged.

#### Curated offerings (unchanged)

```
FQN (V1)           FQN (V2)              Container Name
mongodb             mongodb               zen-offering-mongodb
mongodb:prod        mongodb::prod         zen-offering-mongodb--prod
ollama:adopted      ollama::adopted       zen-offering-ollama--adopted
```

All three columns produce the same container name. V1 → V2 is transparent at the Docker layer.

#### Image-direct offerings (new)

Image-direct offerings are new — no existing containers to migrate. Their container names must avoid collision with curated offerings. The source prefix `image` becomes part of the encoded offering name:

```
FQN                                    Container Name
image:nginx:latest                     zen-offering-img-nginx-latest
image:nginx:latest::staging            zen-offering-img-nginx-latest--staging
image:ghcr.io/org/app:v2               zen-offering-img-ghcr-io-org-app-v2
image:ghcr.io/org/app:v2::prod         zen-offering-img-ghcr-io-org-app-v2--prod
```

Encoding rules for image-direct:
- Source prefix `image:` → container prefix `img-`
- Image reference sanitized: `/` and `:` replaced with `-`, dots preserved where valid
- Instance separator remains `--` (same as curated)

The `img-` prefix ensures no collision between `zen-offering-nginx` (curated) and `zen-offering-img-nginx-latest` (image-direct).

#### Decoding algorithm (updated)

1. Strip `zen-offering-` prefix
2. If remainder starts with `img-` → image-direct: extract sanitized image ref + optional instance after `--`
3. Otherwise → curated: split on `--` for offering + instance, reconstruct FQN with `::`

### 3. Source Schemes

Source schemes are a first-class prefix in the FQN that determines how an offering is resolved:

| Source | Resolution | Example |
|--------|-----------|---------|
| *(none)* | Manifest registry (embedded + custom + repos) | `mongodb` |
| `image:` | Docker registry image inspection | `image:nginx:latest` |
| `repo:` | Remote manifest repository | `repo:community/bookstack` |
| `oci:` | OCI artifact pull (future) | `oci:ghcr.io/zen/manifests/bookstack:1.0` |

Source schemes are **extensible** — new sources can be added without grammar changes.

**Tool FQID interaction:**

The tools domain (TOOLS-0002) uses `tool_fqid = "{tool-type}:{fqid}"`. With the FQN change:

```
offering:ollama                      tool type = offering, FQN = ollama
offering:ollama::dev                 tool type = offering, FQN = ollama::dev
offering:image:nginx:latest          tool type = offering, FQN = image:nginx:latest
seed-bank:default                    tool type = seed-bank, FQN = default
```

Parsing rule: split on first `:` for tool type, the remainder is the FQN (which may itself contain `:` for source scheme and `::` for instance). This is unambiguous because tool types are a known enumeration.

### 4. Image-Direct Deployment

Users deploy any Docker image directly through the standard offering workflow:

```bash
garden-rake offer image nginx:latest              # register + deploy
garden-rake offer image nginx:latest info          # inspect only, don't deploy
garden-rake offer image nginx:latest --at stone-03 # deploy on specific stone
garden-rake offer image mongo:7::analytics         # deploy with named instance
```

**Resolution flow:**

1. **Manifest collision check**: Moss queries the reverse index (image reference → curated offerings). If a curated manifest exists for this image family, the user is prompted:
   > "A curated manifest exists for MongoDB that includes health checks, AVX compatibility fallbacks, and configuration guidance. Use the curated manifest instead? [Y/n]"

2. **Compatibility gate**: If a curated manifest exists for the image family, its compatibility rules are applied even for image-direct deployment. If the stone fails compatibility:
   > "This stone doesn't have AVX support required by mongo:7. Available stones: stone-03, stone-04. Use `--at stone-03` to deploy there."

   If no curated manifest exists for the image, no compatibility check is performed (can't know what we don't know).

3. **Image pull + inspection**: Moss pulls the image via Bollard, then inspects the OCI image config to extract:
   - `ExposedPorts` → port candidates
   - `Volumes` → volume mount points
   - `Env` → default environment variables
   - `Cmd` / `Entrypoint` → command
   - OCI labels (`org.opencontainers.image.*`) → description, URL, license

4. **Synthetic manifest generation**: Moss generates a minimal manifest from the inspection:
   - Snippet YAML with image, ports, volumes, env
   - Frontmatter JSON with name, description (from labels), category `custom`
   - No compatibility rules, no capabilities, no guidance

5. **Deployment**: Standard pipeline — port resolution, volume resolution, container creation, health verification, catalog registration, topology announcement.

6. **Persistence**: The synthetic manifest is saved to `{data_dir}/manifests/custom/{name}/` for future reference and enrichment.

**Post-hoc compatibility signal**: When an image-direct offering crashes with a log pattern that a curated manifest would have caught (e.g., "Illegal instruction" for AVX), Moss surfaces:
> "This offering crashed. A curated manifest for MongoDB handles this hardware. Try `plant mongodb` instead."

### 5. Offering Resolution Pipeline (Redesign)

The offering lifecycle is restructured into two distinct stages:

```
┌────────────────────────────────────────────┐
│          Offering Resolution               │
│                                            │
│  Input: FQN string                         │
│                                            │
│  1. Parse FQN → (source, name, instance)   │
│  2. Resolve source:                        │
│     ├─ (none) → manifest registry lookup   │
│     ├─ image: → registry pull + inspect    │
│     ├─ repo:  → remote manifest fetch      │
│     └─ oci:   → OCI artifact pull          │
│  3. Produce: ResolvedOffering              │
│     (unified type, identical for all sources)│
│  4. Compatibility gate                     │
│     (cross-reference if manifest available)│
│  5. Collision check                        │
│     (flag if curated alternative exists)   │
└─────────────────┬──────────────────────────┘
                  │
                  ▼
┌────────────────────────────────────────────┐
│          Deployment Pipeline               │
│                                            │
│  Input: ResolvedOffering                   │
│  (identical regardless of source)          │
│                                            │
│  1. Port resolution                        │
│  2. Volume resolution                      │
│  3. Image pull                             │
│  4. Container creation                     │
│  5. Health verification                    │
│  6. Catalog registration                   │
│  7. Topology announcement                  │
└────────────────────────────────────────────┘
```

The deployment pipeline does not know or care whether the offering came from an embedded manifest, an image inspection, a remote repo, or an OCI artifact. This separation is the architectural foundation that makes image-direct deployment first-class rather than a bolt-on.

### 6. First-Class Catalog Representation

Image-direct offerings appear in the catalog identically to curated offerings. They participate in all standard behaviors:

| Behavior | Curated | Image-Direct |
|----------|---------|-------------|
| Container naming (`zen-offering-*`) | Yes | Yes |
| Lifecycle (start/stop/restart) | Yes | Yes |
| Log streaming (SSE) | Yes | Yes |
| Metrics (CPU/mem/net/disk) | Yes | Yes |
| Topology participation (TOPO-0002) | Yes | Yes |
| Garden-wide announcement | Yes | Yes |
| Federation policy | Yes | Yes |
| Port conflict resolution | Well-known catalog | Best-effort (remap) |
| Health check | Manifest-defined | Docker HEALTHCHECK or container-state |
| Search/discovery | Full taxonomy | Name + OCI labels only |
| Nourishment (updates) | Image tag tracking | See update policy below |
| Adoption/self-healing | Yes | Yes |
| Compatibility rules | Manifest-defined | Cross-referenced if available |
| Capabilities (sub-resources) | Manifest-defined | Not available |
| Guidance | Rich markdown | Not available (OCI description if present) |
| Config file management | Manifest-defined | Not available |

The offering carries a `source` field indicating its origin:

```rust
pub enum OfferingSource {
    Embedded,       // shipped with Moss
    Custom,         // operator-authored in {data_dir}/manifests/custom/
    ImageDirect,    // from image inspection
    Repository(String),  // from named remote repo
}
```

### 7. Update Policy

- **`:latest` tag**: Moss tracks the image digest at deploy time. During nourishment scans, it pulls the tag again, compares digests, and flags an update if changed. User confirms before recreate.
- **Pinned tags** (e.g., `mongo:7`, `nginx:1.27`): No automatic checking. User explicitly requests a tag change via `garden-rake offer image mongo:8` (new offering) or a future `--update-tag` flag.
- **Private registry authentication**: Delegated entirely to Docker's credential helpers (`~/.docker/config.json`). Moss does not maintain its own auth layer.

### 8. Enrichment Gradient

An offering can exist at any level of richness. Users start anywhere and enrich upward:

| Level | Source | Metadata | Search | Health | Guidance | Capabilities |
|-------|--------|----------|--------|--------|----------|-------------|
| **Bare** | `offer image` | Auto-generated from OCI inspect | Name only | Container state | None | None |
| **Tagged** | User-enriched bare | Tags + description | Intent-based | Container state | None | None |
| **Curated** | Repo or custom manifest | Full frontmatter | Full taxonomy | Manifest health check | Yes | Optional |
| **Complete** | Embedded or verified repo | Everything | Full + compatibility | Full + post-install scan | Yes | Yes |

The enrichment path:

```bash
garden-rake offer image nginx:latest            # Level 0: Bare
garden-rake manifest enrich nginx               # Interactive: add tags, description, health check
garden-rake manifest export nginx ./nginx/      # Export as manifest bundle
garden-rake manifest publish ./nginx/ --to ...  # Share with community
```

### 9. Manifest Authoring Tooling

#### CLI (Rake)

```bash
garden-rake manifest init <name> --image <ref>    # scaffold from image inspection
garden-rake manifest validate <path>              # structural + security validation
garden-rake manifest test <path>                  # deploy, health check, teardown
garden-rake manifest export <offering>            # extract from running container
garden-rake manifest enrich <offering>            # interactive metadata enrichment
```

**Container surface analysis** (`manifest export`): Inspects a running container (including non-Zen-Garden containers) and extracts image, ports, volumes, env vars, command, health check, labels. Filters out Zen Garden injections (topology mount, `KOI_ENDPOINT`, `GARDEN_STONE_ENDPOINT`, `GARDEN_OFFERING_NAME`, DNS config). Generates snippet YAML + frontmatter JSON with `# TODO` markers for fields that require human authorship (category, tags, guidance, compatibility rules, capabilities).

**Validation rules** (security-critical for community manifests):

- No host path mounts outside `{data_dir}/` (prevents filesystem exfiltration)
- No `privileged: true` without explicit confirmation
- No `network_mode: host` without explicit confirmation
- No `pid: host` or `ipc: host`
- Environment variable names validated (no injection via `LD_PRELOAD`)
- Image reference validated
- Manifest schema validation (required fields, valid types)

#### Visual (Portrait — Greenhouse View)

A manifest workshop embedded in the Portrait SPA, accessible at `/api/v1/stone/greenhouse`:

- Pick a running container → inspect → pre-fill form
- Or enter an image reference → pull + inspect → pre-fill
- Edit: named ports, volumes, env vars, tags, category, description
- Add: health check (template suggestions based on category), guidance (markdown editor)
- Validate: real-time schema checking
- Test: deploy-test-teardown cycle with live log output
- Export: download as manifest bundle (zip of YAML/JSON/MD files)

The Greenhouse API is the same API that Rake calls — the SPA is a visual frontend to the same endpoints.

### 10. Remote Manifest Repositories

Users add remote manifest sources. Moss fetches manifest indices and merges them into the local catalog.

```bash
garden-rake repo add community https://github.com/zen-garden/community-manifests.git
garden-rake repo sync                     # refresh all repo indices
garden-rake repo list                     # list configured repos
garden-rake repo remove community         # remove a repo
garden-rake offer --search "wiki"         # searches embedded + all repos
garden-rake offer repo:community/bookstack  # install from repo
```

**Git-based repositories** (primary format):

- Repository is a Git repo with manifests in standard directory structure (same layout as `src/moss/embedded/manifests/sw/`)
- Moss performs sparse checkout of the index + requested manifests
- Community sharing via GitHub/GitLab — fork, PR, star
- Bot validation on submitted PRs: schema check, security scan, test deployment, auto-label by category
- Human curation after bot approval

**Repository resolution order**: embedded (highest priority) → custom → repos (in registration order).

**Manifest bundle format** (for sharing):

```
bookstack/
├── bookstack.snippet.yaml          # Required: container config
├── bookstack.frontmatter.json      # Required: metadata
├── bookstack.compatibility.yaml    # Optional: hardware rules
├── bookstack.guidance.md           # Optional: user docs
├── bookstack.capabilities.yaml     # Optional: sub-resource management
└── bookstack.adopted.yaml          # Optional: native detection
```

This is identical to the embedded manifest structure — no new format.

**Social layer**: Community repositories support social interactions (likes, comments, download counts) through the hosting platform (GitHub stars, issues) and through a Portrait-based browser that aggregates social signals from the repo API.

**Configuration persistence**: `{config_dir}/repos.json` stores registered repos. Cached indices and manifests stored at `{data_dir}/repos/{name}/`.

---

## Rationale

**Why change the FQN separator?** The current single-colon separator creates a three-way collision: Docker image tags use colons (`nginx:latest`), the current FQN uses colons (`mongodb:prod`), and source schemes use colons (`image:`). The double-colon eliminates this by giving each colon-count a distinct meaning: single = source/Docker tag, double = instance.

**Why image-direct deployment?** 80% of Zen Garden's value — lifecycle management, monitoring, topology, metrics, logs, federation — works identically regardless of whether the offering has a curated manifest. Gating 100% of the value behind manifest authorship is artificial friction. The enrichment gradient captures the remaining 20% as an aspirational path, not a prerequisite.

**Why a resolution pipeline redesign?** The current implementation interleaves resolution and deployment. Separating them is what makes image-direct deployment first-class. The deployment pipeline accepts a `ResolvedOffering` and doesn't care where it came from. This is also the foundation for future source types (repos, OCI artifacts) without further pipeline changes.

**Why Greenhouse in Portrait?** Manifest authoring needs visual feedback — seeing extracted ports, editing tags, previewing guidance markdown. A CLI can scaffold, but a visual editor is the on-ramp for users who won't touch YAML. Embedding in Moss (rather than a separate service) means no additional deployment and the SPA has direct access to Docker inspection and the offerings index.

**Why Git-based repos?** Git provides versioning, access control, contribution workflow (PRs), social signals (stars), and CI integration (Actions for bot validation) out of the box. HTTP API repos add hosting burden. Git-based repos give 90% of the functionality with 10% of the operational cost.

---

## Consequences

### Positive

- Any Docker image becomes a first-class garden citizen with one command
- Community can contribute manifests without core team bottleneck
- FQN grammar is extensible to future source types without further breaking changes
- Enrichment gradient means no wasted work — bare deploys can be incrementally enriched
- Compatibility cross-reference protects users even when they bypass curation
- Container names remain deterministic and collision-resistant

### Negative

- Breaking change to FQN format requires migration of existing deployments
- Image-direct offerings have weaker metadata (no taxonomy, no capabilities, no guidance)
- Remote repos introduce a trust/security surface for community manifests
- Double-colon is slightly more verbose than single-colon in CLI usage

### Neutral

- Default instance behavior unchanged (`mongodb` remains canonical, not `mongodb::mongodb`)
- Container prefix `zen-offering-` unchanged
- Existing offering modes (Managed, Adopted, Borrowed) unchanged
- All curated manifests work as before with no modification

---

## Migration & Backward Compatibility

### Invariant: Zero-Downtime Upgrade

A stone running V1 Moss with existing offerings must upgrade to V2 Moss without:
- Stopping or restarting any running container
- Renaming any container
- Losing any persisted offering data
- Breaking cross-stone topology visibility during the transition

### What Does NOT Change

| Component | V1 | V2 | Change? |
|-----------|----|----|---------|
| Docker container names | `zen-offering-mongodb--prod` | `zen-offering-mongodb--prod` | **No change** |
| Container prefix | `zen-offering-` | `zen-offering-` | **No change** |
| Container instance separator | `--` | `--` | **No change** |
| `OFFERING_FQN_CONTAINER_SEPARATOR` | `"--"` | `"--"` | **No change** |
| Docker volumes | `{data_dir}/volumes/mongo-data` | Same | **No change** |
| Config file mounts | Same paths | Same paths | **No change** |
| Companion port ledger | Uses companion names, not FQNs | Same | **No change** |
| Offerings cache | Uses manifest keys, not instance FQNs | Same | **No change** |
| Hardware capabilities | No FQNs | Same | **No change** |

Container names are derived from parsed `OfferingFqn { offering, instance }` fields, which are identical between V1 and V2. The FQN separator change is invisible at the Docker layer.

### What Changes

| Component | V1 Format | V2 Format | Migration |
|-----------|-----------|-----------|-----------|
| `moss-offerings.json` `.name` field | `"ollama:adopted"` | `"ollama::adopted"` | Auto: `normalize_legacy_fqn()` on load |
| `garden-topology.json` service names | `"mongodb:prod"` | `"mongodb::prod"` | Auto: re-flushed within 30s after migration |
| UDP chirp payloads | `"mongodb:prod"` | `"mongodb::prod"` | Auto: next chirp uses new format |
| SSE event payloads | `"mongodb:prod"` | `"mongodb::prod"` | Auto: uses `.fqn()` method |
| Gateway registrations | `"mongodb:prod"` | `"mongodb::prod"` | Auto: in-memory, regenerated by orchestrators |
| Tool FQIDs | `"offering:ollama:dev"` | `"offering:ollama::dev"` | Auto: tool FQID construction uses `.fqn()` |
| CLI output | `ollama:dev` | `ollama::dev` | Auto: display uses `.fqn()` |

### Persistence Migration

`normalize_legacy_fqn()` in `src/moss/src/infra/persistence.rs` already handles `@` → `:` migration. It gains one new rule:

```rust
fn normalize_legacy_fqn(name: &str) -> Option<String> {
    // Existing: @ → : (v0 → v1 migration)
    if name.contains('@') {
        let candidate = name.replace('@', ":");
        return parse_offering_fqn(&candidate).ok().map(|fqn| fqn.fqn());
    }

    // New: : → :: (v1 → v2 migration)
    // Detect single colon that is NOT already part of `::`
    if name.contains(':') && !name.contains("::") {
        let candidate = name.replacen(':', "::", 1);
        return parse_offering_fqn(&candidate).ok().map(|fqn| fqn.fqn());
    }

    None
}
```

On first load after upgrade, `moss-offerings.json` is normalized in-memory and re-saved with V2 format. Subsequent loads find no legacy entries.

### Mixed-Version Garden

During a rolling upgrade, some stones run V1 Moss and others run V2. Topology chirps from V1 stones contain `mongodb:prod` while V2 stones emit `mongodb::prod`. Receivers must handle both:

- V2 Moss receiving V1 chirps: `normalize_legacy_fqn()` applied to incoming topology entries
- V1 Moss receiving V2 chirps: the string `mongodb::prod` is opaque to V1 — it will treat `::prod` as part of the name, which is cosmetically wrong but non-destructive (no crash, no data loss, just a display artifact until V1 is upgraded)

This mixed-version window is acceptable for the upgrade duration. No coordination protocol is needed.

### Tools Domain

Tool FQIDs (`offering:ollama:dev`) become `offering:ollama::dev`. The parsing rule (split on first `:` for tool type) is unchanged. Orchestrators and Koi consumers that parse tool FQIDs update their expectation from single-colon instance separator to double-colon.

---

## Design Decisions (Resolved)

Decisions made during review, recorded for continuity:

1. **`OfferingFqn.source` lives in `garden_common`**, not as a Moss-local wrapper. Source is a fundamental identity property — Rake needs it for display, orchestrators need it for matching, topology entries carry it. All crates are aware of image-direct as a concept.

2. **Phase 1 includes B.1 + B.3 + B.4** (FQN as type, centralized construction, single normalization gateway). The type-level cleanup ships alongside the separator change to avoid double-touching every call site. This means `OfferingFqn` replaces raw `String` FQN fields across all structs, builder methods replace scattered `format!()` calls, and `OfferingFqn::parse()` absorbs legacy normalization.

3. **Integration testing includes orchestrator validation on live garden.** A `#[ignore]` integration test in the MongoDB orchestrator crate discovers available stones, installs `mongodb::test` on 2+ compatible stones, waits for orchestrator recognition, verifies `derive_replica_set_name` resolves to `"zen-garden-test"`, verifies gateway registration with correct connection string, and cleans up. Discovery-first (uses garden topology API), not hardcoded stone names.

---

## Implementation

### Phase 1: FQN v2 + Architectural Cleanup (Core)

Phase 1 combines the separator change with B.1/B.3/B.4 cleanup to avoid double-touching call sites.

**`OfferingFqn` type redesign** (`src/common/src/offerings.rs`):

- Add `source: Option<OfferingSource>` field (Image, Repo, Oci — or None for curated)
- Add `image_ref: Option<String>` field (raw Docker image reference for image-direct)
- Implement `Serialize`/`Deserialize` (custom: serializes as FQN string in JSON for backward-compatible persistence)
- Implement `Display` (delegates to `fqn()`)
- Builder methods: `OfferingFqn::new()`, `::with_instance()`, `::adopted()`, `::image_direct()`
- `OfferingFqn::parse()` replaces `parse_offering_fqn()` — absorbs legacy normalization (`@`→`::`, `:`→`::`) so every entry point gets migration for free
- Unit tests: full V2 coverage per test plan, plus legacy format tests

**Constant change** (`src/common/src/constants/mod.rs`):

- `OFFERING_FQN_SEPARATOR: char = ':'` → `OFFERING_FQN_SEPARATOR: &str = "::"`
- `OFFERING_FQN_CONTAINER_SEPARATOR` remains `"--"` (container names backward compatible)
- `OFFERING_FQN_SEPARATOR` may become private or removed entirely — with builder methods and `parse()`, external code should never need the raw separator

**String→Type migration** (all crates):

- `Offering.name: String` → `Offering.name: OfferingFqn` (with serde as string)
- `TopologyServiceEntry.name: String` → `TopologyServiceEntry.name: OfferingFqn`
- `MongoInstance.fqn: String` → `MongoInstance.fqn: OfferingFqn`
- `PendingAction.fqn: String` → `PendingAction.fqn: OfferingFqn`
- `FqnGatewayEntry.fqn: String` → `FqnGatewayEntry.fqn: OfferingFqn`
- All `format!("{}{}{}", ..., SEPARATOR, ...)` → `OfferingFqn::adopted()` / `::with_instance()`
- All `fqn.strip_prefix("mongodb:")` → `fqn.instance` field access
- Chirp receive: deserialized `TopologyServiceEntry.name` goes through `OfferingFqn` serde (normalization is automatic via custom deserializer)

**Container encoding** (`src/moss/src/docker.rs`):

- `decode_offering_container_suffix()`: reconstructs `OfferingFqn` (constant change handles `::` automatically)
- Add `img-` prefix encoding/decoding for image-direct offerings
- `encoded_for_container()` unchanged for curated offerings (still uses `--`)

**Persistence migration** (`src/moss/src/infra/persistence.rs`):

- `normalize_legacy_fqn()` absorbed into `OfferingFqn`'s custom deserializer — legacy strings auto-normalize on deserialization
- `normalize_legacy_type()` similarly absorbed
- No separate migration function needed; `serde` handles it

**MongoDB orchestrator fixes** (`src/orchestrators/mongodb/`):

- `derive_replica_set_name(fqn: &OfferingFqn)` — reads `fqn.instance` instead of string prefix
- `cluster.rs`: uses `OfferingFqn::with_instance("mongodb", suffix)`
- `discovery.rs`: filter uses typed `OfferingFqn` matching instead of string prefix
- All doc comments updated to V2 format

**Capability search** (`src/rake/src/commands/discovery/find.rs`):

- Colon-counting logic replaced with `OfferingFqn::parse()` delegation

**Orchestrator integration test** (`src/orchestrators/mongodb/tests/`):

- `#[ignore]` test: discover stones → install `mongodb::test` → verify orchestrator recognition → verify replica set name → verify gateway → cleanup
- Runs with `cargo test --ignored` against a live garden

New offering resolution (new):
- `src/moss/src/domain/offering_resolution.rs`: `ResolvedOffering` type, resolution pipeline, image inspection, synthetic manifest generation

API additions:
- `src/moss/src/api/v1/offerings.rs`: `offer image` endpoint, collision check, compatibility cross-reference
- Rake CLI: `offer image` subcommand, `info` variant

**Auto-propagated (no code changes — constant handles it):**

All API handlers, domain modules, infra modules, tasks, bootstrap, Rake commands, and common tools that use `OFFERING_FQN_SEPARATOR` or call `parse_offering_fqn()` / `.fqn()` work automatically.

**Phase 1 status: COMPLETE** (2026-03-02). All items implemented, 799 tests pass, clippy clean. Key deliverables:

| Item | Files |
|------|-------|
| FQN v2 separator (`::`) + `OfferingFqn` type | `common/src/offerings.rs`, `common/src/constants/mod.rs` |
| String→Type migration (all crates) | `common/src/types.rs`, `moss/` (30+ files), orchestrators |
| Container encoding (`img-` prefix for image-direct) | `moss/src/docker.rs` |
| Persistence migration (V0/V1 auto-normalize via serde) | `OfferingFqn` custom deserializer |
| Image inspection infrastructure | `moss/src/infra/image_inspect.rs` (new) |
| Offering resolution domain | `moss/src/domain/offering_resolution.rs` (new) |
| Image-direct API endpoint | `moss/src/api/v1/services.rs`, `offerings.rs` |
| Image-direct async install task | `moss/src/tasks/job_executors.rs` |
| Image inspect endpoint | `GET /api/v1/stone/offerings/inspect?image={ref}` |
| Rake `offer image` subcommand | `rake/src/commands/offering/mod.rs`, `route.rs` |
| Projector FQID bug fix | `moss/src/domain/tools/projector.rs` |
| Doc sweep (V1→V2 examples) | 5 doc files, specs, guides |

### Phase 2: Manifest Authoring CLI

- `src/rake/src/commands/manifest.rs`: init, validate, test, export, enrich commands
- `src/common/src/manifests/validation.rs`: security and schema validation (reusable by Moss and Rake)
- Template scaffolding from image inspection results

### Phase 3: Greenhouse (Portrait)

- `/api/v1/stone/greenhouse` API endpoints in Moss
- Portrait SPA: Greenhouse view (manifest workshop)
- Image inspection endpoint, form-based editing, markdown preview, export

### Phase 4: Remote Repositories

- `src/moss/src/infra/manifest_repos.rs`: Git-based repo client
- `{config_dir}/repos.json`: registered repos
- `{data_dir}/repos/{name}/`: cached indices and manifests
- Search aggregation across embedded + custom + repo sources
- Rake: `repo add/sync/list/remove` commands

### Documentation Updates

Files requiring example updates (`:` → `::`):

- `docs/decisions/OFFER-0003-offering-fqn.md` — mark as superseded, link to this ADR
- `docs/specs/offering-fqn.md` — full rewrite of format, examples, encoding, URL encoding
- `docs/specs/offerings.md` — FQN format section, connection string examples
- `docs/guides/offering-lifecycle.md` — CLI examples
- `docs/guides/offering-sub-capabilities.md` — CLI examples
- `docs/guides/container-collision-avoidance.md` — encoding examples
- `docs/guides/tools-domain.md` — tool FQID examples
- `docs/archive/proposals/moss-tools-domain.md` — tool FQID examples
- `.agentic/rules/docker-ops.md` — container naming convention

### Orchestrator Updates

- `src/orchestrators/common/src/tasks/gateway_sync.rs`: FQN strings in `FqnGatewayEntry` adopt new format automatically (stored as opaque strings)
- `src/orchestrators/mongodb/src/domain/types.rs`: `derive_replica_set_name()` — verify parsing logic handles `::` correctly; the function splits on the FQN separator to extract instance name for replica set naming

### Verification

```bash
cargo check --all
cargo test --package garden-common    # FQN parser tests
cargo test --package moss             # API handler tests
cargo clippy -- -D warnings

# Orchestrators (standalone)
cd src/orchestrators/ollama && cargo check
cd src/orchestrators/mongodb && cargo check
```

---

## Test Plan

This feature requires an extensive test surface covering unit tests, integration tests, and live garden validation. Each phase gates on its test suite passing before the next phase begins.

### Unit Tests (cargo test)

**FQN parser — `src/common/src/offerings.rs`:**

| Test | Input | Expected |
|------|-------|----------|
| Default instance | `"mongodb"` | `{ offering: "mongodb", instance: None, source: None }` |
| Named instance | `"mongodb::prod"` | `{ offering: "mongodb", instance: Some("prod"), source: None }` |
| Canonicalization | `"mongodb::mongodb"` | `{ offering: "mongodb", instance: None }` |
| Case normalization | `"MongoDB::PROD"` | `{ offering: "mongodb", instance: Some("prod") }` |
| Image-direct no instance | `"image:nginx:latest"` | `{ offering: "nginx", source: Some(Image), image_ref: "nginx:latest" }` |
| Image-direct with instance | `"image:nginx:latest::staging"` | `{ source: Image, image_ref: "nginx:latest", instance: Some("staging") }` |
| Image-direct GHCR | `"image:ghcr.io/org/app:v2"` | `{ source: Image, image_ref: "ghcr.io/org/app:v2" }` |
| Image-direct GHCR + instance | `"image:ghcr.io/org/app:v2::prod"` | `{ source: Image, instance: Some("prod") }` |
| Repo source | `"repo:community/bookstack"` | `{ source: Repo("community"), offering: "bookstack" }` |
| Reject empty | `""` | Error |
| Reject multiple `::` | `"a::b::c"` | Error |
| Reject invalid chars | `"mongo db"` | Error |
| Legacy single-colon | `"mongodb:prod"` | Handled by migration, not parser |

**FQN display — `.fqn()` method:**

| Input Struct | Expected Output |
|-------------|-----------------|
| `{ offering: "mongodb", instance: None }` | `"mongodb"` |
| `{ offering: "mongodb", instance: Some("prod") }` | `"mongodb::prod"` |
| `{ source: Image, image_ref: "nginx:latest" }` | `"image:nginx:latest"` |
| `{ source: Image, image_ref: "nginx:latest", instance: Some("staging") }` | `"image:nginx:latest::staging"` |

**Container encoding — `encoded_for_container()`:**

| FQN | Container Name |
|-----|---------------|
| `mongodb` | `zen-offering-mongodb` |
| `mongodb::prod` | `zen-offering-mongodb--prod` |
| `image:nginx:latest` | `zen-offering-img-nginx-latest` |
| `image:nginx:latest::staging` | `zen-offering-img-nginx-latest--staging` |
| `image:ghcr.io/org/app:v2` | `zen-offering-img-ghcr-io-org-app-v2` |

**Container decoding — round-trip:**

| Container Name | Decoded FQN |
|---------------|-------------|
| `zen-offering-mongodb` | `mongodb` |
| `zen-offering-mongodb--prod` | `mongodb::prod` |
| `zen-offering-img-nginx-latest` | `image:nginx:latest` |
| `zen-offering-img-nginx-latest--staging` | `image:nginx:latest::staging` |

**Legacy migration — `normalize_legacy_fqn()`:**

| Input | Output |
|-------|--------|
| `"ollama:adopted"` (V1) | `Some("ollama::adopted")` |
| `"mongodb:prod"` (V1) | `Some("mongodb::prod")` |
| `"ollama@adopted"` (V0) | `Some("ollama::adopted")` |
| `"mongodb"` (no instance) | `None` (no change needed) |
| `"mongodb::prod"` (already V2) | `None` (no change needed) |

**Tool FQID parsing:**

| Input | Tool Type | FQN |
|-------|-----------|-----|
| `"offering:ollama"` | `offering` | `ollama` |
| `"offering:ollama::dev"` | `offering` | `ollama::dev` |
| `"offering:image:nginx:latest"` | `offering` | `image:nginx:latest` |
| `"seed-bank:default"` | `seed-bank` | `default` |

### Integration Tests (cargo test --package moss)

**Persistence round-trip:**
- Write offerings with V2 FQNs to `moss-offerings.json`, reload, verify identical
- Write offerings with V1 FQNs (simulated legacy), reload, verify normalized to V2
- Write offerings with V0 FQNs (`@` separator), reload, verify normalized to V2
- Verify container names in Docker are unchanged after persistence reload

**API handler tests:**
- `POST /api/v1/stone/offerings` with V2 FQN body → offering created with correct name
- `GET /api/v1/stone/services/{fqn}` with URL-encoded V2 FQN → correct service returned
- `POST /api/v1/stone/offerings` with `image:` source → synthetic manifest created, container deployed
- Image collision check: `offer image mongo:7` when curated `mongodb` manifest exists → collision detected
- Compatibility gate: `offer image mongo:7` on incompatible hardware with curated manifest → blocked with guidance

**Topology tests:**
- Stone with V2 offerings broadcasts chirp → topology entry contains `::` format
- Incoming V1 chirp (simulated) → normalized to V2 in receiving stone's topology

### Live Garden Tests (SSH to test stones)

These tests exercise the full deployment pipeline on real hardware with a running garden.

**Pre-upgrade baseline (run on V1 Moss):**

```bash
# Record current state
plink -batch -ssh "stone@stone-coral-prairie" -pw stone "docker ps --format '{{.Names}}'"
plink -batch -ssh "stone@stone-coral-prairie" -pw stone "cat /etc/zen-garden/moss-offerings.json | jq '.[].name'"
```

**Upgrade to V2 Moss:**

```bash
# Deploy V2 binary, restart Moss
plink -batch -ssh "stone@stone-coral-prairie" -pw stone "sudo systemctl restart garden-moss"

# Verify: containers unchanged
plink -batch -ssh "stone@stone-coral-prairie" -pw stone "docker ps --format '{{.Names}}'"
# Expected: identical container names as pre-upgrade

# Verify: persistence migrated
plink -batch -ssh "stone@stone-coral-prairie" -pw stone "cat /etc/zen-garden/moss-offerings.json | jq '.[].name'"
# Expected: all `:` replaced with `::`

# Verify: all offerings healthy
plink -batch -ssh "stone@stone-coral-prairie" -pw stone "curl -s http://localhost:7185/api/v1/stone/services | jq '.[] | {name, status, health}'"
# Expected: all services Running/Healthy, names in V2 format

# Verify: topology visible across garden
garden-rake list --all
# Expected: all offerings on all stones visible with V2 FQN format
```

**Image-direct deployment (on V2 Moss):**

```bash
# Deploy a simple image
garden-rake offer image nginx:latest --at stone-coral-prairie

# Verify: container created with img- prefix
plink -batch -ssh "stone@stone-coral-prairie" -pw stone "docker ps --filter name=zen-offering-img-nginx"
# Expected: zen-offering-img-nginx-latest running

# Verify: appears in catalog
garden-rake list --at stone-coral-prairie
# Expected: image:nginx:latest listed alongside curated offerings

# Verify: topology announcement
garden-rake list --all
# Expected: image:nginx:latest visible from all stones

# Verify: lifecycle operations
garden-rake rest image:nginx:latest --at stone-coral-prairie    # stop
garden-rake wake image:nginx:latest --at stone-coral-prairie    # start
garden-rake logs image:nginx:latest --at stone-coral-prairie    # stream logs

# Verify: collision detection
garden-rake offer image mongo:7 --at stone-coral-prairie
# Expected: prompt about curated mongodb manifest existing

# Cleanup
garden-rake remove image:nginx:latest --at stone-coral-prairie
```

**Named instance (on V2 Moss):**

```bash
# Deploy curated offering with instance
garden-rake plant redis::cache-01 --at stone-coral-prairie

# Verify: container name uses --
plink -batch -ssh "stone@stone-coral-prairie" -pw stone "docker ps --filter name=zen-offering-redis--cache-01"
# Expected: zen-offering-redis--cache-01 running

# Verify: FQN in API
curl -s http://stone-coral-prairie:7185/api/v1/stone/services | jq '.[] | select(.name == "redis::cache-01")'
# Expected: name = "redis::cache-01", offering = "redis"

# Cleanup
garden-rake remove redis::cache-01 --at stone-coral-prairie
```

**Mixed-version garden (rolling upgrade):**

```bash
# Upgrade stone-01 to V2, leave stone-02 on V1
# Verify: stone-01 sees stone-02's offerings (V1 chirps normalized)
# Verify: stone-02 sees stone-01's offerings (V2 chirps are harmless strings to V1)
# Verify: no crashes, no data loss on either side
# Upgrade stone-02 to V2
# Verify: full garden visibility restored with V2 format everywhere
```

**Regression tests (ensure nothing broke):**

```bash
# Standard offering operations still work
garden-rake plant memcached --at stone-coral-prairie
garden-rake offer memcached info --at stone-coral-prairie
garden-rake rest memcached --at stone-coral-prairie
garden-rake wake memcached --at stone-coral-prairie
garden-rake remove memcached --at stone-coral-prairie

# Adoption still works
garden-rake reconcile --at stone-coral-prairie
# Expected: orphaned zen-offering-* containers adopted with V2 FQN

# Search still works
garden-rake offer --search "database"
# Expected: curated offerings found, scored correctly

# Capabilities still work (if offering supports them)
garden-rake capabilities ollama --at stone-coral-prairie
```

### Test Matrix

| Scenario | Unit | Integration | Live Garden |
|----------|------|-------------|-------------|
| FQN parsing (V2 format) | X | | |
| FQN parsing (V1 legacy) | X | | |
| FQN parsing (V0 legacy) | X | | |
| FQN display (`.fqn()`) | X | | |
| Container encoding (curated) | X | | |
| Container encoding (image-direct) | X | | |
| Container decoding round-trip | X | | |
| Tool FQID parsing | X | | |
| Persistence round-trip | | X | |
| Legacy persistence migration | | X | X |
| API handler FQN handling | | X | |
| Image inspection + synthetic manifest | | X | |
| Collision detection | | X | X |
| Compatibility cross-reference | | X | X |
| Zero-downtime upgrade | | | X |
| Container name preservation | | | X |
| Topology broadcast (V2) | | X | X |
| Mixed-version chirp handling | | | X |
| Image-direct full lifecycle | | | X |
| Named instance full lifecycle | | | X |
| Standard offering regression | | | X |
| Adoption with V2 FQN | | | X |
| Search with V2 catalog | | | X |
| Capabilities with V2 FQN | | | X |

---

## References

- [OFFER-0003](OFFER-0003-offering-fqn.md) — superseded FQN format
- [OFFER-0002](OFFER-0002-container-namespace-collision.md) — container naming convention (preserved)
- [OFFER-0005](OFFER-0005-offering-modes.md) — three offering modes (preserved)
- [OFFER-0001](OFFER-0001-taxonomy.md) — taxonomy and search (extended by source schemes)
- [TOOLS-0002](TOOLS-0002-garden-tool-unified-contract.md) — tool FQID format (updated)
- [Offering FQN Spec](../specs/offering-fqn.md) — to be updated
- [Offerings Spec](../specs/offerings.md) — to be updated

---

## Appendix A: Code Path Investigation (2026-03-02)

Concrete walkthrough of critical code paths with V2 inputs. Findings from reading production code, not inference.

### A.1. Constant Type Change: `char` → `&str`

**Location**: `src/common/src/constants/mod.rs:122`

```rust
// V1
pub const OFFERING_FQN_SEPARATOR: char = ':';
// V2
pub const OFFERING_FQN_SEPARATOR: &str = "::";
```

The constant is used in 6 locations. All use `format!()` or `str::split()`, both of which accept `char` and `&str` interchangeably via Rust's `Pattern` trait.

| Call Site | Usage | Compiles? |
|-----------|-------|-----------|
| `offerings.rs:145` | `format!("{}{}{}", ..., SEPARATOR, ...)` | Yes |
| `offerings.rs:187` | `trimmed.split(SEPARATOR)` | Yes |
| `docker.rs:43` | `format!("{}{}{}", ..., SEPARATOR, ...)` | Yes |
| `auto_adoption.rs:307` | `format!("{}{}{}", ..., SEPARATOR, ...)` | Yes |
| `auto_adoption.rs:320` | `format!("{}{}{}", ..., SEPARATOR, ...)` | Yes |
| `adoption.rs:104` | `format!("{}{}{}", ..., SEPARATOR, ...)` | Yes |

**Verdict**: Clean compile. No code changes needed at call sites.

### A.2. Parser Split Behavior Change

**Location**: `src/common/src/offerings.rs:187`

```rust
let mut parts = trimmed.split(crate::constants::OFFERING_FQN_SEPARATOR);
```

Critical behavior difference:

```
"mongodb:prod".split(':')    → ["mongodb", "prod"]       (2 parts)
"mongodb:prod".split("::")   → ["mongodb:prod"]          (1 part — NO split)
"mongodb::prod".split("::")  → ["mongodb", "prod"]       (2 parts)
```

**Implication**: V1 string `"mongodb:prod"` passed through the V2 parser becomes a single segment `"mongodb:prod"`, which then fails `normalize_fqn_segment()` validation because `:` is not in `[a-z0-9_-]`.

**This is CORRECT for API inputs** — V2 Moss should reject V1 format from HTTP requests. Users must use V2 format.

**This requires `normalize_legacy_fqn()` for persistence data** — on-disk V1 strings must be converted before parsing. The migration function handles this (see A.4).

### A.3. Container Encode/Decode Round-Trip (Confirmed Safe)

**Encoding** (`offerings.rs:158-168`): `encoded_for_container()` uses `OFFERING_FQN_CONTAINER_SEPARATOR` (`"--"`), NOT `OFFERING_FQN_SEPARATOR`. It operates on the parsed struct fields (`offering`, `instance`), which are identical between V1 and V2.

```
V1: parse("mongodb:prod")  → { offering: "mongodb", instance: "prod" } → "mongodb--prod"
V2: parse("mongodb::prod") → { offering: "mongodb", instance: "prod" } → "mongodb--prod"
```

**Decoding** (`docker.rs:38-49`): `decode_offering_container_suffix()` splits on `"--"` then reconstructs with `OFFERING_FQN_SEPARATOR`. With V2 constant, decoding `"mongodb--prod"` produces `"mongodb::prod"`. Correct.

**Verified**: Existing containers are fully backward compatible. No Docker operations needed.

### A.4. Persistence Migration — `normalize_legacy_fqn()`

**Location**: `src/moss/src/infra/persistence.rs:110-117`

Current code only handles `@` → `:` migration:

```rust
fn normalize_legacy_fqn(name: &str) -> Option<String> {
    if !name.contains('@') {
        return None;  // ← V1 strings like "mongodb:prod" fall through!
    }
    let candidate = name.replace('@', ":");
    parse_offering_fqn(&candidate).ok().map(|fqn| fqn.fqn())
}
```

**Bug**: V1 strings containing `:` but not `@` are NOT caught. The function returns `None`, and the V1 string remains in persistence.

**Required fix**: Add `:` → `::` rule as specified in the Migration section.

### A.5. `normalize_legacy_type()` — Same Pattern

**Location**: `src/moss/src/infra/persistence.rs:119-126`

```rust
fn normalize_legacy_type(offering_type: &str) -> Option<String> {
    if !(offering_type.contains('@') || offering_type.contains(':')) {
        return None;
    }
    let candidate = offering_type.replace('@', ":");
    parse_offering_fqn(&candidate).ok().map(|fqn| fqn.offering)
}
```

If the offering type somehow contains a `:` (legacy corruption), this calls `parse_offering_fqn("mongodb:something")` which splits on `"::"` → single segment `"mongodb:something"` → validation error.

**Required fix**: Replace single `:` with `::` before parsing, same as `normalize_legacy_fqn()`.

### A.6. MongoDB Orchestrator — HARDCODED COLON (3 Bugs)

**Bug 1**: `src/orchestrators/mongodb/src/domain/types.rs:137`

```rust
pub fn derive_replica_set_name(fqn: &str) -> String {
    match fqn.strip_prefix("mongodb:") {  // ← hardcoded single colon
```

With V2 input `"mongodb::analytics"`:
- `strip_prefix("mongodb:")` → `Some(":analytics")` (matches the first `:`!)
- Result: `"zen-garden-:analytics"` — **produces invalid replica set name with leading colon**

**Required fix**: `fqn.strip_prefix("mongodb::")` — and update test assertions at lines 211, 214, 215.

**Bug 2**: `src/orchestrators/mongodb/src/api/cluster.rs:282`

```rust
Some(suffix) if !suffix.is_empty() => format!("mongodb:{suffix}"),
```

Constructs V1 FQN. **Required fix**: `format!("mongodb::{suffix}")`

**Bug 3**: `src/orchestrators/mongodb/src/tasks/discovery.rs:44`

```rust
fqid_filter: |fqid: &str| {
    fqid == "offering:mongodb" || fqid.starts_with("offering:mongodb:")
},
```

With V2 FQID `"offering:mongodb::analytics"`:
- `starts_with("offering:mongodb:")` → **true** (accidentally, because `"offering:mongodb::"` starts with `"offering:mongodb:"`)

This accidentally works but is fragile. **Should update to**: `fqid.starts_with("offering:mongodb::")`

Also at line 155, comment says `"offering:mongodb:analytics"` — needs updating to V2 format.

### A.7. Tools Domain — `fqid_matches()` Colon Check

**Location**: `src/common/src/tools/types.rs:136`

```rust
if q.contains(':') {
    // Exact instance match
    tool.fqid.eq_ignore_ascii_case(&q)
}
```

With V2 inputs:
- `"mongodb::prod"` → `contains(':')` → true → exact match. **Works.**
- `"mongodb"` → `contains(':')` → false → type match. **Works.**
- `"image:nginx:latest"` → `contains(':')` → true → exact match. **Works.**

**Verdict**: Functionally correct for all V2 inputs, but the comment "Exact instance match" is misleading. A single `:` in V2 means source scheme, not instance. The logic works because both source-qualified and instance-qualified queries need exact FQID matching. **Cosmetic fix only** — update comment.

### A.8. Capability Wishful Search — HARDCODED COLON LOGIC

**Location**: `src/rake/src/commands/discovery/find.rs:429-443`

```rust
if !query.contains(':') && !query.contains('[') {
    return Ok(false);
}
// ...
if !query.contains('[') && query.matches(':').count() == 1 {
    let suffix = query.split(':').nth(1)
```

Parses queries like `"ollama:dev[model1,model2]"`.

With V2 query `"ollama::dev[model1,model2]"`:
- `query.contains(':')` → true. **OK.**
- `query.matches(':').count() == 1` → **false** (2 colons). Fails to enter the instance-extraction branch.

**Required fix**: Change to `query.contains("::")` for instance detection, or update the counting logic to look for `"::"` as a single unit.

### A.9. Topology Chirp Receive — Normalization Gap

**Location**: `src/moss/src/tasks/coordinator.rs:163`

```rust
let chirp: garden_common::TopologyEntry = match serde_json::from_value(payload) {
    Ok(c) => c,
    Err(e) => { continue; }
};
upsert_from_chirp_dirty(&topology_cache, chirp.clone(), &topology_dirty).await;
```

No normalization is applied to `chirp.services[*].name` fields. V1 strings from peer stones are stored as-is.

**Required fix**: Apply `normalize_legacy_fqn()` to each `TopologyServiceEntry.name` before inserting into the topology cache. This enables mixed-version garden operation.

### A.10. Safe Colon References (No Change Needed)

The following colon-based splits are NOT related to FQN and require NO changes:

| Location | Purpose | Safe? |
|----------|---------|-------|
| `sse.rs:176,178` | SSE protocol parsing (`event:`, `data:`) | Yes |
| `client.rs:41` | `host:port` detection | Yes |
| `job_executors.rs:699,1073` | Docker image tag extraction (`image.split(':')`) | Yes |
| `adoption.rs:104` | Docker image tag extraction | Yes |
| `status.rs:210,213` | AI runtime formatting (`cuda:12.2`) | Yes |
| `offering.rs:598` | Docker volume parsing (`host:container`) | Yes |
| `network.rs:83` | MAC address parsing | Yes |
| `system.rs:34,38,651,777,813,849` | System metrics parsing | Yes |
| `system.rs:1628,1649` | Docker image AI runtime detection | Yes |
| `connection.rs:220` | IPv6 address detection | Yes |
| `pulse.rs:761` | IP:port parsing | Yes |
| `route.rs:860-862` | Election type prefix `offering_primary:` | Yes |

### A.11. Summary of Required Code Changes

**Mandatory fixes** (will cause bugs if not addressed):

| # | File | Line | Issue | Fix |
|---|------|------|-------|-----|
| 1 | `constants/mod.rs` | 122 | Separator constant | `char ':'` → `&str "::"` |
| 2 | `offerings.rs` | 192 | Error message | Update colon reference in text |
| 3 | `offerings.rs` | 174 | Doc comment | `[:instance]` → `[::instance]` |
| 4 | `offerings.rs` | 282-296 | Unit tests | Update all V1 test data to V2 |
| 5 | `persistence.rs` | 110-117 | Legacy migration | Add `:` → `::` rule |
| 6 | `persistence.rs` | 119-126 | Legacy type norm | Add `:` → `::` before parse |
| 7 | `orchestrators/mongodb/.../types.rs` | 137 | `derive_replica_set_name` | `"mongodb:"` → `"mongodb::"` |
| 8 | `orchestrators/mongodb/.../types.rs` | 209-215 | Unit tests | Update V1 test data to V2 |
| 9 | `orchestrators/mongodb/.../cluster.rs` | 282 | FQN construction | `"mongodb:{suffix}"` → `"mongodb::{suffix}"` |
| 10 | `orchestrators/mongodb/.../discovery.rs` | 44 | FQID filter | `"offering:mongodb:"` → `"offering:mongodb::"` |
| 11 | `discovery/find.rs` | 441,443 | Capability search | Update colon-counting logic for `::` |
| 12 | `coordinator.rs` | 163+ | Chirp receive | Add FQN normalization for incoming topology |

**Cosmetic fixes** (functional but misleading):

| # | File | Line | Issue |
|---|------|------|-------|
| 13 | `tools/types.rs` | 136 | Comment says "instance match" but `:` now means source scheme |
| 14 | `orchestrators/mongodb/` | multiple | Doc comments with V1 FQN examples |
| 15 | `orchestrators/mongodb/.../discovery.rs` | 155 | Comment with V1 FQID example |

**Auto-propagated** (no code changes, constant handles it):

All 45+ call sites that use `OFFERING_FQN_SEPARATOR` via `format!()` or `parse_offering_fqn()` / `.fqn()` work automatically. See initial scan for complete list.

---

## Appendix B: Architectural Cleanup Opportunities

The code investigation exposed patterns that work today but are fragile, scattered, or rely on implicit knowledge. This refactoring is not about the FQN change — it's about using this change as the forcing function to simplify the architecture. Fewer moving parts, more intent.

### B.1. FQN Should Be a Type, Not a String

**Problem**: Throughout the codebase, FQN is passed as `&str` or `String`. Every consumer must know to call `parse_offering_fqn()` and handle errors. The MongoDB orchestrator bypasses parsing entirely with `fqn.strip_prefix("mongodb:")` — a hardcoded string operation on what should be a typed value.

**Fix**: `OfferingFqn` becomes the single currency for offering identity. No function accepts a raw FQN string; it accepts `&OfferingFqn`. Parsing happens once at the system boundary (API ingress, persistence load, chirp receive). Everything downstream works with the parsed, validated struct.

```rust
// Before: FQN is a string, every consumer must parse
pub fn derive_replica_set_name(fqn: &str) -> String {
    match fqn.strip_prefix("mongodb:") { ... }  // magic string
}

// After: FQN is a type, offering and instance are fields
pub fn derive_replica_set_name(fqn: &OfferingFqn) -> String {
    match &fqn.instance {
        Some(instance) => format!("zen-garden-{instance}"),
        None => "zen-garden".to_string(),
    }
}
```

This eliminates every hardcoded colon reference in the orchestrators. The function doesn't know or care about FQN separator syntax — it works with structured data.

**Scope**: `MongoInstance.fqn`, `PendingAction.fqn`, `FqnGatewayEntry.fqn`, `TopologyServiceEntry.name`, `Offering.name` — all change from `String` to `OfferingFqn` (or serialize via `OfferingFqn`).

### B.2. Tool FQID Should Be a Type, Not a String

**Problem**: Tool FQIDs are `"offering:mongodb::dev"` — a compound string with two different separators (`:` for tool type, `::` for instance). Consumers parse it with `split_once(':')`, `contains(':')`, and `starts_with("offering:mongodb:")`. The MongoDB orchestrator's FQID filter is a closure with hardcoded string matching.

**Fix**: Introduce `ToolFqid` as a parsed struct:

```rust
pub struct ToolFqid {
    pub tool_type: ToolType,  // Offering, SeedBank, etc.
    pub fqn: OfferingFqn,     // parsed, validated
}

impl ToolFqid {
    pub fn parse(raw: &str) -> Result<Self> { ... }
    pub fn matches_offering(&self, offering: &str) -> bool { ... }
}
```

The MongoDB discovery filter becomes:

```rust
// Before: magic string matching
fqid_filter: |fqid: &str| {
    fqid == "offering:mongodb" || fqid.starts_with("offering:mongodb:")
},

// After: typed matching
fqid_filter: |fqid: &ToolFqid| {
    fqid.tool_type == ToolType::Offering && fqid.fqn.offering == "mongodb"
},
```

### B.3. Centralize FQN Construction

**Problem**: FQN strings are constructed in 5+ places with `format!("{}{}{}", name, SEPARATOR, instance)`. The adoption API, auto-adoption task, and persistence all independently construct `"offering::adopted"` FQNs.

**Fix**: `OfferingFqn` gets builder methods:

```rust
impl OfferingFqn {
    pub fn default_instance(offering: &str) -> Self { ... }
    pub fn with_instance(offering: &str, instance: &str) -> Self { ... }
    pub fn adopted(offering: &str) -> Self {
        Self::with_instance(offering, OFFERING_ADOPTED_INSTANCE)
    }
}
```

Every `format!("{}{}{}", ..., SEPARATOR, ...)` becomes a constructor call. The separator constant becomes an implementation detail of `OfferingFqn::fqn()`, not a public API surface.

### B.4. Single Normalization Gateway

**Problem**: FQN normalization (legacy migration) happens in persistence but NOT in topology chirp receive, NOT in API inputs, and NOT in orchestrator discovery. Each path must independently remember to normalize.

**Fix**: Normalization is part of `OfferingFqn::parse()` — the single entry point. If the input is a legacy format, it's normalized during parsing. No separate `normalize_legacy_fqn()` function called from scattered locations.

```rust
impl OfferingFqn {
    pub fn parse(input: &str) -> Result<Self> {
        // 1. Normalize legacy formats (@, single-colon)
        // 2. Parse source scheme
        // 3. Split on ::
        // 4. Validate segments
        // This is the ONLY place FQN strings enter the system
    }
}
```

Chirp receive, persistence load, API ingress — all call the same `OfferingFqn::parse()`. Legacy handling is invisible to callers.

### B.5. Eliminate Capability Query Parsing from Rake

**Problem**: `discovery/find.rs:429-443` manually parses `"ollama::dev[model1,model2]"` by counting colons and splitting on brackets. This is FQN parsing reimplemented in the CLI layer with different logic than the canonical parser.

**Fix**: Capability query format gets its own parser in `garden_common`:

```rust
pub struct CapabilityQuery {
    pub fqn: OfferingFqn,
    pub capabilities: Vec<(String, String)>,  // type:item pairs
}

impl CapabilityQuery {
    pub fn parse(input: &str) -> Result<Self> { ... }
}
```

Rake calls `CapabilityQuery::parse()` instead of doing ad-hoc string surgery. The FQN extraction delegates to `OfferingFqn::parse()`. One parser, one truth.

### B.6. Orchestrator Offering Matching as a Trait

**Problem**: The MongoDB orchestrator matches offerings with `fqid.starts_with("offering:mongodb:")`. The Ollama orchestrator presumably does the same with `"offering:ollama"`. This is a pattern that should be abstracted.

**Fix**: The orchestrator common crate provides:

```rust
pub trait OfferingMatcher {
    /// The canonical offering name this orchestrator manages
    fn offering_name(&self) -> &str;

    /// Whether a tool FQID belongs to this orchestrator
    fn matches(&self, fqid: &ToolFqid) -> bool {
        fqid.tool_type == ToolType::Offering
            && fqid.fqn.offering == self.offering_name()
    }
}
```

Each orchestrator implements `offering_name()` → `"mongodb"` or `"ollama"`. The matching logic lives once in the common crate. No hardcoded string prefixes in orchestrator code.

### B.7. Summary — What This Simplifies

| Before | After |
|--------|-------|
| FQN is a `String`, parsed at every use site | FQN is `OfferingFqn`, parsed once at boundary |
| Tool FQID is a `String`, ad-hoc splitting | Tool FQID is `ToolFqid`, typed fields |
| 5 places construct `format!(...SEPARATOR...)` | `OfferingFqn::adopted()`, `::with_instance()` |
| Normalization in persistence only | Normalization in `parse()`, everywhere for free |
| Capability queries parsed with colon-counting | `CapabilityQuery::parse()` with delegation |
| Orchestrators match with `starts_with()` | `OfferingMatcher` trait |
| `derive_replica_set_name` splits on magic string | Reads `fqn.instance` field |
| Chirp receive stores raw strings | Chirp receive parses to types |

The separator change is the smallest part of this. The real win is: **FQN becomes a type, not a string convention**. Separator syntax becomes an implementation detail of serialization, invisible to all business logic.
