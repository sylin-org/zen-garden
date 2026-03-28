//! Fitness profiling domain types and verdict computation.
//!
//! Defines the benchmark data model and the verdict algorithm that
//! classifies instance×capability performance. The domain layer only
//! stores and evaluates results — actual benchmarking is performed by
//! the offering adapters via `Offering::benchmark()`.
//!
//! Generalized from ollama-orchestrator domain/fitness.rs — uses unified
//! `Capability` enum, threshold computation per-capability is preserved.

use serde::{Deserialize, Serialize};

use super::types::{Capability, Verdict};

// ── Verdict Computation ─────────────────────────────────────────

impl Verdict {
    /// Compute a verdict from benchmark metrics for a given capability.
    ///
    /// Thresholds are per-capability. This is the generalized version of
    /// the Ollama orchestrator's `Verdict::compute()`.
    pub fn compute(capability: Capability, cold_start_ms: u64, tokens_per_second: f64) -> Self {
        match capability.fitness_capability() {
            Capability::Generate | Capability::Vision => {
                if tokens_per_second <= 0.0 {
                    return Self::Blocked;
                }
                if cold_start_ms < 30_000 && tokens_per_second > 5.0 {
                    Self::Fast
                } else if cold_start_ms < 90_000 && tokens_per_second > 1.0 {
                    Self::Degraded
                } else {
                    Self::Vetoed
                }
            }
            Capability::Embed => {
                // Embedding: speed is measured by total duration, not tok/s.
                // cold_start_ms doubles as total duration for embed benchmarks.
                if cold_start_ms < 5_000 {
                    Self::Fast
                } else if cold_start_ms < 30_000 {
                    Self::Degraded
                } else {
                    Self::Vetoed
                }
            }
            Capability::Think => {
                if tokens_per_second <= 0.0 {
                    return Self::Blocked;
                }
                if cold_start_ms < 60_000 && tokens_per_second > 3.0 {
                    Self::Fast
                } else if cold_start_ms < 120_000 && tokens_per_second > 0.5 {
                    Self::Degraded
                } else {
                    Self::Vetoed
                }
            }
            Capability::Imagine => {
                // Image generation: measured by wall-clock time.
                if cold_start_ms < 15_000 {
                    Self::Fast
                } else if cold_start_ms < 60_000 {
                    Self::Degraded
                } else if cold_start_ms < 180_000 {
                    Self::Vetoed
                } else {
                    Self::Blocked
                }
            }
            Capability::Transcribe => {
                // Speech-to-text: measured by wall-clock time for a reference clip.
                if cold_start_ms < 2_000 {
                    Self::Fast
                } else if cold_start_ms < 10_000 {
                    Self::Degraded
                } else if cold_start_ms < 30_000 {
                    Self::Vetoed
                } else {
                    Self::Blocked
                }
            }
            Capability::Speak => {
                // Text-to-speech: measured by time-to-first-byte.
                if cold_start_ms < 500 {
                    Self::Fast
                } else if cold_start_ms < 2_000 {
                    Self::Degraded
                } else if cold_start_ms < 5_000 {
                    Self::Vetoed
                } else {
                    Self::Blocked
                }
            }
            // Tools: uses compute_tools instead.
            // Remaining capabilities use generic thresholds.
            _ => {
                if cold_start_ms < 5_000 {
                    Self::Fast
                } else if cold_start_ms < 30_000 {
                    Self::Degraded
                } else {
                    Self::Vetoed
                }
            }
        }
    }

