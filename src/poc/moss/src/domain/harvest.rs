//! Harvest types - backup artifacts for offerings
//!
//! A harvest captures the state of an offering before nourishment:
//! - Container image (committed if stateful)
//! - Volume archives (compressed with checksums)
//! - Metadata for restoration
//!
//! Harvests are created automatically during nourishment ceremonies
//! and can be used for rollback or manual restoration.

use chrono::{DateTime, Utc};
use garden_common::offerings::OfferingFqn;
use rand::Rng;
use serde::{Deserialize, Serialize};

/// Harvest identifier (format: "{offering}-{timestamp}")
pub type HarvestId = String;

/// Archive information for a single volume
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VolumeArchive {
    /// Volume name (derived from container path)
    pub name: String,
    /// Container mount path (e.g., "/var/lib/mongodb")
    pub container_path: String,
    /// Path to archive file
    pub archive_path: String,
    /// Archive size in bytes
    pub size_bytes: u64,
    /// Checksum (format: "blake3:{hex}")
    pub checksum: String,
}

/// Harvest manifest - saved alongside volume archives
///
/// Stored as `{harvest_dir}/{id}/manifest.json`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarvestManifest {
    /// Unique identifier
    pub id: HarvestId,
    /// Offering name
    pub offering: String,
    /// When harvest was created
    pub created_at: DateTime<Utc>,
    /// Stone that created the harvest
    pub source_stone: String,

    /// Original container image
    pub original_image: String,
    /// Committed image (if container was committed)
    pub committed_image: Option<String>,

    /// Volume archives
    pub volumes: Vec<VolumeArchive>,

    /// Associated ceremony (if part of nourishment)
    pub ceremony_id: Option<String>,
    /// When harvest expires (for auto-cleanup)
    pub expires_at: Option<DateTime<Utc>>,
}

impl HarvestManifest {
    /// Create a new harvest manifest
    pub fn new(offering: &str, source_stone: &str, original_image: &str) -> Self {
        // Use timestamp + random suffix to ensure unique IDs
        let now = Utc::now();
        let random_suffix: u16 = rand::rng().random();
        let safe_offering = OfferingFqn::parse(offering)
            .map(|fqn| fqn.encoded_for_container())
            .unwrap_or_else(|_| offering.to_string());
        let id = format!(
            "{}-{}-{:04x}",
            safe_offering,
            now.format("%Y%m%dT%H%M%S"),
            random_suffix
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

    /// Calculate total size of all volume archives
    pub fn total_size_bytes(&self) -> u64 {
        self.volumes.iter().map(|v| v.size_bytes).sum()
    }

    /// Check if harvest has any volumes
    pub fn has_volumes(&self) -> bool {
        !self.volumes.is_empty()
    }

    /// Check if container image was committed
    pub fn has_committed_image(&self) -> bool {
        self.committed_image.is_some()
    }

    /// Set expiration based on retention duration
    pub fn set_retention(&mut self, retention: chrono::Duration) {
        self.expires_at = Some(self.created_at + retention);
    }

    /// Check if harvest has expired
    pub fn is_expired(&self) -> bool {
        self.expires_at.map(|exp| Utc::now() > exp).unwrap_or(false)
    }

    /// Format size for display
    pub fn format_size(&self) -> String {
        let bytes = self.total_size_bytes();
        if bytes >= 1_073_741_824 {
            format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
        } else if bytes >= 1_048_576 {
            format!("{:.1} MB", bytes as f64 / 1_048_576.0)
        } else if bytes >= 1024 {
            format!("{:.1} KB", bytes as f64 / 1024.0)
        } else {
            format!("{} B", bytes)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_harvest_id_format() {
        let manifest = HarvestManifest::new("mongodb", "stone-01", "mongo:7.0.4");
        assert!(manifest.id.starts_with("mongodb-"));
        assert!(manifest.id.contains("T")); // ISO timestamp contains T
    }

    #[test]
    fn test_total_size() {
        let mut manifest = HarvestManifest::new("mongodb", "stone-01", "mongo:7.0.4");
        manifest.volumes.push(VolumeArchive {
            name: "data".to_string(),
            container_path: "/data/db".to_string(),
            archive_path: "/harvests/test/data.tar.zst".to_string(),
            size_bytes: 1000,
            checksum: "blake3:abc".to_string(),
        });
        manifest.volumes.push(VolumeArchive {
            name: "config".to_string(),
            container_path: "/data/configdb".to_string(),
            archive_path: "/harvests/test/config.tar.zst".to_string(),
            size_bytes: 500,
            checksum: "blake3:def".to_string(),
        });

        assert_eq!(manifest.total_size_bytes(), 1500);
        assert!(manifest.has_volumes());
    }

    #[test]
    fn test_format_size() {
        let mut manifest = HarvestManifest::new("test", "stone-01", "test:1");

        manifest.volumes.push(VolumeArchive {
            name: "data".to_string(),
            container_path: "/data".to_string(),
            archive_path: "/test".to_string(),
            size_bytes: 500,
            checksum: "".to_string(),
        });
        assert_eq!(manifest.format_size(), "500 B");

        manifest.volumes[0].size_bytes = 2048;
        assert_eq!(manifest.format_size(), "2.0 KB");

        manifest.volumes[0].size_bytes = 5_242_880;
        assert_eq!(manifest.format_size(), "5.0 MB");

        manifest.volumes[0].size_bytes = 2_147_483_648;
        assert_eq!(manifest.format_size(), "2.0 GB");
    }

    #[test]
    fn test_expiration() {
        let mut manifest = HarvestManifest::new("test", "stone-01", "test:1");
        assert!(!manifest.is_expired());

        // Set already-expired retention
        manifest.expires_at = Some(Utc::now() - chrono::Duration::hours(1));
        assert!(manifest.is_expired());
    }

    #[test]
    fn test_serialization() {
        let mut manifest = HarvestManifest::new("mongodb", "stone-01", "mongo:7.0.4");
        manifest.committed_image = Some("zen-harvest/mongodb:20240101T120000".to_string());
        manifest.ceremony_id = Some("nourish-stone-01-20240101120000".to_string());

        let json = serde_json::to_string(&manifest).unwrap();
        assert!(json.contains("mongodb"));
        assert!(json.contains("zen-harvest"));

        let parsed: HarvestManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.offering, "mongodb");
        assert!(parsed.has_committed_image());
    }
}
