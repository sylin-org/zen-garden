//! Placement advisor — scores stones for MongoDB placement suitability.
//!
//! Evaluates stones based on: available RAM, disk I/O class, existing
//! offerings (co-tenancy), network proximity, and current load.

use serde::Serialize;

/// Profile of a stone for placement scoring.
#[derive(Debug, Clone)]
pub struct StonePlacementProfile {
    pub stone_name: String,
    pub stone_id: String,
    /// Total RAM in MB.
    pub ram_mb: u64,
    /// CPU core count (physical cores). 0 = unknown.
    pub cpu_cores: u32,
    /// Number of other offerings running on this stone.
    pub other_offerings: u32,
    /// Disk type: `Some(true)` = SSD/NVMe, `Some(false)` = HDD, `None` = unknown.
    /// Unknown disk type is not penalized (detection is best-effort on Linux,
    /// unavailable on Windows, and fails on LVM/device-mapper volumes).
    pub has_ssd: Option<bool>,
    /// MongoDB FQNs already installed on this stone (e.g. ["mongodb", "mongodb::analytics"]).
    pub installed_fqns: Vec<String>,
    /// Total VRAM in MB (GPU pressure indicator for co-tenancy).
    pub vram_mb: u64,
    /// Moss API endpoint (e.g. `http://192.168.1.5:7185`), for install actions.
    pub moss_endpoint: Option<String>,
}

/// A placement recommendation for a stone.
#[derive(Debug, Clone, Serialize)]
pub struct PlacementRecommendation {
    pub stone_name: String,
    pub stone_id: String,
    pub score: i32,
    pub reasons: Vec<String>,
    /// MongoDB FQNs already installed on this stone.
    pub installed_fqns: Vec<String>,
    /// Moss API endpoint for this stone (for install actions).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moss_endpoint: Option<String>,
}

/// Score a stone for MongoDB placement suitability.
///
/// Higher scores are better. Scoring factors:
/// - RAM: +15 per GB above 4 GB, cap +200 (WiredTiger cache = ~50% of RAM)
/// - CPU: +10 per core above 2, cap +80 (parallelism for queries/replication)
/// - SSD: +30
/// - Low co-tenancy: +20 for 0, +10 for 1, 0 for 2+
/// - Existing MongoDB instances: -20 each (resource contention)
/// - High VRAM usage (GPU offerings co-located): -10 (memory pressure)
pub fn score_stone(stone: &StonePlacementProfile) -> (i32, Vec<String>) {
    let mut score: i32 = 0;
    let mut reasons = Vec::new();

    // RAM bonus: +15 per GB above 4 GB, capped at +200.
    // MongoDB's WiredTiger engine uses ~50% of available RAM as cache,
    // making RAM the single most impactful resource for performance.
    if stone.ram_mb > 4096 {
        let gb_above = (stone.ram_mb - 4096) / 1024;
        let bonus = (gb_above as i32 * 15).min(200);
        score += bonus;
        reasons.push(format!("+{bonus}: {:.1} GB RAM", stone.ram_mb as f64 / 1024.0));
    } else {
        reasons.push(format!("0: low RAM ({:.1} GB)", stone.ram_mb as f64 / 1024.0));
    }

    // CPU cores bonus: +10 per core above 2, capped at +80.
    // MongoDB benefits from parallelism for concurrent reads, write journaling,
    // and replication threads. Diminishing returns above ~10 cores.
    if stone.cpu_cores > 2 {
        let cores_above = stone.cpu_cores - 2;
        let bonus = (cores_above as i32 * 10).min(80);
        score += bonus;
        reasons.push(format!("+{bonus}: {} CPU cores", stone.cpu_cores));
    } else if stone.cpu_cores > 0 {
        reasons.push(format!("0: {} CPU cores (low)", stone.cpu_cores));
    }

    // SSD bonus — only penalize confirmed HDD; unknown is neutral
    match stone.has_ssd {
        Some(true) => {
            score += 30;
            reasons.push("+30: SSD/NVMe storage".to_string());
        }
        Some(false) => {
            score -= 10;
            reasons.push("-10: HDD storage (SSD preferred)".to_string());
        }
        None => {
            reasons.push("0: disk type unknown".to_string());
        }
    }

    // Co-tenancy
    match stone.other_offerings {
        0 => {
            score += 20;
            reasons.push("+20: dedicated stone".to_string());
        }
        1 => {
            score += 10;
            reasons.push("+10: one co-tenant".to_string());
        }
        n => {
            reasons.push(format!("0: {n} co-tenants"));
        }
    }

    // Existing MongoDB instances — resource contention penalty.
    // Each running instance competes for WiredTiger cache and I/O.
    // Duplicate FQN blocking is handled by the install modal + API validation.
    let mongo_count = stone.installed_fqns.len() as i32;
    if mongo_count > 0 {
        let penalty = mongo_count * 20;
        score -= penalty;
        reasons.push(format!(
            "-{penalty}: {mongo_count} existing MongoDB instance{}",
            if mongo_count == 1 { "" } else { "s" }
        ));
    }

    // GPU memory pressure
    if stone.vram_mb > 8192 {
        score -= 10;
        reasons.push("-10: high VRAM usage (GPU workloads co-located)".to_string());
    }

    (score, reasons)
}

