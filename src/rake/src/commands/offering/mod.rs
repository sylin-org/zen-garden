//! Offering commands
//!
//! Commands for managing service offerings:
//! - List available offerings
//! - Install offerings
//! - Query/search offerings
//! - View offering details

use crate::command_manifest::cmd;
use crate::commands::Command;
use crate::context::Runtime;
use crate::discovery;
use crate::ui::colors::CliFormatter;
use crate::ui::rendering as ui;
use anyhow::Result;
use garden_common::api_utils::ApiResponse;
use garden_common::client::StoneApi;
use garden_common::offerings::OfferingFqn;
use garden_common::{HardwareCapabilities, ServiceInfo};
use std::collections::BTreeMap;
use std::time::Duration;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, serde::Deserialize)]
pub struct OfferingEntry {
    pub name: String,
    pub category: String,
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub image: String,
    #[serde(default)]
    pub compatibility: OfferingCompatibility,
}

#[derive(Debug, Default, serde::Deserialize)]
pub struct OfferingCompatibility {
    #[serde(default)]
    pub decision: String,
    pub reason: Option<String>,
    pub original_image: Option<String>,
    pub fallback_image: Option<String>,
    pub suggestion: Option<String>,
}

// TaxonomyDictionary moved to garden_common::offerings - search is server-side

