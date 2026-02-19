//! Fitness benchmark runner.
//!
//! Single `BenchmarkRun` tree: options → stones → tests → samples.
//! Persisted after every test completes.  Rich SSE notifications let the
//! dashboard show exactly what's happening.

use crate::app_state::AppState;
use crate::domain::fitness::*;
use crate::domain::types::{JobKind, JobStatus, ModelInfo};
use crate::infra::ollama_client::OllamaClient;

use anyhow::Result;
use base64::Engine;
use chrono::Utc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

// ── Benchmark Payloads ───────────────────────────────────────────

const GENERATE_PROMPTS: &[&str] = &[
    "What is 2 + 2?",
    "Explain why the sky is blue in two sentences.",
    "Write a haiku about a mountain.",
    "List three differences between cats and dogs.",
    "Describe the process of making bread in a short paragraph.",
    "What are the main causes of climate change?",
    "Explain the concept of recursion in programming with an example.",
    "Write a short story opening about a detective finding a mysterious letter.",
    "Compare photosynthesis and cellular respiration, covering inputs, outputs, and energy flow.",
    "Compare and contrast REST and GraphQL APIs, covering authentication, versioning, and caching strategies.",
];

const EMBED_INPUTS: &[&str] = &[
    "photosynthesis",
    "The quick brown fox jumps over the lazy dog.",
    "Machine learning is a subset of artificial intelligence that focuses on building systems \
     that learn from data. These systems improve their performance over time without being \
     explicitly programmed. Applications range from image recognition to natural language \
     processing and autonomous vehicles.",
    "The industrial revolution fundamentally transformed human civilization during the 18th \
     and 19th centuries. Beginning in Britain with innovations in textile manufacturing and \
     steam power, it spread across Europe and North America, reshaping economies, social \
     structures, and daily life. Factory systems replaced cottage industries, leading to \
     urbanisation as workers migrated to cities for employment. Transportation was \
     revolutionised by railways and steamships, enabling faster movement of goods and people. \
     The revolution also brought significant challenges including poor working conditions, \
     child labour, and environmental pollution that society continues to grapple with today.",
    "The history of computing spans several centuries, beginning with mechanical calculators \
     in the 17th century. Charles Babbage's Analytical Engine in the 1830s introduced concepts \
     that would later define modern computers: memory, processing, input, and output. Ada \
     Lovelace wrote what is considered the first computer program for this machine. The 20th \
     century saw explosive progress: Alan Turing formalised computation theory in 1936, ENIAC \
     became the first general-purpose electronic computer in 1945, and the transistor \
     replaced vacuum tubes in the 1950s. The invention of the integrated circuit led to \
     miniaturisation, culminating in the microprocessor revolution of the 1970s. Personal \
     computers democratised computing in the 1980s, the World Wide Web connected them in the \
     1990s, and mobile devices put computing power in every pocket by the 2010s. Today, \
     artificial intelligence and quantum computing represent the latest frontiers, promising \
     to solve problems that remain intractable for classical machines. Each generation built \
     upon the last, creating an accelerating curve of capability that shows no sign of \
     slowing down.",
];

const VISION_IMAGES: &[(&str, &[u8])] = &[
    ("simple object", include_bytes!("../../assets/benchmark/01-simple-object.jpg")),
    ("outdoor scene", include_bytes!("../../assets/benchmark/02-outdoor-scene.jpg")),
    ("text in image", include_bytes!("../../assets/benchmark/03-text-in-image.jpg")),
    ("chart or diagram", include_bytes!("../../assets/benchmark/04-chart-or-diagram.jpg")),
    ("technical diagram", include_bytes!("../../assets/benchmark/05-technical-diagram.jpg")),
];

const VISION_PROMPT: &str = "Describe what you see in this image in detail.";
const NUM_PREDICT: u32 = 80;
const YIELD_DELAY: Duration = Duration::from_secs(5);
const MAX_YIELD_WAIT: Duration = Duration::from_secs(300);

