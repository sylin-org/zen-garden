//! Pulse command — permanent terminal monitor (v2: frame buffer + wire feed)
//!
//! Full-screen, unattended live display for stone observability.
//! Designed for dedicated Linux screens (tty1, OLED sidecar, wall monitor).
//!
//! Consumes the pulse stream (domain + transport events) and polls topology
//! for garden-wide awareness. Adapts layout to terminal geometry:
//! - Split screen (wire left, topology sidebar right) when columns permit
//! - Stacked (wire, then garden summary) on medium terminals
//! - Wire-only on narrow terminals
//!
//! See: PULSE-0001, PULSE-0002 ADRs

use crate::command_manifest::cmd;
use crate::commands::{Command, CommandResult};
use crate::context::Runtime;
use crate::ui::gauge;
use crate::ui::rendering::{self, TerminalInfo};
use colored::Colorize;
use futures_util::StreamExt;
use garden_common::presence::{
    event_types, OfferingState, PresenceSnapshot, StoneLoadUpdatedPayload, StoneState,
};
use garden_common::utils::strings::shorten_stone_name;
use garden_common::{GardenApiResponse, TopologyEntry};
use std::collections::VecDeque;
use std::io::Write;
use std::time::{Duration, Instant};

/// Maximum events in the ring buffer
const MAX_EVENTS: usize = 200;

/// Topology poll interval
const TOPOLOGY_POLL_SECS: u64 = 10;

/// Maximum redraw rate (milliseconds between redraws)
const MIN_REDRAW_INTERVAL_MS: u64 = 250;

/// Reconnection backoff bounds
const RECONNECT_MIN_MS: u64 = 1000;
const RECONNECT_MAX_MS: u64 = 30_000;

/// Minimum width for the topology sidebar in split mode
const SIDEBAR_MIN_COLS: usize = 28;

/// Minimum wire columns to justify split mode
const WIRE_MIN_COLS: usize = 50;

/// Pulse monitor command
pub struct PulseCommand {
    pub quiet: bool,
}

impl PulseCommand {
    pub fn new(quiet: bool) -> Self {
        Self { quiet }
    }
}

impl Command for PulseCommand {
    fn execute<'a>(&'a self, ctx: &'a Runtime) -> std::pin::Pin<Box<dyn std::future::Future<Output = CommandResult> + Send + 'a>> {
        Box::pin(async move {
            let endpoint = ctx.endpoint()?.to_string();
            run_pulse_monitor(&ctx.client, &endpoint, &ctx.term).await
        })
    }

    fn show_stone_header(&self) -> bool {
        false // pulse takes over the entire screen
    }

    fn name(&self) -> &'static str {
        cmd::PULSE
    }
}

// =============================================================================
// Layout detection
// =============================================================================

#[derive(Debug, Clone, Copy)]
enum LayoutMode {
    /// Wire left, topology sidebar right. `wire_cols` excludes the │ separator.
    Split {
        wire_cols: usize,
        sidebar_cols: usize,
    },
    /// Full-width wire, then compact garden summary below.
    Stacked,
    /// Wire only, no garden.
    Narrow,
}

#[derive(Debug, Clone)]
struct Layout {
    mode: LayoutMode,
    cols: usize,
    rows: usize,
    color: bool,
    unicode: bool,
}

impl Layout {
    /// Detect layout based on actual terminal dimensions.
    ///
    /// Split mode requires enough columns for BOTH a useful wire feed AND
    /// a useful sidebar. Calculated, not hardcoded breakpoints.
    fn detect(term: &TerminalInfo) -> Self {
        let (cols, rows) = rendering::terminal_dimensions();
        let color = term.supports_color;
        let unicode = term.supports_unicode;

        // Split mode: we need wire_min + 1 (separator) + sidebar_min
        let split_min = WIRE_MIN_COLS + 1 + SIDEBAR_MIN_COLS;
        let mode = if cols >= split_min {
            // Give sidebar ~35% but cap it and ensure wire gets the lion's share
            let sidebar = ((cols as f64 * 0.35) as usize).clamp(SIDEBAR_MIN_COLS, 40);
            let wire = cols - sidebar - 1; // -1 for │ separator
            LayoutMode::Split {
                wire_cols: wire,
                sidebar_cols: sidebar,
            }
        } else if cols >= WIRE_MIN_COLS {
            LayoutMode::Stacked
        } else {
            LayoutMode::Narrow
        };

        Self {
            mode,
            cols,
            rows,
            color,
            unicode,
        }
    }

    /// How many rows the garden section needs in stacked mode.
    fn garden_rows(&self, topology_len: usize) -> usize {
        match self.mode {
            LayoutMode::Split { .. } => 0, // sidebar handles garden
            LayoutMode::Stacked if topology_len > 0 => {
                // Separator + 2 stones per row (side by side)
                let stone_rows = topology_len.div_ceil(2);
                1 + stone_rows.min(self.rows / 4)
            }
            _ => 0,
        }
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
    // v2: diagnostics for footer
    event_count: u64,
    event_window: VecDeque<Instant>,
    last_transport: Option<Instant>,
    last_domain: Option<Instant>,
    connected_since: Option<Instant>,
    server_shutdown: bool,
}

/// A single event line for the wire feed
struct EventLine {
    time: String,    // "HH:MM:SS" or "HH:MM"
    entity: String,  // offering name, stone name, or "stone"
    message: String, // base label (e.g. "chirp (3 svc)", "election req")
    level: EventLevel,
    /// Detail items for budget-based formatting at paint time.
    /// Pre-extracted from payload JSON. `paint_wire` appends these after
    /// `message` using `fit_items()` with whatever character budget remains.
    detail_items: Vec<String>,
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

/// Why the stream loop ended — determines whether to reconnect or exit.
enum DisconnectReason {
    /// Connection lost unexpectedly — reconnect with backoff.
    ConnectionLost,
    /// Server sent `server.shutdown` — exit cleanly to unblock updates.
    ServerShutdown,
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
            event_count: 0,
            event_window: VecDeque::with_capacity(256),
            last_transport: None,
            last_domain: None,
            connected_since: None,
            server_shutdown: false,
        }
    }

