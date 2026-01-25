# Nourishment Implementation Plan

**Actionable development guide with testable milestones**

**Status:** Ready for Implementation
**Date:** January 2026
**Test Environment:** 3x Dell Wyse 5070 thin clients

---

## Current Codebase State

### What Exists (Reusable)

| Component | Location | Status |
|-----------|----------|--------|
| Job tracking | `app_state.rs:44-54` | Ready - Job struct with status tracking |
| Docker operations | `docker.rs` | Ready - start/stop/remove/upgrade |
| Event emission | `api/v1/events.rs` | Ready - SSE + Console events |
| Service registry | `app_state.rs` | Ready - upsert/remove with persistence |
| Template parsing | `infra/manifests/sw.rs` | Ready - needs ceremony field |
| Stone discovery | `announcement.rs` | Ready - UDP chirp |
| API patterns | `api/v1/*.rs` | Ready - follow existing |
| CLI patterns | `rake/commands/*.rs` | Ready - follow existing |

### What Needs Building

| Component | Priority | Complexity |
|-----------|----------|------------|
| CeremonyPolicy types | P0 | Low |
| Ceremony registry | P0 | Medium |
| Docker commit | P1 | Low |
| Volume archiver | P1 | Medium |
| Harvest store | P1 | Low |
| Ceremony executor | P1 | High |
| Nourish phases | P2 | Medium |
| Transfer protocol | P3 | High |
| Vacate ceremony | P3 | High |

---

## Phase 0: Foundation Types

**Goal:** All types compile, tests pass

**Duration:** ~2 hours

### Task 0.1: Ceremony Policy Types in garden-common

**File:** `src/common/src/manifests/ceremony.rs` (NEW)

```rust
//! Ceremony policy types for offering lifecycle management

use serde::{Deserialize, Serialize};

/// Ceremony mode determines snapshot strategy
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CeremonyMode {
    /// Must stop container before snapshot (default, safest)
    #[default]
    Unsafe,
    /// Can freeze/thaw without stopping (databases with fsync)
    Quiesceable,
    /// No persistent data, commit anytime
    Stateless,
}

/// Command execution configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecConfig {
    pub exec: Vec<String>,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u32,
}

fn default_timeout() -> u32 { 30 }

/// Rollback behavior configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackConfig {
    #[serde(default = "default_true")]
    pub automatic: bool,
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,
    #[serde(default = "default_true")]
    pub preserve_harvest: bool,
    #[serde(default = "default_retention")]
    pub harvest_retention: String,
}

fn default_true() -> bool { true }
fn default_max_attempts() -> u32 { 2 }
fn default_retention() -> String { "168h".to_string() }

impl Default for RollbackConfig {
    fn default() -> Self {
        Self {
            automatic: true,
            max_attempts: 2,
            preserve_harvest: true,
            harvest_retention: "168h".to_string(),
        }
    }
}

/// Ceremony policy for an offering
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CeremonyPolicy {
    #[serde(default)]
    pub mode: CeremonyMode,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub quiesce: Option<ExecConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume: Option<ExecConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub verify: Option<ExecConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_quiesce_seconds: Option<u32>,

    #[serde(default)]
    pub rollback: RollbackConfig,
}

impl CeremonyPolicy {
    /// Validate policy configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.mode == CeremonyMode::Quiesceable {
            if self.quiesce.is_none() {
                return Err("Quiesceable mode requires quiesce command".to_string());
            }
            if self.resume.is_none() {
                return Err("Quiesceable mode requires resume command".to_string());
            }
        }
        Ok(())
    }
}
```

**File:** `src/common/src/manifests/mod.rs` (UPDATE)

```rust
mod ceremony;
pub use ceremony::{CeremonyMode, CeremonyPolicy, ExecConfig, RollbackConfig};
```

### Task 0.2: Ceremony Core Types in Moss

**File:** `src/moss/src/domain/ceremony/types.rs` (NEW)