// ── Public Entry Point ───────────────────────────────────────────

/// Start a benchmark run.  Spawns the work on a background task.
pub async fn start(
    state: AppState,
    client: OllamaClient,
    scope: BenchmarkScope,
    sync: bool,
    wipe: Option<WipeScope>,
) {
    let cancel = CancellationToken::new();
    {
        let mut guard = state.benchmark_cancel.write().await;
        if let Some(prev) = guard.take() {
            prev.cancel();
        }
        *guard = Some(cancel.clone());
    }

    tokio::spawn(async move {
        run_benchmark(state, client, scope, sync, wipe, cancel).await;
    });
}

/// Cancel a running benchmark.
pub async fn cancel(state: &AppState) {
    let mut guard = state.benchmark_cancel.write().await;
    if let Some(token) = guard.take() {
        token.cancel();
    }
    // Mark run as cancelled
    let mut run = state.benchmark_run.write().await;
    if run.is_running() {
        run.status = RunStatus::Cancelled;
        run.completed_at = Some(Utc::now());
    }
}

// ── Core Runner ──────────────────────────────────────────────────

async fn run_benchmark(
    state: AppState,
    client: OllamaClient,
    scope: BenchmarkScope,
    sync: bool,
    wipe: Option<WipeScope>,
    cancel: CancellationToken,
) {
    let scope_label = match &scope {
        BenchmarkScope::Full => "full".to_string(),
        BenchmarkScope::Stone(name) => format!("stone:{name}"),
    };
    tracing::info!(scope = %scope_label, sync, "fitness benchmark starting");

    // ── Step 0: Create Job + initialise BenchmarkRun ─────────────
    let job_id = state
        .create_job(JobKind::Benchmark {
            scope: scope_label.clone(),
            stones: vec![],
        })
        .await;
    state
        .update_job(&job_id, JobStatus::Running, Some("initialising".into()))
        .await;

    let run_id = format!("run-{}", Utc::now().timestamp_millis());
    {
        let mut run = state.benchmark_run.write().await;
        *run = BenchmarkRun {
            id: run_id.clone(),
            status: RunStatus::Running,
            started_at: Some(Utc::now()),
            completed_at: None,
            options: RunOptions {
                scope: scope_label.clone(),
                sync,
                wipe: wipe.is_some(),
            },
            stones: Vec::new(),
            gpu_matrix: GpuMatrix::default(),
            error: None,
        };
    }

    notify(&state, "benchmark.started", &serde_json::json!({
        "id": &run_id, "scope": &scope_label, "sync": sync
    })).await;

    // ── Step 1: Apply wipe (on the previous run's gpu_matrix) ────
    if let Some(ref wipe_scope) = wipe {
        tracing::info!(?wipe_scope, "wiping previous results");
        notify(&state, "benchmark.wipe", &serde_json::json!({
            "scope": format!("{wipe_scope:?}")
        })).await;
        // Wipe only affects the gpu_matrix from a prior run; the new run
        // starts with an empty stones vec anyway.  We clear the old matrix
        // so routing stops using stale data during the run.
        let mut run = state.benchmark_run.write().await;
        match wipe_scope {
            WipeScope::All => run.gpu_matrix = GpuMatrix::default(),
            WipeScope::Stone(name) => {
                run.gpu_matrix.entries.retain(|e| e.stone_name != *name);
            }
        }
    }

    // ── Step 2: Gather target stones ─────────────────────────────
    let targets: Vec<(String, String, String, u64, Vec<String>)> = {
        let instances = state.instances.read().await;
        instances
            .values()
            .filter(|i| {
                i.health.is_routable()
                    && match &scope {
                        BenchmarkScope::Full => true,
                        BenchmarkScope::Stone(name) => i.stone_name == *name,
                    }
            })
            .map(|i| (
                i.endpoint.clone(),
                i.stone_name.clone(),
                i.gpu_name.clone().unwrap_or_else(|| "Unknown GPU".into()),
                i.vram_total_bytes / 1_048_576,
                i.models_available.clone(),
            ))
            .collect()
    };

    if targets.is_empty() {
        tracing::warn!("no healthy stones matched benchmark scope");
        let mut run = state.benchmark_run.write().await;
        run.status = RunStatus::Failed;
        run.error = Some("No healthy stones matched scope".into());
        run.completed_at = Some(Utc::now());
        drop(run);
        persist(&state).await;
        state.fail_job(&job_id, "no healthy stones matched scope").await;
        notify(&state, "benchmark.failed", &serde_json::json!({
            "error": "No healthy stones matched scope"
        })).await;
        return;
    }

    // ── Step 3: Build work plan ──────────────────────────────────
    let all_models: Vec<ModelInfo> = {
        let models = state.models.read().await;
        let mut v: Vec<ModelInfo> = models.values().cloned().collect();
        v.sort_by_key(|m| m.size_disk);
        v
    };

    // Build stone reports with test suites
    {
        let mut run = state.benchmark_run.write().await;
        for (endpoint, stone_name, gpu_model, vram_mb, available) in &targets {
            let vram_bytes = *vram_mb * 1_048_576;
            let mut tests = Vec::new();
            for model_info in &all_models {
                // Universal VRAM gate: skip models that won't fit.
                // Both size_disk and vram are always known; treat zero
                // as corrupt data and skip defensively.
                if vram_bytes == 0 || model_info.size_disk == 0
                    || model_info.size_disk > vram_bytes
                {
                    tracing::debug!(
                        stone = %stone_name, model = %model_info.name,
                        model_mb = model_info.size_disk / 1_048_576, vram_mb,
                        "skipping — model too large for stone VRAM"
                    );
                    continue;
                }
                let on_stone = available.iter().any(|m| m == &model_info.name);
                if !on_stone && !sync {
                    continue;
                }
                for cap in capabilities_to_test(model_info) {
                    tests.push(TestSuite::new(model_info.name.clone(), cap));
                }
            }
            run.stones.push(StoneReport {
                stone_name: stone_name.clone(),
                endpoint: endpoint.clone(),
                gpu_model: gpu_model.clone(),
                vram_mb: *vram_mb,
                status: StoneStatus::Pending,
                tests,
                error: None,
            });
        }
        let (completed, total) = run.progress();
        tracing::info!(total, stones = run.stones.len(), "work plan ready");
        drop(run);
        persist(&state).await;

        let stone_names: Vec<String> = targets.iter().map(|(_, sn, _, _, _)| sn.clone()).collect();
        state
            .update_job(
                &job_id,
                JobStatus::Running,
                Some(format!("{total} tests across {} stones", stone_names.len())),
            )
            .await;
        notify(&state, "benchmark.planned", &serde_json::json!({
            "total": total, "completed": completed,
            "stones": stone_names,
        })).await;
    }

    // ── Step 4: Per-stone parallel execution ───────────────────
    // One tokio task per stone.  Each stone writes only to its own
    // StoneReport so there is no cross-stone contention.  Within a
    // single stone, tests run sequentially (one GPU at a time).
    let stone_count = targets.len();
    let mut handles = Vec::with_capacity(stone_count);

    for (stone_idx, (endpoint, stone_name, _gpu, _vram, available)) in targets.into_iter().enumerate() {
        let state = state.clone();
        let client = client.clone();
        let cancel = cancel.clone();
        let all_models = all_models.clone();
        let job_id = job_id.clone();

        handles.push(tokio::spawn(async move {
            if cancel.is_cancelled() {
                return;
            }

            // Mark stone as testing
            {
                let mut run = state.benchmark_run.write().await;
                if let Some(sr) = run.stones.iter_mut().find(|s| s.stone_name == stone_name) {
                    sr.status = StoneStatus::Testing;
                }
            }
            notify(&state, "benchmark.stone.start", &serde_json::json!({
                "stone": &stone_name,
                "index": stone_idx,
                "of": stone_count,
            })).await;

            let stone_err = run_stone(
                &state, &client, &endpoint, &stone_name, &available,
                &all_models, sync, &cancel, &job_id,
            ).await;

            // Mark stone done or error
            {
                let mut run = state.benchmark_run.write().await;
                if let Some(sr) = run.stones.iter_mut().find(|s| s.stone_name == stone_name) {
                    if cancel.is_cancelled() {
                        sr.status = StoneStatus::Skipped;
                    } else if let Some(ref err) = stone_err {
                        sr.status = StoneStatus::Error;
                        sr.error = Some(err.clone());
                    } else {
                        sr.status = StoneStatus::Done;
                    }
                }
            }
            persist(&state).await;

            notify(&state, "benchmark.stone.done", &serde_json::json!({
                "stone": &stone_name,
                "status": if cancel.is_cancelled() { "cancelled" }
                          else if stone_err.is_some() { "error" }
                          else { "done" },
            })).await;
        }));
    }

    // Wait for all stones to finish
    for handle in handles {
        let _ = handle.await;
    }

    // ── Step 5: Finalise ─────────────────────────────────────────
    let was_cancelled = cancel.is_cancelled();
    {
        let mut run = state.benchmark_run.write().await;
        if was_cancelled {
            run.status = RunStatus::Cancelled;
        } else {
            run.synthesise_matrix();
            run.status = RunStatus::Completed;
        }
        run.completed_at = Some(Utc::now());
    }
    {
        let mut guard = state.benchmark_cancel.write().await;
        *guard = None;
    }
    persist(&state).await;

    if was_cancelled {
        tracing::info!("fitness benchmark cancelled");
        state.fail_job(&job_id, "cancelled by user").await;
        notify(&state, "benchmark.cancelled", &serde_json::json!({})).await;
    } else {
        let run = state.benchmark_run.read().await;
        let (done, total) = run.progress();
        let matrix_count = run.gpu_matrix.entries.len();
        drop(run);
        tracing::info!(results = matrix_count, "fitness benchmark completed");
        state
            .update_job(&job_id, JobStatus::Running, Some(format!("{matrix_count} results")))
            .await;
        state.complete_job(&job_id).await;
        notify(&state, "benchmark.completed", &serde_json::json!({
            "results": matrix_count, "completed": done, "total": total,
        })).await;
    }
}