#[derive(Debug, serde::Deserialize)]
pub struct PlacementResponse {
    pub recommendations: Vec<PlacementRecommendation>,
    pub evaluated_stones: usize,
    pub timestamp: String,
    /// Summary of excluded stones (if any), e.g., "2 stones excluded: offering not available"
    pub exclusion_summary: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct PlacementRecommendation {
    pub stone_id: String,
    pub hostname: String,
    pub score: i32,
    pub is_local: bool,
    pub compatibility: String,
    pub metrics: PlacementMetrics,
    pub services_count: usize,
    pub breakdown: ScoreBreakdown,
}

#[derive(Debug, serde::Deserialize)]
pub struct PlacementMetrics {
    pub memory_free_mb: u64,
    pub memory_total_mb: u64,
    pub cpu_load_percent: u8,
    pub storage_free_gb: u64,
    pub storage_total_gb: u64,
    pub storage_type: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct ScoreBreakdown {
    pub compatibility: i32,
    pub memory: i32,
    pub cpu: i32,
    pub storage: i32,
    pub hardware: i32,
    pub distribution: i32,
    pub tended_bonus: i32,
}

/// Action for offer command
#[derive(Debug, Clone)]
pub enum OfferAction {
    /// List all offerings
    List,
    /// Refresh offerings index
    Refresh,
    /// Show offering info
    Info { name: String },
    /// Install offering
    Install { name: String },
    /// Query/search offerings
    Query { query: String },
    /// Query across all stones
    QueryAnywhere { query: String },
    /// Get intelligent placement recommendation
    PlacementRecommend { name: String, quiet: bool },
    /// Deploy a Docker image directly (OFFER-0006)
    Image {
        image_ref: String,
        instance: Option<String>,
        info_only: bool,
    },
}

pub struct OfferCommand {
    pub action: OfferAction,
    pub prefer: Vec<String>,
    pub anywhere_on_fail: bool,
    pub quiet: bool,
}

// ============================================================================
// Taxonomy / Search Functions
// ============================================================================

// NOTE: Taxonomy dictionary and scoring logic have been moved to Moss.
// Rake is a thin shell - it calls Moss search API and displays results.

/// Stone preference scoring for ranking stones in garden-wide search.
/// This remains in Rake because it's about stone selection, not offering matching.
pub fn stone_prefer_score(prefer: &[String], caps: Option<&HardwareCapabilities>) -> i32 {
    let Some(caps) = caps else {
        return 0;
    };
    let disk_type = caps
        .hardware
        .disk
        .as_ref()
        .and_then(|d| d.disk_type.as_ref())
        .map(|s: &String| s.to_lowercase());

    let mut score = 0i32;
    for p in prefer {
        match p.to_lowercase().as_str() {
            "ssd" => {
                if matches!(disk_type.as_deref(), Some("ssd") | Some("nvme")) {
                    score += 10;
                }
            }
            "nvme" => {
                if disk_type.as_deref() == Some("nvme") {
                    score += 12;
                }
            }
            "hdd" => {
                if disk_type.as_deref() == Some("hdd") {
                    score += 6;
                }
            }
            _ => {}
        }
    }
    score
}

// ============================================================================
// API Functions
// ============================================================================

async fn fetch_offerings(api: &StoneApi) -> Result<Vec<OfferingEntry>> {
    let value: serde_json::Value = api.offerings().list().await
        .map_err(|e| {
            if e.is_not_found() {
                anyhow::anyhow!("This stone's moss does not support validated offerings. Upgrade moss and retry.")
            } else {
                anyhow::anyhow!("{}", e.display_message())
            }
        })?;
    let offerings: Vec<OfferingEntry> = serde_json::from_value(value)?;
    Ok(offerings)
}

async fn fetch_capabilities(
    client: &reqwest::Client,
    endpoint: &str,
) -> Result<HardwareCapabilities> {
    let api = garden_common::client::StoneApi::new(client.clone(), endpoint.to_string());
    let caps = api.stone().capabilities_core().await?;
    Ok(caps)
}

async fn fetch_offering_info_json(api: &StoneApi, offering: &str) -> Result<serde_json::Value> {
    let data: serde_json::Value = api.offerings().get(offering).await
        .map_err(|e| {
            if e.is_not_found() {
                anyhow::anyhow!("Unknown offering: {}", offering)
            } else {
                anyhow::anyhow!("{}", e.display_message())
            }
        })?;
    Ok(data)
}

/// Search offerings via Moss API. All taxonomy/scoring logic is server-side.
async fn fetch_search_results(
    api: &StoneApi,
    query: &str,
    prefer: &[String],
    limit: usize,
) -> Result<garden_common::offerings::OfferingSearchResponse> {
    let prefer_joined = if prefer.is_empty() {
        None
    } else {
        Some(prefer.join(","))
    };
    api.offerings()
        .search(query, prefer_joined.as_deref(), Some(limit as u32))
        .await
        .map_err(|e| {
            if e.is_not_found() {
                anyhow::anyhow!(
                    "This stone's moss does not support offering search. Upgrade moss and retry."
                )
            } else {
                anyhow::anyhow!("{}", e.display_message())
            }
        })
}

async fn refresh_offerings_index(api: &StoneApi) -> Result<()> {
    let body: serde_json::Value = api.offerings().refresh().await.map_err(|e| {
        if e.is_not_found() {
            anyhow::anyhow!(
                "This stone's moss does not support offerings refresh. Upgrade moss and retry."
            )
        } else {
            anyhow::anyhow!("{}", e.display_message())
        }
    })?;

    let count = body.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
    let generated_at = body
        .get("generated_at")
        .and_then(|v| v.as_str())
        .unwrap_or("<unknown>");

    println!("✓ Offerings index rebuilt");
    println!("  Count: {}", count);
    println!("  Generated: {}", generated_at);

    if let Some(fp) = body.get("fingerprint") {
        println!("  Fingerprint: {}", fp);
    }

    Ok(())
}

// ============================================================================
// Display Functions
// ============================================================================

// format_offering_flag removed - search results now use direct compatibility comparison

fn render_services_table(services: &[ServiceInfo], term: &ui::TerminalInfo) {
    let mut table = ui::TableBuilder::new()
        .add_column(ui::constants::MAX_SERVICE_NAME_LEN, ui::Align::Left)
        .add_column(20, ui::Align::Left)
        .add_column(16, ui::Align::Left);

    let mut running_count = 0;
    let mut stopped_count = 0;

    for svc in services {
        let status_str = format!("{:?}", svc.status);
        if status_str
            .to_lowercase()
            .contains(garden_common::constants::SERVICE_RUNNING)
        {
            running_count += 1;
        } else {
            stopped_count += 1;
        }

        let status_display = ui::status_indicator(&status_str.to_lowercase(), term.supports_color);
        table.add_row(vec![
            ui::truncate_name(&svc.name, ui::constants::MAX_SERVICE_NAME_LEN),
            status_display,
            if svc.offering.is_empty() {
                garden_common::constants::VALUE_UNKNOWN.to_string()
            } else {
                svc.offering.clone()
            },
        ]);
    }

    println!("{}", table.render());
    println!();
    println!(
        "{}  {} services ({} running, {} stopped)",
        " ".repeat(ui::constants::DEFAULT_INDENT),
        services.len(),
        running_count,
        stopped_count
    );
}

async fn print_offerings_index(api: &StoneApi) -> Result<()> {
    let term = ui::TerminalInfo::detect();

    // Fetch running services
    let services: Vec<ServiceInfo> = api.services().list().await.unwrap_or_default();

    // Display running services if any
    if !services.is_empty() {
        println!("{}", ui::section_header("SERVICES", &term));
        println!();
        render_services_table(&services, &term);
        println!();
        println!();
    }

    // Fetch and display available offerings
    let offerings = fetch_offerings(api).await?;
    if offerings.is_empty() {
        println!(
            "{}",
            ui::empty_state(
                "No offerings available",
                Some("Try: garden-rake offer refresh")
            )
        );
        return Ok(());
    }

    // Filter out incompatible offerings (decision = "fail")
    let compatible_offerings: Vec<OfferingEntry> = offerings
        .into_iter()
        .filter(|o| o.compatibility.decision != garden_common::constants::COMPAT_FAIL)
        .collect();

    if compatible_offerings.is_empty() {
        println!(
            "{}",
            ui::empty_state(
                "No compatible offerings",
                Some("All offerings are incompatible with this stone")
            )
        );
        return Ok(());
    }

    // Group by category
    let mut by_category: BTreeMap<String, Vec<OfferingEntry>> = BTreeMap::new();
    let mut restricted_offerings: Vec<String> = Vec::new();
    for o in compatible_offerings {
        if o.compatibility.decision == garden_common::constants::COMPAT_FALLBACK {
            restricted_offerings.push(o.name.clone());
        }
        by_category.entry(o.category.clone()).or_default().push(o);
    }

    println!("{}", ui::section_header("AVAILABLE OFFERINGS", &term));
    println!();

    let grid = ui::CategoryGrid::new(&term);

    for (category, mut items) in by_category {
        items.sort_by(|a, b| a.name.cmp(&b.name));

        let grid_items: Vec<String> = items
            .iter()
            .map(|o| {
                if o.compatibility.decision == garden_common::constants::COMPAT_FALLBACK {
                    format!("{}{}", o.name, ui::constants::LEGEND_SYMBOL)
                } else {
                    o.name.clone()
                }
            })
            .collect();

        print!("{}", grid.render_category(&category, &grid_items));
        println!();
    }

    if !restricted_offerings.is_empty() {
        println!(
            "{}  {} restricted (uses compatibility fallback)",
            " ".repeat(ui::constants::DEFAULT_INDENT),
            ui::constants::LEGEND_SYMBOL
        );
        println!();
        println!(
            "{}View compatibility details:",
            " ".repeat(ui::constants::DEFAULT_INDENT)
        );
        for name in &restricted_offerings {
            println!(
                "{}  garden-rake offer {} info",
                " ".repeat(ui::constants::DEFAULT_INDENT * 2),
                name
            );
        }
    }

    Ok(())
}

async fn print_offering_info(api: &StoneApi, offering: &str) -> Result<()> {
    let body: serde_json::Value = api.offerings().get(offering).await.map_err(|e| {
        if e.is_not_found() {
            anyhow::anyhow!("Unknown offering: {}", offering)
        } else {
            anyhow::anyhow!("{}", e.display_message())
        }
    })?;

    let name = body
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(offering);
    let image = body
        .get("image")
        .and_then(|v| v.as_str())
        .unwrap_or("<unknown>");

    println!("Offering: {}", name);
    println!("Image: {}", image);

    if let Some(compat) = body.get("compatibility") {
        let decision = compat
            .get("decision")
            .and_then(|v| v.as_str())
            .unwrap_or(garden_common::constants::COMPAT_PASS);
        match decision {
            garden_common::constants::COMPAT_PASS => println!("Compatibility: pass"),
            garden_common::constants::COMPAT_FALLBACK => {
                let reason = compat
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("<unspecified>");
                let original = compat.get("original_image").and_then(|v| v.as_str());
                let fallback = compat.get("fallback_image").and_then(|v| v.as_str());
                println!("Compatibility: fallback");
                if let (Some(o), Some(f)) = (original, fallback) {
                    println!("  From: {}", o);
                    println!("  To:   {}", f);
                }
                println!("  Reason: {}", reason);
            }
            garden_common::constants::COMPAT_FAIL => {
                let reason = compat
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("<unspecified>");
                println!("Compatibility: fail");
                println!("  Reason: {}", reason);
                if let Some(s) = compat.get("suggestion").and_then(|v| v.as_str()) {
                    println!("  Suggestion: {}", s);
                }
                println!("  Result: this offering cannot be installed on this stone");
            }
            other => println!("Compatibility: {}", other),
        }
    }

    if let Some(ports) = body.get("ports").and_then(|v| v.as_array())
        && !ports.is_empty() {
            println!("Ports:");
            for p in ports {
                if let (Some(host), Some(container)) = (
                    p.get(0).and_then(|v| v.as_u64()),
                    p.get(1).and_then(|v| v.as_u64()),
                ) {
                    println!("  - {}:{}", host, container);
                }
            }
        }

    Ok(())
}

// ============================================================================
// Recommendation Functions
// ============================================================================

async fn print_offer_query_recommendations(
    api: &StoneApi,
    query: &str,
    prefer: &[String],
) -> Result<()> {
    // All search logic is server-side in Moss - Rake is a thin client
    let results = fetch_search_results(api, query, prefer, 3).await?;

    println!("Query: {}", query);
    if !prefer.is_empty() {
        println!("Prefer: {}", prefer.join(", "));
    }

    if results.results.is_empty() {
        println!("No matching offerings found on this stone.");
        return Ok(());
    }

    println!("Top recommendations:");
    for (idx, o) in results.results.iter().enumerate() {
        let flag = if o.compatibility == garden_common::constants::COMPAT_FALLBACK {
            "(!) "
        } else {
            ""
        };
        println!("  {}. {} - {}{}", idx + 1, o.name, flag, o.description);
        println!("     Run: garden-rake offer {} --at {}", o.name, api.endpoint());
    }

    Ok(())
}

async fn print_offer_anywhere_recommendations(
    client: &reqwest::Client,
    query: &str,
    prefer: &[String],
) -> Result<()> {
    // Collect endpoints using streaming API
    let mut endpoints = Vec::new();
    let _ = discovery::discover_all_moss_stream(
        std::time::Duration::from_secs(2),
        |response, _instant| {
            endpoints.push((response.stone_name.clone(), response.address.http_base()));
        },
    );

    if endpoints.is_empty() {
        anyhow::bail!("No stones discovered");
    }

    // Query each stone's search API and aggregate results
    let mut candidates: Vec<(
        i32,
        String,
        String,
        garden_common::offerings::OfferingSearchResult,
    )> = Vec::new();

    for (stone_name, ep) in endpoints {
        let api = StoneApi::new(client.clone(), ep.clone());

        // Get stone hardware capabilities for preference scoring
        let caps = fetch_capabilities(client, &ep).await.ok();
        let stone_bonus = stone_prefer_score(prefer, caps.as_ref());

        // Call search API on this stone
        let results = match fetch_search_results(&api, query, prefer, 10).await {
            Ok(r) => r,
            Err(_) => continue,
        };

        for o in results.results {
            let combined = o.score * 100 + stone_bonus;
            candidates.push((combined, stone_name.clone(), ep.clone(), o));
        }
    }

    candidates.sort_by(|(sa, an, ae, _), (sb, bn, be, _)| {
        sb.cmp(sa).then_with(|| an.cmp(bn)).then_with(|| ae.cmp(be))
    });

    println!("Query: {}", query);
    if !prefer.is_empty() {
        println!("Prefer: {}", prefer.join(", "));
    }

    if candidates.is_empty() {
        println!("No matching offerings found on any discovered stone.");
        return Ok(());
    }

    println!("Top recommendations across stones:");
    for (idx, (_score, stone_name, ep, o)) in candidates.into_iter().take(3).enumerate() {
        let flag = if o.compatibility == garden_common::constants::COMPAT_FALLBACK {
            "(!) "
        } else {
            ""
        };
        println!(
            "  {}. {} @ {} - {}{}",
            idx + 1,
            o.name,
            stone_name,
            flag,
            o.description
        );
        println!("     Run: garden-rake offer {} --at {}", o.name, ep);
    }

    Ok(())
}

async fn print_alternatives_for_failed_install(
    api: &StoneApi,
    offering: &str,
    prefer: &[String],
) -> Result<Option<String>> {
    let info = fetch_offering_info_json(api, offering).await?;

    // Build search query from offering's category and tags
    let mut seed_tokens: Vec<String> = Vec::new();
    if let Some(category) = info.get("category").and_then(|v| v.as_str()) {
        seed_tokens.push(category.to_string());
    }
    if let Some(tags) = info.get("tags").and_then(|v| v.as_array()) {
        for t in tags.iter().filter_map(|v| v.as_str()) {
            seed_tokens.push(t.to_string());
        }
    }

    if seed_tokens.is_empty() {
        return Ok(None);
    }

    let query = seed_tokens.join(" ");

    // Use Moss search API to find alternatives
    let results = match fetch_search_results(api, &query, prefer, 5).await {
        Ok(r) => r,
        Err(_) => return Ok(Some(query)),
    };

    // Filter out the original offering from results
    let alternatives: Vec<_> = results
        .results
        .iter()
        .filter(|o| o.name != offering)
        .take(3)
        .collect();

    if alternatives.is_empty() {
        return Ok(Some(query));
    }

    println!("\nAlternatives:");
    for (idx, o) in alternatives.iter().enumerate() {
        let flag = if o.compatibility == garden_common::constants::COMPAT_FALLBACK {
            "(!) "
        } else {
            ""
        };
        println!("  {}. {} - {}{}", idx + 1, o.name, flag, o.description);
        println!("     Run: garden-rake offer {} --at {}", o.name, api.endpoint());
    }

    if !prefer.is_empty() {
        println!(
            "\nTo search across stones: garden-rake offer {} --at anywhere --prefer {}",
            query,
            prefer.join(",")
        );
    } else {
        println!(
            "\nTo search across stones: garden-rake offer {} --at anywhere",
            query
        );
    }

    Ok(Some(query))
}

// ============================================================================
// Job Progress Streaming
// ============================================================================

/// Stream job progress updates from Moss stone's /api/v1/events endpoint.
/// Falls back to elapsed-time display if endpoint unavailable (older stones).
///
/// Implements golden standard: Physicality Over Theater
/// - Shows real timing, no fake progress bars
/// - Polls every 500ms for container operations (seconds/minutes duration)
/// - Displays percentage when stone reports it, elapsed time always
async fn stream_job_progress(
    api: &StoneApi,
    job_id: &str,
    service_name: &str,
    quiet: bool,
) -> Result<()> {
    let events_url = format!(
        "{}/api/v1/events?job_id={}",
        api.endpoint(),
        job_id
    );
    let client = api.http();
    let term = ui::TerminalInfo::detect();
    let start_time = std::time::Instant::now();

    // Check if stone supports /api/v1/events (probe with HEAD request)
    let probe = client.head(&events_url).send().await;
    let events_supported =
        matches!(probe, Ok(resp) if resp.status() != reqwest::StatusCode::NOT_FOUND);

    if !events_supported {
        // Fallback: show elapsed time without progress details
        if !quiet {
            println!(
                "{}{} Installing... (progress endpoint unavailable)",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::progress_step(true, "")
            );
        }

        // Simple elapsed time loop (5 minute timeout)
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        let timeout = Duration::from_secs(300);

        loop {
            interval.tick().await;
            let elapsed = start_time.elapsed();

            if elapsed >= timeout {
                println!(
                    "\n{}⏱  Operation timeout ({})",
                    " ".repeat(ui::constants::DEFAULT_INDENT),
                    ui::format_elapsed_time(timeout)
                );
                println!(
                    "{}Check status: garden-rake list",
                    " ".repeat(ui::constants::DEFAULT_INDENT)
                );
                break;
            }

            // Check completion by querying service list
            if let Ok(services) = api.services().list().await {
                    if services.iter().any(|s| s.name == service_name) {
                        if !quiet {
                            println!(
                                "\n{}{} Installation complete [{}]",
                                " ".repeat(ui::constants::DEFAULT_INDENT),
                                ui::status_indicator("ok", term.supports_color),
                                ui::format_elapsed_time(elapsed)
                            );
                        }
                        break;
                    }
                }

            // Update progress display every 2 seconds
            if elapsed.as_secs().is_multiple_of(2) && !quiet {
                print!(
                    "\r{}Installing... [{}]",
                    " ".repeat(ui::constants::DEFAULT_INDENT),
                    ui::format_elapsed_time(elapsed)
                );
                use std::io::Write;
                std::io::stdout().flush().ok();
            }
        }

        return Ok(());
    }

    // Full progress streaming from /api/v1/events
    if !quiet {
        println!(
            "{}{} Installation started",
            " ".repeat(ui::constants::DEFAULT_INDENT),
            ui::progress_step(true, "")
        );
    }

    let mut interval = tokio::time::interval(Duration::from_millis(500));
    let timeout = Duration::from_secs(300); // 5 minutes
    let mut last_message = String::new();

    loop {
        interval.tick().await;
        let elapsed = start_time.elapsed();

        if elapsed >= timeout {
            println!(
                "\n{}⏱  Operation timeout ({})",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::format_elapsed_time(timeout)
            );
            println!(
                "{}Check status: garden-rake list",
                " ".repeat(ui::constants::DEFAULT_INDENT)
            );
            break;
        }

        // Poll /api/v1/events for job updates
        match client.get(&events_url).send().await {
            Ok(response) if response.status().is_success() => {
                if let Ok(event) = response.json::<serde_json::Value>().await {
                    let status = event
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let message = event.get("message").and_then(|v| v.as_str()).unwrap_or("");
                    let progress = event.get("progress").and_then(|v| v.as_u64());

                    // Display new status updates
                    if !message.is_empty() && message != last_message && !quiet {
                        if let Some(pct) = progress {
                            println!(
                                "\r{}{}% {} [{}]",
                                " ".repeat(ui::constants::DEFAULT_INDENT),
                                pct,
                                message,
                                ui::format_elapsed_time(elapsed)
                            );
                        } else {
                            println!(
                                "\r{}{} [{}]",
                                " ".repeat(ui::constants::DEFAULT_INDENT),
                                message,
                                ui::format_elapsed_time(elapsed)
                            );
                        }
                        last_message = message.to_string();
                    }

                    // Check for completion
                    if status == garden_common::constants::STATUS_COMPLETED
                        || status == garden_common::constants::STATUS_SUCCESS
                    {
                        if !quiet {
                            println!(
                                "\n{}{} Installation complete [{}]",
                                " ".repeat(ui::constants::DEFAULT_INDENT),
                                ui::status_indicator("ok", term.supports_color),
                                ui::format_elapsed_time(elapsed)
                            );
                        }
                        break;
                    } else if status == garden_common::constants::STATUS_FAILED
                        || status == garden_common::constants::STATUS_ERROR
                    {
                        println!(
                            "\n{}{} Installation failed: {}",
                            " ".repeat(ui::constants::DEFAULT_INDENT),
                            ui::status_indicator("error", term.supports_color),
                            message
                        );
                        break;
                    }
                }
            }
            Ok(response) if response.status() == reqwest::StatusCode::NOT_FOUND => {
                // Job completed or not found
                if !quiet {
                    println!(
                        "\n{}{} Installation complete (job finished) [{}]",
                        " ".repeat(ui::constants::DEFAULT_INDENT),
                        ui::status_indicator("ok", term.supports_color),
                        ui::format_elapsed_time(elapsed)
                    );
                }
                break;
            }
            _ => {
                // Network error or server issue, continue polling
                if elapsed.as_secs().is_multiple_of(5) && !quiet {
                    print!(
                        "\r{}Checking progress... [{}]",
                        " ".repeat(ui::constants::DEFAULT_INDENT),
                        ui::format_elapsed_time(elapsed)
                    );
                    use std::io::Write;
                    std::io::stdout().flush().ok();
                }
            }
        }
    }

    Ok(())
}

/// Handle intelligent placement recommendation
///
/// Interactive mode: Show recommendations, let user select
/// Quiet mode: Auto-select top stone and install
async fn handle_placement_recommendation(
    client: &reqwest::Client,
    offering: &str,
    quiet: bool,
) -> Result<()> {
    use crate::tending;
    use garden_common::TopologyEntry;

    let term = ui::TerminalInfo::detect();
    let indent = " ".repeat(ui::constants::DEFAULT_INDENT);

    // Show waiting message (placement evaluation takes time)
    if !quiet {
        println!(
            "{}⏳ Evaluating placement options for '{}'...",
            indent, offering
        );
        println!();
    }

    // Use execute_on_stone to handle tended + mDNS fallback with SoC
    let placement_result = tending::execute_on_stone(
        Duration::from_secs(3),
        Some(|stone_name: &str| {
            println!(
                "{}Stone '{}' is sleeping (offline). Picking a new stone...",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                stone_name
            );
        }),
        |candidate| {
            let client = client.clone();
            let offering = offering.to_string();
            let stone_name = candidate.stone_name.clone();
            let endpoint = candidate.endpoint.clone();
            async move {
                use crate::tending::StoneError;
                use garden_common::client::StoneApi;

                let api = StoneApi::new(client, endpoint);
                let payload = serde_json::json!({
                    "offering": offering,
                    "preferences": [],
                    "top_n": 3
                });

                let response = api.http()
                    .post(&format!("{}/api/v1/garden/recommend", api.endpoint()))
                    .json(&payload)
                    .timeout(Duration::from_secs(10))
                    .send()
                    .await
                    .map_err(|e| {
                        tracing::debug!(
                            stone = %stone_name,
                            error = ?e,
                            "Failed to reach stone"
                        );
                        StoneError::ConnectionFailed(format!("Failed to reach stone: {}", e))
                    })?;

                let status = response.status();
                if !status.is_success() {
                    tracing::debug!(
                        stone = %stone_name,
                        status = %status,
                        "Stone returned error"
                    );
                    return Err(StoneError::ResponseError(
                        status.as_u16(),
                        format!("Stone returned {}", status),
                    ));
                }

                let json = response.json::<serde_json::Value>().await.map_err(|e| {
                    StoneError::ProcessingError(format!("Failed to read response: {}", e))
                })?;

                // Try both wrapped and unwrapped formats
                if let Ok(data) =
                    serde_json::from_value::<ApiResponse<PlacementResponse>>(json.clone())
                {
                    Ok(data.data)
                } else if let Ok(data) = serde_json::from_value::<PlacementResponse>(json.clone()) {
                    Ok(data)
                } else {
                    Err(StoneError::ProcessingError(
                        "Failed to parse placement response".to_string(),
                    ))
                }
            }
        },
    )
    .await;

    let (placement, responding_stone) = match placement_result {
        Ok((p, s)) => (p, s),
        Err(_) => {
            println!(
                "{}{} Could not get placement recommendations from any stone",
                indent,
                ui::status_indicator("error", term.supports_color)
            );
            println!(
                "{}Verify that Moss is running on at least one stone",
                indent
            );
            return Ok(());
        }
    };

    // Query topology from responding stone to get endpoints for all stones
    let topology_url = format!(
        "{}/api/v1/garden/topology",
        responding_stone.endpoint.trim_end_matches('/')
    );
    let endpoint_map: std::collections::HashMap<String, String> = match client
        .get(&topology_url)
        .timeout(Duration::from_secs(5))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(api_response) = resp.json::<ApiResponse<Vec<TopologyEntry>>>().await {
                api_response
                    .data
                    .into_iter()
                    .map(|entry| (entry.stone_name.to_lowercase(), entry.address.http_base()))
                    .collect()
            } else {
                std::collections::HashMap::new()
            }
        }
        _ => std::collections::HashMap::new(),
    };

