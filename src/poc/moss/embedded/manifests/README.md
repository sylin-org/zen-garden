# Zen Garden Service Manifests

Managed-offering definitions for the `garden-rake offer` command. Every offering
here is compiled into the Moss binary at build time (`rust_embed`) and can be
overridden at runtime by an identically-named file under
`{data_dir}/manifests/sw/<category>/` (the filesystem copy wins; missing fields
are back-filled from the embedded copy).

## Directory layout

```
manifests/
├── sw/<category>/                  software offerings, grouped by category
│   ├── <name>.snippet.yaml         REQUIRED: container definition
│   ├── <name>.frontmatter.json     catalog metadata
│   ├── <name>.compatibility.yaml   pre-flight rules + per-host image fallback
│   ├── <name>.guidance.md          post-install notes (portrait page)
│   └── <name>.research.md          research record (NOT loaded — documentation only)
├── sw/<category>/category.json     category definition (validated by category.schema.json)
├── hw/<vendor>/                    hardware profiles
└── well-known-ports.yaml           port-conflict remediation catalog
```

Categories (folders under `sw/`): `ai`, `auth`, `automation`, `cache`,
`dashboard`, `data`, `devops`, `messaging`, `networking`, `observability`,
`proxy`, `search`, `secrets`, `storage`, `timeseries`, `vector`. Add a new
category folder (with its own `category.json`) only when no existing one fits.
Browse the live catalog with `garden-rake template list`.

## The offering file set

An offering is a set of files sharing the `<name>` stem inside a
`sw/<category>/` folder. **The offering's name and category come from the file
path, not the file contents** — fields like `name`/`category` inside the JSON
are informational and ignored by the loader.

| File | Required | Purpose |
|------|----------|---------|
| `<name>.snippet.yaml` | **yes** | Docker-Compose-style service body. Only this file creates a managed offering; the rest are optional companions paired by stem. |
| `<name>.frontmatter.json` | recommended | Catalog metadata: `description`, `tags`, `port`, `icon`, `homepage`, `documentation`, `connection`, `coordination`, `manageable_env`, `ceremony`, `minimum_memory_gb`. |
| `<name>.compatibility.yaml` | recommended | Pre-flight `when:` rules and per-host image `fallback`. See the [compatibility guide](../../../../docs/guides/offering-manifest-compatibility.md). |
| `<name>.guidance.md` | optional | Post-install notes shown on the stone portrait page. See [guidance-authoring](../../../../docs/guides/guidance-authoring.md). |
| `<name>.research.md` | convention | Human research record (image/arch/RAM/sources). Not parsed. |

### Snippet fields consumed by Moss

`<name>.snippet.yaml` is a Compose service body. Moss parses the keys below;
other Compose keys (`container_name`, `restart`, `networks`) are accepted but ignored.

- `image` — required; pin a tag (`mongo:7`), never rely on `latest`
- `ports` — map of role → `[host, container]`; the `default` role is the primary port
- `environment` — a map or a `K=V` list; use `${VAR:-default}` for overridable secrets
- `volumes` — `name:/container/path`; a bare `name` is namespaced under the offering's volume dir, absolute host paths pass through
- `command` — a string or a list
- `config_files` — inject/patch a config file, then `restart` or signal the container
- `tasks` — scheduled maintenance commands (cron); `action: recycle` restarts the container
- `healthcheck` — Docker healthcheck (`test` / `interval` / `timeout` / `retries` / `start_period`)
- `network.static_ip` — static-IP preference + reason
- `deploy.resources.reservations.devices` — GPU passthrough
- `deploy.resources.limits` — `memory` (e.g. `2g`) and `cpus` (e.g. `1.5`) caps

## Authoring a new offering

Use the `manifest` toolchain (OFFER-0006) instead of hand-writing files:

```bash
# 1. Scaffold all four files from a Docker image (inspects the image on a stone)
garden-rake manifest init <image-ref> --name <name> --category <category> --output <dir> --at <stone>

# 2. Edit the generated files, then validate offline
garden-rake manifest validate <dir>

# 3. Test-deploy on a stone, then clean up
garden-rake manifest test <dir> --at <stone>
garden-rake remove <name>
```

Ship the offering by placing the files under
`src/moss/embedded/manifests/sw/<category>/` and rebuilding Moss (they embed at
compile time). For a runtime-only offering, drop them under
`{data_dir}/manifests/sw/<category>/` and run `garden-rake refresh`.

Related: `garden-rake manifest export <offering> --at <stone>` (export a running
offering's files), `garden-rake manifest enrich <dir>` (add compatibility/guidance
templates to an existing manifest). Full step-by-step:
[authoring an offering](../../../../docs/guides/authoring-an-offering.md).

## Lifecycle & browse commands

```bash
garden-rake offer <name>          # plant (deploy)
garden-rake rest <name>           # stop, keep data
garden-rake wake <name>           # restart a rested offering
garden-rake upgrade <name>        # pull a newer image and recreate
garden-rake remove <name>         # remove (named volumes preserved)
garden-rake offer refresh --at <stone>   # reload manifests after editing the overlay
garden-rake template list         # browse available offerings
garden-rake template show <name>  # show one offering's resolved manifest
```

## Environment variables

Secrets use the `${VARIABLE:-default}` pattern so they can be overridden at plant time:

```bash
export MONGO_PASSWORD=secure123
garden-rake offer mongodb
```

Default credentials are development-only — override every password in production.

## See also

- [Authoring an offering](../../../../docs/guides/authoring-an-offering.md) — full walkthrough
- [Manifest compatibility & ports](../../../../docs/guides/offering-manifest-compatibility.md)
- [Guidance authoring](../../../../docs/guides/guidance-authoring.md)
- [Offerings reference](../../../../docs/reference/offerings.md)
