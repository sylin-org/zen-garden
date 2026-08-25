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
    (
        "simple object",
        include_bytes!("../../assets/benchmark/01-simple-object.jpg"),
    ),
    (
        "outdoor scene",
        include_bytes!("../../assets/benchmark/02-outdoor-scene.jpg"),
    ),
    (
        "text in image",
        include_bytes!("../../assets/benchmark/03-text-in-image.jpg"),
    ),
    (
        "chart or diagram",
        include_bytes!("../../assets/benchmark/04-chart-or-diagram.jpg"),
    ),
    (
        "technical diagram",
        include_bytes!("../../assets/benchmark/05-technical-diagram.jpg"),
    ),
];

const VISION_PROMPT: &str = "Describe what you see in this image in detail.";
const NUM_PREDICT: u32 = 80;
/// Sustained generation for Think benchmark (ORCH-0010).
const THINK_NUM_PREDICT: u32 = 2000;
const YIELD_DELAY: Duration = Duration::from_secs(5);
const MAX_YIELD_WAIT: Duration = Duration::from_secs(300);

// ── Think Benchmark Prompts (ORCH-0010) ─────────────────────────

const THINK_PROMPTS: &[&str] = &[
    "Solve step by step: A farmer has 3 fields of 120, 85, and 200 acres. He plants \
     wheat on 40% of each field, corn on 35%, and leaves the rest fallow. Wheat yields \
     42 bushels/acre at $6.50/bushel, corn yields 155 bushels/acre at $4.25/bushel. \
     Calculate the total revenue from each crop, the total fallow acreage, and the \
     average revenue per planted acre across all fields.",
    "Compare and contrast 5 sorting algorithms (bubble sort, merge sort, quicksort, \
     heapsort, and radix sort) in detail. For each algorithm, explain the mechanism, \
     best/worst/average time complexity, space complexity, stability, and ideal use \
     cases. Then rank them for three scenarios: nearly-sorted data, random integers, \
     and strings of varying length.",
    "Write a detailed project plan for building a community library from scratch in a \
     small town. Cover site selection, funding sources, architectural requirements, \
     construction phases, technology infrastructure, staffing plan, collection \
     development, community programs, and a 24-month timeline with milestones.",
];

// ── Tools Benchmark: Graduated Pool Sizes (ORCH-0010) ───────────

/// A single tools benchmark prompt with its pool context.
struct ToolsPrompt {
    user_message: &'static str,
    tools: serde_json::Value,
    expected_fns: Vec<&'static str>,
    pool_size: usize,
}

/// Build a JSON tool schema from name, description, and parameter specs.
fn tool_schema(name: &str, desc: &str, params: &[(&str, &str)]) -> serde_json::Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for &(pname, ptype) in params {
        properties.insert(
            pname.to_string(),
            serde_json::json!({"type": ptype, "description": pname}),
        );
        required.push(pname);
    }
    serde_json::json!({
        "type": "function",
        "function": {
            "name": name,
            "description": desc,
            "parameters": {
                "type": "object",
                "properties": properties,
                "required": required
            }
        }
    })
}

