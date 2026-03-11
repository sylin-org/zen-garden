//! Tune manifest management
//! Loads tune.yaml files from embedded assets and filesystem
//!
//! Priority: Filesystem tunes override embedded tunes with same name

use anyhow::{Context, Result};
use rust_embed::Embed;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// Embedded official tunes (compiled into binary)
#[derive(Embed)]
#[folder = "tunes/"]
#[prefix = ""]
struct EmbeddedTunes;

/// Tune manifest loaded from tune.yaml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuneManifest {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default = "default_license")]
    pub license: String,
    /// Fallback resource when mapped resource is not found
    #[serde(default)]
    pub fallback: Option<String>,
    /// Event type → audio mapping
    pub events: HashMap<String, EventMapping>,
}

fn default_license() -> String {
    "MIT".to_string()
}

/// Mapping from event to audio action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMapping {
    /// Resource path (relative to tune directory)
    pub resource: String,
    /// Channel to play on: foreground, midground, ambient, background
    #[serde(default = "default_channel")]
    pub channel: String,
    /// Debounce in milliseconds (0 = no debounce)
    #[serde(default)]
    pub debounce_ms: u64,
    /// Whether to loop the audio
    #[serde(default)]
    pub looping: bool,
    /// Volume multiplier (0.0 - 1.0)
    #[serde(default = "default_volume")]
    pub volume: f32,
}

fn default_channel() -> String {
    "foreground".to_string()
}

fn default_volume() -> f32 {
    1.0
}

/// Source of a tune (for resource resolution)
#[derive(Debug, Clone, PartialEq)]
pub enum TuneSource {
    /// Embedded in binary
    Embedded,
    /// Loaded from filesystem
    Filesystem(PathBuf),
}

/// Tune with its source information
#[derive(Debug, Clone)]
pub struct LoadedTune {
    pub manifest: TuneManifest,
    pub source: TuneSource,
}

/// Summary info for listing tunes
#[derive(Debug, Clone)]
pub struct TuneSummary {
    pub name: String,
    pub version: String,
    pub description: String,
    pub event_count: usize,
    pub embedded: bool,
}

/// Manages tune manifests (embedded + filesystem)
pub struct Tunes {
    /// Optional filesystem tunes directory
    #[allow(dead_code)]
    fs_dir: Option<PathBuf>,
    /// All loaded tunes (embedded + filesystem merged)
    tunes: HashMap<String, LoadedTune>,
    /// Currently active tune
    active: RwLock<Option<String>>,
}

impl Tunes {
    /// Create new Tunes
    ///
    /// Loads embedded tunes first, then overlays filesystem tunes.
    /// Filesystem tunes with same name override embedded ones.
    pub fn new(fs_dir: Option<&str>) -> Result<Self> {
        let mut tunes = HashMap::new();

        // 1. Load embedded tunes
        let embedded = Self::load_embedded_tunes()?;
        tracing::debug!(count = embedded.len(), "Loaded embedded tunes");
        tunes.extend(embedded);

        // 2. Overlay filesystem tunes (if directory provided)
        let fs_dir = fs_dir.map(PathBuf::from);
        if let Some(ref dir) = fs_dir {
            if dir.exists() {
                let fs_tunes = Self::scan_filesystem_tunes(dir)?;
                tracing::debug!(count = fs_tunes.len(), dir = %dir.display(), "Loaded filesystem tunes");
                // Filesystem overrides embedded
                tunes.extend(fs_tunes);
            }
        }

        tracing::info!(
            total = tunes.len(),
            embedded = tunes
                .values()
                .filter(|t| t.source == TuneSource::Embedded)
                .count(),
            filesystem = tunes
                .values()
                .filter(|t| matches!(t.source, TuneSource::Filesystem(_)))
                .count(),
            "Tunes initialized"
        );

        Ok(Self {
            fs_dir,
            tunes,
            active: RwLock::new(None),
        })
    }