/// Evaluate placement across available stones, sorted by score descending.
pub fn evaluate_placement(
    stones: &[StonePlacementProfile],
) -> Vec<PlacementRecommendation> {
    let mut recommendations: Vec<PlacementRecommendation> = stones
        .iter()
        .map(|stone| {
            let (score, reasons) = score_stone(stone);
            PlacementRecommendation {
                stone_name: stone.stone_name.clone(),
                stone_id: stone.stone_id.clone(),
                score,
                reasons,
                installed_fqns: stone.installed_fqns.clone(),
                moss_endpoint: stone.moss_endpoint.clone(),
            }
        })
        .collect();

    recommendations.sort_by(|a, b| b.score.cmp(&a.score));
    recommendations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ideal_stone() {
        let stone = StonePlacementProfile {
            stone_name: "stone-a".into(),
            stone_id: "id-a".into(),
            ram_mb: 32768,      // 32 GB
            cpu_cores: 8,       // 8 cores
            other_offerings: 0, // Dedicated
            has_ssd: Some(true),
            installed_fqns: vec![],
            vram_mb: 0,
            moss_endpoint: None,
        };
        let (score, _) = score_stone(&stone);
        // RAM: +200 (capped), CPU: +60, SSD: +30, dedicated: +20 = 310
        assert_eq!(score, 310);
    }

    #[test]
    fn test_existing_mongo_penalized() {
        let stone = StonePlacementProfile {
            stone_name: "stone-b".into(),
            stone_id: "id-b".into(),
            ram_mb: 16384,
            cpu_cores: 4,
            other_offerings: 0,
            has_ssd: Some(true),
            installed_fqns: vec!["mongodb".into()],
            vram_mb: 0,
            moss_endpoint: None,
        };
        let (score, reasons) = score_stone(&stone);
        // RAM: +180, CPU: +20, SSD: +30, dedicated: +20, 1 mongo: -20 = 230
        assert_eq!(score, 230);
        assert!(reasons.iter().any(|r| r.contains("1 existing MongoDB instance")));
    }

    #[test]
    fn test_multiple_mongo_instances_penalized() {
        let stone = StonePlacementProfile {
            stone_name: "stone-b".into(),
            stone_id: "id-b".into(),
            ram_mb: 16384,
            cpu_cores: 4,
            other_offerings: 0,
            has_ssd: Some(true),
            installed_fqns: vec!["mongodb".into(), "mongodb::analytics".into()],
            vram_mb: 0,
            moss_endpoint: None,
        };
        let (score, reasons) = score_stone(&stone);
        // RAM: +180, CPU: +20, SSD: +30, dedicated: +20, 2 mongos: -40 = 210
        assert_eq!(score, 210);
        assert!(reasons.iter().any(|r| r.contains("2 existing MongoDB instances")));
    }

    #[test]
    fn test_placement_ordering() {
        let stones = vec![
            StonePlacementProfile {
                stone_name: "bad".into(),
                stone_id: "1".into(),
                ram_mb: 2048,
                cpu_cores: 2,
                other_offerings: 5,
                has_ssd: Some(false),
                installed_fqns: vec!["mongodb".into()],
                vram_mb: 16384,
                moss_endpoint: None,
            },
            StonePlacementProfile {
                stone_name: "good".into(),
                stone_id: "2".into(),
                ram_mb: 32768,
                cpu_cores: 8,
                other_offerings: 0,
                has_ssd: Some(true),
                installed_fqns: vec![],
                vram_mb: 0,
                moss_endpoint: None,
            },
        ];

        let recs = evaluate_placement(&stones);
        assert_eq!(recs[0].stone_name, "good");
        assert_eq!(recs[1].stone_name, "bad");
    }

    #[test]
    fn test_unknown_disk_neutral() {
        let stone = StonePlacementProfile {
            stone_name: "stone-c".into(),
            stone_id: "id-c".into(),
            ram_mb: 8192,
            cpu_cores: 4,
            other_offerings: 1,
            has_ssd: None, // Unknown — should not be penalized
            installed_fqns: vec![],
            vram_mb: 0,
            moss_endpoint: None,
        };
        let (score, reasons) = score_stone(&stone);
        // RAM: (8192-4096)/1024=4 GB → +60, CPU: +20, disk: 0, co-tenancy: +10 = 90
        assert_eq!(score, 90);
        assert!(reasons.iter().any(|r| r.contains("disk type unknown")));
    }

    #[test]
    fn test_ram_differentiation() {
        // Key test: 16 GB and 32 GB should score differently
        let stone_16gb = StonePlacementProfile {
            stone_name: "s16".into(),
            stone_id: "1".into(),
            ram_mb: 16384,
            cpu_cores: 4,
            other_offerings: 0,
            has_ssd: Some(true),
            installed_fqns: vec![],
            vram_mb: 0,
            moss_endpoint: None,
        };
        let stone_32gb = StonePlacementProfile {
            stone_name: "s32".into(),
            stone_id: "2".into(),
            ram_mb: 32768,
            cpu_cores: 4,
            other_offerings: 0,
            has_ssd: Some(true),
            installed_fqns: vec![],
            vram_mb: 0,
            moss_endpoint: None,
        };
        let (score_16, _) = score_stone(&stone_16gb);
        let (score_32, _) = score_stone(&stone_32gb);
        // 16 GB: RAM +180, CPU +20, SSD +30, dedicated +20 = 250
        // 32 GB: RAM +200, CPU +20, SSD +30, dedicated +20 = 270
        assert_eq!(score_16, 250);
        assert_eq!(score_32, 270);
        assert!(score_32 > score_16);
    }

    #[test]
    fn test_cpu_scoring() {
        let stone_2core = StonePlacementProfile {
            stone_name: "s2".into(),
            stone_id: "1".into(),
            ram_mb: 8192,
            cpu_cores: 2,
            other_offerings: 0,
            has_ssd: None,
            installed_fqns: vec![],
            vram_mb: 0,
            moss_endpoint: None,
        };
        let stone_8core = StonePlacementProfile {
            stone_name: "s8".into(),
            stone_id: "2".into(),
            ram_mb: 8192,
            cpu_cores: 8,
            other_offerings: 0,
            has_ssd: None,
            installed_fqns: vec![],
            vram_mb: 0,
            moss_endpoint: None,
        };
        let (score_2, reasons_2) = score_stone(&stone_2core);
        let (score_8, reasons_8) = score_stone(&stone_8core);
        // 2 cores: RAM +60, CPU +0, disk 0, dedicated +20 = 80
        // 8 cores: RAM +60, CPU +60, disk 0, dedicated +20 = 140
        assert_eq!(score_2, 80);
        assert_eq!(score_8, 140);
        assert!(reasons_2.iter().any(|r| r.contains("CPU cores (low)")));
        assert!(reasons_8.iter().any(|r| r.contains("CPU cores")));
    }
}
