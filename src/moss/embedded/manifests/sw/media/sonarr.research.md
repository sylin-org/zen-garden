# Sonarr — Research

Research record behind the Sonarr offering. Not loaded by Moss. Verified 2026-05-29.

## Overview

| Field | Value |
|-------|-------|
| Official name | Sonarr |
| Category | `media` |
| Primary use | TV series PVR — search, grab, rename, organize |
| License | GPL-3.0 |
| Project URL | https://github.com/Sonarr/Sonarr |
| Container image | `lscr.io/linuxserver/sonarr` (LinuxServer — de-facto) |
| Runtime | .NET; embedded SQLite |

## Docker image analysis

**Tag: `:latest`** (LinuxServer rolling default; guaranteed to pull — versioned
tags carry `-ls<build>` suffixes that aren't stable references). Multi-arch:
amd64, arm64, arm/v7. No special CPU features.

## Resource requirements

~120–200 MB typical; warn-only below 256 MB.

## Network configuration

| Port | Protocol | Purpose |
|------|----------|---------|
| 8989 | HTTP | Web UI + API |

Uncontended port — no catalog entry needed.

## Health check strategy

`GET /ping` (unauthenticated, 200) via `curl`; `start_period` 60s for first-run
DB migration.

## Statefulness & offer→ready

State in `/config` (named volume). Web UI usable immediately; the customization
step is wiring a download client (qBittorrent), indexers (via Prowlarr), and a
root folder for the TV library.

## Environment variables

`PUID`/`PGID` (1000), `TZ` (Etc/UTC).

## Compatibility rules analysis

| Rule | Condition | Action |
|------|-----------|--------|
| `low-memory-warning` | `host.ram.total.mb < 256` | Warn |

## Validation checklist

- [x] Image is the de-facto standard (LinuxServer), multi-arch
- [x] Healthcheck verified (`/ping`)
- [x] License recorded (GPL-3.0)
- [x] Offer→ready: web UI usable on first start; state in `/config`

## Files

| File | Status |
|------|--------|
| `sonarr.snippet.yaml` | Created |
| `sonarr.frontmatter.json` | Created |
| `sonarr.compatibility.yaml` | Created |
| `sonarr.guidance.md` | Created |
| `sonarr.research.md` | Created |

## References

1. https://github.com/Sonarr/Sonarr
2. https://docs.linuxserver.io/images/docker-sonarr/
3. https://wiki.servarr.com/sonarr
