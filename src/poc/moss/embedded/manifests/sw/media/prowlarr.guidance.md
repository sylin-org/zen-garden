---
version: "1"
trigger: post_install
---
# Prowlarr

Prowlarr is running on **{{server-name}}** — your single place to manage indexers
for the *arr apps.

## Open it

```
http://{{server-name}}:{{port}}
```

On first launch, set up authentication under Settings → General. Then add your
indexers, and add **Sonarr/Radarr** as Apps (Settings → Apps) so Prowlarr syncs
indexers to them automatically.

## Cloudflare-gated indexers

If you've planted **flaresolverr**, wire it in under Settings → Indexers → Add
Indexer Proxy → FlareSolverr:

```
http://{{server-name}}:8191
```

Then tag the indexers that need it with the same tag as the proxy.