```rust
//! Ceremony core types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique ceremony identifier
pub type CeremonyId = String;

/// Ceremony type variants
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CeremonyType {
    NourishOffering { offering: String },
    NourishStone { stone: String },
    NourishAll,
    Vacate { stone: String },
    Replant { offering: String, from: String, to: String },
    Store { offering: String },
}

impl CeremonyType {
    pub fn name(&self) -> &'static str {
        match self {
            Self::NourishOffering { .. } => "nourish-offering",
            Self::NourishStone { .. } => "nourish-stone",
            Self::NourishAll => "nourish-all",
            Self::Vacate { .. } => "vacate",
            Self::Replant { .. } => "replant",
            Self::Store { .. } => "store",
        }
    }
}

/// Ceremony lifecycle state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CeremonyState {
    Initiated,
    Planning,
    Executing,
    Completed,
    Failed,
    RolledBack,
    Cancelled,
}

impl CeremonyState {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::RolledBack | Self::Cancelled)
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Self::Initiated | Self::Planning | Self::Executing)
    }
}

/// Phase state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PhaseState {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}

/// A phase in a ceremony
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Phase {
    pub name: String,
    pub state: PhaseState,
    pub jobs: Vec<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

impl Phase {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            state: PhaseState::Pending,
            jobs: Vec::new(),
            started_at: None,
            completed_at: None,
            error: None,
        }
    }
}

/// Ceremony options
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CeremonyOptions {
    pub recklessly: bool,
    pub dry_run: bool,
    pub auto_rollback: bool,
}

/// Ceremony initiator info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CeremonyInitiator {
    pub source: String,
    pub stone_id: Option<String>,
    pub command: Option<String>,
}

/// A ceremony instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ceremony {
    pub id: CeremonyId,
    pub ceremony_type: CeremonyType,
    pub state: CeremonyState,
    pub coordinator: String,
    pub participants: Vec<String>,

    pub phases: Vec<Phase>,
    pub current_phase: usize,

    pub initiated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,

    pub initiator: CeremonyInitiator,
    pub options: CeremonyOptions,

    /// Artifacts created (harvest IDs, stored offering IDs)
    pub artifacts: HashMap<String, String>,

    /// Error details if failed
    pub error: Option<String>,
}

impl Ceremony {
    pub fn new(
        ceremony_type: CeremonyType,
        coordinator: String,
        initiator: CeremonyInitiator,
        options: CeremonyOptions,
    ) -> Self {
        let id = format!(
            "{}-{}-{}",
            ceremony_type.name(),
            coordinator.chars().take(8).collect::<String>(),
            chrono::Utc::now().format("%Y%m%d%H%M%S")
        );

        Self {
            id,
            ceremony_type,
            state: CeremonyState::Initiated,
            coordinator,
            participants: Vec::new(),
            phases: Vec::new(),
            current_phase: 0,
            initiated_at: Utc::now(),
            started_at: None,
            completed_at: None,
            initiator,
            options,
            artifacts: HashMap::new(),
            error: None,
        }
    }

    pub fn current_phase(&self) -> Option<&Phase> {
        self.phases.get(self.current_phase)
    }

    pub fn progress_percent(&self) -> u8 {
        if self.phases.is_empty() {
            return 0;
        }
        let completed = self.phases.iter()
            .filter(|p| p.state == PhaseState::Completed)
            .count();
        ((completed * 100) / self.phases.len()) as u8
    }
}
```

**File:** `src/moss/src/domain/ceremony/mod.rs` (NEW)

```rust
//! Ceremony domain module
//!
//! Orchestrates multi-phase, long-running operations.

mod types;

pub use types::*;
```

**File:** `src/moss/src/domain/mod.rs` (UPDATE - add)

```rust
pub mod ceremony;
pub use ceremony::{Ceremony, CeremonyId, CeremonyType, CeremonyState, Phase, PhaseState};
```

### Task 0.3: Harvest Manifest Types

**File:** `src/moss/src/domain/harvest.rs` (NEW)

```rust
//! Harvest types - backup artifacts for offerings

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Harvest identifier
pub type HarvestId = String;

/// Volume archive info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeArchive {
    pub name: String,
    pub container_path: String,
    pub archive_path: String,
    pub size_bytes: u64,
    pub checksum: String,
}

/// Harvest manifest - saved alongside archives
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarvestManifest {
    pub id: HarvestId,
    pub offering: String,
    pub created_at: DateTime<Utc>,
    pub source_stone: String,

    pub original_image: String,
    pub committed_image: Option<String>,

    pub volumes: Vec<VolumeArchive>,

    pub ceremony_id: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl HarvestManifest {
    pub fn new(offering: &str, source_stone: &str, original_image: &str) -> Self {
        let id = format!(
            "{}-{}",
            offering,
            chrono::Utc::now().format("%Y%m%dT%H%M%S")
        );

        Self {
            id,
            offering: offering.to_string(),
            created_at: Utc::now(),
            source_stone: source_stone.to_string(),
            original_image: original_image.to_string(),
            committed_image: None,
            volumes: Vec::new(),
            ceremony_id: None,
            expires_at: None,
        }
    }

    pub fn total_size_bytes(&self) -> u64 {
        self.volumes.iter().map(|v| v.size_bytes).sum()
    }
}
```

### Gate 0: Compilation Check

```bash
cd src/moss && cargo check
cd src/common && cargo check
cargo test -p garden-common
cargo test -p garden-moss --lib
```

**Expected:** All compile, no test failures

---

## Phase 1: Harvest Infrastructure

**Goal:** Can backup and restore a single offering's volumes

**Duration:** ~4 hours

### Task 1.1: Harvest Paths

**File:** `src/moss/src/infra/paths.rs` (UPDATE or NEW)

```rust
/// Harvest storage directory
#[cfg(target_os = "windows")]
pub const HARVEST_DIR: &str = "C:\\ProgramData\\ZenGarden\\harvests";

#[cfg(not(target_os = "windows"))]
pub const HARVEST_DIR: &str = "/var/lib/zen-garden/harvests";

/// Stored offerings directory
#[cfg(target_os = "windows")]
pub const STORED_DIR: &str = "C:\\ProgramData\\ZenGarden\\stored";

#[cfg(not(target_os = "windows"))]
pub const STORED_DIR: &str = "/var/lib/zen-garden/stored";

/// Ceremony journal directory
#[cfg(target_os = "windows")]
pub const CEREMONY_JOURNAL_DIR: &str = "C:\\ProgramData\\ZenGarden\\ceremonies";

#[cfg(not(target_os = "windows"))]
pub const CEREMONY_JOURNAL_DIR: &str = "/var/lib/zen-garden/ceremonies";
```

### Task 1.2: Docker Commit Wrapper

**File:** `src/moss/src/docker.rs` (UPDATE - add method)

