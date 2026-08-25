---
version: "1"
trigger: post_install
---
# Radarr

Radarr is running on **{{server-name}}** — automated movie management.

## Open it

```
http://{{server-name}}:{{port}}
```

To make it useful, customize three things:

- **Indexers** — easiest via **Prowlarr** (add Radarr as an App there; indexers sync automatically).
- **Download client** — Settings → Download Clients → add **qBittorrent** (`http://{{server-name}}:8080`).
- **Root folder** — Settings → Media Management → add the path where your movie library lives (a mounted volume on this stone).
