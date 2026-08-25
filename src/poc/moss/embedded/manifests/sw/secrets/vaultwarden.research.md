# Vaultwarden — Research

Research record behind the Vaultwarden offering. Not loaded by Moss. Verified 2026-05-29.

## Overview

| Field | Value |
|-------|-------|
| Official name | Vaultwarden (unofficial Bitwarden-compatible server) |
| Category | `secrets` |
| Primary use | Self-hosted password/secrets vault (Bitwarden clients + extension) |
| License | AGPL-3.0 |
| Project URL | https://github.com/dani-garcia/vaultwarden |
| Container image | `vaultwarden/server` (also `ghcr.io/dani-garcia/vaultwarden`) |
| Runtime | Rust (Rocket); embedded SQLite |

## Docker image analysis

**Tag: `1.36.0`** (latest release as of 2026-05-29; the project ships clean `x.y.z`
semver tags — pin and bump rather than `:latest`, which the wiki discourages).
Multi-arch: amd64, arm64, arm/v7, arm/v6 (Debian + Alpine variants). No special
CPU features.

## Resource requirements

~10–50 MB. A `host.ram.total.mb < 128` warn is the only gate — runs on the
smallest stones.

## Network configuration

| Port | Protocol | Purpose |
|------|----------|---------|
| 80 | HTTP | Web vault + API (`ROCKET_PORT`, default 80) |

Container listens on 80; the host port remaps if 80 is taken (persisted).

## Health check strategy

The official image already defines a `HEALTHCHECK` (`/healthcheck.sh`, which curls
`/alive`), and `curl` is present. The manifest specifies the same probe explicitly:
`curl --fail --silent http://localhost:80/alive` — `/alive` returns 200 as soon as
Rocket is up (no DB seeding needed), so it goes healthy on a bare boot.

## Statefulness & offer→ready (data mandate)

A single `/data` mount externalizes **everything**: `db.sqlite3`, the JWT
`rsa_key.pem`/`rsa_key.pub.pem` (as critical as the DB — losing it invalidates all
sessions), `attachments/`, `sends/`, `icon_cache/`, and `config.json`. Mounted as
the `vaultwarden-data` host volume → snapshot-eligible.

Zero-config boot: no required secret. `SIGNUPS_ALLOWED` defaults true so the first
account can be created; lock it down afterward.

## Offer→ready caveat: HTTPS

The web vault uses the browser Web Crypto API, which requires a secure context —
login/registration crypto, the browser extension, and WebAuthn only work over
**HTTPS**. HTTP works for localhost setup; production use needs a TLS reverse proxy
(traefik/caddy) and `DOMAIN=https://…`. This does not affect the container
healthcheck.

## Environment variables

| Variable | Purpose |
|----------|---------|
| `SIGNUPS_ALLOWED` | Default true; set false after creating your account |
| `DOMAIN` | Required for WebAuthn, Sends, email links |
| `ADMIN_TOKEN` | Optional; enables `/admin` (Argon2 hash) |

## Compatibility rules analysis

| Rule | Condition | Action |
|------|-----------|--------|
| `low-memory-warning` | `host.ram.total.mb < 128` | Warn |

## Validation checklist

- [x] Image official, multi-arch, pinned to a current release (1.36.0)
- [x] Healthcheck verified (`/alive`, curl present)
- [x] License recorded (AGPL-3.0)
- [x] Data externalized: single `/data` volume (DB + RSA keys + attachments)
- [x] Offer→ready: boots healthy zero-config; HTTPS caveat documented

## Files

| File | Status |
|------|--------|
| `vaultwarden.snippet.yaml` | Created |
| `vaultwarden.frontmatter.json` | Created |
| `vaultwarden.compatibility.yaml` | Created |
| `vaultwarden.guidance.md` | Created |
| `vaultwarden.research.md` | Created |

## References

1. https://github.com/dani-garcia/vaultwarden
2. https://github.com/dani-garcia/vaultwarden/wiki/Which-container-image-to-use
3. https://github.com/dani-garcia/vaultwarden/wiki/Changing-persistent-data-location