```rust
impl DockerManager {
    /// Commit a container to a new image
    ///
    /// Returns the created image ID.
    pub async fn commit_container(
        &self,
        container_name: &str,
        repo: &str,
        tag: &str,
        pause: bool,
    ) -> Result<String> {
        use bollard::image::CommitContainerOptions;
        use bollard::container::Config;

        let options = CommitContainerOptions {
            container: container_name,
            repo,
            tag,
            pause,
            ..Default::default()
        };

        let config = Config::<String>::default();

        let result = self.docker.commit_container(options, config).await
            .context(format!("Failed to commit container {}", container_name))?;

        Ok(result.id.unwrap_or_default())
    }

    /// Export a container to a tarball
    pub async fn export_container(&self, container_name: &str) -> Result<impl futures::Stream<Item = Result<bytes::Bytes, bollard::errors::Error>>> {
        Ok(self.docker.export_container(container_name))
    }

    /// Get volume mounts for a container
    pub async fn get_container_volumes(&self, container_name: &str) -> Result<Vec<(String, String)>> {
        let info = self.docker.inspect_container(container_name, None).await
            .context("Failed to inspect container")?;

        let mounts = info.mounts.unwrap_or_default();

        Ok(mounts.iter()
            .filter_map(|m| {
                let source = m.source.as_ref()?;
                let dest = m.destination.as_ref()?;
                Some((source.clone(), dest.clone()))
            })
            .collect())
    }
}
```

### Task 1.3: Volume Archiver

**File:** `src/moss/src/infra/backup.rs` (NEW)

```rust
//! Volume backup and restore utilities

use anyhow::{Context, Result};
use std::path::Path;
use tokio::process::Command;

/// Archive a directory to a compressed tarball
pub async fn archive_directory(
    source_path: &Path,
    archive_path: &Path,
) -> Result<u64> {
    // Ensure parent directory exists
    if let Some(parent) = archive_path.parent() {
        tokio::fs::create_dir_all(parent).await
            .context("Failed to create archive directory")?;
    }

    // Use tar with zstd compression
    let status = Command::new("tar")
        .args([
            "-I", "zstd",
            "-cf",
            archive_path.to_str().unwrap(),
            "-C",
            source_path.parent().unwrap().to_str().unwrap(),
            source_path.file_name().unwrap().to_str().unwrap(),
        ])
        .status()
        .await
        .context("Failed to run tar")?;

    if !status.success() {
        anyhow::bail!("tar command failed with status: {}", status);
    }

    // Get archive size
    let metadata = tokio::fs::metadata(archive_path).await
        .context("Failed to get archive metadata")?;

    Ok(metadata.len())
}

/// Restore a directory from a compressed tarball
pub async fn restore_directory(
    archive_path: &Path,
    target_path: &Path,
) -> Result<()> {
    // Ensure target directory exists
    tokio::fs::create_dir_all(target_path).await
        .context("Failed to create target directory")?;

    let status = Command::new("tar")
        .args([
            "-I", "zstd",
            "-xf",
            archive_path.to_str().unwrap(),
            "-C",
            target_path.to_str().unwrap(),
        ])
        .status()
        .await
        .context("Failed to run tar")?;

    if !status.success() {
        anyhow::bail!("tar extract failed with status: {}", status);
    }

    Ok(())
}

/// Calculate blake3 checksum of a file
pub async fn calculate_checksum(path: &Path) -> Result<String> {
    let data = tokio::fs::read(path).await
        .context("Failed to read file for checksum")?;

    let hash = blake3::hash(&data);
    Ok(format!("blake3:{}", hash.to_hex()))
}

/// Verify checksum matches
pub async fn verify_checksum(path: &Path, expected: &str) -> Result<bool> {
    let actual = calculate_checksum(path).await?;
    Ok(actual == expected)
}
```

### Task 1.4: Harvest Store

**File:** `src/moss/src/infra/harvest_store.rs` (NEW)

```rust
//! Harvest storage and retrieval

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use crate::domain::harvest::{HarvestId, HarvestManifest};

pub struct HarvestStore {
    base_dir: PathBuf,
}

impl HarvestStore {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self { base_dir: base_dir.into() }
    }

    /// Get path for a harvest
    pub fn harvest_path(&self, id: &HarvestId) -> PathBuf {
        self.base_dir.join(id)
    }

    /// Get manifest path for a harvest
    pub fn manifest_path(&self, id: &HarvestId) -> PathBuf {
        self.harvest_path(id).join("manifest.json")
    }

    /// Save harvest manifest
    pub async fn save_manifest(&self, manifest: &HarvestManifest) -> Result<()> {
        let path = self.manifest_path(&manifest.id);

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await
                .context("Failed to create harvest directory")?;
        }

        let json = serde_json::to_string_pretty(manifest)
            .context("Failed to serialize manifest")?;

        tokio::fs::write(&path, json).await
            .context("Failed to write manifest")?;

        Ok(())
    }

    /// Load harvest manifest
    pub async fn load_manifest(&self, id: &HarvestId) -> Result<HarvestManifest> {
        let path = self.manifest_path(id);
        let json = tokio::fs::read_to_string(&path).await
            .context("Failed to read manifest")?;

        serde_json::from_str(&json)
            .context("Failed to parse manifest")
    }

    /// List all harvests
    pub async fn list_all(&self) -> Result<Vec<HarvestManifest>> {
        let mut manifests = Vec::new();

        if !self.base_dir.exists() {
            return Ok(manifests);
        }

        let mut entries = tokio::fs::read_dir(&self.base_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                let id = entry.file_name().to_string_lossy().to_string();
                if let Ok(manifest) = self.load_manifest(&id).await {
                    manifests.push(manifest);
                }
            }
        }

        // Sort by creation time, newest first
        manifests.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(manifests)
    }

    /// List harvests for a specific offering
    pub async fn list_for_offering(&self, offering: &str) -> Result<Vec<HarvestManifest>> {
        let all = self.list_all().await?;
        Ok(all.into_iter().filter(|m| m.offering == offering).collect())
    }

    /// Delete a harvest
    pub async fn delete(&self, id: &HarvestId) -> Result<()> {
        let path = self.harvest_path(id);
        if path.exists() {
            tokio::fs::remove_dir_all(&path).await
                .context("Failed to delete harvest")?;
        }
        Ok(())
    }

    /// Prune harvests older than duration
    pub async fn prune(&self, older_than: chrono::Duration) -> Result<usize> {
        let cutoff = chrono::Utc::now() - older_than;
        let mut pruned = 0;

        for manifest in self.list_all().await? {
            if manifest.created_at < cutoff {
                self.delete(&manifest.id).await?;
                pruned += 1;
            }
        }

        Ok(pruned)
    }
}
```

