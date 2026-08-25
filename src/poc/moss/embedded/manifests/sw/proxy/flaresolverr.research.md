# FlareSolverr — Research

Research record behind the FlareSolverr offering. Not loaded by Moss; it documents
why the machine-read manifest files hold the values they do. Verified 2026-05-29.

## Overview

| Field | Value |
|-------|-------|
| Official name | FlareSolverr |
| Category | `proxy` |
| Primary use | Solve Cloudflare / DDoS-Guard challenges on behalf of *arr indexers |
| License | MIT |
| Governance | Community project (FlareSolverr org; maintainer `ilike2burnthing`) |
| Project URL | https://github.com/FlareSolverr/FlareSolverr |
| Container image | ghcr.io/flaresolverr/flaresolverr (mirror: docker.io/flaresolverr/flaresolverr) |
| Runtime | Python 3.11 + headless Chromium (Selenium / undetected-chromedriver) |

FlareSolverr is a stateless HTTP server: callers POST `/v1` with a `request.get`
(or `request.post`) command, FlareSolverr drives a headless browser through the
challenge, and returns the solved cookies/HTML. It is the de-facto Cloudflare
bypass for the *arr indexer stack.

## Docker image analysis

**Selected tag: `ghcr.io/flaresolverr/flaresolverr:v3.5.0`** (released 2026-05-26).
Pin a known-good tag rather than `:latest` — historical regressions (e.g. v3.3.24)
broke the listener on some arches.

Maintenance: the project went dormant for ~11.5 months (last pre-gap release
v3.3.21, 2024-06-26), then revived — v3.3.22 (2025-06-03) through v3.4.6
(2025-11-29) and v3.5.0 (2026-05-26). The repo is **not** archived; ~14k stars.
Cadence is bursty/volunteer-paced, not continuous.

### Architecture support

| Arch | Image published? | Works in practice? |
|------|------------------|--------------------|
| linux/amd64 (x86-64) | Yes | **Yes** — first-class |
| linux/386 | Yes | Yes (not used by Zen Garden) |
| linux/arm64 (aarch64) | Yes | **Unreliable** — see below |
| linux/arm/v7 | Yes | Unreliable |

The published manifest is genuinely multi-arch (verified against the ghcr and
Docker Hub manifest lists), but a multi-year, multi-device pattern of Chromium
failing to launch on ARM single-board hardware is documented upstream:
- #1327 — QNAP ARM64 (Cortex-A57): "Chrome / Chromium version not detected", trace/breakpoint trap
- #1507 / #1509 / #1519 — Raspberry Pi arm64: chromedriver/listener fails to launch (v3.3.24 regression)
- #986 — Pi: "version_main cannot be converted to an integer"
- Pi 5 + Chromium 138 (3.4.0): "session not created: cannot connect to chrome"

**Decision:** gate the offering to x86_64 with a hard deny (`non-x86_64-unsupported`).
The arm64 image exists but cannot be relied upon on the passively-cooled ARM
stones in a typical garden.

## CPU compatibility

No special CPU features required (Python + Chromium; no AVX/SSE gate).

## Resource requirements