// ── Per-Stone Runner ─────────────────────────────────────────────

async fn run_stone(
    state: &AppState,
    client: &OllamaClient,
    endpoint: &str,
    stone_name: &str,
    available_models: &[String],
    _all_models: &[ModelInfo],
    sync: bool,
    cancel: &CancellationToken,
    job_id: &str,
) -> Option<String> {
    tracing::info!(stone = %stone_name, "benchmarking stone");

    // Collect test indices for this stone
    let test_keys: Vec<(String, Capability)> = {
        let run = state.benchmark_run.read().await;
        let sr = run.stones.iter().find(|s| s.stone_name == stone_name)?;
        sr.tests.iter().map(|t| (t.model.clone(), t.capability)).collect()
    };

    // ── Phase 1: Sync — pull all missing models before any tests ──
    if sync {
        // Deduplicate: only pull each model name once
        let mut models_to_pull: Vec<String> = test_keys
            .iter()
            .map(|(m, _)| m.clone())
            .filter(|m| !available_models.iter().any(|a| a == m))
            .collect();
        models_to_pull.dedup(); // safe because test_keys groups same model together

        if !models_to_pull.is_empty() {
            tracing::info!(
                stone = %stone_name,
                count = models_to_pull.len(),
                "syncing missing models before benchmark"
            );
            notify(state, "benchmark.sync.start", &serde_json::json!({
                "stone": stone_name,
                "models": &models_to_pull,
            })).await;

            for model_name in &models_to_pull {
                if cancel.is_cancelled() {
                    return None;
                }
                notify(state, "benchmark.pull", &serde_json::json!({
                    "stone": stone_name, "model": model_name,
                })).await;
                match pull_model_and_wait(client, endpoint, model_name).await {
                    Ok(()) => {
                        tracing::info!(stone = %stone_name, model = %model_name, "pulled model");
                        notify(state, "benchmark.pull.done", &serde_json::json!({
                            "stone": stone_name, "model": model_name,
                        })).await;
                    }
                    Err(e) => {
                        tracing::warn!(stone = %stone_name, model = %model_name, error = %e, "pull failed");
                        // Mark ALL capabilities for this model as error
                        for (m, cap) in &test_keys {
                            if m == model_name {
                                let msg = format!("pull failed: {e}");
                                set_test_error(state, stone_name, m, *cap, &msg).await;
                            }
                        }
                        persist(state).await;
                        notify(state, "benchmark.pull.error", &serde_json::json!({
                            "stone": stone_name, "model": model_name,
                            "error": format!("{e}"),
                        })).await;
                    }
                }
            }
            notify(state, "benchmark.sync.done", &serde_json::json!({
                "stone": stone_name,
            })).await;
        }
    }

    // ── Phase 2: Test — run benchmarks (all models now present) ──
    for (model_name, capability) in &test_keys {
        let model_name = model_name.as_str();
        let capability = *capability;
        if cancel.is_cancelled() {
            return None;
        }

        // Skip tests whose model failed to pull (already marked Error in phase 1)
        {
            let run = state.benchmark_run.read().await;
            if let Some(sr) = run.stones.iter().find(|s| s.stone_name == stone_name) {
                if let Some(test) = sr.tests.iter().find(|t| t.model == model_name && t.capability == capability) {
                    if test.status == TestStatus::Error {
                        continue;
                    }
                }
            }
        }

        let on_stone = available_models.iter().any(|m| m == model_name);
        if !on_stone && !sync {
            // Not on stone and not syncing → skip
            set_test_status(state, stone_name, model_name, capability, TestStatus::Skipped).await;
            continue;
        }

        // Yield to live traffic
        if !yield_to_traffic(state, endpoint, cancel).await {
            return None;
        }

        // Mark test as running
        set_test_status(state, stone_name, &model_name, capability, TestStatus::Running).await;
        let desc = format!("{model_name} ({capability}) on {stone_name}");
        notify(state, "benchmark.test.start", &serde_json::json!({
            "stone": stone_name, "model": &model_name,
            "capability": capability.to_string(), "description": &desc,
        })).await;

        // Update job progress
        {
            let run = state.benchmark_run.read().await;
            let (done, total) = run.progress();
            state
                .update_job(job_id, JobStatus::Running, Some(format!("{done}/{total}: {desc}")))
                .await;
        }

        // Unload model for cold-start measurement
        let _ = client.unload_model(endpoint, &model_name).await;
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Run the benchmark
        let result = match capability {
            Capability::Generate => {
                bench_generate(client, endpoint, stone_name, &model_name, state).await
            }
            Capability::Embed => {
                bench_embed(client, endpoint, stone_name, &model_name, state).await
            }
            Capability::Vision => {
                bench_vision(client, endpoint, stone_name, &model_name, state).await
            }
        };

        match result {
            Ok(()) => {
                // Summarise and mark done
                let summary_info = {
                    let mut run = state.benchmark_run.write().await;
                    if let Some(sr) = run.stones.iter_mut().find(|s| s.stone_name == stone_name) {
                        if let Some(test) = sr.tests.iter_mut().find(|t| t.model == model_name && t.capability == capability) {
                            test.summarise();
                            test.status = TestStatus::Done;
                            test.summary.as_ref().map(|s| (s.verdict, s.median_tps, s.cold_start_ms))
                        } else { None }
                    } else { None }
                };
                persist(state).await;
                if let Some((verdict, tps, cold)) = summary_info {
                    tracing::info!(
                        stone = %stone_name, model = %model_name,
                        mode = %capability, verdict = %verdict,
                        cold_ms = cold, tps = format!("{:.1}", tps),
                        "benchmark result"
                    );
                    notify(state, "benchmark.test.done", &serde_json::json!({
                        "stone": stone_name, "model": &model_name,
                        "capability": capability.to_string(),
                        "verdict": verdict.to_string(),
                        "tps": (tps * 10.0).round() / 10.0,
                        "cold_start_ms": cold,
                    })).await;
                }
            }
            Err(e) => {
                let msg = format!("{e:#}");
                let is_timeout = msg.contains("timed out")
                    || msg.contains("deadline has elapsed")
                    || msg.contains("operation timed out");

                if is_timeout {
                    // Timeout → record as Vetoed with synthetic summary.
                    tracing::info!(
                        stone = %stone_name, model = %model_name,
                        mode = %capability, "benchmark timed out — recording as Vetoed"
                    );
                    let summary_info = {
                        let mut run = state.benchmark_run.write().await;
                        if let Some(sr) = run.stones.iter_mut().find(|s| s.stone_name == stone_name) {
                            if let Some(test) = sr.tests.iter_mut().find(|t| t.model == model_name && t.capability == capability) {
                                test.summary = Some(TestSummary {
                                    median_tps: 0.0,
                                    cold_start_ms: 999_999,
                                    median_duration_ms: 999_999,
                                    verdict: Verdict::Vetoed,
                                });
                                test.status = TestStatus::Done;
                                test.error = Some("timed out".into());
                                Some((Verdict::Vetoed, 0.0_f64, 999_999_u64))
                            } else { None }
                        } else { None }
                    };
                    persist(state).await;
                    if let Some((verdict, tps, cold)) = summary_info {
                        notify(state, "benchmark.test.done", &serde_json::json!({
                            "stone": stone_name, "model": &model_name,
                            "capability": capability.to_string(),
                            "verdict": verdict.to_string(),
                            "tps": tps,
                            "cold_start_ms": cold,
                            "note": "timed out",
                        })).await;
                    }
                } else {
                    tracing::warn!(
                        stone = %stone_name, model = %model_name,
                        mode = %capability, error = %msg, "benchmark failed"
                    );
                    set_test_error(state, stone_name, &model_name, capability, &msg).await;
                    persist(state).await;
                    notify(state, "benchmark.test.error", &serde_json::json!({
                        "stone": stone_name, "model": &model_name,
                        "capability": capability.to_string(), "error": &msg,
                    })).await;
                }
            }
        }
    }

    tracing::info!(stone = %stone_name, "stone benchmark complete");
    None
}

