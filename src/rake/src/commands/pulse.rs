//! Pulse command — permanent terminal monitor
//!
//! Full-screen, unattended live display for stone observability.
//! Designed for dedicated Linux screens (tty1, OLED sidecar, wall monitor).
//!
//! Consumes the presence stream for domain events and polls topology
//! for garden-wide awareness. Adapts to any terminal geometry.
//!
//! See: PULSE-0001 ADR

use crate::command_manifest::cmd;
use crate::commands::{Command, CommandResult};
use crate::context::CommandContext;
use async_trait::async_trait;
use colored::Colorize;
use futures_util::StreamExt;
use garden_common::presence::{
    OfferingState, PresenceSnapshot, StoneLoadUpdatedPayload, StoneState,
};
use garden_common::ui::gauge;
use garden_common::{GardenApiResponse, TopologyEntry};
use std::collections::VecDeque;
use std::io::Write;
use std::time::Duration;

/// Maximum events in the ring buffer
const MAX_EVENTS: usize = 200;

/// Topology poll interval
const TOPOLOGY_POLL_SECS: u64 = 10;

/// Maximum redraw rate (milliseconds between redraws)
const MIN_REDRAW_INTERVAL_MS: u64 = 500;

/// Reconnection backoff bounds
const RECONNECT_MIN_MS: u64 = 1000;
const RECONNECT_MAX_MS: u64 = 30_000;

/// Pulse monitor command
pub struct PulseCommand {
    pub quiet_mode: bool,
}

impl PulseCommand {
    pub fn new(quiet_mode: bool) -> Self {
        Self { quiet_mode }
    }
}

#[async_trait]
impl Command for PulseCommand {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        let endpoint = ctx.endpoint()?.to_string();
        run_pulse_monitor(&ctx.client, &endpoint, &ctx.term).await
    }

    fn show_stone_header(&self) -> bool {
        false // pulse takes over the entire screen
    }

    fn name(&self) -> &'static str {
        cmd::PULSE
    }
}

// =============================================================================
// Monitor state
// =============================================================================

/// Current state of the pulse monitor
struct MonitorState {
    stone: Option<StoneState>,
    offerings: Vec<OfferingState>,
    events: VecDeque<EventLine>,
    topology: Vec<TopologyEntry>,
    connection_status: ConnectionStatus,
    stone_name: String,
}

/// A single event line for the feed
struct EventLine {
    time: String,    // "HH:MM:SS" or "HH:MM"
    entity: String,  // offering name or "stone"
    message: String, // human-readable
    level: EventLevel,
}

#[derive(Clone, Copy)]
enum EventLevel {
    Info,
    Warn,
    Error,
    Dim,
}

enum ConnectionStatus {
    Connected,
    Reconnecting { wait_secs: u64 },
    Connecting,
}

impl MonitorState {
    fn new() -> Self {
        Self {
            stone: None,
            offerings: Vec::new(),
            events: VecDeque::with_capacity(MAX_EVENTS),
            topology: Vec::new(),
            connection_status: ConnectionStatus::Connecting,
            stone_name: String::new(),
        }
    }