    /// Compute a tools verdict with correctness gating.
    ///
    /// Tools benchmarks measure both speed and correctness (valid_ratio).
    /// A model that is fast but picks wrong tools is worse than a slow
    /// model that picks correctly.
    pub fn compute_tools(cold_start_ms: u64, tokens_per_second: f64, valid_ratio: f64) -> Self {
        // Zero valid calls = hard block (not routable).
        if valid_ratio <= 0.0 {
            return Self::Blocked;
        }
        if valid_ratio < 0.5 {
            return Self::Vetoed;
        }

        let speed_verdict = Self::compute(Capability::Generate, cold_start_ms, tokens_per_second);

        if valid_ratio < 1.0 {
            // Partially correct: cap at Degraded regardless of speed.
            match speed_verdict {
                Self::Fast => Self::Degraded,
                other => other,
            }
        } else {
            speed_verdict
        }
    }
}

// ── GPU Matrix ──────────────────────────────────────────────────

/// Synthesized fitness matrix: one entry per (model, capability, endpoint).
///
/// Read-optimized view used by the routing engine for O(n) fitness lookups
/// where n = number of entries (typically small per model).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GpuMatrix {
    pub generated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub entries: Vec<GpuMatrixEntry>,
}

impl GpuMatrix {
    /// Best fitness score for a model on an endpoint (across all capabilities).
    ///
    /// Returns the highest non-blocked verdict score, or `None` if no
    /// entries exist for this (model, endpoint) pair.
    pub fn fitness_score(&self, model: &str, endpoint: &str) -> Option<u32> {
        self.entries
            .iter()
            .filter(|e| e.model == model && e.endpoint == endpoint)
            .map(|e| e.verdict.score())
            .max()
    }
}

/// One entry in the fitness matrix.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuMatrixEntry {
    pub model: String,
    pub capability: Capability,
    pub stone_name: String,
    pub endpoint: String,
    pub gpu_model: String,
    pub verdict: Verdict,
    pub median_tps: f64,
    pub cold_start_ms: u64,
    /// Tool-calling correctness ratio (0.0-1.0). Only set for Tools capability.
    pub valid_ratio: Option<f64>,
}

// ── Benchmark Run ───────────────────────────────────────────────

/// Status of a benchmark run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Idle,
    Running,
    Completed,
    Cancelled,
    Failed,
}

/// Per-stone status during a benchmark run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoneStatus {
    Pending,
    Testing,
    Done,
    Skipped,
    Error,
}

/// Per-test status during a benchmark run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestStatus {
    Pending,
    Running,
    Done,
    Skipped,
    Error,
}

/// Summary of a test suite (model x capability) after samples are collected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSummary {
    pub median_tps: f64,
    pub cold_start_ms: u64,
    pub median_duration_ms: u64,
    pub verdict: Verdict,
    pub valid_ratio: Option<f64>,
}

/// A test suite: one model x one capability on one stone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSuite {
    pub model: String,
    pub capability: Capability,
    pub status: TestStatus,
    pub samples: Vec<super::types::Sample>,
    pub summary: Option<TestSummary>,
    pub error: Option<String>,
}

impl TestSuite {
    /// Compute summary from collected samples.
    pub fn summarise(&mut self) {
        let successful: Vec<&super::types::Sample> = self
            .samples
            .iter()
            .filter(|s| s.error.is_none())
            .collect();

        if successful.is_empty() {
            self.summary = Some(TestSummary {
                median_tps: 0.0,
                cold_start_ms: 0,
                median_duration_ms: 0,
                verdict: Verdict::Blocked,
                valid_ratio: None,
            });
            return;
        }

        let mut tps_values: Vec<f64> = successful
            .iter()
            .filter_map(|s| s.tokens_per_second)
            .collect();
        let mut durations: Vec<u64> = successful.iter().map(|s| s.total_duration_ms).collect();
        let cold_start = successful
            .first()
            .map(|s| s.cold_start_ms)
            .unwrap_or(0);

        tps_values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        durations.sort();

        let median_tps = if tps_values.is_empty() {
            0.0
        } else {
            tps_values[tps_values.len() / 2]
        };
        let median_duration = if durations.is_empty() {
            0
        } else {
            durations[durations.len() / 2]
        };

        let valid_ratio = if self.capability == Capability::Tools {
            let valid_count = successful.iter().filter(|s| {
                s.valid_ratio.map_or(false, |r| r >= 1.0)
            }).count();
            Some(valid_count as f64 / successful.len() as f64)
        } else {
            None
        };

        let verdict = if self.capability == Capability::Tools {
            Verdict::compute_tools(cold_start, median_tps, valid_ratio.unwrap_or(1.0))
        } else {
            Verdict::compute(self.capability, cold_start, median_tps)
        };

        self.summary = Some(TestSummary {
            median_tps,
            cold_start_ms: cold_start,
            median_duration_ms: median_duration,
            verdict,
            valid_ratio,
        });
    }
}

