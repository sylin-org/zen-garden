//! Resource collection and normalization for stone placement scoring.
//!
//! Reusable functions for fetching resource snapshots from local and
//! remote stones. Provides normalized data structures for consistent
//! scoring and comparison. Renamed from `metrics_collection.rs` in
//! ARCH-0018 Book I Chapter 2 — "metrics" is now reserved for software
//! observability (see `domain::metrics`).

use anyhow::{Context, Result};
use garden_common::{DiskType, StoneResources};
use std::time::Duration;

/// Normalized stone resources for placement evaluation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NormalizedResources {
    pub memory_free_mb: u64,
    pub memory_total_mb: u64,
    pub cpu_load_percent: u8,
    pub storage_free_gb: u64,
    pub storage_total_gb: u64,
    pub storage_type: DiskType,
    pub architecture: String,
}

/// Get resources for tended stone (zero latency, no HTTP)
///
/// This is optimized for local evaluation - no network overhead.
pub fn get_local_resources() -> Result<NormalizedResources> {
    let resources = garden_common::resources::system::collect_stone_resources()
        .context("Failed to collect local stone resources")?;

    let (_, _, architecture) =
        garden_common::resources::system::get_cpu_info().unwrap_or_else(|_| {
            (
                "Unknown".to_string(),
                vec![],
                std::env::consts::ARCH.to_string(),
            )
        });

    // Find primary storage mount
    let primary = resources
        .storage
        .iter()
        .find(|s| s.mount_point == "/" || s.mount_point == "C:\\")
        .or_else(|| resources.storage.iter().max_by_key(|s| s.total_gb));

    let storage_type = primary
        .map(|s| &s.disk_type)
        .cloned()
        .unwrap_or(DiskType::Unknown);

    Ok(normalize_resources(
        &resources,
        &architecture,
        &storage_type,
    ))
}

/// Fetch resources from remote stone via HTTP
///
/// Uses the `/api/v1/stone/resources` endpoint for real-time data.
/// Architecture is fetched from `/capabilities` since it's not in the
/// resources snapshot.
pub async fn fetch_stone_resources(
    endpoint: &str,
    timeout: Duration,
) -> Result<NormalizedResources> {
    let base = endpoint.trim_end_matches('/');
    let resources_url = format!("{}/api/v1/stone/resources", base);

    let response = crate::http::HTTP
        .get(&resources_url)
        .timeout(timeout)
        .send()
        .await
        .context("Failed to fetch /api/v1/stone/resources")?;

    if !response.status().is_success() {
        anyhow::bail!(
            "/api/v1/stone/resources returned error: {}",
            response.status()
        );
    }

    // Response is wrapped in ApiResponse
    #[derive(serde::Deserialize)]
    struct ApiResponse<T> {
        data: T,
    }

    let api_response: ApiResponse<garden_common::ResourcesSnapshot> = response
        .json()
        .await
        .context("Failed to parse /api/v1/stone/resources response")?;

    let snapshot = api_response.data;

    // Architecture from /capabilities (not in resources snapshot)
    let architecture = fetch_architecture(&crate::http::HTTP, base)
        .await
        .unwrap_or_else(|_| std::env::consts::ARCH.to_string());

    // ResourcesSnapshot uses old disk field for backward compat
    let storage_type = DiskType::Unknown; // Type not available in ResourcesSnapshot

    let (storage_free_gb, storage_total_gb) = (
        snapshot.disk.available_bytes / 1024 / 1024 / 1024,
        snapshot.disk.total_bytes / 1024 / 1024 / 1024,
    );

    Ok(NormalizedResources {
        memory_free_mb: snapshot.memory.available_bytes / 1024 / 1024,
        memory_total_mb: snapshot.memory.total_bytes / 1024 / 1024,
        cpu_load_percent: snapshot.cpu.usage_percent as u8,
        storage_free_gb,
        storage_total_gb,
        storage_type,
        architecture,
    })
}

/// Fetch architecture from /capabilities endpoint
async fn fetch_architecture(client: &reqwest::Client, base: &str) -> Result<String> {
    let url = format!("{}/capabilities", base);
    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        anyhow::bail!("capabilities returned error: {}", response.status());
    }

    let caps: garden_common::HardwareCapabilities = response.json().await?;
    Ok(caps.hardware.cpu.architecture)
}

/// Fetch resources from multiple stones in parallel
///
/// Returns results in same order as input endpoints.
/// Failed fetches return Error variants.
pub async fn fetch_resources_batch(
    endpoints: Vec<String>,
    timeout: Duration,
) -> Vec<Result<NormalizedResources>> {
    let futures: Vec<_> = endpoints
        .into_iter()
        .map(|endpoint| {
            let ep = endpoint.clone();
            async move { fetch_stone_resources(&ep, timeout).await }
        })
        .collect();

    futures_util::future::join_all(futures).await
}

