# Unified Offering Model

**Status**: Part 7 (Runtime Instances) Complete, Parts 2-6 (Manifest Unification) Pending
**Last Updated**: 2026-02-03

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

## Part 7: Unified Runtime Instances

### Problem Statement

The codebase has **three separate types** for runtime offering instances:

1. **ServiceInfo** (`moss-registry.json`) - Managed container offerings
2. **AdoptedOfferingInfo** (`moss-adopted.json`) - Adopted native services
3. **BorrowedOfferingInfo** (`moss-borrowed.json`) - External borrowed services

This creates:
- Three separate AppState fields: `registry`, `adopted_offerings`, `borrowed_offerings`
- Three separate persistence files
- ~74 lines of duplicated code (conversions, parallel iterations)
- Inconsistent capability handling

### Solution: UnifiedOffering

Replace all three types with a single `UnifiedOffering` struct:

```rust
pub struct UnifiedOffering {
    // IDENTITY (common to all modes)
    pub offering_id: String,           // GUIDv7 for all modes
    pub name: String,                  // Instance name
    pub offering: String,              // Template/manifest name
    pub version: String,               // Always present ("unknown" if undetected)

    // STATE (common to all modes)
    pub status: OfferingStatus,        // Running/Stopped/Installing/etc
    pub health: ServiceHealthStatus,   // Healthy/Degraded/Offline
    pub sub_capabilities: Vec<SubCapability>,

    // LOCATION (unified)
    pub location: OfferingLocation,    // host, port, protocol, agnostic_port

    // MODE-SPECIFIC (enum with associated data)
    pub mode_data: OfferingModeData,

    // TIMESTAMPS
    pub registered_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// Mode-specific data as tagged enum
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum OfferingModeData {
    Managed(ManagedData),   // resources, job_id, guidance
    Adopted(AdoptedData),   // control_level, commands, health_check_url, detected_at
    Borrowed(BorrowedData), // health_method, credentials_key, connection_template, announced_at
}
```

### Mode-Specific Data Structures

```rust
/// Managed mode: container-based deployment tracked by Moss
pub struct ManagedData {
    pub resources: Option<ContainerResources>,  // CPU, memory usage
    pub job_id: Option<String>,                  // Installation tracking
    pub guidance: Option<OfferingGuidance>,      // Post-install docs
}

/// Adopted mode: native service detected and adopted by Moss
pub struct AdoptedData {
    pub control_level: AdoptedControlLevel,      // Full/Monitor/Announce
    pub start_command: Option<String>,
    pub stop_command: Option<String>,
    pub restart_command: Option<String>,
    pub health_check_url: Option<String>,
    pub container_name: Option<String>,          // If adopted from container
    pub detected_at: DateTime<Utc>,
}

/// Borrowed mode: external service announced by Moss
pub struct BorrowedData {
    pub health_method: Option<HealthMethod>,     // Http/Tcp/None
    pub credentials_key: Option<String>,
    pub connection_template: Option<String>,
    pub announced_at: DateTime<Utc>,
}
```

### UnifiedOffering Helper Methods

```rust
impl UnifiedOffering {
    // === Mode Checks ===
    pub fn mode(&self) -> OfferingMode;
    pub fn is_managed(&self) -> bool;
    pub fn is_adopted(&self) -> bool;
    pub fn is_borrowed(&self) -> bool;

    // === Mode Data Access ===
    pub fn managed_data(&self) -> Option<&ManagedData>;
    pub fn managed_data_mut(&mut self) -> Option<&mut ManagedData>;
    pub fn adopted_data(&self) -> Option<&AdoptedData>;
    pub fn adopted_data_mut(&mut self) -> Option<&mut AdoptedData>;
    pub fn borrowed_data(&self) -> Option<&BorrowedData>;
    pub fn borrowed_data_mut(&mut self) -> Option<&mut BorrowedData>;

    // === Legacy Conversion ===
    pub fn from_service_info(info: ServiceInfo) -> Self;
    pub fn from_adopted_offering(info: AdoptedOfferingInfo) -> Self;
    pub fn from_borrowed_offering(info: BorrowedOfferingInfo) -> Self;
    pub fn to_service_info(&self) -> Option<ServiceInfo>;  // Only for managed

    // === Timestamps ===
    pub fn touch(&mut self);  // Updates updated_at to now
}
```

### Unified Storage

**Single file**: `moss-offerings.json`

```json
[
  {
    "offering_id": "018d3c8f-1a2b-7c3d-8e4f-5a6b7c8d9e0f",
    "name": "ollama",
    "offering": "ollama",
    "version": "0.1.24",
    "status": "running",
    "health": "healthy",
    "location": { "host": "localhost", "port": 11434, "protocol": "http" },
    "mode_data": {
      "mode": "adopted",
      "control_level": "monitor",
      "health_check_url": "http://localhost:11434/api/tags",
      "detected_at": "2026-02-01T12:00:00Z"
    },
    "sub_capabilities": [{ "type": "model", "items": ["llama2", "mistral"] }],
    "registered_at": "2026-02-01T12:00:00Z"
  }
]
```

