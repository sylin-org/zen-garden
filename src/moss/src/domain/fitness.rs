//! Fitness scoring for offering orchestration (ORCH-0001)
//!
//! Computes a single opaque `i16` fitness score for election candidacy.
//! The score is Moss-private — the election protocol only sees "candidates
//! have scores, highest wins." How a stone computes that number is its
//! own business and can evolve without protocol changes.
//!
//! Score range: `[-1000, 1000]`. Pinned = `1001` (always wins).
//! Ineligible stones return `None` (don't respond to election at all).
//!
//! Fitness leverages the **existing per-stone compatibility evaluation**
//! (see `domain::compatibility`). Each stone already evaluates offering
//! manifest rules against its own hardware capabilities at index time —
//! we reuse that `CompiledCompatibility` result rather than reinventing
//! constraint checking.

use garden_common::constants::orchestration::{
    FITNESS_SCORE_MAX, FITNESS_SCORE_MIN, FITNESS_SCORE_PINNED,
};
use garden_common::Offering;

use super::compatibility::CompiledCompatibility;
use super::scoring;

// ============================================================================
// Fitness Scoring
// ============================================================================

/// Compute this stone's fitness for hosting an offering.
///
/// Returns `i16` in `[-1000, 1000]`. Higher is better.
/// Returns `None` if ineligible (caller should not respond to election).
///
/// If the offering is pinned, returns `1001` unconditionally.
///
/// Eligibility is determined by the **per-stone compatibility evaluation**
/// that already runs when the offerings index is built. A "fail" decision
/// makes the stone ineligible. All other decisions (pass/warning/fallback)
/// are eligible with appropriate score penalties applied via
/// `scoring::calculate_compatibility_penalty()`.
///
/// # Arguments
/// * `offering` - The offering instance being evaluated
/// * `compatibility` - Pre-computed per-stone compatibility evaluation
/// * `metrics` - Optional normalised stone metrics for resource scoring
/// * `offering_count` - Number of offerings currently on this stone
pub fn compute_fitness_score(
    offering: &Offering,
    compatibility: &CompiledCompatibility,
    metrics: Option<&crate::domain::metrics_collection::StoneMetrics>,
    offering_count: usize,
) -> Option<i16> {
    // Pinned → always wins
    if let Some(ref orch) = offering.orchestration {
        if orch.pinned {
            return Some(FITNESS_SCORE_PINNED);
        }
    }

    // Use the existing per-stone compatibility evaluation for eligibility.
    // A "fail" decision means the stone cannot host this offering at all.
    let compat_decision = compatibility_decision_from_compiled(compatibility);
    if matches!(
        compat_decision,
        super::compatibility::CompatibilityDecision::Fail { .. }
    ) {
        return None;
    }

    // Start with compatibility penalty (Pass=0, Warning=-50, Fallback=-15)
    let compat_penalty = scoring::calculate_compatibility_penalty(&compat_decision);

    // Compute resource-based score from normalised metrics
    let (resource_score, capacity_score) = if let Some(m) = metrics {
        let mem = scoring::score_memory_headroom(m.memory_free_mb, m.memory_total_mb);
        let cpu = scoring::score_cpu_availability(m.cpu_load_percent);
        let storage_cap = scoring::score_storage_capacity(m.storage_free_gb);
        let storage_hw = scoring::score_storage_type(&m.storage_type);
        (mem + cpu, storage_cap + storage_hw)
    } else {
        // No metrics yet — conservative mid-range estimate
        (20, 10)
    };

    // Distribution penalty: more offerings → lower score
    let distribution = scoring::calculate_distribution_penalty(offering_count);

    // Health bonus
    let health_bonus: i32 = if offering.health == garden_common::ServiceHealthStatus::Healthy {
        15
    } else {
        0
    };

    // Compose final score.
    // Raw components sum to roughly [-999, 70] in worst-to-best case.
    // Scale to [-1000, 1000] range:
    //   resource_score: 0..40   cpu (0..20) + memory (0..20)
    //   capacity_score: 0..27   storage_cap (0..15) + storage_hw (0..12)
    //   distribution:   -N..0   -3 per offering
    //   health_bonus:   0..15
    //   compat_penalty: -999..0
    //
    // Positive components max = 40 + 27 + 15 = 82. Scale ×12 → ~984 ≈ 1000.
    let positive = (resource_score + capacity_score + health_bonus + distribution) as f64;
    let scaled = (positive * 12.0) + compat_penalty as f64;
    let clamped = (scaled as i16).clamp(FITNESS_SCORE_MIN, FITNESS_SCORE_MAX);
    Some(clamped)
}

