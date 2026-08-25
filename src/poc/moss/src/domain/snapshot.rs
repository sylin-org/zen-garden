//! Snapshot manifest types and on-disk storage.
//!
//! A **snapshot** is a persisted, addressable capture of an
//! offering instance at a specific moment, carrying:
//!
//! - A **manifest** ([`SnapshotManifest`]) — JSON metadata
//!   listing every file in the snapshot with its SHA512 hash,
//!   the source offering's FQN, the watermark event_id, and a
//!   digest of the offering's compiled manifest at capture time.
//! - An **image** — captured one of two ways, recorded by the
//!   [`ImageTransport`] discriminator (ORCH-0040). Registry-backed
//!   images are captured *by reference* (the `repo@sha256:…` digest;
//!   no bytes stored). Images without a registry digest fall back to
//!   a `docker save` tarball alongside the manifest, so the snapshot
//!   stays self-contained.
//! - **Volumes** — tar.gz archives of each Docker bind mount
//!   (existing harvest format).
//! - **External mounts** — tar.gz archives of every directory
//!   declared as an external mount in the offering's compiled
//!   manifest. Per ORCH-0039 we pack everything declared, no
//!   opt-out — principle of least surprise.
//!
//! Snapshots are written to a [`SnapshotStore`]. The local-disk
//! adapter [`LocalSnapshotStore`] is the first; bank-backed
//! storage layers on top in a later commit.
//!
//! See [ORCH-0039] §"Seed metadata schema" for the design.
//!
//! [ORCH-0039]: ../../../../docs/decisions/ORCH-0039-seed-based-offering-replication.md

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};

/// How a snapshot's image is carried (ORCH-0040). The variant
/// determines whether bytes live in the store or the image is
/// reproduced by reference at plant time. Keeping it an enum lets
/// older readers fail loud on an unknown transport rather than
/// silently misinterpret bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageTransport {
    /// `docker save` tarball — a plain tar of layered fs + metadata,
    /// loadable on the target via `docker load`. Self-contained;
    /// used for images that aren't registry-reproducible (locally
    /// built, committed, or image-direct loaded from a tarball).
    DockerSave,
    /// Reproduced by registry reference at plant time — no bytes in
    /// the store. Used when the running image has a registry digest
    /// (`SnapshotImage::ref_string` holds the `repo@sha256:…` pin and
    /// `size_bytes`/`sha512` are zero/empty). See [ORCH-0040].
    ///
    /// [ORCH-0040]: ../../../../docs/decisions/ORCH-0040-snapshot-image-by-reference.md
    Registry,
}

/// Snapshot's image artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotImage {
    /// Image reference. For [`ImageTransport::Registry`] this is the
    /// functional identity — the `repo@sha256:…` digest pin the image
    /// is reproduced from at plant time. For [`ImageTransport::DockerSave`]
    /// it is the committed source tag (e.g.
    /// `zen-harvest/mongodb--prd:2026-05-05T10-30-00`), diagnostic only:
    /// the load is from the tarball, not this ref.
    pub ref_string: String,
    /// Transport used to capture the image.
    pub transport: ImageTransport,
    /// Bytes on disk for the image artifact.
    pub size_bytes: u64,
    /// SHA512 of the image artifact, lowercase hex without
    /// prefix. Verifies integrity on transfer.
    pub sha512: String,
}

/// One captured volume (Docker bind mount).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotVolume {
    /// Volume name as recorded by harvest (typically the
    /// container_path's basename).
    pub name: String,
    /// Path inside the container the volume mounts at.
    pub container_path: String,
    /// Bytes on disk for the volume archive.
    pub size_bytes: u64,
    /// SHA512 of the archive, lowercase hex without prefix.
    pub sha512: String,
}

