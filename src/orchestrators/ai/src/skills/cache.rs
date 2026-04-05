//! Dependency cache — content-addressed, deduplicated model storage (ORCH-0022).
//!
//! Layout:
//! ```text
//! {data_dir}/cache/dependencies/{provider}/
//!   manifest.json           — checksum + alias registry
//!   {model-files}           — cached model files
//!
//! {data_dir}/cache/dependencies/workspace/{skill}/
//!   {downloading-files}     — ephemeral, cleaned after use
//! ```
//!
//! All downloads stream to disk — never buffered in memory.
//! SHA-256 is computed during the download stream (single pass).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::AsyncWriteExt;

// ── Manifest ──────────────────────────────────────────────────

/// Dependency cache manifest — tracks files, checksums, and aliases.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DependencyManifest {
    /// filename → "sha256:{hex}"
    pub files: HashMap<String, String>,
    /// requested_name → canonical_name (for dedup when same content, different name)
    pub aliases: HashMap<String, String>,
}

impl DependencyManifest {
    /// Load from disk, or return empty if not found.
    pub async fn load(path: &Path) -> Self {
        match fs::read_to_string(path).await {
            Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Save to disk (atomic: write tmp then rename).
    pub async fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)
            .context("serialize manifest")?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, json.as_bytes()).await.context("write manifest tmp")?;
        fs::rename(&tmp, path).await.context("rename manifest")?;
        Ok(())
    }

    /// Find the canonical filename for a checksum.
    pub fn filename_for_checksum(&self, checksum: &str) -> Option<&str> {
        self.files
            .iter()
            .find(|(_, cs)| cs.eq_ignore_ascii_case(checksum))
            .map(|(name, _)| name.as_str())
    }

    /// Resolve a requested filename through aliases. Returns owned string.
    pub fn resolve(&self, requested: &str) -> String {
        self.aliases
            .get(requested)
            .cloned()
            .unwrap_or_else(|| requested.to_string())
    }

    /// Generate a non-conflicting filename: name(2).ext, name(3).ext, ...
    pub fn next_available_name(&self, filename: &str) -> String {
        let (stem, ext) = split_stem_ext(filename);
        let mut n = 2;
        loop {
            let candidate = format!("{stem}({n}).{ext}");
            if !self.files.contains_key(&candidate) {
                return candidate;
            }
            n += 1;
        }
    }
}

/// Split "model.safetensors" into ("model", "safetensors").
fn split_stem_ext(filename: &str) -> (&str, &str) {
    match filename.rsplit_once('.') {
        Some((stem, ext)) => (stem, ext),
        None => (filename, ""),
    }
}

// ── Cache Directory ───────────────────────────────────────────

/// Paths for a provider's dependency cache.
pub struct CachePaths {
    /// {data_dir}/cache/dependencies/{provider}/
    pub provider_dir: PathBuf,
    /// {data_dir}/cache/dependencies/{provider}/manifest.json
    pub manifest_path: PathBuf,
    /// {data_dir}/cache/dependencies/workspace/
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

    /// Ensure all directories exist.
    pub async fn ensure_dirs(&self) -> Result<()> {
        fs::create_dir_all(&self.provider_dir).await.context("create provider cache dir")?;
        fs::create_dir_all(&self.workspace_dir).await.context("create workspace dir")?;
        Ok(())
    }

    /// Workspace path for a skill's in-flight downloads.
    pub fn workspace_for_skill(&self, skill_moniker: &str) -> PathBuf {
        self.workspace_dir.join(skill_moniker)
    }
}

// ── Streaming Download with SHA-256 ───────────────────────────

/// Progress callback: (downloaded_bytes, total_bytes_if_known).
pub type ProgressFn = Box<dyn Fn(u64, Option<u64>) + Send + Sync>;

