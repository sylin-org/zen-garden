//! Observe command - garden overview with topology cache
//!
//! Displays a comprehensive view of all stones in the garden:
//! - Default: Queries tended Moss's topology cache (populated by chirps)
//! - Fresh mode (--fresh): Triggers UDP discovery for real-time network scan
//! - Fallback to Lantern registry if available

use crate::command_manifest::cmd;
use crate::commands::{Command, CommandResult};
use crate::context::Runtime;
use crate::discovery;
use crate::suggestions;
use crate::tending;
use crate::ui::colors::CliFormatter;
use crate::ui::layout::{IndentLevel, Layout};
use crate::ui::rendering as ui;
use colored::Colorize;
use garden_common::{GardenApiResponse, TopologyEntry};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// Global counter for stones displayed (for footer)
static STONE_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Max offerings shown per row before truncation (+N)
const MAX_OFFERINGS_SHOWN: usize = 3;

/// Column widths for compact table
const COL_NAME: usize = 24;
const COL_OS: usize = 4;
const COL_CORES: usize = 5;
const COL_MEM: usize = 7;
const COL_AI: usize = 16;

/// Observe command for garden overview
pub struct ObserveCommand {
    pub stone_filter: Option<String>,
    pub offering_filter: Option<String>,
    pub quiet: bool,
}

impl ObserveCommand {
    pub fn new(stone_filter: Option<String>, offering_filter: Option<String>, quiet: bool) -> Self {
        Self {
            stone_filter,
            offering_filter,
            quiet,
        }
    }
}

impl Command for ObserveCommand {
    fn execute<'a>(&'a self, ctx: &'a Runtime) -> std::pin::Pin<Box<dyn std::future::Future<Output = CommandResult> + Send + 'a>> {
        Box::pin(async move {
            observe_garden(ctx, self.stone_filter.clone(), self.offering_filter.clone()).await?;

            // Self-teaching suggestions
            suggestions::print_suggestions(cmd::OBSERVE, self.quiet);

            Ok(())
        })
    }

    fn requires_endpoint(&self) -> bool {
        false // Observe discovers all stones, doesn't need a specific endpoint
    }

    fn name(&self) -> &'static str {
        cmd::OBSERVE
    }
}