### Task 1.5: Harvest Creation Logic

**File:** `src/moss/src/domain/harvest.rs` (UPDATE - add functions)

```rust
use crate::docker::DockerManager;
use crate::infra::{backup, harvest_store::HarvestStore};
use anyhow::{Context, Result};
use std::path::Path;

/// Create a harvest for an offering
pub async fn create_harvest(
    docker: &DockerManager,
    store: &HarvestStore,
    offering: &str,
    source_stone: &str,
    commit_image: bool,
) -> Result<HarvestManifest> {
    let container_name = format!("zen-offering-{}", offering);

    // Get current image
    let original_image = docker.get_service_image(&container_name).await
        .context("Failed to get container image")?;

    let mut manifest = HarvestManifest::new(offering, source_stone, &original_image);
    let harvest_dir = store.harvest_path(&manifest.id);

    // Commit container if requested
    if commit_image {
        let repo = format!("zen-harvest/{}", offering);
        let tag = chrono::Utc::now().format("%Y%m%dT%H%M%S").to_string();

        let image_id = docker.commit_container(&container_name, &repo, &tag, true).await
            .context("Failed to commit container")?;

        manifest.committed_image = Some(format!("{}:{}", repo, tag));
        tracing::info!(offering, image_id, "Committed container image");
    }

    // Archive volumes
    let volumes = docker.get_container_volumes(&container_name).await?;
    let volumes_dir = harvest_dir.join("volumes");

    for (host_path, container_path) in volumes {
        let volume_name = Path::new(&container_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "data".to_string());

        let archive_name = format!("{}.tar.zst", volume_name);
        let archive_path = volumes_dir.join(&archive_name);

        let size = backup::archive_directory(
            Path::new(&host_path),
            &archive_path,
        ).await.context(format!("Failed to archive volume {}", volume_name))?;

        let checksum = backup::calculate_checksum(&archive_path).await?;

        manifest.volumes.push(VolumeArchive {
            name: volume_name,
            container_path,
            archive_path: archive_path.to_string_lossy().to_string(),
            size_bytes: size,
            checksum,
        });

        tracing::info!(volume = %volume_name, size, "Archived volume");
    }

    // Save manifest
    store.save_manifest(&manifest).await?;

    tracing::info!(
        harvest_id = %manifest.id,
        offering,
        total_size = manifest.total_size_bytes(),
        "Created harvest"
    );

    Ok(manifest)
}

/// Restore an offering from a harvest
pub async fn restore_harvest(
    docker: &DockerManager,
    store: &HarvestStore,
    harvest_id: &str,
) -> Result<()> {
    let manifest = store.load_manifest(&harvest_id.to_string()).await?;
    let container_name = format!("zen-offering-{}", manifest.offering);

    // Verify checksums
    for volume in &manifest.volumes {
        let valid = backup::verify_checksum(
            Path::new(&volume.archive_path),
            &volume.checksum,
        ).await?;

        if !valid {
            anyhow::bail!("Checksum mismatch for volume {}", volume.name);
        }
    }

    // Stop container if running
    let _ = docker.stop_service(&container_name).await;

    // Restore volumes
    let volumes = docker.get_container_volumes(&container_name).await?;

    for volume_archive in &manifest.volumes {
        // Find matching host path
        if let Some((host_path, _)) = volumes.iter()
            .find(|(_, cp)| *cp == volume_archive.container_path)
        {
            backup::restore_directory(
                Path::new(&volume_archive.archive_path),
                Path::new(host_path),
            ).await?;

            tracing::info!(volume = %volume_archive.name, "Restored volume");
        }
    }

    tracing::info!(harvest_id, offering = %manifest.offering, "Restored harvest");

    Ok(())
}
```

### Gate 1: Harvest Roundtrip Test

**File:** `src/moss/tests/harvest_test.rs` (NEW)