/// Per-stone report within a benchmark run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoneReport {
    pub stone_name: String,
    pub endpoint: String,
    pub gpu_model: Option<String>,
    pub vram_mb: Option<u64>,
    pub status: StoneStatus,
    pub tests: Vec<TestSuite>,
    pub error: Option<String>,
}

/// Root benchmark run object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkRun {
    pub id: String,
    pub status: RunStatus,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub stones: Vec<StoneReport>,
    pub gpu_matrix: GpuMatrix,
    pub error: Option<String>,
}

impl BenchmarkRun {
    /// Create an idle (no-op) benchmark run.
    pub fn idle() -> Self {
        Self {
            id: String::new(),
            status: RunStatus::Idle,
            started_at: None,
            completed_at: None,
            stones: vec![],
            gpu_matrix: GpuMatrix::default(),
            error: None,
        }
    }

    /// Whether a benchmark is currently running.
    pub fn is_running(&self) -> bool {
        self.status == RunStatus::Running
    }

    /// Synthesize the GPU matrix from completed stone reports.
    pub fn synthesise_matrix(&mut self) {
        let mut entries = Vec::new();
        for stone in &self.stones {
            for test in &stone.tests {
                if let Some(ref summary) = test.summary {
                    entries.push(GpuMatrixEntry {
                        model: test.model.clone(),
                        capability: test.capability,
                        stone_name: stone.stone_name.clone(),
                        endpoint: stone.endpoint.clone(),
                        gpu_model: stone.gpu_model.clone().unwrap_or_default(),
                        verdict: summary.verdict,
                        median_tps: summary.median_tps,
                        cold_start_ms: summary.cold_start_ms,
                        valid_ratio: summary.valid_ratio,
                    });
                }
            }
        }
        self.gpu_matrix = GpuMatrix {
            generated_at: Some(chrono::Utc::now()),
            entries,
        };
    }
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::types::Sample;

    #[test]
    fn generate_verdict_fast() {
        let v = Verdict::compute(Capability::Generate, 5_000, 20.0);
        assert_eq!(v, Verdict::Fast);
    }

    #[test]
    fn generate_verdict_degraded() {
        let v = Verdict::compute(Capability::Generate, 50_000, 2.0);
        assert_eq!(v, Verdict::Degraded);
    }

    #[test]
    fn generate_verdict_blocked_zero_tps() {
        let v = Verdict::compute(Capability::Generate, 5_000, 0.0);
        assert_eq!(v, Verdict::Blocked);
    }

    #[test]
    fn embed_verdict_fast() {
        let v = Verdict::compute(Capability::Embed, 2_000, 0.0);
        assert_eq!(v, Verdict::Fast);
    }

    #[test]
    fn think_verdict_fast() {
        let v = Verdict::compute(Capability::Think, 30_000, 5.0);
        assert_eq!(v, Verdict::Fast);
    }

    #[test]
    fn tools_verdict_correctness_gate() {
        // Fast speed but low correctness → Vetoed
        let v = Verdict::compute_tools(5_000, 20.0, 0.3);
        assert_eq!(v, Verdict::Vetoed);

        // Fast speed, perfect correctness → Fast
        let v = Verdict::compute_tools(5_000, 20.0, 1.0);
        assert_eq!(v, Verdict::Fast);

        // Fast speed, partial correctness → Degraded (capped)
        let v = Verdict::compute_tools(5_000, 20.0, 0.8);
        assert_eq!(v, Verdict::Degraded);

        // Zero valid calls → Blocked (hard routing exclusion)
        let v = Verdict::compute_tools(5_000, 20.0, 0.0);
        assert_eq!(v, Verdict::Blocked);
    }