/// Main observe implementation
async fn observe_garden(
    ctx: &Runtime,
    stone_filter: Option<String>,
    offering_filter: Option<String>,
) -> anyhow::Result<()> {
    // Reset stone counter
    STONE_COUNT.store(0, Ordering::SeqCst);

    // Keep offering_filter as-is for Lantern call, create offerings_filter for legacy code
    let offerings_filter: Option<Vec<String>> = offering_filter
        .as_ref()
        .map(|s| s.split(',').map(|o| o.trim().to_lowercase()).collect());

    // Get currently tended stone name for marking (compare by name, not endpoint)
    let tended_state = tending::read_tending().ok();
    let tended_stone_name = tended_state
        .as_ref()
        .filter(|s| s.is_valid())
        .map(|s| s.stone_name.clone());

    // Display header immediately (no waiting for discovery)
    let layout = Layout::new();

    // Verbose mode diagnostics
    if ctx.verbose > 0 {
        layout.blank();
        layout
            .line(&format!("Verbose mode enabled (level {})", ctx.verbose))
            .level(IndentLevel::Card)
            .tag("verbose")
            .print();
        if let Some(ref tended) = tended_state {
            layout
                .field("Tending")
                .value(format!("{} at {}", tended.stone_name, tended.endpoint))
                .level(IndentLevel::Card)
                .tag("verbose")
                .print();
        } else {
            layout
                .field("Tending")
                .value("none")
                .level(IndentLevel::Card)
                .tag("verbose")
                .print();
        }
        layout
            .field("Fresh mode")
            .value(if ctx.fresh { "enabled" } else { "disabled" })
            .level(IndentLevel::Card)
            .tag("verbose")
            .print();
    }

    // Header is printed by display functions (print_summary_header) with stone counts

    // Fresh mode: For detailed stone info with resource metrics
    // Requires UDP discovery + HTTP fetches per stone (not yet implemented)
    if ctx.fresh {
        layout
            .status("Fresh mode not yet supported, using topology cache")
            .level(IndentLevel::Card)
            .warn()
            .print();
    }

    // Use execute_on_stone to handle tended + mDNS fallback with SoC
    let topology_result = tending::execute_on_stone(
        Duration::from_secs(3),
        Some(|stone_name: &str| {
            Layout::new()
                .status(&format!(
                    "Stone \"{}\" is sleeping (offline). Picking a new stone...",
                    stone_name
                ))
                .level(IndentLevel::Card)
                .warn()
                .print();
        }),
        |candidate| {
            let client = ctx.client.clone();
            let verbose = ctx.verbose;
            let stone_name = candidate.stone_name.clone();
            let endpoint = candidate.endpoint.clone();
            async move {
                use crate::tending::StoneError;

                let layout = Layout::new();
                layout
                    .line(&format!("querying topology from {}...", stone_name))
                    .level(IndentLevel::Card)
                    .print();
                layout.blank();

                let topology_url =
                    format!("{}/api/v1/garden/topology", endpoint.trim_end_matches('/'));

                if verbose > 0 {
                    layout
                        .field("GET")
                        .value(&topology_url)
                        .level(IndentLevel::Card)
                        .tag("verbose")
                        .print();
                }

                let response = client
                    .get(&topology_url)
                    .timeout(Duration::from_secs(5))
                    .send()
                    .await
                    .map_err(|e| {
                        if verbose > 0 {
                            layout
                                .field("Connection error")
                                .value(e.to_string())
                                .level(IndentLevel::Card)
                                .tag("verbose")
                                .print();
                        }
                        StoneError::ConnectionFailed(format!("Failed to reach stone: {}", e))
                    })?;

                let status = response.status();
                if !status.is_success() {
                    if verbose > 0 {
                        layout
                            .field("Response status")
                            .value(status.to_string())
                            .level(IndentLevel::Card)
                            .tag("verbose")
                            .print();
                    }
                    return Err(StoneError::ResponseError(
                        status.as_u16(),
                        format!("Stone returned {}", status),
                    ));
                }

                let api_response = response
                    .json::<GardenApiResponse<Vec<TopologyEntry>>>()
                    .await
                    .map_err(|e| {
                        tracing::warn!(error = ?e, "Failed to parse topology JSON");
                        if verbose > 0 {
                            layout
                                .field("JSON parse error")
                                .value(e.to_string())
                                .level(IndentLevel::Card)
                                .tag("verbose")
                                .print();
                        }
                        StoneError::ProcessingError(format!("JSON parse failed: {}", e))
                    })?;

                if verbose > 0 {
                    layout
                        .field("Response")
                        .value(format!("{} stones in topology", api_response.data.len()))
                        .level(IndentLevel::Card)
                        .tag("verbose")
                        .print();
                    for stone in &api_response.data {
                        layout
                            .line(&format!(
                                "- {} (id: {}, endpoint: {}, health: {})",
                                stone.stone_name, stone.stone_id, stone.address, stone.health
                            ))
                            .level(IndentLevel::Section)
                            .tag("verbose")
                            .print();
                    }
                    layout.blank();
                }
                Ok(api_response.data)
            }
        },
    )
    .await;

    // If execute_on_stone succeeded, display and return
    if let Ok((stones, responding_stone)) = topology_result {
        display_topology_compact(
            &stones,
            &stone_filter,
            &offerings_filter,
            Some(&responding_stone.stone_name),
            ctx.verbose,
        );
        return Ok(());
    }

    // Final fallback: Try Lantern registry
    discovery::discover_lantern_background();
    let lantern_endpoint = discovery::get_cached_lantern();

    if let Some(ref lantern) = lantern_endpoint {
        tracing::info!(endpoint = %lantern, "Using cached Lantern endpoint for topology queries");

        let topology_url = format!("{}/api/v1/stones", lantern);
        match ctx
            .client
            .get(&topology_url)
            .timeout(Duration::from_secs(5))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(topology) = resp.json::<garden_common::LanternTopology>().await {
                    display_lantern_compact(
                        &topology,
                        offering_filter.as_deref(),
                        tended_stone_name.as_deref(),
                    );
                    return Ok(());
                }
            }
            Ok(resp) => {
                tracing::warn!(status = ?resp.status(), "Lantern returned error");
            }
            Err(e) => {
                tracing::warn!(error = ?e, "Failed to reach Lantern");
            }
        }
    }

    // No stones responded
    layout
        .line("No stones available")
        .level(IndentLevel::Card)
        .print();
    layout.blank();
    layout
        .line("Try: garden-rake tend <stone>  (to specify a stone to tend)")
        .level(IndentLevel::Card)
        .tag("hint")
        .print();

    display_footer_empty();
    Ok(())
}