| Tier | Memory | Notes |
|------|--------|-------|
| Idle | ~180 MB | container started, no requests served (verified via `docker stats`, #1511) |
| Per request | +100–200 MB | a full headless Chromium is spawned per in-flight request |
| 24h soak | ~1.2 GB | documented growth/leak (#1387); Chromium/zombie processes accumulate |
| Recommended | 2 GB | hard cap applied via `deploy.resources.limits.memory: 2g` |

Mitigations baked into the manifest: a `2g` memory cap and a `nightly-recycle`
task (`action: recycle`) that restarts the container daily to reclaim leaked
memory. The upstream README states plainly: "Web browsers consume a lot of memory."

## Network configuration

| Port | Protocol | Purpose |
|------|----------|---------|
| 8191 | HTTP | API (`POST /v1`), health (`GET /health`) |
| 8192 | HTTP | Optional Prometheus exporter (`PROMETHEUS_ENABLED=true`); not exposed by default |

Not a contended port — no `well-known-ports.yaml` entry needed.

## Health check strategy

`GET /health` returns `200 {"status":"ok"}` and does no browser work — ideal for a
container healthcheck. The base image (`python:3.11-slim`) ships **no curl/wget**,
so the healthcheck uses the bundled Python runtime:
`python -c "import urllib.request; urllib.request.urlopen('http://localhost:8191/health')"`.

## Environment variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `LOG_LEVEL` | info | Logging verbosity |
| `LOG_HTML` | false | Debug: log all proxied HTML |
| `CAPTCHA_SOLVER` | none | Captcha adapter (bundled solvers are non-functional upstream — leave `none`) |
| `TZ` | UTC | Timezone for logs/browser |
| `PROXY_URL` / `PROXY_USERNAME` / `PROXY_PASSWORD` | none | Upstream proxy |
| `DISABLE_MEDIA` | false | Skip images/CSS to reduce memory |
| `HEADLESS` | true | Debug only |
| `TEST_URL` | https://www.google.com | Start-up browser self-test |
| `PORT` / `HOST` | 8191 / 0.0.0.0 | Listener (no need to change under Docker) |
| `PROMETHEUS_ENABLED` / `PROMETHEUS_PORT` | false / 8192 | Metrics exporter |

`manageable_env` exposes `LOG_LEVEL`, `CAPTCHA_SOLVER`, `TZ`, `PROXY_URL`,
`DISABLE_MEDIA` for runtime tuning via the `/env` endpoints.

## Compatibility rules analysis

**Pre-flight:**

| Rule | Condition | Action | Rationale |
|------|-----------|--------|-----------|
| `non-x86_64-unsupported` | `host.architecture IN (aarch64,arm64,armv7l,armv6l)` | Fail | Chromium unreliable on ARM |
| `insufficient-memory` | `host.ram.total.mb < 512` | Fail | Cannot run a headless browser |
| `low-memory-warning` | `host.ram.total.mb < 2048` | Warn | Memory-heavy; prefer 2GB+ |

**Post-install (advisory — no fallback image exists):** scans logs for failed
Chromium launches ("cannot connect to chrome", etc.) and OOM signatures.

## Statefulness

Stateless — no volumes. Sessions and cookies live in memory and are lost on
restart (which is fine; the nightly recycle is harmless).

## Comparison with alternatives

| Project | Maintained | Arch | API-compatible | Notes |
|---------|-----------|------|----------------|-------|
| **FlareSolverr** | Yes (revived) | multi-arch (ARM flaky) | — | Native, tag-aware Prowlarr/Jackett support |
| Byparr | Yes | multi-arch (weak ARM/NAS testing) | Drop-in (port 8191) | Strongest fallback if FlareSolverr regresses |
| yoori/flare-bypasser | Yes | Docker multi-arch (source x64-only) | `/v1` compatible | zendriver-based |
| 21hsmw/FlareSolverr | Stale (early 2025) | — | fork | Bridged the dormancy; superseded |
| Solvearr | Archived 2025-05-15 | amd64+arm64 | spec-compatible | Dead — do not use |

**For Zen Garden:** FlareSolverr is the de-facto choice — it has native, tag-gated
Prowlarr/Jackett integration (via the maintained FlareSolverrSharp .NET library),
the broadest official arch matrix, and is actively released again. Byparr is the
recommended drop-in fallback (same API/port) should FlareSolverr stall.

## Security considerations

| Concern | Mitigation |
|---------|------------|
| Runs Chrome with `--no-sandbox` | Upstream default; container provides the isolation boundary. No extra `cap_add`/`privileged`. |
| Arbitrary URL fetching (SSRF-like) | It is a deliberate fetch proxy; keep it on the `zen-garden` network, reachable only by trusted indexers. |
| Memory exhaustion / DoS | `2g` cap + nightly recycle + the low-memory pre-flight warning. |

## Validation checklist

- [x] Image exists and is official (`ghcr.io/flaresolverr/flaresolverr`)
- [x] Multi-arch verified; ARM limitation documented and gated (hard deny)
- [x] CPU-feature requirements assessed (none)
- [x] Memory floor + cap documented and enforced
- [x] Healthcheck verified (`GET /health`; Python-based, no curl in image)
- [x] License recorded (MIT)
- [x] Stateless (no volumes)

## Files

| File | Status |
|------|--------|
| `flaresolverr.snippet.yaml` | Created |
| `flaresolverr.frontmatter.json` | Created |
| `flaresolverr.compatibility.yaml` | Created |
| `flaresolverr.guidance.md` | Created |
| `flaresolverr.research.md` | Created |

## References

1. https://github.com/FlareSolverr/FlareSolverr — repo, README, releases
2. https://github.com/FlareSolverr/FlareSolverr/pkgs/container/flaresolverr — image manifest (arches)
3. https://github.com/FlareSolverr/FlareSolverr/issues/1509 — arm64 launch regression
4. https://github.com/FlareSolverr/FlareSolverr/issues/1387 — "container is 1.2G RAM after 24h"
5. https://trash-guides.info/Prowlarr/prowlarr-setup-flaresolverr/ — Prowlarr integration
6. https://github.com/ThePhaseless/Byparr — drop-in alternative