    fn push_event(&mut self, event: EventLine) {
        if self.events.len() >= MAX_EVENTS {
            self.events.pop_front();
        }
        self.events.push_back(event);
        self.event_count += 1;
        self.event_window.push_back(Instant::now());
        // Trim window to last 60 seconds
        let cutoff = Instant::now() - Duration::from_secs(60);
        while self.event_window.front().is_some_and(|t| *t < cutoff) {
            self.event_window.pop_front();
        }
    }

    fn events_per_minute(&self) -> usize {
        let cutoff = Instant::now() - Duration::from_secs(60);
        self.event_window.iter().filter(|t| **t >= cutoff).count()
    }

    fn apply_snapshot(&mut self, snapshot: PresenceSnapshot) {
        self.stone_name = snapshot.stone.name.clone();
        self.stone = Some(snapshot.stone);
        self.offerings = snapshot.offerings;
        self.connection_status = ConnectionStatus::Connected;
        if self.connected_since.is_none() {
            self.connected_since = Some(Instant::now());
        }
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
    term: &TerminalInfo,
) -> CommandResult {
    let mut state = MonitorState::new();
    let mut backoff_ms = RECONNECT_MIN_MS;

    // Pulse stream URL (v2: full firehose — domain + transport events)
    let pulse_url = format!(
        "{}{}",
        endpoint.trim_end_matches('/'),
        event_types::PULSE_STREAM_PATH,
    );
    let topology_url = format!("{}/api/v1/garden/topology", endpoint.trim_end_matches('/'));

    // Clear screen once before first frame; subsequent frames overwrite in-place
    print!("\x1b[2J");

    loop {
        state.connection_status = ConnectionStatus::Connecting;
        render_frame(&state, term);

        // Try to connect — no total timeout (SSE streams are indefinite).
        // The client's connect_timeout handles connect-phase failures.
        let connect_result = client
            .get(&pulse_url)
            .header("Accept", "text/event-stream")
            .send()
            .await;

        match connect_result {
            Ok(response) if response.status().is_success() => {
                backoff_ms = RECONNECT_MIN_MS; // reset backoff on success
                state.connection_status = ConnectionStatus::Connected;
                state.connected_since = Some(Instant::now());

                // Run the streaming loop
                let reason = stream_loop(&mut state, response, client, &topology_url, term).await;

                match reason {
                    DisconnectReason::ServerShutdown => {
                        // Server shutting down — exit cleanly to unblock updates
                        let (_, rows) = rendering::terminal_dimensions();
                        println!("\x1b[{};1H\x1b[0m", rows);
                        let _ = std::io::stdout().flush();
                        return Ok(());
                    }
                    DisconnectReason::ConnectionLost => {
                        // Connection lost — fall through to reconnect
                        state.connected_since = None;
                        state.push_event(EventLine {
                            time: rendering::format_wall_clock(),
                            entity: "connection".to_string(),
                            message: "lost".to_string(),
                            level: EventLevel::Warn,
                            detail_items: Vec::new(),
                        });
                    }
                }
            }
            Ok(response) => {
                state.push_event(EventLine {
                    time: rendering::format_wall_clock(),
                    entity: "connection".to_string(),
                    message: format!("HTTP {}", response.status()),
                    level: EventLevel::Error,
                    detail_items: Vec::new(),
                });
            }
            Err(e) => {
                state.push_event(EventLine {
                    time: rendering::format_wall_clock(),
                    entity: "connection".to_string(),
                    message: format!("{}", e),
                    level: EventLevel::Error,
                    detail_items: Vec::new(),
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

/// Stream events until disconnection. Returns reason for disconnect.
async fn stream_loop(
    state: &mut MonitorState,
    response: reqwest::Response,
    client: &reqwest::Client,
    topology_url: &str,
    term: &TerminalInfo,
) -> DisconnectReason {
    let mut stream = response.bytes_stream();
    let mut sse_buffer = String::new();
    let mut last_render = Instant::now();
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
                    Some(Err(e)) => {
                        tracing::debug!("SSE stream error: {}", e);
                        return DisconnectReason::ConnectionLost;
                    }
                    None => {
                        // Stream ended — check if server told us it's shutting down
                        if state.server_shutdown {
                            return DisconnectReason::ServerShutdown;
                        }
                        return DisconnectReason::ConnectionLost;
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
                let (_, rows) = rendering::terminal_dimensions();
                println!("\x1b[{};1H\x1b[0m", rows);
                let _ = std::io::stdout().flush();
                std::process::exit(0);
            }
        }

        // Throttle redraws
        if dirty && last_render.elapsed() >= Duration::from_millis(MIN_REDRAW_INTERVAL_MS) {
            render_frame(state, term);
            dirty = false;
            last_render = Instant::now();
        }
    }
}

// =============================================================================
// SSE message processing (pulse stream: domain.* + transport.* + pulse.snapshot)
// =============================================================================

/// Process a single SSE message and update monitor state.
fn process_sse_message(state: &mut MonitorState, message: &str) {
    let mut event_type = String::new();
    let mut data = String::new();

    for line in message.lines() {
        if let Some(et) = line.strip_prefix("event: ") {
            event_type = et.to_string();
        } else if let Some(et) = line.strip_prefix("event:") {
            event_type = et.trim().to_string();
        } else if let Some(d) = line.strip_prefix("data: ") {
            data.push_str(d);
        } else if let Some(d) = line.strip_prefix("data:") {
            data.push_str(d.trim_start());
        }
    }

    // Server shutdown: exit cleanly instead of reconnecting
    if event_type == "server.shutdown" {
        state.server_shutdown = true;
        return;
    }

    if data.is_empty() {
        return;
    }

    // Pulse stream events: "pulse.snapshot", "domain.{type}", "transport.{type}"
    if event_type == "pulse.snapshot" || event_type == "presence.snapshot" {
        if let Ok(snapshot) = serde_json::from_str::<PresenceSnapshot>(&data) {
            state.apply_snapshot(snapshot);
        }
        return;
    }

    if let Some(domain_type) = event_type.strip_prefix("domain.") {
        process_domain_event(state, domain_type, &data);
        state.last_domain = Some(Instant::now());
        return;
    }

    if let Some(transport_type) = event_type.strip_prefix("transport.") {
        process_transport_event(state, transport_type, &data);
        state.last_transport = Some(Instant::now());
        return;
    }

    // Fallback: legacy presence-style event types (backward compat)
    process_domain_event(state, &event_type, &data);
    state.last_domain = Some(Instant::now());
}

/// Process a domain event from the pulse stream.
fn process_domain_event(state: &mut MonitorState, event_type: &str, data: &str) {
    match event_type {
        "stone.load.updated" => {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data)
                && let Some(payload_value) = parsed.get("data")
                    && let Ok(payload) =
                        serde_json::from_value::<StoneLoadUpdatedPayload>(payload_value.clone())
                    {
                        state.apply_load_update(payload);

                        let cpu = state.stone.as_ref().map(|s| s.cpu_percent).unwrap_or(0.0);
                        let mem = state
                            .stone
                            .as_ref()
                            .map(|s| s.memory_percent)
                            .unwrap_or(0.0);

                        // Extra metrics as detail items (only non-zero to avoid clutter)
                        let mut details = Vec::new();
                        if let Some(stone) = state.stone.as_ref() {
                            if stone.disk_percent > 0.0 {
                                details.push(format!("dsk {:.0}", stone.disk_percent));
                            }
                            if stone.has_gpu && stone.gpu_percent > 0.0 {
                                details.push(format!("gpu {:.0}", stone.gpu_percent));
                            }
                        }

                        state.push_event(EventLine {
                            time: rendering::extract_sse_time(&parsed),
                            entity: "stone".to_string(),
                            message: format!("load  cpu {:.0}  mem {:.0}", cpu, mem),
                            level: EventLevel::Dim,
                            detail_items: details,
                        });
                    }
        }
        "stone.health.changed" => {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                let data_obj = parsed.get("data");
                let health = data_obj
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

                // Show cpu/mem at transition time — explains *why* health changed
                let mut details = Vec::new();
                if let Some(cpu) = data_obj
                    .and_then(|d| d.get("cpu_percent"))
                    .and_then(|v| v.as_f64())
                {
                    details.push(format!("cpu {:.0}", cpu));
                }
                if let Some(mem) = data_obj
                    .and_then(|d| d.get("memory_percent"))
                    .and_then(|v| v.as_f64())
                {
                    details.push(format!("mem {:.0}", mem));
                }

                state.push_event(EventLine {
                    time: rendering::extract_sse_time(&parsed),
                    entity: "stone".to_string(),
                    message: format!("health  {}", health),
                    level,
                    detail_items: details,
                });
            }
        }
        "service.started" | "service.stopped" | "service.sprouted" | "service.uprooted"
        | "service.updated" | "service.renamed" => {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                let entity = parsed
                    .get("entity")
                    .or_else(|| parsed.get("service"))
                    .and_then(|s| s.as_str())
                    .unwrap_or("unknown");
                let action = event_type.strip_prefix("service.").unwrap_or(event_type);
                state.apply_offering_event(entity, action);
                let level = match action {
                    "stopped" | "uprooted" => EventLevel::Warn,
                    _ => EventLevel::Info,
                };
                state.push_event(EventLine {
                    time: rendering::extract_sse_time(&parsed),
                    entity: entity.to_string(),
                    message: action.to_string(),
                    level,
                    detail_items: Vec::new(),
                });
            }
        }
        "service.health.changed" => {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                let entity = parsed
                    .get("entity")
                    .or_else(|| parsed.get("service"))
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
                    time: rendering::extract_sse_time(&parsed),
                    entity: entity.to_string(),
                    message: format!("health  {}", health),
                    level,
                    detail_items: Vec::new(),
                });
            }
        }
        "stone.tended" => {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                let msg = parsed
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("tended");

                // Who tended and from where
                let mut details = Vec::new();
                let data_obj = parsed.get("data");
                if let Some(by) = data_obj.and_then(|d| d.get("by")).and_then(|v| v.as_str()) {
                    details.push(format!("by {}", by));
                }
                if let Some(from) = data_obj
                    .and_then(|d| d.get("from"))
                    .and_then(|v| v.as_str())
                {
                    details.push(format!("from {}", from));
                }

                state.push_event(EventLine {
                    time: rendering::extract_sse_time(&parsed),
                    entity: "stone".to_string(),
                    message: msg.to_string(),
                    level: EventLevel::Info,
                    detail_items: details,
                });
            }
        }
        _ => {
            // Generic domain event
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                let msg = parsed
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or(event_type);
                let entity = parsed
                    .get("entity")
                    .or_else(|| parsed.get("service"))
                    .and_then(|s| s.as_str())
                    .unwrap_or("stone");
                state.push_event(EventLine {
                    time: rendering::extract_sse_time(&parsed),
                    entity: entity.to_string(),
                    message: msg.to_string(),
                    level: EventLevel::Dim,
                    detail_items: Vec::new(),
                });
            }
        }
    }
}

