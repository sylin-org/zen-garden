---
audience: [developer, contributor]
doc_type: guide
status: current
last_verified: 2026-05-29
canonical: true
---

# Authoring an Offering

This guide walks through adding a new managed offering end to end, using
**FlareSolverr** (a Cloudflare/DDoS-Guard bypass proxy used by *arr indexers) as
the worked example. By the end you will have a validated offering that Moss can
plant.

> An offering is a set of files sharing a `<name>` stem inside a category folder.
> Only `<name>.snippet.yaml` is required; the rest enrich it. The offering's
> **name and category come from the file path**, not the file contents. See the
> [offerings spec](../specs/offerings.md) for the format reference and the
> [compatibility guide](offering-manifest-compatibility.md) for the rule DSL.

| File | Required | Purpose |
|------|----------|---------|
| `<name>.snippet.yaml` | **yes** | Container definition (Compose service body) |
| `<name>.frontmatter.json` | recommended | Catalog metadata (description, tags, port, connection, coordination, manageable_env) |
| `<name>.compatibility.yaml` | recommended | Pre-flight rules + per-host image fallback |
| `<name>.guidance.md` | optional | Post-install notes on the portrait page |
| `<name>.research.md` | convention | Human research record (not parsed) |

---

## Step 0 — Research the upstream software

Before writing anything, record what the offering needs in
`<name>.research.md`. This file is documentation only (Moss never parses it),
but every value in the machine-read files should trace back to it. Capture:

- **Image & registry** — the canonical image and a pinned tag. FlareSolverr:
  `ghcr.io/flaresolverr/flaresolverr:v3.5.0` (pin a known-good tag, not `latest`).
- **Architecture support** — which platforms the image actually runs on. The
  FlareSolverr manifest lists `linux/amd64`, `linux/arm64`, `linux/arm/v7`,
  `linux/386`, but multiple primary-source reports show Chromium failing to
  launch on arm64 single-board hardware (Raspberry Pi 3/4/5, ARM NAS) across
  releases. Treat amd64 as first-class and arm64 as best-effort.
- **CPU features** — none required (Python + Chromium).
- **Memory** — ~180 MB idle, but it spawns a full headless Chromium per request
  and can climb toward ~1.2 GB over a day, with documented leak/zombie-browser
  behaviour. The upstream README states plainly: "Web browsers consume a lot of
  memory."
- **Ports** — `8191` (HTTP, `POST /v1`). Optional Prometheus exporter on `8192`.
- **Health** — `GET /health` returns `200 {"status":"ok"}`.
- **State** — stateless: no volumes; sessions/cookies live in memory and are
  lost on restart.
- **Environment** — `LOG_LEVEL`, `LOG_HTML`, `CAPTCHA_SOLVER`, `TZ`, `PROXY_URL`,
  `DISABLE_MEDIA`, `TEST_URL`, `HEADLESS`, `PROMETHEUS_ENABLED`, …
- **License & ecosystem** — record the upstream license; note consumers
  (Prowlarr/Jackett point at it as an indexer proxy).
- **Alternatives** — e.g. Byparr is a same-port (`8191`) drop-in if FlareSolverr
  regresses.

Use a shipped research file such as
[`sw/networking/pihole.research.md`](../../src/moss/embedded/manifests/sw/networking/pihole.research.md)
as the section template (Overview, Image Analysis, Architecture matrix, Resource
Requirements, Health, Environment, Compatibility analysis, Alternatives,
Validation checklist).

---

## Step 1 — Pick the category

Categories are folders under `src/moss/embedded/manifests/sw/`, each with a
`category.json`. Existing categories: `ai`, `auth`, `automation`, `cache`,
`dashboard`, `data`, `devops`, `messaging`, `networking`, `observability`,
`proxy`, `search`, `secrets`, `storage`, `timeseries`, `vector`.

FlareSolverr is an HTTP request proxy, so it belongs in **`proxy`** (alongside
`traefik`). Add a new category folder only when none fits.

---

## Step 2 — Scaffold the files

Use the `manifest` toolchain instead of hand-writing from scratch. `manifest
init` inspects the image on a stone and generates all four files:

```bash
garden-rake manifest init ghcr.io/flaresolverr/flaresolverr:v3.5.0 \
  --name flaresolverr --category proxy --output ./flaresolverr --at stone-01
```

