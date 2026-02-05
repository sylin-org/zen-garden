# Manifest Directory Restructuring Plan

**Status:** Draft
**Created:** 2026-01-24
**Author:** Claude (with human oversight)

---

## Overview

This plan restructures manifest handling with two major changes:

1. **ManifestRegistry Architecture** (HIGH PRIORITY) - Unified manifest loading with single source of truth
2. **Directory Restructuring** - `manifests/sw/` and `manifests/hw/` separation

### Goals

1. **Single load, single source of truth** - ManifestRegistry loads all manifests once at startup
2. **Clean SoC** - Manifest loading separated from template parsing
3. Separate software offerings from hardware manifests
4. Enable hardware firmware management ("nourish" for stones)
5. Remove hardcoded category lists - dynamic discovery
6. **No backward compatibility** - greenfield approach

---

## Priority 1: ManifestRegistry Architecture (HIGH)

### Problem Statement

Current codebase has fragmented manifest handling:

| Component | Location | Purpose | Problem |
|-----------|----------|---------|---------|
| `manifest_loader.rs` | infra | Loads `OfferingManifest` | Separate from templates |
| `TemplateLoader` | templates.rs | Loads snippets | Rescans filesystem repeatedly |
| `offerings_index` | domain | Caches offerings | Yet another cache layer |
| Hardcoded categories | templates.rs (4 places) | Category lists | DRY violation |

**Result:** Multiple filesystem scans, redundant caches, confused terminology.

### Solution: Unified ManifestRegistry

```rust
// src/moss/src/infra/manifests/mod.rs

/// Single source of truth for all manifests - loaded once at startup
pub struct ManifestRegistry {
    pub sw: SwManifests,
    pub hw: HwManifests,
}

pub struct SwManifests {
    /// All software offerings, keyed by name (e.g., "mongodb")
    pub entries: HashMap<String, SwEntry>,
    /// Discovered category names (derived from entries)
    pub categories: Vec<String>,
}

pub struct SwEntry {
    pub name: String,
    pub category: String,
    pub snippet_yaml: String,                    // Raw YAML
    pub compatibility: Option<CompatibilityRules>,
    pub frontmatter: Option<SwFrontmatter>,
}

pub struct HwManifests {
    /// All hardware manifests, keyed by "vendor/model" (e.g., "dell/wyse-5070")
    pub entries: HashMap<String, HwEntry>,
    /// Discovered vendor names (derived from entries)
    pub vendors: Vec<String>,
}

pub struct HwEntry {
    pub vendor: String,
    pub model: String,
    pub manifest_yaml: String,                   // Raw YAML
    pub compatibility: Option<HwCompatibilityRules>,
    pub frontmatter: Option<HwFrontmatter>,
}

impl ManifestRegistry {
    /// Load all manifests from base directory - SINGLE FILESYSTEM SCAN
    pub fn load(base_dir: &Path) -> Result<Self> {
        let sw = SwManifests::load(&base_dir.join("sw"))?;
        let hw = HwManifests::load(&base_dir.join("hw"))?;
        Ok(Self { sw, hw })
    }
}
```

### AppState Changes

**Before:**
```rust
pub struct AppState {
    pub manifests: Arc<RwLock<Vec<OfferingManifest>>>,  // Confusing
    pub templates: Arc<TemplateLoader>,                  // Separate loader
    pub offerings_index: Arc<RwLock<Option<...>>>,       // Another cache
    // ...
}
```

**After:**
```rust
pub struct AppState {
    pub manifests: Arc<ManifestRegistry>,  // SINGLE SOURCE OF TRUTH
    // offerings_index derived from manifests.sw when needed
    // ...
}
```

### Migration Path

| Current | After | Action |
|---------|-------|--------|
| `manifest_loader.rs` | `ManifestRegistry::load()` | Merge logic |
| `TemplateLoader` struct | `SwEntry::parse_template()` | Convert to method |
| `TemplateLoader::load()` | `registry.sw.entries.get(name)` | Direct lookup |
| `TemplateLoader::list_templates()` | `registry.sw.entries.values()` | Direct iteration |
| `AppState.templates` | Removed | Use `manifests.sw` |
| `AppState.manifests` | `Arc<ManifestRegistry>` | Unified type |
| Hardcoded category arrays | `registry.sw.categories` | Dynamic from entries |

### Files to Modify