/// 100 distractor tool schemas across 10 categories.
/// None overlap with target tools (get_weather, calculate, search,
/// get_time, send_email, translate_text, get_directions).
fn distractor_pool() -> Vec<serde_json::Value> {
    // (name, description, [(param_name, param_type)])
    let defs: &[(&str, &str, &[(&str, &str)])] = &[
        // ── Communication ──────────────────────────────────
        ("post_slack_message", "Post a message to Slack", &[("channel", "string"), ("text", "string")]),
        ("send_sms", "Send an SMS message", &[("phone_number", "string"), ("message", "string")]),
        ("create_notification", "Create a push notification", &[("title", "string"), ("body", "string")]),
        ("forward_message", "Forward a message", &[("message_id", "string"), ("recipient", "string")]),
        ("read_inbox", "Read inbox messages", &[("folder", "string")]),
        ("archive_message", "Archive a message", &[("message_id", "string")]),
        ("create_channel", "Create a chat channel", &[("name", "string"), ("topic", "string")]),
        ("list_contacts", "List contacts", &[("group", "string")]),
        ("block_user", "Block a user", &[("username", "string")]),
        ("unsubscribe", "Unsubscribe from notifications", &[("list_id", "string")]),
        // ── Calendar ───────────────────────────────────────
        ("create_event", "Create a calendar event", &[("title", "string"), ("date", "string"), ("time", "string")]),
        ("list_events", "List calendar events", &[("date", "string")]),
        ("delete_event", "Delete a calendar event", &[("event_id", "string")]),
        ("update_event", "Update a calendar event", &[("event_id", "string"), ("title", "string")]),
        ("set_reminder", "Set a reminder", &[("message", "string"), ("time", "string")]),
        ("find_free_slot", "Find available time slots", &[("date", "string"), ("duration", "string")]),
        ("accept_invite", "Accept a meeting invite", &[("invite_id", "string")]),
        ("decline_invite", "Decline a meeting invite", &[("invite_id", "string")]),
        ("reschedule_meeting", "Reschedule a meeting", &[("meeting_id", "string"), ("new_time", "string")]),
        ("get_agenda", "Get today's agenda", &[("date", "string")]),
        // ── File Operations ────────────────────────────────
        ("read_file", "Read a file", &[("path", "string")]),
        ("write_file", "Write to a file", &[("path", "string"), ("content", "string")]),
        ("delete_file", "Delete a file", &[("path", "string")]),
        ("list_directory", "List directory contents", &[("path", "string")]),
        ("copy_file", "Copy a file", &[("source", "string"), ("destination", "string")]),
        ("move_file", "Move a file", &[("source", "string"), ("destination", "string")]),
        ("compress_files", "Compress files into archive", &[("files", "string"), ("output", "string")]),
        ("extract_archive", "Extract an archive", &[("archive", "string"), ("destination", "string")]),
        ("get_file_info", "Get file metadata", &[("path", "string")]),
        ("create_folder", "Create a directory", &[("path", "string")]),
        // ── Database ───────────────────────────────────────
        ("query_database", "Execute a database query", &[("query", "string"), ("database", "string")]),
        ("insert_record", "Insert a database record", &[("table", "string"), ("data", "string")]),
        ("update_record", "Update a database record", &[("table", "string"), ("id", "string"), ("data", "string")]),
        ("delete_record", "Delete a database record", &[("table", "string"), ("id", "string")]),
        ("create_table", "Create a database table", &[("name", "string"), ("schema", "string")]),
        ("drop_table", "Drop a database table", &[("name", "string")]),
        ("run_migration", "Run database migration", &[("version", "string")]),
        ("count_records", "Count records in a table", &[("table", "string")]),
        ("export_data", "Export data to file", &[("table", "string"), ("format", "string")]),
        ("import_data", "Import data from file", &[("file", "string"), ("table", "string")]),
        // ── Web & API ──────────────────────────────────────
        ("fetch_url", "Fetch content from URL", &[("url", "string")]),
        ("scrape_webpage", "Scrape a webpage", &[("url", "string"), ("selector", "string")]),
        ("check_website", "Check website status", &[("url", "string")]),
        ("download_file", "Download a file from URL", &[("url", "string"), ("destination", "string")]),
        ("upload_file", "Upload a file", &[("file", "string"), ("url", "string")]),
        ("parse_html", "Parse HTML content", &[("html", "string")]),
        ("validate_url", "Validate a URL", &[("url", "string")]),
        ("shorten_url", "Shorten a URL", &[("url", "string")]),
        ("ping_server", "Ping a server", &[("host", "string")]),
        ("check_ssl_cert", "Check SSL certificate", &[("domain", "string")]),
        // ── Finance ────────────────────────────────────────
        ("get_stock_price", "Get stock price", &[("symbol", "string")]),
        ("convert_currency", "Convert between currencies", &[("amount", "string"), ("from", "string"), ("to", "string")]),
        ("calculate_interest", "Calculate interest", &[("principal", "string"), ("rate", "string"), ("years", "string")]),
        ("track_expense", "Track an expense", &[("amount", "string"), ("category", "string")]),
        ("create_invoice", "Create an invoice", &[("client", "string"), ("amount", "string")]),
        ("process_payment", "Process a payment", &[("amount", "string"), ("method", "string")]),
        ("get_balance", "Get account balance", &[("account", "string")]),
        ("calculate_tax", "Calculate tax", &[("income", "string"), ("jurisdiction", "string")]),
        ("get_exchange_rate", "Get exchange rate", &[("from", "string"), ("to", "string")]),
        ("generate_report", "Generate financial report", &[("period", "string"), ("type", "string")]),
        // ── System & DevOps ────────────────────────────────
        ("get_system_info", "Get system information", &[("component", "string")]),
        ("check_disk_space", "Check disk space", &[("path", "string")]),
        ("list_processes", "List running processes", &[("filter", "string")]),
        ("kill_process", "Kill a process", &[("pid", "string")]),
        ("restart_service", "Restart a service", &[("service", "string")]),
        ("check_memory", "Check memory usage", &[("unit", "string")]),
        ("get_cpu_usage", "Get CPU usage", &[("interval", "string")]),
        ("clear_cache", "Clear system cache", &[("type", "string")]),
        ("rotate_logs", "Rotate log files", &[("service", "string")]),
        ("run_healthcheck", "Run health check", &[("service", "string")]),
        // ── Smart Home ─────────────────────────────────────
        ("set_thermostat", "Set thermostat temperature", &[("temperature", "string"), ("zone", "string")]),
        ("toggle_lights", "Toggle lights on/off", &[("room", "string"), ("state", "string")]),
        ("lock_door", "Lock or unlock a door", &[("door", "string"), ("action", "string")]),
        ("set_alarm", "Set an alarm", &[("time", "string"), ("label", "string")]),
        ("check_camera", "Check security camera", &[("camera_id", "string")]),
        ("play_music", "Play music", &[("song", "string"), ("room", "string")]),
        ("set_timer", "Set a countdown timer", &[("duration", "string"), ("label", "string")]),
        ("adjust_volume", "Adjust speaker volume", &[("level", "string"), ("room", "string")]),
        ("water_plants", "Water the plants", &[("zone", "string"), ("duration", "string")]),
        ("feed_pet", "Dispense pet food", &[("pet", "string"), ("portion", "string")]),
        // ── Development ────────────────────────────────────
        ("run_tests", "Run test suite", &[("path", "string"), ("filter", "string")]),
        ("format_code", "Format source code", &[("file", "string"), ("style", "string")]),
        ("lint_code", "Lint source code", &[("file", "string")]),
        ("deploy_app", "Deploy application", &[("environment", "string"), ("version", "string")]),
        ("create_branch", "Create a git branch", &[("name", "string"), ("base", "string")]),
        ("review_code", "Review code changes", &[("pr_id", "string")]),
        ("generate_docs", "Generate documentation", &[("source", "string"), ("output", "string")]),
        ("profile_performance", "Profile code performance", &[("target", "string"), ("duration", "string")]),
        ("run_benchmark_suite", "Run performance benchmarks", &[("suite", "string")]),
        ("analyze_logs", "Analyze log files", &[("file", "string"), ("pattern", "string")]),
        // ── Data & AI ──────────────────────────────────────
        ("classify_text", "Classify text into categories", &[("text", "string"), ("categories", "string")]),
        ("sentiment_analysis", "Analyze text sentiment", &[("text", "string")]),
        ("extract_entities", "Extract named entities", &[("text", "string")]),
        ("summarize_document", "Summarize a document", &[("text", "string"), ("max_length", "string")]),
        ("generate_image", "Generate an image", &[("prompt", "string"), ("size", "string")]),
        ("resize_image", "Resize an image", &[("path", "string"), ("width", "string"), ("height", "string")]),
        ("detect_language", "Detect text language", &[("text", "string")]),
        ("spell_check", "Check spelling", &[("text", "string")]),
        ("convert_format", "Convert file format", &[("input", "string"), ("output_format", "string")]),
        ("recognize_speech", "Recognize speech from audio", &[("audio_file", "string")]),
    ];

    defs.iter()
        .map(|(name, desc, params)| tool_schema(name, desc, params))
        .collect()
}