/// Process a transport event from the pulse stream.
fn process_transport_event(state: &mut MonitorState, transport_type: &str, data: &str) {
    let parsed: serde_json::Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(_) => return,
    };

    let summary = parsed
        .get("summary")
        .and_then(|s| s.as_str())
        .unwrap_or(transport_type);

    // Extract a short entity name from the summary or payload
    let entity = extract_transport_entity(&parsed, summary);

    let level = transport_event_level(transport_type);

    let (label, details) = transport_label(transport_type, &parsed);

    state.push_event(EventLine {
        time: rendering::extract_sse_time(&parsed),
        entity,
        message: label,
        level,
        detail_items: details,
    });
}

/// Extract a short entity name from transport event data.
fn extract_transport_entity(parsed: &serde_json::Value, summary: &str) -> String {
    // Try payload_preview first, then summary parsing
    if let Some(preview) = parsed.get("payload_preview")
        && let Some(name) = preview
            .get("name")
            .or_else(|| preview.get("stone_name"))
            .or_else(|| preview.get("requester_name"))
            .or_else(|| preview.get("winner_name"))
            .and_then(|n| n.as_str())
        {
            return shorten_stone_name(name).to_string();
        }

    // Parse from "from" address as last resort
    if let Some(from) = parsed.get("from").and_then(|f| f.as_str()) {
        // "192.168.1.5:7184" → extract IP
        if let Some(ip) = from.split(':').next() {
            return ip.to_string();
        }
    }

    // Use first word of summary
    summary.split_whitespace().last().unwrap_or("?").to_string()
}