// ── Compact table display ────────────────────────────────────────────

/// Build the summary header: "GARDEN OBSERVE — 7 stones, all thriving"
/// or "GARDEN OBSERVE — 7 stones (5 thriving, 1 degraded, 1 dormant)"
fn print_summary_header(count: usize, health_counts: &HealthCounts, term: &ui::TerminalInfo) {
    let fmt = CliFormatter::new();
    let indent = " ".repeat(ui::constants::DEFAULT_INDENT);

    let summary = if count == 0 {
        "no stones".to_string()
    } else if health_counts.all_thriving() {
        let label = if count == 1 { "stone" } else { "stones" };
        if term.supports_color {
            format!("{} {}, {}", count, label, "all thriving".green())
        } else {
            format!("{} {}, all thriving", count, label)
        }
    } else {
        let label = if count == 1 { "stone" } else { "stones" };
        let mut parts: Vec<String> = Vec::new();
        if health_counts.thriving > 0 {
            let s = format!("{} thriving", health_counts.thriving);
            parts.push(if term.supports_color {
                s.green().to_string()
            } else {
                s
            });
        }
        if health_counts.degraded > 0 {
            let s = format!("{} degraded", health_counts.degraded);
            parts.push(if term.supports_color {
                s.yellow().to_string()
            } else {
                s
            });
        }
        if health_counts.withering > 0 {
            let s = format!("{} withering", health_counts.withering);
            parts.push(if term.supports_color {
                s.red().to_string()
            } else {
                s
            });
        }
        if health_counts.dormant > 0 {
            let s = format!("{} dormant", health_counts.dormant);
            parts.push(if term.supports_color {
                s.truecolor(128, 128, 128).to_string()
            } else {
                s
            });
        }
        format!("{} {} ({})", count, label, parts.join(", "))
    };

    println!();
    println!(
        "{}{} \u{2014} {}",
        indent,
        fmt.title("GARDEN OBSERVE"),
        summary
    );
}

/// Health classification counters
struct HealthCounts {
    thriving: usize,
    degraded: usize,
    withering: usize,
    dormant: usize,
}

impl HealthCounts {
    fn new() -> Self {
        Self {
            thriving: 0,
            degraded: 0,
            withering: 0,
            dormant: 0,
        }
    }

    fn add(&mut self, health: &str) {
        match ui::classify_health(health) {
            ui::VitalityClass::Thriving => self.thriving += 1,
            ui::VitalityClass::Degraded => self.degraded += 1,
            ui::VitalityClass::Withering => self.withering += 1,
            ui::VitalityClass::Dormant => self.dormant += 1,
        }
    }

    fn all_thriving(&self) -> bool {
        self.degraded == 0 && self.withering == 0 && self.dormant == 0
    }
}

