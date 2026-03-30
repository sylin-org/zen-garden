//! Fitness profiler domain types.
//!
//! A single `BenchmarkRun` tree captures everything: user options, per-stone
//! hardware, per-model test suites with raw samples, and a synthesised
//! `GpuMatrix` used by the router for advisory scoring.
//!
//! Pure data — no I/O, no async.  Serialised to `{data_dir}/fitness.json`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::types::Capability;

// ── Verdict ──────────────────────────────────────────────────────

/// Performance verdict for one (model, capability) pair on one GPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Cold start < 30 s AND tok/s > 5 — interactive quality.
    Fast,
    /// Cold start < 90 s AND tok/s > 1 — functional but slow.
    Degraded,
    /// Exceeds thresholds — performance is poor. Deprioritised but still
    /// routable as a last resort (can be overridden by queue pressure).
    Vetoed,
    /// Model errors on this stone (all samples failed, or zero output).
    /// Hard block — router will NOT route here, even as last resort.
    Blocked,
}

impl Verdict {
    /// Routing score: higher is better.
    /// Fast/Degraded/Vetoed are advisory (ORCH-0002). Blocked is hard.
    pub fn score(self) -> u32 {
        match self {
            Self::Fast => 100,
            Self::Degraded => 50,
            Self::Vetoed => 10,
            Self::Blocked => 0,
        }
    }

    /// Whether the router must refuse to route to this verdict.
    pub fn is_blocked(self) -> bool {
        matches!(self, Self::Blocked)
    }

    pub fn compute(capability: Capability, cold_start_ms: u64, tokens_per_second: f64) -> Self {
        match capability {
            Capability::Chat | Capability::Vision => {
                // Zero tok/s on a generate/vision model means it produced
                // no output at all — the model is fundamentally broken on
                // this GPU.  Block it hard.
                if tokens_per_second <= 0.0 {
                    Self::Blocked
                } else if cold_start_ms < 30_000 && tokens_per_second > 5.0 {
                    Self::Fast
                } else if cold_start_ms < 90_000 && tokens_per_second > 1.0 {
                    Self::Degraded
                } else {
                    Self::Vetoed
                }
            }
            Capability::Embed => {
                if cold_start_ms < 5_000 {
                    Self::Fast
                } else if cold_start_ms < 30_000 {
                    Self::Degraded
                } else {
                    Self::Vetoed
                }
            }
            Capability::Think => {
                // Thinking = sustained long generation (2000+ tokens).
                // Relaxed thresholds: users expect slower responses.
                if tokens_per_second <= 0.0 {
                    Self::Blocked
                } else if cold_start_ms < 60_000 && tokens_per_second > 3.0 {
                    Self::Fast
                } else if cold_start_ms < 120_000 && tokens_per_second > 0.5 {
                    Self::Degraded
                } else {
                    Self::Vetoed
                }
            }
            Capability::Tools => {
                // Tools verdict is computed by the benchmark runner using
                // compute_tools_verdict() which factors in correctness.
                // This branch handles the fallback if called directly.
                if tokens_per_second <= 0.0 {
                    Self::Blocked
                } else if cold_start_ms < 30_000 && tokens_per_second > 5.0 {
                    Self::Fast
                } else if cold_start_ms < 90_000 && tokens_per_second > 1.0 {
                    Self::Degraded
                } else {
                    Self::Vetoed
                }
            }
            // Capabilities without established benchmark thresholds
            // use the same thresholds as Generate/Vision.
            _ => {
                if tokens_per_second <= 0.0 {
                    Self::Blocked
                } else if cold_start_ms < 30_000 && tokens_per_second > 5.0 {
                    Self::Fast
                } else if cold_start_ms < 90_000 && tokens_per_second > 1.0 {
                    Self::Degraded
                } else {
                    Self::Vetoed
                }
            }
        }
    }

    /// Compute verdict for tools capability factoring in correctness.
    ///
    /// `valid_count` / `total_prompts` determines the correctness gate:
    /// - All valid: speed determines Fast vs Degraded
    /// - 3-4/5 valid: Degraded (flaky) regardless of speed
    /// - 0-2/5 valid: Vetoed or Blocked
    pub fn compute_tools(
        valid_count: u32,
        total_prompts: u32,
        cold_start_ms: u64,
        tokens_per_second: f64,
    ) -> Self {
        if total_prompts == 0 || valid_count == 0 {
            return Self::Blocked;
        }
        let ratio = valid_count as f64 / total_prompts as f64;
        if ratio < 0.5 {
            // < 50% valid: model can't reliably produce tool calls
            Self::Vetoed
        } else if ratio < 1.0 {
            // Flaky: works sometimes — Degraded regardless of speed
            Self::Degraded
        } else {
            // All valid: let speed decide
            Self::compute(Capability::Tools, cold_start_ms, tokens_per_second)
        }
    }
}