// ── Individual Benchmarks ────────────────────────────────────────

async fn bench_generate(
    client: &OllamaClient,
    endpoint: &str,
    stone_name: &str,
    model: &str,
    state: &AppState,
) -> Result<()> {
    for (i, prompt) in GENERATE_PROMPTS.iter().enumerate() {
        let resp = client
            .benchmark_generate(endpoint, model, prompt, NUM_PREDICT)
            .await?;

        let cold_ms = resp.load_duration / 1_000_000;
        let tps = if resp.eval_duration > 0 {
            resp.eval_count as f64 / (resp.eval_duration as f64 / 1_000_000_000.0)
        } else {
            0.0
        };
        let total_ms = resp.total_duration / 1_000_000;

        add_sample(state, stone_name, model, Capability::Generate, Sample {
            prompt_index: i as u32,
            cold_start_ms: cold_ms,
            tokens_per_second: tps,
            total_duration_ms: total_ms,
            error: None,
        }).await;

        notify(state, "benchmark.sample", &serde_json::json!({
            "stone": stone_name, "model": model,
            "capability": "generate", "index": i,
            "of": GENERATE_PROMPTS.len(),
            "tps": (tps * 10.0).round() / 10.0,
        })).await;
    }
    Ok(())
}

async fn bench_embed(
    client: &OllamaClient,
    endpoint: &str,
    stone_name: &str,
    model: &str,
    state: &AppState,
) -> Result<()> {
    for (i, input) in EMBED_INPUTS.iter().enumerate() {
        let resp = client.benchmark_embed(endpoint, model, input).await?;

        let cold_ms = resp.load_duration / 1_000_000;
        let total_ms = resp.total_duration / 1_000_000;

        add_sample(state, stone_name, model, Capability::Embed, Sample {
            prompt_index: i as u32,
            cold_start_ms: cold_ms,
            tokens_per_second: 0.0,
            total_duration_ms: total_ms,
            error: None,
        }).await;

        notify(state, "benchmark.sample", &serde_json::json!({
            "stone": stone_name, "model": model,
            "capability": "embed", "index": i,
            "of": EMBED_INPUTS.len(),
        })).await;
    }
    Ok(())
}

