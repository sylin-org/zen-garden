---
audience: [operator, developer]
doc_type: guide
status: current
last_verified: 2026-03-25
note: "Web-based offering manifest authoring and catalog browser."
---

# Greenhouse

Greenhouse is the offering upkeep module built into every stone. It serves a standalone single-page application directly from Moss that provides a unified catalog of all offerings (installed, available, and image-direct), an in-browser manifest editor with real-time validation, and manifest generation from Docker image inspection.

---

## Accessing Greenhouse

Open any browser and navigate to:

```
http://<stone-name>.local:7185/greenhouse
```

For example: `http://stone-crystal-forest.local:7185/greenhouse`

No authentication is required on the local network. The SPA is compiled into the Moss binary and loads instantly.

---

## API Endpoints

All API endpoints live under `/api/v1/stone/greenhouse/`.

| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/greenhouse` | Serve the Greenhouse SPA (HTML page) |
| GET | `/api/v1/stone/greenhouse/catalog` | Unified offering inventory with compatibility, file list, and install state |
| GET | `/api/v1/stone/greenhouse/file?offering={name}&type={type}` | Read a manifest file (runtime overlay first, then built-in fallback) |
| PUT | `/api/v1/stone/greenhouse/file?offering={name}&type={type}` | Write or create a manifest file in the runtime directory |
| DELETE | `/api/v1/stone/greenhouse/file?offering={name}&type={type}` | Delete a custom overlay file (resets to built-in if one exists) |
| GET | `/api/v1/stone/greenhouse/export?offering={name}` | Export all manifest files for an offering as a JSON bundle |
| GET | `/api/v1/stone/greenhouse/containers` | List running managed offerings for the container picker |
| POST | `/api/v1/stone/greenhouse/validate` | Validate snippet YAML and optional frontmatter JSON; returns findings with severity |
| POST | `/api/v1/stone/greenhouse/generate` | Generate a full manifest set from Docker image inspection JSON |

The `type` query parameter accepts: `snippet`, `frontmatter`, `compatibility`, `guidance`, `research`, `adopted`, `adopted-guidance`, `capabilities`.

---

## Use Cases

### Visual manifest editing (Greenhouse)

Use Greenhouse when you want to browse the catalog visually, inspect an offering's compatibility on the current stone, or edit manifest files with immediate validation feedback. The editor highlights errors and warnings as you type and supports save with `Ctrl+S`. Custom overlays are written to the runtime manifests directory; deleting an overlay reveals the built-in file again.

### Manifest generation from images

To create a manifest for an image not yet in the catalog, use the generate endpoint (or the "new from image" flow in the UI). Greenhouse calls `POST /api/v1/stone/greenhouse/generate` with inspection JSON from the offerings inspect endpoint and produces a complete manifest set: snippet YAML, frontmatter JSON, compatibility YAML, and guidance Markdown.

### CLI workflows (Rake)

Use `garden-rake manifest` when you prefer terminal-based authoring, need to script manifest creation in CI, or want to batch-validate files locally without a stone connection. The CLI subcommands (`init`, `validate`, `test`, `export`, `enrich`) cover the same lifecycle as Greenhouse. See [rake-automation](rake-automation.md) for details.

### Catalog browsing

The catalog endpoint returns every offering known to the stone -- curated (embedded), custom (user-authored), installed, and available -- sorted with installed offerings first. Each entry includes compatibility status, file inventory with origin markers (built-in, custom, customized), Docker image reference, tags, port mappings, and volume count.

---

## Runtime Manifests Directory

Custom and overlay files are stored separately from the built-in files compiled into Moss:

- **Linux**: `/var/lib/zen-garden/manifests/sw/{category}/`
- **Override**: Set `GARDEN_MANIFESTS_DIR` to use a custom path

Files in this directory take precedence over built-in versions. Deleting a custom file through the Greenhouse editor or DELETE endpoint reveals the built-in file again.

---

## Related

- [Offering manifest compatibility](offering-manifest-compatibility.md)
- [Guidance authoring](guidance-authoring.md)
- [Offering lifecycle](offering-lifecycle.md)
- [Rake automation](rake-automation.md)