/// Download a file by streaming to disk, computing SHA-256 during the stream.
/// Returns the file path and the hex-encoded checksum.
///
/// If a partial file exists (interrupted download), attempts HTTP Range resume.
/// The optional `on_progress` callback fires every 5 seconds with bytes downloaded.
pub async fn stream_download(
    http: &reqwest::Client,
    url: &str,
    dest: &Path,
    total_bytes: Option<u64>,
    on_progress: Option<ProgressFn>,
) -> Result<(PathBuf, String)> {
    use sha2::{Sha256, Digest};
    use futures_util::StreamExt;

    fs::create_dir_all(dest.parent().unwrap_or(Path::new("."))).await?;

    // Check for existing partial file (resume support)
    let existing_size = fs::metadata(dest).await.map(|m| m.len()).unwrap_or(0);
    let mut hasher = Sha256::new();

    let mut file = if existing_size > 0 {
        // Hash the existing partial content first
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

    // Build request with optional Range header
    let mut req = http.get(url);
    if existing_size > 0 {
        req = req.header("Range", format!("bytes={existing_size}-"));
    }

    let resp = req.send().await.with_context(|| format!("GET {url}"))?;

    // If server doesn't support Range (200 instead of 206), start over
    if existing_size > 0 && resp.status().as_u16() == 200 {
        tracing::debug!("server does not support Range — restarting download");
        drop(file);
        hasher = Sha256::new();
        file = fs::File::create(dest).await?;
    } else if resp.status().as_u16() == 401 || resp.status().as_u16() == 403 {
        // Strip query params (may contain tokens) from the URL for logging
        let safe_url = url.split('?').next().unwrap_or(url);
        anyhow::bail!(
            "download requires authentication (HTTP {}). \
             Set the API key in Dashboard → Secrets for this provider. URL: {safe_url}",
            resp.status()
        );
    } else if !resp.status().is_success() {
        anyhow::bail!("download failed HTTP {}: {url}", resp.status());
    }

    let content_length = resp.content_length();
    let total = total_bytes.or(content_length.map(|cl| cl + existing_size));

    let mut stream = resp.bytes_stream();
    let mut downloaded = existing_size;
    let mut last_log = std::time::Instant::now();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.with_context(|| format!("read chunk from: {url}"))?;
        file.write_all(&chunk).await.context("write chunk")?;
        hasher.update(&chunk);
        downloaded += chunk.len() as u64;

        // Log less frequently for large files (every 30s if >1GB, every 10s otherwise)
        let log_interval = if total.unwrap_or(0) > 1_000_000_000 { 30 } else { 10 };
        if last_log.elapsed() > std::time::Duration::from_secs(log_interval) {
            if let Some(t) = total {
                let pct = (downloaded as f64 / t as f64 * 100.0) as u32;
                // Strip query params from URL to avoid logging secrets
                let safe_url = url.split('?').next().unwrap_or(url);
                tracing::info!(
                    url = %safe_url,
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

// ── Dedup + Ingest ────────────────────────────────────────────

/// Result of ingesting a downloaded file into the cache.
pub enum IngestResult {
    /// File was new — moved to cache.
    Added { canonical_name: String },
    /// Same checksum existed under same name — dropped duplicate.
    AlreadyCached,
    /// Same checksum existed under different name — recorded alias.
    Aliased { canonical_name: String, alias_from: String },
    /// Different checksum, name conflict — stored with incremented name.
    Renamed { canonical_name: String, original_name: String },
}

/// Ingest a downloaded file from the workspace into the provider cache.
///
/// Implements the 4-case dedup logic from ORCH-0022:
/// - Case A: checksum match, same name → drop workspace copy
/// - Case B: checksum match, different name → alias
/// - Case C: new checksum, name available → move to cache
/// - Case D: new checksum, name taken → increment name(N), move
pub async fn ingest_to_cache(
    manifest: &mut DependencyManifest,
    cache_dir: &Path,
    workspace_file: &Path,
    requested_name: &str,
    checksum: &str,
) -> Result<IngestResult> {
    // Check if this checksum already exists in the cache
    let existing = manifest.filename_for_checksum(checksum).map(String::from);
    if let Some(existing_name) = existing {
        if existing_name == requested_name {
            // Case A: same checksum, same name — already cached
            let _ = fs::remove_file(workspace_file).await;
            return Ok(IngestResult::AlreadyCached);
        }

        // Case B: same checksum, different name — record alias
        manifest.aliases.insert(requested_name.to_string(), existing_name.clone());
        let _ = fs::remove_file(workspace_file).await;
        return Ok(IngestResult::Aliased {
            canonical_name: existing_name,
            alias_from: requested_name.to_string(),
        });
    }

    // New checksum — check for name conflicts
    if manifest.files.contains_key(requested_name) {
        // Case D: name taken by different content — increment
        let new_name = manifest.next_available_name(requested_name);
        let dest = cache_dir.join(&new_name);
        fs::rename(workspace_file, &dest).await
            .with_context(|| format!("move {} to cache as {}", workspace_file.display(), new_name))?;
        manifest.files.insert(new_name.clone(), checksum.to_string());
        return Ok(IngestResult::Renamed {
            canonical_name: new_name,
            original_name: requested_name.to_string(),
        });
    }

    // Case C: new checksum, name available — move to cache
    let dest = cache_dir.join(requested_name);
    fs::rename(workspace_file, &dest).await
        .with_context(|| format!("move {} to cache", workspace_file.display()))?;
    manifest.files.insert(requested_name.to_string(), checksum.to_string());

    Ok(IngestResult::Added {
        canonical_name: requested_name.to_string(),
    })
}

/// Compute SHA-256 of an existing file on disk (fallback for interrupted downloads).
pub async fn checksum_file(path: &Path) -> Result<String> {
    use sha2::{Sha256, Digest};
    use tokio::io::AsyncReadExt;

    let mut file = fs::File::open(path).await
        .with_context(|| format!("open for checksum: {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024]; // 64KB chunks

    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 { break; }
        hasher.update(&buf[..n]);
    }

    Ok(format!("sha256:{:x}", hasher.finalize()))
}

// ── Garbage Collection ────────────────────────────────────────

/// Remove cached models that are not referenced by any skill.
///
/// Scans all skill.json files to collect referenced model filenames,
/// then removes cache entries (and files) with zero references.
pub async fn garbage_collect(
    skills_dir: &Path,
    cache_paths: &CachePaths,
) -> Result<usize> {
    let mut manifest = DependencyManifest::load(&cache_paths.manifest_path).await;
    if manifest.files.is_empty() {
        return Ok(0);
    }

    // Collect all model filenames referenced by any skill
    let mut referenced: std::collections::HashSet<String> = std::collections::HashSet::new();

    let providers = match super::loader::read_subdirs(skills_dir).await {
        Ok(dirs) => dirs,
        Err(_) => return Ok(0),
    };

    for provider_dir in providers {
        let monikers = match super::loader::read_subdirs(&provider_dir).await {
            Ok(dirs) => dirs,
            Err(_) => continue,
        };

        for moniker_dir in monikers {
            let skill_path = moniker_dir.join("skill.json");
            if let Ok(json_str) = fs::read_to_string(&skill_path).await {
                if let Ok(raw) = serde_json::from_str::<serde_json::Value>(&json_str) {
                    if let Some(models) = raw.get("required_models").and_then(|v| v.as_array()) {
                        for model in models {
                            if let Some(filename) = model.get("filename").and_then(|v| v.as_str()) {
                                // Add the filename and its resolved form
                                referenced.insert(filename.to_string());
                                referenced.insert(manifest.resolve(filename));
                            }
                        }
                    }
                }
            }
        }
    }

    // Remove unreferenced files
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
            tracing::debug!(file = %name, error = %e, "failed to remove cached model (may be already gone)");
        }
        manifest.files.remove(name);
        removed += 1;
        tracing::info!(file = %name, "GC: removed unreferenced model from cache");
    }

    // Clean stale aliases (pointing to removed files)
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

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_filename_for_checksum() {
        let mut m = DependencyManifest::default();
        m.files.insert("model.pth".into(), "sha256:abc123".into());

        assert_eq!(m.filename_for_checksum("sha256:abc123"), Some("model.pth"));
        assert_eq!(m.filename_for_checksum("sha256:xyz"), None);
    }

    #[test]
    fn manifest_resolve_alias() {
        let mut m = DependencyManifest::default();
        m.aliases.insert("alt-name.pth".into(), "real-name.pth".into());

        assert_eq!(m.resolve("alt-name.pth"), "real-name.pth");
        assert_eq!(m.resolve("no-alias.pth"), "no-alias.pth");
    }

    #[test]
    fn manifest_next_available_name() {
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

    #[tokio::test]
    async fn ingest_case_a_already_cached() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join("cache");
        fs::create_dir_all(&cache_dir).await.unwrap();

        let mut manifest = DependencyManifest::default();
        manifest.files.insert("model.pth".into(), "sha256:abc".into());

        // Create a workspace file
        let ws_file = dir.path().join("workspace_model.pth");
        fs::write(&ws_file, b"data").await.unwrap();

        let result = ingest_to_cache(&mut manifest, &cache_dir, &ws_file, "model.pth", "sha256:abc").await.unwrap();
        assert!(matches!(result, IngestResult::AlreadyCached));
        assert!(!ws_file.exists()); // workspace file cleaned up
    }

    #[tokio::test]
    async fn ingest_case_b_alias() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join("cache");
        fs::create_dir_all(&cache_dir).await.unwrap();

        let mut manifest = DependencyManifest::default();
        manifest.files.insert("original.pth".into(), "sha256:abc".into());

        let ws_file = dir.path().join("alt-name.pth");
        fs::write(&ws_file, b"data").await.unwrap();

        let result = ingest_to_cache(&mut manifest, &cache_dir, &ws_file, "alt-name.pth", "sha256:abc").await.unwrap();
        match result {
            IngestResult::Aliased { canonical_name, alias_from } => {
                assert_eq!(canonical_name, "original.pth");
                assert_eq!(alias_from, "alt-name.pth");
            }
            _ => panic!("expected Aliased"),
        }
        assert_eq!(manifest.aliases["alt-name.pth"], "original.pth");
    }

    #[tokio::test]
    async fn ingest_case_c_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join("cache");
        fs::create_dir_all(&cache_dir).await.unwrap();

        let mut manifest = DependencyManifest::default();

        let ws_file = dir.path().join("new-model.pth");
        fs::write(&ws_file, b"data").await.unwrap();

        let result = ingest_to_cache(&mut manifest, &cache_dir, &ws_file, "new-model.pth", "sha256:xyz").await.unwrap();
        match result {
            IngestResult::Added { canonical_name } => {
                assert_eq!(canonical_name, "new-model.pth");
            }
            _ => panic!("expected Added"),
        }
        assert!(cache_dir.join("new-model.pth").exists());
        assert_eq!(manifest.files["new-model.pth"], "sha256:xyz");
    }

    #[tokio::test]
    async fn ingest_case_d_name_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join("cache");
        fs::create_dir_all(&cache_dir).await.unwrap();

        let mut manifest = DependencyManifest::default();
        manifest.files.insert("model.pth".into(), "sha256:aaa".into());

        let ws_file = dir.path().join("model.pth");
        fs::write(&ws_file, b"different data").await.unwrap();

        let result = ingest_to_cache(&mut manifest, &cache_dir, &ws_file, "model.pth", "sha256:bbb").await.unwrap();
        match result {
            IngestResult::Renamed { canonical_name, original_name } => {
                assert_eq!(canonical_name, "model(2).pth");
                assert_eq!(original_name, "model.pth");
            }
            _ => panic!("expected Renamed"),
        }
        assert!(cache_dir.join("model(2).pth").exists());
        assert_eq!(manifest.files["model(2).pth"], "sha256:bbb");
    }
}
