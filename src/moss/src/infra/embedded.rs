//! Embedded Assets
//!
//! Provides access to embedded manifests and Companions compiled into the Moss binary.
//! Uses overlay pattern: filesystem files take precedence over embedded files.
//!
//! # Directory Structure (embedded/)
//!
//! ```text
//! embedded/
//! ├── manifests/           # Same structure as manifests/ folder
//! │   └── sw/
//! │       ├── data/
//! │       │   ├── mongodb.snippet.yaml
//! │       │   └── ...
//! │       └── ...
//! └── Companions/            # Platform-specific Companion binaries
//!     ├── windows/
//!     │   └── garden-cricket.exe
//!     └── linux/
//!         └── garden-cricket
//! ```
//!
//! # Loading Priority
//!
//! 1. **Filesystem first**: Check `{data_dir}/manifests/` and `{data_dir}/companions/`
//! 2. **Embedded fallback**: If file not found on filesystem, use embedded version
//!
//! # Extraction
//!
//! Embedded assets allow Moss to ship with manifests and Companions directly compiled in.
//! The overlay pattern allows filesystem files to override embedded defaults.

use anyhow::{Context, Result};
use rust_embed::Embed;
use std::path::Path;
use tracing::debug;

// ============================================================================
// Embedded Manifests
// ============================================================================

/// Embedded manifest files (sw/ directory structure)
#[derive(Embed)]
#[folder = "embedded/manifests/"]
#[prefix = ""]
pub struct EmbeddedManifests;

impl EmbeddedManifests {
    /// Get embedded file content by path (relative to manifests/)
    pub fn get_file(path: &str) -> Option<Vec<u8>> {
        Self::get(path).map(|f| f.data.to_vec())
    }

    /// List all embedded manifest files
    pub fn list_files() -> Vec<String> {
        Self::iter().map(|s| s.to_string()).collect()
    }

    /// Check if an embedded manifest exists
    pub fn exists(path: &str) -> bool {
        Self::get(path).is_some()
    }

    /// Get file content as string
    pub fn get_string(path: &str) -> Option<String> {
        Self::get(path).and_then(|f| String::from_utf8(f.data.to_vec()).ok())
    }
}

// ============================================================================
// Embedded Companions (Platform-Specific)
// ============================================================================

/// Embedded Windows Companions
#[cfg(target_os = "windows")]
#[derive(Embed)]
#[folder = "embedded/companions/windows/"]
#[prefix = ""]
pub struct EmbeddedCompanions;

/// Embedded Linux Companions
#[cfg(target_os = "linux")]
#[derive(Embed)]
#[folder = "embedded/companions/linux/"]
#[prefix = ""]
pub struct EmbeddedCompanions;

/// Fallback for other platforms (empty)
#[cfg(not(any(target_os = "windows", target_os = "linux")))]
#[derive(Embed)]
#[folder = "embedded/companions/linux/"]
#[prefix = ""]
pub struct EmbeddedCompanions;

impl EmbeddedCompanions {
    /// Get embedded Companion binary by name
    pub fn get_companion(name: &str) -> Option<Vec<u8>> {
        Self::get(name).map(|f| f.data.to_vec())
    }

    /// List all embedded Companions
    pub fn list_companions() -> Vec<String> {
        Self::iter().map(|s| s.to_string()).collect()
    }

    /// Check if an embedded Companion exists
    pub fn exists(name: &str) -> bool {
        Self::get(name).is_some()
    }
}

// ============================================================================
// Embedded Seeds (First-Boot Configuration)
// ============================================================================

/// Embedded seed files for offering first-boot configuration.
///
/// Decoupled from manifests: manifests are declarative metadata describing
/// *what* to deploy; seeds are runtime content deployed *into* volumes.
///
/// # Directory Convention
///
/// ```text
/// embedded/seeds/
/// └── {offering}/
///     └── {volume-name}/
///         └── {file-path...}
/// ```
#[derive(Embed)]
#[folder = "embedded/seeds/"]
#[prefix = ""]
pub struct EmbeddedSeeds;

// ============================================================================
// Overlay Loading Helpers
// ============================================================================