/// Build a compact base label and optional detail items for transport events.
///
/// Returns `(base_label, detail_items)`:
/// - `base_label`: short descriptor (e.g. "chirp (3 svc)", "election req")
/// - `detail_items`: ordered list of detail strings for budget-based fitting
fn transport_label(transport_type: &str, parsed: &serde_json::Value) -> (String, Vec<String>) {
    let preview = parsed.get("payload_preview");

    match transport_type {
        "stone_chirp" => {
            let services = preview.and_then(|p| p.get("services"));
            match services {
                Some(serde_json::Value::Array(arr)) => {
                    let names: Vec<String> = arr
                        .iter()
                        .filter_map(|s| {
                            s.get("offering")
                                .or_else(|| s.get("name"))
                                .and_then(|n| n.as_str())
                                .map(|n| n.to_string())
                        })
                        .collect();
                    let count = if names.is_empty() {
                        arr.len()
                    } else {
                        names.len()
                    };
                    (format!("chirp ({} svc)", count), names)
                }
                Some(serde_json::Value::String(t)) => {
                    let count = t
                        .trim_start_matches("[...")
                        .trim_end_matches(" items]")
                        .parse::<usize>()
                        .unwrap_or(0);
                    (format!("chirp ({} svc)", count), Vec::new())
                }
                _ => ("chirp".to_string(), Vec::new()),
            }
        }
        "stone_goodbye" => ("goodbye".to_string(), Vec::new()),
        "discovery_request" => ("discovery req".to_string(), Vec::new()),
        "discovery_response" => ("discovery rsp".to_string(), Vec::new()),
        "election_request" => {
            let mut details = Vec::new();
            if let Some(et) = preview.and_then(|p| p.get("election_type")) {
                // Simple variants: "update_source" → "update source"
                // Tuple variants: {"offering_primary": "weaviate:dev"} → "offering primary (weaviate:dev)"
                if let Some(s) = et.as_str() {
                    details.push(s.replace('_', " "));
                } else if let Some(obj) = et.as_object()
                    && let Some(key) = obj.keys().next() {
                        let val = obj[key].as_str().unwrap_or("");
                        if val.is_empty() {
                            details.push(key.replace('_', " "));
                        } else {
                            details.push(format!("{} ({})", key.replace('_', " "), val));
                        }
                    }
            }
            ("election req".to_string(), details)
        }
        "election_candidate" => {
            let mut details = Vec::new();
            if let Some(name) = preview
                .and_then(|p| p.get("stone_name").or_else(|| p.get("name")))
                .and_then(|n| n.as_str())
            {
                details.push(shorten_stone_name(name).to_string());
            }
            if let Some(score) = preview
                .and_then(|p| p.get("score"))
                .and_then(|s| s.as_i64())
            {
                details.push(format!("score={}", score));
            }
            ("election candidate".to_string(), details)
        }
        "election_result" => {
            let mut details = Vec::new();
            if let Some(winner) = preview
                .and_then(|p| p.get("winner_name").or_else(|| p.get("name")))
                .and_then(|n| n.as_str())
            {
                details.push(format!("winner={}", shorten_stone_name(winner)));
            }
            ("election result".to_string(), details)
        }
        "storage_beacon" => {
            let banks = preview.and_then(|p| p.get("seed_banks").or_else(|| p.get("banks")));
            match banks {
                Some(serde_json::Value::Array(arr)) => {
                    let names: Vec<String> = arr
                        .iter()
                        .filter_map(|b| {
                            b.get("name")
                                .and_then(|n| n.as_str())
                                .map(|s| s.to_string())
                        })
                        .collect();
                    let count = if names.is_empty() {
                        arr.len()
                    } else {
                        names.len()
                    };
                    (format!("beacon ({} banks)", count), names)
                }
                Some(serde_json::Value::String(t)) => {
                    let count = t
                        .trim_start_matches("[...")
                        .trim_end_matches(" items]")
                        .parse::<usize>()
                        .unwrap_or(0);
                    (format!("beacon ({} banks)", count), Vec::new())
                }
                _ => ("storage beacon".to_string(), Vec::new()),
            }
        }
        "tools_beacon" => {
            let deltas = preview.and_then(|p| p.get("deltas"));
            match deltas {
                Some(serde_json::Value::Array(arr)) => {
                    let names: Vec<String> = arr
                        .iter()
                        .filter_map(|d| {
                            d.get("tool_fqid")
                                .and_then(|n| n.as_str())
                                .map(|s| s.to_string())
                        })
                        .collect();
                    let count = if names.is_empty() {
                        arr.len()
                    } else {
                        names.len()
                    };
                    (format!("tools ({} deltas)", count), names)
                }
                _ => ("tools beacon".to_string(), Vec::new()),
            }
        }
        other => (other.replace('_', " "), Vec::new()),
    }
}