    fn push_event(&mut self, event: EventLine) {
        if self.events.len() >= MAX_EVENTS {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }

    fn apply_snapshot(&mut self, snapshot: PresenceSnapshot) {
        self.stone_name = snapshot.stone.name.clone();
        self.stone = Some(snapshot.stone);
        self.offerings = snapshot.offerings;
        self.connection_status = ConnectionStatus::Connected;
    }

    fn apply_load_update(&mut self, payload: StoneLoadUpdatedPayload) {
        if let Some(ref mut stone) = self.stone {
            stone.cpu_percent = payload.cpu_percent;
            stone.memory_percent = payload.memory_percent;
            stone.disk_percent = payload.disk_percent;
            stone.gpu_percent = payload.gpu_percent;
            stone.gpu_active = payload.gpu_active;
            stone.net_rx_bytes_per_sec = payload.net_rx_bytes_per_sec;
            stone.net_tx_bytes_per_sec = payload.net_tx_bytes_per_sec;
        }
    }

    fn apply_health_change(&mut self, health: &str) {
        if let Some(ref mut stone) = self.stone {
            stone.health = health.to_string();
        }
    }

    fn apply_offering_event(&mut self, name: &str, status: &str) {
        if let Some(o) = self.offerings.iter_mut().find(|o| o.name == name) {
            match status {
                "started" | "sprouted" => {
                    o.status = "running".to_string();
                    o.health = "healthy".to_string();
                }
                "stopped" | "uprooted" => {
                    o.status = "stopped".to_string();
                }
                _ => {}
            }
        }
    }

    fn apply_offering_health(&mut self, name: &str, health: &str) {
        if let Some(o) = self.offerings.iter_mut().find(|o| o.name == name) {
            o.health = health.to_string();
        }
    }
}

// =============================================================================
// Main monitor loop
// =============================================================================

async fn run_pulse_monitor(
    client: &reqwest::Client,
    endpoint: &str,
    term: &garden_common::ui::rendering::TerminalInfo,
) -> CommandResult {
    let mut state = MonitorState::new();
    let mut backoff_ms = RECONNECT_MIN_MS;

    // Presence stream URL
    let presence_url = format!(
        "{}/api/v1/stone/presence/stream",
        endpoint.trim_end_matches('/')
    );
    let topology_url = format!(
        "{}/api/v1/garden/topology",
        endpoint.trim_end_matches('/')
    );

    loop {
        state.connection_status = ConnectionStatus::Connecting;
        render_frame(&state, term);

        // Try to connect
        let connect_result = client
            .get(&presence_url)
            .header("Accept", "text/event-stream")
            .timeout(Duration::from_secs(10))
            .send()
            .await;

        match connect_result {
            Ok(response) if response.status().is_success() => {
                backoff_ms = RECONNECT_MIN_MS; // reset backoff on success
                state.connection_status = ConnectionStatus::Connected;

                // Run the streaming loop
                let disconnected = stream_loop(
                    &mut state,
                    response,
                    client,
                    &topology_url,
                    term,
                )
                .await;

                if !disconnected {
                    // Clean exit (e.g., Ctrl+C propagation from server)
                    return Ok(());
                }

                // Connection lost — fall through to reconnect
                state.push_event(EventLine {
                    time: wall_clock(),
                    entity: "connection".to_string(),
                    message: "lost".to_string(),
                    level: EventLevel::Warn,
                });
            }
            Ok(response) => {
                state.push_event(EventLine {
                    time: wall_clock(),
                    entity: "connection".to_string(),
                    message: format!("HTTP {}", response.status()),
                    level: EventLevel::Error,
                });
            }
            Err(e) => {
                state.push_event(EventLine {
                    time: wall_clock(),
                    entity: "connection".to_string(),
                    message: format!("{}", e),
                    level: EventLevel::Error,
                });
            }
        }

        // Reconnect with backoff
        let wait_secs = backoff_ms / 1000;
        state.connection_status = ConnectionStatus::Reconnecting {
            wait_secs: wait_secs.max(1),
        };
        render_frame(&state, term);

        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
        backoff_ms = (backoff_ms * 2).min(RECONNECT_MAX_MS);
    }
}

/// Stream events until disconnection. Returns true if disconnected (should reconnect).
async fn stream_loop(
    state: &mut MonitorState,
    response: reqwest::Response,
    client: &reqwest::Client,
    topology_url: &str,
    term: &garden_common::ui::rendering::TerminalInfo,
) -> bool {
    let mut stream = response.bytes_stream();
    let mut sse_buffer = String::new();
    let mut last_render = std::time::Instant::now();
    let mut topology_interval = tokio::time::interval(Duration::from_secs(TOPOLOGY_POLL_SECS));
    topology_interval.tick().await; // skip first immediate tick
    let mut dirty = false;

    loop {
        tokio::select! {
            chunk = stream.next() => {
                match chunk {
                    Some(Ok(bytes)) => {
                        sse_buffer.push_str(&String::from_utf8_lossy(&bytes));

                        // Process complete SSE messages
                        while let Some(pos) = sse_buffer.find("\n\n") {
                            let message = sse_buffer[..pos].to_string();
                            sse_buffer.drain(..pos + 2);
                            process_sse_message(state, &message);
                            dirty = true;
                        }
                    }
                    Some(Err(_)) | None => {
                        // Disconnected
                        return true;
                    }
                }
            }

            _ = topology_interval.tick() => {
                // Poll topology in background
                if let Ok(entries) = fetch_topology(client, topology_url).await {
                    state.topology = entries;
                    dirty = true;
                }
            }

            _ = tokio::signal::ctrl_c() => {
                // Clean exit — restore terminal and exit
                // Move cursor below the frame so the shell prompt appears cleanly
                let (_, rows) = terminal_size();
                println!("\x1b[{};1H\x1b[0m", rows);
                let _ = std::io::stdout().flush();
                std::process::exit(0);
            }
        }

        // Throttle redraws
        if dirty && last_render.elapsed() >= Duration::from_millis(MIN_REDRAW_INTERVAL_MS) {
            render_frame(state, term);
            dirty = false;
            last_render = std::time::Instant::now();
        }
    }
}

/// Process a single SSE message and update monitor state.
fn process_sse_message(state: &mut MonitorState, message: &str) {
    let mut event_type = String::new();
    let mut data = String::new();

    for line in message.lines() {
        if let Some(et) = line.strip_prefix("event: ") {
            event_type = et.to_string();
        } else if let Some(d) = line.strip_prefix("data: ") {
            data.push_str(d);
        }
    }

    if data.is_empty() {
        return;
    }

    match event_type.as_str() {
        "presence.snapshot" => {
            if let Ok(snapshot) = serde_json::from_str::<PresenceSnapshot>(&data) {
                state.apply_snapshot(snapshot);
            }
        }
        "stone.load.updated" => {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&data) {
                if let Some(payload_value) = parsed.get("data") {
                    if let Ok(payload) =
                        serde_json::from_value::<StoneLoadUpdatedPayload>(payload_value.clone())
                    {
                        state.apply_load_update(payload);

                        // Add event line with inline values
                        let cpu = state.stone.as_ref().map(|s| s.cpu_percent).unwrap_or(0.0);
                        let mem = state
                            .stone
                            .as_ref()
                            .map(|s| s.memory_percent)
                            .unwrap_or(0.0);
                        state.push_event(EventLine {
                            time: extract_time(&parsed),
                            entity: "stone".to_string(),
                            message: format!("load updated  cpu {:.0} mem {:.0}", cpu, mem),
                            level: EventLevel::Dim,
                        });
                    }
                }
            }
        }
        "stone.health.changed" => {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&data) {
                let health = parsed
                    .get("data")
                    .and_then(|d| d.get("health"))
                    .and_then(|h| h.as_str())
                    .unwrap_or("unknown");
                state.apply_health_change(health);

                let level = match health {
                    "thriving" => EventLevel::Info,
                    "withering" => EventLevel::Warn,
                    "wilting" => EventLevel::Error,
                    _ => EventLevel::Info,
                };
                state.push_event(EventLine {
                    time: extract_time(&parsed),
                    entity: "stone".to_string(),
                    message: format!("health changed  {}", health),
                    level,
                });
            }
        }
        "service.started" | "service.stopped" | "service.sprouted" | "service.uprooted"
        | "service.updated" | "service.renamed" => {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&data) {
                let entity = parsed
                    .get("service")
                    .and_then(|s| s.as_str())
                    .unwrap_or("unknown");
                let action = event_type
                    .strip_prefix("service.")
                    .unwrap_or(&event_type);
                state.apply_offering_event(entity, action);
                let level = match action {
                    "stopped" | "uprooted" => EventLevel::Warn,
                    _ => EventLevel::Info,
                };
                state.push_event(EventLine {
                    time: extract_time(&parsed),
                    entity: entity.to_string(),
                    message: action.to_string(),
                    level,
                });
            }
        }
        "service.health.changed" => {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&data) {
                let entity = parsed
                    .get("service")
                    .and_then(|s| s.as_str())
                    .unwrap_or("unknown");
                let health = parsed
                    .get("data")
                    .and_then(|d| d.get("health"))
                    .and_then(|h| h.as_str())
                    .unwrap_or("unknown");
                state.apply_offering_health(entity, health);
                let level = if health == "healthy" {
                    EventLevel::Info
                } else {
                    EventLevel::Warn
                };
                state.push_event(EventLine {
                    time: extract_time(&parsed),
                    entity: entity.to_string(),
                    message: format!("health {}", health),
                    level,
                });
            }
        }
        "stone.tended" => {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&data) {
                let msg = parsed
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("tended");
                state.push_event(EventLine {
                    time: extract_time(&parsed),
                    entity: "stone".to_string(),
                    message: msg.to_string(),
                    level: EventLevel::Info,
                });
            }
        }
        _ => {
            // Generic event — show message if available
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&data) {
                let msg = parsed
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or(&event_type);
                let entity = parsed
                    .get("service")
                    .and_then(|s| s.as_str())
                    .unwrap_or("stone");
                state.push_event(EventLine {
                    time: extract_time(&parsed),
                    entity: entity.to_string(),
                    message: msg.to_string(),
                    level: EventLevel::Dim,
                });
            }
        }
    }
}