// ============================================================================
// Helpers
// ============================================================================

/// Convert a serialised `CompiledCompatibility` back to the domain enum
/// so we can feed it into `scoring::calculate_compatibility_penalty()`.
fn compatibility_decision_from_compiled(
    compiled: &CompiledCompatibility,
) -> super::compatibility::CompatibilityDecision {
    match compiled.decision.as_str() {
        "pass" => super::compatibility::CompatibilityDecision::Pass,
        "warning" => super::compatibility::CompatibilityDecision::Warning {
            reason: compiled.reason.clone().unwrap_or_default(),
            suggestion: compiled.suggestion.clone(),
        },
        "fallback" => super::compatibility::CompatibilityDecision::Fallback {
            image: compiled.fallback_image.clone().unwrap_or_default(),
            reason: compiled.reason.clone().unwrap_or_default(),
        },
        _ => super::compatibility::CompatibilityDecision::Fail {
            reason: compiled
                .reason
                .clone()
                .unwrap_or_else(|| "Incompatible".into()),
            suggestion: compiled.suggestion.clone(),
        },
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::compatibility::CompiledCompatibility;
    use crate::domain::metrics_collection::StoneMetrics;
    use garden_common::types::*;
    use garden_common::DiskType;

    fn compat_pass() -> CompiledCompatibility {
        CompiledCompatibility {
            decision: "pass".to_string(),
            reason: None,
            original_image: None,
            fallback_image: None,
            suggestion: None,
        }
    }

    fn compat_warning(reason: &str) -> CompiledCompatibility {
        CompiledCompatibility {
            decision: "warning".to_string(),
            reason: Some(reason.to_string()),
            original_image: None,
            fallback_image: None,
            suggestion: None,
        }
    }

    fn compat_fallback(reason: &str) -> CompiledCompatibility {
        CompiledCompatibility {
            decision: "fallback".to_string(),
            reason: Some(reason.to_string()),
            original_image: Some("original:latest".to_string()),
            fallback_image: Some("fallback:latest".to_string()),
            suggestion: None,
        }
    }

    fn compat_fail(reason: &str) -> CompiledCompatibility {
        CompiledCompatibility {
            decision: "fail".to_string(),
            reason: Some(reason.to_string()),
            original_image: None,
            fallback_image: None,
            suggestion: Some("Install GPU drivers".to_string()),
        }
    }

    fn make_metrics(cpu_load: u8, mem_free_mb: u64, mem_total_mb: u64) -> StoneMetrics {
        StoneMetrics {
            memory_free_mb: mem_free_mb,
            memory_total_mb: mem_total_mb,
            cpu_load_percent: cpu_load,
            storage_free_gb: 200,
            storage_total_gb: 500,
            storage_type: DiskType::SSD,
            architecture: "x86_64".to_string(),
        }
    }

    fn make_offering(pinned: bool) -> Offering {
        Offering {
            offering_id: "test-id".to_string(),
            name: garden_common::offerings::OfferingFqn::new("test-offering").unwrap(),
            offering: "test".to_string(),
            version: "1.0".to_string(),
            status: OfferingStatus::Running,
            health: ServiceHealthStatus::Healthy,
            sub_capabilities: vec![],
            location: OfferingLocation {
                host: "localhost".to_string(),
                port: 8080,
                protocol: "http".to_string(),
                agnostic_port: None,
                port_map: std::collections::HashMap::new(),
            },
            mode_data: OfferingModeData::Managed(ManagedData::default()),
            registered_at: chrono::Utc::now(),
            updated_at: None,
            orchestration: if pinned {
                Some(OrchestrationState {
                    role: OfferingRole::Primary,
                    primary_stone_id: None,
                    pinned: true,
                    pin_timestamp: Some("2026-02-16T00:00:00Z".to_string()),
                })
            } else {
                Some(OrchestrationState::default())
            },
        }
    }

    // ========================================================================
    // compatibility_decision_from_compiled tests
    // ========================================================================

    #[test]
    fn test_compiled_pass_round_trips() {
        let compiled = compat_pass();
        let decision = compatibility_decision_from_compiled(&compiled);
        assert!(matches!(
            decision,
            super::super::compatibility::CompatibilityDecision::Pass
        ));
    }

    #[test]
    fn test_compiled_fail_round_trips() {
        let compiled = compat_fail("no GPU");
        let decision = compatibility_decision_from_compiled(&compiled);
        assert!(matches!(
            decision,
            super::super::compatibility::CompatibilityDecision::Fail { .. }
        ));
    }

    #[test]
    fn test_compiled_warning_round_trips() {
        let compiled = compat_warning("low VRAM");
        let decision = compatibility_decision_from_compiled(&compiled);
        assert!(matches!(
            decision,
            super::super::compatibility::CompatibilityDecision::Warning { .. }
        ));
    }

    #[test]
    fn test_compiled_fallback_round_trips() {
        let compiled = compat_fallback("no AVX");
        let decision = compatibility_decision_from_compiled(&compiled);
        assert!(matches!(
            decision,
            super::super::compatibility::CompatibilityDecision::Fallback { .. }
        ));
    }

    // ========================================================================
    // compute_fitness_score tests
    // ========================================================================

    #[test]
    fn test_pinned_returns_1001() {
        let offering = make_offering(true);
        let result = compute_fitness_score(&offering, &compat_pass(), None, 0);
        assert_eq!(result, Some(FITNESS_SCORE_PINNED));
    }

    #[test]
    fn test_pinned_ignores_compat_fail() {
        // Pinned wins even if compatibility says "fail"
        let offering = make_offering(true);
        let result = compute_fitness_score(&offering, &compat_fail("no GPU"), None, 0);
        assert_eq!(result, Some(FITNESS_SCORE_PINNED));
    }

    #[test]
    fn test_fail_compat_returns_none() {
        let offering = make_offering(false);
        let result = compute_fitness_score(&offering, &compat_fail("no GPU"), None, 0);
        assert_eq!(result, None);
    }

    #[test]
    fn test_healthy_idle_stone_scores_high() {
        let offering = make_offering(false);
        let metrics = make_metrics(10, 28000, 32000); // 10% CPU, 28GB free / 32GB
        let result = compute_fitness_score(&offering, &compat_pass(), Some(&metrics), 1);
        let score = result.unwrap();
        // memory: ~18/20, cpu: 18/20, storage cap: 15, storage hw: 10, health: 15, dist: -3
        // positive ≈ 73, scaled ≈ 876
        assert!(score > 600, "Expected high score, got {}", score);
    }

    #[test]
    fn test_busy_stone_scores_lower() {
        let offering = make_offering(false);
        let metrics = make_metrics(80, 5000, 32000); // 80% CPU, 5GB free / 32GB
        let result = compute_fitness_score(&offering, &compat_pass(), Some(&metrics), 5);
        let score = result.unwrap();
        // memory: ~3/20, cpu: 4/20, storage cap: 15, storage hw: 10, health: 15, dist: -15
        // positive ≈ 32, scaled ≈ 384
        assert!(score < 600, "Expected lower score, got {}", score);
    }

    #[test]
    fn test_no_metrics_mid_range() {
        let offering = make_offering(false);
        let result = compute_fitness_score(&offering, &compat_pass(), None, 0);
        let score = result.unwrap();
        // No metrics → resource=20, capacity=10; health: 15, dist: 0
        // positive: 45, scaled: 540
        assert!(
            score > 300 && score < 800,
            "Expected mid-range score, got {}",
            score
        );
    }

    #[test]
    fn test_score_clamped_to_valid_range() {
        let offering = make_offering(false);
        let metrics = make_metrics(0, 32000, 32000); // fully idle
        let result = compute_fitness_score(&offering, &compat_pass(), Some(&metrics), 0);
        let score = result.unwrap();
        assert!(
            score >= FITNESS_SCORE_MIN && score <= FITNESS_SCORE_MAX,
            "Score {} out of range [{}, {}]",
            score,
            FITNESS_SCORE_MIN,
            FITNESS_SCORE_MAX
        );
    }

    #[test]
    fn test_many_offerings_penalty() {
        let offering = make_offering(false);
        let metrics = make_metrics(30, 20000, 32000);

        let score_few =
            compute_fitness_score(&offering, &compat_pass(), Some(&metrics), 1).unwrap();
        let score_many =
            compute_fitness_score(&offering, &compat_pass(), Some(&metrics), 20).unwrap();

        assert!(
            score_few > score_many,
            "Fewer offerings should score higher: {} vs {}",
            score_few,
            score_many
        );
    }

    #[test]
    fn test_warning_compat_reduces_score() {
        let offering = make_offering(false);
        let metrics = make_metrics(30, 20000, 32000);

        let score_pass =
            compute_fitness_score(&offering, &compat_pass(), Some(&metrics), 1).unwrap();
        let score_warn =
            compute_fitness_score(&offering, &compat_warning("low VRAM"), Some(&metrics), 1)
                .unwrap();

        assert!(
            score_pass > score_warn,
            "Warning compat should reduce score: pass={} vs warn={}",
            score_pass,
            score_warn
        );
    }

    #[test]
    fn test_fallback_compat_reduces_score() {
        let offering = make_offering(false);
        let metrics = make_metrics(30, 20000, 32000);

        let score_pass =
            compute_fitness_score(&offering, &compat_pass(), Some(&metrics), 1).unwrap();
        let score_fallback =
            compute_fitness_score(&offering, &compat_fallback("no AVX"), Some(&metrics), 1)
                .unwrap();

        assert!(
            score_pass > score_fallback,
            "Fallback compat should reduce score: pass={} vs fallback={}",
            score_pass,
            score_fallback
        );
    }

    #[test]
    fn test_warning_worse_than_fallback() {
        // Warning (-50) is a bigger penalty than Fallback (-15)
        let offering = make_offering(false);
        let metrics = make_metrics(30, 20000, 32000);

        let score_warn =
            compute_fitness_score(&offering, &compat_warning("degraded"), Some(&metrics), 1)
                .unwrap();
        let score_fallback =
            compute_fitness_score(&offering, &compat_fallback("no AVX"), Some(&metrics), 1)
                .unwrap();

        assert!(
            score_fallback > score_warn,
            "Fallback (-15) should score higher than Warning (-50): fallback={} vs warn={}",
            score_fallback,
            score_warn
        );
    }

    // ========================================================================
    // Integration tests: fitness → candidate → election resolution
    // ========================================================================
    //
    // These tests exercise the full pipeline across domain::fitness,
    // domain::scoring, and tasks::election_service::resolve_fitness_election
    // — the three modules that compose ORCH-0001's election path.

    use garden_common::election::ElectionCandidate;

    /// Helper: simulate a stone computing its fitness and assembling a candidate.
    fn simulate_stone(
        stone_id: &str,
        stone_name: &str,
        pinned: bool,
        compat: &CompiledCompatibility,
        metrics: Option<&StoneMetrics>,
        offering_count: usize,
    ) -> Option<ElectionCandidate> {
        let offering = make_offering(pinned);
        let score = compute_fitness_score(&offering, compat, metrics, offering_count)?;
        let pin_timestamp = offering
            .orchestration
            .as_ref()
            .and_then(|o| o.pin_timestamp.clone());
        Some(ElectionCandidate {
            election_id: "integration-test".to_string(),
            stone_id: stone_id.to_string(),
            stone_name: stone_name.to_string(),
            score: Some(score),
            pin_timestamp,
        })
    }

    #[test]
    fn integration_idle_stone_beats_busy_stone() {
        let idle_metrics = make_metrics(10, 28000, 32000);
        let busy_metrics = make_metrics(85, 4000, 32000);

        let idle = simulate_stone(
            "stone-idle",
            "Idle",
            false,
            &compat_pass(),
            Some(&idle_metrics),
            1,
        )
        .unwrap();
        let busy = simulate_stone(
            "stone-busy",
            "Busy",
            false,
            &compat_pass(),
            Some(&busy_metrics),
            8,
        )
        .unwrap();

        assert!(
            idle.score.unwrap() > busy.score.unwrap(),
            "Idle should outscore busy"
        );

        let winner =
            crate::tasks::election_service::resolve_fitness_election(&[idle, busy]).unwrap();
        assert_eq!(winner.stone_id, "stone-idle");
    }

    #[test]
    fn integration_pinned_beats_better_hardware() {
        let great_metrics = make_metrics(5, 30000, 32000);
        let mediocre_metrics = make_metrics(50, 16000, 32000);

        let great = simulate_stone(
            "stone-great",
            "Great",
            false,
            &compat_pass(),
            Some(&great_metrics),
            1,
        )
        .unwrap();
        let pinned = simulate_stone(
            "stone-pinned",
            "Pinned",
            true,
            &compat_pass(),
            Some(&mediocre_metrics),
            5,
        )
        .unwrap();

        assert_eq!(pinned.score.unwrap(), FITNESS_SCORE_PINNED);
        assert!(great.score.unwrap() < FITNESS_SCORE_PINNED);

        let winner =
            crate::tasks::election_service::resolve_fitness_election(&[great, pinned]).unwrap();
        assert_eq!(winner.stone_id, "stone-pinned");
    }

    #[test]
    fn integration_compat_fail_excluded_from_election() {
        let good = simulate_stone("stone-good", "Good", false, &compat_pass(), None, 1).unwrap();
        let fail = simulate_stone("stone-fail", "Fail", false, &compat_fail("no GPU"), None, 1);

        // fail stone returns None — doesn't participate
        assert!(fail.is_none());

        let winner = crate::tasks::election_service::resolve_fitness_election(&[good]).unwrap();
        assert_eq!(winner.stone_id, "stone-good");
    }

    #[test]
    fn integration_compat_warning_loses_to_pass() {
        let metrics = make_metrics(30, 20000, 32000);

        let pass = simulate_stone(
            "stone-pass",
            "Pass",
            false,
            &compat_pass(),
            Some(&metrics),
            2,
        )
        .unwrap();
        let warn = simulate_stone(
            "stone-warn",
            "Warn",
            false,
            &compat_warning("low VRAM"),
            Some(&metrics),
            2,
        )
        .unwrap();

        assert!(pass.score.unwrap() > warn.score.unwrap());

        let winner =
            crate::tasks::election_service::resolve_fitness_election(&[pass, warn]).unwrap();
        assert_eq!(winner.stone_id, "stone-pass");
    }

    #[test]
    fn integration_three_stones_mixed_conditions() {
        // Stone A: great hardware, compat pass, few offerings
        let a = simulate_stone(
            "stone-a",
            "Alpha",
            false,
            &compat_pass(),
            Some(&make_metrics(15, 25000, 32000)),
            2,
        )
        .unwrap();

        // Stone B: mediocre hardware, compat fallback
        let b = simulate_stone(
            "stone-b",
            "Bravo",
            false,
            &compat_fallback("no AVX"),
            Some(&make_metrics(40, 15000, 32000)),
            3,
        )
        .unwrap();

        // Stone C: compat fail — excluded entirely
        let c = simulate_stone(
            "stone-c",
            "Charlie",
            false,
            &compat_fail("unsupported arch"),
            Some(&make_metrics(5, 30000, 32000)),
            1,
        );

        assert!(c.is_none(), "Fail compat should be excluded");
        assert!(
            a.score.unwrap() > b.score.unwrap(),
            "Pass compat should beat fallback"
        );

        let winner = crate::tasks::election_service::resolve_fitness_election(&[a, b]).unwrap();
        assert_eq!(winner.stone_id, "stone-a");
    }

    #[test]
    fn integration_all_ineligible_no_winner() {
        let fail1 = simulate_stone("s1", "S1", false, &compat_fail("a"), None, 1);
        let fail2 = simulate_stone("s2", "S2", false, &compat_fail("b"), None, 1);
        assert!(fail1.is_none());
        assert!(fail2.is_none());

        // Empty candidate list → no winner
        let winner = crate::tasks::election_service::resolve_fitness_election(&[]);
        assert!(winner.is_none());
    }

    #[test]
    fn integration_identical_stones_deterministic_tiebreak() {
        let metrics = make_metrics(30, 20000, 32000);

        let a =
            simulate_stone("stone-a", "Alpha", false, &compat_pass(), Some(&metrics), 2).unwrap();
        let z =
            simulate_stone("stone-z", "Zulu", false, &compat_pass(), Some(&metrics), 2).unwrap();

        // Same conditions → same score
        assert_eq!(
            a.score, z.score,
            "Identical conditions should yield same score"
        );

        // Deterministic tiebreak: lexicographically higher stone_id wins
        let winner = crate::tasks::election_service::resolve_fitness_election(&[a, z]).unwrap();
        assert_eq!(winner.stone_id, "stone-z");
    }
}
