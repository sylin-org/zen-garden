---
version: "1"
trigger: post_install
---
# qBittorrent

qBittorrent is running on **{{server-name}}** with its Web UI ready.

## Open it

```
http://{{server-name}}:{{port}}
```

## First login

On first start a **temporary password for the `admin` user is printed to the
container log**. Retrieve it:

```
garden-rake logs qbittorrent
```

Log in, then set a permanent username and password under Options → Web UI —
otherwise a new temporary password is generated on every restart.

## If the Web UI rejects login

When reached on a remapped or proxied port, qBittorrent's host-header validation
can block the request. Turn off Options → Web UI → "Enable Host header validation",
or access it on its mapped port directly.

## Use with Sonarr / Radarr

Add this client in Sonarr/Radarr (Settings → Download Clients) at
`http://{{server-name}}:{{port}}`, and set a download path under a mounted
library volume.