/// Print the table header row
fn print_table_header(_term: &ui::TerminalInfo, table_width: usize) {
    let fmt = CliFormatter::new();
    let indent = " ".repeat(ui::constants::DEFAULT_INDENT);
    let divider = "\u{2500}".repeat(table_width);

    println!("{}{}", indent, fmt.divider(&divider));

    let header = format!(
        " {:<name_w$} {:>os_w$}  {:>cores_w$}  {:>mem_w$}  {:<ai_w$}  {}",
        "NAME",
        "OS",
        "CORES",
        "MEM",
        "AI",
        "OFFERINGS",
        name_w = COL_NAME,
        os_w = COL_OS,
        cores_w = COL_CORES,
        mem_w = COL_MEM,
        ai_w = COL_AI,
    );
    println!("{}{}", indent, fmt.group(&header));
    println!("{}{}", indent, fmt.divider(&divider));
}

/// Print a single stone row in the compact table
#[expect(clippy::too_many_arguments)]
fn print_stone_row(
    name: &str,
    health: &str,
    is_tended: bool,
    os_str: &str,
    cores: usize,
    mem_gb: u64,
    ai: &str,
    offerings: &str,
    term: &ui::TerminalInfo,
) {
    let indent = " ".repeat(ui::constants::DEFAULT_INDENT);

    // Status prefix: symbol only when NOT thriving
    let status_sym = ui::compact_status_symbol(health, term.supports_unicode);
    let tended_mark = if is_tended {
        if term.supports_color {
            ""
        } else {
            "*"
        }
    } else {
        ""
    };

    // Compose the prefix column (status + tended markers)
    let prefix = match (status_sym, tended_mark) {
        (Some(sym), "*") => format!("{}{}", sym, tended_mark),
        (Some(sym), _) => format!("{} ", sym),
        (None, "*") => " *".to_string(),
        _ => "  ".to_string(),
    };

    // Color the stone name by vitality (tended = gold)
    let colored_name = ui::colored_stone_name(name, health, is_tended, term.supports_color);
    let name_padded = ui::pad_visible(&colored_name, COL_NAME);

    // OS indicator
    let os_icon = ui::os_indicator(os_str, term.supports_unicode);

    // Color the status symbol if present
    let colored_prefix = if term.supports_color {
        match ui::classify_health(health) {
            ui::VitalityClass::Thriving => prefix.clone(),
            ui::VitalityClass::Degraded => prefix.yellow().to_string(),
            ui::VitalityClass::Withering => prefix.red().to_string(),
            ui::VitalityClass::Dormant => prefix.truecolor(128, 128, 128).to_string(),
        }
    } else {
        prefix
    };

    println!(
        "{}{}{} {:>os_w$}  {:>cores_w$}  {:>mem_w$}  {:<ai_w$}  {}",
        indent,
        colored_prefix,
        name_padded,
        os_icon,
        cores,
        format!("{} GB", mem_gb),
        ai,
        offerings,
        os_w = COL_OS,
        cores_w = COL_CORES,
        mem_w = COL_MEM,
        ai_w = COL_AI,
    );
}

/// Print footer with adaptive legend
fn print_table_footer(
    has_tended: bool,
    has_windows: bool,
    has_linux: bool,
    table_width: usize,
    term: &ui::TerminalInfo,
) {
    let fmt = CliFormatter::new();
    let indent = " ".repeat(ui::constants::DEFAULT_INDENT);
    let divider = "\u{2500}".repeat(table_width);

    println!("{}{}", indent, fmt.divider(&divider));

    let legend = ui::compact_legend(has_tended, has_windows, has_linux, term);
    println!("{} {}", indent, fmt.hint(&legend));

    println!();
    println!(
        "{} {}",
        indent,
        fmt.hint("garden-rake <stone>? for details")
    );
}

/// Display footer when no stones are available
fn display_footer_empty() {
    // No-op: summary header already communicates the stone count
}

// ── Topology display (primary path) ─────────────────────────────────

