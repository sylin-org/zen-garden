//! Content-addressed dependency cache for skill models (ORCH-0029).
//!
//! Layout:
//! ```text
//! {data_dir}/cache/dependencies/{provider}/
//!   manifest.json           — checksum + alias registry
//!   {model files}           — cached model files (content-addressed)
//!
//! {data_dir}/cache/dependencies/workspace/{skill_moniker}/
//!   {downloading files}     — ephemeral, cleaned after use
//! ```
//!
//! ## Invariants (ORCH-0029 §Phase 2 — cache + provisioner + queue)
//!
//! - **`manifest.json` schema**: `{ files: { name: "sha256:hex" },
//!   aliases: { requested_name: canonical_name } }`. Byte-compatible
//!   with the prior system's `.zen-garden/ai-orchestrator/cache/
//!   dependencies/comfyui/manifest.json` — the 90 GB of models
//!   already on disk must read without modification.
//! - **Streaming downloads** compute SHA-256 in-line with the write.
//!   Never buffer more than one chunk in memory.
//! - **Resume support**: if a partial file exists, hash the existing
//!   bytes and send `Range: bytes={n}-`. If the server returns 200
//!   (no Range support), start over cleanly.
//! - **Atomic manifest writes**: write `.tmp`, rename. Losing the
//!   manifest loses the ability to resolve aliases, which would
//!   strand the on-disk models.
//! - **4-case dedup on ingest**:
//!   - **A** (same checksum, same name)    → drop workspace copy
//!   - **B** (same checksum, different name) → record alias
//!   - **C** (new checksum, name available) → move to cache
//!   - **D** (new checksum, name conflict)  → `name(2).ext` rename

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::AsyncWriteExt;

// ── Manifest ──────────────────────────────────────────────────

/// On-disk manifest — tracks files, checksums, and aliases.
///
/// Serializes with the exact field names the prior system wrote, so
/// the existing 90 GB cache reads as-is:
///
/// ```json
/// {
///   "files":   { "model.safetensors": "sha256:abc123..." },
///   "aliases": { "requested_name.safetensors": "canonical_name.safetensors" }
/// }
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DependencyManifest {
    /// `filename → "sha256:{hex}"`
    #[serde(default)]
    pub files: HashMap<String, String>,
    /// `requested_name → canonical_name` (alias chain). The requested
    /// name might not exist as a real file; the canonical name is
    /// always a live entry in `files`.
    #[serde(default)]
    pub aliases: HashMap<String, String>,
}

impl DependencyManifest {
    /// Load from disk, or return an empty manifest if the file is
    /// absent. Parse failures log a warning and also return empty —
    /// the only thing that strands a cache is deleting files with a
    /// corrupt manifest, which we can't distinguish from first run.
    pub async fn load(path: &Path) -> Self {
        match fs::read_to_string(path).await {
            Ok(json) => match serde_json::from_str(&json) {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!(
                        path = %path.display(),
                        error = %e,
                        "dependency manifest parse failed; starting empty"
                    );
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    }

    /// Save atomically — write to `path.json.tmp`, rename into place.
    pub async fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .with_context(|| format!("create manifest parent {}", parent.display()))?;
        }
        let json = serde_json::to_string_pretty(self).context("serialize manifest")?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, json.as_bytes())
            .await
            .with_context(|| format!("write manifest tmp {}", tmp.display()))?;
        fs::rename(&tmp, path)
            .await
            .with_context(|| format!("rename manifest {}", path.display()))?;
        Ok(())
    }

    /// Find the canonical filename for a known checksum.
    pub fn filename_for_checksum(&self, checksum: &str) -> Option<&str> {
        self.files
            .iter()
            .find(|(_, cs)| cs.eq_ignore_ascii_case(checksum))
            .map(|(name, _)| name.as_str())
    }

    /// Resolve a requested filename through the alias chain. Returns
    /// the canonical name (which may or may not be a live file — the
    /// caller checks `files.contains_key`).
    pub fn resolve(&self, requested: &str) -> String {
        self.aliases
            .get(requested)
            .cloned()
            .unwrap_or_else(|| requested.to_string())
    }

    /// Check whether a filename is cached (either directly or via
    /// alias).
    pub fn is_cached(&self, filename: &str) -> bool {
        let canonical = self.resolve(filename);
        self.files.contains_key(&canonical)
    }

    /// Generate a non-conflicting filename: `name(2).ext`,
    /// `name(3).ext`, … Used by case D of `ingest_to_cache` when a
    /// new checksum collides with an existing name.
    pub fn next_available_name(&self, filename: &str) -> String {
        let (stem, ext) = split_stem_ext(filename);
        let mut n = 2;
        loop {
            let candidate = if ext.is_empty() {
                format!("{stem}({n})")
            } else {
                format!("{stem}({n}).{ext}")
            };
            if !self.files.contains_key(&candidate) {
                return candidate;
            }
            n += 1;
        }
    }
}

