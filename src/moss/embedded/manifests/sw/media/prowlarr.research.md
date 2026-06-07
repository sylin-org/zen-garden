# Prowlarr — Research

Research record behind the Prowlarr offering. Not loaded by Moss. Verified 2026-05-29.

## Overview

| Field | Value |
|-------|-------|
| Official name | Prowlarr |
| Category | `media` |
| Primary use | Indexer manager/proxy for the *arr stack (Torrent + Usenet) |
| License | GPL-3.0 |
| Project URL | https://github.com/Prowlarr/Prowlarr |
| Container image | `lscr.io/linuxserver/prowlarr` (LinuxServer — de-facto; no first-party image) |
| Runtime | .NET; embedded SQLite |

Prowlarr centralizes indexer definitions and pushes them to Sonarr/Radarr. It is
the primary consumer of the **flaresolverr** offering for Cloudflare-gated indexers.

## Docker image analysis

**Tag: `:latest`.** LinuxServer publishes a rolling `:latest` plus versioned tags
that carry an `-ls<build>` suffix; `:latest` is their sanctioned default and is
guaranteed to pull (which the offer→ready contract requires). Pin a specific
`lscr.io/linuxserver/prowlarr:<version>` tag for strict reproducibility if needed.

### Architecture support

| Arch | Supported |
|------|-----------|
| linux/amd64 | Yes |
| linux/arm64 | Yes |
| linux/arm/v7 | Yes |

No special CPU features.

## Resource requirements

~80–150 MB typical. A `host.ram.total.mb < 256` warn-only rule is the only gate.

## Network configuration

| Port | Protocol | Purpose |
|------|----------|---------|
| 9696 | HTTP | Web UI + API |

`9696` is uncontended — no `well-known-ports.yaml` entry needed.

## Health check strategy

`GET /ping` is unauthenticated and returns `200 {"status":"OK"}` once the app has
initialized — used as the container healthcheck (via `curl`, present in the
LinuxServer base image). `start_period` is 60s to cover first-run DB migration.

## Statefulness & offer→ready

State lives in `/config` (named volume). The web UI is fully usable on first start
with no configuration (auth setup + indexer/app wiring are the customization step) —
a genuine offer→ready instance.

## Environment variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `PUID` / `PGID` | 1000 | File ownership for `/config` |
| `TZ` | Etc/UTC | Timezone |

## Compatibility rules analysis

| Rule | Condition | Action |
|------|-----------|--------|
| `low-memory-warning` | `host.ram.total.mb < 256` | Warn |

## Validation checklist

- [x] Image exists and is the de-facto standard (LinuxServer)
- [x] Multi-arch (amd64/arm64/armv7)
- [x] Healthcheck verified (`/ping`, unauthenticated)
- [x] License recorded (GPL-3.0)
- [x] Offer→ready: web UI usable on first start, state in `/config`

## Files

| File | Status |
|------|--------|
| `prowlarr.snippet.yaml` | Created |
| `prowlarr.frontmatter.json` | Created |
| `prowlarr.compatibility.yaml` | Created |
| `prowlarr.guidance.md` | Created |
| `prowlarr.research.md` | Created |

## References

1. https://github.com/Prowlarr/Prowlarr
2. https://docs.linuxserver.io/images/docker-prowlarr/
3. https://wiki.servarr.com/prowlarr
4. https://trash-guides.info/Prowlarr/prowlarr-setup-flaresolverr/
