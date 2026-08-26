---
version: "1"
trigger: post_install
---
# Sonarr

Sonarr is running on **{{server-name}}** — automated TV series management.

## Open it

```
http://{{server-name}}:{{port}}
```

To make it useful, customize three things:

- **Indexers** — easiest via **Prowlarr** (add Sonarr as an App there; indexers sync automatically).
- **Download client** — Settings → Download Clients → add **qBittorrent** (`http://{{server-name}}:8080`).
- **Root folder** — Settings → Media Management → add the path where your TV library lives (a mounted volume on this stone).
