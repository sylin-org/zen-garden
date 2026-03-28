//! GPU projected fitness catalog.
//!
//! Maps GPU model names to relative performance scores (0–100 scale).
//! Used as the initial fitness estimate before benchmarks run.
//!
//! Harvested from ollama-orchestrator domain/gpu_catalog.rs — this module
//! is fully generic (no offering-specific logic).

use serde::{Deserialize, Serialize};

/// GPU performance score on a 0–100 scale.
pub type GpuScore = u32;

/// Score assigned to CPU-only inference (no GPU detected).
const CPU_SCORE: GpuScore = 10;

/// Score assigned to GPUs not found in the catalog.
const UNKNOWN_GPU_SCORE: GpuScore = 35;

/// Where the fitness estimate came from (priority: observed > benchmarked > projected).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FitnessSource {
    /// Heuristic from GPU name lookup.
    Projected,
    /// Formal benchmark run.
    Benchmarked,
    /// Live proxy metrics (exponential decay average).
    Observed,
}

/// Resolved fitness for a (model, instance) pair.
#[derive(Debug, Clone)]
pub struct ResolvedFitness {
    pub tokens_per_sec: f64,
    pub source: FitnessSource,
}

/// Project a GPU score from its name. Case-insensitive substring match.
///
/// Returns `CPU_SCORE` for `None` / empty, `UNKNOWN_GPU_SCORE` for
/// unrecognized GPUs, or the catalog entry for recognized ones.
pub fn projected_score(gpu_name: Option<&str>) -> GpuScore {
    let name = match gpu_name {
        Some(n) if !n.is_empty() => n,
        _ => return CPU_SCORE,
    };

    let lower = name.to_lowercase();

    for &(pattern, score) in CATALOG {
        if lower.contains(pattern) {
            return score;
        }
    }

    UNKNOWN_GPU_SCORE
}

/// Normalized score in `0.0..=1.0` range.
pub fn normalized_score(gpu_name: Option<&str>) -> f64 {
    projected_score(gpu_name) as f64 / 100.0
}

/// All catalog entries (for dashboard display).
pub fn catalog_entries() -> Vec<(&'static str, GpuScore)> {
    CATALOG.iter().copied().collect()
}

/// Resolve the best available fitness estimate.
///
/// Priority: observed (live proxy metrics) > benchmarked (formal run) > projected (GPU name).
pub fn resolve_fitness(
    observed_tps: Option<f64>,
    benchmarked_tps: Option<f64>,
    gpu_name: Option<&str>,
) -> ResolvedFitness {
    if let Some(tps) = observed_tps {
        if tps > 0.0 {
            return ResolvedFitness {
                tokens_per_sec: tps,
                source: FitnessSource::Observed,
            };
        }
    }

    if let Some(tps) = benchmarked_tps {
        if tps > 0.0 {
            return ResolvedFitness {
                tokens_per_sec: tps,
                source: FitnessSource::Benchmarked,
            };
        }
    }

    // Projected: convert score to a rough tok/s estimate.
    // RTX 4090 (score 100) ≈ 100 tok/s. Linear scale.
    let score = projected_score(gpu_name);
    ResolvedFitness {
        tokens_per_sec: score as f64,
        source: FitnessSource::Projected,
    }
}

// ── GPU Catalog ─────────────────────────────────────────────────
//
// (pattern, score) — pattern is lowercase, matched via `contains()`.
// Order matters: more specific patterns should come first.