async fn bench_vision(
    client: &OllamaClient,
    endpoint: &str,
    stone_name: &str,
    model: &str,
    state: &AppState,
) -> Result<()> {
    let b64_engine = base64::engine::general_purpose::STANDARD;

    for (i, (label, image_bytes)) in VISION_IMAGES.iter().enumerate() {
        let image_b64 = b64_engine.encode(image_bytes);

        tracing::debug!(model, stone_name, label, "vision benchmark image");

        let resp = client
            .benchmark_generate_vision(endpoint, model, VISION_PROMPT, &[image_b64], NUM_PREDICT)
            .await?;

        let cold_ms = resp.load_duration / 1_000_000;
        let tps = if resp.eval_duration > 0 {
            resp.eval_count as f64 / (resp.eval_duration as f64 / 1_000_000_000.0)
        } else {
            0.0
        };
        let total_ms = resp.total_duration / 1_000_000;

        add_sample(state, stone_name, model, Capability::Vision, Sample {
            prompt_index: i as u32,
            cold_start_ms: cold_ms,
            tokens_per_second: tps,
            total_duration_ms: total_ms,
            error: None,
        }).await;

        notify(state, "benchmark.sample", &serde_json::json!({
            "stone": stone_name, "model": model,
            "capability": "vision", "index": i,
            "of": VISION_IMAGES.len(),
            "tps": (tps * 10.0).round() / 10.0,
        })).await;
    }
    Ok(())
}