impl std::fmt::Display for Verdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fast => write!(f, "fast"),
            Self::Degraded => write!(f, "degraded"),
            Self::Vetoed => write!(f, "vetoed"),
            Self::Blocked => write!(f, "blocked"),
        }
    }
}

// ── Status enums ─────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Idle,
    Running,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoneStatus {
    Pending,
    Testing,
    Done,
    Skipped,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestStatus {
    Pending,
    Running,
    Done,
    Skipped,
    Error,
}

// ── Run Options ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunOptions {
    /// "full" or "stone:<name>"
    pub scope: String,
    /// Pull missing models before testing.
    pub sync: bool,
    /// Wipe previous results before starting.
    pub wipe: bool,
}

// ── Sample ───────────────────────────────────────────────────────

/// One raw measurement — a single prompt/input on a stone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sample {
    pub prompt_index: u32,
    /// Time to load model into VRAM (ms). Only meaningful for first sample.
    pub cold_start_ms: u64,
    /// Tokens per second (eval_count / eval_duration). Zero for embed.
    pub tokens_per_second: f64,
    /// Total wall-clock duration (ms).
    pub total_duration_ms: u64,
    /// Error message if this sample failed.
    pub error: Option<String>,
}

// ── Test Summary ─────────────────────────────────────────────────

/// Aggregated metrics from a completed test suite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSummary {
    pub median_tps: f64,
    pub cold_start_ms: u64,
    pub median_duration_ms: u64,
    pub verdict: Verdict,
    /// Fraction of valid results (0.0–1.0).  Meaningful for quality-gated
    /// capabilities like Tools; `None` for speed-only benchmarks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_ratio: Option<f64>,
}

// ── Test Suite ───────────────────────────────────────────────────

/// One (model × capability) test on a stone: all samples + summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSuite {
    pub model: String,
    pub capability: Capability,
    pub status: TestStatus,
    pub samples: Vec<Sample>,
    pub summary: Option<TestSummary>,
    pub error: Option<String>,
}

impl TestSuite {
    pub fn new(model: String, capability: Capability) -> Self {
        Self {
            model,
            capability,
            status: TestStatus::Pending,
            samples: Vec::new(),
            summary: None,
            error: None,
        }
    }

    /// Compute the summary from collected samples.
    ///
    /// If ALL samples errored, produces a `Blocked` verdict so the router
    /// refuses to route here (the model is fundamentally broken on this GPU).
    pub fn summarise(&mut self) {
        let ok: Vec<&Sample> = self.samples.iter().filter(|s| s.error.is_none()).collect();
        if ok.is_empty() {
            // Every sample failed → hard-block this model on this stone.
            if !self.samples.is_empty() {
                self.summary = Some(TestSummary {
                    median_tps: 0.0,
                    cold_start_ms: 0,
                    median_duration_ms: 0,
                    verdict: Verdict::Blocked,
                    valid_ratio: None,
                });
            }
            return;
        }
        let cold_start_ms = ok.first().map(|s| s.cold_start_ms).unwrap_or(0);
        let median_tps = median_f64(&ok.iter().map(|s| s.tokens_per_second).collect::<Vec<_>>());
        let median_duration_ms =
            median_u64(&ok.iter().map(|s| s.total_duration_ms).collect::<Vec<_>>());
        let verdict = Verdict::compute(self.capability, cold_start_ms, median_tps);
        self.summary = Some(TestSummary {
            median_tps,
            cold_start_ms,
            median_duration_ms,
            verdict,
            valid_ratio: None,
        });
    }
}

// ── Stone Report ─────────────────────────────────────────────────

/// Per-stone benchmark: hardware snapshot + all test suites.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoneReport {
    pub stone_name: String,
    pub endpoint: String,
    pub gpu_model: String,
    pub vram_mb: u64,
    pub status: StoneStatus,
    pub tests: Vec<TestSuite>,
    pub error: Option<String>,
}

impl StoneReport {
    pub fn completed(&self) -> usize {
        self.tests
            .iter()
            .filter(|t| {
                matches!(
                    t.status,
                    TestStatus::Done | TestStatus::Skipped | TestStatus::Error
                )
            })
            .count()
    }
    pub fn total(&self) -> usize {
        self.tests.len()
    }
}

// ── GPU Matrix ───────────────────────────────────────────────────