    // Also add the responding stone to the map
    let mut endpoint_map = endpoint_map;
    endpoint_map.insert(
        responding_stone.stone_name.to_lowercase(),
        responding_stone.endpoint.clone(),
    );

    if placement.recommendations.is_empty() {
        println!(
            "{}{} No compatible stones found for '{}'",
            indent,
            ui::status_indicator("error", term.supports_color),
            offering
        );
        println!(
            "{}This offering may not be available or compatible with your network",
            indent
        );
        return Ok(());
    }

    // Quiet mode: Auto-select top recommendation
    if quiet {
        let top = &placement.recommendations[0];
        println!("{}Installing '{}' on {}...", indent, offering, top.hostname);

        // Find the stone's endpoint from topology
        if let Some(endpoint) = endpoint_map.get(&top.hostname.to_lowercase()) {
            return install_on_stone(client, endpoint, offering, quiet).await;
        } else {
            println!(
                "{}{} Could not find endpoint for stone '{}'",
                indent,
                ui::status_indicator("error", term.supports_color),
                top.hostname
            );
            return Ok(());
        }
    }

    // Interactive mode: Show recommendations
    let fmt = CliFormatter::new();
    println!(
        "{}{}",
        indent,
        fmt.title(&format!(
            "PLACEMENT RECOMMENDATIONS FOR '{}'",
            offering.to_uppercase()
        ))
    );
    println!("{}{}", indent, fmt.divider(&"─".repeat(60)));
    println!();