// =============================================================================
// Rendering
// =============================================================================

/// Render the full monitor frame to stdout.
fn render_frame(
    state: &MonitorState,
    term: &garden_common::ui::rendering::TerminalInfo,
) {
    let (cols, rows) = terminal_size();
    let color = term.supports_color;

    // Build entire frame in a buffer, then flush once
    let mut buf = String::with_capacity(cols * rows);

    // Clear screen + cursor home
    buf.push_str("\x1b[2J\x1b[H");

    let mut row = 0;

    // --- Region 1: Header (1 line) ---
    row += render_header(&mut buf, state, cols, color);

    // --- Region 2: Gauges (1-3 lines) ---
    if row < rows.saturating_sub(4) {
        row += render_gauges(&mut buf, state, cols, color);
    }

    // --- Region 3: Offerings (1 line) ---
    if row < rows.saturating_sub(3) && !state.offerings.is_empty() {
        row += render_offerings(&mut buf, state, cols, color);
    }

    // --- Region 4: Divider ---
    if row < rows.saturating_sub(2) {
        let divider: String = if term.supports_unicode {
            "\u{2500}".repeat(cols.min(120))
        } else {
            "-".repeat(cols.min(120))
        };
        if color {
            buf.push_str(&format!(" {}\n", divider.dimmed()));
        } else {
            buf.push_str(&format!(" {}\n", divider));
        }
        row += 1;
    }

    // --- Region 6: Garden (if enough rows, rendered after events) ---
    // Calculate how many rows garden needs, reserve them
    let garden_rows = if rows > 30 && !state.topology.is_empty() {
        // header line + one per stone, capped
        let count = state.topology.len().min(rows / 4);
        count + 2 // "garden ---" header + divider + entries
    } else {
        0
    };

    // --- Region 5: Events (remaining rows) ---
    let events_available = rows.saturating_sub(row).saturating_sub(garden_rows);
    if events_available > 0 {
        row += render_events(&mut buf, state, cols, events_available, color);
    }

    // --- Region 6: Garden peers ---
    if garden_rows > 0 {
        let divider: String = if term.supports_unicode {
            "\u{2500}".repeat(cols.min(120))
        } else {
            "-".repeat(cols.min(120))
        };
        if color {
            buf.push_str(&format!(" {} {}\n", "garden".dimmed(), divider.dimmed()));
        } else {
            buf.push_str(&format!(" garden {}\n", divider));
        }
        row += 1;
        let garden_space = rows.saturating_sub(row);
        render_garden(&mut buf, state, cols, garden_space, color);
    }

    // Flush entire frame in one write
    let _ = std::io::stdout().write_all(buf.as_bytes());
    let _ = std::io::stdout().flush();
}