/// Split `"model.safetensors"` into `("model", "safetensors")`.
/// `"model.tar.gz"` → `("model.tar", "gz")`. No extension → `("name", "")`.
fn split_stem_ext(filename: &str) -> (&str, &str) {
    match filename.rsplit_once('.') {
        Some((stem, ext)) => (stem, ext),
        None => (filename, ""),
    }
}

// ── Cache directory layout ───────────────────────────────────

/// Paths for a provider's dependency cache.
#[derive(Debug, Clone)]
pub struct CachePaths {
    /// `{data_dir}/cache/dependencies/{provider}/`
    pub provider_dir: PathBuf,
    /// `{data_dir}/cache/dependencies/{provider}/manifest.json`
    pub manifest_path: PathBuf,
    /// `{data_dir}/cache/dependencies/workspace/`
    pub workspace_dir: PathBuf,
}

impl CachePaths {
    pub fn new(data_dir: &Path, provider: &str) -> Self {
        let base = data_dir.join("cache").join("dependencies");
        let provider_dir = base.join(provider);
        Self {
            manifest_path: provider_dir.join("manifest.json"),
            provider_dir,
            workspace_dir: base.join("workspace"),
        }
    }

    pub async fn ensure_dirs(&self) -> Result<()> {
        fs::create_dir_all(&self.provider_dir)
            .await
            .with_context(|| format!("create provider cache dir {}", self.provider_dir.display()))?;
        fs::create_dir_all(&self.workspace_dir)
            .await
            .with_context(|| format!("create workspace dir {}", self.workspace_dir.display()))?;
        Ok(())
    }

    pub fn workspace_for_skill(&self, skill_moniker: &str) -> PathBuf {
        self.workspace_dir.join(skill_moniker)
    }

    pub fn file_path(&self, filename: &str) -> PathBuf {
        self.provider_dir.join(filename)
    }
}

// ── Streaming download with SHA-256 ──────────────────────────

/// Progress callback: `(downloaded_bytes, total_bytes_if_known)`.
pub type ProgressFn = Box<dyn Fn(u64, Option<u64>) + Send + Sync>;