/// Normalize StoneResources to NormalizedResources
///
/// Pure function for converting internal resource format to normalized placement form.
pub fn normalize_resources(
    resources: &StoneResources,
    architecture: &str,
    storage_type: &DiskType,
) -> NormalizedResources {
    // Find primary storage mount
    let primary = resources
        .storage
        .iter()
        .find(|s| s.mount_point == "/" || s.mount_point == "C:\\")
        .or_else(|| resources.storage.iter().max_by_key(|s| s.total_gb));

    let (storage_free_gb, storage_total_gb) = primary
        .map(|s| (s.available_gb, s.total_gb))
        .unwrap_or((0, 0));

    NormalizedResources {
        memory_free_mb: resources.memory.available_bytes / 1024 / 1024,
        memory_total_mb: resources.memory.total_bytes / 1024 / 1024,
        cpu_load_percent: resources.cpu.usage_percent as u8,
        storage_free_gb,
        storage_total_gb,
        storage_type: storage_type.clone(),
        architecture: architecture.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use garden_common::{CpuResources, DiskType, MemoryResources, StorageResources};

    fn make_test_resources() -> StoneResources {
        StoneResources {
            cpu: CpuResources {
                cores: 8,
                usage_percent: 25.0,
                usage_friendly: "25%".to_string(),
            },
            memory: MemoryResources {
                total_bytes: 32 * 1024 * 1024 * 1024,     // 32 GB
                used_bytes: 16 * 1024 * 1024 * 1024,      // 16 GB used
                available_bytes: 16 * 1024 * 1024 * 1024, // 16 GB free
                used_percent: 50.0,
                total_friendly: "32 GB".to_string(),
                used_friendly: "16 GB".to_string(),
                available_friendly: "16 GB".to_string(),
            },
            storage: vec![StorageResources {
                identifier: "sda".to_string(),
                mount_point: "/".to_string(),
                total_gb: 500,
                used_gb: 250,
                available_gb: 250,
                used_percent: 50.0,
                disk_type: DiskType::SSD,
                filesystem: "ext4".to_string(),
            }],
            uptime_seconds: 10000,
            uptime_friendly: "2h 46m".to_string(),
            cpu_temperature: Some(42.0),
        }
    }

    #[test]
    fn test_normalize_resources() {
        let resources = make_test_resources();
        let normalized = normalize_resources(&resources, "x86_64", &DiskType::NVMe);

        assert_eq!(normalized.memory_total_mb, 32768); // 32 GB in MB
        assert_eq!(normalized.memory_free_mb, 16384); // 16 GB in MB
        assert_eq!(normalized.cpu_load_percent, 25);
        assert_eq!(normalized.storage_total_gb, 500);
        assert_eq!(normalized.storage_free_gb, 250);
        assert_eq!(normalized.architecture, "x86_64");
        assert!(matches!(normalized.storage_type, DiskType::NVMe));
    }

    #[test]
    fn test_normalize_resources_with_different_storage() {
        let resources = StoneResources {
            cpu: CpuResources {
                cores: 4,
                usage_percent: 50.0,
                usage_friendly: "50%".to_string(),
            },
            memory: MemoryResources {
                total_bytes: 8 * 1024 * 1024 * 1024,
                used_bytes: 6 * 1024 * 1024 * 1024,
                available_bytes: 2 * 1024 * 1024 * 1024,
                used_percent: 75.0,
                total_friendly: "8 GB".to_string(),
                used_friendly: "6 GB".to_string(),
                available_friendly: "2 GB".to_string(),
            },
            storage: vec![StorageResources {
                identifier: "nvme0n1".to_string(),
                mount_point: "/data".to_string(),
                total_gb: 1000,
                used_gb: 900,
                available_gb: 100,
                used_percent: 90.0,
                disk_type: DiskType::NVMe,
                filesystem: "xfs".to_string(),
            }],
            uptime_seconds: 5000,
            uptime_friendly: "1h 23m".to_string(),
            cpu_temperature: Some(65.0),
        };

        let normalized = normalize_resources(&resources, "aarch64", &DiskType::HDD);

        assert_eq!(normalized.memory_total_mb, 8192);
        assert_eq!(normalized.memory_free_mb, 2048);
        assert_eq!(normalized.cpu_load_percent, 50);
        assert_eq!(normalized.storage_total_gb, 1000);
        assert_eq!(normalized.storage_free_gb, 100);
        assert_eq!(normalized.architecture, "aarch64");
        assert!(matches!(normalized.storage_type, DiskType::HDD));
    }

    #[test]
    fn test_local_resources_returns_normalized_data() {
        // This test validates the function executes without panicking
        // Actual values depend on the system
        let result = get_local_resources();

        // Should succeed on any system with sysinfo
        match result {
            Ok(normalized) => {
                assert!(
                    normalized.memory_total_mb > 0,
                    "Should have non-zero total memory"
                );
                assert!(
                    normalized.cpu_load_percent <= 100,
                    "CPU load should be <= 100%"
                );
                assert!(
                    !normalized.architecture.is_empty(),
                    "Architecture should not be empty"
                );
            }
            Err(e) => {
                // Log but don't fail - test environments may have restricted access
                println!(
                    "Local resources collection failed (may be expected in CI): {}",
                    e
                );
            }
        }
    }
}