    let top_n = placement.recommendations.len().min(3);
    for (idx, rec) in placement.recommendations.iter().take(top_n).enumerate() {
        let rank = idx + 1;

        // Compatibility icon (using constants from garden_common)
        use garden_common::constants::{COMPAT_FALLBACK, COMPAT_PASS};
        let compat_icon = match rec.compatibility.as_str() {
            COMPAT_PASS => {
                if term.supports_color {
                    "✅"
                } else {
                    "[OK]"
                }
            }
            COMPAT_FALLBACK => {
                if term.supports_color {
                    "⚠️"
                } else {
                    "[WARN]"
                }
            }
            _ => {
                if term.supports_color {
                    "❌"
                } else {
                    "[FAIL]"
                }
            }
        };

        // Tended stone marker (inline with hostname per spec)
        // Mark if this recommendation is the responding stone (which is now tended)
        let is_responding_stone = rec
            .hostname
            .eq_ignore_ascii_case(&responding_stone.stone_name);
        let tended_marker = if is_responding_stone {
            if term.supports_color {
                "⭐ "
            } else {
                "* "
            }
        } else {
            "  "
        };
        let tended_label = if is_responding_stone {
            " ← tended stone"
        } else {
            ""
        };

        // Format: "1. ⭐ hostname     [Score: 87/100] ← tended stone"
        println!(
            "{}  {}. {}{} {:<16} [Score: {}/100]{}",
            indent, rank, compat_icon, tended_marker, rec.hostname, rec.score, tended_label
        );

        // Memory in absolute values (GB), not percentage
        let mem_free_gb = rec.metrics.memory_free_mb / 1024;

        // Storage type (clean up Debug format)
        let storage_type = rec
            .metrics
            .storage_type
            .trim_matches('"')
            .replace("Unknown", "");
        let storage_display = if storage_type.is_empty() {
            format!("{} GB", rec.metrics.storage_free_gb)
        } else {
            format!("{} GB ({})", rec.metrics.storage_free_gb, storage_type)
        };

        // Format: "     Memory: 24 GB free | CPU: 12% | Storage: 450 GB (NVMe)"
        println!(
            "{}     Memory: {} GB free | CPU: {}% | Storage: {}",
            indent, mem_free_gb, rec.metrics.cpu_load_percent, storage_display
        );

        println!("{}     Services: {} running", indent, rec.services_count);

        println!();
    }

