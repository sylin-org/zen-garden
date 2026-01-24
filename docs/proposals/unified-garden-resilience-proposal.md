# Unified Garden Resilience Development Proposal

**Status**: Proposal
**Date**: 2026-01-23
**Scope**: Intelligent Placement Completion + Cultivation System
**Codename**: Garden Resilience

---

## Executive Summary

This proposal consolidates two overlapping concerns into a single development effort:

1. **Intelligent Offering Placement** (partially implemented) — Fix critical metrics gap and complete feature
2. **Cultivation** (not implemented) — Full backup, recovery, and seed bank system

Both features share foundational concerns:
- Stone resource metrics collection
- Garden-wide topology awareness
- Offering identity and lifecycle management
- Multi-stone coordination

By developing them together, we avoid duplicate infrastructure and ensure consistent patterns.

---

## Goals

### Primary Goals

1. **Complete Placement Intelligence** — Real-time metrics for accurate scoring
2. **Enable Disaster Recovery** — Offerings survive stone death via seed banks
3. **Preserve Offering Identity** — Offerings maintain identity across renames and recovery
4. **Garden Self-Awareness** — Stones know their peers, resources, and collective health

### Non-Goals (This Phase)

- Machine learning for placement optimization
- Cross-subnet placement via Lantern
- Cloud-based seed banks (S3, Azure Blob)
- Real-time replication (this is backup, not HA)

---

## Architecture Overview

### Shared Infrastructure

```
┌─────────────────────────────────────────────────────────────────────┐
│                    SHARED INFRASTRUCTURE                             │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐ │
│  │  Topology       │    │  Metrics        │    │  Identity       │ │
│  │  Discovery      │    │  Collection     │    │  Management     │ │
│  │                 │    │                 │    │                 │ │
│  │  • Stone cache  │    │  • Real-time    │    │  • offering_id  │ │
│  │  • Peer refresh │    │  • CPU/mem/disk │    │  • Name history │ │
│  │  • mDNS         │    │  • Parallel     │    │  • Provenance   │ │
│  └────────┬────────┘    └────────┬────────┘    └────────┬────────┘ │
│           │                      │                      │          │
└───────────┼──────────────────────┼──────────────────────┼──────────┘
            │                      │                      │
    ┌───────┴───────┐      ┌───────┴───────┐      ┌───────┴───────┐
    │               │      │               │      │               │
    │  PLACEMENT    │      │  CULTIVATION  │      │  RECOVERY     │
    │               │      │               │      │               │
    │  • Scoring    │      │  • Snapshot   │      │  • Wishful    │
    │  • Ranking    │      │  • Seed banks │      │    planting   │
    │  • Selection  │      │  • Retention  │      │  • Restore    │
    │               │      │               │      │               │
    └───────────────┘      └───────────────┘      └───────────────┘
```

### Module Organization

```
src/
├── common/src/
│   ├── identity.rs              # NEW: Offering/Stone identity types
│   ├── seed_bank.rs             # NEW: Seed bank types and paths
│   └── manifests/
│       ├── offering.rs          # EXTEND: Add migration fields
│       └── migration.rs         # NEW: Migration manifest schema
│
├── moss/src/
│   ├── domain/
│   │   ├── placement.rs         # EXISTS: Minor fixes
│   │   ├── scoring.rs           # EXISTS: No changes
│   │   ├── metrics_collection.rs # EXISTS: Fix to use /metrics
│   │   ├── topology.rs          # EXISTS: Extend for cultivation
│   │   ├── identity.rs          # NEW: Identity tracking
│   │   ├── cultivation.rs       # NEW: Backup orchestration
│   │   ├── seed_bank.rs         # NEW: Seed bank abstraction
│   │   └── recovery.rs          # NEW: Recovery orchestration
│   │
│   ├── infra/
│   │   ├── config.rs            # EXTEND: Add [cultivation] section
│   │   └── seed_bank_access.rs  # NEW: Local/Network/Remote access
│   │
│   ├── api/v1/
│   │   ├── garden.rs            # EXISTS: Minor additions
│   │   └── cultivation.rs       # NEW: Cultivation endpoints
│   │
│   └── tasks/
│       ├── cultivation_scheduler.rs  # NEW: Cron-like backup trigger
│       └── seed_bank_monitor.rs      # NEW: Mount detection
│
└── rake/src/
    ├── commands/
    │   ├── offering/mod.rs      # EXISTS: Add wishful planting
    │   ├── cultivate.rs         # NEW: Backup commands
    │   ├── recover.rs           # NEW: Recovery commands
    │   └── seed_bank.rs         # NEW: Seed bank management
    │
    └── parser.rs                # EXTEND: Add wishfully keyword
```

---

## Development Milestones

### Phase 0: Foundation Fixes (2-3 days)

**Goal**: Fix placement metrics gap, establish identity model

#### 0.1 Real-Time Metrics Collection

**Files**: `src/moss/src/domain/metrics_collection.rs`