```rust
//! Integration test for harvest creation and restoration

use garden_moss::docker::DockerManager;
use garden_moss::domain::harvest::{create_harvest, restore_harvest};
use garden_moss::infra::harvest_store::HarvestStore;
use tempfile::TempDir;

#[tokio::test]
#[ignore] // Requires Docker
async fn test_harvest_roundtrip() {
    // Setup
    let docker = DockerManager::new().await.unwrap();
    let temp_dir = TempDir::new().unwrap();
    let store = HarvestStore::new(temp_dir.path());

    // Create a test container with a volume
    // (This would use a simple nginx or similar)

    // Create harvest
    let manifest = create_harvest(
        &docker,
        &store,
        "test-service",
        "test-stone",
        true,
    ).await.unwrap();

    assert!(!manifest.id.is_empty());
    assert_eq!(manifest.offering, "test-service");

    // List harvests
    let harvests = store.list_all().await.unwrap();
    assert_eq!(harvests.len(), 1);

    // Restore from harvest
    restore_harvest(&docker, &store, &manifest.id).await.unwrap();

    // Cleanup
    store.delete(&manifest.id).await.unwrap();
    assert!(store.list_all().await.unwrap().is_empty());
}
```

**Manual test (on Wyse machine):**

```bash
# 1. Start a test service
garden-rake offer nginx

# 2. Write some test data
docker exec zen-offering-nginx sh -c "echo 'test' > /usr/share/nginx/html/test.txt"

# 3. Create harvest (via debug API or test binary)
curl -X POST http://localhost:7185/api/v1/harvests/nginx

# 4. Verify harvest exists
curl http://localhost:7185/api/v1/harvests

# 5. Destroy and recreate container
garden-rake rest nginx
docker rm zen-offering-nginx
garden-rake offer nginx

# 6. Restore from harvest
curl -X POST http://localhost:7185/api/v1/harvests/nginx-TIMESTAMP/restore

# 7. Verify data restored
docker exec zen-offering-nginx cat /usr/share/nginx/html/test.txt
# Should output: test
```

---

## Phase 2: Ceremony Engine

**Goal:** Can execute and recover multi-phase ceremonies

**Duration:** ~6 hours

### Task 2.1: Ceremony Registry

**File:** `src/moss/src/domain/ceremony/registry.rs` (NEW)

```rust
//! Thread-safe ceremony registry

use super::types::*;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct CeremonyRegistry {
    ceremonies: RwLock<HashMap<CeremonyId, Ceremony>>,
}

impl CeremonyRegistry {
    pub fn new() -> Self {
        Self {
            ceremonies: RwLock::new(HashMap::new()),
        }
    }

    pub async fn insert(&self, ceremony: Ceremony) -> CeremonyId {
        let id = ceremony.id.clone();
        self.ceremonies.write().await.insert(id.clone(), ceremony);
        id
    }

    pub async fn get(&self, id: &CeremonyId) -> Option<Ceremony> {
        self.ceremonies.read().await.get(id).cloned()
    }

    pub async fn update(&self, ceremony: Ceremony) {
        self.ceremonies.write().await.insert(ceremony.id.clone(), ceremony);
    }

    pub async fn list_active(&self) -> Vec<Ceremony> {
        self.ceremonies.read().await
            .values()
            .filter(|c| c.state.is_active())
            .cloned()
            .collect()
    }

    pub async fn list_all(&self) -> Vec<Ceremony> {
        self.ceremonies.read().await.values().cloned().collect()
    }

    pub async fn remove(&self, id: &CeremonyId) -> Option<Ceremony> {
        self.ceremonies.write().await.remove(id)
    }
}

impl Default for CeremonyRegistry {
    fn default() -> Self {
        Self::new()
    }
}
```

### Task 2.2: Ceremony Journal (Persistence)

**File:** `src/moss/src/infra/ceremony_journal.rs` (NEW)

```rust
//! Persistent ceremony journal for crash recovery

use crate::domain::ceremony::{Ceremony, CeremonyId, CeremonyState};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub struct CeremonyJournal {
    dir: PathBuf,
}

impl CeremonyJournal {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn active_path(&self, id: &CeremonyId) -> PathBuf {
        self.dir.join("active").join(format!("{}.json", id))
    }

    fn archive_path(&self, id: &CeremonyId) -> PathBuf {
        self.dir.join("archive").join(format!("{}.json", id))
    }

    pub async fn persist(&self, ceremony: &Ceremony) -> Result<()> {
        let path = if ceremony.state.is_terminal() {
            self.archive_path(&ceremony.id)
        } else {
            self.active_path(&ceremony.id)
        };

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let json = serde_json::to_string_pretty(ceremony)?;
        tokio::fs::write(&path, json).await?;

        // If terminal, remove from active
        if ceremony.state.is_terminal() {
            let active = self.active_path(&ceremony.id);
            let _ = tokio::fs::remove_file(&active).await;
        }

        Ok(())
    }

    pub async fn load_active(&self) -> Result<Vec<Ceremony>> {
        let active_dir = self.dir.join("active");

        if !active_dir.exists() {
            return Ok(Vec::new());
        }

        let mut ceremonies = Vec::new();
        let mut entries = tokio::fs::read_dir(&active_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            if entry.path().extension().map(|e| e == "json").unwrap_or(false) {
                let json = tokio::fs::read_to_string(entry.path()).await?;
                if let Ok(ceremony) = serde_json::from_str::<Ceremony>(&json) {
                    ceremonies.push(ceremony);
                }
            }
        }

        Ok(ceremonies)
    }

    pub async fn prune_archive(&self, older_than: chrono::Duration) -> Result<usize> {
        let archive_dir = self.dir.join("archive");
        let cutoff = chrono::Utc::now() - older_than;
        let mut pruned = 0;

        if !archive_dir.exists() {
            return Ok(0);
        }

        let mut entries = tokio::fs::read_dir(&archive_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let json = tokio::fs::read_to_string(entry.path()).await?;
            if let Ok(ceremony) = serde_json::from_str::<Ceremony>(&json) {
                if ceremony.completed_at.map(|t| t < cutoff).unwrap_or(false) {
                    tokio::fs::remove_file(entry.path()).await?;
                    pruned += 1;
                }
            }
        }

        Ok(pruned)
    }
}
```