    /// Load all embedded tunes
    fn load_embedded_tunes() -> Result<HashMap<String, LoadedTune>> {
        let mut tunes = HashMap::new();

        // Find all tune.yaml files in embedded assets
        for file_path in EmbeddedTunes::iter() {
            let path_str = file_path.as_ref();

            // Look for tune.yaml or tune.yml
            if !path_str.ends_with("/tune.yaml") && !path_str.ends_with("/tune.yml") {
                continue;
            }

            if let Some(content) = EmbeddedTunes::get(path_str) {
                match serde_yaml::from_slice::<TuneManifest>(&content.data) {
                    Ok(manifest) => {
                        let name = manifest.name.clone();
                        tunes.insert(
                            name,
                            LoadedTune {
                                manifest,
                                source: TuneSource::Embedded,
                            },
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            path = path_str,
                            error = %e,
                            "Failed to parse embedded tune manifest"
                        );
                    }
                }
            }
        }

        Ok(tunes)
    }

    /// Scan filesystem directory for tunes
    fn scan_filesystem_tunes(tunes_dir: &Path) -> Result<HashMap<String, LoadedTune>> {
        let mut tunes = HashMap::new();

        let entries = std::fs::read_dir(tunes_dir).context("Failed to read tunes directory")?;

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            // Check for tune.yaml or tune.yml
            let manifest_path = if path.join("tune.yaml").exists() {
                path.join("tune.yaml")
            } else if path.join("tune.yml").exists() {
                path.join("tune.yml")
            } else {
                continue;
            };

            match Self::load_manifest_file(&manifest_path) {
                Ok(manifest) => {
                    let name = manifest.name.clone();
                    tunes.insert(
                        name,
                        LoadedTune {
                            manifest,
                            source: TuneSource::Filesystem(path),
                        },
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        path = %manifest_path.display(),
                        error = %e,
                        "Failed to load tune manifest"
                    );
                }
            }
        }