// ── Capability Detection ─────────────────────────────────────────

fn capabilities_to_test(model: &ModelInfo) -> Vec<Capability> {
    let mut modes = Vec::new();

    if model.capabilities.is_empty()
        || model.capabilities.iter().any(|c| c == "completion")
    {
        modes.push(Capability::Generate);
    }

    if model.capabilities.iter().any(|c| c == "embedding") {
        modes.push(Capability::Embed);
    }

    if model.capabilities.iter().any(|c| c == "vision") {
        modes.push(Capability::Vision);
    }

    if modes.is_empty() {
        modes.push(Capability::Generate);
    }

    modes
}

// ── Run Mutation Helpers ─────────────────────────────────────────

async fn add_sample(
    state: &AppState,
    stone_name: &str,
    model: &str,
    capability: Capability,
    sample: Sample,
) {
    let mut run = state.benchmark_run.write().await;
    if let Some(sr) = run.stones.iter_mut().find(|s| s.stone_name == stone_name) {
        if let Some(test) = sr.tests.iter_mut().find(|t| t.model == model && t.capability == capability) {
            test.samples.push(sample);
        }
    }
}

async fn set_test_status(
    state: &AppState,
    stone_name: &str,
    model: &str,
    capability: Capability,
    status: TestStatus,
) {
    let mut run = state.benchmark_run.write().await;
    if let Some(sr) = run.stones.iter_mut().find(|s| s.stone_name == stone_name) {
        if let Some(test) = sr.tests.iter_mut().find(|t| t.model == model && t.capability == capability) {
            test.status = status;
        }
    }
}