    // Show exclusion summary if any stones were excluded
    if let Some(ref summary) = placement.exclusion_summary {
        let info_icon = if term.supports_color {
            "ℹ️"
        } else {
            "[INFO]"
        };
        println!("{}{} {}", indent, info_icon, summary);
        println!();
    }

    println!("{}{}", indent, fmt.divider(&"─".repeat(60)));

    if placement.recommendations.len() == 1 {
        // Single option: ask for confirmation
        println!(
            "{}Proceed with installation on '{}'? [Y/n]: ",
            indent, placement.recommendations[0].hostname
        );
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();

        if input.is_empty() || input == "y" || input == "yes" {
            let stone = &placement.recommendations[0];
            if let Some(endpoint) = endpoint_map.get(&stone.hostname.to_lowercase()) {
                return install_on_stone(client, endpoint, offering, quiet).await;
            } else {
                println!(
                    "{}{} Could not find endpoint for '{}'",
                    indent,
                    ui::status_indicator("error", term.supports_color),
                    stone.hostname
                );
            }
        } else {
            println!("{}Installation cancelled", indent);
        }
    } else {
        // Multiple options: let user select
        println!("{}Select stone (1-{}) or 'q' to quit: ", indent, top_n);
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();

        if input == "q" || input == "quit" || input == "exit" {
            println!("{}Installation cancelled", indent);
            return Ok(());
        }

        if let Ok(choice) = input.parse::<usize>() {
            if choice >= 1 && choice <= top_n {
                let stone = &placement.recommendations[choice - 1];
                if let Some(endpoint) = endpoint_map.get(&stone.hostname.to_lowercase()) {
                    return install_on_stone(client, endpoint, offering, quiet).await;
                } else {
                    println!(
                        "{}{} Could not find endpoint for '{}'",
                        indent,
                        ui::status_indicator("error", term.supports_color),
                        stone.hostname
                    );
                }
            } else {
                println!(
                    "{}{} Invalid selection",
                    indent,
                    ui::status_indicator("error", term.supports_color)
                );
            }
        } else {
            println!(
                "{}{} Invalid input",
                indent,
                ui::status_indicator("error", term.supports_color)
            );
        }
    }