| File | Change |
|------|--------|
| `src/moss/src/infra/manifests/mod.rs` | **NEW** - ManifestRegistry module |
| `src/moss/src/infra/manifests/sw.rs` | **NEW** - SwManifests, SwEntry |
| `src/moss/src/infra/manifests/hw.rs` | **NEW** - HwManifests, HwEntry |
| `src/moss/src/infra/mod.rs` | Add `pub mod manifests;` |
| `src/moss/src/templates.rs` | Simplify to parsing only, use registry |
| `src/moss/src/infra/manifest_loader.rs` | **REMOVE** - merged into registry |
| `src/moss/src/app_state.rs` | Replace `templates` + `manifests` with single `manifests: Arc<ManifestRegistry>` |
| `src/moss/src/bootstrap/run.rs` | Load ManifestRegistry once at startup |
| `src/moss/src/tasks/coordinator.rs` | Use registry instead of template loader |
| `src/moss/src/domain/offerings.rs` | Derive from registry.sw |
| `src/moss/src/api/v1/services.rs` | Use registry lookups |

### Implementation Approach

**IMPORTANT: Complete revamp, not Companion pattern.**

- `TemplateLoader` is **removed**, not wrapped
- `manifest_loader.rs` is **deleted**, not abstracted
- All consumers are **rewritten** to use `ManifestRegistry` directly
- No shims, no compatibility layers, no forwarding methods

---

## Priority 2: Directory Structure

### Before

```
manifests/
├── data/
│   ├── mongodb.snippet.yaml
│   ├── mongodb.compatibility.yaml
│   ├── mongodb.frontmatter.json
│   └── ...
├── messaging/
├── ai/
├── vector/
├── secrets/
├── observability/
├── cache/
├── proxy/
├── auth/
├── dashboard/
├── networking/
├── automation/
├── devops/
├── timeseries/
├── storage/
├── hw/                          # Already exists (new)
│   └── dell/
│       └── wyse-5070.*
└── taxonomy.dictionary.yaml
```

### After

```
manifests/
├── sw/                          # Software offerings
│   ├── data/
│   │   ├── mongodb.snippet.yaml
│   │   ├── mongodb.compatibility.yaml
│   │   ├── mongodb.frontmatter.json
│   │   └── ...
│   ├── messaging/
│   ├── ai/
│   ├── vector/
│   ├── secrets/
│   ├── observability/
│   ├── cache/
│   ├── proxy/
│   ├── auth/
│   ├── dashboard/
│   ├── networking/
│   ├── automation/
│   ├── devops/
│   ├── timeseries/
│   └── storage/
├── hw/                          # Hardware manifests
│   ├── dell/
│   │   ├── wyse-5070.manifest.yaml
│   │   ├── wyse-5070.compatibility.yaml
│   │   ├── wyse-5070.frontmatter.json
│   │   └── wyse-5070.research.md
│   ├── hp/
│   └── lenovo/
└── taxonomy.dictionary.yaml     # Stays at root
```

---

## Priority 3: Installer Scripts

### `installer/NewStone.ps1`

**Current (lines 989-1006):**

```powershell
# Copy each category directory
$categories = @("data", "messaging", "ai", "vector", "secrets", "observability", "cache")
foreach ($category in $categories) {
    $categoryPath = Join-Path $manifestsSource $category
    if (Test-Path $categoryPath) {
        # ... copy logic ...
    }
}
```

**Change to:**

```powershell
# Copy manifests from sw/ directory (dynamic category discovery)
$swManifestsSource = Join-Path $manifestsSource "sw"
if (Test-Path $swManifestsSource) {
    # Copy all category directories dynamically
    Get-ChildItem -Path $swManifestsSource -Directory | ForEach-Object {
        $categoryName = $_.Name
        $categoryDest = Join-Path $templatesDir $categoryName
        New-Item -ItemType Directory -Path $categoryDest -Force | Out-Null

        # Copy all offering artifacts
        Copy-Item (Join-Path $_.FullName "*.snippet.yaml") $categoryDest -Force -ErrorAction SilentlyContinue
        Copy-Item (Join-Path $_.FullName "*.compatibility.yaml") $categoryDest -Force -ErrorAction SilentlyContinue
        Copy-Item (Join-Path $_.FullName "*.frontmatter.json") $categoryDest -Force -ErrorAction SilentlyContinue
    }
    Write-Step "manifests/sw/* → stone-root/var/lib/zen-garden/manifests/" "OK"
}
```

Also update the `$manifestsSource` path reference:

```powershell
# Line 62 - Config
ManifestsDir = (Join-Path $PSScriptRoot "..\manifests")  # No change needed
```

---

### 3. `installer/build.ps1`

**Current (lines 472-474, 531-533):**

```powershell
# Copy manifests if they exist
if (Test-Path $manifestsDir) {
    Copy-Item $manifestsDir (Join-Path $packageDir "manifests") -Recurse
}
```

**No structural change needed** - it copies the entire `manifests/` directory recursively, which will include both `sw/` and `hw/`.

**Optional enhancement:** Add a step to verify expected structure:

