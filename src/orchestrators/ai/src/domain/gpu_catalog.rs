//! GPU projected fitness catalog: maps GPU names to relative performance.
//!
//! Pure domain — no I/O. Provides a coarse performance estimate from the
//! GPU name alone, useful at T=0 before any benchmarks or observed metrics.
//!
//! The score is relative (0–100 scale normalized to consumer GPUs).
//! Actual tok/s varies wildly by model size, quantization, and batch size,
//! but within a tier the ranking is directionally correct.
//!
//! Source: memory bandwidth and FP16 throughput of each architecture.

use std::sync::LazyLock;

/// Relative performance score (0–100) for a GPU.
///
/// Higher = faster inference throughput. The scale is anchored to:
/// - 100 = RTX 4090 (top consumer)
/// - 50  = typical mid-range (RTX 3060 / A2000)
/// - 20  = low-end / old gen (GTX 1660)
/// - 10  = CPU-only fallback
pub type GpuScore = u32;

/// Default score for GPUs not in the catalog.
pub const UNKNOWN_GPU_SCORE: GpuScore = 35;

/// Score for CPU-only inference (no GPU detected).
pub const CPU_SCORE: GpuScore = 10;

/// Lookup table: GPU name substring → projected performance score.
///
/// Entries are checked by substring match (case-insensitive) against the
/// GPU name reported by the stone's hardware detection. First match wins,
/// so more specific entries should appear before broader ones.
static GPU_CATALOG: LazyLock<Vec<(&str, GpuScore)>> = LazyLock::new(|| vec![
    // ── NVIDIA Data Center ──────────────────────────────────────
    ("H100",     100),
    ("A100",      95),
    ("L40S",      90),
    ("L40",       85),
    ("A40",       80),
    ("A30",       75),
    ("A10G",      70),
    ("A10",       68),
    ("A16",       55),
    ("T4",        45),
    ("V100",      55),
    ("P100",      40),

    // ── NVIDIA RTX 50 Series (Blackwell, 2025) ─────────────────
    ("RTX 5090",  98),
    ("RTX 5080",  88),
    ("RTX 5070 Ti", 78),
    ("RTX 5070",  72),
    ("RTX 5060 Ti", 62),
    ("RTX 5060",  55),

    // ── NVIDIA RTX 40 Series (Ada Lovelace) ────────────────────
    ("RTX 4090",  95),
    ("RTX 4080 SUPER", 85),
    ("RTX 4080",  82),
    ("RTX 4070 Ti SUPER", 78),
    ("RTX 4070 Ti", 75),
    ("RTX 4070 SUPER", 72),
    ("RTX 4070",  68),
    ("RTX 4060 Ti", 58),
    ("RTX 4060",  50),

    // ── NVIDIA RTX 30 Series (Ampere) ──────────────────────────
    ("RTX 3090 Ti", 80),
    ("RTX 3090",  75),
    ("RTX 3080 Ti", 72),
    ("RTX 3080",  68),
    ("RTX 3070 Ti", 62),
    ("RTX 3070",  58),
    ("RTX 3060 Ti", 52),
    ("RTX 3060",  48),
    ("RTX 3050",  35),

    // ── NVIDIA RTX 20 Series (Turing) ──────────────────────────
    ("RTX 2080 Ti", 58),
    ("RTX 2080 SUPER", 52),
    ("RTX 2080",  50),
    ("RTX 2070 SUPER", 48),
    ("RTX 2070",  45),
    ("RTX 2060 SUPER", 42),
    ("RTX 2060",  40),

    // ── NVIDIA GTX 16 Series ───────────────────────────────────
    ("GTX 1660 Ti", 30),
    ("GTX 1660 SUPER", 30),
    ("GTX 1660",  28),
    ("GTX 1650",  22),

    // ── NVIDIA GTX 10 Series ───────────────────────────────────
    ("GTX 1080 Ti", 38),
    ("GTX 1080",  32),
    ("GTX 1070 Ti", 30),
    ("GTX 1070",  28),

    // ── NVIDIA Workstation (RTX A-series) ──────────────────────
    ("RTX A6000", 85),
    ("RTX A5500", 78),
    ("RTX A5000", 75),
    ("RTX A4500", 68),
    ("RTX A4000", 62),
    ("RTX A2000", 45),

    // ── AMD Radeon (ROCm) ──────────────────────────────────────
    ("RX 7900 XTX", 70),
    ("RX 7900 XT",  62),
    ("RX 7800 XT",  52),
    ("RX 7700 XT",  45),
    ("RX 7600",     38),
    ("RX 6900 XT",  55),
    ("RX 6800 XT",  50),
    ("RX 6800",     45),
    ("RX 6700 XT",  38),

    // ── AMD Instinct (Data Center) ─────────────────────────────
    ("MI300X",    98),
    ("MI300A",    92),
    ("MI250X",    85),
    ("MI250",     80),
    ("MI210",     70),
    ("MI100",     60),

    // ── Apple Silicon (Metal) ──────────────────────────────────
    ("M4 Ultra",  72),
    ("M4 Max",    65),
    ("M4 Pro",    50),
    ("M4",        38),
    ("M3 Ultra",  65),
    ("M3 Max",    58),
    ("M3 Pro",    42),
    ("M3",        32),
    ("M2 Ultra",  58),
    ("M2 Max",    50),
    ("M2 Pro",    38),
    ("M2",        28),
    ("M1 Ultra",  48),
    ("M1 Max",    42),
    ("M1 Pro",    32),
    ("M1",        22),

    // ── Intel Arc (oneAPI) ─────────────────────────────────────
    ("Arc A770",  40),
    ("Arc A750",  35),
    ("Arc A580",  30),
    ("Arc A380",  20),
]);