async fn set_test_error(
    state: &AppState,
    stone_name: &str,
    model: &str,
    capability: Capability,
    error: &str,
) {
    let mut run = state.benchmark_run.write().await;
    if let Some(sr) = run.stones.iter_mut().find(|s| s.stone_name == stone_name) {
        if let Some(test) = sr.tests.iter_mut().find(|t| t.model == model && t.capability == capability) {
            test.status = TestStatus::Error;
            test.error = Some(error.to_string());
        }
    }
}

// ── Yield to Traffic ─────────────────────────────────────────────

async fn yield_to_traffic(
    state: &AppState,
    endpoint: &str,
    cancel: &CancellationToken,
) -> bool {
    let start = std::time::Instant::now();
    loop {
        let counter = state.queue_counter(endpoint).await;
        let depth = counter.load(Ordering::Relaxed);
        if depth == 0 {
            return true;
        }
        tracing::debug!(endpoint, depth, "yielding to live traffic");
        if start.elapsed() > MAX_YIELD_WAIT {
            tracing::warn!(endpoint, "yield timeout exceeded, proceeding");
            return true;
        }
        tokio::select! {
            _ = tokio::time::sleep(YIELD_DELAY) => {}
            _ = cancel.cancelled() => return false,
        }
    }
}

// ── Notifications ────────────────────────────────────────────────