/// Download a file by streaming to disk, computing SHA-256 in the
/// same pass. Returns `(path, "sha256:hex")` on success.
///
/// - If a partial file exists, the existing bytes are hashed first
///   and an HTTP `Range:` header resumes the transfer.
/// - If the server returns 200 instead of 206 (no Range support), the
///   stream restarts from byte 0 with a fresh hasher.
/// - 401/403 responses bail with a clear error, stripping query
///   parameters from the URL in the log so tokens don't leak.
/// - The optional `on_progress` callback fires at most once every
///   `progress_interval` (10s for small files, 30s for >1 GB).
pub async fn stream_download(
    http: &reqwest::Client,
    url: &str,
    dest: &Path,
    total_bytes: Option<u64>,
    on_progress: Option<ProgressFn>,
) -> Result<(PathBuf, String)> {
    use futures_util::StreamExt;
    use sha2::{Digest, Sha256};

    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).await?;
    }

    // Check for an existing partial file — if present, hash it and
    // ask the server to resume from where we left off.
    let existing_size = fs::metadata(dest).await.map(|m| m.len()).unwrap_or(0);
    let mut hasher = Sha256::new();

    let mut file = if existing_size > 0 {
        tracing::info!(
            path = %dest.display(),
            existing_bytes = existing_size,
            "resuming partial download"
        );
        let existing_data = fs::read(dest).await?;
        hasher.update(&existing_data);
        drop(existing_data);
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(dest)
            .await
            .context("open partial file for append")?
    } else {
        fs::File::create(dest).await.context("create download file")?
    };

    // Build the request with an optional Range header.
    let mut req = http.get(url);
    if existing_size > 0 {
        req = req.header("Range", format!("bytes={existing_size}-"));
    }
    let resp = req.send().await.with_context(|| format!("GET {}", safe_url(url)))?;

    // Handle the response status up-front before streaming bytes.
    if existing_size > 0 && resp.status().as_u16() == 200 {
        // Server doesn't support Range — start over cleanly.
        tracing::debug!("server does not support Range; restarting download");
        drop(file);
        hasher = Sha256::new();
        file = fs::File::create(dest).await?;
    } else if matches!(resp.status().as_u16(), 401 | 403) {
        anyhow::bail!(
            "download requires authentication (HTTP {}). \
             Set the API key in Dashboard \u{2192} Secrets for this provider. URL: {}",
            resp.status(),
            safe_url(url)
        );
    } else if !resp.status().is_success() {
        anyhow::bail!(
            "download failed HTTP {}: {}",
            resp.status(),
            safe_url(url)
        );
    }

    let content_length = resp.content_length();
    let total = total_bytes.or_else(|| content_length.map(|cl| cl + existing_size));

    let mut stream = resp.bytes_stream();
    let mut downloaded = existing_size;
    let mut last_log = std::time::Instant::now();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk
            .with_context(|| format!("read chunk from {}", safe_url(url)))?;
        file.write_all(&chunk).await.context("write chunk")?;
        hasher.update(&chunk);
        downloaded += chunk.len() as u64;

        // Throttled progress reporting — 30s for >1 GB files, 10s
        // for everything else. Large downloads don't need a chatty
        // log stream.
        let log_interval_secs = if total.unwrap_or(0) > 1_000_000_000 {
            30
        } else {
            10
        };
        if last_log.elapsed() > std::time::Duration::from_secs(log_interval_secs) {
            if let Some(t) = total {
                let pct = ((downloaded as f64 / t as f64) * 100.0) as u32;
                tracing::info!(
                    url = %safe_url(url),
                    downloaded,
                    total = t,
                    pct,
                    "download progress"
                );
            }
            if let Some(ref cb) = on_progress {
                cb(downloaded, total);
            }
            last_log = std::time::Instant::now();
        }
    }

    file.flush().await?;
    drop(file);

    let checksum = format!("sha256:{:x}", hasher.finalize());
    tracing::info!(
        path = %dest.display(),
        bytes = downloaded,
        checksum = %checksum,
        "download complete"
    );

    Ok((dest.to_path_buf(), checksum))
}

/// Strip query parameters from a URL for logging. CivitAI and
/// HuggingFace download URLs carry auth tokens as query params; they
/// must never land in the logs.
fn safe_url(url: &str) -> &str {
    url.split('?').next().unwrap_or(url)
}