/// Look up projected performance for a GPU by name.
///
/// Performs case-insensitive substring matching against the catalog.
/// Returns `CPU_SCORE` for "CPU" or empty names, `UNKNOWN_GPU_SCORE`
/// for unrecognized GPUs.
pub fn projected_score(gpu_name: Option<&str>) -> GpuScore {
    let name = match gpu_name {
        Some(n) if !n.is_empty() => n,
        _ => return CPU_SCORE,
    };

    if name.eq_ignore_ascii_case("CPU") || name.eq_ignore_ascii_case("cpu-only") {
        return CPU_SCORE;
    }

    let upper = name.to_uppercase();
    for &(pattern, score) in GPU_CATALOG.iter() {
        if upper.contains(&pattern.to_uppercase()) {
            return score;
        }
    }

    UNKNOWN_GPU_SCORE
}

/// Normalize a GPU score to a 0.0–1.0 scale.
pub fn normalized_score(gpu_name: Option<&str>) -> f64 {
    projected_score(gpu_name) as f64 / 100.0
}

/// All known GPU names in the catalog (for diagnostics).
pub fn catalog_entries() -> Vec<(&'static str, GpuScore)> {
    GPU_CATALOG.clone()
}

// ── Fitness Source ──────────────────────────────────────────────

/// How a fitness value was determined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FitnessSource {
    /// GPU name → lookup table heuristic.
    Projected,
    /// Formal benchmark evaluation.
    Benchmarked,
    /// Live throughput observed from proxy requests.
    Observed,
}

/// Resolved fitness for a (model, stone) pair.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ResolvedFitness {
    /// Throughput estimate in tokens/second.
    pub tokens_per_sec: f64,
    /// How this value was determined.
    pub source: FitnessSource,
}

/// Resolve fitness for a model on a stone using the three-source priority:
/// observed > benchmarked > projected.
///
/// - `observed_tps`: from DemandLedger (live proxy metrics)
/// - `benchmarked_tps`: from GpuMatrix (formal benchmark)
/// - `gpu_name`: for projected fallback
pub fn resolve_fitness(
    observed_tps: Option<f64>,
    benchmarked_tps: Option<f64>,
    gpu_name: Option<&str>,
) -> ResolvedFitness {
    if let Some(tps) = observed_tps {
        if tps > 0.1 {
            return ResolvedFitness {
                tokens_per_sec: tps,
                source: FitnessSource::Observed,
            };
        }
    }

    if let Some(tps) = benchmarked_tps {
        if tps > 0.1 {
            return ResolvedFitness {
                tokens_per_sec: tps,
                source: FitnessSource::Benchmarked,
            };
        }
    }

    // Projected: scale the normalized GPU score to a rough tok/s estimate.
    // An RTX 4090 (score 95) might do ~95 tok/s on a 7B model; we use
    // the score directly as a coarse tok/s proxy. Not accurate in absolute
    // terms, but preserves relative ordering for placement decisions.
    let score = projected_score(gpu_name);
    ResolvedFitness {
        tokens_per_sec: score as f64,
        source: FitnessSource::Projected,
    }
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_gpus() {
        assert_eq!(projected_score(Some("NVIDIA GeForce RTX 4090")), 95);
        assert_eq!(projected_score(Some("NVIDIA GeForce RTX 3060")), 48);
        assert_eq!(projected_score(Some("Apple M2 Max")), 50);
    }

    #[test]
    fn unknown_gpu() {
        assert_eq!(projected_score(Some("FooBar GPU 9000")), UNKNOWN_GPU_SCORE);
    }

    #[test]
    fn cpu_fallback() {
        assert_eq!(projected_score(None), CPU_SCORE);
        assert_eq!(projected_score(Some("CPU")), CPU_SCORE);
        assert_eq!(projected_score(Some("")), CPU_SCORE);
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(
            projected_score(Some("nvidia geforce rtx 4070 ti super")),
            78
        );
    }

    #[test]
    fn resolve_priority() {
        // Observed wins
        let f = resolve_fitness(Some(80.0), Some(50.0), Some("RTX 4090"));
        assert_eq!(f.source, FitnessSource::Observed);
        assert!((f.tokens_per_sec - 80.0).abs() < 0.01);

        // Benchmarked wins when no observed
        let f = resolve_fitness(None, Some(50.0), Some("RTX 4090"));
        assert_eq!(f.source, FitnessSource::Benchmarked);

        // Projected when nothing else
        let f = resolve_fitness(None, None, Some("RTX 4090"));
        assert_eq!(f.source, FitnessSource::Projected);
        assert!((f.tokens_per_sec - 95.0).abs() < 0.01);
    }
}