```rust
// Change from /capabilities to /metrics endpoint
pub async fn fetch_stone_metrics(endpoint: &str, timeout: Duration) -> Result<StoneMetrics> {
    let metrics_url = format!("{}/metrics", endpoint.trim_end_matches('/'));

    let response: MetricsApiResponse = client
        .get(&metrics_url)
        .timeout(timeout)
        .send()
        .await?
        .json()
        .await?;

    Ok(StoneMetrics {
        memory_free_mb: response.memory.available_mb,
        memory_total_mb: response.memory.total_mb,
        cpu_load_percent: response.cpu.usage_percent as u8,
        storage_free_gb: response.disk.available_gb,
        storage_total_gb: response.disk.total_gb,
        storage_type: detect_storage_type(&response.disk),
        architecture: response.system.architecture,
    })
}

// Fallback for older Moss versions
pub async fn fetch_stone_metrics_with_fallback(
    endpoint: &str,
    timeout: Duration
) -> Result<StoneMetrics> {
    match fetch_stone_metrics(endpoint, timeout).await {
        Ok(metrics) => Ok(metrics),
        Err(_) => {
            warn!("Falling back to estimated metrics for {}", endpoint);
            fetch_capabilities_estimated(endpoint, timeout).await
        }
    }
}
```

**Acceptance Criteria**:
- [ ] `garden-rake offer X somewhere` shows real CPU percentages (not all 0%)
- [ ] Storage free values reflect actual disk usage
- [ ] Fallback works for stones without /metrics endpoint
- [ ] Unit tests for metrics parsing

#### 0.2 Offering Identity Model

**Files**: `src/common/src/identity.rs` (new)

```rust
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Permanent offering identity (immutable)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct OfferingId(Uuid);

impl OfferingId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn parse(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Name change record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NameRecord {
    pub name: String,
    pub from: DateTime<Utc>,
    pub to: Option<DateTime<Utc>>,
}

/// Offering identity with history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfferingIdentity {
    pub offering_id: OfferingId,
    pub offering_type: String,
    pub current_name: String,
    pub names: Vec<NameRecord>,
    pub provenance: Vec<StoneProvenance>,
}

/// Where an offering has lived
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoneProvenance {
    pub stone_id: String,
    pub stone_name: String,
    pub from: DateTime<Utc>,
    pub to: Option<DateTime<Utc>>,
}
```

**Acceptance Criteria**:
- [ ] OfferingId generates UUID v7
- [ ] Identity struct serializes to YAML matching spec
- [ ] Name history tracks changes with timestamps

#### 0.3 Extend Offering Manifest Schema

**Files**: `src/common/src/manifests/offering.rs`