/// Read a manifest file with overlay pattern
///
/// Priority: filesystem > embedded
pub fn read_manifest_overlay(manifests_dir: &Path, relative_path: &str) -> Option<String> {
    let fs_path = manifests_dir.join(relative_path);

    // 1. Check filesystem first
    if fs_path.exists()
        && let Ok(content) = std::fs::read_to_string(&fs_path) {
            debug!(path = %relative_path, source = "filesystem", "Loaded manifest");
            return Some(content);
        }

    // 2. Fall back to embedded
    if let Some(content) = EmbeddedManifests::get_string(relative_path) {
        debug!(path = %relative_path, source = "embedded", "Loaded manifest");
        return Some(content);
    }

    None
}

/// Check if a manifest exists (filesystem or embedded)
pub fn manifest_exists(manifests_dir: &Path, relative_path: &str) -> bool {
    let fs_path = manifests_dir.join(relative_path);
    fs_path.exists() || EmbeddedManifests::exists(relative_path)
}

/// List all available manifest files (merged: filesystem + embedded)
///
/// Returns unique paths with filesystem taking precedence.
pub fn list_all_manifests(manifests_dir: &Path) -> Vec<ManifestSource> {
    use std::collections::HashMap;

    let mut manifests: HashMap<String, ManifestSource> = HashMap::new();

    // 1. Add embedded manifests first
    for path in EmbeddedManifests::iter() {
        let path_str = path.to_string();
        manifests.insert(
            path_str.clone(),
            ManifestSource {
                path: path_str,
                source: AssetSource::Embedded,
            },
        );
    }

    // 2. Overlay with filesystem manifests (they take precedence)
    if manifests_dir.exists() {
        for entry in walkdir::WalkDir::new(manifests_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file()
                && let Ok(relative) = entry.path().strip_prefix(manifests_dir) {
                    let relative_str = relative.to_string_lossy().replace('\\', "/");
                    manifests.insert(
                        relative_str.clone(),
                        ManifestSource {
                            path: relative_str,
                            source: AssetSource::Filesystem,
                        },
                    );
                }
        }
    }

    let mut result: Vec<_> = manifests.into_values().collect();
    result.sort_by(|a, b| a.path.cmp(&b.path));
    result
}

/// Source of a manifest/Companion
#[derive(Debug, Clone, PartialEq)]
pub enum AssetSource {
    /// Loaded from filesystem (takes precedence)
    Filesystem,
    /// Loaded from embedded assets
    Embedded,
}

/// Manifest with source information
#[derive(Debug, Clone)]
pub struct ManifestSource {
    /// Relative path within manifests directory
    pub path: String,
    /// Where the manifest was loaded from
    pub source: AssetSource,
}

// ============================================================================
// Manifest Loading with Overlay
// ============================================================================

use garden_common::manifests::{Offering, OfferingRegistry};