This writes `flaresolverr.snippet.yaml`, `flaresolverr.frontmatter.json`,
`flaresolverr.compatibility.yaml`, and `flaresolverr.guidance.md` into
`./flaresolverr`. Edit them as below.

---

## Step 3 — Write the snippet

`flaresolverr.snippet.yaml` is a Compose service body. Moss parses `image`,
`ports`, `environment`, `volumes`, `command`, `config_files`, `tasks`,
`network`, and `deploy.resources.reservations.devices`. Other Compose keys
(`container_name`, `healthcheck`, `restart`, `networks`) are accepted but
ignored — keep them for readability and Compose parity.

```yaml
# flaresolverr.snippet.yaml
container_name: flaresolverr
image: ghcr.io/flaresolverr/flaresolverr:v3.5.0
ports:
  default: [8191, 8191]
environment:
  LOG_LEVEL: "${FLARESOLVERR_LOG_LEVEL:-info}"
  LOG_HTML: "false"
  CAPTCHA_SOLVER: "none"
  TZ: "${TZ:-UTC}"
healthcheck:
  test: ["CMD", "curl", "-fsS", "http://localhost:8191/health"]
  interval: 30s
  timeout: 10s
  retries: 5
  start_period: 30s
deploy:
  resources:
    limits:
      memory: "2g"        # hard cap — Chromium is memory-heavy and grows over time
tasks:
  nightly-recycle:
    description: "Restart nightly to reclaim leaked browser memory"
    schedule: "0 4 * * *"
    action: recycle
    category: maintenance
restart: unless-stopped
networks: [zen-garden]
```

Notes specific to FlareSolverr:

- **No volumes** — it is stateless.
- **No `shm_size`, `cap_add`, or `security_opt`** — the image already launches
  Chrome with `--disable-dev-shm-usage` and `--no-sandbox`.
- **`deploy.resources.limits.memory`** is FlareSolverr's key safeguard: Moss maps
  it to the container's memory limit. Without a cap it spawns a Chromium per
  request and trends toward ~1 GB+ over a day.
- **The `nightly-recycle` task** (`action: recycle`) restarts the container on a
  cron schedule to reclaim leaked memory — a Moss-level restart, not an
  in-container command.

---

## Step 4 — Write the frontmatter

`flaresolverr.frontmatter.json` supplies catalog metadata. `name`/`category` here
are informational (the loader uses the file path).

```json
{
  "name": "flaresolverr",
  "description": "Proxy server that solves Cloudflare/DDoS-Guard challenges for *arr indexers",
  "category": "proxy",
  "tags": ["proxy", "cloudflare", "scraping", "indexer", "arr"],
  "port": 8191,
  "homepage": "https://github.com/FlareSolverr/FlareSolverr",
  "connection": { "protocol": "http", "uri_template": "http://{host}:{port}" },
  "coordination": "independent",
  "manageable_env": {
    "service_name": "flaresolverr",
    "restart_required": true,
    "vars": ["LOG_LEVEL", "CAPTCHA_SOLVER", "TZ", "PROXY_URL", "DISABLE_MEDIA"]
  }
}
```

- `connection.uri_template` uses `{host}`/`{port}` placeholders filled at connect
  time — clients reach the API at `http://{host}:8191/v1`.
- `coordination: independent` — FlareSolverr holds no shared state; each instance
  is autonomous (no Primary/Replica election).
- `manageable_env` lists the env vars Moss may read/write via the `/env`
  endpoints after deployment.

---

## Step 5 — Write the compatibility rules

FlareSolverr's image is multi-arch, but arm64 is unreliable on small SBCs and it
is memory-hungry. Encode that honestly with the `when:` predicate DSL (full
reference in the [compatibility guide](offering-manifest-compatibility.md)):

```yaml
# flaresolverr.compatibility.yaml
version: "1"
compatibility_rules:
  - name: "insufficient-memory"
    when:
      - host.ram.total.mb < 512
    reason: "FlareSolverr launches a headless Chromium per request and needs headroom"
    suggestion: "Use a stone with at least 2GB RAM, or choose a lighter solver"

  - name: "memory-pressure-warning"
    when:
      - host.ram.total.mb < 2048
    reason: "Chromium is memory-heavy and can grow toward ~1GB+ under load"
    suggestion: "Co-tenant carefully; restart periodically to reclaim leaked browser memory"
    warn_only: true

  - name: "arm-sbc-warning"
    when:
      - host.architecture IN (aarch64,arm64,armv7l)
    reason: "The arm64 image is published but Chromium launch is unreliable on ARM single-board hardware"
    suggestion: "Prefer an x86_64 stone, or pin a known-good tag and verify the browser launches"
    warn_only: true
```