/// Display stones from topology API as a compact table.
///
/// Uses data already in the topology cache — NO HTTP calls per stone.
fn display_topology_compact(
    stones: &[TopologyEntry],
    stone_filter: &Option<String>,
    offerings_filter: &Option<Vec<String>>,
    tended_stone_name: Option<&str>,
    verbose: u8,
) {
    let layout = Layout::new();
    let term = ui::TerminalInfo::detect();

    if stones.is_empty() {
        print_summary_header(0, &HealthCounts::new(), &term);
        layout.blank();
        layout
            .line("No stones in topology cache")
            .level(IndentLevel::Card)
            .print();
        layout
            .line("Try: garden-rake observe --fresh  (to scan network)")
            .level(IndentLevel::Card)
            .tag("hint")
            .print();
        return;
    }

    // Filter stones if name specified
    let filtered_stones: Vec<&TopologyEntry> = if let Some(filter_name) = stone_filter {
        stones
            .iter()
            .filter(|s| s.stone_name.eq_ignore_ascii_case(filter_name))
            .collect()
    } else {
        stones.iter().collect()
    };

    if filtered_stones.is_empty() && stone_filter.is_some() {
        print_summary_header(0, &HealthCounts::new(), &term);
        layout
            .status(&format!(
                "Stone '{}' not found in topology",
                stone_filter.as_ref().unwrap()
            ))
            .level(IndentLevel::Card)
            .error()
            .print();
        return;
    }

    // Collect displayable stones (those with capabilities data)
    let displayable: Vec<&TopologyEntry> = filtered_stones
        .iter()
        .filter(|s| {
            if s.capabilities.is_none() && verbose > 0 {
                layout
                    .status(&format!(
                        "Stone {} has no capabilities data (may be offline)",
                        s.stone_name
                    ))
                    .level(IndentLevel::Card)
                    .tag("verbose")
                    .print();
            }
            s.capabilities.is_some()
        })
        .copied()
        .collect();

    // Compute health summary
    let mut health_counts = HealthCounts::new();
    let mut has_tended = false;
    let mut has_windows = false;
    let mut has_linux = false;

    for stone in &displayable {
        // Offline stones always count as dormant regardless of last-known health
        let effective_health = if stone.status == garden_common::StoneStatus::Offline {
            garden_common::constants::VITALITY_DORMANT
        } else {
            &stone.health
        };
        health_counts.add(effective_health);
        let is_tended = tended_stone_name
            .map(|t| t.eq_ignore_ascii_case(&stone.stone_name))
            .unwrap_or(false);
        if is_tended {
            has_tended = true;
        }

        if let Some(ref caps) = stone.capabilities
            && let Some(ref rt) = caps.runtime {
                let family = ui::os_family_from_runtime(&rt.os);
                if family.starts_with("windows") || family.starts_with("microsoft") {
                    has_windows = true;
                } else {
                    has_linux = true;
                }
            }
    }

    STONE_COUNT.store(displayable.len(), Ordering::SeqCst);

    // Print header + table
    let table_width = COL_NAME + COL_OS + COL_CORES + COL_MEM + COL_AI + 20 + 12; // padding + OFFERINGS label room
    print_summary_header(displayable.len(), &health_counts, &term);
    print_table_header(&term, table_width);

    for stone in &displayable {
        let caps = stone.capabilities.as_ref().unwrap();
        let is_tended = tended_stone_name
            .map(|t| t.eq_ignore_ascii_case(&stone.stone_name))
            .unwrap_or(false);

        // Offline stones always render as dormant
        let effective_health = if stone.status == garden_common::StoneStatus::Offline {
            garden_common::constants::VITALITY_DORMANT
        } else {
            &stone.health
        };

        let os_str = caps
            .runtime
            .as_ref()
            .map(|r| ui::os_family_from_runtime(&r.os))
            .unwrap_or("unknown");

        let cores = caps.hardware.cpu.cores;
        let mem_gb = caps.hardware.memory.total_mb / 1024;
        let ai = ui::compact_ai(caps);

        // Filter offerings if needed
        let filtered_services: Vec<_> = if let Some(filters) = offerings_filter {
            stone
                .services
                .iter()
                .filter(|s| filters.contains(&s.offering.to_lowercase()))
                .collect()
        } else {
            stone.services.iter().collect()
        };

        let offerings_text = if offerings_filter.is_some() && filtered_services.is_empty() {
            "\u{2014}".to_string()
        } else {
            // Use TopologyServiceEntry vec for compact_offerings
            let svc_refs: Vec<garden_common::TopologyServiceEntry> =
                filtered_services.iter().map(|s| (*s).clone()).collect();
            ui::compact_offerings(&svc_refs, MAX_OFFERINGS_SHOWN)
        };

        print_stone_row(
            &stone.stone_name,
            effective_health,
            is_tended,
            os_str,
            cores,
            mem_gb,
            &ai,
            &offerings_text,
            &term,
        );
    }

    print_table_footer(has_tended, has_windows, has_linux, table_width, &term);
}

