# Jellyfin — Research

Research record behind the Jellyfin offering. Not loaded by Moss. Verified 2026-05-29.

## Overview

| Field | Value |
|-------|-------|
| Official name | Jellyfin |
| Category | `media` |
| Primary use | Media server — stream movies/TV/music with transcoding |
| License | GPL-2.0 |
| Project URL | https://github.com/jellyfin/jellyfin |
| Container image | `lscr.io/linuxserver/jellyfin` (also official `jellyfin/jellyfin`) |
| Runtime | .NET; embedded SQLite library DB |

## Docker image analysis

**Tag: `:latest`** (LinuxServer rolling default; guaranteed to pull). Multi-arch:
amd64, arm64, arm/v7. No special CPU features for software transcoding.

## Resource requirements

~150–300 MB idle. Software transcoding spikes CPU and RAM substantially; a
`host.ram.total.mb < 512` warn covers small stones. Direct-play (no transcode)
is the lightest path.

## Hardware transcoding (schema gap)

HW acceleration (Intel/AMD `/dev/dri`, NVIDIA NVENC) requires a device-file
passthrough (`--device /dev/dri`) that the snippet schema does **not** express
(only `deploy.resources.reservations.devices` for the NVIDIA GPU *capability*).
The offering therefore ships CPU/software transcoding, which works on any stone.
A future schema addition for host-device passthrough would unlock HW transcode.

## Network configuration

| Port | Protocol | Purpose |
|------|----------|---------|
| 8096 | HTTP | Web UI + API (DLNA/discovery ports optional, not published) |

Uncontended port — no catalog entry.

## Health check strategy

`GET /health` returns 200 (`Healthy`) — used as the container healthcheck via
`curl` (present in the LinuxServer base image).

## Statefulness & offer→ready (data mandate)

- `/config` — settings + the SQLite library DB → externalized to the
  `jellyfin-config` host volume (snapshot-eligible).
- `/cache` — regenerable transcode cache → externalized to `jellyfin-cache`.
  Note: snapshots currently archive all volumes, so the transient cache is
  captured too; an "exclude-from-snapshot" volume annotation (anticipated by
  ORCH-0039's deferred `seedable: false`) would avoid backing up cache.
- **Media libraries** are user-mounted host paths (the customize-later step).

The web UI runs a first-launch setup wizard (admin account + libraries) — a
working media server on first plant.

## Environment variables

`PUID`/`PGID` (1000), `TZ` (Etc/UTC).

## Compatibility rules analysis

| Rule | Condition | Action |
|------|-----------|--------|
| `low-memory-warning` | `host.ram.total.mb < 512` | Warn |

## Validation checklist

- [x] Image is the de-facto standard (LinuxServer), multi-arch
- [x] Healthcheck verified (`/health`)
- [x] License recorded (GPL-2.0)
- [x] Data externalized: `/config` + `/cache` on host volumes
- [x] Offer→ready: setup wizard on first start; CPU transcode works everywhere

## Files

| File | Status |
|------|--------|
| `jellyfin.snippet.yaml` | Created |
| `jellyfin.frontmatter.json` | Created |
| `jellyfin.compatibility.yaml` | Created |
| `jellyfin.guidance.md` | Created |
| `jellyfin.research.md` | Created |

## References

1. https://github.com/jellyfin/jellyfin
2. https://docs.linuxserver.io/images/docker-jellyfin/
3. https://jellyfin.org/docs/