/// Synthesised read-optimised view: one entry per (model, capability, stone).
/// Built once after a run completes.  Used by the router.
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
    /// Fraction of valid results (0.0–1.0) for quality-gated capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_ratio: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GpuMatrix {
    pub generated_at: Option<DateTime<Utc>>,
    pub entries: Vec<GpuMatrixEntry>,
}

impl GpuMatrix {
    /// Routing fitness score for a model on an endpoint.
    /// Returns `None` when no data exists (caller treats as Unknown = 25).
    pub fn fitness_score(&self, model: &str, endpoint: &str) -> Option<u32> {
        self.entries
            .iter()
            .filter(|e| e.model == model && e.endpoint == endpoint)
            .map(|e| e.verdict.score())
            .max()
    }
}

// ── Benchmark Run ────────────────────────────────────────────────

/// The single root object — everything about one benchmark run.
/// Persisted to `fitness.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkRun {
    pub id: String,
    pub status: RunStatus,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub options: RunOptions,
    pub stones: Vec<StoneReport>,
    pub gpu_matrix: GpuMatrix,
    pub error: Option<String>,
}

impl Default for BenchmarkRun {
    fn default() -> Self {
        Self {
            id: String::new(),
            status: RunStatus::Idle,
            started_at: None,
            completed_at: None,
            options: RunOptions {
                scope: "full".into(),
                sync: false,
                wipe: false,
            },
            stones: Vec::new(),
            gpu_matrix: GpuMatrix::default(),
            error: None,
        }
    }
}

impl BenchmarkRun {
    pub fn idle() -> Self {
        Self::default()
    }

    pub fn is_running(&self) -> bool {
        self.status == RunStatus::Running
    }

    /// Total progress across all stones: (completed, total).
    pub fn progress(&self) -> (usize, usize) {
        let completed: usize = self.stones.iter().map(|s| s.completed()).sum();
        let total: usize = self.stones.iter().map(|s| s.total()).sum();
        (completed, total)
    }

    /// Synthesise the GPU matrix from completed stone reports.
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
                        gpu_model: stone.gpu_model.clone(),
                        verdict: summary.verdict,
                        median_tps: summary.median_tps,
                        cold_start_ms: summary.cold_start_ms,
                        valid_ratio: summary.valid_ratio,
                    });
                }
            }
        }
        self.gpu_matrix = GpuMatrix {
            generated_at: Some(Utc::now()),
            entries,
        };
    }

    /// Wipe all results (keeps id/options for audit).
    pub fn wipe_all(&mut self) {
        for stone in &mut self.stones {
            stone.tests.clear();
            stone.status = StoneStatus::Pending;
        }
        self.gpu_matrix = GpuMatrix::default();
    }

    /// Wipe results for one stone.
    pub fn wipe_stone(&mut self, stone_name: &str) {
        if let Some(stone) = self.stones.iter_mut().find(|s| s.stone_name == stone_name) {
            stone.tests.clear();
            stone.status = StoneStatus::Pending;
        }
        self.gpu_matrix
            .entries
            .retain(|e| e.stone_name != stone_name);
        self.gpu_matrix.generated_at = Some(Utc::now());
    }
}

// ── Scope ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum BenchmarkScope {
    Full,
    Stone(String),
}

#[derive(Debug, Clone)]
pub enum WipeScope {
    All,
    Stone(String),
}

// ── Math helpers ─────────────────────────────────────────────────

pub fn median_f64(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted: Vec<f64> = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    }
}

