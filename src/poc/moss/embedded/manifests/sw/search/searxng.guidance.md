---
version: "1"
trigger: post_install
---
# SearXNG

## Quick Start

**Web UI:** http://{{server-name}}:{{default-port}}/

Search privately — results are aggregated from 70+ engines without tracking.

## Preferences

Click **Preferences** in the top-right to:

- Enable/disable search engines
- Set default language and region
- Choose interface theme
- Configure safe search level

Preferences are stored in a browser cookie by default.

## API Search

SearXNG's JSON API is enabled out of the box — no extra configuration needed.

```bash
curl "http://{{server-name}}:{{default-port}}/search?q=rust+programming&format=json"
```

## Configuration

Custom settings persist in the `searxng-data` volume at `/etc/searxng/settings.yml`.
Zen Garden seeds this file on first install with `use_default_settings: true` and
JSON API enabled. Your edits are preserved across updates.

**Edit settings:**
```bash
docker exec -it {{name}} cat /etc/searxng/settings.yml
```

**Restart after changes:**
```bash
docker restart {{name}}
```

## Common Tasks

**View logs:**
```bash
docker logs {{name}} -f
```

**Check health:**
```bash
curl -s http://{{server-name}}:{{default-port}}/ | head -1
```

## Further Reading

- [SearXNG Documentation](https://docs.searxng.org/)
- [Search Engine Configuration](https://docs.searxng.org/admin/settings/settings_engine.html)
- [Admin Guide](https://docs.searxng.org/admin/index.html)
