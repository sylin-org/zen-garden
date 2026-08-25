# Uptime Kuma — Research

Research record behind the Uptime Kuma offering. Not loaded by Moss. Verified 2026-05-29.

## Overview

| Field | Value |
|-------|-------|
| Official name | Uptime Kuma |
| Category | `observability` |
| Primary use | Self-hosted uptime/status monitor with alerting and status pages |
| License | MIT |
| Project URL | https://github.com/louislam/uptime-kuma |
| Container image | `louislam/uptime-kuma` (Docker Hub) |
| Runtime | Node.js + SQLite (full image bundles Chromium) |

## Docker image analysis

**Tag: `2`** (floating major — the project ships `2`, `2.x.y`, and `:latest`; the
`2` major tag is the recommended stable channel and avoids surprise major bumps
while still receiving patch/minor updates). Multi-arch: amd64, arm64, arm/v7.
A `:2-slim` variant drops Chromium for low-memory hosts.

No special CPU features.

## Resource requirements

Idle ~100–150 MB; the full image carries Chromium for real-browser monitors.
A `host.ram.total.mb < 512` warn is informational — HTTP/TCP/ping monitors run
fine well below that; the `:2-slim` image is the path for the smallest stones.

## Network configuration

| Port | Protocol | Purpose |
|------|----------|---------|
| 3001 | HTTP | Web UI + API (`UPTIME_KUMA_PORT`, default 3001) |

Container listens on 3001; host port remaps if taken (persisted).

## Health check strategy

**Inherit the image's built-in HEALTHCHECK — do not override.** The image defines
its own `extra/healthcheck` that requests `GET /` and treats a **302** as healthy
(the root redirects to `/dashboard` or `/setup`), with a 180s start period. A naive
`curl --fail http://localhost:3001/` would treat the 302 as success only with
`-L`, and a `/health`-style probe doesn't exist — so the safest correct behavior is
to omit the manifest healthcheck block and let Docker use the image's own probe.
The offer→ready "working instance" still has a real readiness probe; it just comes
from the image rather than the snippet.

## Statefulness & offer→ready (data mandate)

Critical: the image declares **no `VOLUME`**, so `/app/data` MUST be mounted or the
SQLite DB, settings, admin account, and all monitor history are lost on every
recreate. Mounted as the `uptime-kuma-data` host volume → snapshot-eligible.

Zero-config boot: no env required. First visit is an interactive admin-account
setup (no temp password to scrape from logs) — appropriate for a UI-first tool.

## Environment variables

| Variable | Purpose |
|----------|---------|
| `UPTIME_KUMA_PORT` | Listen port (default 3001) |
| `TZ` | Timezone for schedules and timestamps |

## Compatibility rules analysis

| Rule | Condition | Action |
|------|-----------|--------|
| `low-memory-warning` | `host.ram.total.mb < 512` | Warn (suggest `:2-slim`) |

## Validation checklist

- [x] Image official, multi-arch, pinned to the stable `2` major channel
- [x] Healthcheck strategy verified (inherit image's 302-aware built-in)
- [x] License recorded (MIT)
- [x] Data externalized: `/app/data` mount mandatory (image has no VOLUME)
- [x] Offer→ready: boots to interactive admin setup; persistence guaranteed

## Files

| File | Status |
|------|--------|
| `uptime-kuma.snippet.yaml` | Created |
| `uptime-kuma.frontmatter.json` | Created |
| `uptime-kuma.compatibility.yaml` | Created |
| `uptime-kuma.guidance.md` | Created |
| `uptime-kuma.research.md` | Created |

## References

1. https://github.com/louislam/uptime-kuma
2. https://github.com/louislam/uptime-kuma/wiki/%F0%9F%94%A7-How-to-Install
3. https://github.com/louislam/uptime-kuma/blob/master/extra/healthcheck.go