/// Render header line. Returns number of rows consumed.
fn render_header(buf: &mut String, state: &MonitorState, cols: usize, color: bool) -> usize {
    let name = if !state.stone_name.is_empty() {
        // In narrow mode, strip "stone-" prefix
        if cols < 60 {
            state.stone_name.strip_prefix("stone-").unwrap_or(&state.stone_name)
        } else {
            &state.stone_name
        }
    } else {
        "connecting..."
    };

    let (health_str, uptime_str) = if let Some(ref stone) = state.stone {
        let health = &stone.health;
        let uptime = garden_common::format_uptime(stone.uptime_seconds);
        (health.clone(), format!("up {}", uptime))
    } else {
        match &state.connection_status {
            ConnectionStatus::Connected => ("connected".to_string(), String::new()),
            ConnectionStatus::Connecting => ("connecting...".to_string(), String::new()),
            ConnectionStatus::Reconnecting { wait_secs } => {
                (format!("reconnecting...  {}s", wait_secs), String::new())
            }
        }
    };

    let right_part = if uptime_str.is_empty() {
        health_str.clone()
    } else {
        format!("{}  {}", health_str, uptime_str)
    };

    let padding = cols.saturating_sub(name.len() + right_part.len() + 2);

    if color {
        let colored_health = match health_str.as_str() {
            "thriving" => health_str.green(),
            "withering" => health_str.yellow(),
            "wilting" => health_str.red(),
            s if s.starts_with("reconnecting") => health_str.yellow(),
            _ => health_str.normal(),
        };

        let right = if uptime_str.is_empty() {
            format!("{}", colored_health)
        } else {
            format!("{}  {}", colored_health, uptime_str.dimmed())
        };
        buf.push_str(&format!(
            " {}{}{}\n",
            name.bold(),
            " ".repeat(padding),
            right,
        ));
    } else {
        buf.push_str(&format!(" {}{}{}\n", name, " ".repeat(padding), right_part));
    }
    1
}