/// Map transport event types to display levels.
fn transport_event_level(transport_type: &str) -> EventLevel {
    match transport_type {
        "stone_chirp" | "discovery_response" | "storage_beacon" | "tools_beacon" => EventLevel::Dim,
        "discovery_request" | "stone_goodbye" => EventLevel::Info,
        "election_request" | "election_candidate" | "election_result" => EventLevel::Warn,
        _ => EventLevel::Dim,
    }
}

// =============================================================================
// Frame buffer rendering (v2: paint + composite)
// =============================================================================

/// Render the full monitor frame to stdout.
fn render_frame(state: &MonitorState, term: &TerminalInfo) {
    let layout = Layout::detect(term);

    // Paint independent regions
    let header_lines = paint_header(state, &layout);
    let gauge_lines = paint_gauges(state, &layout);
    let offering_line = paint_offerings(state, &layout);
    let footer_line = paint_footer(state, &layout);

    // Calculate chrome height
    let chrome_rows = header_lines.len()
        + gauge_lines.len()
        + if offering_line.is_some() { 1 } else { 0 }
        + 1 // divider
        + 1; // footer

    let content_rows = layout.rows.saturating_sub(chrome_rows);

    // Build frame buffer
    let mut buf = String::with_capacity(layout.cols * layout.rows);
    buf.push_str("\x1b[H"); // cursor home (no clear — frame buffer overwrites every cell)

    // Header
    for line in &header_lines {
        buf.push_str(line);
        buf.push('\n');
    }

    // Gauges
    for line in &gauge_lines {
        buf.push_str(line);
        buf.push('\n');
    }

    // Offerings
    if let Some(ref line) = offering_line {
        buf.push_str(line);
        buf.push('\n');
    }

    // Divider
    paint_separator(&mut buf, None, layout.cols, layout.unicode, layout.color);

    // Content region (wire + optional sidebar/garden)
    match layout.mode {
        LayoutMode::Split {
            wire_cols,
            sidebar_cols,
        } => {
            let wire_lines = paint_wire(state, wire_cols, content_rows, &layout);
            let sidebar_lines = paint_sidebar(state, sidebar_cols, content_rows, &layout);
            composite_split(
                &mut buf,
                &wire_lines,
                &sidebar_lines,
                wire_cols,
                layout.unicode,
            );
        }
        LayoutMode::Stacked => {
            let garden_rows = layout.garden_rows(state.topology.len());
            let wire_rows = content_rows.saturating_sub(garden_rows);
            let wire_lines = paint_wire(state, layout.cols, wire_rows, &layout);
            for line in &wire_lines {
                buf.push_str(line);
                buf.push('\n');
            }
            // Garden summary
            if garden_rows > 0 {
                let label = garden_summary(&state.topology, layout.cols);
                paint_separator(
                    &mut buf,
                    Some(&label),
                    layout.cols,
                    layout.unicode,
                    layout.color,
                );
                paint_garden_compact(
                    &mut buf,
                    state,
                    layout.cols,
                    garden_rows.saturating_sub(1),
                    &layout,
                );
            }
        }
        LayoutMode::Narrow => {
            let wire_lines = paint_wire(state, layout.cols, content_rows, &layout);
            for line in &wire_lines {
                buf.push_str(line);
                buf.push('\n');
            }
        }
    }

    // Enforce exact row budget: the buffer must have exactly (rows - 1) newlines
    // before the footer so the frame fits the terminal without scrolling.
    let newline_count = buf.chars().filter(|&c| c == '\n').count();
    let target_newlines = layout.rows.saturating_sub(1);
    if newline_count < target_newlines {
        for _ in 0..(target_newlines - newline_count) {
            buf.push('\n');
        }
    } else if newline_count > target_newlines {
        // Too many lines — truncate buffer to fit
        let mut keep_to = 0;
        let mut seen = 0;
        for (i, c) in buf.char_indices() {
            if c == '\n' {
                seen += 1;
                if seen == target_newlines {
                    keep_to = i + 1;
                    break;
                }
            }
        }
        if keep_to > 0 {
            buf.truncate(keep_to);
        }
    }

    // Footer
    buf.push_str(&footer_line);

    // Flush entire frame in one write
    let _ = std::io::stdout().write_all(buf.as_bytes());
    let _ = std::io::stdout().flush();
}

// =============================================================================
// Paint functions — each returns pre-formatted lines
// =============================================================================