// ── Lantern fallback display ─────────────────────────────────────────

/// Display topology from Lantern registry as a compact table.
///
/// Lantern has less data than topology (no capabilities), so some columns
/// show abbreviated info.
fn display_lantern_compact(
    topology: &garden_common::LanternTopology,
    offering_filter: Option<&str>,
    tended_stone_name: Option<&str>,
) {
    let term = ui::TerminalInfo::detect();

    if topology.stones.is_empty() {
        print_summary_header(0, &HealthCounts::new(), &term);
        let indent = " ".repeat(ui::constants::DEFAULT_INDENT);
        println!("{}No stones registered", indent);
        return;
    }

    // Compute summary
    let mut health_counts = HealthCounts::new();
    let mut has_tended = false;

    for stone in &topology.stones {
        health_counts.add(&stone.status);
        let is_tended = tended_stone_name
            .map(|t| t.eq_ignore_ascii_case(&stone.name))
            .unwrap_or(false);
        if is_tended {
            has_tended = true;
        }
    }

    STONE_COUNT.store(topology.stones.len(), Ordering::SeqCst);

    let table_width = COL_NAME + COL_OS + COL_CORES + COL_MEM + COL_AI + 20 + 12;
    print_summary_header(topology.stones.len(), &health_counts, &term);
    print_table_header(&term, table_width);

    for stone in &topology.stones {
        let is_tended = tended_stone_name
            .map(|t| t.eq_ignore_ascii_case(&stone.name))
            .unwrap_or(false);

        // Lantern doesn't have capabilities — show what we can
        let filtered_services: Vec<_> = if let Some(filter) = offering_filter {
            stone
                .services
                .iter()
                .filter(|s| {
                    s.name.to_lowercase().contains(&filter.to_lowercase())
                        || s.service_type
                            .to_lowercase()
                            .contains(&filter.to_lowercase())
                })
                .collect()
        } else {
            stone.services.iter().collect()
        };

        let offerings_text = if filtered_services.is_empty() {
            "\u{2014}".to_string()
        } else {
            let names: Vec<&str> = filtered_services.iter().map(|s| s.name.as_str()).collect();
            if names.len() <= MAX_OFFERINGS_SHOWN {
                names.join(" ")
            } else {
                let shown: Vec<&str> = names[..MAX_OFFERINGS_SHOWN].to_vec();
                format!("{} +{}", shown.join(" "), names.len() - MAX_OFFERINGS_SHOWN)
            }
        };

        // Lantern lacks hardware info — use dashes
        print_stone_row(
            &stone.name,
            &stone.status,
            is_tended,
            "unknown",
            0,
            0,
            "\u{2014}",
            &offerings_text,
            &term,
        );
    }

    // Lantern doesn't know OS — only show what we know
    print_table_footer(has_tended, false, false, table_width, &term);
}