/// Load software manifests with embedded + filesystem overlay
///
/// This is the core manifest loading function for Moss.
///
/// **Loading order:**
/// 1. Load all manifests from embedded assets (compiled into binary)
/// 2. Scan filesystem for manifests - overlay on top:
///    - If matching an embedded entry → overwrite
///    - If new → add new entry
///
/// Use `-vvv` to see detailed loading progress.
pub fn load_sw_manifests_with_overlay(fs_dir: &Path) -> Result<OfferingRegistry> {
    let mut manifests = OfferingRegistry::empty();
    let mut embedded_count = 0;
    let mut fs_new_count = 0;
    let mut fs_override_count = 0;

    // Phase 1: Load all from embedded assets
    tracing::info!("Loading manifests from embedded assets...");

    // Debug: list all embedded files with guidance extension
    let guidance_files: Vec<_> = EmbeddedManifests::iter()
        .filter(|p| p.ends_with(".guidance.md"))
        .collect();
    tracing::info!(
        count = guidance_files.len(),
        files = ?guidance_files,
        "Embedded guidance files found"
    );

    for path in EmbeddedManifests::iter() {
        let path_str = path.as_ref();

        // Only process sw/*.snippet.yaml files
        if !path_str.starts_with("sw/") || !path_str.ends_with(".snippet.yaml") {
            continue;
        }

        // Get the snippet content
        let Some(snippet_content) = EmbeddedManifests::get_string(path_str) else {
            tracing::warn!(path = %path_str, "Failed to read embedded manifest");
            continue;
        };

        // Try to load compatibility file
        let compat_path = path_str.replace(".snippet.yaml", ".compatibility.yaml");
        let compat_content = EmbeddedManifests::get_string(&compat_path);

        // Try to load frontmatter file
        let frontmatter_path = path_str.replace(".snippet.yaml", ".frontmatter.json");
        let frontmatter_content = EmbeddedManifests::get_string(&frontmatter_path);

        // Try to load guidance file
        let guidance_path = path_str.replace(".snippet.yaml", ".guidance.md");
        let guidance_content = EmbeddedManifests::get_string(&guidance_path);

        if guidance_content.is_none() {
            tracing::debug!(
                snippet_path = %path_str,
                guidance_path = %guidance_path,
                "No guidance file found for manifest"
            );
        }

        // Parse and add the entry
        match OfferingRegistry::load_from_content(
            path_str,
            &snippet_content,
            compat_content.as_deref(),
            frontmatter_content.as_deref(),
            guidance_content.as_deref(),
        ) {
            Ok(entry) => {
                let has_guidance = entry.guidance.is_some();
                tracing::debug!(
                    offering = %entry.name,
                    category = %entry.category,
                    has_guidance = has_guidance,
                    guidance_path = %guidance_path,
                    source = "embedded",
                    "Loaded manifest"
                );
                if !has_guidance && guidance_content.is_some() {
                    tracing::warn!(
                        offering = %entry.name,
                        guidance_path = %guidance_path,
                        "Guidance content found but not loaded into entry"
                    );
                }
                manifests.upsert(entry);
                embedded_count += 1;
            }
            Err(e) => {
                tracing::warn!(
                    path = %path_str,
                    error = %e,
                    "Failed to parse embedded manifest"
                );
            }
        }
    }

    tracing::info!(
        count = embedded_count,
        "Loaded manifests from embedded assets"
    );

    // Phase 2: Overlay filesystem manifests (upsert each entry)
    let sw_dir = fs_dir.join("sw");
    if sw_dir.exists() {
        tracing::info!(path = %sw_dir.display(), "Scanning filesystem for manifest overlays...");

        // Walk filesystem and upsert each manifest found
        for entry in walkdir::WalkDir::new(&sw_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            // Only process .snippet.yaml files
            if !path.is_file() {
                continue;
            }
            let Some(filename) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !filename.ends_with(".snippet.yaml") {
                continue;
            }

            // Extract category from parent directory
            let Some(parent) = path.parent() else {
                continue;
            };
            let Some(category) = parent.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let offering_name = filename.trim_end_matches(".snippet.yaml");

            // Read snippet content
            let Ok(snippet_content) = std::fs::read_to_string(path) else {
                tracing::warn!(path = %path.display(), "Failed to read filesystem manifest");
                continue;
            };

            // Try to load compatibility file
            let compat_path = path.with_file_name(format!("{}.compatibility.yaml", offering_name));
            let compat_content = std::fs::read_to_string(&compat_path).ok();

            // Try to load frontmatter file
            let frontmatter_path =
                path.with_file_name(format!("{}.frontmatter.json", offering_name));
            let frontmatter_content = std::fs::read_to_string(&frontmatter_path).ok();

            // Try to load guidance file
            let guidance_path = path.with_file_name(format!("{}.guidance.md", offering_name));
            let guidance_content = std::fs::read_to_string(&guidance_path).ok();

            // Build relative path for load_from_content
            let relative_path = format!("sw/{}/{}.snippet.yaml", category, offering_name);

            match OfferingRegistry::load_from_content(
                &relative_path,
                &snippet_content,
                compat_content.as_deref(),
                frontmatter_content.as_deref(),
                guidance_content.as_deref(),
            ) {
                Ok(mut sw_entry) => {
                    // Preserve embedded fields when filesystem doesn't provide them
                    // This allows filesystem to override specific files while keeping
                    // embedded defaults for files not present on disk
                    if let Some(existing) = manifests.get(offering_name) {
                        if sw_entry.guidance.is_none() && existing.guidance.is_some() {
                            sw_entry.guidance = existing.guidance.clone();
                            tracing::debug!(
                                offering = %offering_name,
                                "Preserved embedded guidance during filesystem overlay"
                            );
                        }
                        if sw_entry.compatibility.is_none() && existing.compatibility.is_some() {
                            sw_entry.compatibility = existing.compatibility.clone();
                        }
                        if sw_entry.connection.is_none() && existing.connection.is_some() {
                            sw_entry.connection = existing.connection.clone();
                            tracing::debug!(
                                offering = %offering_name,
                                "Preserved embedded connection profile during filesystem overlay"
                            );
                        }
                        // Preserve metadata fields if filesystem didn't provide them
                        if sw_entry.metadata.description.is_none()
                            && existing.metadata.description.is_some()
                        {
                            sw_entry.metadata.description = existing.metadata.description.clone();
                        }
                        if sw_entry.metadata.tags.is_empty() && !existing.metadata.tags.is_empty() {
                            sw_entry.metadata.tags = existing.metadata.tags.clone();
                        }
                    }

                    let was_override = manifests.upsert(sw_entry);
                    if was_override {
                        tracing::debug!(
                            offering = %offering_name,
                            category = %category,
                            source = "filesystem",
                            "Overlaid embedded manifest (preserved missing fields)"
                        );
                        fs_override_count += 1;
                    } else {
                        tracing::debug!(
                            offering = %offering_name,
                            category = %category,
                            source = "filesystem",
                            "Added new manifest"
                        );
                        fs_new_count += 1;
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        path = %path.display(),
                        error = %e,
                        "Failed to parse filesystem manifest"
                    );
                }
            }
        }

        if fs_new_count > 0 || fs_override_count > 0 {
            tracing::info!(
                new = fs_new_count,
                overwritten = fs_override_count,
                "Applied filesystem manifest overlays"
            );
        }
    } else {
        tracing::debug!(
            path = %sw_dir.display(),
            "No filesystem manifests directory, using embedded only"
        );
    }

    // Summary
    tracing::info!(
        total = manifests.len(),
        embedded = embedded_count,
        fs_new = fs_new_count,
        fs_overwritten = fs_override_count,
        categories = manifests.categories.len(),
        "ManifestRegistry populated"
    );

    Ok(manifests)
}

