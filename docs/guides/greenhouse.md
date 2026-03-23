---
audience: [operator, contributor]
doc_type: guide
status: current
last_verified: 2026-03-22
---

# Greenhouse

Greenhouse is the offering upkeep module built into every stone. It provides a unified catalog of all offerings — installed, available, and image-direct — together with an in-browser editor for authoring and customizing manifest files. It runs as a standalone single-page application served directly by Moss.

---

## Accessing Greenhouse

Open a browser and navigate to:

```
http://<stone-name>.local:7185/greenhouse
```

For example: `http://stone-crystal-forest.local:7185/greenhouse`

The page loads immediately with no authentication required on the local network.

---

## Catalog View

The catalog shows all offerings known to the stone, grouped by installation state. Installed offerings appear first, then available ones. Each tile shows:

- Offering name and category
- Current status (running, stopped, or available)
- A compatibility indicator — a warning badge appears when the offering may not work on this stone's hardware

Use the search bar to filter by name. Incompatible offerings are dimmed but still accessible.

To create a manifest for an offering not yet in the catalog, click **new from image** and enter a Docker image reference (for example `nginx:latest` or `ghcr.io/org/app:v2`). Greenhouse inspects the image on the stone and generates a draft manifest.

---

## Detail View

Clicking any catalog tile opens the detail view for that offering. The detail view has three tiers.

### Identity and compatibility

The identity card shows the offering's description, Docker image reference, category, and tags. The compatibility field reports whether the stone's hardware can run the offering and why if it cannot.

### Status and service actions

The status card shows the current operational state and exposes action buttons:

| State | Available actions |
|-------|-------------------|
| Running | Stop, Restart |
| Stopped (installed) | Start, Restart, Remove |
| Available (not installed) | Install |

The garden section below the status card lists other stones in the garden that have the same offering installed, with links to their portrait pages.

### Files and editor

The **files** section contains the manifest bundle for the offering. Click the section header to expand it.

Each file in the bundle appears as a tab. A colored dot on the tab indicates the file's origin:

| Dot color | Meaning |
|-----------|---------|
| Sage (green) | Built-in — shipped with this version of Zen Garden |
| Clay (orange) | Custom — created by you, no built-in counterpart |
| Purple | Customized — your overlay of a built-in file |

The editor provides line numbers, real-time validation (for `.snippet.yaml`, `.frontmatter.json`, and `.compatibility.yaml` files), and keyboard save (`Ctrl+S`). The validation panel below the editor reports errors and warnings as you type.

To save changes, click **save** or press `Ctrl+S`. The file is written to the runtime manifests directory (`{data_dir}/manifests/sw/{category}/`).

To discard your customization and restore the built-in version, click **reset**. The reset button appears only when a custom overlay exists for a built-in file.

---

## Manifest File Types

Each offering can have up to eight associated files:

| File | Extension | Purpose |
|------|-----------|---------|
| `{name}.snippet.yaml` | yaml | Docker Compose-style service definition |
| `{name}.frontmatter.json` | json | Metadata: description, tags, image, category |
| `{name}.compatibility.yaml` | yaml | Hardware compatibility rules |
| `{name}.guidance.md` | md | Post-install instructions shown to the user |
| `{name}.research.md` | md | Research notes and background context |
| `{name}.adopted.yaml` | yaml | Configuration for adopted (non-managed) containers |
| `{name}.adopted.guidance.md` | md | Post-adopt instructions for adopted containers |
| `{name}.capabilities.yaml` | yaml | Capability declarations (models, extensions, etc.) |

Not all files need to exist. At minimum, a managed offering requires a `.snippet.yaml`.

---

## Authoring Manifests with Rake

`garden-rake manifest` provides CLI tooling for the full manifest authoring lifecycle. All subcommands that contact a stone accept `--at <stone-name>` to target a specific stone.

### Scaffold from a Docker image

```bash
garden-rake manifest init <image-ref> [--at <stone>] [--output <dir>] [--name <name>] [--category <category>]
```

Inspects the image on the connected stone and writes four files to `--output` (default: current directory):

- `{name}.snippet.yaml`
- `{name}.frontmatter.json`
- `{name}.compatibility.yaml`
- `{name}.guidance.md`

