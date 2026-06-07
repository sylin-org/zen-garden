# qBittorrent — Research

Research record behind the qBittorrent offering. Not loaded by Moss. Verified 2026-05-29.

## Overview

| Field | Value |
|-------|-------|
| Official name | qBittorrent (qbittorrent-nox, headless) |
| Category | `media` |
| Primary use | BitTorrent download client with a Web UI |
| License | GPL-2.0 |
| Project URL | https://github.com/qbittorrent/qBittorrent |
| Container image | `lscr.io/linuxserver/qbittorrent` (LinuxServer) |
| Runtime | C++/Qt + libtorrent |

## Docker image analysis

**Tag: `:latest`** (LinuxServer rolling default; guaranteed to pull). Multi-arch:
amd64, arm64, arm/v7. No special CPU features.

## Resource requirements

~150–400 MB idle; can grow under load (libtorrent disk cache, many active
torrents). Capped at **2 GB** via `deploy.resources.limits.memory` (OFFER-0009),
with a `host.ram.total.mb < 512` warn.

## Network configuration

| Port | Protocol | Purpose |
|------|----------|---------|
| 8080 | HTTP | Web UI (`WEBUI_PORT`) |
| 6881 | TCP | Torrent traffic (BitTorrent also uses UDP 6881) |

`8080` is uncontended in the catalog; if it collides the port system remaps it.
**Note:** the snippet `ports` map expresses TCP only, so torrent **UDP** 6881 is
not published — TCP peering still works; full DHT/UDP would need a UDP port field
in the schema (future enhancement).

## Health check strategy

`GET /` returns 200 (the Web UI) once up — used as the healthcheck via `curl`.

## Statefulness & offer→ready

State in `/config`; downloads in `/downloads` (named volume). The Web UI is live
on first start. **First-run auth:** a temporary `admin` password is printed to the
container log on startup; the user retrieves it (`garden-rake logs qbittorrent`),
logs in, and sets permanent credentials — the "basic but working, customize later"
model. Host-header validation may need disabling when accessed via a remapped/
proxied port (documented in guidance).

## Environment variables

`PUID`/`PGID` (1000), `TZ` (Etc/UTC), `WEBUI_PORT` (8080), `TORRENTING_PORT` (6881).

## Compatibility rules analysis

| Rule | Condition | Action |
|------|-----------|--------|
| `low-memory-warning` | `host.ram.total.mb < 512` | Warn |

## Validation checklist

- [x] Image is the de-facto standard (LinuxServer), multi-arch
- [x] Healthcheck verified (Web UI `/`)
- [x] License recorded (GPL-2.0)
- [x] Memory cap applied (2g)
- [x] Offer→ready: Web UI live on first start; temp-password retrieval documented

## Files

| File | Status |
|------|--------|
| `qbittorrent.snippet.yaml` | Created |
| `qbittorrent.frontmatter.json` | Created |
| `qbittorrent.compatibility.yaml` | Created |
| `qbittorrent.guidance.md` | Created |
| `qbittorrent.research.md` | Created |

## References

1. https://github.com/qbittorrent/qBittorrent
2. https://docs.linuxserver.io/images/docker-qbittorrent/
