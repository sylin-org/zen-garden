---
audience: developer
doc_type: decision
status: accepted
---

# OFFER-0007: FQN-Scoped Volume Isolation and Absolute Data Paths

**Date**: 2026-03-31
**Status**: Accepted

---

## Context

Zen Garden offerings use Docker bind mounts for persistent data (model weights, config, caches). Two architectural gaps surfaced during Windows development and multi-instance deployment planning:

1. **Relative paths on Windows**: `data_dir()` returned `.zen-garden` (relative to CWD). Docker Desktop for Windows rejects relative bind-mount paths. All downstream paths — `volumes_dir()`, `offering_config_dir()`, `topology_dir()` — inherited this problem, making `rake plant <offering>` fail with "includes invalid characters for a local volume name".

2. **Flat volume namespace**: All instances of an offering shared the same volume directories. `comfyui` and `comfyui::prod` would both mount `{volumes_dir}/comfyui-models`, making multi-instance deployment impossible — the second instance would read/write the first instance's data.

---

## Decision

### 1. Absolute data directory on Windows

`data_dir()` on Windows resolves `.zen-garden` against `std::env::current_dir()` at call time. The directory stays next to the service executable (same location as before), but the path is absolute so Docker and external tools can consume it.

On Linux, `data_dir()` continues to return `/var/lib/zen-garden` (already absolute).

The `GARDEN_DATA_DIR` environment variable override remains and takes precedence on both platforms.

### 2. Per-FQN volume directories

Volume host paths are namespaced by the FQN's encoded container name, not the offering name:

```
{volumes_dir}/{fqn_encoded}/{volume_name}
```

| FQN | Volume directory |
|-----|-----------------|
| `comfyui` | `volumes/comfyui/comfyui-models` |
| `comfyui::prod` | `volumes/comfyui--prod/comfyui-models` |
| `comfyui::staging` | `volumes/comfyui--staging/comfyui-models` |

The encoding uses `OfferingFqn::encoded_for_container()` which produces Docker-safe names (`--` separator for instances, consistent with container naming in OFFER-0006).

Default instances (no `::instance` suffix) produce the same path as the offering name, so existing single-instance deployments are unaffected.

### 3. Two-tier resolution

Volume resolution happens at two points with different FQN awareness:

- **Index/cache time** (`parse_template`): Uses the offering name. Produces template-level volumes for hashing, catalog display, and compatibility evaluation. FQN is not known at this stage.

- **Deploy time** (`parse_template_for_fqn`, `volumes_for_fqn`): Uses the FQN-encoded name. Called during install, upgrade (nourish), adoption, reconfigure, and updates. Seed extraction also uses FQN-aware paths so initial config lands in the correct instance directory.

---

## Consequences

- `rake plant comfyui` and `rake plant comfyui::prod` produce isolated volume trees. No data sharing between instances.
- Docker bind mounts work on Windows without manual `GARDEN_DATA_DIR` override.
- `companions_dir()` on Windows now derives from `data_dir()` instead of being a separate relative path.
- `shared_data_dir()` remains distinct (`%ProgramData%\zen-garden` on Windows) — it serves a different purpose (cross-process shared data for Koan clients and containers).
- Existing single-instance deployments on Linux are unaffected: `data_dir()` is already absolute, and the default FQN encodes to the offering name.

---

## Files Changed

| File | Change |
|------|--------|
| `common/src/constants/paths.rs` | `data_dir()` resolves to absolute on Windows; `companions_dir()` derives from it |
| `common/src/manifests/offering.rs` | `parse_template_for_fqn()`, volume namespace parameter |
| `common/src/utils/platform.rs` | `WindowsPaths` aligned with `data_dir()` |
| `moss/src/domain/offerings.rs` | `CompiledOffering::volumes_for_fqn()` |
| `moss/src/tasks/job_executors.rs` | Install uses FQN-aware volumes |
| `moss/src/domain/service_lifecycle.rs` | Nourish uses FQN-aware volumes |
| `moss/src/domain/adoption.rs` | Adoption uses FQN-aware volumes |
| `moss/src/domain/services_internal.rs` | Reconfigure uses FQN-aware volumes |
| `moss/src/api/v1/updates.rs` | Updates use FQN-aware volumes |
| `moss/src/domain/ceremony/phases/nourish.rs` | Ceremony nourish uses FQN-aware volumes |