/// Compute the SHA-256 of an existing file on disk.
///
/// Used by the provisioner when the orchestrator restarts mid-
/// download and needs to know whether an orphaned workspace file is
/// usable.
pub async fn checksum_file(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    use tokio::io::AsyncReadExt;

    let mut file = fs::File::open(path)
        .await
        .with_context(|| format!("open for checksum: {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

// ── Dedup + ingest ───────────────────────────────────────────

/// The four outcomes of `ingest_to_cache`.
#[derive(Debug)]
pub enum IngestResult {
    /// Case C: new checksum, name available. File moved into cache.
    Added { canonical_name: String },
    /// Case A: same checksum under the same name. Workspace copy dropped.
    AlreadyCached,
    /// Case B: same checksum under a different name. Alias recorded.
    Aliased { canonical_name: String, alias_from: String },
    /// Case D: new checksum collided with an existing name. Stored
    /// as `name(2).ext`.
    Renamed { canonical_name: String, original_name: String },
}

/// Move a downloaded file from the workspace into the provider cache
/// with dedup.
///
/// The caller holds the manifest lock (usually by loading + saving
/// around a sequence of ingest calls). This function does NOT write
/// the manifest to disk — it mutates the in-memory struct only.
pub async fn ingest_to_cache(
    manifest: &mut DependencyManifest,
    cache_dir: &Path,
    workspace_file: &Path,
    requested_name: &str,
    checksum: &str,
) -> Result<IngestResult> {
    // Is this checksum already in the cache?
    let existing = manifest.filename_for_checksum(checksum).map(String::from);
    if let Some(existing_name) = existing {
        if existing_name == requested_name {
            // Case A — drop the duplicate.
            let _ = fs::remove_file(workspace_file).await;
            return Ok(IngestResult::AlreadyCached);
        }
        // Case B — record alias, drop workspace copy.
        manifest
            .aliases
            .insert(requested_name.to_string(), existing_name.clone());
        let _ = fs::remove_file(workspace_file).await;
        return Ok(IngestResult::Aliased {
            canonical_name: existing_name,
            alias_from: requested_name.to_string(),
        });
    }

    // New checksum — check for name conflicts.
    if manifest.files.contains_key(requested_name) {
        // Case D — rename.
        let new_name = manifest.next_available_name(requested_name);
        let dest = cache_dir.join(&new_name);
        fs::rename(workspace_file, &dest)
            .await
            .with_context(|| format!("move {} to cache as {}", workspace_file.display(), new_name))?;
        manifest.files.insert(new_name.clone(), checksum.to_string());
        return Ok(IngestResult::Renamed {
            canonical_name: new_name,
            original_name: requested_name.to_string(),
        });
    }

    // Case C — move to cache under the requested name.
    let dest = cache_dir.join(requested_name);
    fs::rename(workspace_file, &dest)
        .await
        .with_context(|| format!("move {} to cache", workspace_file.display()))?;
    manifest.files.insert(requested_name.to_string(), checksum.to_string());
    Ok(IngestResult::Added {
        canonical_name: requested_name.to_string(),
    })
}

// ── Garbage collection ──────────────────────────────────────

/// Remove cached models that are not referenced by any skill on disk.
///
/// Scans every `skill.json` under `skills_dir`, collects the
/// referenced filenames (and their alias resolutions), and deletes
/// cache entries with zero references. Also cleans stale aliases
/// pointing to removed files. Returns the number of files removed.
pub async fn garbage_collect(skills_dir: &Path, cache_paths: &CachePaths) -> Result<usize> {
    let mut manifest = DependencyManifest::load(&cache_paths.manifest_path).await;
    if manifest.files.is_empty() {
        return Ok(0);
    }

    let mut referenced: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Walk skills_dir/{provider}/{moniker}/skill.json and collect
    // required_models[].filename for every loadable skill.
    let provider_dirs = match read_subdirs(skills_dir).await {
        Ok(d) => d,
        Err(_) => return Ok(0),
    };
    for provider_dir in provider_dirs {
        let monikers = match read_subdirs(&provider_dir).await {
            Ok(d) => d,
            Err(_) => continue,
        };
        for moniker_dir in monikers {
            let skill_path = moniker_dir.join("skill.json");
            let Ok(json) = fs::read_to_string(&skill_path).await else {
                continue;
            };
            let Ok(raw) = serde_json::from_str::<serde_json::Value>(&json) else {
                continue;
            };
            if let Some(models) = raw.get("required_models").and_then(|v| v.as_array()) {
                for model in models {
                    if let Some(filename) = model.get("filename").and_then(|v| v.as_str()) {
                        referenced.insert(filename.to_string());
                        referenced.insert(manifest.resolve(filename));
                    }
                }
            }
        }
    }

    // Remove unreferenced files.
    let mut removed = 0;
    let unreferenced: Vec<String> = manifest
        .files
        .keys()
        .filter(|name| !referenced.contains(*name))
        .cloned()
        .collect();
    for name in &unreferenced {
        let path = cache_paths.provider_dir.join(name);
        if let Err(e) = fs::remove_file(&path).await {
            tracing::debug!(
                file = %name,
                error = %e,
                "cache GC: failed to remove model (may already be gone)"
            );
        }
        manifest.files.remove(name);
        removed += 1;
        tracing::info!(file = %name, "cache GC: removed unreferenced model");
    }

    // Clean stale aliases.
    let stale_aliases: Vec<String> = manifest
        .aliases
        .iter()
        .filter(|(_, target)| !manifest.files.contains_key(*target))
        .map(|(alias, _)| alias.clone())
        .collect();
    for alias in &stale_aliases {
        manifest.aliases.remove(alias);
    }

    if removed > 0 || !stale_aliases.is_empty() {
        manifest.save(&cache_paths.manifest_path).await?;
    }
    Ok(removed)
}

async fn read_subdirs(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut entries = fs::read_dir(dir)
        .await
        .with_context(|| format!("read_dir {}", dir.display()))?;
    let mut out = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        if entry.file_type().await?.is_dir() {
            out.push(entry.path());
        }
    }
    Ok(out)
}

// ── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_filename_for_checksum() {
        let mut m = DependencyManifest::default();
        m.files.insert("model.pth".into(), "sha256:abc123".into());
        assert_eq!(m.filename_for_checksum("sha256:abc123"), Some("model.pth"));
        assert_eq!(m.filename_for_checksum("sha256:zzz"), None);
        // Case-insensitive match (SHA-256 hex can come from either case).
        assert_eq!(m.filename_for_checksum("SHA256:ABC123"), Some("model.pth"));
    }

    #[test]
    fn manifest_resolve_alias_chain() {
        let mut m = DependencyManifest::default();
        m.aliases.insert("alt.pth".into(), "real.pth".into());
        assert_eq!(m.resolve("alt.pth"), "real.pth");
        assert_eq!(m.resolve("untouched.pth"), "untouched.pth");
    }

    #[test]
    fn manifest_is_cached_follows_alias() {
        let mut m = DependencyManifest::default();
        m.files.insert("canonical.pth".into(), "sha256:aaa".into());
        m.aliases.insert("alt.pth".into(), "canonical.pth".into());
        assert!(m.is_cached("canonical.pth"));
        assert!(m.is_cached("alt.pth"));
        assert!(!m.is_cached("missing.pth"));
    }

    #[test]
    fn next_available_name_increments() {
        let mut m = DependencyManifest::default();
        m.files.insert("model.safetensors".into(), "sha256:aaa".into());
        assert_eq!(m.next_available_name("model.safetensors"), "model(2).safetensors");
        m.files.insert("model(2).safetensors".into(), "sha256:bbb".into());
        assert_eq!(m.next_available_name("model.safetensors"), "model(3).safetensors");
    }

    #[test]
    fn split_stem_ext_cases() {
        assert_eq!(split_stem_ext("model.safetensors"), ("model", "safetensors"));
        assert_eq!(split_stem_ext("model.tar.gz"), ("model.tar", "gz"));
        assert_eq!(split_stem_ext("noext"), ("noext", ""));
    }

    #[test]
    fn safe_url_strips_query() {
        assert_eq!(safe_url("https://example.com/file?token=secret"), "https://example.com/file");
        assert_eq!(safe_url("https://example.com/file"), "https://example.com/file");
    }

    #[tokio::test]
    async fn manifest_save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");
        let mut m = DependencyManifest::default();
        m.files.insert("a.safetensors".into(), "sha256:001".into());
        m.files.insert("b.pth".into(), "sha256:002".into());
        m.aliases.insert("a-old.safetensors".into(), "a.safetensors".into());
        m.save(&path).await.unwrap();

        let loaded = DependencyManifest::load(&path).await;
        assert_eq!(loaded.files, m.files);
        assert_eq!(loaded.aliases, m.aliases);
    }

    #[tokio::test]
    async fn manifest_load_missing_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = DependencyManifest::load(&dir.path().join("nope.json")).await;
        assert!(loaded.files.is_empty());
        assert!(loaded.aliases.is_empty());
    }

    #[tokio::test]
    async fn ingest_case_a_already_cached() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("cache");
        fs::create_dir_all(&cache).await.unwrap();
        let mut m = DependencyManifest::default();
        m.files.insert("model.pth".into(), "sha256:abc".into());

        let ws = dir.path().join("ws.pth");
        fs::write(&ws, b"data").await.unwrap();

        let r = ingest_to_cache(&mut m, &cache, &ws, "model.pth", "sha256:abc").await.unwrap();
        assert!(matches!(r, IngestResult::AlreadyCached));
        assert!(!ws.exists(), "workspace copy should be dropped");
    }

    #[tokio::test]
    async fn ingest_case_b_alias() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("cache");
        fs::create_dir_all(&cache).await.unwrap();
        let mut m = DependencyManifest::default();
        m.files.insert("original.pth".into(), "sha256:abc".into());

        let ws = dir.path().join("alt.pth");
        fs::write(&ws, b"data").await.unwrap();

        let r = ingest_to_cache(&mut m, &cache, &ws, "alt.pth", "sha256:abc").await.unwrap();
        match r {
            IngestResult::Aliased { canonical_name, alias_from } => {
                assert_eq!(canonical_name, "original.pth");
                assert_eq!(alias_from, "alt.pth");
            }
            other => panic!("expected Aliased, got {other:?}"),
        }
        assert_eq!(m.aliases["alt.pth"], "original.pth");
    }

    #[tokio::test]
    async fn ingest_case_c_added() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("cache");
        fs::create_dir_all(&cache).await.unwrap();
        let mut m = DependencyManifest::default();

        let ws = dir.path().join("new.pth");
        fs::write(&ws, b"data").await.unwrap();

        let r = ingest_to_cache(&mut m, &cache, &ws, "new.pth", "sha256:zzz").await.unwrap();
        match r {
            IngestResult::Added { canonical_name } => assert_eq!(canonical_name, "new.pth"),
            other => panic!("expected Added, got {other:?}"),
        }
        assert!(cache.join("new.pth").exists());
        assert_eq!(m.files["new.pth"], "sha256:zzz");
    }

    #[tokio::test]
    async fn ingest_case_d_renamed_on_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("cache");
        fs::create_dir_all(&cache).await.unwrap();
        let mut m = DependencyManifest::default();
        m.files.insert("model.pth".into(), "sha256:aaa".into());

        let ws = dir.path().join("model.pth");
        fs::write(&ws, b"different").await.unwrap();

        let r = ingest_to_cache(&mut m, &cache, &ws, "model.pth", "sha256:bbb").await.unwrap();
        match r {
            IngestResult::Renamed { canonical_name, original_name } => {
                assert_eq!(canonical_name, "model(2).pth");
                assert_eq!(original_name, "model.pth");
            }
            other => panic!("expected Renamed, got {other:?}"),
        }
        assert!(cache.join("model(2).pth").exists());
        assert_eq!(m.files["model(2).pth"], "sha256:bbb");
    }
}