`garden-rake manifest validate` parses every `when:` predicate, so a typo'd fact
or operator is caught at authoring time (COMPAT003) rather than silently dropped
at deploy.

> The `post_install_healthcheck` log-scan block catches runtime crash patterns
> after deployment and can trigger a fallback image — see the
> [compatibility guide](offering-manifest-compatibility.md#post-install-healthcheck).

---

## Step 6 — Write the guidance

`flaresolverr.guidance.md` shows post-install notes on the stone portrait page.
Use the supported markdown subset and template variables (see
[guidance-authoring](guidance-authoring.md)); keep it actionable and minimal.

```markdown
---
version: "1"
trigger: post_install
---
# FlareSolverr

FlareSolverr is running on **{{server-name}}**. Point your indexer manager at it.

## Prowlarr / Jackett

Add an indexer proxy of type **FlareSolverr** with this URL:

```
http://{{server-name}}:{{port}}
```

## Test it

```
curl -L -X POST "http://{{server-name}}:{{port}}/v1" \
  -H "Content-Type: application/json" \
  -d '{"cmd":"request.get","url":"https://www.google.com","maxTimeout":60000}'
```

Sessions are in-memory only — a restart clears all cookies.
```

---

## Step 7 — Validate

```bash
garden-rake manifest validate ./flaresolverr
```

Validation reports findings by code and severity. **Errors** block loading;
**warnings/info** are advisory:

- Snippet: `SCHEMA001/002` (image), `SCHEMA003` (no ports), `SEC001-005`
  (privileged, host network, sensitive mounts, port 0, duplicate ports)
- Frontmatter: `FM001` (JSON), `FM002` (name), `FM003` (description),
  `FM004` (port range)
- Compatibility: `COMPAT001` (YAML), `COMPAT003` (invalid `when:` predicate)
- Cross-file: `FM005` (unknown category), `FM006` (frontmatter/snippet port mismatch), `FM007` (unknown frontmatter key)

Fix every error before continuing (warnings are advisory).

---

## Step 8 — Test on a stone

```bash
garden-rake manifest test ./flaresolverr --at stone-01
# ... verify it deploys and /health returns 200 ...
garden-rake remove flaresolverr --at stone-01
```

---

## Step 9 — Ship it

**Built-in offering (embedded):** place the files under
`src/moss/embedded/manifests/sw/proxy/` and rebuild Moss — manifests are embedded
at compile time:

```bash
cargo check --all
cargo build --package moss
```

**Runtime-only offering (no rebuild):** drop the files under
`{data_dir}/manifests/sw/proxy/` on the stone and reload the index:

```bash
garden-rake offer refresh --at stone-01
```

The filesystem overlay overrides an embedded offering of the same name; fields
absent from the overlay are back-filled from the embedded copy.

`8191` is not a commonly contested port, so no `well-known-ports.yaml` entry is
needed — Moss binds it as-is and only remaps on a live conflict.

---

## Checklist

- [ ] `<name>.research.md` records image, pinned tag, arch matrix, RAM, ports, health, env, license, alternatives
- [ ] `<name>.snippet.yaml` has a pinned `image` and a `ports.default`
- [ ] `<name>.frontmatter.json` has `description`, `tags`, `port`, `connection`; `coordination` set if stateful
- [ ] `<name>.compatibility.yaml` gates RAM and unsupported architectures (deny or `warn_only`)
- [ ] `<name>.guidance.md` is actionable, uses only supported markdown, and has one `#` title
- [ ] `garden-rake manifest validate` passes with no errors
- [ ] `garden-rake manifest test` deploys and the health check passes on a target stone

---

## See also

- [Offerings spec](../specs/offerings.md) — format and validation reference
- [Manifest compatibility & ports](offering-manifest-compatibility.md)
- [Guidance authoring](guidance-authoring.md)
- [Offering lifecycle](offering-lifecycle.md) — plant / wake / rest / upgrade / remove
- [manifests/README.md](../../src/moss/embedded/manifests/README.md)