/// Emit a rich SSE event.  The dashboard picks these up for the activity
/// log and for live UI updates.
async fn notify(state: &AppState, event_type: &str, data: &serde_json::Value) {
    state.emit_event(event_type, &data.to_string()).await;
}

// ── Persistence ──────────────────────────────────────────────────

/// Write the BenchmarkRun to `{data_dir}/fitness.json`.
pub async fn persist(state: &AppState) {
    let run = state.benchmark_run.read().await;
    let path = std::path::Path::new(&state.data_dir).join("fitness.json");
    match serde_json::to_string_pretty(&*run) {
        Ok(json) => {
            if let Err(e) = tokio::fs::write(&path, json).await {
                tracing::warn!(error = %e, "failed to persist benchmark run");
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to serialize benchmark run");
        }
    }
}

/// Load BenchmarkRun from `{data_dir}/fitness.json`.
pub async fn load(data_dir: &str) -> BenchmarkRun {
    let path = std::path::Path::new(data_dir).join("fitness.json");
    match tokio::fs::read_to_string(&path).await {
        Ok(content) => match serde_json::from_str::<BenchmarkRun>(&content) {
            Ok(mut run) => {
                // Crash recovery: if a run was in progress, mark it failed
                if run.is_running() {
                    tracing::warn!("found in-progress benchmark run — marking as failed (crash recovery)");
                    run.status = RunStatus::Failed;
                    run.error = Some("interrupted by restart".into());
                    run.completed_at = Some(Utc::now());
                    // Synthesise matrix from whatever was completed
                    run.synthesise_matrix();
                }
                tracing::info!(
                    id = %run.id,
                    status = ?run.status,
                    matrix = run.gpu_matrix.entries.len(),
                    "loaded benchmark run"
                );
                run
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to parse fitness.json, starting fresh");
                BenchmarkRun::idle()
            }
        },
        Err(_) => {
            tracing::info!("no fitness.json found, starting with idle benchmark");
            BenchmarkRun::idle()
        }
    }
}

// ── Model Pull Helper ────────────────────────────────────────────

async fn pull_model_and_wait(
    client: &OllamaClient,
    endpoint: &str,
    model: &str,
) -> Result<()> {
    use futures_util::StreamExt;
    let stream = client.pull_model(endpoint, model).await?;
    tokio::pin!(stream);
    while let Some(chunk) = stream.next().await {
        let _bytes = chunk?;
    }
    Ok(())
}