// ============================================================================
// Embedded Offering Manifests (Adopted/Borrowed)
// ============================================================================

/// Load embedded offerings for adoption/borrowing
///
/// Scans embedded assets for `.adopted.yaml` files and parses them
/// as unified Offering structs with AdoptedConfig.
///
/// # Returns
/// Vec of Offering definitions
pub fn load_embedded_adopted_offerings() -> Vec<Offering> {
    use garden_common::manifests::{
        AdoptedConfig, ConnectionProfile, ConnectivityConfig, ControlConfig, HealthConfig,
        ManageableEnv, OfferingMetadata, OsDetectionRules,
    };
    use garden_common::types::AdoptedControlLevel;
    use serde::Deserialize;

    /// Subset of frontmatter fields relevant for adopted offerings.
    #[derive(Debug, Deserialize)]
    struct FrontmatterData {
        description: Option<String>,
        #[serde(default)]
        tags: Vec<String>,
        icon: Option<String>,
        homepage: Option<String>,
        documentation: Option<String>,
        port: Option<u16>,
        connection: Option<ConnectionProfile>,
        manageable_env: Option<ManageableEnv>,
    }

    /// File format for .adopted.yaml files
    #[derive(Debug, Deserialize)]
    struct AdoptedFile {
        name: Option<String>,
        category: Option<String>,
        description: Option<String>,
        #[serde(default)]
        tags: Option<Vec<String>>,
        detection: OsDetectionRules,
        control: Option<ControlConfig>,
        default_control_level: Option<AdoptedControlLevel>,
        health_check: Option<HealthConfig>,
        guidance: Option<String>,
        connectivity: Option<ConnectivityConfig>,
        connection: Option<garden_common::manifests::ConnectionProfile>,
        #[serde(default)]
        coordination: garden_common::CoordinationMode,
    }

    let mut offerings = Vec::new();

    for path in EmbeddedManifests::iter() {
        let path_str = path.as_ref();

        // Only process .adopted.yaml files
        if !path_str.ends_with(".adopted.yaml") {
            continue;
        }

        let Some(content) = EmbeddedManifests::get_string(path_str) else {
            tracing::warn!(path = %path_str, "Failed to read embedded adopted manifest");
            continue;
        };

        // Strip UTF-8 BOM if present (some editors add it)
        let content = garden_common::utils::strings::strip_bom(&content);

        // Extract category and name from path (sw/category/name.adopted.yaml)
        let parts: Vec<&str> = path_str.split('/').collect();
        let (category, name) = if parts.len() >= 3 {
            let cat = parts[1].to_string();
            let filename = parts.last().unwrap();
            let n = filename.trim_end_matches(".adopted.yaml").to_string();
            (cat, n)
        } else {
            tracing::warn!(path = %path_str, "Invalid path format for adopted manifest");
            continue;
        };

        // Try to load adopted guidance file (if present)
        let guidance_path = path_str.replace(".adopted.yaml", ".adopted.guidance.md");
        let guidance_content = EmbeddedManifests::get_string(&guidance_path)
            .map(|md| garden_common::manifests::offering::strip_markdown_frontmatter(&md));

        // Load frontmatter (shared with managed counterpart, e.g. ollama.frontmatter.json)
        let frontmatter_path = path_str.replace(".adopted.yaml", ".frontmatter.json");
        let frontmatter: Option<FrontmatterData> = EmbeddedManifests::get_string(&frontmatter_path)
            .and_then(|json| {
                let json = garden_common::utils::strings::strip_bom(&json);
                serde_json::from_str::<FrontmatterData>(json).ok()
            });

        match serde_yml::from_str::<AdoptedFile>(content) {
            Ok(file) => {
                let fm = frontmatter.as_ref();
                let offering = Offering {
                    name: file.name.unwrap_or(name.clone()),
                    category: file.category.unwrap_or(category),
                    managed: None,
                    adopted: Some(AdoptedConfig {
                        detection: file.detection,
                        control: file.control,
                        default_control_level: file.default_control_level.unwrap_or_default(),
                        health_check: file.health_check,
                        guidance: file.guidance.or(guidance_content),
                        connectivity: file.connectivity,
                    }),
                    borrowed: None,
                    metadata: OfferingMetadata {
                        description: file
                            .description
                            .or_else(|| fm.and_then(|f| f.description.clone())),
                        tags: file
                            .tags
                            .unwrap_or_else(|| fm.map(|f| f.tags.clone()).unwrap_or_default()),
                        icon: fm.and_then(|f| f.icon.clone()),
                        homepage: fm.and_then(|f| f.homepage.clone()),
                        documentation: fm.and_then(|f| f.documentation.clone()),
                        port: fm.and_then(|f| f.port),
                    },
                    compatibility: None,
                    guidance: None,
                    connection: file
                        .connection
                        .or_else(|| fm.and_then(|f| f.connection.clone())),
                    manageable_env: fm.and_then(|f| f.manageable_env.clone()),
                    coordination: file.coordination,
                };

                tracing::debug!(
                    name = %offering.name,
                    path = %path_str,
                    "Loaded embedded adopted offering"
                );
                offerings.push(offering);
            }
            Err(e) => {
                tracing::warn!(
                    path = %path_str,
                    error = %e,
                    "Failed to parse embedded adopted manifest"
                );
            }
        }
    }

    tracing::info!(count = offerings.len(), "Loaded embedded adopted offerings");

    offerings
}

