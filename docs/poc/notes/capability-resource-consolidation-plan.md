# Capability / Resource consolidation plan

> Output of a deep-dive investigation (parallel map of the type model + producers + consumers +
> wire/cache/DSL contracts, then an adversarial plan pass).
> Builds on the already-done convergence: `data_partition()`, `disk_capabilities()`, one canonical
> capability detector (see `capability-detection-audit.md`).
>
> **Status: EXECUTED & verified on-device (S1–S7).** `disk_type` returns the `DiskType` enum;
> health collects resources once (4×→1×); the `DiskResources` wire shim is documented; the
> colliding storage `DiskResources` is renamed `DiskMeasurement`; the FactSource↔DSL contract is
> documented. S3 and S6 were already satisfied by the prior `data_partition` / `disk_capabilities`
> / canonical-detector work. No wire/cache/DSL contract changed; the dual-source model is kept.

## Headline verdict (refined)

**Do NOT structurally merge `HardwareCapabilities` and `StoneResources` into one type.** The
dual-source model is *correct*: capabilities are a **static, cached, chirped** view that backs the
manifest compatibility DSL; resources are the **live** snapshot for health/placement. They answer
different questions. The convergence target is eliminating **field duplication** — capabilities
*projected from* resources — not merging the structs. The audit confirmed a naive merge would break
wire/cache/DSL contracts.

## Hard contracts that constrain any change

1. **Manifest compatibility DSL (COMPAT-0002)** — 12 immutable `host.*` fact paths bind directly to
   `HardwareCapabilities` fields: `host.architecture` → `cpu.architecture`, `host.cpu.features` →
   `cpu.features`, `host.ram.total.mb` → `memory.total_mb`, `host.ai.runtime` →
   `gpus[].capabilities`, `host.gpu.*`, `host.os.family` → `runtime.os`, `host.cpu.model/pattern`.
   **These field names/paths are a contract** — deployed manifests break if they move.
2. **`capabilities.json` cache** — serde struct, **no version/migration layer**. Don't remove fields.
3. **`TopologyEntry.capabilities` chirp payload** — peers deserialize it; `stripped_for_chirp()`
   drops unconsumed fields. New fields must be `Option` + `skip_serializing_if`.
4. **Four API shapes** — `/capabilities` (static), `/resources` (live + `DiskResources` shim),
   `/portrait`, `/presence`. Keep all four; just make them read canonical accessors.

So: **the structs stay; only how they're *populated* and *read* converges.**

## The plan — quick wins → reshape

### Quick wins (safe, isolated, zero wire/API change)
- **S1 — `disk_type` enum, not String round-trip.** `detect_disk_type_for_mount()` returns
  `Option<DiskType>` directly; drop the `match s.as_str()` at the storage-build site and the
  enum→String round-trip. (`disk_capabilities()` keeps `DiskCapabilities.disk_type: Option<String>`
  at the *wire boundary only* — compat preserved.) `resources/system.rs`.
- **S2 — health collects once.** `check_disk_health` / `check_memory_health` / `build_disk_component`
  / `build_memory_component` take `&StoneResources` instead of each calling
  `collect_stone_resources()` (I/O ×4 → ×1). `domain/health/system.rs` + the health handler.
- **(within S1) extract `resolve_mount_source(mount)`** — dedup the `findmnt` calls in
  `detect_via_lsblk` + `detect_via_sysfs`.

### Medium (isolated, low risk)
- **S3 — public `resolve_disk_type(mount)`**; `portrait` + storage health use `data_partition()`
  instead of re-scanning mounts.
- **S4 — deprecate the `hardware.rs::DiskResources` shim** (the `/resources` endpoint already builds
  it from `data_partition()`); `#[deprecated]` + comment, no wire change.
- **S5 — rename `volume.rs::DiskResources` → `DiskMeasurement`** to kill the name collision with the
  hardware shim. Internal-only.

### The big reshape (the prize)
- **S6 — capabilities projected from resources.** At detection, populate `CpuCapabilities` (cores,
  architecture) and `MemoryCapabilities` (`total_mb` = `memory.total_bytes/1024/1024`) from a single
  `collect_stone_resources()` (features/threads still from `get_cpu_info()`; disk already via
  `disk_capabilities()`). The capability **struct is unchanged** (wire-safe); only its construction
  stops re-deriving what resources already have. `tasks/hardware_detection.rs`.
- **S7 — document** the `FactSource`↔DSL bindings and the dual-source rationale.

## Recommended sequencing
1. **S1 + S2 now** — independent, isolated, no wire/API change, immediate value (enum cleanup +
   4×→1× health I/O).
2. **S3–S5** next cycle (after S1) — consolidation + the rename.
3. **S6 gated** on the health-caching decision below; it's the largest, do it last with a startup
   cold-path + peer-chirp test.
4. **S7** documentation pass.

## Decisions needed from the maintainer
1. **`disk_type` case** — enum serializes lowercase (`ssd`); the `DiskCapabilities` wire string is
   uppercase (`SSD`). Keep the uppercase wire string (compat) and only use the enum internally?
   (Recommended: yes — no wire change.)
2. **Health freshness** — should health read the shared 5–30 s resources cache (cheap, slightly
   stale) or re-collect once per request (fresh)? S2 picks one.
3. **Cache schema migration** — `capabilities.json` has no versioning. Add a migration/`probe_version`
   layer, or treat the cache as disposable (regenerate on restart)? (Recommended: disposable.)
4. **`NormalizedResources`** — keep as the placement-scoring intermediate (compute-on-demand), or add
   `From<StoneResources>`? (Low stakes; keep for now.)

## Recommendation
Execute **S1 + S2** (safe, high-value, no contracts touched). Queue **S3–S5**. Hold **S6** for the
health-caching decision. **Do not** merge the two types — converge the duplication, keep the
dual-source model.