    #[test]
    fn imagine_verdict_thresholds() {
        assert_eq!(Verdict::compute(Capability::Imagine, 10_000, 0.0), Verdict::Fast);
        assert_eq!(Verdict::compute(Capability::Imagine, 30_000, 0.0), Verdict::Degraded);
        assert_eq!(Verdict::compute(Capability::Imagine, 100_000, 0.0), Verdict::Vetoed);
        assert_eq!(Verdict::compute(Capability::Imagine, 200_000, 0.0), Verdict::Blocked);
    }

    #[test]
    fn transcribe_verdict_thresholds() {
        assert_eq!(Verdict::compute(Capability::Transcribe, 1_000, 0.0), Verdict::Fast);
        assert_eq!(Verdict::compute(Capability::Transcribe, 5_000, 0.0), Verdict::Degraded);
        assert_eq!(Verdict::compute(Capability::Transcribe, 35_000, 0.0), Verdict::Blocked);
    }

    #[test]
    fn gpu_matrix_fitness_score() {
        let matrix = GpuMatrix {
            generated_at: None,
            entries: vec![
                GpuMatrixEntry {
                    model: "m7b".into(),
                    capability: Capability::Generate,
                    stone_name: "s1".into(),
                    endpoint: "a".into(),
                    gpu_model: "RTX 4090".into(),
                    verdict: Verdict::Fast,
                    median_tps: 25.0,
                    cold_start_ms: 3_000,
                    valid_ratio: None,
                },
                GpuMatrixEntry {
                    model: "m7b".into(),
                    capability: Capability::Embed,
                    stone_name: "s1".into(),
                    endpoint: "a".into(),
                    gpu_model: "RTX 4090".into(),
                    verdict: Verdict::Degraded,
                    median_tps: 0.0,
                    cold_start_ms: 10_000,
                    valid_ratio: None,
                },
            ],
        };

        // Best score across capabilities = Fast (100)
        assert_eq!(matrix.fitness_score("m7b", "a"), Some(100));
        // Unknown model/endpoint = None
        assert_eq!(matrix.fitness_score("m7b", "unknown"), None);
    }

    #[test]
    fn test_suite_summarise() {
        let mut suite = TestSuite {
            model: "m7b".into(),
            capability: Capability::Generate,
            status: TestStatus::Done,
            samples: vec![
                Sample {
                    prompt_index: 0,
                    cold_start_ms: 5_000,
                    tokens_per_second: Some(20.0),
                    total_duration_ms: 8_000,
                    valid_ratio: None,
                    error: None,
                },
                Sample {
                    prompt_index: 1,
                    cold_start_ms: 1_000,
                    tokens_per_second: Some(25.0),
                    total_duration_ms: 6_000,
                    valid_ratio: None,
                    error: None,
                },
            ],
            summary: None,
            error: None,
        };

        suite.summarise();
        let summary = suite.summary.unwrap();
        assert_eq!(summary.verdict, Verdict::Fast);
        assert!(summary.median_tps > 0.0);
    }

    #[test]
    fn all_errors_produce_blocked() {
        let mut suite = TestSuite {
            model: "m7b".into(),
            capability: Capability::Generate,
            status: TestStatus::Done,
            samples: vec![Sample {
                prompt_index: 0,
                cold_start_ms: 0,
                tokens_per_second: None,
                total_duration_ms: 0,
                valid_ratio: None,
                error: Some("connection refused".into()),
            }],
            summary: None,
            error: None,
        };

        suite.summarise();
        assert_eq!(suite.summary.unwrap().verdict, Verdict::Blocked);
    }
}