    Ok(())
}

/// Install offering on a specific stone
async fn install_on_stone(
    client: &reqwest::Client,
    endpoint: &str,
    offering: &str,
    quiet: bool,
) -> Result<()> {
    // Delegate to existing install logic by creating a context
    let ctx = crate::context::Runtime::with_endpoint(
        client.clone(),
        endpoint.to_string(),
        None,
        quiet,
        false,
        0, // verbose
    );

    let install_cmd = OfferCommand::install(offering.to_string(), vec![], false, quiet);

    install_cmd.execute(&ctx).await
}

// ============================================================================
// Runtime Implementation
// ============================================================================

impl OfferCommand {
    pub fn list(quiet: bool) -> Self {
        Self {
            action: OfferAction::List,
            prefer: vec![],
            anywhere_on_fail: false,
            quiet,
        }
    }

    pub fn refresh(quiet: bool) -> Self {
        Self {
            action: OfferAction::Refresh,
            prefer: vec![],
            anywhere_on_fail: false,
            quiet,
        }
    }

    pub fn info(name: String, quiet: bool) -> Self {
        Self {
            action: OfferAction::Info { name },
            prefer: vec![],
            anywhere_on_fail: false,
            quiet,
        }
    }

    pub fn install(name: String, prefer: Vec<String>, anywhere_on_fail: bool, quiet: bool) -> Self {
        Self {
            action: OfferAction::Install { name },
            prefer,
            anywhere_on_fail,
            quiet,
        }
    }

    pub fn query(query: String, prefer: Vec<String>, quiet: bool) -> Self {
        Self {
            action: OfferAction::Query { query },
            prefer,
            anywhere_on_fail: false,
            quiet,
        }
    }

    pub fn query_anywhere(query: String, prefer: Vec<String>, quiet: bool) -> Self {
        Self {
            action: OfferAction::QueryAnywhere { query },
            prefer,
            anywhere_on_fail: false,
            quiet,
        }
    }

    pub fn placement_recommend(name: String, quiet: bool) -> Self {
        Self {
            action: OfferAction::PlacementRecommend { name, quiet },
            prefer: vec![],
            anywhere_on_fail: false,
            quiet,
        }
    }

    pub fn image(
        image_ref: String,
        instance: Option<String>,
        info_only: bool,
        quiet: bool,
    ) -> Self {
        Self {
            action: OfferAction::Image {
                image_ref,
                instance,
                info_only,
            },
            prefer: vec![],
            anywhere_on_fail: false,
            quiet,
        }
    }

    /// Check if the given name is a known offering (for query detection)
    pub async fn is_known_offering(client: &reqwest::Client, endpoint: &str, name: &str) -> bool {
        let offering_type = match OfferingFqn::parse(name) {
            Ok(fqn) => fqn.offering,
            Err(_) => return false,
        };

        let api = StoneApi::new(client.clone(), endpoint.to_string());
        if let Ok(offerings) = fetch_offerings(&api).await {
            offerings.iter().any(|o| o.name == offering_type)
        } else {
            false
        }
    }
}

impl Command for OfferCommand {
    fn requires_endpoint(&self) -> bool {
        !matches!(
            self.action,
            OfferAction::QueryAnywhere { .. } | OfferAction::PlacementRecommend { .. }
        )
    }

    fn show_stone_header(&self) -> bool {
        // Offer command manages its own display
        false
    }