/// Target tool definitions used in benchmark prompts.
fn target_tools() -> Vec<(&'static str, &'static str, Vec<(&'static str, &'static str)>)> {
    vec![
        ("get_weather", "Get current weather for a city", vec![("city", "string")]),
        ("calculate", "Evaluate a math expression", vec![("expression", "string")]),
        ("search", "Search for documents", vec![("query", "string"), ("limit", "integer")]),
        ("get_time", "Get current time in a city", vec![("city", "string")]),
        ("send_email", "Send an email message", vec![("to", "string"), ("subject", "string"), ("body", "string")]),
        ("translate_text", "Translate text to another language", vec![("text", "string"), ("target_language", "string")]),
        ("get_directions", "Get directions between two locations", vec![("origin", "string"), ("destination", "string")]),
    ]
}

/// Build a tool array: the target tool(s) embedded in a pool of distractors.
fn build_tool_pool(
    target_names: &[&str],
    pool_size: usize,
    targets: &[(&str, &str, Vec<(&str, &str)>)],
    distractors: &[serde_json::Value],
) -> serde_json::Value {
    let mut tools: Vec<serde_json::Value> = Vec::with_capacity(pool_size);

    // Add target tools
    for name in target_names {
        if let Some((n, desc, params)) = targets.iter().find(|(n, _, _)| n == name) {
            tools.push(tool_schema(n, desc, &params));
        }
    }

    // Fill remaining slots with distractors
    let needed = pool_size.saturating_sub(tools.len());
    for d in distractors.iter().take(needed) {
        tools.push(d.clone());
    }

    serde_json::Value::Array(tools)
}

/// Build the graduated tools benchmark prompt suite.
///
/// Pool sizes: 1, 1, 3, 3, 5, 10, 25, 50, 100
/// Tests precision at increasing distractor density.
fn build_tools_prompts() -> Vec<ToolsPrompt> {
    let targets = target_tools();
    let distractors = distractor_pool();

    vec![
        // ── Tier 1: Pool=1 — Baseline (no distractors) ────
        ToolsPrompt {
            user_message: "What's the weather in Tokyo?",
            tools: build_tool_pool(&["get_weather"], 1, &targets, &distractors),
            expected_fns: vec!["get_weather"],
            pool_size: 1,
        },
        ToolsPrompt {
            user_message: "Calculate 15% tip on $84.50",
            tools: build_tool_pool(&["calculate"], 1, &targets, &distractors),
            expected_fns: vec!["calculate"],
            pool_size: 1,
        },
        // ── Tier 2: Pool=3 — Basic resolution ─────────────
        ToolsPrompt {
            user_message: "Find recent papers on transformers, limit 3",
            tools: build_tool_pool(&["search"], 3, &targets, &distractors),
            expected_fns: vec!["search"],
            pool_size: 3,
        },
        ToolsPrompt {
            user_message: "What time is it in London and Tokyo?",
            tools: build_tool_pool(&["get_time"], 3, &targets, &distractors),
            expected_fns: vec!["get_time", "get_time"],
            pool_size: 3,
        },
        // ── Tier 3: Pool=5 — Multi-function resolution ────
        ToolsPrompt {
            user_message: "Search for 'rust async' and get weather in Berlin",
            tools: build_tool_pool(&["search", "get_weather"], 5, &targets, &distractors),
            expected_fns: vec!["search", "get_weather"],
            pool_size: 5,
        },
        // ── Tier 4: Pool=10 — Moderate noise ──────────────
        ToolsPrompt {
            user_message: "Send an email to alice@example.com with subject 'Project Update' and body 'The release is on track.'",
            tools: build_tool_pool(&["send_email"], 10, &targets, &distractors),
            expected_fns: vec!["send_email"],
            pool_size: 10,
        },
        // ── Tier 5: Pool=25 — High noise ──────────────────
        ToolsPrompt {
            user_message: "What's the weather like in Paris right now?",
            tools: build_tool_pool(&["get_weather"], 25, &targets, &distractors),
            expected_fns: vec!["get_weather"],
            pool_size: 25,
        },
        // ── Tier 6: Pool=50 — Stress test ─────────────────
        ToolsPrompt {
            user_message: "Translate 'hello world' to French",
            tools: build_tool_pool(&["translate_text"], 50, &targets, &distractors),
            expected_fns: vec!["translate_text"],
            pool_size: 50,
        },
        // ── Tier 7: Pool=100 — Full stress test ───────────
        ToolsPrompt {
            user_message: "Get directions from New York to Boston",
            tools: build_tool_pool(&["get_directions"], 100, &targets, &distractors),
            expected_fns: vec!["get_directions"],
            pool_size: 100,
        },
    ]
}

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

    notify(
        &state,
        "benchmark.started",
        &serde_json::json!({
            "id": &run_id, "scope": &scope_label, "sync": sync
        }),
    )
    .await;

    // ── Step 1: Apply wipe (on the previous run's gpu_matrix) ────
    if let Some(ref wipe_scope) = wipe {
        tracing::info!(?wipe_scope, "wiping previous results");
        notify(
            &state,
            "benchmark.wipe",
            &serde_json::json!({
                "scope": format!("{wipe_scope:?}")
            }),
        )
        .await;
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
            .map(|i| {
                (
                    i.endpoint.clone(),
                    i.stone_name.clone(),
                    i.gpu_name.clone().unwrap_or_else(|| "Unknown GPU".into()),
                    i.vram_total_bytes / 1_048_576,
                    i.models_available.clone(),
                )
            })
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
        state
            .fail_job(&job_id, "no healthy stones matched scope")
            .await;
        notify(
            &state,
            "benchmark.failed",
            &serde_json::json!({
                "error": "No healthy stones matched scope"
            }),
        )
        .await;
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
                if vram_bytes == 0 || model_info.size_disk == 0 || model_info.size_disk > vram_bytes
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
        notify(
            &state,
            "benchmark.planned",
            &serde_json::json!({
                "total": total, "completed": completed,
                "stones": stone_names,
            }),
        )
        .await;
    }

    // ── Step 4: Per-stone parallel execution ───────────────────
    // One tokio task per stone.  Each stone writes only to its own
    // StoneReport so there is no cross-stone contention.  Within a
    // single stone, tests run sequentially (one GPU at a time).
    let stone_count = targets.len();
    let mut handles = Vec::with_capacity(stone_count);

    for (stone_idx, (endpoint, stone_name, _gpu, _vram, available)) in
        targets.into_iter().enumerate()
    {
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
            notify(
                &state,
                "benchmark.stone.start",
                &serde_json::json!({
                    "stone": &stone_name,
                    "index": stone_idx,
                    "of": stone_count,
                }),
            )
            .await;

            let stone_err = run_stone(
                &state,
                &client,
                &endpoint,
                &stone_name,
                &available,
                &all_models,
                sync,
                &cancel,
                &job_id,
            )
            .await;

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

            notify(
                &state,
                "benchmark.stone.done",
                &serde_json::json!({
                    "stone": &stone_name,
                    "status": if cancel.is_cancelled() { "cancelled" }
                              else if stone_err.is_some() { "error" }
                              else { "done" },
                }),
            )
            .await;
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
    state.refresh_recommendations().await;

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
            .update_job(
                &job_id,
                JobStatus::Running,
                Some(format!("{matrix_count} results")),
            )
            .await;
        state.complete_job(&job_id).await;
        notify(
            &state,
            "benchmark.completed",
            &serde_json::json!({
                "results": matrix_count, "completed": done, "total": total,
            }),
        )
        .await;
    }
}