### AppState Changes

```rust
pub struct AppState {
    /// Unified offering registry (single collection for all modes)
    pub offerings: Arc<RwLock<Vec<UnifiedOffering>>>,

    // ... other fields unchanged
}

impl AppState {
    // === Primary Accessors ===
    pub async fn get_offerings(&self) -> Vec<UnifiedOffering>;
    pub async fn get_managed_offerings(&self) -> Vec<UnifiedOffering>;
    pub async fn get_adopted_offerings(&self) -> Vec<UnifiedOffering>;
    pub async fn get_borrowed_offerings(&self) -> Vec<UnifiedOffering>;
    pub async fn find_offering(&self, name: &str) -> Option<UnifiedOffering>;
    pub async fn find_offering_by_id(&self, id: &str) -> Option<UnifiedOffering>;

    // === Mutators (auto-persist + auto-chirp) ===
    pub async fn upsert_offering(&self, offering: UnifiedOffering, auto_chirp: bool);
    pub async fn remove_offering(&self, offering_id: &str, auto_chirp: bool);
    pub async fn replace_offerings(&self, offerings: Vec<UnifiedOffering>, auto_chirp: bool);

    // === Legacy Compatibility ===
    pub async fn get_services(&self) -> Vec<ServiceInfo>;  // Managed only
    pub async fn upsert_service(&self, service: ServiceInfo, auto_chirp: bool);
    pub async fn replace_services(&self, services: Vec<ServiceInfo>, auto_chirp: bool);

    // === Persistence ===
    pub async fn persist_offerings(&self) -> Result<()>;
    pub async fn persist_registry(&self) -> Result<()>;  // Alias for persist_offerings
}
```

### Benefits

- **Single source of truth** for all running offerings
- **Uniform capability handling** across all modes
- **Simplified APIs** - one collection to iterate
- **Consistent persistence** - one file to manage
- **74+ lines of duplicated code eliminated**

### Migration Support

The persistence layer includes automatic migration from legacy files:

```rust
// In persistence.rs
pub async fn load_unified_offerings() -> Result<Vec<UnifiedOffering>> {
    // 1. Try loading unified file first
    if let Ok(offerings) = load_from_file("moss-offerings.json") {
        return Ok(offerings);
    }

    // 2. Migrate from legacy files if unified doesn't exist
    let mut offerings = Vec::new();

    // Load and convert legacy managed services
    if let Ok(services) = load_registry().await {
        offerings.extend(services.into_iter().map(UnifiedOffering::from_service_info));
    }

    // Load and convert legacy adopted offerings
    if let Ok(adopted) = load_adopted_offerings().await {
        offerings.extend(adopted.into_iter().map(UnifiedOffering::from_adopted_offering));
    }

    // Load and convert legacy borrowed offerings
    if let Ok(borrowed) = load_borrowed_offerings().await {
        offerings.extend(borrowed.into_iter().map(UnifiedOffering::from_borrowed_offering));
    }

    // 3. Save unified format (legacy files preserved)
    if !offerings.is_empty() {
        save_unified_offerings(&offerings).await?;
    }

    Ok(offerings)
}
```

**Legacy files are preserved** during migration for safety. They can be manually archived after verifying the unified file is correct.

### Runtime Instance Status

| Task | Status |
|------|--------|
| Add UnifiedOffering types | ✅ Done |
| Add unified persistence | ✅ Done |
| Update AppState | ✅ Done |
| Update portrait.rs | ✅ Done |
| Update offering_capabilities.rs | ✅ Done |
| Update auto_adoption.rs | ✅ Done |
| Update adoption.rs | ✅ Done |
| Update services.rs | ✅ Done |
| Update health_monitor.rs | ✅ Done |
| Update coordinator.rs | ✅ Done |
| Update job_executors.rs | ✅ Done |
| Update nurturing_scheduler.rs | ✅ Done |
| Update task_scheduler.rs | ✅ Done |
| Update state_provider.rs | ✅ Done |
| Update stone.rs | ✅ Done |
| Remove legacy types | ⏳ Deferred (kept for API compat)

---

## Naming Considerations

The user feedback indicated preference for these names:
- **OfferingManifest**: The definition/blueprint (file on disk)
- **Offering**: Runtime entity (manifest + fitness assessment)
- **OfferingInstance**: Actually deployed/adopted/borrowed service
- **OfferingFitness**: Compatibility evaluation result

Current `SwEntry` may be renamed to `OfferingManifest` in a future phase to align with domain language.

### Field Naming Conventions

In `UnifiedOffering`:
- **`name`**: Instance identifier (e.g., `"my-mongodb"`, `"ollama@adopted"`)
- **`offering`**: Template/manifest type (e.g., `"mongodb"`, `"ollama"`)

For adopted services, the naming convention is `"{offering}@adopted"` to distinguish from managed instances.

For borrowed services, `name` and `offering` are typically the same since there's no manifest template.