/// Render gauge bars. Returns number of rows consumed.
fn render_gauges(buf: &mut String, state: &MonitorState, cols: usize, color: bool) -> usize {
    let stone = match &state.stone {
        Some(s) => s,
        None => {
            // No data yet — show placeholder
            if color {
                buf.push_str(&format!(" {}\n", "waiting for data...".dimmed()));
            } else {
                buf.push_str(" waiting for data...\n");
            }
            return 1;
        }
    };

    let mut rows = 0;

    if cols >= 80 {
        // Wide: two gauges per line
        let gauge_width = (cols - 4) / 2; // two gauges + spacing
        let cpu = gauge::format_gauge("CPU", stone.cpu_percent, gauge_width, color);
        let mem = gauge::format_gauge("MEM", stone.memory_percent, gauge_width, color);
        buf.push_str(&format!(" {}   {}\n", cpu, mem));
        rows += 1;

        let dsk = gauge::format_gauge("DSK", stone.disk_percent, gauge_width, color);
        if stone.has_gpu {
            let gpu = gauge::format_gauge("GPU", stone.gpu_percent, gauge_width, color);
            buf.push_str(&format!(" {}   {}\n", dsk, gpu));
        } else {
            buf.push_str(&format!(" {}\n", dsk));
        }
        rows += 1;

        // Network rates on separate line if present
        if stone.net_rx_bytes_per_sec > 0 || stone.net_tx_bytes_per_sec > 0 {
            let rx = gauge::format_net_rate(stone.net_rx_bytes_per_sec);
            let tx = gauge::format_net_rate(stone.net_tx_bytes_per_sec);
            let net_line = format!("NET  {} dn  {} up", rx, tx);
            let indent = " ".repeat(gauge_width.saturating_sub(net_line.len()) + 5);
            if color {
                buf.push_str(&format!("{}{}\n", indent, net_line.dimmed()));
            } else {
                buf.push_str(&format!("{}{}\n", indent, net_line));
            }
            rows += 1;
        }
    } else {
        // Narrow: one gauge per line
        let gauge_width = cols.saturating_sub(2);
        buf.push_str(&format!(
            " {}\n",
            gauge::format_gauge("CPU", stone.cpu_percent, gauge_width, color)
        ));
        buf.push_str(&format!(
            " {}\n",
            gauge::format_gauge("MEM", stone.memory_percent, gauge_width, color)
        ));
        buf.push_str(&format!(
            " {}\n",
            gauge::format_gauge("DSK", stone.disk_percent, gauge_width, color)
        ));
        rows += 3;

        if stone.has_gpu {
            buf.push_str(&format!(
                " {}\n",
                gauge::format_gauge("GPU", stone.gpu_percent, gauge_width, color)
            ));
            rows += 1;
        }
    }

    rows
}

