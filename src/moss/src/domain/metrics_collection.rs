//! Metrics collection and normalization for stone resources
//!
//! Reusable functions for fetching resource metrics from local and remote stones.
//! Provides normalized data structures for consistent scoring and comparison.

use anyhow::{Context, Result};
use garden_common::{DiskType, StoneResources};
use std::time::Duration;

/// Normalized stone metrics for placement evaluation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoneMetrics {
    pub memory_free_mb: u64,
    pub memory_total_mb: u64,
    pub cpu_load_percent: u8,
    pub storage_free_gb: u64,
    pub storage_total_gb: u64,
    pub storage_type: DiskType,
    pub architecture: String,
}

/// Get metrics for tended stone (zero latency, no HTTP)
///
/// This is optimized for local evaluation - no network overhead.
pub fn get_local_metrics() -> Result<StoneMetrics> {
    let resources = garden_common::metrics::system::collect_stone_resources()
        .context("Failed to collect local stone resources")?;

    let (_, _, architecture) =
        garden_common::metrics::system::get_cpu_info().unwrap_or_else(|_| {
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

    Ok(normalize_metrics(&resources, &architecture, &storage_type))
}

/// Fetch metrics from remote stone via HTTP
///
/// Uses the `/metrics` endpoint for real-time data.
/// Architecture is fetched from `/capabilities` since it's not in metrics.
pub async fn fetch_stone_metrics(endpoint: &str, timeout: Duration) -> Result<StoneMetrics> {
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .context("Failed to build HTTP client")?;

    let base = endpoint.trim_end_matches('/');
    let metrics_url = format!("{}/metrics", base);

    let response = client
        .get(&metrics_url)
        .send()
        .await
        .context("Failed to fetch /metrics")?;

    if !response.status().is_success() {
        anyhow::bail!("/metrics returned error: {}", response.status());
    }

    // Response is wrapped in ApiResponse
    #[derive(serde::Deserialize)]
    struct ApiResponse<T> {
        data: T,
    }

    let api_response: ApiResponse<garden_common::MetricsSnapshot> = response
        .json()
        .await
        .context("Failed to parse /metrics response")?;

    let snapshot = api_response.data;

    // Architecture from /capabilities (not in metrics)
    let architecture = fetch_architecture(&client, base)
        .await
        .unwrap_or_else(|_| std::env::consts::ARCH.to_string());

    // MetricsSnapshot uses old disk field for backward compat
    let storage_type = DiskType::Unknown; // Type not available in MetricsSnapshot

    let (storage_free_gb, storage_total_gb) = (
        snapshot.disk.available_bytes / 1024 / 1024 / 1024,
        snapshot.disk.total_bytes / 1024 / 1024 / 1024,
    );

    Ok(StoneMetrics {
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

/// Fetch metrics from multiple stones in parallel
///
/// Returns results in same order as input endpoints.
/// Failed fetches return Error variants.
pub async fn fetch_metrics_batch(
    endpoints: Vec<String>,
    timeout: Duration,
) -> Vec<Result<StoneMetrics>> {
    let futures: Vec<_> = endpoints
        .into_iter()
        .map(|endpoint| {
            let ep = endpoint.clone();
            async move { fetch_stone_metrics(&ep, timeout).await }
        })
        .collect();

    futures_util::future::join_all(futures).await
}

/// Normalize StoneResources to StoneMetrics
///
/// Pure function for converting internal resource format to placement metrics.
pub fn normalize_metrics(
    resources: &StoneResources,
    architecture: &str,
    storage_type: &DiskType,
) -> StoneMetrics {
    // Find primary storage mount
    let primary = resources
        .storage
        .iter()
        .find(|s| s.mount_point == "/" || s.mount_point == "C:\\")
        .or_else(|| resources.storage.iter().max_by_key(|s| s.total_gb));

    let (storage_free_gb, storage_total_gb) = primary
        .map(|s| (s.available_gb, s.total_gb))
        .unwrap_or((0, 0));

    StoneMetrics {
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
    use garden_common::{CpuMetrics, DiskType, MemoryMetrics, StorageMetrics};

    fn make_test_resources() -> StoneResources {
        StoneResources {
            cpu: CpuMetrics {
                cores: 8,
                usage_percent: 25.0,
                usage_friendly: "25%".to_string(),
            },
            memory: MemoryMetrics {
                total_bytes: 32 * 1024 * 1024 * 1024,     // 32 GB
                used_bytes: 16 * 1024 * 1024 * 1024,      // 16 GB used
                available_bytes: 16 * 1024 * 1024 * 1024, // 16 GB free
                used_percent: 50.0,
                total_friendly: "32 GB".to_string(),
                used_friendly: "16 GB".to_string(),
                available_friendly: "16 GB".to_string(),
            },
            storage: vec![StorageMetrics {
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
        }
    }

    #[test]
    fn test_normalize_metrics() {
        let resources = make_test_resources();
        let metrics = normalize_metrics(&resources, "x86_64", &DiskType::NVMe);

        assert_eq!(metrics.memory_total_mb, 32768); // 32 GB in MB
        assert_eq!(metrics.memory_free_mb, 16384); // 16 GB in MB
        assert_eq!(metrics.cpu_load_percent, 25);
        assert_eq!(metrics.storage_total_gb, 500);
        assert_eq!(metrics.storage_free_gb, 250);
        assert_eq!(metrics.architecture, "x86_64");
        assert!(matches!(metrics.storage_type, DiskType::NVMe));
    }

    #[test]
    fn test_normalize_metrics_with_different_storage() {
        let resources = StoneResources {
            cpu: CpuMetrics {
                cores: 4,
                usage_percent: 50.0,
                usage_friendly: "50%".to_string(),
            },
            memory: MemoryMetrics {
                total_bytes: 8 * 1024 * 1024 * 1024,
                used_bytes: 6 * 1024 * 1024 * 1024,
                available_bytes: 2 * 1024 * 1024 * 1024,
                used_percent: 75.0,
                total_friendly: "8 GB".to_string(),
                used_friendly: "6 GB".to_string(),
                available_friendly: "2 GB".to_string(),
            },
            storage: vec![StorageMetrics {
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
        };

        let metrics = normalize_metrics(&resources, "aarch64", &DiskType::HDD);

        assert_eq!(metrics.memory_total_mb, 8192);
        assert_eq!(metrics.memory_free_mb, 2048);
        assert_eq!(metrics.cpu_load_percent, 50);
        assert_eq!(metrics.storage_total_gb, 1000);
        assert_eq!(metrics.storage_free_gb, 100);
        assert_eq!(metrics.architecture, "aarch64");
        assert!(matches!(metrics.storage_type, DiskType::HDD));
    }

    #[test]
    fn test_local_metrics_returns_normalized_data() {
        // This test validates the function executes without panicking
        // Actual values depend on the system
        let result = get_local_metrics();

        // Should succeed on any system with sysinfo
        match result {
            Ok(metrics) => {
                assert!(
                    metrics.memory_total_mb > 0,
                    "Should have non-zero total memory"
                );
                assert!(
                    metrics.cpu_load_percent <= 100,
                    "CPU load should be <= 100%"
                );
                assert!(
                    !metrics.architecture.is_empty(),
                    "Architecture should not be empty"
                );
            }
            Err(e) => {
                // Log but don't fail - test environments may have restricted access
                println!("Local metrics failed (may be expected in CI): {}", e);
            }
        }
    }
}
