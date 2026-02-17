//! SeedBankStore — single I/O chokepoint for all seed bank content (STORAGE-0006)
//!
//! **Every** read/write to seed bank files flows through this struct:
//! - Nurturing harvests (tar.gz blobs under `garden/memories/`)
//! - Object storage API (arbitrary objects under `garden/storage/`)
//! - S3 surface (same objects, different protocol)
//! - Replication (byte-copy between replicas)
//!
//! ```text
//! Nurturing store ──┐
//! Object storage API ──┤──→ SeedBankStore ──→ filesystem
//! S3 surface ──┤     (encrypt/decrypt if dek present)
//! Replication ──┘
//! ```
//!
//! ## Encryption
//!
//! If `dek` is `Some`, every write is encrypted with ChaCha20-Poly1305 and every
//! read is decrypted. If `dek` is `None`, the store is a passthrough. Callers
//! always work with plaintext — encryption is invisible.
//!
//! DEK is derived from `pond_data_key` + seed bank name (FQN), so all replicas
//! of the same logical seed bank share a key. Replication between them is
//! pure byte-copy of encrypted files.
//!
//! ## Infrastructure layer
//!
//! This is infrastructure — no business rules, no domain logic. It moves bytes
//! between callers and a filesystem, with optional encryption.

use anyhow::{Context, Result};
use garden_common::storage::{ChangelogEntry, ChangelogOp, ChangesResponse, StorageTick};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use tracing::{debug, warn};

/// Changelog file path relative to mount root.
const CHANGELOG_REL: &str = ".zen-garden/changelog.jsonl";

/// Last-synced cursor persisted by Dormant replicas.
const LAST_CURSOR_REL: &str = ".zen-garden/last_cursor";

/// Pin file path relative to mount root — persists pin_id across restarts.
const PIN_REL: &str = ".zen-garden/pin.json";

/// Single I/O chokepoint for all seed bank content.
///
/// Constructed at mount time by the registry. Passed to ObjectStore,
/// NurturingStore, replication tasks, and API handlers.
///
/// When `notify_tx` is set, writes and deletes emit `StorageTick` events
/// on the broadcast channel — subscribers (SSE stream, replication task)
/// are notified that the changelog has advanced.
#[derive(Debug, Clone)]
pub struct SeedBankStore {
    /// Root mount path of the seed bank (e.g., `/var/lib/zen-garden/mounts/my-bank/01956a3e`)
    mount_root: PathBuf,

    /// Data encryption key — `None` for public seed banks, `Some` for pond-encrypted.
    /// Derived from: `BLAKE3-KDF(pond_data_key, "zen-garden-seedbank", seed_bank_name)`
    dek: Option<[u8; 32]>,

    /// Seed bank name — used in StorageTick notifications.
    seed_bank_name: Option<String>,

    /// Optional notification channel for changelog ticks.
    /// Set on Primary stores so Dormant listeners can subscribe.
    notify_tx: Option<tokio::sync::broadcast::Sender<StorageTick>>,
}

impl SeedBankStore {
    /// Create a new store for a public (unencrypted) seed bank.
    pub fn new_public(mount_root: impl Into<PathBuf>) -> Self {
        Self {
            mount_root: mount_root.into(),
            dek: None,
            seed_bank_name: None,
            notify_tx: None,
        }
    }

    /// Create a new store for a pond-encrypted seed bank.
    pub fn new_encrypted(mount_root: impl Into<PathBuf>, dek: [u8; 32]) -> Self {
        Self {
            mount_root: mount_root.into(),
            dek: Some(dek),
            seed_bank_name: None,
            notify_tx: None,
        }
    }

    /// Create a store with optional encryption.
    pub fn new(mount_root: impl Into<PathBuf>, dek: Option<[u8; 32]>) -> Self {
        Self {
            mount_root: mount_root.into(),
            dek,
            seed_bank_name: None,
            notify_tx: None,
        }
    }

    /// Attach a notification channel and seed bank name.
    ///
    /// Call this on Primary stores so writes/deletes emit `StorageTick`
    /// events to the SSE notification stream.
    pub fn with_notifications(
        mut self,
        name: String,
        tx: tokio::sync::broadcast::Sender<StorageTick>,
    ) -> Self {
        self.seed_bank_name = Some(name);
        self.notify_tx = Some(tx);
        self
    }

    /// The mount root path.
    pub fn mount_root(&self) -> &Path {
        &self.mount_root
    }

    /// Whether this store encrypts content.
    pub fn is_encrypted(&self) -> bool {
        self.dek.is_some()
    }

    /// Read a file from the seed bank. Returns plaintext regardless of encryption.
    ///
    /// `rel` is relative to `mount_root` (e.g., `garden/storage/bucket/key`).
    pub async fn read(&self, rel: &Path) -> Result<Vec<u8>> {
        let full_path = self.mount_root.join(rel);
        let raw = tokio::fs::read(&full_path)
            .await
            .with_context(|| format!("Failed to read {}", full_path.display()))?;

        match &self.dek {
            None => Ok(raw),
            Some(dek) => decrypt(dek, &raw)
                .with_context(|| format!("Failed to decrypt {}", full_path.display())),
        }
    }

    /// Write plaintext data to the seed bank. Encrypts transparently if dek is set.
    ///
    /// `rel` is relative to `mount_root`. Parent directories are created automatically.
    /// Write is atomic (tmp + fsync + rename). Appends a changelog entry after success.
    pub async fn write(&self, rel: &Path, data: &[u8]) -> Result<()> {
        let full_path = self.mount_root.join(rel);

        // Determine if this is a create or modify (for changelog)
        let existed = full_path.exists();

        // Ensure parent directories exist
        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Failed to create dirs for {}", full_path.display()))?;
        }