const CATALOG: &[(&str, GpuScore)] = &[
    // NVIDIA Data Center
    ("h200", 100),
    ("h100", 98),
    ("a100 80g", 92),
    ("a100", 88),
    ("l40s", 85),
    ("l40", 82),
    ("l4", 65),
    ("a40", 78),
    ("a30", 72),
    ("a16", 62),
    ("a10g", 60),
    ("a10", 58),
    ("a2", 40),
    ("t4", 50),
    ("v100", 70),
    ("p100", 55),
    ("p40", 48),
    // NVIDIA RTX 50 series
    ("rtx 5090", 98),
    ("rtx 5080", 90),
    ("rtx 5070 ti", 82),
    ("rtx 5070", 78),
    // NVIDIA RTX 40 series
    ("rtx 4090", 100),
    ("rtx 4080 super", 88),
    ("rtx 4080", 85),
    ("rtx 4070 ti super", 78),
    ("rtx 4070 ti", 75),
    ("rtx 4070 super", 72),
    ("rtx 4070", 68),
    ("rtx 4060 ti", 60),
    ("rtx 4060", 55),
    // NVIDIA RTX 30 series
    ("rtx 3090 ti", 82),
    ("rtx 3090", 80),
    ("rtx 3080 ti", 75),
    ("rtx 3080", 72),
    ("rtx 3070 ti", 65),
    ("rtx 3070", 62),
    ("rtx 3060 ti", 55),
    ("rtx 3060", 50),
    // NVIDIA RTX 20 series
    ("rtx 2080 ti", 65),
    ("rtx 2080 super", 60),
    ("rtx 2080", 58),
    ("rtx 2070 super", 55),
    ("rtx 2070", 52),
    ("rtx 2060 super", 48),
    ("rtx 2060", 45),
    // NVIDIA GTX 16 series
    ("gtx 1660 ti", 38),
    ("gtx 1660 super", 37),
    ("gtx 1660", 35),
    ("gtx 1650 super", 32),
    ("gtx 1650", 28),
    // NVIDIA GTX 10 series
    ("gtx 1080 ti", 50),
    ("gtx 1080", 45),
    ("gtx 1070 ti", 40),
    ("gtx 1070", 38),
    ("gtx 1060", 30),
    // AMD Instinct
    ("mi300x", 95),
    ("mi300", 90),
    ("mi250x", 85),
    ("mi250", 82),
    ("mi210", 75),
    ("mi100", 70),
    // AMD Radeon RX 7000
    ("rx 7900 xtx", 80),
    ("rx 7900 xt", 75),
    ("rx 7900 gre", 70),
    ("rx 7800 xt", 62),
    ("rx 7700 xt", 55),
    ("rx 7600 xt", 48),
    ("rx 7600", 45),
    // AMD Radeon RX 6000
    ("rx 6950 xt", 68),
    ("rx 6900 xt", 65),
    ("rx 6800 xt", 60),
    ("rx 6800", 55),
    ("rx 6700 xt", 48),
    ("rx 6600 xt", 40),
    ("rx 6600", 35),
    // Apple Silicon
    ("m4 ultra", 88),
    ("m4 max", 78),
    ("m4 pro", 62),
    ("m4", 50),
    ("m3 ultra", 82),
    ("m3 max", 72),
    ("m3 pro", 58),
    ("m3", 45),
    ("m2 ultra", 78),
    ("m2 max", 68),
    ("m2 pro", 52),
    ("m2", 40),
    ("m1 ultra", 72),
    ("m1 max", 62),
    ("m1 pro", 48),
    ("m1", 35),
    // Intel Arc
    ("arc a770", 45),
    ("arc a750", 40),
    ("arc a580", 35),
    ("arc a380", 25),
];

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_gpu() {
        assert_eq!(projected_score(Some("NVIDIA GeForce RTX 4090")), 100);
        assert_eq!(projected_score(Some("NVIDIA A100 80GB")), 92);
    }

    #[test]
    fn unknown_gpu() {
        assert_eq!(projected_score(Some("Exotic GPU 9999")), UNKNOWN_GPU_SCORE);
    }

    #[test]
    fn cpu_fallback() {
        assert_eq!(projected_score(None), CPU_SCORE);
        assert_eq!(projected_score(Some("")), CPU_SCORE);
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(
            projected_score(Some("nvidia geforce rtx 4090")),
            projected_score(Some("NVIDIA GeForce RTX 4090"))
        );
    }

    #[test]
    fn resolve_priority() {
        // Observed wins over benchmarked
        let r = resolve_fitness(Some(80.0), Some(60.0), Some("RTX 4090"));
        assert_eq!(r.source, FitnessSource::Observed);
        assert!((r.tokens_per_sec - 80.0).abs() < f64::EPSILON);

        // Benchmarked wins over projected
        let r = resolve_fitness(None, Some(60.0), Some("RTX 4090"));
        assert_eq!(r.source, FitnessSource::Benchmarked);

        // Projected fallback
        let r = resolve_fitness(None, None, Some("RTX 4090"));
        assert_eq!(r.source, FitnessSource::Projected);
    }

    #[test]
    fn apple_silicon() {
        assert_eq!(projected_score(Some("Apple M4 Ultra")), 88);
        assert_eq!(projected_score(Some("Apple M1")), 35);
    }
}
