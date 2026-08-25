//! WiredTiger cache advisor — evaluates cache health and recommendations.
//!
//! Monitors WiredTiger internal cache metrics from `serverStatus().wiredTiger.cache`
//! and provides sizing recommendations based on stone RAM and workload.

use serde::Serialize;

/// WiredTiger cache status snapshot.
#[derive(Debug, Clone, Serialize)]
pub struct CacheStatus {
    /// Configured cache size in MB.
    pub configured_mb: f64,
    /// Currently used cache in MB.
    pub used_mb: f64,
    /// Cache read hit ratio (0.0 - 1.0).
    pub hit_ratio: f64,
    /// Dirty data ratio in cache (0.0 - 1.0).
    pub dirty_ratio: f64,
    /// Application-level eviction count (indicates cache pressure).
    pub app_evictions: u64,
}

/// Cache health severity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheHealth {
    /// Cache is well-sized.
    Healthy,
    /// Cache is under mild pressure.
    Warning,
    /// Cache is under significant pressure.
    Critical,
}

/// A cache sizing recommendation.
#[derive(Debug, Clone, Serialize)]
pub struct CacheRecommendation {
    pub severity: CacheHealth,
    pub message: String,
    pub recommended_mb: Option<u64>,
}

/// Evaluate WiredTiger cache health and produce recommendations.
pub fn evaluate_cache(
    status: &CacheStatus,
    stone_ram_mb: u64,
    other_offerings: u32,
) -> Vec<CacheRecommendation> {
    let mut recommendations = Vec::new();

    // Check hit ratio
    if status.hit_ratio < 0.90 && status.configured_mb > 0.0 {
        let recommended = recommended_cache_mb(stone_ram_mb, other_offerings);
        recommendations.push(CacheRecommendation {
            severity: CacheHealth::Warning,
            message: format!(
                "Cache hit ratio is {:.1}% (below 90%). Consider increasing cache to {} MB.",
                status.hit_ratio * 100.0,
                recommended
            ),
            recommended_mb: Some(recommended),
        });
    }

    // Check dirty ratio (above 20% = eviction pressure)
    if status.dirty_ratio > 0.20 {
        recommendations.push(CacheRecommendation {
            severity: if status.dirty_ratio > 0.50 {
                CacheHealth::Critical
            } else {
                CacheHealth::Warning
            },
            message: format!(
                "Cache dirty ratio is {:.1}% — eviction pressure is {}.",
                status.dirty_ratio * 100.0,
                if status.dirty_ratio > 0.50 {
                    "severe"
                } else {
                    "elevated"
                }
            ),
            recommended_mb: None,
        });
    }

    // Check app evictions (any > 0 is concerning in a healthy system)
    if status.app_evictions > 100 {
        recommendations.push(CacheRecommendation {
            severity: CacheHealth::Critical,
            message: format!(
                "Application-level evictions detected ({} since boot). Cache is undersized.",
                status.app_evictions
            ),
            recommended_mb: Some(recommended_cache_mb(stone_ram_mb, other_offerings)),
        });
    }

    // Check utilization (>95% = at ceiling)
    if status.configured_mb > 0.0 && (status.used_mb / status.configured_mb) > 0.95 {
        let recommended = recommended_cache_mb(stone_ram_mb, other_offerings);
        if recommended as f64 > status.configured_mb {
            recommendations.push(CacheRecommendation {
                severity: CacheHealth::Warning,
                message: format!(
                    "Cache is {:.0}% utilized ({:.0}/{:.0} MB). Room to grow to {} MB.",
                    (status.used_mb / status.configured_mb) * 100.0,
                    status.used_mb,
                    status.configured_mb,
                    recommended
                ),
                recommended_mb: Some(recommended),
            });
        }
    }

    if recommendations.is_empty() {
        recommendations.push(CacheRecommendation {
            severity: CacheHealth::Healthy,
            message: format!(
                "Cache is healthy: {:.0}/{:.0} MB, {:.1}% hit ratio.",
                status.used_mb,
                status.configured_mb,
                status.hit_ratio * 100.0
            ),
            recommended_mb: None,
        });
    }

    recommendations
}