// ── Per-Stone Runner ─────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
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
        sr.tests
            .iter()
            .map(|t| (t.model.clone(), t.capability))
            .collect()
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
            notify(
                state,
                "benchmark.sync.start",
                &serde_json::json!({
                    "stone": stone_name,
                    "models": &models_to_pull,
                }),
            )
            .await;

            for model_name in &models_to_pull {
                if cancel.is_cancelled() {
                    return None;
                }
                notify(
                    state,
                    "benchmark.pull",
                    &serde_json::json!({
                        "stone": stone_name, "model": model_name,
                    }),
                )
                .await;
                match pull_model_and_wait(client, endpoint, model_name).await {
                    Ok(()) => {
                        // Refresh the instance registry so the rest of the
                        // system (routing, recommendations) sees the new model.
                        if let Ok((avail, loaded, infos, _)) =
                            client.full_profile(endpoint).await
                        {
                            state
                                .update_instance_models(endpoint, avail, loaded)
                                .await;
                            for info in infos {
                                state.upsert_model(info).await;
                            }
                        }
                        tracing::info!(stone = %stone_name, model = %model_name, "pulled model");
                        notify(
                            state,
                            "benchmark.pull.done",
                            &serde_json::json!({
                                "stone": stone_name, "model": model_name,
                            }),
                        )
                        .await;
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
                        notify(
                            state,
                            "benchmark.pull.error",
                            &serde_json::json!({
                                "stone": stone_name, "model": model_name,
                                "error": format!("{e}"),
                            }),
                        )
                        .await;
                    }
                }
            }
            notify(
                state,
                "benchmark.sync.done",
                &serde_json::json!({
                    "stone": stone_name,
                }),
            )
            .await;
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
                if let Some(test) = sr
                    .tests
                    .iter()
                    .find(|t| t.model == model_name && t.capability == capability)
                {
                    if test.status == TestStatus::Error {
                        continue;
                    }
                }
            }
        }

        let on_stone = available_models.iter().any(|m| m == model_name);
        if !on_stone && !sync {
            // Not on stone and not syncing → skip
            set_test_status(
                state,
                stone_name,
                model_name,
                capability,
                TestStatus::Skipped,
            )
            .await;
            continue;
        }

        // Yield to live traffic
        if !yield_to_traffic(state, endpoint, cancel).await {
            return None;
        }

        // Mark test as running
        set_test_status(
            state,
            stone_name,
            model_name,
            capability,
            TestStatus::Running,
        )
        .await;
        let desc = format!("{model_name} ({capability}) on {stone_name}");
        notify(
            state,
            "benchmark.test.start",
            &serde_json::json!({
                "stone": stone_name, "model": &model_name,
                "capability": capability.to_string(), "description": &desc,
            }),
        )
        .await;

        // Update job progress
        {
            let run = state.benchmark_run.read().await;
            let (done, total) = run.progress();
            state
                .update_job(
                    job_id,
                    JobStatus::Running,
                    Some(format!("{done}/{total}: {desc}")),
                )
                .await;
        }

        // Unload model for cold-start measurement
        let _ = client.unload_model(endpoint, model_name).await;
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Run the benchmark
        let result = match capability {
            Capability::Generate => {
                bench_generate(client, endpoint, stone_name, model_name, state).await
            }
            Capability::Embed => bench_embed(client, endpoint, stone_name, model_name, state).await,
            Capability::Vision => {
                bench_vision(client, endpoint, stone_name, model_name, state).await
            }
            Capability::Tools => {
                bench_tools(client, endpoint, stone_name, model_name, state).await
            }
            Capability::Think => {
                bench_think(client, endpoint, stone_name, model_name, state).await
            }
        };

        match result {
            Ok(()) => {
                // Summarise and mark done
                let summary_info = {
                    let mut run = state.benchmark_run.write().await;
                    if let Some(sr) = run.stones.iter_mut().find(|s| s.stone_name == stone_name) {
                        if let Some(test) = sr
                            .tests
                            .iter_mut()
                            .find(|t| t.model == model_name && t.capability == capability)
                        {
                            // Tools has its own verdict logic (override_tools_verdict)
                            // that already set the summary — don't overwrite it.
                            if test.summary.is_none() {
                                test.summarise();
                            }
                            test.status = TestStatus::Done;
                            test.summary
                                .as_ref()
                                .map(|s| (s.verdict, s.median_tps, s.cold_start_ms, s.valid_ratio))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };
                persist(state).await;
                if let Some((verdict, tps, cold, valid_ratio)) = summary_info {
                    tracing::info!(
                        stone = %stone_name, model = %model_name,
                        mode = %capability, verdict = %verdict,
                        cold_ms = cold, tps = format!("{:.1}", tps),
                        "benchmark result"
                    );
                    let mut event = serde_json::json!({
                        "stone": stone_name, "model": &model_name,
                        "capability": capability.to_string(),
                        "verdict": verdict.to_string(),
                        "tps": (tps * 10.0).round() / 10.0,
                        "cold_start_ms": cold,
                    });
                    if let Some(vr) = valid_ratio {
                        event["valid_ratio"] = serde_json::json!(vr);
                    }
                    notify(state, "benchmark.test.done", &event).await;
                }
            }
            Err(e) => {
                let msg = format!("{e:#}");
                let is_timeout = msg.contains("timed out")
                    || msg.contains("deadline has elapsed")
                    || msg.contains("operation timed out");

                // Ollama reports resource exhaustion as HTTP 500 with
                // messages like "model requires more system memory …" or
                // "model failed to load … resource limitations".  These
                // are hard constraints — the model physically cannot run
                // on this stone — so we record Blocked (not Vetoed).
                let is_resource_limit = msg.contains("requires more system memory")
                    || msg.contains("requires more memory")
                    || msg.contains("out of memory")
                    || (msg.contains("resource limitation") || msg.contains("failed to load"));

                if is_timeout {
                    // Timeout → record as Blocked.  A model that cannot
                    // finish within the benchmark window is unusable; the
                    // router must not route here.
                    tracing::info!(
                        stone = %stone_name, model = %model_name,
                        mode = %capability, "benchmark timed out — recording as Blocked"
                    );
                    record_synthetic_verdict(
                        state, stone_name, model_name, capability,
                        Verdict::Blocked, "timed out",
                    ).await;
                } else if is_resource_limit {
                    // Resource exhaustion → record as Blocked.
                    tracing::info!(
                        stone = %stone_name, model = %model_name,
                        mode = %capability, error = %msg,
                        "model cannot load on this stone — recording as Blocked"
                    );
                    record_synthetic_verdict(
                        state, stone_name, model_name, capability,
                        Verdict::Blocked, &msg,
                    ).await;
                } else {
                    // Any other benchmark error — the model failed to
                    // produce output on this stone.  Record as Blocked so
                    // the verdict appears in the GPU matrix and the router
                    // steers traffic away.
                    tracing::warn!(
                        stone = %stone_name, model = %model_name,
                        mode = %capability, error = %msg,
                        "benchmark failed — recording as Blocked"
                    );
                    record_synthetic_verdict(
                        state, stone_name, model_name, capability,
                        Verdict::Blocked, &msg,
                    ).await;
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

        add_sample(
            state,
            stone_name,
            model,
            Capability::Generate,
            Sample {
                prompt_index: i as u32,
                cold_start_ms: cold_ms,
                tokens_per_second: tps,
                total_duration_ms: total_ms,
                error: None,
            },
        )
        .await;

        notify(
            state,
            "benchmark.sample",
            &serde_json::json!({
                "stone": stone_name, "model": model,
                "capability": "generate", "index": i,
                "of": GENERATE_PROMPTS.len(),
                "tps": (tps * 10.0).round() / 10.0,
            }),
        )
        .await;
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

        add_sample(
            state,
            stone_name,
            model,
            Capability::Embed,
            Sample {
                prompt_index: i as u32,
                cold_start_ms: cold_ms,
                tokens_per_second: 0.0,
                total_duration_ms: total_ms,
                error: None,
            },
        )
        .await;

        notify(
            state,
            "benchmark.sample",
            &serde_json::json!({
                "stone": stone_name, "model": model,
                "capability": "embed", "index": i,
                "of": EMBED_INPUTS.len(),
            }),
        )
        .await;
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

        add_sample(
            state,
            stone_name,
            model,
            Capability::Vision,
            Sample {
                prompt_index: i as u32,
                cold_start_ms: cold_ms,
                tokens_per_second: tps,
                total_duration_ms: total_ms,
                error: None,
            },
        )
        .await;

        notify(
            state,
            "benchmark.sample",
            &serde_json::json!({
                "stone": stone_name, "model": model,
                "capability": "vision", "index": i,
                "of": VISION_IMAGES.len(),
                "tps": (tps * 10.0).round() / 10.0,
            }),
        )
        .await;
    }
    Ok(())
}

// ── Tools Benchmark (ORCH-0010) ─────────────────────────────────

async fn bench_tools(
    client: &OllamaClient,
    endpoint: &str,
    stone_name: &str,
    model: &str,
    state: &AppState,
) -> Result<()> {
    let prompts = build_tools_prompts();
    let mut valid_count: u32 = 0;
    let total_prompts = prompts.len() as u32;

    for (i, prompt) in prompts.iter().enumerate() {
        let resp = client
            .benchmark_chat_tools(endpoint, model, prompt.user_message, &prompt.tools)
            .await?;

        // Extract timing from the chat response
        let cold_ms = resp
            .get("load_duration")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            / 1_000_000;
        let eval_count = resp
            .get("eval_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let eval_dur = resp
            .get("eval_duration")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let total_ms = resp
            .get("total_duration")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            / 1_000_000;
        let tps = if eval_dur > 0 {
            eval_count as f64 / (eval_dur as f64 / 1_000_000_000.0)
        } else {
            0.0
        };

        // Validate tool calls in the response
        let is_valid = validate_tool_calls(&resp, &prompt.expected_fns);
        if is_valid {
            valid_count += 1;
        }

        add_sample(
            state,
            stone_name,
            model,
            Capability::Tools,
            Sample {
                prompt_index: i as u32,
                cold_start_ms: cold_ms,
                tokens_per_second: tps,
                total_duration_ms: total_ms,
                error: if is_valid {
                    None
                } else {
                    Some(format!("invalid tool call (pool={})", prompt.pool_size))
                },
            },
        )
        .await;

        notify(
            state,
            "benchmark.sample",
            &serde_json::json!({
                "stone": stone_name, "model": model,
                "capability": "tools", "index": i,
                "of": total_prompts,
                "pool_size": prompt.pool_size,
                "valid": is_valid,
                "tps": (tps * 10.0).round() / 10.0,
            }),
        )
        .await;
    }

    // Override the summarise() verdict with tools-specific correctness logic.
    // We do this after all samples are recorded.
    override_tools_verdict(state, stone_name, model, valid_count, total_prompts).await;

    Ok(())
}

/// Validate that the chat response contains valid tool calls matching expected function names.
fn validate_tool_calls(resp: &serde_json::Value, expected_fns: &[&str]) -> bool {
    // Ollama returns tool calls in message.tool_calls
    let tool_calls = resp
        .get("message")
        .and_then(|m| m.get("tool_calls"))
        .and_then(|tc| tc.as_array());

    let calls = match tool_calls {
        Some(arr) if !arr.is_empty() => arr,
        _ => return false,
    };

    // Check that at least one expected function name appears in the tool calls.
    // We validate: (1) function name exists, (2) it matches an expected name,
    // (3) arguments parse as a JSON object.
    let mut matched_fns: Vec<bool> = vec![false; expected_fns.len()];

    for call in calls {
        let fn_name = call
            .get("function")
            .and_then(|f| f.get("name"))
            .and_then(|n| n.as_str());

        let has_args = call
            .get("function")
            .and_then(|f| f.get("arguments"))
            .map(|a| a.is_object())
            .unwrap_or(false);

        if let Some(name) = fn_name {
            if has_args {
                // Mark the first unmatched expected function with this name
                for (idx, expected) in expected_fns.iter().enumerate() {
                    if !matched_fns[idx] && *expected == name {
                        matched_fns[idx] = true;
                        break;
                    }
                }
            }
        }
    }

    // All expected functions must have been matched
    matched_fns.iter().all(|&m| m)
}

/// Override the standard summarise() verdict for tools with correctness-aware logic.
async fn override_tools_verdict(
    state: &AppState,
    stone_name: &str,
    model: &str,
    valid_count: u32,
    total_prompts: u32,
) {
    let mut run = state.benchmark_run.write().await;
    if let Some(sr) = run.stones.iter_mut().find(|s| s.stone_name == stone_name) {
        if let Some(test) = sr
            .tests
            .iter_mut()
            .find(|t| t.model == model && t.capability == Capability::Tools)
        {
            // Compute median tps and cold start from successful samples
            let ok_samples: Vec<&Sample> = test.samples.iter().filter(|s| s.error.is_none()).collect();
            let median_tps = if ok_samples.is_empty() {
                0.0
            } else {
                super::super::domain::fitness::median_f64(
                    &ok_samples.iter().map(|s| s.tokens_per_second).collect::<Vec<_>>(),
                )
            };
            let cold_start_ms = ok_samples.first().map(|s| s.cold_start_ms).unwrap_or(0);
            let median_duration_ms = if ok_samples.is_empty() {
                0
            } else {
                super::super::domain::fitness::median_u64(
                    &ok_samples.iter().map(|s| s.total_duration_ms).collect::<Vec<_>>(),
                )
            };

            let ratio = if total_prompts > 0 {
                valid_count as f64 / total_prompts as f64
            } else {
                0.0
            };
            let verdict = Verdict::compute_tools(valid_count, total_prompts, cold_start_ms, median_tps);
            test.summary = Some(TestSummary {
                median_tps,
                cold_start_ms,
                median_duration_ms,
                verdict,
                valid_ratio: Some(ratio),
            });
        }
    }
}

// ── Think Benchmark (ORCH-0010) ─────────────────────────────────

async fn bench_think(
    client: &OllamaClient,
    endpoint: &str,
    stone_name: &str,
    model: &str,
    state: &AppState,
) -> Result<()> {
    for (i, prompt) in THINK_PROMPTS.iter().enumerate() {
        let resp = client
            .benchmark_generate_long(endpoint, model, prompt, THINK_NUM_PREDICT)
            .await?;

        let cold_ms = resp.load_duration / 1_000_000;
        let tps = if resp.eval_duration > 0 {
            resp.eval_count as f64 / (resp.eval_duration as f64 / 1_000_000_000.0)
        } else {
            0.0
        };
        let total_ms = resp.total_duration / 1_000_000;

        add_sample(
            state,
            stone_name,
            model,
            Capability::Think,
            Sample {
                prompt_index: i as u32,
                cold_start_ms: cold_ms,
                tokens_per_second: tps,
                total_duration_ms: total_ms,
                error: None,
            },
        )
        .await;

        notify(
            state,
            "benchmark.sample",
            &serde_json::json!({
                "stone": stone_name, "model": model,
                "capability": "think", "index": i,
                "of": THINK_PROMPTS.len(),
                "tps": (tps * 10.0).round() / 10.0,
            }),
        )
        .await;
    }
    Ok(())
}

// ── Capability Detection ─────────────────────────────────────────

fn capabilities_to_test(model: &ModelInfo) -> Vec<Capability> {
    let mut modes = Vec::new();

    if model.capabilities.is_empty() || model.capabilities.iter().any(|c| c == "completion") {
        modes.push(Capability::Generate);
    }

    if model.capabilities.iter().any(|c| c == "embedding") {
        modes.push(Capability::Embed);
    }

    if model.capabilities.iter().any(|c| c == "vision") {
        modes.push(Capability::Vision);
    }

    if model.capabilities.iter().any(|c| c == "tools") {
        modes.push(Capability::Tools);
    }

    if model.capabilities.iter().any(|c| c == "thinking") {
        modes.push(Capability::Think);
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
        if let Some(test) = sr
            .tests
            .iter_mut()
            .find(|t| t.model == model && t.capability == capability)
        {
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
        if let Some(test) = sr
            .tests
            .iter_mut()
            .find(|t| t.model == model && t.capability == capability)
        {
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
        if let Some(test) = sr
            .tests
            .iter_mut()
            .find(|t| t.model == model && t.capability == capability)
        {
            test.status = TestStatus::Error;
            test.error = Some(error.to_string());
        }
    }
}

/// Record a synthetic verdict (Vetoed, Blocked, etc.) when the actual
/// benchmark cannot complete — e.g. timeout, OOM, or resource exhaustion.
///
/// Creates a `TestSummary` with sentinel values so the GPU matrix still
/// gets an entry for this (model, stone) pair, and the router knows not
/// to route traffic there.
async fn record_synthetic_verdict(
    state: &AppState,
    stone_name: &str,
    model_name: &str,
    capability: Capability,
    verdict: Verdict,
    note: &str,
) {
    let summary_info = {
        let mut run = state.benchmark_run.write().await;
        if let Some(sr) = run.stones.iter_mut().find(|s| s.stone_name == stone_name) {
            if let Some(test) = sr
                .tests
                .iter_mut()
                .find(|t| t.model == model_name && t.capability == capability)
            {
                test.summary = Some(TestSummary {
                    median_tps: 0.0,
                    cold_start_ms: 999_999,
                    median_duration_ms: 999_999,
                    verdict,
                    valid_ratio: None,
                });
                test.status = TestStatus::Done;
                test.error = Some(note.to_string());
                Some((verdict, 0.0_f64, 999_999_u64))
            } else {
                None
            }
        } else {
            None
        }
    };
    persist(state).await;
    if let Some((v, tps, cold)) = summary_info {
        notify(
            state,
            "benchmark.test.done",
            &serde_json::json!({
                "stone": stone_name, "model": model_name,
                "capability": capability.to_string(),
                "verdict": v.to_string(),
                "tps": tps,
                "cold_start_ms": cold,
                "note": note,
            }),
        )
        .await;
    }
}

// ── Yield to Traffic ─────────────────────────────────────────────

async fn yield_to_traffic(state: &AppState, endpoint: &str, cancel: &CancellationToken) -> bool {
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
                    tracing::warn!(
                        "found in-progress benchmark run — marking as failed (crash recovery)"
                    );
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

async fn pull_model_and_wait(client: &OllamaClient, endpoint: &str, model: &str) -> Result<()> {
    use futures_util::StreamExt;
    let stream = client.pull_model(endpoint, model).await?;
    tokio::pin!(stream);
    while let Some(chunk) = stream.next().await {
        let _bytes = chunk?;
    }
    Ok(())
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::types::ModelInfo;
    use serde_json::json;

    fn make_model_info(caps: &[&str]) -> ModelInfo {
        ModelInfo {
            name: "test:latest".into(),
            parameter_count: None,
            parameter_size: Some("7B".into()),
            quantization_level: None,
            family: None,
            families: vec![],
            capabilities: caps.iter().map(|s| s.to_string()).collect(),
            format: None,
            size_disk: 4_000_000_000,
            vram_bytes: None,
            context_length: None,
        }
    }

    // ── validate_tool_calls ─────────────────────────────────────

    #[test]
    fn valid_single_tool_call() {
        let resp = json!({
            "message": {
                "tool_calls": [{
                    "function": {
                        "name": "get_weather",
                        "arguments": {"city": "Tokyo"}
                    }
                }]
            }
        });
        assert!(validate_tool_calls(&resp, &["get_weather"]));
    }

    #[test]
    fn valid_multiple_tool_calls_different_functions() {
        let resp = json!({
            "message": {
                "tool_calls": [
                    {"function": {"name": "search", "arguments": {"query": "rust"}}},
                    {"function": {"name": "get_weather", "arguments": {"city": "Berlin"}}}
                ]
            }
        });
        assert!(validate_tool_calls(&resp, &["search", "get_weather"]));
    }

    #[test]
    fn valid_multiple_tool_calls_same_function() {
        let resp = json!({
            "message": {
                "tool_calls": [
                    {"function": {"name": "get_time", "arguments": {"city": "London"}}},
                    {"function": {"name": "get_time", "arguments": {"city": "Tokyo"}}}
                ]
            }
        });
        assert!(validate_tool_calls(&resp, &["get_time", "get_time"]));
    }

    #[test]
    fn missing_tool_calls_field() {
        let resp = json!({"message": {"content": "I can't do that"}});
        assert!(!validate_tool_calls(&resp, &["get_weather"]));
    }

    #[test]
    fn empty_tool_calls_array() {
        let resp = json!({"message": {"tool_calls": []}});
        assert!(!validate_tool_calls(&resp, &["get_weather"]));
    }

    #[test]
    fn wrong_function_name() {
        let resp = json!({
            "message": {
                "tool_calls": [{
                    "function": {"name": "get_time", "arguments": {"city": "Tokyo"}}
                }]
            }
        });
        assert!(!validate_tool_calls(&resp, &["get_weather"]));
    }

    #[test]
    fn missing_arguments_field() {
        let resp = json!({
            "message": {
                "tool_calls": [{
                    "function": {"name": "get_weather"}
                }]
            }
        });
        assert!(!validate_tool_calls(&resp, &["get_weather"]));
    }

    #[test]
    fn arguments_not_an_object() {
        let resp = json!({
            "message": {
                "tool_calls": [{
                    "function": {"name": "get_weather", "arguments": "not an object"}
                }]
            }
        });
        assert!(!validate_tool_calls(&resp, &["get_weather"]));
    }

    #[test]
    fn arguments_is_array_not_object() {
        let resp = json!({
            "message": {
                "tool_calls": [{
                    "function": {"name": "get_weather", "arguments": ["Tokyo"]}
                }]
            }
        });
        assert!(!validate_tool_calls(&resp, &["get_weather"]));
    }

    #[test]
    fn extra_tool_calls_beyond_expected_still_valid() {
        // Model calls more tools than expected — OK as long as expected ones are present
        let resp = json!({
            "message": {
                "tool_calls": [
                    {"function": {"name": "get_weather", "arguments": {"city": "Tokyo"}}},
                    {"function": {"name": "get_time", "arguments": {"city": "Tokyo"}}}
                ]
            }
        });
        assert!(validate_tool_calls(&resp, &["get_weather"]));
    }

    #[test]
    fn missing_one_of_two_expected_functions() {
        let resp = json!({
            "message": {
                "tool_calls": [
                    {"function": {"name": "search", "arguments": {"query": "rust"}}}
                ]
            }
        });
        // Expected both search and get_weather, only got search
        assert!(!validate_tool_calls(&resp, &["search", "get_weather"]));
    }

    #[test]
    fn duplicate_expected_but_only_one_actual_call() {
        let resp = json!({
            "message": {
                "tool_calls": [
                    {"function": {"name": "get_time", "arguments": {"city": "London"}}}
                ]
            }
        });
        // Expected 2 get_time calls, only got 1
        assert!(!validate_tool_calls(&resp, &["get_time", "get_time"]));
    }

    #[test]
    fn missing_function_name_field() {
        let resp = json!({
            "message": {
                "tool_calls": [{
                    "function": {"arguments": {"city": "Tokyo"}}
                }]
            }
        });
        assert!(!validate_tool_calls(&resp, &["get_weather"]));
    }

    #[test]
    fn null_arguments_treated_as_missing() {
        let resp = json!({
            "message": {
                "tool_calls": [{
                    "function": {"name": "get_weather", "arguments": null}
                }]
            }
        });
        assert!(!validate_tool_calls(&resp, &["get_weather"]));
    }

    #[test]
    fn no_message_field_at_all() {
        let resp = json!({"model": "llama3:8b", "done": true});
        assert!(!validate_tool_calls(&resp, &["get_weather"]));
    }

    // ── capabilities_to_test ────────────────────────────────────

    #[test]
    fn caps_completion_only_gets_generate() {
        let model = make_model_info(&["completion"]);
        let caps = capabilities_to_test(&model);
        assert_eq!(caps, vec![Capability::Generate]);
    }

    #[test]
    fn caps_tools_tag_adds_tools_capability() {
        let model = make_model_info(&["completion", "tools"]);
        let caps = capabilities_to_test(&model);
        assert!(caps.contains(&Capability::Generate));
        assert!(caps.contains(&Capability::Tools));
        assert_eq!(caps.len(), 2);
    }

    #[test]
    fn caps_thinking_tag_adds_think_capability() {
        let model = make_model_info(&["completion", "thinking"]);
        let caps = capabilities_to_test(&model);
        assert!(caps.contains(&Capability::Generate));
        assert!(caps.contains(&Capability::Think));
        assert_eq!(caps.len(), 2);
    }

    #[test]
    fn caps_all_five_capabilities() {
        let model = make_model_info(&["completion", "embedding", "vision", "tools", "thinking"]);
        let caps = capabilities_to_test(&model);
        assert!(caps.contains(&Capability::Generate));
        assert!(caps.contains(&Capability::Embed));
        assert!(caps.contains(&Capability::Vision));
        assert!(caps.contains(&Capability::Tools));
        assert!(caps.contains(&Capability::Think));
        assert_eq!(caps.len(), 5);
    }

    #[test]
    fn caps_empty_defaults_to_generate() {
        let model = make_model_info(&[]);
        let caps = capabilities_to_test(&model);
        assert_eq!(caps, vec![Capability::Generate]);
    }

    #[test]
    fn caps_embedding_only_excludes_generate() {
        let model = make_model_info(&["embedding"]);
        let caps = capabilities_to_test(&model);
        assert_eq!(caps, vec![Capability::Embed]);
    }

    #[test]
    fn caps_vision_model_gets_generate_and_vision() {
        let model = make_model_info(&["completion", "vision"]);
        let caps = capabilities_to_test(&model);
        assert!(caps.contains(&Capability::Generate));
        assert!(caps.contains(&Capability::Vision));
        assert_eq!(caps.len(), 2);
    }

    #[test]
    fn caps_tools_without_completion_still_no_generate() {
        // A model with only "tools" tag — no "completion" means no Generate
        let model = make_model_info(&["tools"]);
        let caps = capabilities_to_test(&model);
        assert!(caps.contains(&Capability::Tools));
        // No "completion" tag → no Generate, but empty fallback kicks in
        // Actually: capabilities is not empty, so the completion check fails.
        // The function only adds Generate if capabilities is empty OR has "completion".
        assert!(!caps.contains(&Capability::Generate));
    }

    // ── Graduated pool generation ───────────────────────────────

    #[test]
    fn tools_prompts_have_graduated_pool_sizes() {
        let prompts = build_tools_prompts();
        assert!(prompts.len() >= 9, "expected at least 9 tools prompts");

        let pool_sizes: Vec<usize> = prompts.iter().map(|p| p.pool_size).collect();
        // Should include small, medium, and large pools
        assert!(pool_sizes.contains(&1), "missing pool_size=1");
        assert!(pool_sizes.contains(&3), "missing pool_size=3");
        assert!(pool_sizes.contains(&10), "missing pool_size=10");
        assert!(pool_sizes.contains(&25), "missing pool_size=25");
        assert!(pool_sizes.contains(&50), "missing pool_size=50");
        assert!(pool_sizes.contains(&100), "missing pool_size=100");
    }

    #[test]
    fn tools_pool_contains_correct_number_of_tools() {
        let prompts = build_tools_prompts();
        for prompt in &prompts {
            let tools = prompt.tools.as_array().expect("tools should be an array");
            assert_eq!(
                tools.len(),
                prompt.pool_size,
                "prompt '{}' declares pool_size={} but has {} tools",
                prompt.user_message,
                prompt.pool_size,
                tools.len()
            );
        }
    }

    #[test]
    fn target_tool_present_in_every_pool() {
        let prompts = build_tools_prompts();
        for prompt in &prompts {
            let tools = prompt.tools.as_array().unwrap();
            for expected_fn in &prompt.expected_fns {
                let found = tools.iter().any(|t| {
                    t.get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(|n| n.as_str())
                        == Some(expected_fn)
                });
                assert!(
                    found,
                    "expected function '{}' not found in pool for prompt '{}'",
                    expected_fn, prompt.user_message
                );
            }
        }
    }

    #[test]
    fn distractor_pool_has_100_unique_tools() {
        let pool = distractor_pool();
        assert_eq!(pool.len(), 100, "distractor pool should have exactly 100 tools");

        let names: Vec<&str> = pool
            .iter()
            .map(|t| {
                t.get("function")
                    .unwrap()
                    .get("name")
                    .unwrap()
                    .as_str()
                    .unwrap()
            })
            .collect();

        // All unique
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(
            names.len(),
            unique.len(),
            "distractor pool has duplicate tool names"
        );
    }

    #[test]
    fn distractor_pool_does_not_overlap_with_targets() {
        let pool = distractor_pool();
        let targets = target_tools();
        let target_names: Vec<&str> = targets.iter().map(|(n, _, _)| *n).collect();

        for tool in &pool {
            let name = tool
                .get("function")
                .unwrap()
                .get("name")
                .unwrap()
                .as_str()
                .unwrap();
            assert!(
                !target_names.contains(&name),
                "distractor '{}' overlaps with target tool",
                name
            );
        }
    }

    #[test]
    fn tool_schema_generates_valid_json() {
        let schema = tool_schema("my_func", "Does something", &[("arg1", "string"), ("arg2", "integer")]);
        assert_eq!(
            schema["function"]["name"].as_str().unwrap(),
            "my_func"
        );
        assert_eq!(
            schema["function"]["description"].as_str().unwrap(),
            "Does something"
        );
        let props = schema["function"]["parameters"]["properties"].as_object().unwrap();
        assert!(props.contains_key("arg1"));
        assert!(props.contains_key("arg2"));
        assert_eq!(props["arg1"]["type"].as_str().unwrap(), "string");
        assert_eq!(props["arg2"]["type"].as_str().unwrap(), "integer");
    }
}