/// Paint header: stone name, health, uptime, evt/min.
fn paint_header(state: &MonitorState, layout: &Layout) -> Vec<String> {
    let cols = layout.cols;
    let color = layout.color;

    let name = if !state.stone_name.is_empty() {
        if cols < 60 {
            state
                .stone_name
                .strip_prefix("stone-")
                .unwrap_or(&state.stone_name)
        } else {
            &state.stone_name
        }
    } else {
        "connecting..."
    };

    let (health_str, uptime_str) = if let Some(ref stone) = state.stone {
        let uptime = garden_common::format_uptime(stone.uptime_seconds);
        (stone.health.clone(), format!("up {}", uptime))
    } else {
        match &state.connection_status {
            ConnectionStatus::Connected => ("connected".to_string(), String::new()),
            ConnectionStatus::Connecting => ("connecting...".to_string(), String::new()),
            ConnectionStatus::Reconnecting { wait_secs } => {
                (format!("reconnecting...  {}s", wait_secs), String::new())
            }
        }
    };

    // evt/min counter
    let epm = state.events_per_minute();
    let epm_str = if epm > 0 {
        format!("{}evt/min", epm)
    } else {
        String::new()
    };

    let right_parts: Vec<&str> = [health_str.as_str(), uptime_str.as_str(), epm_str.as_str()]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect();
    let right_plain = right_parts.join("  ");
    let padding = cols.saturating_sub(name.len() + right_plain.len() + 2);

    let line = if color {
        let colored_health = match health_str.as_str() {
            "thriving" => health_str.green().to_string(),
            "withering" => health_str.yellow().to_string(),
            "wilting" => health_str.red().to_string(),
            s if s.starts_with("reconnecting") => health_str.yellow().to_string(),
            _ => health_str.clone(),
        };

        let mut right = colored_health;
        if !uptime_str.is_empty() {
            right = format!("{}  {}", right, uptime_str.dimmed());
        }
        if !epm_str.is_empty() {
            right = format!("{}  {}", right, epm_str.dimmed());
        }

        format!(" {}{}{}", name.bold(), " ".repeat(padding), right)
    } else {
        format!(" {}{}{}", name, " ".repeat(padding), right_plain)
    };

    vec![line]
}

/// Paint gauge bars.
fn paint_gauges(state: &MonitorState, layout: &Layout) -> Vec<String> {
    let cols = layout.cols;
    let color = layout.color;

    let stone = match &state.stone {
        Some(s) => s,
        None => {
            let line = if color {
                format!(" {}", "waiting for data...".dimmed())
            } else {
                " waiting for data...".to_string()
            };
            return vec![line];
        }
    };

    let mut lines = Vec::new();

    if cols >= 80 {
        // Wide: two gauges per line
        let gauge_width = (cols - 4) / 2;
        let cpu = gauge::format_gauge("CPU", stone.cpu_percent, gauge_width, color);
        let mem = gauge::format_gauge("MEM", stone.memory_percent, gauge_width, color);
        lines.push(format!(" {}   {}", cpu, mem));

        let dsk = gauge::format_gauge("DSK", stone.disk_percent, gauge_width, color);
        if stone.has_gpu {
            let gpu = gauge::format_gauge("GPU", stone.gpu_percent, gauge_width, color);
            lines.push(format!(" {}   {}", dsk, gpu));
        } else {
            lines.push(format!(" {}", dsk));
        }

        // Network rates
        if stone.net_rx_bytes_per_sec > 0 || stone.net_tx_bytes_per_sec > 0 {
            let rx = gauge::format_net_rate(stone.net_rx_bytes_per_sec);
            let tx = gauge::format_net_rate(stone.net_tx_bytes_per_sec);
            let net_line = format!("NET  {} dn  {} up", rx, tx);
            let indent = " ".repeat(gauge_width.saturating_sub(net_line.len()) + 5);
            if color {
                lines.push(format!("{}{}", indent, net_line.dimmed()));
            } else {
                lines.push(format!("{}{}", indent, net_line));
            }
        }
    } else {
        // Narrow: one gauge per line
        let gauge_width = cols.saturating_sub(2);
        lines.push(format!(
            " {}",
            gauge::format_gauge("CPU", stone.cpu_percent, gauge_width, color)
        ));
        lines.push(format!(
            " {}",
            gauge::format_gauge("MEM", stone.memory_percent, gauge_width, color)
        ));
        lines.push(format!(
            " {}",
            gauge::format_gauge("DSK", stone.disk_percent, gauge_width, color)
        ));

        if stone.has_gpu {
            lines.push(format!(
                " {}",
                gauge::format_gauge("GPU", stone.gpu_percent, gauge_width, color)
            ));
        }
    }

    lines
}

