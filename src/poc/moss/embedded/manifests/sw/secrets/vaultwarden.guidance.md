---
version: "1"
trigger: post_install
---
# Vaultwarden

Vaultwarden — a Bitwarden-compatible vault — is running on **{{server-name}}**.

## Open it

```
http://{{server-name}}:{{port}}
```

Create your account on first visit, then **lock down sign-ups** so no one else can
register: set `SIGNUPS_ALLOWED=false` in the service's environment.

## HTTPS is required for real use

Browsers only enable the vault's crypto (login, the browser extension, WebAuthn)
over **HTTPS**. Plain HTTP works for first-time setup on localhost, but for real
use put Vaultwarden behind a TLS reverse proxy (the **traefik** or **caddy**
offering) and set `DOMAIN=https://your-host`.

## Admin panel (optional)

Set `ADMIN_TOKEN` to an Argon2 hash to enable the `/admin` page.

All data — including the RSA signing keys (losing them logs everyone out) — lives
on the `vaultwarden-data` volume and is captured by snapshots.
