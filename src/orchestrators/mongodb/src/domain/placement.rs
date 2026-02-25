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
    /// Number of other offerings running on this stone.
    pub other_offerings: u32,
    /// Disk type: `Some(true)` = SSD/NVMe, `Some(false)` = HDD, `None` = unknown.
    /// Unknown disk type is not penalized (detection is best-effort on Linux,
    /// unavailable on Windows, and fails on LVM/device-mapper volumes).
    pub has_ssd: Option<bool>,
    /// Whether this stone already runs a MongoDB instance for this FQN.
    pub already_has_mongo: bool,
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
    /// Whether this stone already runs MongoDB for the evaluated FQN.
    pub already_has_mongo: bool,
    /// Moss API endpoint for this stone (for install actions).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moss_endpoint: Option<String>,
}

/// Score a stone for MongoDB placement suitability.
///
/// Higher scores are better. Scoring factors:
/// - RAM: +10 per GB above 4GB
/// - SSD: +30
/// - Low co-tenancy: +20 for 0, +10 for 1, 0 for 2+
/// - Already has mongo (same FQN): -100 (avoid same-stone replicas)
/// - High VRAM usage (GPU offerings co-located): -10 (memory pressure)
pub fn score_stone(stone: &StonePlacementProfile) -> (i32, Vec<String>) {
    let mut score: i32 = 0;
    let mut reasons = Vec::new();

    // RAM bonus: +10 per GB above 4GB
    if stone.ram_mb > 4096 {
        let gb_above = (stone.ram_mb - 4096) / 1024;
        let bonus = (gb_above as i32 * 10).min(100);
        score += bonus;
        reasons.push(format!("+{bonus}: {:.1} GB RAM", stone.ram_mb as f64 / 1024.0));
    } else {
        reasons.push(format!("0: low RAM ({:.1} GB)", stone.ram_mb as f64 / 1024.0));
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

    // Already has mongo for same FQN = disqualify
    if stone.already_has_mongo {
        score -= 100;
        reasons.push("-100: already runs MongoDB for this FQN".to_string());
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
                already_has_mongo: stone.already_has_mongo,
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
            ram_mb: 32768,     // 32 GB
            other_offerings: 0, // Dedicated
            has_ssd: Some(true),
            already_has_mongo: false,
            vram_mb: 0,
            moss_endpoint: None,
        };
        let (score, _) = score_stone(&stone);
        // RAM: (32768-4096)/1024 = 28 GB → +100 (capped), SSD: +30, dedicated: +20
        assert!(score >= 100);
    }

    #[test]
    fn test_already_has_mongo_penalized() {
        let stone = StonePlacementProfile {
            stone_name: "stone-b".into(),
            stone_id: "id-b".into(),
            ram_mb: 16384,
            other_offerings: 0,
            has_ssd: Some(true),
            already_has_mongo: true,
            vram_mb: 0,
            moss_endpoint: None,
        };
        let (score, _) = score_stone(&stone);
        assert!(score < 100); // The -100 penalty should dominate
    }

    #[test]
    fn test_placement_ordering() {
        let stones = vec![
            StonePlacementProfile {
                stone_name: "bad".into(),
                stone_id: "1".into(),
                ram_mb: 2048,
                other_offerings: 5,
                has_ssd: Some(false),
                already_has_mongo: true,
                vram_mb: 16384,
                moss_endpoint: None,
            },
            StonePlacementProfile {
                stone_name: "good".into(),
                stone_id: "2".into(),
                ram_mb: 32768,
                other_offerings: 0,
                has_ssd: Some(true),
                already_has_mongo: false,
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
            other_offerings: 1,
            has_ssd: None, // Unknown — should not be penalized
            already_has_mongo: false,
            vram_mb: 0,
            moss_endpoint: None,
        };
        let (score, reasons) = score_stone(&stone);
        // RAM: +40, disk: 0, co-tenancy: +10 = 50
        assert_eq!(score, 50);
        assert!(reasons.iter().any(|r| r.contains("disk type unknown")));
    }
}