pub fn median_u64(values: &[u64]) -> u64 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort();
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) / 2
    } else {
        sorted[mid]
    }
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdict_scoring_order() {
        assert!(Verdict::Fast.score() > Verdict::Degraded.score());
        assert!(Verdict::Degraded.score() > Verdict::Vetoed.score());
        assert!(Verdict::Vetoed.score() > Verdict::Blocked.score());
        assert_eq!(Verdict::Blocked.score(), 0);
    }

    #[test]
    fn compute_verdict_generate() {
        assert_eq!(
            Verdict::compute(Capability::Chat, 5_000, 10.0),
            Verdict::Fast
        );
        assert_eq!(
            Verdict::compute(Capability::Chat, 50_000, 3.0),
            Verdict::Degraded
        );
        assert_eq!(
            Verdict::compute(Capability::Chat, 100_000, 0.5),
            Verdict::Vetoed
        );
        // Zero tok/s means output was never produced — hard block.
        assert_eq!(
            Verdict::compute(Capability::Chat, 100_000, 0.0),
            Verdict::Blocked
        );
    }

    #[test]
    fn compute_verdict_embed() {
        assert_eq!(
            Verdict::compute(Capability::Embed, 2_000, 0.0),
            Verdict::Fast
        );
        assert_eq!(
            Verdict::compute(Capability::Embed, 15_000, 0.0),
            Verdict::Degraded
        );
        assert_eq!(
            Verdict::compute(Capability::Embed, 45_000, 0.0),
            Verdict::Vetoed
        );
    }

    #[test]
    fn test_suite_summarise() {
        let mut suite = TestSuite::new("llama3.2:3b".into(), Capability::Chat);
        suite.samples.push(Sample {
            prompt_index: 0,
            cold_start_ms: 5_000,
            tokens_per_second: 20.0,
            total_duration_ms: 8_000,
            error: None,
        });
        suite.samples.push(Sample {
            prompt_index: 1,
            cold_start_ms: 100,
            tokens_per_second: 25.0,
            total_duration_ms: 3_000,
            error: None,
        });
        suite.summarise();
        let s = suite.summary.unwrap();
        assert_eq!(s.cold_start_ms, 5_000);
        assert_eq!(s.verdict, Verdict::Fast);
        assert!(s.median_tps > 0.0);
    }

    #[test]
    fn summarise_all_errors_produces_blocked() {
        let mut suite = TestSuite::new("glm-ocr:latest".into(), Capability::Vision);
        suite.samples.push(Sample {
            prompt_index: 0,
            cold_start_ms: 0,
            tokens_per_second: 0.0,
            total_duration_ms: 0,
            error: Some("model not compatible with GPU".into()),
        });
        suite.samples.push(Sample {
            prompt_index: 1,
            cold_start_ms: 0,
            tokens_per_second: 0.0,
            total_duration_ms: 0,
            error: Some("CUDA OOM".into()),
        });
        suite.summarise();
        let s = suite
            .summary
            .as_ref()
            .expect("should produce a summary even when all samples error");
        assert_eq!(s.verdict, Verdict::Blocked);
        assert_eq!(s.median_tps, 0.0);
    }

    #[test]
    fn run_synthesise_matrix() {
        let mut run = BenchmarkRun::idle();
        run.stones.push(StoneReport {
            stone_name: "s1".into(),
            endpoint: "http://a".into(),
            gpu_model: "RTX 3060 Ti".into(),
            vram_mb: 8192,
            status: StoneStatus::Done,
            tests: vec![{
                let mut t = TestSuite::new("m1".into(), Capability::Chat);
                t.status = TestStatus::Done;
                t.summary = Some(TestSummary {
                    median_tps: 25.0,
                    cold_start_ms: 5_000,
                    median_duration_ms: 8_000,
                    verdict: Verdict::Fast,
                    valid_ratio: None,
                });
                t
            }],
            error: None,
        });
        run.synthesise_matrix();
        assert_eq!(run.gpu_matrix.entries.len(), 1);
        assert_eq!(run.gpu_matrix.entries[0].verdict, Verdict::Fast);
        assert_eq!(run.gpu_matrix.fitness_score("m1", "http://a"), Some(100));
        assert_eq!(run.gpu_matrix.fitness_score("unknown", "http://a"), None);
    }

    #[test]
    fn compute_verdict_think() {
        // Relaxed thresholds for sustained generation
        assert_eq!(
            Verdict::compute(Capability::Think, 10_000, 8.0),
            Verdict::Fast
        );
        assert_eq!(
            Verdict::compute(Capability::Think, 50_000, 1.0),
            Verdict::Degraded
        );
        // Below 0.5 tok/s sustained = vetoed
        assert_eq!(
            Verdict::compute(Capability::Think, 50_000, 0.3),
            Verdict::Vetoed
        );
        assert_eq!(
            Verdict::compute(Capability::Think, 10_000, 0.0),
            Verdict::Blocked
        );
    }

    #[test]
    fn compute_tools_verdict_correctness() {
        // All valid + fast = Fast
        assert_eq!(
            Verdict::compute_tools(5, 5, 5_000, 20.0),
            Verdict::Fast
        );
        // All valid + slow = Degraded
        assert_eq!(
            Verdict::compute_tools(5, 5, 50_000, 3.0),
            Verdict::Degraded
        );
        // Flaky (4/5) = Degraded regardless of speed
        assert_eq!(
            Verdict::compute_tools(4, 5, 5_000, 50.0),
            Verdict::Degraded
        );
        // Low correctness (2/5) = Vetoed
        assert_eq!(
            Verdict::compute_tools(2, 5, 5_000, 50.0),
            Verdict::Vetoed
        );
        // Zero valid = Blocked
        assert_eq!(
            Verdict::compute_tools(0, 5, 5_000, 50.0),
            Verdict::Blocked
        );
    }
}