```powershell
# Validate manifest structure
$swDir = Join-Path $manifestsDir "sw"
$hwDir = Join-Path $manifestsDir "hw"
if (-not (Test-Path $swDir)) {
    Write-Warning "manifests/sw/ not found - software offerings may be missing"
}
```

---

### 4. `installer/moss-update-helper.sh`

**Current (line 134-136):**

```bash
# Deploy manifests
if [[ -d "$pkg_dir/manifests" ]]; then
    mkdir -p /var/lib/zen-garden/manifests
    cp -r "$pkg_dir/manifests/"* /var/lib/zen-garden/manifests/
    log "Updated manifests"
fi
```

**Change to:**

```bash
# Deploy software manifests
if [[ -d "$pkg_dir/manifests/sw" ]]; then
    mkdir -p /var/lib/zen-garden/manifests
    cp -r "$pkg_dir/manifests/sw/"* /var/lib/zen-garden/manifests/
    log "Updated software manifests"
fi

# Deploy hardware manifests (future use)
if [[ -d "$pkg_dir/manifests/hw" ]]; then
    mkdir -p /var/lib/zen-garden/manifests/hw
    cp -r "$pkg_dir/manifests/hw/"* /var/lib/zen-garden/manifests/hw/
    log "Updated hardware manifests"
fi
```

---

## Implementation Order

### Step 1: Directory Structure (DONE)

```bash
# Already completed:
mkdir -p manifests/sw
mv manifests/{data,messaging,ai,...} manifests/sw/
# hw/ already in place
```

### Step 2: ManifestRegistry Module

1. Create `src/moss/src/infra/manifests/` module
2. Implement `ManifestRegistry`, `SwManifests`, `HwManifests`
3. Single `load()` function scans filesystem once

### Step 3: Replace TemplateLoader

1. Update `AppState` to use `Arc<ManifestRegistry>`
2. Rewrite all consumers to use registry directly
3. Delete `templates.rs` and `manifest_loader.rs`

### Step 4: Update Installer Scripts

1. Update `NewStone.ps1` - copy from `manifests/sw/*`
2. Update `moss-update-helper.sh` - deploy to correct paths

### Step 5: Verify

1. `cargo test -p moss`
2. Build and deploy to test stone
3. Verify `garden-rake list` and `garden-rake install`

---

## Future Work

### Hardware Firmware Management

With `ManifestRegistry.hw` in place, future work includes:
- Hardware detection matching (`registry.hw.entries.find(|e| e.matches(dmidecode))`)
- Firmware update commands in `garden-rake nourish`
- Integration with `capabilities.rs` for hardware identification

### Runtime Paths

| Purpose | Linux Path | Windows Path |
|---------|------------|--------------|
| SW Manifests | `/var/lib/zen-garden/manifests/` | `.zen-garden\manifests\` |
| HW Manifests | `/var/lib/zen-garden/manifests/hw/` | `.zen-garden\manifests\hw\` |

---

## Checklist

### Priority 1: ManifestRegistry Architecture
- [ ] Create `src/moss/src/infra/manifests/mod.rs` - ManifestRegistry struct
- [ ] Create `src/moss/src/infra/manifests/sw.rs` - SwManifests, SwEntry
- [ ] Create `src/moss/src/infra/manifests/hw.rs` - HwManifests, HwEntry
- [ ] Update `src/moss/src/infra/mod.rs` - add `pub mod manifests;`
- [ ] Update `src/moss/src/app_state.rs` - replace `templates` + `manifests` with `Arc<ManifestRegistry>`
- [ ] Update `src/moss/src/bootstrap/run.rs` - load ManifestRegistry once at startup
- [ ] Delete `src/moss/src/templates.rs` - all functionality moved to ManifestRegistry
- [ ] Delete `src/moss/src/infra/manifest_loader.rs` - merged into ManifestRegistry
- [ ] Update all consumers to use ManifestRegistry directly
- [ ] Run `cargo test -p moss`

### Priority 2: Directory Structure
- [x] Create `manifests/sw/` directory
- [x] Move all category directories to `manifests/sw/`
- [x] Verify `manifests/hw/` structure intact

### Priority 3: Installer Scripts
- [ ] Update `NewStone.ps1` - dynamic category copy from `sw/`
- [ ] Update `moss-update-helper.sh` - handle `sw/` and `hw/` paths
- [ ] Verify `build.ps1` works with new structure

### Verification
- [ ] Build distribution: `.\installer\build.ps1`
- [ ] Create USB: `.\installer\NewStone.ps1`
- [ ] Deploy to test stone
- [ ] Verify `garden-rake list` shows all offerings
- [ ] Verify `garden-rake install mongodb` works

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 1.1 | 2026-01-24 | Added ManifestRegistry architecture as Priority 1 |
| 1.0 | 2026-01-24 | Initial plan |