### Task 2.3: Ceremony Events

**File:** `src/common/src/events.rs` (UPDATE - add)

```rust
/// Ceremony-related events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CeremonyEvent {
    Started { ceremony_id: String, ceremony_type: String },
    PhaseStarted { ceremony_id: String, phase: String },
    PhaseCompleted { ceremony_id: String, phase: String },
    PhaseFailed { ceremony_id: String, phase: String, error: String },
    Progress { ceremony_id: String, percent: u8, message: String },
    Completed { ceremony_id: String },
    Failed { ceremony_id: String, error: String },
    RolledBack { ceremony_id: String },
    Cancelled { ceremony_id: String },
}
```

### Task 2.4: Add to AppState

**File:** `src/moss/src/app_state.rs` (UPDATE)

```rust
use crate::domain::ceremony::registry::CeremonyRegistry;
use crate::infra::{ceremony_journal::CeremonyJournal, harvest_store::HarvestStore};

pub struct AppState {
    // ... existing fields ...

    pub ceremony_registry: Arc<CeremonyRegistry>,
    pub ceremony_journal: Arc<CeremonyJournal>,
    pub harvest_store: Arc<HarvestStore>,
}

impl AppState {
    pub fn new(/* ... */) -> Self {
        // ... existing ...

        let ceremony_registry = Arc::new(CeremonyRegistry::new());
        let ceremony_journal = Arc::new(CeremonyJournal::new(paths::CEREMONY_JOURNAL_DIR));
        let harvest_store = Arc::new(HarvestStore::new(paths::HARVEST_DIR));

        Self {
            // ... existing ...
            ceremony_registry,
            ceremony_journal,
            harvest_store,
        }
    }

    /// Recover incomplete ceremonies on startup
    pub async fn recover_ceremonies(&self) -> Result<usize> {
        let incomplete = self.ceremony_journal.load_active().await?;
        let count = incomplete.len();

        for ceremony in incomplete {
            tracing::warn!(
                ceremony_id = %ceremony.id,
                state = ?ceremony.state,
                "Found incomplete ceremony from previous run"
            );
            self.ceremony_registry.insert(ceremony).await;
        }

        Ok(count)
    }
}
```

### Gate 2: Ceremony Recovery Test

```rust
#[tokio::test]
async fn test_ceremony_journal_recovery() {
    let temp_dir = TempDir::new().unwrap();
    let journal = CeremonyJournal::new(temp_dir.path());

    // Create a ceremony
    let ceremony = Ceremony::new(
        CeremonyType::NourishOffering { offering: "test".to_string() },
        "stone-01".to_string(),
        CeremonyInitiator { source: "test".to_string(), stone_id: None, command: None },
        CeremonyOptions::default(),
    );

    // Persist it
    journal.persist(&ceremony).await.unwrap();

    // Simulate crash by creating new journal
    let journal2 = CeremonyJournal::new(temp_dir.path());

    // Load active ceremonies
    let active = journal2.load_active().await.unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, ceremony.id);
}
```

---

## Phase 3: Nourish Offering Flow

**Goal:** Can safely update a single offering with rollback

**Duration:** ~8 hours

### Task 3.1: Nourish Phases

**File:** `src/moss/src/domain/ceremony/phases/mod.rs` (NEW)

```rust
//! Ceremony phase implementations

pub mod collect;
pub mod nourish;
pub mod water;
```

**File:** `src/moss/src/domain/ceremony/phases/collect.rs` (NEW)

```rust
//! Collect phase - create harvest before nourishment

use crate::{AppState, Job, JobStatus};
use crate::domain::harvest::create_harvest;
use garden_common::CeremonyMode;
use anyhow::Result;

pub async fn execute_collect(
    state: &AppState,
    offering: &str,
    ceremony_mode: CeremonyMode,
    recklessly: bool,
) -> Result<Option<String>> {
    if recklessly {
        tracing::info!(offering, "Skipping collect (recklessly mode)");
        return Ok(None);
    }

    let container_name = format!("zen-offering-{}", offering);

    // Quiesce if supported
    if ceremony_mode == CeremonyMode::Quiesceable {
        // Execute quiesce command via docker exec
        // (implementation depends on template policy)
        tracing::info!(offering, "Quiescing service");
    }

    // Create harvest
    let manifest = create_harvest(
        &state.docker,
        &state.harvest_store,
        offering,
        &state.stone_id,
        ceremony_mode != CeremonyMode::Stateless,
    ).await?;

    // Resume if quiesced
    if ceremony_mode == CeremonyMode::Quiesceable {
        tracing::info!(offering, "Resuming service");
    }

    Ok(Some(manifest.id))
}
```