/// Recommended WiredTiger cache size given stone RAM and co-tenancy.
///
/// MongoDB's default is `(RAM - 1GB) / 2` for a dedicated server.
/// When co-located with other offerings, we reduce proportionally.
pub fn recommended_cache_mb(stone_ram_mb: u64, other_offerings: u32) -> u64 {
    if stone_ram_mb == 0 {
        return 256; // Absolute minimum
    }

    // Base: (RAM - 1024) / 2
    let base = stone_ram_mb.saturating_sub(1024) / 2;

    // Reduce by 15% per co-located offering (capped at 60% reduction)
    let reduction_pct = (other_offerings as f64 * 15.0).min(60.0);
    let adjusted = (base as f64 * (1.0 - reduction_pct / 100.0)) as u64;

    // Floor at 256 MB
    adjusted.max(256)
}

/// Parse WiredTiger cache metrics from a `serverStatus` document.
pub fn parse_cache_status(server_status: &mongodb::bson::Document) -> Option<CacheStatus> {
    let wt = server_status.get_document("wiredTiger").ok()?;
    let cache = wt.get_document("cache").ok()?;

    let configured_bytes = cache
        .get_i64("maximum bytes configured")
        .or_else(|_| cache.get_i32("maximum bytes configured").map(|v| v as i64))
        .unwrap_or(0) as f64;

    let used_bytes = cache
        .get_i64("bytes currently in the cache")
        .or_else(|_| {
            cache
                .get_i32("bytes currently in the cache")
                .map(|v| v as i64)
        })
        .unwrap_or(0) as f64;

    let dirty_bytes = cache
        .get_i64("tracked dirty bytes in the cache")
        .or_else(|_| {
            cache
                .get_i32("tracked dirty bytes in the cache")
                .map(|v| v as i64)
        })
        .unwrap_or(0) as f64;

    let pages_read = cache
        .get_i64("pages read into cache")
        .or_else(|_| cache.get_i32("pages read into cache").map(|v| v as i64))
        .unwrap_or(0) as f64;

    let pages_requested = cache
        .get_i64("pages requested from the cache")
        .or_else(|_| {
            cache
                .get_i32("pages requested from the cache")
                .map(|v| v as i64)
        })
        .unwrap_or(1) as f64; // Avoid div-by-zero

    let app_evictions = cache
        .get_i64("application threads page evictions")
        .or_else(|_| {
            cache
                .get_i32("application threads page evictions")
                .map(|v| v as i64)
        })
        .unwrap_or(0) as u64;

    let configured_mb = configured_bytes / 1_048_576.0;
    let used_mb = used_bytes / 1_048_576.0;
    let dirty_ratio = if used_bytes > 0.0 {
        dirty_bytes / used_bytes
    } else {
        0.0
    };
    let hit_ratio = if pages_requested > 0.0 {
        1.0 - (pages_read / pages_requested)
    } else {
        1.0
    };

    Some(CacheStatus {
        configured_mb,
        used_mb,
        hit_ratio: hit_ratio.clamp(0.0, 1.0),
        dirty_ratio: dirty_ratio.clamp(0.0, 1.0),
        app_evictions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recommended_cache_dedicated() {
        // 16 GB RAM, dedicated server
        let mb = recommended_cache_mb(16384, 0);
        assert_eq!(mb, (16384 - 1024) / 2); // 7680
    }

    #[test]
    fn test_recommended_cache_colocated() {
        // 16 GB RAM, 2 other offerings
        let mb = recommended_cache_mb(16384, 2);
        let base = (16384 - 1024) / 2; // 7680
        let expected = (base as f64 * 0.70) as u64; // 30% reduction
        assert_eq!(mb, expected);
    }

    #[test]
    fn test_recommended_cache_floor() {
        // 1 GB RAM — should floor at 256
        let mb = recommended_cache_mb(1024, 0);
        assert_eq!(mb, 256);
    }

    #[test]
    fn test_healthy_cache() {
        let status = CacheStatus {
            configured_mb: 4096.0,
            used_mb: 2000.0,
            hit_ratio: 0.98,
            dirty_ratio: 0.05,
            app_evictions: 0,
        };
        let recs = evaluate_cache(&status, 16384, 0);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].severity, CacheHealth::Healthy);
    }
}