/// One captured external mount — a host directory declared in
/// the offering's compiled manifest as a mount source. Per
/// ORCH-0039 every external mount declared at capture time is
/// included; offering authors who don't want a mount packed
/// must not declare it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotExternalMount {
    /// Host filesystem path that was packed (the `source` field
    /// of the offering's external-mount declaration).
    pub host_path: String,
    /// Path inside the container the mount appears at.
    pub container_path: String,
    /// Bytes on disk for the archive.
    pub size_bytes: u64,
    /// SHA512 of the archive, lowercase hex without prefix.
    pub sha512: String,
}

/// Top-level snapshot metadata. Serialized to `manifest.json`
/// inside each snapshot directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotManifest {
    /// GUIDV7 — unique snapshot identifier, time-ordered.
    pub id: String,
    /// FQN of the offering this snapshot was captured from.
    /// Plant-from-snapshot defaults to this; an explicit
    /// `as_fqn` override is the fork path.
    pub source_fqn: String,
    /// Stone where the snapshot was captured.
    pub source_stone: String,
    /// The event_id of the `BackupTaken` event that recorded
    /// this snapshot in the source's event log. Used by the
    /// am-I-behind comparison: an instance whose watermark is
    /// older than this id must sync.
    pub source_event_id: String,
    /// Wall-clock time of capture.
    pub created_at: DateTime<Utc>,
    /// SHA256 of the offering's compiled manifest at capture
    /// time, lowercase hex without prefix. Plant-time drift
    /// detection uses this — restoring a `mongodb::prd` seed
    /// on a stone whose mongodb manifest has changed warns
    /// the user.
    pub manifest_digest: String,
    /// The offering image artifact.
    pub image: SnapshotImage,
    /// Captured Docker volumes.
    #[serde(default)]
    pub volumes: Vec<SnapshotVolume>,
    /// Captured external mounts.
    #[serde(default)]
    pub external_mounts: Vec<SnapshotExternalMount>,
    /// Sum of `image.size_bytes`, all volumes, and all external
    /// mounts. Stored explicitly so consumers (the manifest-only
    /// preview endpoint) don't need to walk the whole structure.
    pub size_total_bytes: u64,
}

impl SnapshotManifest {
    /// Recompute and set `size_total_bytes` from the current
    /// image / volumes / external_mounts. Useful when building
    /// the manifest piecewise during capture.
    pub fn refresh_total_size(&mut self) {
        self.size_total_bytes = self.image.size_bytes
            + self.volumes.iter().map(|v| v.size_bytes).sum::<u64>()
            + self
                .external_mounts
                .iter()
                .map(|m| m.size_bytes)
                .sum::<u64>();
    }
}