/// Render offerings status line. Returns number of rows consumed.
fn render_offerings(buf: &mut String, state: &MonitorState, cols: usize, color: bool) -> usize {
    let mut line = String::from(" ");

    for (count, offering) in state.offerings.iter().enumerate() {
        let status_char = match offering.status.as_str() {
            "running" => {
                if offering.health == "healthy" {
                    "ok"
                } else {
                    "degraded"
                }
            }
            "stopped" | "dormant" => "off",
            _ => "?",
        };

        let entry = if color {
            let colored_status = match status_char {
                "ok" => format!(":{}", "ok".green()),
                "degraded" => format!(":{}", "degraded".yellow()),
                "off" => format!(":{}", "off".dimmed()),
                _ => format!(":{}", status_char),
            };
            format!("{}{}", offering.name, colored_status)
        } else {
            format!("{}:{}", offering.name, status_char)
        };

        // Check if adding this would overflow the line
        // Use approximate visible length (without ANSI)
        let visible_entry_len = offering.name.len() + 1 + status_char.len();
        let separator = if count > 0 { "  " } else { "" };
        let visible_line_len = line.len() + separator.len() + visible_entry_len;

        if visible_line_len > cols && count > 0 {
            let remaining = state.offerings.len() - count;
            if remaining > 0 {
                if color {
                    line.push_str(&format!("  +{}", remaining.to_string().dimmed()));
                } else {
                    line.push_str(&format!("  +{}", remaining));
                }
            }
            break;
        }

        line.push_str(separator);
        line.push_str(&entry);
    }

    buf.push_str(&line);
    buf.push('\n');
    1
}

/// Render event feed. Returns number of rows consumed.
fn render_events(
    buf: &mut String,
    state: &MonitorState,
    cols: usize,
    max_rows: usize,
    color: bool,
) -> usize {
    if state.events.is_empty() {
        return 0;
    }

    // Show most recent events that fit
    let start = state.events.len().saturating_sub(max_rows);
    let visible = &state.events.as_slices();
    let all_events: Vec<&EventLine> = visible.0.iter().chain(visible.1.iter()).collect();
    let display_events = &all_events[start..];

    // Compute entity column width (max entity name length, capped)
    let entity_width = display_events
        .iter()
        .map(|e| e.entity.len())
        .max()
        .unwrap_or(6)
        .clamp(6, 16);

    let mut rows = 0;
    for event in display_events {
        if rows >= max_rows {
            break;
        }

        let time = if cols >= 60 { &event.time } else {
            // Short time: strip seconds if full "HH:MM:SS"
            if event.time.len() > 5 { &event.time[..5] } else { &event.time }
        };
        let entity = &event.entity;
        let padded_entity = if entity.len() < entity_width {
            format!("{}{}", entity, " ".repeat(entity_width - entity.len()))
        } else {
            entity[..entity_width].to_string()
        };

        // Truncate message to fit
        let prefix_len = 1 + time.len() + 2 + entity_width + 2; // " HH:MM:SS  entity  "
        let msg_width = cols.saturating_sub(prefix_len);
        let msg = if event.message.len() > msg_width {
            &event.message[..msg_width]
        } else {
            &event.message
        };

        if color {
            let line = match event.level {
                EventLevel::Info => format!(
                    " {}  {}  {}",
                    time.dimmed(),
                    padded_entity.green(),
                    msg
                ),
                EventLevel::Warn => format!(
                    " {}  {}  {}",
                    time.dimmed(),
                    padded_entity.yellow(),
                    msg.yellow()
                ),
                EventLevel::Error => format!(
                    " {}  {}  {}",
                    time.dimmed(),
                    padded_entity.red(),
                    msg.red()
                ),
                EventLevel::Dim => format!(
                    " {}  {}  {}",
                    time.dimmed(),
                    padded_entity.dimmed(),
                    msg.dimmed()
                ),
            };
            buf.push_str(&line);
        } else {
            buf.push_str(&format!(" {}  {}  {}", time, padded_entity, msg));
        }
        buf.push('\n');
        rows += 1;
    }

    rows
}