Example:

```bash
garden-rake manifest init nginx:latest --at stone-crystal-forest --output ./my-nginx
```

### Validate manifest files

```bash
garden-rake manifest validate [<path>]
```

Validates a single file or a directory of manifest files. Runs locally — no stone connection needed. Reports errors and warnings with severity codes. Exits with a non-zero status when errors are present.

```bash
garden-rake manifest validate ./my-nginx
```

### Test-deploy on a stone

```bash
garden-rake manifest test [<path>] [--at <stone>]
```

Uploads the manifest directory to the stone and starts a test deployment. Validates the manifest before sending. Prints the job ID for monitoring. To clean up afterward: `garden-rake remove {name}`.

```bash
garden-rake manifest test ./my-nginx --at stone-crystal-forest
```

### Export a running offering's manifest

```bash
garden-rake manifest export <offering> [--at <stone>] [--output <dir>]
```

Downloads all manifest files for a running offering from the stone and writes them to `--output` (default: current directory). Useful for inspecting or forking a curated offering.

```bash
garden-rake manifest export ollama --output ./ollama-custom
```

### Enrich with compatibility and guidance templates

```bash
garden-rake manifest enrich <path> [--auto]
```

Adds `.compatibility.yaml` and `.guidance.md` template files to an existing manifest directory when they are missing or minimal. Without `--auto`, prompts for confirmation before writing each file.

```bash
garden-rake manifest enrich ./my-nginx --auto
```

---

## Typical Workflow

To create and deploy a custom manifest:

1. Scaffold from the image:
   ```bash
   garden-rake manifest init myapp:latest --at stone-crystal-forest --output ./myapp
   ```

2. Review and edit the generated files. Open Greenhouse in the browser to validate interactively, or run:
   ```bash
   garden-rake manifest validate ./myapp
   ```

3. Enrich with compatibility rules and guidance if needed:
   ```bash
   garden-rake manifest enrich ./myapp
   ```

4. Test on the stone:
   ```bash
   garden-rake manifest test ./myapp --at stone-crystal-forest
   ```

5. Once satisfied, copy the files into the runtime manifests directory on the stone or use the Greenhouse editor to paste and save each file. The offering then appears as a custom entry in the catalog.

---

## Runtime Manifests Directory

Custom and overlay files are stored in the runtime manifests directory, separate from the built-in files compiled into Moss. The location is:

- **Linux**: `/var/lib/zen-garden/manifests/sw/{category}/`
- **Override**: Set `GARDEN_MANIFESTS_DIR` to use a custom path

Files in this directory take precedence over the built-in (embedded) versions. Deleting a custom file from the Greenhouse editor reveals the built-in file again.

---

## API Reference

All Greenhouse endpoints are under `/api/v1/stone/greenhouse/`.

| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/greenhouse` | Serves the Greenhouse SPA |
| GET | `/api/v1/stone/greenhouse/catalog` | Unified offering inventory with compatibility and file list |
| GET | `/api/v1/stone/greenhouse/file?offering=&type=` | Read a manifest file (runtime overlay first, then built-in) |
| PUT | `/api/v1/stone/greenhouse/file?offering=&type=` | Write a manifest file to the runtime directory |
| DELETE | `/api/v1/stone/greenhouse/file?offering=&type=` | Delete a custom/overlay file (resets to built-in) |
| GET | `/api/v1/stone/greenhouse/export?offering=` | Export all manifest files as a JSON bundle |
| GET | `/api/v1/stone/greenhouse/containers` | List running managed offerings (for the image picker) |
| POST | `/api/v1/stone/greenhouse/validate` | Real-time manifest validation |
| POST | `/api/v1/stone/greenhouse/generate` | Generate manifest files from image inspection JSON |

The `type` query parameter accepts: `snippet`, `frontmatter`, `compatibility`, `guidance`, `research`, `adopted`, `adopted-guidance`, `capabilities`.

---

## Related

- [Offering manifest compatibility](offering-manifest-compatibility.md)
- [Guidance authoring](guidance-authoring.md)
- [Offering lifecycle](offering-lifecycle.md)
- [Rake automation](rake-automation.md)
