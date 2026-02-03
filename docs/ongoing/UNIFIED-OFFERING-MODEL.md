# Unified Offering Model

**Status**: Implementation In Progress
**Last Updated**: 2026-02-02

This document consolidates research and implementation decisions for unifying `SwEntry` and `OfferingManifest` into a single offering model.

---

## Executive Summary

The codebase has two parallel systems for defining offerings:
1. **SwEntry** (container templates) - stored in `manifest_registry.sw.entries`
2. **OfferingManifest** (detection/adoption) - stored in `manifest_registry.offering_manifests`

This creates duplication, split identity, and prevents multi-mode offerings. The unified model merges these into a single `SwEntry` structure that supports all modes through optional configuration blocks.

---

## Part 1: Current State Analysis

### SwEntry (`src/common/src/manifests/sw.rs`)

**Purpose**: Container-based software offerings for Docker deployment.

**Structure** (before unification):
```rust
pub struct SwEntry {
    pub name: String,                    // Identity
    pub category: String,                // Organization
    pub snippet_yaml: String,            // Docker Compose template (raw)
    pub compatibility: Option<CompatibilityRules>,
    pub frontmatter: Option<SwFrontmatter>,
    pub guidance: Option<String>,
}
```

**Use Cases**:
| Consumer | Field Used | Purpose |
|----------|------------|---------|
| `TemplateEngine` | `snippet_yaml` | Render Docker Compose |
| Portrait API | `frontmatter` | UI display (description, icon, tags) |
| Offerings API | `compatibility` | Hardware capability filtering |
| Portrait UI | `guidance` | Installation documentation |

**Key Insight**: `snippet_yaml` contains Tera template expressions (`{{ offering_name }}`). It must remain a raw string, not parsed.

### OfferingManifest (`src/common/src/manifests/offering.rs`)

**Purpose**: Multi-mode offering definition supporting managed, adopted, and borrowed deployment.

**Structure**:
```rust
pub struct OfferingManifest {
    pub name: String,
    pub category: String,
    pub description: String,
    pub modes: Vec<OfferingMode>,       // [Managed, Adopted, Borrowed]
    pub tags: Vec<String>,

    // Managed mode (DUPLICATES snippet content!)
    pub image: Option<String>,
    pub ports: Vec<(u16, u16)>,
    pub environment: Vec<String>,
    pub volumes: Vec<(String, String)>,

    // Adopted mode
    pub detection: Option<OsDetectionRules>,
    pub control: Option<ControlConfig>,

    // Borrowed mode
    pub location: Option<LocationConfig>,
    pub health: Option<HealthConfig>,

    pub connection_template: Option<String>,
}
```

**Use Cases**:
| Consumer | Field Used | Purpose |
|----------|------------|---------|
| `auto_adoption_task` | `detection` | Native service detection |
| `DetectionOrchestrator` | `detection.get_current_os_rules()` | OS-specific detection |
| Service control | `control` | Start/stop/restart commands |

### Problems with Current Design

1. **Duplication**: `OfferingManifest.image/ports/volumes` duplicates `SwEntry.snippet_yaml`
2. **Split Identity**: Same offering exists in two collections with potentially different data
3. **Mode Fragmentation**: Managed-only in `sw.entries`, adopted-only in `offering_manifests`
4. **No Multi-Mode**: Can't have one offering that supports BOTH managed AND adopted
5. **Two Sources of Truth**: Which `name`/`category` is authoritative?

---

## Part 2: Validated Code References

### Auto-Adoption Flow (`src/moss/src/tasks/auto_adoption.rs`)

```rust
// Gets adoptable manifests from offering_manifests collection
let adoptable_manifests = state.manifest_registry.offerings_by_mode(&OfferingMode::Adopted);

for manifest in adoptable_manifests {
    // Uses detection rules
    match orchestrator.detect(manifest).await {
        Ok(result) if result.detected && result.stable => {
            // Uses control config
            start_command: manifest.control.as_ref().and_then(|c| c.start_command.clone()),
            // Uses port from manifest
            port: manifest.default_host_port(),
        }
    }
}
```

### Service Discovery Flow (`src/moss/src/domain/service_discovery.rs:470-494`)