/// Render garden peers section.
fn render_garden(
    buf: &mut String,
    state: &MonitorState,
    cols: usize,
    max_rows: usize,
    color: bool,
) {
    let now = chrono::Utc::now();

    for (i, entry) in state.topology.iter().enumerate() {
        if i >= max_rows {
            break;
        }

        let name = if cols < 60 {
            entry
                .stone_name
                .strip_prefix("stone-")
                .unwrap_or(&entry.stone_name)
        } else {
            &entry.stone_name
        };

        let is_self = entry.stone_name == state.stone_name;
        let svc_count = entry.services.len();
        let health_dot = match entry.health.as_str() {
            "thriving" => {
                if color { "●".green().to_string() } else { "o".to_string() }
            }
            "degraded" | "withering" => {
                if color { "●".yellow().to_string() } else { "!".to_string() }
            }
            _ => {
                if color { "○".dimmed().to_string() } else { "-".to_string() }
            }
        };

        let age = if is_self {
            "self".to_string()
        } else {
            let elapsed = now.signed_duration_since(entry.last_seen);
            let secs = elapsed.num_seconds().max(0) as u64;
            if secs < 60 {
                format!("{}s ago", secs)
            } else {
                format!("{}m ago", secs / 60)
            }
        };

        // Offering names (abbreviated)
        let offerings: String = if cols > 80 {
            let names: Vec<&str> = entry
                .services
                .iter()
                .take(3)
                .map(|s| s.name.as_str())
                .collect();
            let extra = entry.services.len().saturating_sub(3);
            let mut s = names.join(" ");
            if extra > 0 {
                s.push_str(&format!(" +{}", extra));
            }
            s
        } else {
            String::new()
        };

        let name_width = 24.min(cols / 3);
        let padded_name = if name.len() < name_width {
            format!("{}{}", name, " ".repeat(name_width - name.len()))
        } else {
            name[..name_width].to_string()
        };

        if color {
            let name_display = if is_self {
                padded_name.bold().to_string()
            } else {
                padded_name.to_string()
            };
            buf.push_str(&format!(
                " {} {}  {} svc  {}  {}\n",
                health_dot,
                name_display,
                svc_count,
                age.dimmed(),
                offerings.dimmed()
            ));
        } else {
            buf.push_str(&format!(
                " {} {}  {} svc  {}  {}\n",
                health_dot, padded_name, svc_count, age, offerings
            ));
        }
    }
}

// =============================================================================
// Helpers
// =============================================================================

/// Get terminal dimensions (cols, rows).
fn terminal_size() -> (usize, usize) {
    terminal_size::terminal_size()
        .map(|(w, h)| (w.0 as usize, h.0 as usize))
        .unwrap_or((80, 24))
}

/// Get current wall clock as "HH:MM:SS".
fn wall_clock() -> String {
    let now = chrono::Local::now();
    now.format("%H:%M:%S").to_string()
}

/// Extract HH:MM:SS from an SSE event's timestamp field.
fn extract_time(parsed: &serde_json::Value) -> String {
    parsed
        .get("timestamp")
        .and_then(|t| t.as_str())
        .and_then(|t| {
            // ISO timestamp: "2026-02-28T14:32:01.123Z" → "14:32:01"
            if t.len() >= 19 {
                Some(t[11..19].to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(wall_clock)
}

/// Fetch topology from stone.
async fn fetch_topology(
    client: &reqwest::Client,
    url: &str,
) -> anyhow::Result<Vec<TopologyEntry>> {
    let response = client
        .get(url)
        .timeout(Duration::from_secs(5))
        .send()
        .await?;

    if !response.status().is_success() {
        anyhow::bail!("Topology fetch failed: {}", response.status());
    }

    let api_response = response
        .json::<GardenApiResponse<Vec<TopologyEntry>>>()
        .await?;

    Ok(api_response.data)
}