    fn name(&self) -> &'static str {
        cmd::OFFER
    }

    fn execute<'a>(&'a self, ctx: &'a Runtime) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let term = ui::TerminalInfo::detect();

            match &self.action {
                OfferAction::List => {
                    let api = ctx.stone_api()?;
                    print_offerings_index(api).await?;
                }
                OfferAction::Refresh => {
                    let api = ctx.stone_api()?;
                    refresh_offerings_index(api).await?;
                }
                OfferAction::Info { name } => {
                    let api = ctx.stone_api()?;
                    print_offering_info(api, name).await?;
                }
                OfferAction::Query { query } => {
                    let api = ctx.stone_api()?;
                    print_offer_query_recommendations(api, query, &self.prefer)
                        .await?;
                }
                OfferAction::QueryAnywhere { query } => {
                    print_offer_anywhere_recommendations(&ctx.client, query, &self.prefer).await?;
                }
                OfferAction::PlacementRecommend { name, quiet } => {
                    handle_placement_recommendation(&ctx.client, name, *quiet).await?;
                }
                OfferAction::Install { name } => {
                    let api = ctx.stone_api()?;
                    let offering_fqn = OfferingFqn::parse(name)
                        .map_err(|e| anyhow::anyhow!("Invalid offering name '{}': {}", name, e))?;
                    let service_name = offering_fqn.fqn();
                    let offering_type = offering_fqn.offering.clone();
                    // Check if service is already installed
                    if let Ok(services) = api.services().list().await {
                            if let Some(existing) = services.iter().find(|s| s.name == service_name) {
                                let status_str = format!("{:?}", existing.status).to_lowercase();
                                let status_icon =
                                    ui::status_indicator(&status_str, term.supports_color);

                                println!(
                                    "{}{} Service '{}' is already installed ({})",
                                    " ".repeat(ui::constants::DEFAULT_INDENT),
                                    status_icon,
                                    existing.name,
                                    status_str
                                );
                                println!();
                                println!("{}Options:", " ".repeat(ui::constants::DEFAULT_INDENT));
                                println!(
                                    "{}  • View details:  garden-rake show {}",
                                    " ".repeat(ui::constants::DEFAULT_INDENT * 2),
                                    existing.name
                                );
                                println!(
                                    "{}  • Remove service: garden-rake remove {}",
                                    " ".repeat(ui::constants::DEFAULT_INDENT * 2),
                                    existing.name
                                );
                                if status_str.contains(garden_common::constants::SERVICE_STOPPED) {
                                    println!(
                                        "{}  • Start service:  garden-rake start {}",
                                        " ".repeat(ui::constants::DEFAULT_INDENT * 2),
                                        existing.name
                                    );
                                } else if status_str.contains(garden_common::constants::SERVICE_RUNNING)
                                {
                                    println!(
                                        "{}  • Stop service:   garden-rake stop {}",
                                        " ".repeat(ui::constants::DEFAULT_INDENT * 2),
                                        existing.name
                                    );
                                    println!(
                                        "{}  • Restart service: garden-rake restart {}",
                                        " ".repeat(ui::constants::DEFAULT_INDENT * 2),
                                        existing.name
                                    );
                                }
                                return Ok(());
                            }
                        }

                    // Create service from offering
                    let payload = serde_json::json!({
                        "offering": service_name,
                        "ports": [],
                        "environment": {}
                    });

                    let response = api.services().create(&payload).await?;
                    let status = response.status();
                    let body = response.json::<serde_json::Value>().await.ok();

                    match status {
                        reqwest::StatusCode::ACCEPTED | reqwest::StatusCode::OK => {
                            if let Some(body) = body {
                                let fmt = CliFormatter::new();
                                let indent = " ".repeat(ui::constants::DEFAULT_INDENT);

                                let response_service_name = body
                                    .get("service")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(&service_name);
                                let action = body
                                    .get("action")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("create");
                                let api_status = body
                                    .get("status")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("pending");
                                let message =
                                    body.get("message").and_then(|v| v.as_str()).unwrap_or("");

                                // Display: lowercase name with status on same line
                                // mongodb      [pending create]
                                println!();
                                let status_text = format!("[{} {}]", api_status, action);
                                let padding = 16usize.saturating_sub(response_service_name.len());
                                println!(
                                    "{}{}{}{}",
                                    indent,
                                    response_service_name,
                                    " ".repeat(padding),
                                    status_text
                                );
                                println!("{}{}", indent, fmt.divider(&"─".repeat(47)));

                                // Extract job_id from message if present
                                let job_id = if message.contains("Job ID:") || message.contains("job:")
                                {
                                    message
                                        .split_whitespace()
                                        .skip_while(|s| !s.contains("ID") && !s.contains("job"))
                                        .nth(1)
                                        .map(|s| s.trim_end_matches(&['.', ',', '!'][..]).to_string())
                                } else {
                                    None
                                };

                                if let Some(job_id) = job_id {
                                    stream_job_progress(
                                        api,
                                        &job_id,
                                        response_service_name,
                                        self.quiet,
                                    )
                                    .await?;
                                } else if message.contains("Adopted") {
                                    println!(
                                        "{}{} Service already exists (adopted)",
                                        indent,
                                        ui::status_indicator("ok", term.supports_color)
                                    );
                                    println!("{}{}", indent, message);
                                } else if message.contains("maintenance") {
                                    println!(
                                        "{}{} Under maintenance, retry later",
                                        indent,
                                        ui::status_indicator("pending", term.supports_color)
                                    );
                                } else if !message.is_empty() {
                                    println!("{}{}", indent, message);
                                }

                                // Display suggestions from v1 API (if not quiet)
                                if !self.quiet
                                    && let Some(suggestions) =
                                        body.get("suggestions").and_then(|v| v.as_array())
                                        && !suggestions.is_empty() {
                                            println!();
                                            println!("{}{}", indent, fmt.divider(&"─".repeat(47)));
                                            println!("{}{}", indent, fmt.group("SUGGESTIONS"));
                                            for suggestion in suggestions {
                                                if let Some(s) = suggestion.as_str() {
                                                    println!("{}    • {}", indent, s);
                                                }
                                            }
                                        }
                                println!();
                            }
                        }
                        reqwest::StatusCode::BAD_REQUEST => {
                            if let Some(body) = body {
                                let code = body
                                    .get("error")
                                    .and_then(|e| e.get("code"))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("<unknown>");
                                let msg = body
                                    .get("error")
                                    .and_then(|e| e.get("message"))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("Request failed");

                                println!(
                                    "{}{} {} ({})",
                                    " ".repeat(ui::constants::DEFAULT_INDENT),
                                    ui::status_indicator("error", term.supports_color),
                                    msg,
                                    code
                                );

                                if let Some(details) = body.get("error").and_then(|e| e.get("details"))
                                {
                                    if let Some(reason) = details.get("reason").and_then(|v| v.as_str())
                                    {
                                        println!(
                                            "{}Reason: {}",
                                            " ".repeat(ui::constants::DEFAULT_INDENT * 2),
                                            reason
                                        );
                                    }
                                    if let Some(suggestion) =
                                        details.get("suggestion").and_then(|v| v.as_str())
                                    {
                                        println!(
                                            "{}Suggestion: {}",
                                            " ".repeat(ui::constants::DEFAULT_INDENT * 2),
                                            suggestion
                                        );
                                    }
                                }

                                if code == garden_common::constants::COMPATIBILITY_FAILED {
                                    let derived_query = print_alternatives_for_failed_install(
                                        api,
                                        &offering_type,
                                        &self.prefer,
                                    )
                                    .await
                                    .ok()
                                    .flatten();

                                    if self.anywhere_on_fail
                                        && let Some(q) = derived_query {
                                            println!(
                                                "\n{}Searching across stones...",
                                                " ".repeat(ui::constants::DEFAULT_INDENT)
                                            );
                                            let _ = print_offer_anywhere_recommendations(
                                                &ctx.client,
                                                &q,
                                                &self.prefer,
                                            )
                                            .await;
                                        }
                                }
                            } else {
                                println!(
                                    "{}{} Failed: {}",
                                    " ".repeat(ui::constants::DEFAULT_INDENT),
                                    ui::status_indicator("error", term.supports_color),
                                    status
                                );
                            }
                        }
                        reqwest::StatusCode::NOT_FOUND => {
                            println!(
                                "{}{} Unknown offering: {}",
                                " ".repeat(ui::constants::DEFAULT_INDENT),
                                ui::status_indicator("error", term.supports_color),
                                name
                            );
                            let _ = print_offer_query_recommendations(
                                api,
                                name,
                                &self.prefer,
                            )
                            .await;
                        }
                        s if s.is_success() => {
                            println!(
                                "{}{} Offered {}",
                                " ".repeat(ui::constants::DEFAULT_INDENT),
                                ui::status_indicator("ok", term.supports_color),
                                name
                            );
                        }
                        reqwest::StatusCode::NOT_IMPLEMENTED => {
                            println!(
                                "{}ℹ️  Offer not implemented on server",
                                " ".repeat(ui::constants::DEFAULT_INDENT)
                            );
                        }
                        _ => {
                            println!(
                                "{}{} Failed: {}",
                                " ".repeat(ui::constants::DEFAULT_INDENT),
                                ui::status_indicator("error", term.supports_color),
                                status
                            );
                        }
                    }
                }
                OfferAction::Image {
                    image_ref,
                    instance,
                    info_only,
                } => {
                    let api = ctx.stone_api()?;

                    if *info_only {
                        // Info-only: inspect the image without deploying
                        let url = format!(
                            "{}/api/v1/stone/offerings/inspect?image={}",
                            api.endpoint(),
                            urlencoding::encode(image_ref)
                        );
                        let response = api.http().get(&url).send().await?;
                        let status = response.status();
                        let body: serde_json::Value = response.json().await?;

                        if status.is_success() {
                            let indent = " ".repeat(ui::constants::DEFAULT_INDENT);
                            println!();
                            println!("{}Image: {}", indent, image_ref);
                            if let Some(arch) = body.get("architecture").and_then(|v| v.as_str()) {
                                println!("{}Architecture: {}", indent, arch);
                            }
                            if let Some(ports) = body.get("exposed_ports").and_then(|v| v.as_array())
                                && !ports.is_empty() {
                                    let port_strs: Vec<String> = ports
                                        .iter()
                                        .filter_map(|p| p.as_u64().map(|n| n.to_string()))
                                        .collect();
                                    println!("{}Ports: {}", indent, port_strs.join(", "));
                                }
                            if let Some(vols) = body.get("volumes").and_then(|v| v.as_array())
                                && !vols.is_empty() {
                                    println!("{}Volumes:", indent);
                                    for v in vols {
                                        if let Some(s) = v.as_str() {
                                            println!("{}  {}", indent, s);
                                        }
                                    }
                                }
                            if let Some(alt) = body.get("curated_alternative")
                                && !alt.is_null() {
                                    let alt_name = alt
                                        .get("offering_name")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("?");
                                    let alt_desc = alt
                                        .get("description")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("");
                                    println!();
                                    println!(
                                        "{}{} A curated manifest '{}' exists for this image.",
                                        indent,
                                        ui::status_indicator("info", term.supports_color),
                                        alt_name
                                    );
                                    if !alt_desc.is_empty() {
                                        println!("{}  {}", indent, alt_desc);
                                    }
                                    println!("{}  Use: garden-rake offer {}", indent, alt_name);
                                }
                            println!();
                        } else {
                            let msg = body
                                .get("error")
                                .and_then(|e| e.get("message"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("Inspection failed");
                            println!(
                                "{}{} {}",
                                " ".repeat(ui::constants::DEFAULT_INDENT),
                                ui::status_indicator("error", term.supports_color),
                                msg
                            );
                        }
                    } else {
                        // Deploy: construct image-direct FQN and POST to services API
                        let fqn_string = if let Some(inst) = instance {
                            format!("image:{}::{}", image_ref, inst)
                        } else {
                            format!("image:{}", image_ref)
                        };

                        let payload = serde_json::json!({
                            "offering": fqn_string,
                        });

                        let response = api.services().create(&payload).await?;
                        let status = response.status();
                        let body: serde_json::Value = response.json().await?;

                        let indent = " ".repeat(ui::constants::DEFAULT_INDENT);

                        if status.is_success() {
                            let svc = body
                                .get("data")
                                .and_then(|d| d.get("service"))
                                .and_then(|v| v.as_str())
                                .unwrap_or(image_ref);
                            let api_status = body
                                .get("data")
                                .and_then(|d| d.get("status"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("accepted");
                            let message = body
                                .get("data")
                                .and_then(|d| d.get("message"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("");

                            println!();
                            println!("{}{}  [{}]", indent, svc, api_status);
                            if !message.is_empty() {
                                println!("{}{}", indent, message);
                            }
                            println!();
                        } else {
                            let msg = body
                                .get("error")
                                .and_then(|e| e.get("message"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("Deployment failed");
                            println!(
                                "{}{} {}",
                                indent,
                                ui::status_indicator("error", term.supports_color),
                                msg
                            );
                        }
                    }
                }
            }

            Ok(())
        })
    }
}