// ============================================================================
// Seed Extraction (First-Boot Configuration)
// ============================================================================

/// Extract seed files for an offering into its volume directories.
///
/// Seeds provide initial configuration files needed before a container's first boot.
/// Files are only written if they don't already exist (**no-clobber**), preserving
/// any user customizations from a previous install or manual edit.
///
/// # Convention
///
/// Embedded path: `{offering}/{volume-name}/{path...}`
/// Extracts to:   `{host_volume_path}/{path...}`
///
/// The `{volume-name}` segment is matched against the last component of each
/// host volume path from the compiled manifest.
///
/// # Returns
///
/// Number of seed files written (skipped files are not counted).
pub fn extract_seeds(offering: &str, volumes: &[(String, String)]) -> Result<usize> {
    let prefix = format!("{}/", offering);
    let mut extracted = 0;

    for path in EmbeddedSeeds::iter() {
        let path_str = path.as_ref();

        // Match: {offering}/{volume-name}/{rest...}
        let Some(after_prefix) = path_str.strip_prefix(&prefix) else {
            continue;
        };

        // Split into volume-name and file-path-within-volume
        let Some((volume_name, file_path)) = after_prefix.split_once('/') else {
            tracing::debug!(
                offering,
                path = %path_str,
                "Seed path has no file beneath volume directory, skipping"
            );
            continue;
        };

        // Find the matching host path from compiled volumes.
        // Convention: host path ends with /{volume-name}
        let Some((host_path, _)) = volumes.iter().find(|(hp, _)| {
            hp.ends_with(&format!("/{}", volume_name))
                || hp.ends_with(&format!("\\{}", volume_name))
        }) else {
            tracing::debug!(
                offering,
                volume_name,
                "Seed references volume not present in offering, skipping"
            );
            continue;
        };

        let target = std::path::Path::new(host_path).join(file_path);

        // No-clobber: never overwrite existing files
        if target.exists() {
            tracing::debug!(
                offering,
                target = %target.display(),
                "Seed target exists, preserving user file"
            );
            continue;
        }

        // Read embedded content
        let Some(content) = EmbeddedSeeds::get(path_str) else {
            continue;
        };

        // Post-process: replace well-known placeholders with generated values.
        // This lets seed files contain e.g. unique secrets per install.
        let final_content = post_process_seed(&content.data);

        // Ensure parent directories exist
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create seed directory {}", parent.display()))?;
        }

        std::fs::write(&target, &final_content)
            .with_context(|| format!("Failed to write seed file {}", target.display()))?;

        tracing::info!(
            offering,
            target = %target.display(),
            "Extracted seed file"
        );
        extracted += 1;
    }

    if extracted > 0 {
        tracing::info!(offering, count = extracted, "Seed files extracted");
    }

    Ok(extracted)
}