```rust
async fn get_offering_port(offering: &str, state: &AppState) -> u16 {
    // First try OfferingManifest (multi-mode definitions)
    if let Some(manifest) = state.manifest_registry.get_offering_manifest(offering) {
        let port = manifest.default_host_port();
        if port > 0 {
            return port;
        }
    }

    // Then try SwEntry (container templates)
    if let Some(entry) = state.manifest_registry.sw.get(offering) {
        if let Ok(template) = entry.parse_template() {
            return template.default_host_port();
        }
    }

    8080 // Fallback
}
```

This shows the **two-collection problem** - consumers must check both sources.

### ManifestRegistry (`src/common/src/manifests/registry.rs`)

```rust
pub struct ManifestRegistry {
    pub sw: SwManifests,                    // Container templates
    pub hw: HwManifests,                    // Hardware definitions
    pub offering_manifests: HashMap<String, OfferingManifest>,  // SEPARATE!
}

impl ManifestRegistry {
    pub fn offerings_by_mode(&self, mode: &OfferingMode) -> Vec<&OfferingManifest> {
        self.offering_manifests
            .values()
            .filter(|m| m.modes.contains(mode))
            .collect()
    }
}
```

---

## Part 3: Unified Model Implementation

### Core Principle

**Mode as Configuration, Not Type**: An offering supports a mode if its configuration is present.

### New SwEntry Structure (Implemented)

```rust
pub struct SwEntry {
    // ═══════════════════════════════════════════════════════════════
    // IDENTITY (required)
    // ═══════════════════════════════════════════════════════════════
    pub name: String,
    pub category: String,

    // ═══════════════════════════════════════════════════════════════
    // LEGACY FIELDS (backward compatibility)
    // ═══════════════════════════════════════════════════════════════
    pub snippet_yaml: String,               // Still populated from .snippet.yaml
    pub compatibility: Option<CompatibilityRules>,
    pub frontmatter: Option<SwFrontmatter>,
    pub guidance: Option<String>,

    // ═══════════════════════════════════════════════════════════════
    // MODE CONFIGURATIONS (Unified Model)
    // ═══════════════════════════════════════════════════════════════
    pub managed: Option<ManagedModeConfig>,   // Container deployment
    pub adopted: Option<AdoptedModeConfig>,   // Native service detection
    pub borrowed: Option<BorrowedModeConfig>, // External service

    // ═══════════════════════════════════════════════════════════════
    // CROSS-MODE FIELDS
    // ═══════════════════════════════════════════════════════════════
    pub connection_template: Option<String>,
}
```

### Mode-Specific Configurations

```rust
/// Managed mode: container-based deployment
pub struct ManagedModeConfig {
    pub snippet_yaml: String,           // Raw Docker Compose template
    pub network: Option<NetworkRequirements>,
    pub tasks: Option<Vec<TaskDefinition>>,
}

/// Adopted mode: native service detection and control
pub struct AdoptedModeConfig {
    pub detection: OsDetectionRules,    // OS-specific detection rules
    pub control: Option<ControlConfig>, // Start/stop/restart commands
    pub default_control_level: AdoptedControlLevel,
    pub health_check: Option<HealthConfig>,
}

/// Borrowed mode: external service announcement
pub struct BorrowedModeConfig {
    pub default_location: Option<LocationConfig>,
    pub health: Option<HealthConfig>,
    pub location_required: bool,
}
```

### Mode Support Methods

```rust
impl SwEntry {
    /// Get supported modes (derived from config presence)
    pub fn modes(&self) -> Vec<OfferingMode> {
        let mut modes = Vec::new();
        if self.managed.is_some() || !self.snippet_yaml.is_empty() {
            modes.push(OfferingMode::Managed);
        }
        if self.adopted.is_some() {
            modes.push(OfferingMode::Adopted);
        }
        if self.borrowed.is_some() {
            modes.push(OfferingMode::Borrowed);
        }
        modes
    }

    pub fn supports_mode(&self, mode: &OfferingMode) -> bool {
        match mode {
            OfferingMode::Managed => self.managed.is_some() || !self.snippet_yaml.is_empty(),
            OfferingMode::Adopted => self.adopted.is_some(),
            OfferingMode::Borrowed => self.borrowed.is_some(),
        }
    }
}
```

---

## Part 4: Migration Path

### Phase 1: Extend SwEntry (COMPLETED)

- Added `ManagedModeConfig`, `AdoptedModeConfig`, `BorrowedModeConfig` structs
- Extended `SwEntry` with optional mode configs
- Added `modes()` and `supports_mode()` methods
- Updated loading to initialize new fields as `None`