        let bytes_to_write = match &self.dek {
            None => data.to_vec(),
            Some(dek) => encrypt(dek, data)?,
        };

        // Atomic write: tmp → fsync → rename
        let tmp_path = full_path.with_extension("tmp");

        tokio::fs::write(&tmp_path, &bytes_to_write)
            .await
            .with_context(|| format!("Failed to write tmp {}", tmp_path.display()))?;

        // Best-effort fsync
        if let Ok(file) = std::fs::File::open(&tmp_path) {
            let _ = file.sync_all();
        }

        // Windows doesn't allow rename over existing file
        #[cfg(windows)]
        if full_path.exists() {
            let _ = tokio::fs::remove_file(&full_path).await;
        }

        tokio::fs::rename(&tmp_path, &full_path)
            .await
            .with_context(|| {
                format!(
                    "Failed to rename {} → {}",
                    tmp_path.display(),
                    full_path.display()
                )
            })?;

        debug!(path = %rel.display(), encrypted = self.dek.is_some(), size = data.len(), "Object written");

        // Append changelog entry (best-effort — never fails the write)
        let rel_str = rel.to_string_lossy();
        if !rel_str.starts_with(".zen-garden/") {
            let entry = if existed {
                ChangelogEntry::modified(&rel_str, data.len() as u64)
            } else {
                ChangelogEntry::created(&rel_str, data.len() as u64)
            };
            self.append_changelog(&entry).await;
        }

