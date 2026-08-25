# ntfy — Research

Research record behind the ntfy offering. Not loaded by Moss. Verified 2026-05-29.

## Overview

| Field | Value |
|-------|-------|
| Official name | ntfy |
| Category | `communication` (new category) |
| Primary use | HTTP pub/sub push notifications to phone/desktop |
| License | Apache-2.0 / GPL-2.0 (dual) |
| Project URL | https://github.com/binwiederhier/ntfy |
| Container image | `binwiederhier/ntfy` (Docker Hub) |
| Runtime | Single static Go binary |

## Docker image analysis

**Tag: `v2.23.0`** (latest release as of 2026-05-29; the project ships clean
`vX.Y.Z` tags — pin and bump). Multi-arch: amd64, arm64, arm/v7. The image
`ENTRYPOINT` is `["ntfy"]`, so the container needs the **`serve`** argument
(`command: ["serve"]`) — without it the container runs the CLI and exits.

## Resource requirements

~20–40 MB. A `host.ram.total.mb < 128` warn is the only gate — runs on the
smallest stones.

## Network configuration

| Port | Protocol | Purpose |
|------|----------|---------|
| 80 | HTTP | Web UI + publish/subscribe API + `/v1/health` |

Container listens on 80 (`NTFY_LISTEN_HTTP` default `:80`); host port remaps if
taken (persisted).

## Health check strategy

No built-in `HEALTHCHECK` and **no `curl`** in the image, but BusyBox `wget` is
present. Probe: `wget -q --tries=1 --spider http://localhost:80/v1/health` —
`/v1/health` is ntfy's dedicated liveness endpoint and returns 200 `{"healthy":true}`
as soon as the server is serving.

## Statefulness & offer→ready (data mandate)

ntfy's defaults are **ephemeral**: the message cache is in-memory and there is no
auth DB until configured. To make a real, persistent instance the manifest points
all three at on-disk paths and externalizes them:

| Env | Path | Volume |
|-----|------|--------|
| `NTFY_CACHE_FILE` | `/var/cache/ntfy/cache.db` | `ntfy-cache` |
| `NTFY_ATTACHMENT_CACHE_DIR` | `/var/cache/ntfy/attachments` | `ntfy-cache` |
| `NTFY_AUTH_FILE` | `/var/lib/ntfy/user.db` | `ntfy-data` |

Both volumes are host-mounted → snapshot-eligible. Without these, cached messages
and users would vanish on recreate.

Zero-config boot: starts open (anyone can read/write any topic) — a working,
demonstrable instance out of the box. Guidance documents the `deny-all` lock-down.

## Environment variables

| Variable | Purpose |
|----------|---------|
| `NTFY_BASE_URL` | Public URL — required for attachments, click actions, mobile push |
| `NTFY_AUTH_DEFAULT_ACCESS` | `read-write` (default) → set `deny-all` to lock down |
| `TZ` | Timezone for scheduled delivery |

## Compatibility rules analysis

| Rule | Condition | Action |
|------|-----------|--------|
| `low-memory-warning` | `host.ram.total.mb < 128` | Warn |

## Validation checklist

- [x] Image official, multi-arch, pinned to a current release (v2.23.0)
- [x] `serve` command required and supplied
- [x] Healthcheck verified (`/v1/health`, BusyBox wget — no curl)
- [x] License recorded (Apache-2.0 / GPL-2.0)
- [x] Data externalized: cache + attachments + auth DB on host volumes
- [x] Offer→ready: boots open + healthy; persistence + lock-down documented

## Files

| File | Status |
|------|--------|
| `category.json` (communication) | Created |
| `ntfy.snippet.yaml` | Created |
| `ntfy.frontmatter.json` | Created |
| `ntfy.compatibility.yaml` | Created |
| `ntfy.guidance.md` | Created |
| `ntfy.research.md` | Created |

## References

1. https://github.com/binwiederhier/ntfy
2. https://docs.ntfy.sh/install/#docker
3. https://docs.ntfy.sh/config/