/// Well-known placeholder for a unique random secret (hex, 64 chars).
const SECRET_PLACEHOLDER: &str = "__ZEN_GARDEN_GENERATE_SECRET__";

/// Replace well-known placeholders in seed content.
///
/// Supported placeholders:
/// - `__ZEN_GARDEN_GENERATE_SECRET__` → 64-char random hex string
fn post_process_seed(raw: &[u8]) -> Vec<u8> {
    // Strip UTF-8 BOM if present (editors on Windows sometimes add one).
    let data = if raw.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &raw[3..]
    } else {
        raw
    };

    // Only process text-like files (UTF-8). Binary seeds pass through unchanged.
    let Ok(text) = std::str::from_utf8(data) else {
        return data.to_vec();
    };

    if !text.contains(SECRET_PLACEHOLDER) {
        return data.to_vec();
    }

    let secret = generate_hex_secret(64);
    text.replace(SECRET_PLACEHOLDER, &secret).into_bytes()
}

/// Generate a cryptographically random hex string of `len` characters.
fn generate_hex_secret(len: usize) -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    (0..len)
        .map(|_| format!("{:x}", rng.random::<u8>() % 16))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_embedded_manifests_list() {
        // This will be empty if no manifests are in embedded/manifests/
        let files = EmbeddedManifests::list_files();
        // Just verify it doesn't panic
        println!("Embedded manifests: {:?}", files);
    }

    #[test]
    fn test_embedded_companions_list() {
        // This will be empty if no companions are in embedded/companions/{platform}/
        let companions = EmbeddedCompanions::list_companions();
        // Just verify it doesn't panic
        println!("Embedded companions: {:?}", companions);
    }

    #[test]
    fn test_embedded_adopted_offerings() {
        let offerings = load_embedded_adopted_offerings();
        println!(
            "Embedded adopted offerings: {:?}",
            offerings.iter().map(|o| &o.name).collect::<Vec<_>>()
        );

        for offering in &offerings {
            println!(
                "  {} ({:?}): {:?}",
                offering.name,
                offering.category,
                offering.modes()
            );
            // Verify detection rules are loaded
            if offering.adopted.is_some() {
                let rules = offering.get_detection_rules();
                println!("    Detection rules: {} for current OS", rules.len());
            }
        }

        // Verify ollama is loaded
        assert!(
            offerings.iter().any(|o| o.name == "ollama"),
            "Ollama should be in adopted offerings"
        );
    }

    #[test]
    fn test_extract_seeds_writes_to_volume() {
        let temp = TempDir::new().unwrap();
        let vol_dir = temp.path().join("volumes").join("searxng-data");
        let volumes = vec![(
            vol_dir.to_string_lossy().to_string(),
            "/etc/searxng".to_string(),
        )];
        let count = extract_seeds("searxng", &volumes).unwrap();
        if count > 0 {
            // Verify seed file was written
            assert!(vol_dir.join("settings.yml").exists());
            let content = std::fs::read_to_string(vol_dir.join("settings.yml")).unwrap();
            assert!(content.contains("use_default_settings"));
        }
    }

    #[test]
    fn test_extract_seeds_no_clobber() {
        let temp = TempDir::new().unwrap();
        let vol_dir = temp.path().join("volumes").join("searxng-data");
        std::fs::create_dir_all(&vol_dir).unwrap();
        std::fs::write(vol_dir.join("settings.yml"), "user-custom: true").unwrap();

        let volumes = vec![(
            vol_dir.to_string_lossy().to_string(),
            "/etc/searxng".to_string(),
        )];
        let _ = extract_seeds("searxng", &volumes).unwrap();

        // User's file should be preserved, not overwritten
        let content = std::fs::read_to_string(vol_dir.join("settings.yml")).unwrap();
        assert_eq!(content, "user-custom: true");
    }

    #[test]
    fn test_post_process_seed_strips_bom() {
        let with_bom = b"\xEF\xBB\xBFuse_default_settings: true\n";
        let result = post_process_seed(with_bom);
        assert_eq!(result, b"use_default_settings: true\n");
    }

    #[test]
    fn test_post_process_seed_replaces_secret_with_bom() {
        let input = format!("\u{FEFF}secret_key: \"{SECRET_PLACEHOLDER}\"");
        let result = post_process_seed(input.as_bytes());
        let text = std::str::from_utf8(&result).unwrap();
        assert!(!text.starts_with('\u{FEFF}'), "BOM should be stripped");
        assert!(
            !text.contains(SECRET_PLACEHOLDER),
            "placeholder should be replaced"
        );
        assert!(text.contains("secret_key:"), "key should remain");
    }

    #[test]
    fn test_overlay_preserves_embedded_connection_when_missing_in_fs() {
        let temp = TempDir::new().unwrap();
        let data_dir = temp.path().join("sw").join("data");
        fs::create_dir_all(&data_dir).unwrap();

        // Overlay snippet without frontmatter connection. The merge should keep
        // embedded mongodb connection profile.
        fs::write(
            data_dir.join("mongodb.snippet.yaml"),
            "services:\n  mongodb:\n    image: mongodb:latest\n    ports:\n      default: [27017, 27017]\n",
        )
        .unwrap();
        fs::write(
            data_dir.join("mongodb.frontmatter.json"),
            r#"{"name":"mongodb","description":"FS override","category":"data","tags":["database"],"port":27017}"#,
        )
        .unwrap();

        let registry = load_sw_manifests_with_overlay(temp.path()).unwrap();
        let mongodb = registry
            .get("mongodb")
            .expect("embedded mongodb should exist");
        assert_eq!(
            mongodb
                .connection
                .as_ref()
                .and_then(|c| c.uri_template.as_deref()),
            Some("mongodb://{host}:{port}")
        );
    }
}
