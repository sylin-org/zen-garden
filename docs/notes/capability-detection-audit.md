# Hardware capability detection — audit & collapse plan

> Audit of how `HardwareCapabilities` is built across moss, prompted by a phone Stone
> reporting wrong storage. Records what was collapsed and what remains (a behavior-affecting
> consistency collapse, deliberately deferred for a go-ahead).

## Detection paths (as found)

| Path | Role | Notes |
|------|------|-------|
| `tasks/hardware_detection.rs::detect_capabilities_background` | **Canonical** (active) | Spawned at startup. 3-phase progressive (CPU → GPU → merge). GPU-merge preserves cached VRAM, builds `ai_capabilities`, persists `capabilities.json`, syncs topology, rebuilds catalog. |
| `infra/hardware.rs::detect_hardware` | **Dead** | `pub`-exported but **zero callers**. One-shot; no GPU-merge, no AI summary, no cache, no post-detection integration. Has DMI detection. |
| `api/v1/garden.rs::get_capabilities` | **Live override** | Rebuilds capabilities **synchronously on every request**; never reads the cache. Has `stone_id` + `docker_version`; no AI summary, no DMI, no GPU-merge. |
| `api/v1/capabilities.rs::read_core` | **Cache reader** | Reads `state.current.capabilities` (populated by the canonical task). |

## The divergence (the real finding)

The three builders are **not equivalent** — which fields you get depends on which endpoint you hit:

| Field | task (canonical) | infra (dead) | garden (sync) |
|-------|:---:|:---:|:---:|
| `stone_id` | ✗ (None) | ✗ | ✓ |
| `ai_capabilities` | ✓ | ✗ | ✗ |
| `system_manufacturer/product` (DMI) | ✗ | ✓ | ✗ |
| `docker_version` | ✗ | ✓ (detect_docker) | ✓ (platform.container) |
| GPU VRAM preservation (`merge_gpus`) | ✓ | ✗ | ✗ |
| cache persistence / topology sync / catalog rebuild | ✓ | ✗ | ✗ |

This is a latent inconsistency bug, not just duplication.

## Collapsed so far (commit d42d7dcd)

- **Disk capability** — the one part that *was* identical across all three: collapsed into
  `StoneResources::disk_capabilities()` (derived from `data_partition()`). All builders call it.
- **`disk_type` on Android** — `detect_via_proc_mounts()` (findmnt/lsblk-free) so phone storage
  classifies as SSD, not Unknown.
- Earlier: `StoneResources::data_partition()` — the single storage-partition accessor (commit
  0607da30), replacing 12 divergent `mount_point == "/"` selections.

## Remaining: the consistency collapse (deferred — behavior-affecting)

Goal: **one canonical builder, every field, served consistently from cache.**

1. Populate the gaps in the canonical task (`detect_capabilities_background`): set `stone_id`
   (Phase 1, from `state.current.stone.id`), `docker_version` (via `detect_docker()`), and DMI
   `system_manufacturer/product` (reuse `infra/hardware.rs::detect_system_manufacturer/product`).
2. Redirect `api/v1/garden.rs::get_capabilities` to **read the cached** capabilities
   (`state.current.capabilities`, with a `create_skeleton` fallback) instead of rebuilding —
   same source as `capabilities.rs::read_core`.
3. Remove the dead `infra/hardware.rs::detect_hardware` (zero callers) once its DMI helpers are
   reused by step 1.

**Behavior change:** garden endpoints would serve the progressively-detected cached capabilities
(the design intent) instead of a fresh synchronous rebuild on each request. This is why it is
deferred for an explicit go rather than bundled with the safe collapse above.