**File:** `src/moss/src/domain/ceremony/phases/nourish.rs` (NEW)

```rust
//! Nourish phase - pull new image and recreate container

use crate::AppState;
use anyhow::Result;

pub async fn execute_nourish(
    state: &AppState,
    offering: &str,
    new_image: &str,
) -> Result<()> {
    let container_name = format!("zen-offering-{}", offering);

    // Pull new image
    tracing::info!(offering, new_image, "Pulling new image");
    state.docker.pull_image(new_image, Some(&state.console)).await?;

    // Get current container config (ports, env, volumes)
    let volumes = state.docker.get_container_volumes(&container_name).await?;

    // Stop and remove old container
    state.docker.stop_service(&container_name).await?;
    state.docker.remove_container(&container_name).await?;

    // Create new container with same config but new image
    // (This reuses the install_service logic)

    tracing::info!(offering, new_image, "Recreated container with new image");

    Ok(())
}
```

**File:** `src/moss/src/domain/ceremony/phases/water.rs` (NEW)

```rust
//! Water phase - bring service up and verify health

use crate::AppState;
use crate::domain::harvest::restore_harvest;
use anyhow::Result;
use std::time::Duration;

pub async fn execute_water(
    state: &AppState,
    offering: &str,
    harvest_id: Option<&str>,
    auto_rollback: bool,
) -> Result<()> {
    let container_name = format!("zen-offering-{}", offering);

    // Start container
    state.docker.start_service(&container_name).await?;

    // Wait for health
    let healthy = wait_for_health(state, &container_name, Duration::from_secs(60)).await;

    if healthy {
        tracing::info!(offering, "Service is healthy");
        Ok(())
    } else if auto_rollback && harvest_id.is_some() {
        tracing::warn!(offering, "Health check failed, rolling back");

        // Stop failed container
        state.docker.stop_service(&container_name).await?;

        // Restore from harvest
        restore_harvest(&state.docker, &state.harvest_store, harvest_id.unwrap()).await?;

        // Start with original image
        state.docker.start_service(&container_name).await?;

        anyhow::bail!("Health check failed, rolled back to previous version");
    } else {
        anyhow::bail!("Health check failed");
    }
}

async fn wait_for_health(
    state: &AppState,
    container_name: &str,
    timeout: Duration,
) -> bool {
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        if let Ok(health) = state.docker.get_service_health(container_name).await {
            if health == garden_common::ServiceHealthStatus::Healthy {
                return true;
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    false
}
```

### Task 3.2: Nourish Ceremony Orchestrator

**File:** `src/moss/src/domain/ceremony/nourish.rs` (NEW)

```rust
//! Nourish offering ceremony

use super::phases::{collect, nourish, water};
use super::types::*;
use crate::AppState;
use garden_common::{CeremonyMode, CeremonyPolicy};
use anyhow::Result;

pub async fn execute_nourish_offering(
    state: &AppState,
    ceremony: &mut Ceremony,
    offering: &str,
    new_image: &str,
    policy: &CeremonyPolicy,
) -> Result<()> {
    // Build phases
    ceremony.phases = vec![
        Phase::new("collect"),
        Phase::new("nourish"),
        Phase::new("water"),
    ];

    ceremony.state = CeremonyState::Executing;
    ceremony.started_at = Some(chrono::Utc::now());
    state.ceremony_journal.persist(ceremony).await?;

    // Phase 1: Collect
    ceremony.phases[0].state = PhaseState::Running;
    ceremony.phases[0].started_at = Some(chrono::Utc::now());
    state.ceremony_journal.persist(ceremony).await?;

    let harvest_id = match collect::execute_collect(
        state,
        offering,
        policy.mode.clone(),
        ceremony.options.recklessly,
    ).await {
        Ok(id) => {
            ceremony.phases[0].state = PhaseState::Completed;
            ceremony.phases[0].completed_at = Some(chrono::Utc::now());
            if let Some(ref id) = id {
                ceremony.artifacts.insert("harvest_id".to_string(), id.clone());
            }
            id
        }
        Err(e) => {
            ceremony.phases[0].state = PhaseState::Failed;
            ceremony.phases[0].error = Some(e.to_string());
            ceremony.state = CeremonyState::Failed;
            ceremony.error = Some(e.to_string());
            state.ceremony_journal.persist(ceremony).await?;
            return Err(e);
        }
    };

    state.ceremony_journal.persist(ceremony).await?;
    ceremony.current_phase = 1;

    // Phase 2: Nourish
    ceremony.phases[1].state = PhaseState::Running;
    ceremony.phases[1].started_at = Some(chrono::Utc::now());
    state.ceremony_journal.persist(ceremony).await?;

    if let Err(e) = nourish::execute_nourish(state, offering, new_image).await {
        ceremony.phases[1].state = PhaseState::Failed;
        ceremony.phases[1].error = Some(e.to_string());
        ceremony.state = CeremonyState::Failed;
        ceremony.error = Some(e.to_string());
        state.ceremony_journal.persist(ceremony).await?;
        return Err(e);
    }

    ceremony.phases[1].state = PhaseState::Completed;
    ceremony.phases[1].completed_at = Some(chrono::Utc::now());
    state.ceremony_journal.persist(ceremony).await?;
    ceremony.current_phase = 2;

    // Phase 3: Water
    ceremony.phases[2].state = PhaseState::Running;
    ceremony.phases[2].started_at = Some(chrono::Utc::now());
    state.ceremony_journal.persist(ceremony).await?;

    match water::execute_water(
        state,
        offering,
        harvest_id.as_deref(),
        ceremony.options.auto_rollback,
    ).await {
        Ok(()) => {
            ceremony.phases[2].state = PhaseState::Completed;
            ceremony.phases[2].completed_at = Some(chrono::Utc::now());
            ceremony.state = CeremonyState::Completed;
        }
        Err(e) => {
            if e.to_string().contains("rolled back") {
                ceremony.phases[2].state = PhaseState::Failed;
                ceremony.state = CeremonyState::RolledBack;
            } else {
                ceremony.phases[2].state = PhaseState::Failed;
                ceremony.phases[2].error = Some(e.to_string());
                ceremony.state = CeremonyState::Failed;
            }
            ceremony.error = Some(e.to_string());
        }
    }

    ceremony.completed_at = Some(chrono::Utc::now());
    state.ceremony_journal.persist(ceremony).await?;

    if ceremony.state == CeremonyState::Completed {
        Ok(())
    } else {
        anyhow::bail!("{}", ceremony.error.as_deref().unwrap_or("Ceremony failed"))
    }
}
```