        Ok(())
    }

    /// Delete a file from the seed bank. Appends a changelog entry after success.
    pub async fn delete(&self, rel: &Path) -> Result<bool> {
        let full_path = self.mount_root.join(rel);
        if !full_path.exists() {
            return Ok(false);
        }
        tokio::fs::remove_file(&full_path)
            .await
            .with_context(|| format!("Failed to delete {}", full_path.display()))?;
        debug!(path = %rel.display(), "Object deleted");

        // Append changelog entry (best-effort)
        let rel_str = rel.to_string_lossy();
        if !rel_str.starts_with(".zen-garden/") {
            let entry = ChangelogEntry::deleted(&rel_str);
            self.append_changelog(&entry).await;
        }

        Ok(true)
    }

    /// Check if a file exists.
    pub async fn exists(&self, rel: &Path) -> bool {
        self.mount_root.join(rel).exists()
    }

    /// Read a file as a string (decrypting first if encrypted, then UTF-8 decode).
    pub async fn read_string(&self, rel: &Path) -> Result<String> {
        let bytes = self.read(rel).await?;
        String::from_utf8(bytes).context("Content is not valid UTF-8")
    }

    /// Write a string to the seed bank.
    pub async fn write_string(&self, rel: &Path, data: &str) -> Result<()> {
        self.write(rel, data.as_bytes()).await
    }

    /// Full filesystem path for a relative path (for metadata/stat operations).
    pub fn full_path(&self, rel: &Path) -> PathBuf {
        self.mount_root.join(rel)
    }

    /// Derive the DEK for a seed bank from the pond data key and the seed bank name.
    ///
    /// All replicas of the same logical seed bank share this key.
    pub fn derive_dek(pond_data_key: &[u8; 32], seed_bank_name: &str) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new_keyed(pond_data_key);
        hasher.update(b"zen-garden-seedbank");
        hasher.update(seed_bank_name.as_bytes());
        *hasher.finalize().as_bytes()
    }

    // ========================================================================
    // Changelog (STORAGE-0006 cursor-based replication)
    // ========================================================================

    /// Append a changelog entry and optionally emit a storage tick.
    ///
    /// Best-effort: failures are logged but never propagate to callers.
    /// The changelog is append-only JSONL at `.zen-garden/changelog.jsonl`.
    async fn append_changelog(&self, entry: &ChangelogEntry) {
        let changelog_path = self.mount_root.join(CHANGELOG_REL);

        // Ensure .zen-garden/ exists
        if let Some(parent) = changelog_path.parent() {
            if !parent.exists() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }
        }

        // Serialize + newline
        let line = match serde_json::to_string(entry) {
            Ok(json) => format!("{}\n", json),
            Err(e) => {
                warn!(error = %e, "Failed to serialize changelog entry");
                return;
            }
        };

        // Append (open in append mode)
        let result = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&changelog_path)
            .await;

        match result {
            Ok(mut file) => {
                if let Err(e) = file.write_all(line.as_bytes()).await {
                    warn!(error = %e, "Failed to append changelog entry");
                }
            }
            Err(e) => {
                warn!(error = %e, "Failed to open changelog for append");
            }
        }

        // Emit notification tick if configured
        if let (Some(tx), Some(name)) = (&self.notify_tx, &self.seed_bank_name) {
            let (c, m, d) = match entry.op {
                ChangelogOp::C => (1, 0, 0),
                ChangelogOp::M => (0, 1, 0),
                ChangelogOp::D => (0, 0, 1),
            };
            let tick = StorageTick {
                cursor: entry.c.clone(),
                seed_bank: name.clone(),
                creates: c,
                modifies: m,
                deletes: d,
            };
            // Best-effort: if no receivers, that's fine
            let _ = tx.send(tick);
        }
    }

    /// Read all changelog entries from the seed bank.
    pub async fn read_changelog(&self) -> Result<Vec<ChangelogEntry>> {
        self.read_changelog_since(None).await
    }

    /// Read changelog entries newer than the given cursor.
    ///
    /// If `since` is `None`, returns all entries.
    /// Cursors are GUIDv7 strings — string comparison gives correct time ordering.
    pub async fn read_changelog_since(&self, since: Option<&str>) -> Result<Vec<ChangelogEntry>> {
        let changelog_path = self.mount_root.join(CHANGELOG_REL);

        if !changelog_path.exists() {
            return Ok(Vec::new());
        }

        let content = tokio::fs::read_to_string(&changelog_path)
            .await
            .with_context(|| format!("Failed to read {}", changelog_path.display()))?;

        let mut entries = Vec::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<ChangelogEntry>(line) {
                Ok(entry) => {
                    if let Some(cursor) = since {
                        if entry.c.as_str() > cursor {
                            entries.push(entry);
                        }
                    } else {
                        entries.push(entry);
                    }
                }
                Err(e) => {
                    warn!(error = %e, line = %line, "Skipping malformed changelog entry");
                }
            }
        }

        Ok(entries)
    }

    /// Build a `ChangesResponse` for the pull endpoint.
    ///
    /// The raw changelog is append-only and may contain multiple entries for
    /// the same path (e.g. C → M → M, or C → D).  The pull endpoint serves
    /// *squashed* results — one entry per path representing the **net effect**
    /// of all operations since the cursor.
    ///
    /// ## Squash rules (per path)
    ///
    /// | Raw sequence      | Net effect | Rationale                              |
    /// |-------------------|------------|----------------------------------------|
    /// | C                 | C          | File created, grab it.                 |
    /// | C → M             | C          | Created then modified — still a create. |
    /// | C → M → M         | C          | Multiple edits — still a single create. |
    /// | M                 | M          | File modified, grab latest.            |
    /// | M → M → M         | M          | Coalesce — one grab, latest bytes.     |
    /// | D                 | D          | File deleted, propagate removal.       |
    /// | C → D             | _(omit)_   | Created and destroyed in same window.  |
    /// | C → M → D         | _(omit)_   | Net no-op — never existed at boundary. |
    /// | M → D             | D          | Modified then deleted, just delete.    |
    /// | D → C             | M          | Recreated — grab as modification.      |
    /// | D → C → M         | M          | Recreated and edited — still M.        |
    ///
    /// This guarantees the replication task grabs each file at most once per
    /// pull cycle, regardless of how chatty the writer was.
    pub async fn changes_since(&self, since: Option<&str>) -> Result<ChangesResponse> {
        let raw = self.read_changelog_since(since).await?;

        // --- stale cursor detection ---
        // If the caller provided a cursor but the changelog's oldest entry is
        // newer than that cursor, the requested history has been compacted away.
        // Signal full_sync_required so the Dormant reconciles from scratch.
        if let Some(requested) = since {
            if !requested.is_empty() {
                let all = self.read_changelog().await?;
                if let Some(oldest) = all.first() {
                    if requested < oldest.c.as_str() {
                        warn!(
                            requested_cursor = %requested,
                            oldest_cursor = %oldest.c,
                            "Requested cursor predates oldest changelog entry — full sync required"
                        );
                        return Ok(ChangesResponse {
                            cursor: all.last().map(|e| e.c.clone()).unwrap_or_default(),
                            changes: Vec::new(),
                            full_sync_required: true,
                        });
                    }
                }
            }
        }

        let cursor = raw
            .last()
            .map(|e| e.c.clone())
            .or_else(|| since.map(|s| s.to_string()))
            .unwrap_or_default();

        // --- squash: walk entries in order, accumulate net effect per path ---
        use std::collections::HashMap;

        // Track the first-seen op (at window boundary) and the latest entry.
        // `first_op` tells us whether the file already existed before this window.
        struct Accum {
            first_op: ChangelogOp,
            latest: ChangelogEntry,
        }

        // Preserve insertion order for deterministic output.
        let mut order: Vec<String> = Vec::new();
        let mut map: HashMap<String, Accum> = HashMap::new();

        for entry in raw {
            let path = entry.path.clone();
            match map.get_mut(&path) {
                None => {
                    order.push(path.clone());
                    map.insert(
                        path,
                        Accum {
                            first_op: entry.op,
                            latest: entry,
                        },
                    );
                }
                Some(acc) => {
                    acc.latest = entry;
                }
            }
        }

        let mut changes = Vec::with_capacity(order.len());
        for path in order {
            let acc = map.remove(&path).unwrap();
            match (&acc.first_op, &acc.latest.op) {
                // C … D → net no-op, omit entirely
                (ChangelogOp::C, ChangelogOp::D) => continue,
                // C … (C|M) → emit as C (file didn't exist before)
                (ChangelogOp::C, _) => {
                    changes.push(ChangelogEntry {
                        op: ChangelogOp::C,
                        ..acc.latest
                    });
                }
                // D … (C|M) → file was deleted then recreated — emit as M
                (ChangelogOp::D, ChangelogOp::C | ChangelogOp::M) => {
                    changes.push(ChangelogEntry {
                        op: ChangelogOp::M,
                        ..acc.latest
                    });
                }
                // Everything else → keep the latest op as-is (M→M, M→D, D→D, etc.)
                _ => {
                    changes.push(acc.latest);
                }
            }
        }

        Ok(ChangesResponse {
            cursor,
            changes,
            full_sync_required: false,
        })
    }

    /// Read the last-synced cursor (used by Dormant replicas).
    pub async fn read_last_cursor(&self) -> Option<String> {
        let path = self.mount_root.join(LAST_CURSOR_REL);
        tokio::fs::read_to_string(&path)
            .await
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// Persist the last-synced cursor (used by Dormant replicas).
    pub async fn write_last_cursor(&self, cursor: &str) -> Result<()> {
        let path = self.mount_root.join(LAST_CURSOR_REL);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&path, cursor.as_bytes()).await?;
        Ok(())
    }

    // ========================================================================
    // Pin persistence (STORAGE-0006 Phase 5)
    // ========================================================================

    /// Read a persisted pin_id from the seed bank's `.zen-garden/pin.json`.
    ///
    /// Returns `None` if the file does not exist or cannot be parsed.
    pub async fn read_pin(&self) -> Option<String> {
        let path = self.mount_root.join(PIN_REL);
        let data = tokio::fs::read_to_string(&path).await.ok()?;
        let parsed: serde_json::Value = serde_json::from_str(&data).ok()?;
        parsed
            .get("pin_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Persist a pin_id to the seed bank's `.zen-garden/pin.json`.
    pub async fn write_pin(&self, pin_id: &str) -> Result<()> {
        let path = self.mount_root.join(PIN_REL);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let json = serde_json::json!({ "pin_id": pin_id });
        tokio::fs::write(&path, serde_json::to_string_pretty(&json)?.as_bytes()).await?;
        Ok(())
    }

    /// Delete the persisted pin file (on unpin or auto-unpin).
    pub async fn delete_pin(&self) -> Result<()> {
        let path = self.mount_root.join(PIN_REL);
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Compact the changelog, keeping only entries newer than `oldest_cursor`.
    ///
    /// Atomic rewrite: reads all entries, filters, writes to tmp, renames.
    pub async fn compact_changelog(&self, oldest_cursor: &str) -> Result<usize> {
        let all = self.read_changelog().await?;
        let before = all.len();
        let kept: Vec<&ChangelogEntry> = all
            .iter()
            .filter(|e| e.c.as_str() > oldest_cursor)
            .collect();
        let after = kept.len();
        let pruned = before - after;

        if pruned == 0 {
            return Ok(0);
        }

        let changelog_path = self.mount_root.join(CHANGELOG_REL);
        let tmp_path = changelog_path.with_extension("tmp");

        let mut content = String::new();
        for entry in &kept {
            if let Ok(line) = serde_json::to_string(entry) {
                content.push_str(&line);
                content.push('\n');
            }
        }

        tokio::fs::write(&tmp_path, content.as_bytes()).await?;

        #[cfg(windows)]
        if changelog_path.exists() {
            let _ = tokio::fs::remove_file(&changelog_path).await;
        }

        tokio::fs::rename(&tmp_path, &changelog_path).await?;

        debug!(pruned = pruned, kept = after, "Changelog compacted");
        Ok(pruned)
    }
}

// ============================================================================
// Encryption helpers (ChaCha20-Poly1305)
// ============================================================================

/// On-disk format version. Allows future format changes without breaking existing data.
const ENCRYPTION_VERSION: u8 = 1;

/// Encrypt plaintext with ChaCha20-Poly1305.
///
/// Output format: `version(1) + nonce(12) + ciphertext + tag(16)`
fn encrypt(dek: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>> {
    use chacha20poly1305::aead::{Aead, KeyInit, OsRng};
    use chacha20poly1305::{AeadCore, ChaCha20Poly1305};

    let cipher = ChaCha20Poly1305::new(dek.into());
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);

    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

    // version(1) + nonce(12) + ciphertext_with_tag
    let mut output = Vec::with_capacity(1 + 12 + ciphertext.len());
    output.push(ENCRYPTION_VERSION);
    output.extend_from_slice(&nonce);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

/// Decrypt ciphertext produced by `encrypt()`.
fn decrypt(dek: &[u8; 32], data: &[u8]) -> Result<Vec<u8>> {
    use chacha20poly1305::aead::{Aead, KeyInit};
    use chacha20poly1305::{ChaCha20Poly1305, Nonce};

    // Minimum: version(1) + nonce(12) + tag(16) = 29 bytes
    if data.len() < 29 {
        anyhow::bail!(
            "Encrypted data too short ({} bytes, need at least 29)",
            data.len()
        );
    }

    let version = data[0];
    if version != ENCRYPTION_VERSION {
        anyhow::bail!(
            "Unknown encryption version {} (expected {})",
            version,
            ENCRYPTION_VERSION
        );
    }

    let nonce = Nonce::from_slice(&data[1..13]);
    let ciphertext = &data[13..];

    let cipher = ChaCha20Poly1305::new(dek.into());
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| anyhow::anyhow!("Decryption failed — wrong key or corrupted data"))?;

    Ok(plaintext)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_public_store_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let store = SeedBankStore::new_public(tmp.path());

        let rel = Path::new("garden/storage/test/hello.txt");
        let data = b"Hello, seed bank!";

        store.write(rel, data).await.unwrap();
        assert!(store.exists(rel).await);

        let read_back = store.read(rel).await.unwrap();
        assert_eq!(read_back, data);

        // Raw file on disk should be plaintext
        let raw = std::fs::read(tmp.path().join(rel)).unwrap();
        assert_eq!(raw, data);
    }

    #[tokio::test]
    async fn test_encrypted_store_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let dek = SeedBankStore::derive_dek(b"test-pond-key-32-bytes-long!!!!!", "my-bank");
        let store = SeedBankStore::new_encrypted(tmp.path(), dek);

        let rel = Path::new("garden/storage/test/secret.txt");
        let data = b"Top secret data";

        store.write(rel, data).await.unwrap();
        assert!(store.exists(rel).await);

        // Read back through store → plaintext
        let read_back = store.read(rel).await.unwrap();
        assert_eq!(read_back, data);

        // Raw file on disk should NOT be plaintext
        let raw = std::fs::read(tmp.path().join(rel)).unwrap();
        assert_ne!(raw, data);
        assert_eq!(raw[0], ENCRYPTION_VERSION);
        // 1 (version) + 12 (nonce) + len(data) + 16 (tag) = 29 + len(data)
        assert_eq!(raw.len(), 29 + data.len());
    }

    #[tokio::test]
    async fn test_encrypted_store_wrong_key_fails() {
        let tmp = TempDir::new().unwrap();
        let dek = SeedBankStore::derive_dek(b"test-pond-key-32-bytes-long!!!!!", "my-bank");
        let store = SeedBankStore::new_encrypted(tmp.path(), dek);

        let rel = Path::new("garden/storage/test/secret.txt");
        store.write(rel, b"secret").await.unwrap();

        // Try reading with a different key
        let wrong_dek = SeedBankStore::derive_dek(b"wrong-pond-key-32-bytes-long!!!!", "my-bank");
        let wrong_store = SeedBankStore::new_encrypted(tmp.path(), wrong_dek);
        assert!(wrong_store.read(rel).await.is_err());
    }

    #[tokio::test]
    async fn test_replicas_share_dek() {
        // Two replicas of the same name derive the same DEK
        let pdk = b"pond-data-key-for-testing-32byte";
        let dek1 = SeedBankStore::derive_dek(pdk, "private-seed-bank");
        let dek2 = SeedBankStore::derive_dek(pdk, "private-seed-bank");
        assert_eq!(dek1, dek2);

        // Different name → different DEK
        let dek3 = SeedBankStore::derive_dek(pdk, "public-seed-bank");
        assert_ne!(dek1, dek3);
    }

    #[tokio::test]
    async fn test_delete() {
        let tmp = TempDir::new().unwrap();
        let store = SeedBankStore::new_public(tmp.path());

        let rel = Path::new("garden/storage/test/deleteme.txt");
        store.write(rel, b"bye").await.unwrap();
        assert!(store.exists(rel).await);

        assert!(store.delete(rel).await.unwrap());
        assert!(!store.exists(rel).await);
        assert!(!store.delete(rel).await.unwrap()); // Already gone
    }

    #[tokio::test]
    async fn test_string_ops() {
        let tmp = TempDir::new().unwrap();
        let dek = SeedBankStore::derive_dek(b"test-pond-key-32-bytes-long!!!!!", "my-bank");
        let store = SeedBankStore::new_encrypted(tmp.path(), dek);

        let rel = Path::new("garden/memories/index.json");
        let json = r#"{"version": 1, "snapshots": []}"#;

        store.write_string(rel, json).await.unwrap();
        let read_back = store.read_string(rel).await.unwrap();
        assert_eq!(read_back, json);
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = [42u8; 32];
        let plaintext = b"Hello, World!";

        let encrypted = encrypt(&key, plaintext).unwrap();
        assert_ne!(encrypted.as_slice(), plaintext);

        let decrypted = decrypt(&key, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_decrypt_wrong_key() {
        let key1 = [42u8; 32];
        let key2 = [99u8; 32];
        let plaintext = b"sensitive data";

        let encrypted = encrypt(&key1, plaintext).unwrap();
        assert!(decrypt(&key2, &encrypted).is_err());
    }

    #[test]
    fn test_decrypt_truncated_data() {
        let key = [42u8; 32];
        assert!(decrypt(&key, &[1u8; 10]).is_err()); // Too short
        assert!(decrypt(&key, &[]).is_err()); // Empty
    }

    #[test]
    fn test_decrypt_wrong_version() {
        let key = [42u8; 32];
        let plaintext = b"test";
        let mut encrypted = encrypt(&key, plaintext).unwrap();
        encrypted[0] = 99; // Wrong version
        assert!(decrypt(&key, &encrypted).is_err());
    }

    // ========================================================================
    // Changelog tests
    // ========================================================================

    #[tokio::test]
    async fn test_changelog_appended_on_write() {
        let tmp = TempDir::new().unwrap();
        let store = SeedBankStore::new_public(tmp.path());

        // Create .zen-garden dir so changelog can be written
        tokio::fs::create_dir_all(tmp.path().join(".zen-garden"))
            .await
            .unwrap();

        let rel = Path::new("garden/storage/bucket/file.txt");
        store.write(rel, b"hello").await.unwrap();

        let entries = store.read_changelog().await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].op, ChangelogOp::C);
        assert_eq!(entries[0].path, "garden/storage/bucket/file.txt");
        assert_eq!(entries[0].bytes, Some(5));
    }

    #[tokio::test]
    async fn test_changelog_modify_on_overwrite() {
        let tmp = TempDir::new().unwrap();
        let store = SeedBankStore::new_public(tmp.path());
        tokio::fs::create_dir_all(tmp.path().join(".zen-garden"))
            .await
            .unwrap();

        let rel = Path::new("garden/storage/bucket/file.txt");
        store.write(rel, b"v1").await.unwrap();
        store.write(rel, b"v2 longer").await.unwrap();

        let entries = store.read_changelog().await.unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].op, ChangelogOp::C);
        assert_eq!(entries[1].op, ChangelogOp::M);
        assert_eq!(entries[1].bytes, Some(9));
    }

    #[tokio::test]
    async fn test_changelog_delete_entry() {
        let tmp = TempDir::new().unwrap();
        let store = SeedBankStore::new_public(tmp.path());
        tokio::fs::create_dir_all(tmp.path().join(".zen-garden"))
            .await
            .unwrap();

        let rel = Path::new("garden/storage/bucket/file.txt");
        store.write(rel, b"data").await.unwrap();
        store.delete(rel).await.unwrap();

        let entries = store.read_changelog().await.unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].op, ChangelogOp::C);
        assert_eq!(entries[1].op, ChangelogOp::D);
        assert_eq!(entries[1].bytes, None);
    }

    #[tokio::test]
    async fn test_changelog_since_cursor() {
        let tmp = TempDir::new().unwrap();
        let store = SeedBankStore::new_public(tmp.path());
        tokio::fs::create_dir_all(tmp.path().join(".zen-garden"))
            .await
            .unwrap();

        store
            .write(Path::new("garden/storage/a.txt"), b"a")
            .await
            .unwrap();
        store
            .write(Path::new("garden/storage/b.txt"), b"b")
            .await
            .unwrap();
        store
            .write(Path::new("garden/storage/c.txt"), b"c")
            .await
            .unwrap();

        let all = store.read_changelog().await.unwrap();
        assert_eq!(all.len(), 3);

        // Get entries after the first cursor
        let since = &all[0].c;
        let after = store.read_changelog_since(Some(since)).await.unwrap();
        assert_eq!(after.len(), 2);
        assert_eq!(after[0].path, "garden/storage/b.txt");
        assert_eq!(after[1].path, "garden/storage/c.txt");
    }

    #[tokio::test]
    async fn test_changelog_no_entry_for_metadata() {
        let tmp = TempDir::new().unwrap();
        let store = SeedBankStore::new_public(tmp.path());
        tokio::fs::create_dir_all(tmp.path().join(".zen-garden"))
            .await
            .unwrap();

        // Writes to .zen-garden/ paths should NOT generate changelog entries
        let rel = Path::new(".zen-garden/manifest.json");
        store.write(rel, b"{}").await.unwrap();

        let entries = store.read_changelog().await.unwrap();
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn test_changelog_compact() {
        let tmp = TempDir::new().unwrap();
        let store = SeedBankStore::new_public(tmp.path());
        tokio::fs::create_dir_all(tmp.path().join(".zen-garden"))
            .await
            .unwrap();

        store
            .write(Path::new("garden/storage/a.txt"), b"a")
            .await
            .unwrap();
        store
            .write(Path::new("garden/storage/b.txt"), b"b")
            .await
            .unwrap();
        store
            .write(Path::new("garden/storage/c.txt"), b"c")
            .await
            .unwrap();

        let all = store.read_changelog().await.unwrap();
        assert_eq!(all.len(), 3);

        // Compact: keep only entries after the second
        let pruned = store.compact_changelog(&all[1].c).await.unwrap();
        assert_eq!(pruned, 2); // removed entries 0 and 1

        let remaining = store.read_changelog().await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].path, "garden/storage/c.txt");
    }

    #[tokio::test]
    async fn test_changes_since_response() {
        let tmp = TempDir::new().unwrap();
        let store = SeedBankStore::new_public(tmp.path());
        tokio::fs::create_dir_all(tmp.path().join(".zen-garden"))
            .await
            .unwrap();

        store
            .write(Path::new("garden/storage/a.txt"), b"a")
            .await
            .unwrap();
        store
            .write(Path::new("garden/storage/b.txt"), b"bb")
            .await
            .unwrap();

        let resp = store.changes_since(None).await.unwrap();
        assert_eq!(resp.changes.len(), 2);
        assert!(!resp.cursor.is_empty());

        // Now query since the first cursor
        let all = store.read_changelog().await.unwrap();
        let resp2 = store.changes_since(Some(&all[0].c)).await.unwrap();
        assert_eq!(resp2.changes.len(), 1);
        assert_eq!(resp2.changes[0].path, "garden/storage/b.txt");
    }

    #[tokio::test]
    async fn test_last_cursor_persistence() {
        let tmp = TempDir::new().unwrap();
        let store = SeedBankStore::new_public(tmp.path());

        assert!(store.read_last_cursor().await.is_none());

        store
            .write_last_cursor("01956a3e-1234-7def-8000-abcdef012345")
            .await
            .unwrap();
        let cursor = store.read_last_cursor().await;
        assert_eq!(
            cursor.as_deref(),
            Some("01956a3e-1234-7def-8000-abcdef012345")
        );
    }

    #[tokio::test]
    async fn test_notify_tx_fires_on_write() {
        let tmp = TempDir::new().unwrap();
        let (tx, mut rx) = tokio::sync::broadcast::channel::<StorageTick>(16);
        let store =
            SeedBankStore::new_public(tmp.path()).with_notifications("test-bank".to_string(), tx);

        tokio::fs::create_dir_all(tmp.path().join(".zen-garden"))
            .await
            .unwrap();

        store
            .write(Path::new("garden/storage/obj.bin"), b"data")
            .await
            .unwrap();

        let tick = rx.try_recv().unwrap();
        assert_eq!(tick.seed_bank, "test-bank");
        assert_eq!(tick.creates, 1);
        assert_eq!(tick.modifies, 0);
        assert_eq!(tick.deletes, 0);
        assert!(!tick.cursor.is_empty());
    }

    // ========================================================================
    // Squash tests — changes_since() must coalesce per-path entries
    // ========================================================================

    #[tokio::test]
    async fn test_squash_create_then_delete_is_omitted() {
        let tmp = TempDir::new().unwrap();
        let store = SeedBankStore::new_public(tmp.path());
        tokio::fs::create_dir_all(tmp.path().join(".zen-garden"))
            .await
            .unwrap();

        // C → D for the same path = net no-op
        store
            .write(Path::new("garden/storage/ephemeral.txt"), b"tmp")
            .await
            .unwrap();
        store
            .delete(Path::new("garden/storage/ephemeral.txt"))
            .await
            .unwrap();

        let resp = store.changes_since(None).await.unwrap();
        assert!(resp.changes.is_empty(), "C→D should be squashed to nothing");
    }

    #[tokio::test]
    async fn test_squash_create_modify_delete_is_omitted() {
        let tmp = TempDir::new().unwrap();
        let store = SeedBankStore::new_public(tmp.path());
        tokio::fs::create_dir_all(tmp.path().join(".zen-garden"))
            .await
            .unwrap();

        // C → M → D = net no-op (never existed at boundary)
        store
            .write(Path::new("garden/storage/tmp.bin"), b"v1")
            .await
            .unwrap();
        store
            .write(Path::new("garden/storage/tmp.bin"), b"v2")
            .await
            .unwrap();
        store
            .delete(Path::new("garden/storage/tmp.bin"))
            .await
            .unwrap();

        let resp = store.changes_since(None).await.unwrap();
        assert!(
            resp.changes.is_empty(),
            "C→M→D should be squashed to nothing"
        );
    }

    #[tokio::test]
    async fn test_squash_multiple_modifies_coalesce() {
        let tmp = TempDir::new().unwrap();
        let store = SeedBankStore::new_public(tmp.path());
        tokio::fs::create_dir_all(tmp.path().join(".zen-garden"))
            .await
            .unwrap();

        // Write creates the file, then overwrite it twice
        store
            .write(Path::new("garden/storage/chatty.txt"), b"v1")
            .await
            .unwrap();

        // Grab a cursor after the initial create
        let baseline = store.read_changelog().await.unwrap();
        let cursor = &baseline.last().unwrap().c;

        // Two more writes = M → M in the window
        store
            .write(Path::new("garden/storage/chatty.txt"), b"v2 longer")
            .await
            .unwrap();
        store
            .write(Path::new("garden/storage/chatty.txt"), b"v3 longest")
            .await
            .unwrap();

        let resp = store.changes_since(Some(cursor)).await.unwrap();
        assert_eq!(resp.changes.len(), 1, "Multiple M's should coalesce to one");
        assert_eq!(resp.changes[0].op, ChangelogOp::M);
        assert_eq!(resp.changes[0].bytes, Some(10)); // latest size wins
    }

    #[tokio::test]
    async fn test_squash_create_then_modifies_stays_create() {
        let tmp = TempDir::new().unwrap();
        let store = SeedBankStore::new_public(tmp.path());
        tokio::fs::create_dir_all(tmp.path().join(".zen-garden"))
            .await
            .unwrap();

        // C → M → M within same window = still a C (file didn't exist before)
        store
            .write(Path::new("garden/storage/new.txt"), b"v1")
            .await
            .unwrap();
        store
            .write(Path::new("garden/storage/new.txt"), b"v2")
            .await
            .unwrap();
        store
            .write(Path::new("garden/storage/new.txt"), b"v3 final")
            .await
            .unwrap();

        let resp = store.changes_since(None).await.unwrap();
        assert_eq!(resp.changes.len(), 1);
        assert_eq!(resp.changes[0].op, ChangelogOp::C, "C→M→M squashes to C");
        assert_eq!(resp.changes[0].bytes, Some(8)); // latest size
    }

    #[tokio::test]
    async fn test_squash_delete_then_recreate_becomes_modify() {
        let tmp = TempDir::new().unwrap();
        let store = SeedBankStore::new_public(tmp.path());
        tokio::fs::create_dir_all(tmp.path().join(".zen-garden"))
            .await
            .unwrap();

        // Create file, snapshot cursor, then D → C within the window
        store
            .write(Path::new("garden/storage/phoenix.txt"), b"original")
            .await
            .unwrap();

        let baseline = store.read_changelog().await.unwrap();
        let cursor = &baseline.last().unwrap().c;

        store
            .delete(Path::new("garden/storage/phoenix.txt"))
            .await
            .unwrap();
        store
            .write(Path::new("garden/storage/phoenix.txt"), b"reborn")
            .await
            .unwrap();

        let resp = store.changes_since(Some(cursor)).await.unwrap();
        assert_eq!(resp.changes.len(), 1);
        assert_eq!(resp.changes[0].op, ChangelogOp::M, "D→C squashes to M");
        assert_eq!(resp.changes[0].bytes, Some(6));
    }

    #[tokio::test]
    async fn test_squash_modify_then_delete_stays_delete() {
        let tmp = TempDir::new().unwrap();
        let store = SeedBankStore::new_public(tmp.path());
        tokio::fs::create_dir_all(tmp.path().join(".zen-garden"))
            .await
            .unwrap();

        // Create the file before the window
        store
            .write(Path::new("garden/storage/doomed.txt"), b"exists")
            .await
            .unwrap();

        let baseline = store.read_changelog().await.unwrap();
        let cursor = &baseline.last().unwrap().c;

        // M → D within the window
        store
            .write(Path::new("garden/storage/doomed.txt"), b"edited")
            .await
            .unwrap();
        store
            .delete(Path::new("garden/storage/doomed.txt"))
            .await
            .unwrap();

        let resp = store.changes_since(Some(cursor)).await.unwrap();
        assert_eq!(resp.changes.len(), 1);
        assert_eq!(resp.changes[0].op, ChangelogOp::D, "M→D stays D");
    }

    #[tokio::test]
    async fn test_squash_mixed_paths_independent() {
        let tmp = TempDir::new().unwrap();
        let store = SeedBankStore::new_public(tmp.path());
        tokio::fs::create_dir_all(tmp.path().join(".zen-garden"))
            .await
            .unwrap();

        // a.txt: C (new file, stays in result)
        // b.txt: C → D (omitted)
        // c.txt: C → M (squashed to C)
        store
            .write(Path::new("garden/storage/a.txt"), b"a")
            .await
            .unwrap();
        store
            .write(Path::new("garden/storage/b.txt"), b"b")
            .await
            .unwrap();
        store
            .write(Path::new("garden/storage/c.txt"), b"c1")
            .await
            .unwrap();
        store
            .delete(Path::new("garden/storage/b.txt"))
            .await
            .unwrap();
        store
            .write(Path::new("garden/storage/c.txt"), b"c2")
            .await
            .unwrap();

        let resp = store.changes_since(None).await.unwrap();
        assert_eq!(resp.changes.len(), 2, "b.txt (C→D) should be omitted");

        assert_eq!(resp.changes[0].path, "garden/storage/a.txt");
        assert_eq!(resp.changes[0].op, ChangelogOp::C);

        assert_eq!(resp.changes[1].path, "garden/storage/c.txt");
        assert_eq!(resp.changes[1].op, ChangelogOp::C);
        assert_eq!(resp.changes[1].bytes, Some(2)); // latest size
    }

    // ========================================================================
    // Full-sync fallback tests
    // ========================================================================

    #[tokio::test]
    async fn test_stale_cursor_triggers_full_sync() {
        let tmp = TempDir::new().unwrap();
        let store = SeedBankStore::new_public(tmp.path());
        tokio::fs::create_dir_all(tmp.path().join(".zen-garden"))
            .await
            .unwrap();

        // Write three entries
        store
            .write(Path::new("garden/storage/a.txt"), b"a")
            .await
            .unwrap();
        store
            .write(Path::new("garden/storage/b.txt"), b"b")
            .await
            .unwrap();
        store
            .write(Path::new("garden/storage/c.txt"), b"c")
            .await
            .unwrap();

        // Grab cursor of entry 0 (will be used as "stale" cursor later)
        let all = store.read_changelog().await.unwrap();
        let old_cursor = all[0].c.clone();

        // Compact entries 0 and 1, keeping only entry 2
        store.compact_changelog(&all[1].c).await.unwrap();

        // Now request with old_cursor — it predates the remaining entry
        let resp = store.changes_since(Some(&old_cursor)).await.unwrap();

        assert!(
            resp.full_sync_required,
            "Stale cursor should trigger full_sync_required"
        );
        assert!(
            resp.changes.is_empty(),
            "Stale cursor response should have no changes"
        );
    }

    #[tokio::test]
    async fn test_valid_cursor_no_full_sync() {
        let tmp = TempDir::new().unwrap();
        let store = SeedBankStore::new_public(tmp.path());
        tokio::fs::create_dir_all(tmp.path().join(".zen-garden"))
            .await
            .unwrap();

        store
            .write(Path::new("garden/storage/a.txt"), b"a")
            .await
            .unwrap();
        store
            .write(Path::new("garden/storage/b.txt"), b"b")
            .await
            .unwrap();

        let all = store.read_changelog().await.unwrap();
        let cursor = &all[0].c;

        let resp = store.changes_since(Some(cursor)).await.unwrap();
        assert!(
            !resp.full_sync_required,
            "Valid cursor should not trigger full_sync_required"
        );
        assert_eq!(resp.changes.len(), 1);
        assert_eq!(resp.changes[0].path, "garden/storage/b.txt");
    }

    #[tokio::test]
    async fn test_no_cursor_no_full_sync() {
        let tmp = TempDir::new().unwrap();
        let store = SeedBankStore::new_public(tmp.path());
        tokio::fs::create_dir_all(tmp.path().join(".zen-garden"))
            .await
            .unwrap();

        store
            .write(Path::new("garden/storage/x.txt"), b"x")
            .await
            .unwrap();

        let resp = store.changes_since(None).await.unwrap();
        assert!(
            !resp.full_sync_required,
            "No cursor (initial sync) should not trigger full_sync_required"
        );
        assert_eq!(resp.changes.len(), 1);
    }

    #[tokio::test]
    async fn test_empty_changelog_with_cursor_no_full_sync() {
        let tmp = TempDir::new().unwrap();
        let store = SeedBankStore::new_public(tmp.path());
        tokio::fs::create_dir_all(tmp.path().join(".zen-garden"))
            .await
            .unwrap();

        // Empty changelog — no entries to compare against.
        // A cursor against an empty changelog means nothing happened yet.
        let resp = store
            .changes_since(Some("01956a3e-0000-7000-8000-000000000000"))
            .await
            .unwrap();
        assert!(
            !resp.full_sync_required,
            "Empty changelog should not trigger full_sync_required"
        );
        assert!(resp.changes.is_empty());
    }
}