### Phase 2: Load Adopted Config (NEXT)

Update manifest loading to parse `.adopted.yaml` files and populate `SwEntry.adopted`:

```rust
// In SwManifests::load_entry
fn load_entry(category_dir: &Path, category: &str, offering_name: &str) -> Result<SwEntry> {
    // ... existing snippet loading ...

    // Load adopted config (new)
    let adopted_path = category_dir.join(format!("{}.adopted.yaml", offering_name));
    let adopted = if adopted_path.exists() {
        match parse_adopted_config(&adopted_path) {
            Ok(config) => Some(config),
            Err(e) => {
                tracing::warn!("Failed to parse adopted config: {}", e);
                None
            }
        }
    } else {
        None
    };

    Ok(SwEntry {
        // ... existing fields ...
        adopted,
        // ...
    })
}
```

### Phase 3: Update ManifestRegistry

Change from two collections to one:

```rust
// BEFORE
pub struct ManifestRegistry {
    pub sw: SwManifests,
    pub offering_manifests: HashMap<String, OfferingManifest>,
}

// AFTER
pub struct ManifestRegistry {
    pub offerings: HashMap<String, SwEntry>,  // Single collection
    pub hw: HwManifests,
}

impl ManifestRegistry {
    pub fn offerings_by_mode(&self, mode: &OfferingMode) -> Vec<&SwEntry> {
        self.offerings.values()
            .filter(|o| o.supports_mode(mode))
            .collect()
    }
}
```

### Phase 4: Update Consumers

Update `auto_adoption_task` and other consumers to use unified `SwEntry`:

```rust
// BEFORE
let adoptable_manifests = state.manifest_registry.offerings_by_mode(&OfferingMode::Adopted);
for manifest in adoptable_manifests {
    manifest.detection.get_current_os_rules()
}

// AFTER
let adoptable = state.manifest_registry.offerings_by_mode(&OfferingMode::Adopted);
for offering in adoptable {
    offering.get_detection_rules()  // Convenience method
}
```

---

## Part 5: File Format Evolution

### Legacy Format (Supported Indefinitely)

```yaml
# mongodb.snippet.yaml - Container template only
image: mongo:7
ports:
  default: [27017, 27017]
volumes:
  - mongodb_data:/data/db
```

### Full Manifest Format (New)

```yaml
# mongodb.manifest.yaml - All modes in one file
name: mongodb
category: data

metadata:
  description: MongoDB document database
  tags: [nosql, document, database]
  port: 27017

managed:
  snippet: |
    image: mongo:{{ version | default("7") }}
    ports:
      default: [{{ port | default(27017) }}, 27017]

adopted:
  detection:
    linux:
      - method: command
        config:
          command: mongod --version
          expected_pattern: "db version v([0-9.]+)"
    windows:
      - method: command
        config:
          command: mongod --version
          expected_pattern: "db version v([0-9.]+)"
  control:
    level: monitor
    start_command: systemctl start mongod
    stop_command: systemctl stop mongod

connection_template: |
  mongodb://{{ host }}:{{ port }}/{{ database | default("") }}
```

---

## Part 6: Benefits Summary

1. **Single Source of Truth**: One `SwEntry` per offering
2. **Multi-Mode Support**: MongoDB can be managed AND adopted
3. **No Duplication**: Mode support derived from config presence
4. **Simpler Mental Model**: "Offering with optional mode configs"
5. **Backwards Compatible**: Existing `.snippet.yaml` files work unchanged
6. **DDD Aligned**: `SwEntry` is the aggregate root, modes are value objects

---

## Implementation Status

| Task | Status |
|------|--------|
| Extend SwEntry with mode configs | Done |
| Add mode support methods | Done |
| Export new types from manifests module | Done |
| Update manifest loading for adopted | Pending |
| Update ManifestRegistry | Pending |
| Update auto_adoption_task | Pending |
| Update service_discovery | Pending |

---

## Naming Considerations

The user feedback indicated preference for these names:
- **OfferingManifest**: The definition/blueprint (file on disk)
- **Offering**: Runtime entity (manifest + fitness assessment)
- **OfferingInstance**: Actually deployed/adopted/borrowed service
- **OfferingFitness**: Compatibility evaluation result

Current `SwEntry` may be renamed to `OfferingManifest` in a future phase to align with domain language.