Add migration support to offering manifests:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfferingManifest {
    // ... existing fields ...

    /// Optional migration configuration for stateful offerings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration: Option<MigrationConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationConfig {
    pub strategy: MigrationStrategy,
    pub snapshot: Option<SnapshotConfig>,
    pub restore: Option<RestoreConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MigrationStrategy {
    VolumeSnapshot,      // Default: tar the volume
    StatefulSnapshot,    // Custom command (mongodump, pg_dump)
    Ephemeral,           // No backup needed
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotConfig {
    pub method: String,
    pub command: Vec<String>,
    pub volume: String,
    pub timeout_seconds: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreConfig {
    pub command: Vec<String>,
    pub post_restore_healthcheck: bool,
    pub timeout_seconds: u32,
}
```

**Acceptance Criteria**:
- [ ] Existing manifests parse without migration field
- [ ] New manifests can include migration configuration
- [ ] Schema documented in manifest guide

---

### Phase 1: Seed Bank Infrastructure (3-4 days)

**Goal**: Storage abstraction for backup targets

#### 1.1 Seed Bank Types

**Files**: `src/common/src/seed_bank.rs` (new)

```rust
/// Seed bank location configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SeedBankConfig {
    /// Local path (USB, local disk)
    Path {
        path: PathBuf,
        #[serde(default)]
        announce: bool,
    },

    /// Network mount (NFS, SMB)
    Network {
        protocol: NetworkProtocol,
        host: String,
        share: String,
        mount_point: PathBuf,
        #[serde(skip_serializing_if = "Option::is_none")]
        username: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        password: Option<String>,
        #[serde(default)]
        announce: bool,
    },

    /// Remote via another stone's API
    Remote {
        stone: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetworkProtocol {
    Nfs,
    Smb,
    Cifs,
}
```

#### 1.2 Seed Bank Access Layer

**Files**: `src/moss/src/infra/seed_bank_access.rs` (new)

```rust
/// Unified interface for seed bank operations
#[async_trait]
pub trait SeedBankAccess: Send + Sync {
    /// Write a backup to the seed bank
    async fn write_backup(
        &self,
        offering_id: &OfferingId,
        manifest: &BackupManifest,
        data: &[u8],
    ) -> Result<BackupRecord>;

    /// Get latest backup for an offering
    async fn get_latest(
        &self,
        offering_id: &OfferingId,
    ) -> Result<Option<Backup>>;

    /// Get specific backup by timestamp
    async fn get_backup(
        &self,
        offering_id: &OfferingId,
        timestamp: &str,
    ) -> Result<Option<Backup>>;

    /// List all backups for an offering
    async fn list_backups(
        &self,
        offering_id: &OfferingId,
    ) -> Result<Vec<BackupRecord>>;

    /// List all offerings with backups
    async fn list_offerings(&self) -> Result<Vec<OfferingRecord>>;

    /// Prune old backups according to retention policy
    async fn prune(
        &self,
        offering_id: &OfferingId,
        retention: &RetentionPolicy,
    ) -> Result<PruneResult>;

    /// Update the seed bank index
    async fn update_index(&self) -> Result<()>;
}

/// Local/Network filesystem implementation
pub struct FilesystemSeedBank {
    root: PathBuf,
}

/// Remote API implementation
pub struct RemoteSeedBank {
    endpoint: String,
    client: reqwest::Client,
}

impl SeedBankAccess for FilesystemSeedBank {
    async fn write_backup(&self, offering_id: &OfferingId, manifest: &BackupManifest, data: &[u8]) -> Result<BackupRecord> {
        let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let backup_dir = self.root
            .join("offerings")
            .join(offering_id.to_string())
            .join(&timestamp);

        tokio::fs::create_dir_all(&backup_dir).await?;

        // Write manifest
        let manifest_path = backup_dir.join("manifest.yaml");
        let manifest_yaml = serde_yaml::to_string(manifest)?;
        tokio::fs::write(&manifest_path, manifest_yaml).await?;

        // Write data
        let data_path = backup_dir.join("data.archive.gz");
        tokio::fs::write(&data_path, data).await?;

        // Update latest symlink
        let latest_link = self.root
            .join("offerings")
            .join(offering_id.to_string())
            .join("latest");
        let _ = tokio::fs::remove_file(&latest_link).await;
        tokio::fs::symlink(&backup_dir, &latest_link).await?;

        Ok(BackupRecord {
            timestamp,
            size_bytes: data.len() as u64,
            checksum: compute_checksum(data),
        })
    }

    // ... other implementations
}
```

#### 1.3 Configuration Extension

**Files**: `src/moss/src/infra/config.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MossConfig {
    // ... existing fields ...

    /// Cultivation (backup) configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cultivation: Option<CultivationConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CultivationConfig {
    /// Enable cultivation
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Cron schedule for automatic backups
    #[serde(default = "default_schedule")]
    pub schedule: String,  // "0 3 * * *"

    /// Local staging directory
    #[serde(default)]
    pub local: LocalStagingConfig,

    /// Seed bank targets (in priority order)
    #[serde(default)]
    pub seed_banks: Vec<SeedBankConfig>,

    /// Retention policy
    #[serde(default)]
    pub retention: RetentionPolicy,

    /// Write strategy
    #[serde(default)]
    pub strategy: WriteStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RetentionPolicy {
    #[serde(default = "default_daily")]
    pub daily: u32,    // 7
    #[serde(default = "default_weekly")]
    pub weekly: u32,   // 4
    #[serde(default = "default_monthly")]
    pub monthly: u32,  // 6
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WriteStrategy {
    #[serde(default)]
    pub write: WriteMode,   // "first" or "all"
    #[serde(default)]
    pub read: ReadMode,     // "first"
}
```

**Acceptance Criteria**:
- [ ] FilesystemSeedBank reads/writes correct directory structure
- [ ] RemoteSeedBank calls cultivation API endpoints
- [ ] Configuration parses from moss.toml
- [ ] Index file updated atomically with locking
- [ ] Unit tests for all access patterns

---

### Phase 2: Cultivation Domain (4-5 days)

**Goal**: Backup orchestration and snapshot execution

#### 2.1 Snapshot Execution

**Files**: `src/moss/src/domain/cultivation.rs` (new)

```rust
pub struct CultivationService {
    seed_bank: Arc<dyn SeedBankAccess>,
    container_runtime: Arc<dyn ContainerRuntime>,
    config: CultivationConfig,
}

impl CultivationService {
    /// Backup all offerings on this stone
    pub async fn cultivate_all(&self) -> Result<CultivationReport> {
        let mut report = CultivationReport::new();

        // Get available seed bank
        let seed_bank = self.get_active_seed_bank().await?;

        // Get all offerings
        let offerings = self.registry.get_all_offerings().await?;

        for offering in offerings {
            match self.cultivate_offering(&offering, &seed_bank).await {
                Ok(backup) => report.succeeded(offering.id.clone(), backup),
                Err(e) => {
                    warn!("Failed to backup {}: {}", offering.name, e);
                    report.failed(offering.id.clone(), e);
                }
            }
        }

        // Update stone manifest
        self.update_stone_manifest(&seed_bank).await?;

        // Apply retention policy
        self.apply_retention(&seed_bank).await?;

        Ok(report)
    }

    /// Backup single offering
    pub async fn cultivate_offering(
        &self,
        offering: &Offering,
        seed_bank: &dyn SeedBankAccess,
    ) -> Result<BackupRecord> {
        info!("Cultivating {} ({})", offering.name, offering.id);

        // 1. Determine snapshot method
        let method = self.get_snapshot_method(offering);

        // 2. Create snapshot
        let snapshot = match method {
            SnapshotMethod::VolumeSnapshot { volume } => {
                self.snapshot_volume(offering, &volume).await?
            }
            SnapshotMethod::Custom { command, volume } => {
                self.snapshot_custom(offering, &command, &volume).await?
            }
            SnapshotMethod::Ephemeral => {
                return Err(CultivationError::EphemeralOffering);
            }
        };

        // 3. Build manifest
        let manifest = self.build_backup_manifest(offering, &snapshot);

        // 4. Write to local staging
        let local_path = self.write_local_staging(&snapshot).await?;

        // 5. Write to seed bank
        let record = seed_bank.write_backup(
            &offering.id,
            &manifest,
            &snapshot.data,
        ).await?;

        info!("Cultivated {} -> {} bytes", offering.name, record.size_bytes);

        Ok(record)
    }

    async fn snapshot_volume(&self, offering: &Offering, volume: &str) -> Result<Snapshot> {
        // Create tar.gz of volume contents
        let volume_path = self.get_volume_path(offering, volume)?;
        let data = self.tar_gz_directory(&volume_path).await?;

        Ok(Snapshot {
            method: "volume-tar".to_string(),
            data,
            checksum: compute_checksum(&data),
        })
    }

    async fn snapshot_custom(
        &self,
        offering: &Offering,
        command: &[String],
        volume: &str,
    ) -> Result<Snapshot> {
        // Execute custom snapshot command in container
        let output_path = format!("/backup/snapshot-{}.archive", Utc::now().timestamp());

        let exit_code = self.container_runtime.exec(
            &offering.container_id,
            command,
            Some(Duration::from_secs(300)),
        ).await?;

        if exit_code != 0 {
            return Err(CultivationError::SnapshotCommandFailed(exit_code));
        }

        // Read snapshot file from container
        let data = self.container_runtime.read_file(
            &offering.container_id,
            &output_path,
        ).await?;

        Ok(Snapshot {
            method: command[0].clone(),
            data,
            checksum: compute_checksum(&data),
        })
    }
}
```

#### 2.2 Cultivation API Endpoints

**Files**: `src/moss/src/api/v1/cultivation.rs` (new)

```rust
/// POST /api/v1/cultivation/offerings/{offering_id}/backups
pub async fn write_backup(
    State(state): State<AppState>,
    Path(offering_id): Path<String>,
    mut multipart: Multipart,
) -> Result<Json<BackupRecord>, ApiError> {
    let offering_id = OfferingId::parse(&offering_id)?;

    let mut manifest: Option<BackupManifest> = None;
    let mut data: Option<Vec<u8>> = None;

    while let Some(field) = multipart.next_field().await? {
        match field.name() {
            Some("manifest") => {
                let bytes = field.bytes().await?;
                manifest = Some(serde_yaml::from_slice(&bytes)?);
            }
            Some("data") => {
                data = Some(field.bytes().await?.to_vec());
            }
            _ => {}
        }
    }

    let manifest = manifest.ok_or(ApiError::MissingField("manifest"))?;
    let data = data.ok_or(ApiError::MissingField("data"))?;

    let record = state.seed_bank.write_backup(&offering_id, &manifest, &data).await?;

    Ok(Json(record))
}

/// GET /api/v1/cultivation/offerings
pub async fn list_offerings(
    State(state): State<AppState>,
) -> Result<Json<OfferingsListResponse>, ApiError> {
    let offerings = state.seed_bank.list_offerings().await?;
    Ok(Json(OfferingsListResponse { offerings }))
}

/// GET /api/v1/cultivation/offerings/{offering_id}/backups
pub async fn list_backups(
    State(state): State<AppState>,
    Path(offering_id): Path<String>,
) -> Result<Json<BackupsListResponse>, ApiError> {
    let offering_id = OfferingId::parse(&offering_id)?;
    let backups = state.seed_bank.list_backups(&offering_id).await?;
    Ok(Json(BackupsListResponse { offering_id, backups }))
}

/// GET /api/v1/cultivation/offerings/{offering_id}/backups/latest
pub async fn get_latest_backup(
    State(state): State<AppState>,
    Path(offering_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let offering_id = OfferingId::parse(&offering_id)?;

    let backup = state.seed_bank.get_latest(&offering_id).await?
        .ok_or(ApiError::NotFound)?;

    // Return multipart response with manifest + data
    let boundary = "----ZenGardenBoundary";
    let body = format_multipart(&backup.manifest, &backup.data, boundary);

    Ok((
        [(header::CONTENT_TYPE, format!("multipart/form-data; boundary={}", boundary))],
        body
    ))
}

/// DELETE /api/v1/cultivation/offerings/{offering_id}/prune
pub async fn prune_backups(
    State(state): State<AppState>,
    Path(offering_id): Path<String>,
    Query(params): Query<PruneParams>,
) -> Result<Json<PruneResult>, ApiError> {
    let offering_id = OfferingId::parse(&offering_id)?;

    let retention = RetentionPolicy {
        daily: params.keep_daily.unwrap_or(7),
        weekly: params.keep_weekly.unwrap_or(4),
        monthly: params.keep_monthly.unwrap_or(6),
    };

    let result = state.seed_bank.prune(&offering_id, &retention).await?;
    Ok(Json(result))
}
```

**Acceptance Criteria**:
- [ ] All 6 cultivation endpoints functional
- [ ] Multipart upload/download works correctly
- [ ] Prune respects retention policy (daily/weekly/monthly)
- [ ] Integration tests with mock seed bank

---

### Phase 3: Recovery System (3-4 days)

**Goal**: Restore offerings from seed banks

#### 3.1 Recovery Domain

**Files**: `src/moss/src/domain/recovery.rs` (new)

```rust
pub struct RecoveryService {
    seed_bank: Arc<dyn SeedBankAccess>,
    offering_service: Arc<OfferingService>,
    container_runtime: Arc<dyn ContainerRuntime>,
}

impl RecoveryService {
    /// Recover full garden from seed bank
    pub async fn recover_garden(
        &self,
        target_stone: Option<&str>,
        filters: RecoveryFilters,
    ) -> Result<RecoveryReport> {
        let mut report = RecoveryReport::new();

        // Read seed bank index
        let offerings = self.seed_bank.list_offerings().await?;

        for offering_record in offerings {
            // Apply filters
            if !filters.matches(&offering_record) {
                continue;
            }

            match self.recover_offering(&offering_record.offering_id, None).await {
                Ok(result) => report.succeeded(offering_record.offering_id, result),
                Err(e) => report.failed(offering_record.offering_id, e),
            }
        }

        Ok(report)
    }

    /// Recover single offering
    pub async fn recover_offering(
        &self,
        offering_id: &OfferingId,
        new_name: Option<String>,
    ) -> Result<RecoveryResult> {
        info!("Recovering offering {}", offering_id);

        // 1. Get latest backup
        let backup = self.seed_bank.get_latest(offering_id).await?
            .ok_or(RecoveryError::NoBackupFound)?;

        // 2. Verify checksum
        let computed = compute_checksum(&backup.data);
        if computed != backup.manifest.snapshot.checksum {
            return Err(RecoveryError::ChecksumMismatch);
        }

        // 3. Determine identity
        let (use_id, use_name) = if let Some(name) = new_name {
            // Fork: new identity
            (OfferingId::new(), name)
        } else {
            // Recovery: preserve identity
            (offering_id.clone(), backup.manifest.offering_name.clone())
        };

        // 4. Plant offering wishfully
        let offering = self.offering_service.plant_wishfully(
            &backup.manifest.offering_type,
            &use_name,
            Some(&use_id),
            &backup.manifest.container,
        ).await?;

        // 5. Restore data before starting
        self.restore_data(&offering, &backup).await?;

        // 6. Start and verify health
        self.offering_service.start(&offering).await?;
        self.offering_service.wait_healthy(&offering, Duration::from_secs(60)).await?;

        info!("Recovered {} as {}", offering_id, use_name);

        Ok(RecoveryResult {
            offering_id: use_id,
            offering_name: use_name,
            backup_timestamp: backup.manifest.source.timestamp.clone(),
        })
    }

    async fn restore_data(&self, offering: &Offering, backup: &Backup) -> Result<()> {
        match &backup.manifest.restore {
            Some(restore_config) => {
                // Custom restore command
                let exit_code = self.container_runtime.exec(
                    &offering.container_id,
                    &restore_config.command,
                    Some(Duration::from_secs(restore_config.timeout_seconds as u64)),
                ).await?;

                if exit_code != 0 {
                    return Err(RecoveryError::RestoreCommandFailed(exit_code));
                }
            }
            None => {
                // Default: extract tar.gz to volume
                let volume_path = self.get_volume_path(offering)?;
                self.extract_tar_gz(&backup.data, &volume_path).await?;
            }
        }

        Ok(())
    }
}
```

#### 3.2 Wishful Planting

**Files**: `src/moss/src/domain/offerings.rs` (extend)

```rust
impl OfferingService {
    /// Plant offering with preserved or specific identity
    pub async fn plant_wishfully(
        &self,
        offering_type: &str,
        name: &str,
        wishful_id: Option<&OfferingId>,
        container_config: &ContainerConfig,
    ) -> Result<Offering> {
        // Use provided ID or generate new
        let offering_id = wishful_id
            .cloned()
            .unwrap_or_else(OfferingId::new);

        // Create offering with specified identity
        let offering = Offering {
            id: offering_id,
            name: name.to_string(),
            offering_type: offering_type.to_string(),
            container_id: None,
            state: OfferingState::Creating,
            created_at: Utc::now(),
        };

        // Pull image
        self.container_runtime.pull(&container_config.image).await?;

        // Create container
        let container_id = self.container_runtime.create(
            &offering.name,
            container_config,
        ).await?;

        // Note: Don't start yet - caller may need to restore data first

        Ok(Offering { container_id: Some(container_id), ..offering })
    }
}
```

**Acceptance Criteria**:
- [ ] Full garden recovery restores all offerings
- [ ] Single offering recovery works by name or ID
- [ ] Fork creates new identity with copied data
- [ ] Checksum verification catches corruption
- [ ] Restore falls back to previous backup on failure

---

### Phase 4: Rake Commands (3-4 days)

**Goal**: CLI commands for cultivation and recovery

#### 4.1 Cultivate Commands

**Files**: `src/rake/src/commands/cultivate.rs` (new)

```rust
#[derive(Debug, Clone)]
pub enum CultivateCommand {
    /// Backup all offerings
    All,
    /// Backup specific offering
    One { name: String },
    /// Show cultivation status
    Status,
    /// Show backup history
    History { name: String },
    /// Prune old backups
    Prune { keep_daily: u32, keep_weekly: u32, keep_monthly: u32 },
}

impl CultivateCommand {
    pub async fn execute(&self, client: &Client, quiet: bool) -> Result<()> {
        match self {
            CultivateCommand::All => {
                if !quiet {
                    println!("🌱 Cultivating all offerings...");
                }

                let response = client
                    .post("/api/v1/cultivation/cultivate")
                    .send()
                    .await?;

                let report: CultivationReport = response.json().await?;

                println!("\nCultivation complete:");
                println!("  Succeeded: {}", report.succeeded.len());
                println!("  Failed: {}", report.failed.len());

                for (id, record) in &report.succeeded {
                    println!("  ✅ {} -> {} bytes", id, record.size_bytes);
                }

                for (id, error) in &report.failed {
                    println!("  ❌ {} -> {}", id, error);
                }
            }

            CultivateCommand::Status => {
                let response = client
                    .get("/api/v1/cultivation/status")
                    .send()
                    .await?;

                let status: CultivationStatus = response.json().await?;

                println!("Cultivation Status");
                println!("──────────────────");
                println!("Last backup: {}", status.last_backup.unwrap_or("never".into()));
                println!("Next scheduled: {}", status.next_scheduled);
                println!("Seed bank: {}", status.seed_bank.unwrap_or("none".into()));
                println!("Offerings backed up: {}", status.offering_count);
            }

            // ... other variants
        }

        Ok(())
    }
}
```

#### 4.2 Recover Commands

**Files**: `src/rake/src/commands/recover.rs` (new)

```rust
#[derive(Debug, Clone)]
pub enum RecoverCommand {
    /// Recover full garden
    Garden {
        from: Option<String>,
        at: Option<String>,
        only: Option<Vec<String>>,
        exclude: Option<Vec<String>>,
        dry_run: bool,
    },
    /// Recover single offering
    One {
        name: String,
        at: Option<String>,
        as_name: Option<String>,  // Fork with new name
    },
    /// Recover by offering ID
    ById {
        id: String,
        at: Option<String>,
    },
}

impl RecoverCommand {
    pub async fn execute(&self, client: &Client, quiet: bool) -> Result<()> {
        match self {
            RecoverCommand::Garden { from, dry_run, .. } => {
                if !quiet {
                    println!("🌱 Recovering garden{}...",
                        if *dry_run { " (dry run)" } else { "" });
                }

                // List what would be recovered
                let offerings = client
                    .get("/api/v1/cultivation/offerings")
                    .send()
                    .await?
                    .json::<OfferingsListResponse>()
                    .await?;

                println!("\nOfferings to recover:");
                for offering in &offerings.offerings {
                    println!("  {} ({}) - last backup: {}",
                        offering.name,
                        offering.offering_type,
                        offering.latest);
                }

                if *dry_run {
                    println!("\nDry run complete. No changes made.");
                    return Ok(());
                }

                // Confirm
                print!("\nProceed with recovery? [y/N] ");
                io::stdout().flush()?;

                let mut input = String::new();
                io::stdin().read_line(&mut input)?;

                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Cancelled.");
                    return Ok(());
                }

                // Execute recovery
                let response = client
                    .post("/api/v1/cultivation/recover")
                    .json(&RecoverRequest { /* ... */ })
                    .send()
                    .await?;

                let report: RecoveryReport = response.json().await?;

                println!("\nRecovery complete:");
                for (id, result) in &report.succeeded {
                    println!("  ✅ {} recovered as {}", id, result.offering_name);
                }
                for (id, error) in &report.failed {
                    println!("  ❌ {} failed: {}", id, error);
                }
            }

            RecoverCommand::One { name, as_name, .. } => {
                let action = if as_name.is_some() { "Forking" } else { "Recovering" };
                println!("🌱 {} {}...", action, name);

                // ... implementation
            }

            // ... other variants
        }

        Ok(())
    }
}
```

#### 4.3 Wishfully Keyword Parsing

**Files**: `src/rake/src/parser.rs` (extend)

```rust
pub struct ParsedKeywords {
    // ... existing fields ...
    pub wishfully: bool,
    pub wishful_id: Option<String>,  // Extracted from "name:id" syntax
}

// In parse_zen_keywords:
"wishfully" if *style == CommandStyle::Zen => {
    keywords.wishfully = true;
}

// Parse "name:id" syntax
fn parse_wishful_target(arg: &str) -> (String, Option<String>) {
    if let Some((name, id)) = arg.split_once(':') {
        (name.to_string(), Some(id.to_string()))
    } else {
        (arg.to_string(), None)
    }
}
```

**Acceptance Criteria**:
- [ ] `garden-rake cultivate` backs up all offerings
- [ ] `garden-rake cultivate mongo` backs up specific offering
- [ ] `garden-rake cultivation status` shows backup state
- [ ] `garden-rake recover garden` restores all offerings
- [ ] `garden-rake recover mongo --at stone-02` restores to specific stone
- [ ] `garden-rake recover mongo as mongo-copy` forks with new identity
- [ ] `garden-rake offer X wishfully as name:id` preserves identity

---

### Phase 5: Background Tasks (2-3 days)

**Goal**: Automatic scheduled cultivation

#### 5.1 Cultivation Scheduler

**Files**: `src/moss/src/tasks/cultivation_scheduler.rs` (new)

```rust
pub struct CultivationScheduler {
    cultivation_service: Arc<CultivationService>,
    schedule: cron::Schedule,
    enabled: bool,
}

impl CultivationScheduler {
    pub fn new(config: &CultivationConfig) -> Result<Self> {
        let schedule = cron::Schedule::from_str(&config.schedule)?;

        Ok(Self {
            cultivation_service: Arc::new(CultivationService::new(config)?),
            schedule,
            enabled: config.enabled,
        })
    }

    pub async fn run(&self, mut shutdown: broadcast::Receiver<()>) {
        if !self.enabled {
            info!("Cultivation scheduler disabled");
            return;
        }

        info!("Cultivation scheduler started, schedule: {}", self.schedule);

        loop {
            // Calculate next run time
            let next = self.schedule.upcoming(Utc).next().unwrap();
            let duration = (next - Utc::now()).to_std().unwrap_or(Duration::from_secs(60));

            tokio::select! {
                _ = tokio::time::sleep(duration) => {
                    info!("Starting scheduled cultivation");

                    match self.cultivation_service.cultivate_all().await {
                        Ok(report) => {
                            info!("Cultivation complete: {} succeeded, {} failed",
                                report.succeeded.len(), report.failed.len());
                        }
                        Err(e) => {
                            error!("Cultivation failed: {}", e);
                        }
                    }
                }
                _ = shutdown.recv() => {
                    info!("Cultivation scheduler shutting down");
                    break;
                }
            }
        }
    }
}
```

#### 5.2 Seed Bank Monitor

**Files**: `src/moss/src/tasks/seed_bank_monitor.rs` (new)

```rust
pub struct SeedBankMonitor {
    config: CultivationConfig,
    announcer: Arc<MdnsAnnouncer>,
}

impl SeedBankMonitor {
    pub async fn run(&self, mut shutdown: broadcast::Receiver<()>) {
        let mut interval = tokio::time::interval(Duration::from_secs(30));

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.check_seed_banks().await;
                }
                _ = shutdown.recv() => {
                    info!("Seed bank monitor shutting down");
                    break;
                }
            }
        }
    }

    async fn check_seed_banks(&self) {
        for seed_bank_config in &self.config.seed_banks {
            match seed_bank_config {
                SeedBankConfig::Path { path, announce } if *announce => {
                    let available = path.exists() && path.is_dir();

                    if available {
                        // Announce capability
                        self.announcer.add_capability("cultivation:seed-bank").await;
                    } else {
                        // Remove announcement
                        self.announcer.remove_capability("cultivation:seed-bank").await;
                    }
                }
                _ => {}
            }
        }
    }
}
```

**Acceptance Criteria**:
- [ ] Backups run on configured cron schedule
- [ ] USB mount/unmount detected within 30 seconds
- [ ] mDNS announces cultivation:seed-bank when available
- [ ] Graceful shutdown of background tasks

---

### Phase 6: Integration & Testing (3-4 days)

**Goal**: End-to-end testing and documentation

#### 6.1 Integration Tests

```rust
#[tokio::test]
async fn test_full_cultivation_cycle() {
    let garden = TestGarden::spawn().await;

    // Plant offering
    garden.rake("offer mongodb as mongo-test").await;
    garden.wait_healthy("mongo-test").await;

    // Insert test data
    garden.exec_in("mongo-test", &["mongosh", "--eval", "db.test.insert({x:1})"]).await;

    // Cultivate
    garden.rake("cultivate mongo-test").await;

    // Verify backup exists
    let status = garden.rake("cultivation status").await;
    assert!(status.contains("mongo-test"));

    // Destroy offering
    garden.rake("uproot mongo-test").await;

    // Recover
    garden.rake("recover mongo-test").await;
    garden.wait_healthy("mongo-test").await;

    // Verify data restored
    let result = garden.exec_in("mongo-test", &["mongosh", "--eval", "db.test.count()"]).await;
    assert!(result.contains("1"));
}

#[tokio::test]
async fn test_placement_with_real_metrics() {
    let garden = TestGarden::spawn_multi_stone(3).await;

    // Generate different loads on stones
    garden.stress_stone("stone-1", CpuStress::High).await;
    garden.stress_stone("stone-2", CpuStress::Low).await;
    garden.stress_stone("stone-3", CpuStress::Medium).await;

    // Request placement
    let recommendations = garden.api_call::<PlacementResponse>(
        "POST /api/v1/garden/recommend",
        json!({"offering": "redis", "top_n": 3})
    ).await;

    // Verify CPU scores differ
    let cpu_scores: Vec<i32> = recommendations.recommendations
        .iter()
        .map(|r| r.breakdown.cpu)
        .collect();

    assert!(cpu_scores[0] != cpu_scores[1] || cpu_scores[1] != cpu_scores[2],
        "CPU scores should differ: {:?}", cpu_scores);

    // Low-stress stone should rank highest
    assert_eq!(recommendations.recommendations[0].hostname, "stone-2");
}
```

#### 6.2 Documentation Updates

Create or update:

- [ ] `docs/guides/cultivation-quickstart.md` - Getting started with backups
- [ ] `docs/guides/seed-bank-setup.md` - Configuring seed banks (USB, NAS)
- [ ] `docs/guides/disaster-recovery.md` - Full recovery scenarios
- [ ] `docs/reference/cultivation-api.md` - API endpoint reference
- [ ] `docs/reference/cultivation-config.md` - Configuration reference
- [ ] Update `docs/reference/cli.md` - New commands

---

## Milestone Summary

| Phase | Duration | Deliverables |
|-------|----------|--------------|
| **Phase 0: Foundation** | 2-3 days | Real metrics, identity model, manifest schema |
| **Phase 1: Seed Bank** | 3-4 days | Storage abstraction, config, access layer |
| **Phase 2: Cultivation** | 4-5 days | Snapshot execution, API endpoints |
| **Phase 3: Recovery** | 3-4 days | Recovery service, wishful planting |
| **Phase 4: Rake CLI** | 3-4 days | cultivate, recover, seed-bank commands |
| **Phase 5: Background** | 2-3 days | Scheduler, mount monitor |
| **Phase 6: Integration** | 3-4 days | Tests, documentation |
| **Total** | **~20-27 days** | Full garden resilience |

---

## Risk Mitigation

### Technical Risks

| Risk | Mitigation |
|------|------------|
| Large backup files overwhelm network | Chunked uploads, compression, local staging |
| Concurrent writes corrupt index | File locking with timeout, atomic updates |
| Container exec timeout during snapshot | Configurable timeouts, graceful cancellation |
| USB removal during backup | Write to staging first, sync to seed bank |

### Operational Risks

| Risk | Mitigation |
|------|------------|
| Users forget to configure seed banks | Default to local staging, warn on status |
| Retention policy deletes needed backups | Conservative defaults (7/4/6), manual prune |
| Recovery to wrong stone | Confirmation prompt, dry-run mode |

---

## Success Criteria

### Phase 0 Complete When:
- [ ] Placement shows real CPU percentages (not 0%)
- [ ] Storage scores reflect actual disk usage
- [ ] OfferingId generates and parses correctly

### Phase 1-2 Complete When:
- [ ] `garden-rake cultivate` creates backups in seed bank
- [ ] Seed bank directory structure matches spec
- [ ] All cultivation API endpoints return valid responses

### Phase 3-4 Complete When:
- [ ] `garden-rake recover garden` restores all offerings
- [ ] Recovered offerings have their data intact
- [ ] Fork creates independent copy with new ID

### Phase 5-6 Complete When:
- [ ] Backups run automatically on schedule
- [ ] USB seed bank detected on mount
- [ ] All integration tests pass
- [ ] Documentation complete

---

## Implementation Notes for AI Agents

### Context Files to Read First

1. `src/common/src/manifests/offering.rs` - Understand current manifest schema
2. `src/moss/src/infra/config.rs` - Understand configuration patterns
3. `src/moss/src/domain/placement.rs` - Example of domain orchestration
4. `src/moss/src/api/v1/garden.rs` - Example of API endpoint patterns
5. `src/rake/src/commands/offering/mod.rs` - Example of CLI command patterns

### Coding Conventions

- Use `tokio` for async runtime
- Use `serde` with YAML for manifests, JSON for API
- Use `tracing` for logging (`info!`, `warn!`, `error!`)
- Use `thiserror` for error types
- Follow existing module organization patterns

### Testing Approach

- Unit tests for pure functions (scoring, parsing)
- Integration tests with `TestGarden` harness
- Mock external dependencies (container runtime, filesystem)

### Dependencies to Add

```toml
# Cargo.toml additions
cron = "0.12"           # Cron schedule parsing
sha2 = "0.10"           # Checksum computation
flate2 = "1.0"          # Gzip compression
tar = "0.4"             # Tar archive creation
```

---

## References

- [Intelligent Offering Placement Spec](intelligent-offering-placement.md)
- [Intelligent Offering Placement Delta](intelligent-offering-placement-delta.md)
- [Cultivation Spec](zen-garden-spec-cultivation.md)
- [Topology Caching Spec](zen-garden-spec-topology-caching.md)
- [Technical Spec](zen-garden-technical-spec.md)