        Ok(tunes)
    }

    /// Load a manifest from filesystem
    fn load_manifest_file(path: &Path) -> Result<TuneManifest> {
        let content = std::fs::read_to_string(path).context("Failed to read manifest file")?;

        let manifest: TuneManifest =
            serde_yaml::from_str(&content).context("Failed to parse manifest YAML")?;

        Ok(manifest)
    }

    /// List all available tunes
    pub fn list_tunes(&self) -> Vec<TuneSummary> {
        self.tunes
            .values()
            .map(|t| TuneSummary {
                name: t.manifest.name.clone(),
                version: t.manifest.version.clone(),
                description: t.manifest.description.clone(),
                event_count: t.manifest.events.len(),
                embedded: t.source == TuneSource::Embedded,
            })
            .collect()
    }

    /// Get a tune by name
    pub fn get_tune(&self, name: &str) -> Option<TuneManifest> {
        self.tunes.get(name).map(|t| t.manifest.clone())
    }

    /// Get loaded tune with source info
    #[allow(dead_code)]
    pub fn get_loaded_tune(&self, name: &str) -> Option<&LoadedTune> {
        self.tunes.get(name)
    }

    /// Select active tune
    pub fn select(&self, name: &str) -> Result<()> {
        if !self.tunes.contains_key(name) {
            anyhow::bail!(
                "Tune '{}' not found. Available: {:?}",
                name,
                self.tunes.keys().collect::<Vec<_>>()
            );
        }

        *self.active.write().unwrap() = Some(name.to_string());
        tracing::info!(tune = name, "Selected tune");
        Ok(())
    }

    /// Get active tune
    pub fn active(&self) -> Option<TuneManifest> {
        let guard = self.active.read().unwrap();
        guard.as_ref().and_then(|name| self.get_tune(name))
    }

    /// Get active tune name
    pub fn active_name(&self) -> Option<String> {
        self.active.read().unwrap().clone()
    }

    /// Get event mapping from active tune
    pub fn get_event_mapping(&self, event_type: &str) -> Option<EventMapping> {
        self.active()
            .and_then(|tune| tune.events.get(event_type).cloned())
    }

    /// Resolve resource to bytes (works for both embedded and filesystem)
    /// Resources starting with "_shared/" are resolved from the tunes root
    pub fn resolve_resource_bytes(&self, tune_name: &str, resource: &str) -> Option<Vec<u8>> {
        let tune = self.tunes.get(tune_name)?;

        // Check if this is a shared resource (starts with _shared/)
        let is_shared = resource.starts_with("_shared/");

        match &tune.source {
            TuneSource::Embedded => {
                // For shared: resource path is just the resource (e.g., "_shared/loading-chime.mp3")
                // For tune-specific: prefix with tune name (e.g., "zen-tech/samples/beep.mp3")
                let embedded_path = if is_shared {
                    resource.to_string()
                } else {
                    format!("{}/{}", tune_name, resource)
                };
                EmbeddedTunes::get(&embedded_path).map(|f| f.data.to_vec())
            }
            TuneSource::Filesystem(base_path) => {
                let full_path = if is_shared {
                    // Go up one level from tune dir to tunes root
                    base_path.parent()?.join(resource)
                } else {
                    base_path.join(resource)
                };
                std::fs::read(&full_path).ok()
            }
        }
    }

    /// Resolve resource to bytes with fallback support
    /// First tries the specified resource, then falls back to tune's fallback if defined
    pub fn resolve_resource_bytes_with_fallback(
        &self,
        tune_name: &str,
        resource: &str,
    ) -> Option<Vec<u8>> {
        // Try primary resource first
        if let Some(data) = self.resolve_resource_bytes(tune_name, resource) {
            return Some(data);
        }

        // Try fallback if defined
        let tune = self.tunes.get(tune_name)?;
        if let Some(ref fallback) = tune.manifest.fallback {
            tracing::debug!(
                resource = resource,
                fallback = fallback.as_str(),
                "Resource not found, using fallback"
            );
            return self.resolve_resource_bytes(tune_name, fallback);
        }

        None
    }

    /// Get a Cursor for audio playback (works for both sources)
    #[allow(dead_code)]
    pub fn get_audio_cursor(&self, tune_name: &str, resource: &str) -> Option<Cursor<Vec<u8>>> {
        self.resolve_resource_bytes(tune_name, resource)
            .map(Cursor::new)
    }

    /// Get filesystem tunes directory (if set)
    #[allow(dead_code)]
    pub fn fs_dir(&self) -> Option<&Path> {
        self.fs_dir.as_deref()
    }

    /// Reload filesystem tunes (keeps embedded, refreshes fs)
    #[allow(dead_code)]
    pub fn reload(&mut self) -> Result<()> {
        // Reload embedded
        let mut tunes = Self::load_embedded_tunes()?;

        // Overlay filesystem
        if let Some(ref dir) = self.fs_dir {
            if dir.exists() {
                let fs_tunes = Self::scan_filesystem_tunes(dir)?;
                tunes.extend(fs_tunes);
            }
        }

        self.tunes = tunes;
        tracing::info!(count = self.tunes.len(), "Reloaded tunes");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_mapping_defaults() {
        let yaml = r#"
            resource: "ding.wav"
        "#;

        let mapping: EventMapping = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(mapping.channel, "foreground");
        assert_eq!(mapping.debounce_ms, 0);
        assert!(!mapping.looping);
        assert_eq!(mapping.volume, 1.0);
    }

    #[test]
    fn test_tune_manifest_parse() {
        let yaml = r#"
            name: test-tune
            version: "1.0.0"
            description: Test tune
            author: Test
            events:
              stone-online:
                resource: online.wav
                channel: foreground
              stone-offline:
                resource: offline.wav
                channel: midground
                debounce_ms: 5000
        "#;

        let manifest: TuneManifest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(manifest.name, "test-tune");
        assert_eq!(manifest.events.len(), 2);
        assert_eq!(manifest.events["stone-online"].channel, "foreground");
        assert_eq!(manifest.events["stone-offline"].debounce_ms, 5000);
    }

    #[test]
    fn test_embedded_tunes_load() {
        // This will work once tunes are embedded
        let manager = Tunes::new(None).unwrap();
        // Should have at least zen-garden embedded
        let tunes = manager.list_tunes();
        assert!(!tunes.is_empty(), "Should have embedded tunes");
    }
}