### Gate 3: Nourish with Rollback Test

**Test on Wyse machine:**

```bash
# Setup: Install mongodb 7.0.4
garden-rake offer mongodb

# Insert test data
docker exec zen-offering-mongodb mongosh --eval "db.test.insertOne({x:1})"

# Attempt nourish to bad image (simulates failure)
curl -X POST http://localhost:7185/api/v1/nourishment/offerings \
  -H "Content-Type: application/json" \
  -d '{"offerings": ["mongodb"], "target_image": "mongo:7.0.4-bad"}'

# Verify rollback occurred
docker exec zen-offering-mongodb mongosh --eval "db.test.find()"
# Should return {x:1}

# Nourish to real update
curl -X POST http://localhost:7185/api/v1/nourishment/offerings \
  -H "Content-Type: application/json" \
  -d '{"offerings": ["mongodb"], "target_image": "mongo:7.0.5"}'

# Verify data preserved
docker exec zen-offering-mongodb mongosh --eval "db.test.find()"
# Should return {x:1}
```

---

## Phase 4-8: See Full Proposal

The remaining phases follow the same pattern:
- Phase 4: Rake CLI (nourish, store, harvests commands)
- Phase 5: Stone-to-stone transfer
- Phase 6: Vacate ceremony
- Phase 7: Stone nourishment (firmware)
- Phase 8: Ceremony discovery

---

## Test Scenarios for 3 Wyse Machines

### Environment Setup

```
Wyse-01 (stone-coral-prairie)
├── mongodb
└── redis

Wyse-02 (stone-amber-brook)
├── postgres
└── nginx

Wyse-03 (stone-sage-meadow)
└── (empty - target for vacate)
```

### Test Matrix

| Test | Wyse-01 | Wyse-02 | Wyse-03 | Validates |
|------|---------|---------|---------|-----------|
| T1: Single nourish | mongodb 7.0.4→7.0.5 | - | - | Collect/nourish/water |
| T2: Rollback | redis (bad image) | - | - | Auto-rollback |
| T3: Multi-offering | mongodb + redis | - | - | Parallel nourish |
| T4: Recklessly | nginx | - | - | Skip backup |
| T5: Store | - | postgres | - | Stored offering |
| T6: Replant | mongodb | - | receives | Cross-stone transfer |
| T7: Vacate | all offerings | - | receives | Full vacate |
| T8: Stone nourish | BIOS update | - | - | Firmware (1x only!) |
| T9: Rolling nourish | BIOS | BIOS | - | Zero-downtime |
| T10: Ceremony discovery | active | observer | observer | UDP broadcast |

### Firmware Update Test (CAREFUL - Limited!)

```bash
# On Wyse-01, with Wyse-02 and Wyse-03 available

# 1. Check current BIOS version
fwupdmgr get-devices

# 2. Vacate offerings first
garden-rake vacate stone-coral-prairie

# 3. Verify offerings moved
garden-rake stones
# Should show mongodb/redis on other stones

# 4. Apply firmware
garden-rake nourish stone-coral-prairie

# 5. After reboot, verify
garden-rake status
```

---

## Success Criteria

### Phase 0
- [ ] `cargo check` passes for all crates
- [ ] `cargo test` passes for all crates
- [ ] CeremonyPolicy can be deserialized from YAML

### Phase 1
- [ ] Can archive a volume to .tar.zst
- [ ] Can restore a volume from archive
- [ ] Checksums validate correctly
- [ ] HarvestStore lists/prunes harvests

### Phase 2
- [ ] Ceremony persists to journal
- [ ] Ceremony recovers on restart
- [ ] Active ceremonies tracked in registry

### Phase 3
- [ ] Single offering nourish works
- [ ] Failed health check triggers rollback
- [ ] Data preserved after nourish
- [ ] Events emitted at each phase

### Phase 4
- [ ] `garden-rake nourish` shows report
- [ ] `garden-rake nourish mongodb` updates offering
- [ ] `garden-rake harvests` lists backups

### Phase 5+
- [ ] Replant transfers offering between stones
- [ ] Vacate empties a stone
- [ ] Ceremony discovery shows active ceremonies