/// Compute the SHA512 of a file. Streams 64 KiB chunks so memory
/// stays bounded for large image / volume archives. Returns
/// lowercase-hex without any algorithm prefix — the
/// [`SnapshotManifest`] schema implies SHA512 for every hash
/// field.
pub async fn sha512_file(path: &Path) -> Result<String> {
    use tokio::io::AsyncReadExt;
    let mut file = tokio::fs::File::open(path)
        .await
        .with_context(|| format!("open file for hashing: {}", path.display()))?;
    let mut hasher = Sha512::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .await
            .with_context(|| format!("read for hashing: {}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    Ok(format!("{:x}", digest))
}

/// Compute the SHA256 of a manifest body (offering's compiled
/// manifest). Used for `SnapshotManifest::manifest_digest`. The
/// caller passes the canonical bytes — typically `serde_json::to_vec`
/// of the in-memory manifest, or the source YAML bytes.
pub fn sha256_bytes(bytes: &[u8]) -> String {
    use sha2::Sha256;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

// ── Volume classification ──────────────────────────────────────

/// How a captured volume should be categorised in the snapshot.
/// The Docker `volumes` array on a CompiledOffering doesn't
/// distinguish managed Docker volumes from arbitrary host bind
/// mounts — both come back as `(host_path, container_path)`
/// tuples. The distinction is computed from the host_path: if it
/// lives under the platform's `volumes_dir()` for *this* offering,
/// it's a managed volume; otherwise it's an external mount and
/// must be packed under the offering author's declared host
/// path so plant can restore it to the same place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeClass {
    /// Docker-managed named volume — host_path lives under
    /// `<volumes_dir>/<offering_encoded>/`.
    Managed,
    /// External mount — host_path is anywhere else, including
    /// user data directories declared in the offering manifest.
    External,
}

/// Classify a captured volume by comparing its `host_path` to
/// the per-offering managed-volumes root. Pure function, no I/O.
///
/// `host_path` and `managed_root` should both be in the same
/// path style (the platform's native form). The check is a
/// path-prefix comparison: any volume whose host_path starts
/// with the managed root is Managed; everything else is
/// External.
pub fn classify_volume(host_path: &Path, managed_root: &Path) -> VolumeClass {
    if host_path.starts_with(managed_root) {
        VolumeClass::Managed
    } else {
        VolumeClass::External
    }
}

// ── Storage adapter ─────────────────────────────────────────────

/// Operations a snapshot store provides. Both the local-disk
/// adapter and the (future) bank-backed adapter implement this.
///
/// Layout under a store is one directory per snapshot, named
/// after `manifest.id`:
///
/// ```text
/// <root>/
///   <snapshot_id>/
///     manifest.json
///     image.tar
///     volumes/<name>.tar.gz
///     external_mounts/<encoded_path>.tar.gz
/// ```
///
/// Encoded-path mapping for external mounts is a function of
/// the host path; the [`SnapshotStore::external_mount_filename`]
/// helper centralises the encoding so reads and writes agree.
#[allow(async_fn_in_trait)] // Concrete adapters only — no dyn dispatch needed yet.
pub trait SnapshotStore: Send + Sync {
    /// Persist the manifest JSON for a snapshot. The directory
    /// is created if missing.
    async fn save_manifest(&self, manifest: &SnapshotManifest) -> Result<()>;

    /// Load the manifest for `id`.
    async fn load_manifest(&self, id: &str) -> Result<SnapshotManifest>;

    /// List every snapshot id present in the store, in
    /// chronological order (lexicographic on GUIDV7 id).
    async fn list_ids(&self) -> Result<Vec<String>>;

    /// Filesystem path the image artifact for `id` should live
    /// at. Used by capture (write) and plant (read).
    fn image_path(&self, id: &str) -> PathBuf;

    /// Filesystem path for a named volume archive.
    fn volume_path(&self, id: &str, name: &str) -> PathBuf;

    /// Filesystem path for an external mount archive,
    /// disambiguated by the encoded host path.
    fn external_mount_path(&self, id: &str, host_path: &str) -> PathBuf;

    /// Remove a snapshot's directory entirely. Used by
    /// retention and explicit user delete. Idempotent — calling
    /// for an unknown id is `Ok(())`.
    async fn delete(&self, id: &str) -> Result<()>;
}

/// Local-disk snapshot store rooted at a single directory.
/// Used as the M2 default target when the user picks
/// "Local disk" in the snapshot picker. Bank-backed storage
/// reuses the same trait surface — see the [bank] adapter
/// (commit S5).
pub struct LocalSnapshotStore {
    root: PathBuf,
}

impl LocalSnapshotStore {
    /// Open the store rooted at `root`. The directory is
    /// created lazily on first write.
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Root directory of the store.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Convert a host path into a filename component safe for
    /// nesting under `external_mounts/`. We replace path
    /// separators with `--` and strip leading separators so the
    /// result is a single filesystem path component. This is
    /// not reversible — callers needing the original host path
    /// read it from the manifest, not the filename.
    fn encoded_external_mount(host_path: &str) -> String {
        Self::encoded_external_mount_for(host_path)
    }

    /// Public façade over the internal encoder so cross-stone
    /// plant can compute the same URL-segment encoding the
    /// store uses on the wire side. The returned string is a
    /// single filesystem-safe path component (no separators).
    pub fn encoded_external_mount_for(host_path: &str) -> String {
        let trimmed = host_path
            .trim_start_matches(|c: char| c == '/' || c == '\\')
            .replace('\\', "/");
        trimmed.replace('/', "--").replace(':', "_")
    }

    /// Remove every snapshot directory that lacks a `manifest.json`.
    ///
    /// A snapshot directory without a manifest is an aborted capture:
    /// the manifest is the *last* artifact written (after image and
    /// volume archives), so its absence means the capture failed
    /// partway and left orphaned bytes — typically a multi-hundred-MB
    /// `image.tar` with no way to plant from it ([`list_ids`] already
    /// skips these). Returns the reaped ids for logging.
    ///
    /// Caller responsibility: only invoke when no capture is in
    /// flight for this offering (e.g. the startup sweep, before the
    /// periodic loop begins capturing). A concurrent capture's
    /// not-yet-finalised directory would otherwise be eligible for
    /// reaping.
    ///
    /// [`list_ids`]: SnapshotStore::list_ids
    pub async fn reap_orphans(&self) -> Result<Vec<String>> {
        let mut reaped = Vec::new();
        let mut entries = match tokio::fs::read_dir(&self.root).await {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => {
                return Err(anyhow::Error::from(e)
                    .context(format!("read snapshot root: {}", self.root.display())));
            }
        };
        while let Some(entry) = entries.next_entry().await? {
            if !entry.file_type().await?.is_dir() {
                continue;
            }
            let dir = entry.path();
            // A manifest means a complete, plantable snapshot — keep it.
            // On a stat error, err on the side of keeping: never delete a
            // directory we couldn't confirm is an orphan.
            if tokio::fs::try_exists(dir.join("manifest.json"))
                .await
                .unwrap_or(true)
            {
                continue;
            }
            tokio::fs::remove_dir_all(&dir)
                .await
                .with_context(|| format!("reap orphaned snapshot dir: {}", dir.display()))?;
            if let Some(name) = entry.file_name().to_str() {
                reaped.push(name.to_string());
            }
        }
        Ok(reaped)
    }
}

/// Prune a store down to its `keep` most-recent snapshots, deleting
/// the rest. Snapshot ids are GUIDV7 (time-ordered), so
/// [`SnapshotStore::list_ids`]'s lexicographic sort is chronological —
/// the oldest ids sort first and are the ones removed. Returns the
/// deleted ids.
///
/// Trait-level (works for any store) because it relies only on
/// `list_ids` + `delete`. The local-disk-specific orphan cleanup is
/// [`LocalSnapshotStore::reap_orphans`]; the two are complementary —
/// `list_ids` only sees manifest-bearing snapshots, so retention never
/// touches (nor cleans up) aborted captures.
pub async fn prune_snapshots<S: SnapshotStore + ?Sized>(
    store: &S,
    keep: usize,
) -> Result<Vec<String>> {
    let ids = store.list_ids().await?;
    if ids.len() <= keep {
        return Ok(Vec::new());
    }
    let cutoff = ids.len() - keep;
    let mut deleted = Vec::with_capacity(cutoff);
    for id in &ids[..cutoff] {
        // Best-effort: a stuck deletion (e.g. an open file handle) must
        // not block pruning the remaining older snapshots.
        match store.delete(id).await {
            Ok(()) => deleted.push(id.clone()),
            Err(e) => tracing::warn!(
                error = %e,
                snapshot_id = %id,
                "prune: failed to delete old snapshot, skipping"
            ),
        }
    }
    Ok(deleted)
}

impl SnapshotStore for LocalSnapshotStore {
    async fn save_manifest(&self, manifest: &SnapshotManifest) -> Result<()> {
        let dir = self.root.join(&manifest.id);
        tokio::fs::create_dir_all(&dir)
            .await
            .with_context(|| format!("create snapshot dir: {}", dir.display()))?;
        let path = dir.join("manifest.json");
        let body = serde_json::to_vec_pretty(manifest).context("serialize SnapshotManifest")?;
        let tmp = path.with_extension("tmp");
        tokio::fs::write(&tmp, &body)
            .await
            .with_context(|| format!("write manifest tmp: {}", tmp.display()))?;
        tokio::fs::rename(&tmp, &path)
            .await
            .with_context(|| format!("rename manifest tmp: {}", tmp.display()))?;
        Ok(())
    }

    async fn load_manifest(&self, id: &str) -> Result<SnapshotManifest> {
        let path = self.root.join(id).join("manifest.json");
        let body = tokio::fs::read_to_string(&path)
            .await
            .with_context(|| format!("read manifest: {}", path.display()))?;
        serde_json::from_str(&body)
            .with_context(|| format!("parse manifest: {}", path.display()))
    }

    async fn list_ids(&self) -> Result<Vec<String>> {
        let mut ids = Vec::new();
        let mut entries = match tokio::fs::read_dir(&self.root).await {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => {
                return Err(anyhow::Error::from(e)
                    .context(format!("read snapshot root: {}", self.root.display())));
            }
        };
        while let Some(entry) = entries.next_entry().await? {
            if !entry.file_type().await?.is_dir() {
                continue;
            }
            // Only count directories that actually contain a manifest —
            // skips half-written or unrelated dirs. A stat error means we
            // can't confirm a manifest, so we skip (don't list) it.
            if !tokio::fs::try_exists(entry.path().join("manifest.json"))
                .await
                .unwrap_or(false)
            {
                continue;
            }
            if let Some(name) = entry.file_name().to_str() {
                ids.push(name.to_string());
            }
        }
        ids.sort();
        Ok(ids)
    }

    fn image_path(&self, id: &str) -> PathBuf {
        self.root.join(id).join("image.tar")
    }

    fn volume_path(&self, id: &str, name: &str) -> PathBuf {
        self.root.join(id).join("volumes").join(format!(
            "{}.tar.gz",
            name.replace('/', "_").replace('\\', "_")
        ))
    }

    fn external_mount_path(&self, id: &str, host_path: &str) -> PathBuf {
        self.root.join(id).join("external_mounts").join(format!(
            "{}.tar.gz",
            Self::encoded_external_mount(host_path)
        ))
    }

    async fn delete(&self, id: &str) -> Result<()> {
        let dir = self.root.join(id);
        match tokio::fs::remove_dir_all(&dir).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(anyhow::Error::from(e)
                .context(format!("remove snapshot dir: {}", dir.display()))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn manifest(id: &str) -> SnapshotManifest {
        let mut m = SnapshotManifest {
            id: id.to_string(),
            source_fqn: "mongodb::prd".into(),
            source_stone: "stone-alpha".into(),
            source_event_id: "01ABC".into(),
            created_at: Utc::now(),
            manifest_digest: "sha256-placeholder".into(),
            image: SnapshotImage {
                ref_string: "zen-harvest/mongodb--prd:t1".into(),
                transport: ImageTransport::DockerSave,
                size_bytes: 100,
                sha512: "placeholder".into(),
            },
            volumes: vec![SnapshotVolume {
                name: "data".into(),
                container_path: "/data/db".into(),
                size_bytes: 50,
                sha512: "v-hash".into(),
            }],
            external_mounts: vec![SnapshotExternalMount {
                host_path: "/var/data/photos".into(),
                container_path: "/photos".into(),
                size_bytes: 200,
                sha512: "em-hash".into(),
            }],
            size_total_bytes: 0,
        };
        m.refresh_total_size();
        m
    }

    #[tokio::test]
    async fn save_then_load_round_trips_manifest() {
        let dir = TempDir::new().unwrap();
        let store = LocalSnapshotStore::new(dir.path().join("snapshots"));
        let m = manifest("snap-1");
        store.save_manifest(&m).await.unwrap();
        let loaded = store.load_manifest("snap-1").await.unwrap();
        assert_eq!(loaded, m);
    }

    #[tokio::test]
    async fn refresh_total_size_sums_image_volumes_and_external_mounts() {
        let m = manifest("snap-1");
        // 100 (image) + 50 (one volume) + 200 (one external mount) = 350.
        assert_eq!(m.size_total_bytes, 350);
    }

    #[tokio::test]
    async fn save_manifest_creates_dir_lazily() {
        // Root doesn't exist yet — save must create it.
        let dir = TempDir::new().unwrap();
        let nested = dir.path().join("a").join("b").join("c");
        assert!(!nested.exists());
        let store = LocalSnapshotStore::new(nested);
        store.save_manifest(&manifest("snap-1")).await.unwrap();
        assert!(store.load_manifest("snap-1").await.is_ok());
    }

    #[tokio::test]
    async fn list_ids_returns_sorted_ids_skipping_dirs_without_manifest() {
        let dir = TempDir::new().unwrap();
        let store = LocalSnapshotStore::new(dir.path().to_path_buf());

        // GUIDV7-shaped ids: chronological = lexicographic.
        store.save_manifest(&manifest("01-aa")).await.unwrap();
        store.save_manifest(&manifest("01-cc")).await.unwrap();
        store.save_manifest(&manifest("01-bb")).await.unwrap();

        // A stray directory without a manifest must not appear
        // in the listing — protects against half-deleted /
        // half-created snapshots showing up as catalog entries.
        std::fs::create_dir_all(dir.path().join("not-a-snapshot")).unwrap();

        let ids = store.list_ids().await.unwrap();
        assert_eq!(ids, vec!["01-aa", "01-bb", "01-cc"]);
    }

    #[tokio::test]
    async fn list_ids_returns_empty_when_root_missing() {
        let dir = TempDir::new().unwrap();
        let store = LocalSnapshotStore::new(dir.path().join("never-created"));
        assert!(store.list_ids().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn delete_is_idempotent_for_unknown_id() {
        let dir = TempDir::new().unwrap();
        let store = LocalSnapshotStore::new(dir.path().to_path_buf());
        store.delete("snap-does-not-exist").await.unwrap();
    }

    #[tokio::test]
    async fn delete_removes_the_snapshot_directory() {
        let dir = TempDir::new().unwrap();
        let store = LocalSnapshotStore::new(dir.path().to_path_buf());
        store.save_manifest(&manifest("snap-1")).await.unwrap();
        assert!(dir.path().join("snap-1").exists());
        store.delete("snap-1").await.unwrap();
        assert!(!dir.path().join("snap-1").exists());
    }

    /// Helper: create a manifest-less snapshot directory holding a
    /// stand-in `image.tar`, mimicking a capture that died before
    /// writing its manifest.
    async fn write_orphan(root: &Path, id: &str) {
        let dir = root.join(id);
        tokio::fs::create_dir_all(&dir).await.unwrap();
        tokio::fs::write(dir.join("image.tar"), b"partial").await.unwrap();
    }

    #[tokio::test]
    async fn reap_orphans_removes_manifestless_dirs_and_keeps_valid() {
        let dir = TempDir::new().unwrap();
        let store = LocalSnapshotStore::new(dir.path().to_path_buf());

        // Two complete snapshots (have manifests) + two aborted captures.
        store.save_manifest(&manifest("01-valid-a")).await.unwrap();
        store.save_manifest(&manifest("01-valid-b")).await.unwrap();
        write_orphan(dir.path(), "01-orphan-a").await;
        write_orphan(dir.path(), "01-orphan-b").await;

        let mut reaped = store.reap_orphans().await.unwrap();
        reaped.sort();
        assert_eq!(reaped, vec!["01-orphan-a", "01-orphan-b"]);

        // Orphans gone, valid snapshots untouched.
        assert!(!dir.path().join("01-orphan-a").exists());
        assert!(!dir.path().join("01-orphan-b").exists());
        assert!(dir.path().join("01-valid-a").join("manifest.json").exists());
        assert!(dir.path().join("01-valid-b").join("manifest.json").exists());
        assert_eq!(store.list_ids().await.unwrap(), vec!["01-valid-a", "01-valid-b"]);
    }

    #[tokio::test]
    async fn reap_orphans_is_noop_when_all_snapshots_are_valid() {
        let dir = TempDir::new().unwrap();
        let store = LocalSnapshotStore::new(dir.path().to_path_buf());
        store.save_manifest(&manifest("01-aa")).await.unwrap();
        assert!(store.reap_orphans().await.unwrap().is_empty());
        assert_eq!(store.list_ids().await.unwrap(), vec!["01-aa"]);
    }

    #[tokio::test]
    async fn reap_orphans_returns_empty_when_root_missing() {
        let dir = TempDir::new().unwrap();
        let store = LocalSnapshotStore::new(dir.path().join("never-created"));
        assert!(store.reap_orphans().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn prune_snapshots_keeps_the_n_most_recent() {
        let dir = TempDir::new().unwrap();
        let store = LocalSnapshotStore::new(dir.path().to_path_buf());
        // Seven snapshots, ids ascending = oldest → newest.
        for id in ["01-a", "01-b", "01-c", "01-d", "01-e", "01-f", "01-g"] {
            store.save_manifest(&manifest(id)).await.unwrap();
        }

        let mut deleted = prune_snapshots(&store, 5).await.unwrap();
        deleted.sort();
        // The two oldest are pruned.
        assert_eq!(deleted, vec!["01-a", "01-b"]);
        assert_eq!(
            store.list_ids().await.unwrap(),
            vec!["01-c", "01-d", "01-e", "01-f", "01-g"]
        );
    }

    #[tokio::test]
    async fn prune_snapshots_is_noop_when_within_keep() {
        let dir = TempDir::new().unwrap();
        let store = LocalSnapshotStore::new(dir.path().to_path_buf());
        for id in ["01-a", "01-b", "01-c"] {
            store.save_manifest(&manifest(id)).await.unwrap();
        }
        assert!(prune_snapshots(&store, 5).await.unwrap().is_empty());
        assert_eq!(store.list_ids().await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn prune_snapshots_at_exact_keep_boundary_deletes_nothing() {
        let dir = TempDir::new().unwrap();
        let store = LocalSnapshotStore::new(dir.path().to_path_buf());
        for id in ["01-a", "01-b", "01-c", "01-d", "01-e"] {
            store.save_manifest(&manifest(id)).await.unwrap();
        }
        // len == keep: nothing to prune.
        assert!(prune_snapshots(&store, 5).await.unwrap().is_empty());
        assert_eq!(store.list_ids().await.unwrap().len(), 5);
    }

    #[tokio::test]
    async fn external_mount_filename_encoding_is_filesystem_safe() {
        // The host path `/var/lib/zen-garden/photos` must
        // produce a single, separator-free filename component.
        let encoded =
            LocalSnapshotStore::encoded_external_mount("/var/lib/zen-garden/photos");
        assert_eq!(encoded, "var--lib--zen-garden--photos");
        // Windows-style paths normalise too.
        let win = LocalSnapshotStore::encoded_external_mount("C:\\data\\photos");
        assert_eq!(win, "C_--data--photos");
    }

    #[tokio::test]
    async fn paths_under_a_snapshot_id_are_predictable() {
        let dir = TempDir::new().unwrap();
        let store = LocalSnapshotStore::new(dir.path().to_path_buf());
        let id = "snap-1";
        assert!(store.image_path(id).ends_with("snap-1/image.tar"));
        assert!(
            store
                .volume_path(id, "data")
                .ends_with("snap-1/volumes/data.tar.gz")
        );
        assert!(
            store
                .external_mount_path(id, "/var/data/photos")
                .ends_with("snap-1/external_mounts/var--data--photos.tar.gz")
        );
    }

    #[tokio::test]
    async fn sha512_file_matches_known_vector() {
        // Empty file → known SHA512 of the empty string.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("empty");
        tokio::fs::write(&path, b"").await.unwrap();
        let hash = sha512_file(&path).await.unwrap();
        // SHA512("") published value.
        assert_eq!(
            hash,
            "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"
        );
    }

    #[tokio::test]
    async fn sha512_file_streams_in_chunks_for_large_inputs() {
        // 256 KiB > the 64 KiB read buffer, so the streaming
        // loop must run multiple iterations. The hash must
        // match a single-shot computation over the same bytes.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("big");
        let bytes: Vec<u8> = (0..256 * 1024).map(|i| (i & 0xff) as u8).collect();
        tokio::fs::write(&path, &bytes).await.unwrap();

        let mut single_shot = Sha512::new();
        single_shot.update(&bytes);
        let expected = format!("{:x}", single_shot.finalize());

        let streamed = sha512_file(&path).await.unwrap();
        assert_eq!(streamed, expected);
    }

    #[test]
    fn sha256_bytes_matches_known_vector() {
        // SHA256("hello world") published value.
        assert_eq!(
            sha256_bytes(b"hello world"),
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn manifest_serialization_uses_snake_case_for_image_transport() {
        let m = manifest("snap-1");
        let json = serde_json::to_string(&m).unwrap();
        assert!(
            json.contains("\"transport\":\"docker_save\""),
            "image transport must serialize as snake_case: {json}"
        );
    }

    #[test]
    fn image_transport_variants_have_stable_wire_format() {
        // The wire format is a cross-version contract (ORCH-0040): a peer
        // running an older build must keep reading `docker_save`, and the
        // new `registry` variant must round-trip.
        assert_eq!(
            serde_json::to_string(&ImageTransport::DockerSave).unwrap(),
            "\"docker_save\""
        );
        assert_eq!(
            serde_json::to_string(&ImageTransport::Registry).unwrap(),
            "\"registry\""
        );
        let back: ImageTransport = serde_json::from_str("\"registry\"").unwrap();
        assert_eq!(back, ImageTransport::Registry);
    }

    #[test]
    fn classify_volume_distinguishes_managed_from_external() {
        // The managed root is `<volumes_dir>/<encoded_offering>`,
        // e.g. on Linux `/var/lib/zen-garden/volumes/mongodb--prd`.
        let managed_root = std::path::PathBuf::from("/var/lib/zen-garden/volumes/mongodb--prd");

        // Host path under the managed root → Managed.
        let inside = std::path::PathBuf::from("/var/lib/zen-garden/volumes/mongodb--prd/data");
        assert_eq!(
            classify_volume(&inside, &managed_root),
            VolumeClass::Managed
        );

        // The managed root itself counts as Managed.
        assert_eq!(
            classify_volume(&managed_root, &managed_root),
            VolumeClass::Managed
        );

        // Sibling FQN's volumes directory is External (different
        // offering instance — must be packed as a foreign mount).
        let sibling = std::path::PathBuf::from("/var/lib/zen-garden/volumes/mongodb--staging/data");
        assert_eq!(
            classify_volume(&sibling, &managed_root),
            VolumeClass::External
        );

        // User-data directory anywhere else is External.
        let user_data = std::path::PathBuf::from("/var/data/photos");
        assert_eq!(
            classify_volume(&user_data, &managed_root),
            VolumeClass::External
        );

        // Subtle prefix match: a path that starts with the same
        // characters but is NOT under the managed root must
        // classify as External. `starts_with` on Path operates
        // on path components, not raw bytes — so
        // `/var/lib/zen-garden/volumes/mongodb--prd-staging` is
        // NOT a child of `/var/lib/zen-garden/volumes/mongodb--prd`.
        let look_alike =
            std::path::PathBuf::from("/var/lib/zen-garden/volumes/mongodb--prd-staging/data");
        assert_eq!(
            classify_volume(&look_alike, &managed_root),
            VolumeClass::External,
            "Path::starts_with must respect component boundaries"
        );
    }
}