/// Paint offerings status line. Returns None if no offerings.
fn paint_offerings(state: &MonitorState, layout: &Layout) -> Option<String> {
    if state.offerings.is_empty() {
        return None;
    }

    let cols = layout.cols;
    let color = layout.color;
    let mut line = String::from(" ");

    for (count, offering) in state.offerings.iter().enumerate() {
        let status_char = match offering.status.as_str() {
            "running" | "adopted" | "borrowed" => {
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

    Some(line)
}

/// Paint wire feed events.
fn paint_wire(state: &MonitorState, cols: usize, max_rows: usize, layout: &Layout) -> Vec<String> {
    let color = layout.color;

    if state.events.is_empty() || max_rows == 0 {
        return Vec::new();
    }

    // Most recent first (newest at top, oldest at bottom)
    let all_events: Vec<&EventLine> = state.events.iter().rev().collect();
    let display_events = &all_events[..all_events.len().min(max_rows)];

    // Compute entity column width
    let entity_width = display_events
        .iter()
        .map(|e| e.entity.len())
        .max()
        .unwrap_or(6)
        .clamp(6, 16);

    let mut lines = Vec::new();
    for event in display_events {
        if lines.len() >= max_rows {
            break;
        }

        let time = if cols >= 60 {
            &event.time
        } else {
            // Short time: strip seconds
            if event.time.len() > 5 {
                &event.time[..5]
            } else {
                &event.time
            }
        };

        let padded_entity = if event.entity.len() < entity_width {
            format!(
                "{}{}",
                event.entity,
                " ".repeat(entity_width - event.entity.len())
            )
        } else {
            event.entity[..entity_width].to_string()
        };

        // Build display message: base label + budget-fitted detail items
        let prefix_len = 1 + time.len() + 2 + entity_width + 2;
        let msg_width = cols.saturating_sub(prefix_len);

        let msg = if !event.detail_items.is_empty() && msg_width > event.message.len() + 2 {
            // Budget for details: total width minus base label minus "  " separator
            let detail_budget = msg_width - event.message.len() - 2;
            let detail_refs: Vec<&str> = event.detail_items.iter().map(|s| s.as_str()).collect();
            let fitted = rendering::fit_items(&detail_refs, detail_budget);
            if fitted.is_empty() {
                rendering::truncate_visible(&event.message, msg_width)
            } else {
                let full = format!("{}  {}", event.message, fitted);
                rendering::truncate_visible(&full, msg_width)
            }
        } else {
            rendering::truncate_visible(&event.message, msg_width)
        };

        let line = if color {
            match event.level {
                EventLevel::Info => {
                    format!(" {}  {}  {}", time.dimmed(), padded_entity.green(), msg)
                }
                EventLevel::Warn => format!(
                    " {}  {}  {}",
                    time.dimmed(),
                    padded_entity.yellow(),
                    msg.yellow()
                ),
                EventLevel::Error => {
                    format!(" {}  {}  {}", time.dimmed(), padded_entity.red(), msg.red())
                }
                EventLevel::Dim => format!(
                    " {}  {}  {}",
                    time.dimmed(),
                    padded_entity.dimmed(),
                    msg.dimmed()
                ),
            }
        } else {
            format!(" {}  {}  {}", time, padded_entity, msg)
        };

        lines.push(line);
    }

    lines
}

/// Paint topology sidebar for split mode.
fn paint_sidebar(
    state: &MonitorState,
    sidebar_cols: usize,
    max_rows: usize,
    layout: &Layout,
) -> Vec<String> {
    let color = layout.color;
    let now = chrono::Utc::now();
    let mut lines = Vec::new();

    if state.topology.is_empty() {
        if max_rows > 0 {
            let label = if color {
                format!("  {}", "no topology".dimmed())
            } else {
                "  no topology".to_string()
            };
            lines.push(rendering::pad_visible(&label, sidebar_cols));
        }
        return lines;
    }

    // Header
    let summary = garden_summary(&state.topology, sidebar_cols);
    let header = if color {
        format!("  {}", summary.dimmed())
    } else {
        format!("  {}", summary)
    };
    lines.push(rendering::pad_visible(&header, sidebar_cols));

    // Sort: self pinned first, then by last_seen descending
    let mut sorted: Vec<&TopologyEntry> = state.topology.iter().collect();
    sorted.sort_by(|a, b| {
        let a_self = a.stone_name == state.stone_name;
        let b_self = b.stone_name == state.stone_name;
        match (a_self, b_self) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => b.last_seen.cmp(&a.last_seen),
        }
    });

    for entry in sorted.iter().take(max_rows.saturating_sub(1)) {
        let name = shorten_stone_name(&entry.stone_name);
        let is_self = entry.stone_name == state.stone_name;
        let svc_count = entry.services.len();

        let health_dot = match entry.health.as_str() {
            "thriving" => {
                if color {
                    "●".green().to_string()
                } else {
                    "o".to_string()
                }
            }
            "degraded" | "withering" => {
                if color {
                    "●".yellow().to_string()
                } else {
                    "!".to_string()
                }
            }
            _ => {
                if color {
                    "○".dimmed().to_string()
                } else {
                    "-".to_string()
                }
            }
        };

        let age = if is_self {
            "self".to_string()
        } else {
            let elapsed = now.signed_duration_since(entry.last_seen);
            let secs = elapsed.num_seconds().max(0) as u64;
            if secs < 60 {
                format!("{}s", secs)
            } else {
                format!("{}m", secs / 60)
            }
        };

        // Name width: sidebar minus health dot (2) - svc (4) - age (~5) - spacing
        let name_width = sidebar_cols.saturating_sub(16).max(8);
        let display_name = if name.len() > name_width {
            &name[..name_width]
        } else {
            name
        };
        let name_pad = name_width.saturating_sub(display_name.len());

        let line = if color {
            let name_display = if is_self {
                display_name.bold().to_string()
            } else {
                display_name.to_string()
            };
            format!(
                "  {} {}{}  {}  {}",
                health_dot,
                name_display,
                " ".repeat(name_pad),
                format!("{}", svc_count).dimmed(),
                age.dimmed(),
            )
        } else {
            format!(
                "  {} {}{}  {}  {}",
                health_dot,
                display_name,
                " ".repeat(name_pad),
                svc_count,
                age,
            )
        };

        lines.push(rendering::pad_visible(&line, sidebar_cols));
    }

    // Pad remaining rows
    while lines.len() < max_rows {
        lines.push(" ".repeat(sidebar_cols));
    }

    lines
}

/// Paint footer with connection diagnostics.
fn paint_footer(state: &MonitorState, layout: &Layout) -> String {
    let cols = layout.cols;
    let color = layout.color;

    let mut parts: Vec<String> = Vec::new();

    // Connected duration
    if let Some(since) = state.connected_since {
        let elapsed = since.elapsed();
        let secs = elapsed.as_secs();
        if secs >= 3600 {
            parts.push(format!(
                "connected {}h {}m",
                secs / 3600,
                (secs % 3600) / 60
            ));
        } else if secs >= 60 {
            parts.push(format!("connected {}m {}s", secs / 60, secs % 60));
        } else {
            parts.push(format!("connected {}s", secs));
        }
    } else {
        parts.push("disconnected".to_string());
    }

    // Events per minute
    let epm = state.events_per_minute();
    if cols >= 60 {
        parts.push(format!("{} evt/min", epm));
    } else {
        parts.push(format!("{}/min", epm));
    }

    // Last chirp age
    if let Some(last) = state.last_transport {
        let age = last.elapsed().as_secs();
        if cols >= 60 {
            parts.push(format!("last chirp {}s", age));
        } else {
            parts.push(format!("chirp {}s", age));
        }
    }

    // Last domain event age
    if let Some(last) = state.last_domain {
        let age = last.elapsed().as_secs();
        if cols >= 60 {
            parts.push(format!("last health {}s", age));
        } else {
            parts.push(format!("health {}s", age));
        }
    }

    let separator = if layout.unicode { " \u{2502} " } else { " | " };
    let text = parts.join(separator);

    // Pad/truncate to fit
    let display = if text.len() + 1 > cols {
        format!(" {}", &text[..cols.saturating_sub(1)])
    } else {
        format!(" {}", text)
    };

    if color {
        format!("{}", display.dimmed())
    } else {
        display
    }
}

// =============================================================================
// Composite functions
// =============================================================================

/// Composite split layout: wire left │ sidebar right.
fn composite_split(
    buf: &mut String,
    wire_lines: &[String],
    sidebar_lines: &[String],
    wire_cols: usize,
    unicode: bool,
) {
    let sep = if unicode { "\u{2502}" } else { "|" };
    let max_lines = wire_lines.len().max(sidebar_lines.len());

    for i in 0..max_lines {
        let wire = wire_lines.get(i).map(|s| s.as_str()).unwrap_or("");
        let sidebar = sidebar_lines.get(i).map(|s| s.as_str()).unwrap_or("");

        // Wire line needs to be padded to wire_cols visible characters.
        // Since ANSI codes make len() unreliable, we pad the raw string.
        buf.push_str(wire);

        // Pad wire to fill its column width.
        // Approximate: strip ANSI for measurement.
        let wire_visible = rendering::visible_length(wire);
        if wire_visible < wire_cols {
            buf.push_str(&" ".repeat(wire_cols - wire_visible));
        }

        buf.push_str(sep);
        buf.push_str(sidebar);
        buf.push('\n');
    }
}

/// Paint compact garden for stacked mode (two stones per row).
fn paint_garden_compact(
    buf: &mut String,
    state: &MonitorState,
    cols: usize,
    max_rows: usize,
    layout: &Layout,
) {
    let color = layout.color;
    let now = chrono::Utc::now();

    // Sort: self pinned first, then by last_seen descending
    let mut sorted: Vec<&TopologyEntry> = state.topology.iter().collect();
    sorted.sort_by(|a, b| {
        let a_self = a.stone_name == state.stone_name;
        let b_self = b.stone_name == state.stone_name;
        match (a_self, b_self) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => b.last_seen.cmp(&a.last_seen),
        }
    });

    let half_cols = cols / 2;

    for (row_count, pair) in sorted.chunks(2).enumerate() {
        if row_count >= max_rows {
            break;
        }

        let mut line = String::new();
        for entry in pair {
            let name = shorten_stone_name(&entry.stone_name);
            let is_self = entry.stone_name == state.stone_name;
            let svc_count = entry.services.len();

            let health_dot = match entry.health.as_str() {
                "thriving" => {
                    if color {
                        "●".green().to_string()
                    } else {
                        "o".to_string()
                    }
                }
                "degraded" | "withering" => {
                    if color {
                        "●".yellow().to_string()
                    } else {
                        "!".to_string()
                    }
                }
                _ => {
                    if color {
                        "○".dimmed().to_string()
                    } else {
                        "-".to_string()
                    }
                }
            };

            let age = if is_self {
                "self".to_string()
            } else {
                let elapsed = now.signed_duration_since(entry.last_seen);
                let secs = elapsed.num_seconds().max(0) as u64;
                if secs < 60 {
                    format!("{}s", secs)
                } else {
                    format!("{}m", secs / 60)
                }
            };

            let name_width = half_cols.saturating_sub(14).max(6);
            let display_name = if name.len() > name_width {
                &name[..name_width]
            } else {
                name
            };

            if color {
                let n = if is_self {
                    display_name.bold().to_string()
                } else {
                    display_name.to_string()
                };
                let entry_str = format!(" {} {}  {}  {}", health_dot, n, svc_count, age.dimmed());
                // Pad to half_cols
                let visible = rendering::visible_length(&entry_str);
                line.push_str(&entry_str);
                if visible < half_cols {
                    line.push_str(&" ".repeat(half_cols - visible));
                }
            } else {
                let entry_str = format!(" {} {}  {}  {}", health_dot, display_name, svc_count, age);
                let padded = if entry_str.len() < half_cols {
                    format!("{}{}", entry_str, " ".repeat(half_cols - entry_str.len()))
                } else {
                    entry_str[..half_cols].to_string()
                };
                line.push_str(&padded);
            }
        }

        buf.push_str(&line);
        buf.push('\n');
    }
}

// =============================================================================
// Separator helper
// =============================================================================

/// Write a horizontal divider line directly to buffer.
fn paint_separator(buf: &mut String, label: Option<&str>, cols: usize, unicode: bool, color: bool) {
    let sep = rendering::format_separator(label, cols, unicode);
    if color {
        buf.push_str(&format!("{}\n", sep.dimmed()));
    } else {
        buf.push_str(&sep);
        buf.push('\n');
    }
}

// =============================================================================
// Helpers
// =============================================================================

/// Build a garden section header label with health summary.
fn garden_summary(topology: &[TopologyEntry], cols: usize) -> String {
    let total = topology.len();
    let mut thriving = 0u32;
    let mut withering = 0u32;
    let mut offline = 0u32;
    for entry in topology {
        match entry.health.as_str() {
            "thriving" => thriving += 1,
            "withering" | "degraded" => withering += 1,
            _ => offline += 1,
        }
    }

    if cols < 60 {
        let ok = thriving + withering;
        return format!("garden ({}/{} ok)", ok, total);
    }

    let mut parts = Vec::new();
    if thriving > 0 {
        parts.push(format!("{} thriving", thriving));
    }
    if withering > 0 {
        parts.push(format!("{} withering", withering));
    }
    if offline > 0 {
        parts.push(format!("{} offline", offline));
    }
    format!("garden ({})", parts.join(", "))
}

/// Fetch topology from stone.
async fn fetch_topology(client: &reqwest::Client, url: &str) -> anyhow::Result<Vec<TopologyEntry>> {
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
