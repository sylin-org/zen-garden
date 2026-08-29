//! The pulse wall (ADR-0013): the garden, alive on a screen.
//!
//! A renderer, not a collector — everything it shows arrives on the
//! moss's pulse feed (snapshot-first, seq'd). The craft is ported from
//! the PoC's wall: geometry ladder (wide / stacked / tall / narrow /
//! tiny), gauges with Firefly-shared thresholds, the wire ring, plain
//! garden English. Layout and paint are PURE functions so the geometry
//! gallery can assert gorgeousness without a terminal.

use crate::moss_http;
use crate::Cli;
use garden_contract::pulse::PulseEvent;
use std::collections::VecDeque;
use std::io::Write as _;
use std::time::{Duration, Instant};

/// The ring keeps the last N wire lines.
pub const MAX_RING: usize = 200;
/// Redraw throttle: a slow terminal must never flicker.
pub const MIN_REDRAW_MS: u64 = 250;
pub const RECONNECT_MIN_MS: u64 = 1_000;
pub const RECONNECT_MAX_MS: u64 = 30_000;
/// Gauge color thresholds — shared with the Firefly LED companion's
/// vocabulary (PoC parity).
pub const THRESHOLD_WARN: f64 = 60.0;
pub const THRESHOLD_CRIT: f64 = 85.0;

// --- geometry --------------------------------------------------------------

/// Which rung of the ladder this geometry lands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// >= 100 cols: wire left, beds right.
    Wide,
    /// 60..100 cols: everything stacked.
    Stacked,
    /// Portrait (rows > cols): one column — a case screen is a feed's
    /// best friend.
    Tall,
    /// 40..60 cols.
    Narrow,
    /// < 40 cols (OLED sidecar): status + last few events.
    Tiny,
}

/// Region row counts. Every region's paint fits its rows exactly; text
/// is never truncated mid-word — regions DROP instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Regions {
    pub mode: Mode,
    pub width: usize,
    pub height: usize,
    pub header: usize,
    pub gauges: usize,
    pub status: usize,
    pub wire: usize,
    pub garden: usize,
    pub footer: usize,
    /// Sidebar width in wide mode (0 = none).
    pub side: usize,
}

/// The ladder: pure, total, and the single place geometry is decided.
pub fn layout(cols: usize, rows: usize) -> Regions {
    let mode = if cols < 40 || rows < 10 {
        Mode::Tiny
    } else if rows > cols {
        // Portrait wins over narrowness: a case screen is tall, and a
        // tall screen is a feed's best friend.
        Mode::Tall
    } else if cols < 60 {
        Mode::Narrow
    } else if cols >= 100 {
        Mode::Wide
    } else {
        Mode::Stacked
    };

    let header = if rows >= 4 { 1 } else { 0 };
    let status = if rows >= 6 { 1 } else { 0 };
    let footer = if rows >= 8 { 1 } else { 0 };
    let gauges = if rows >= 8 && mode != Mode::Tiny { 1 } else { 0 };

    let side = if mode == Mode::Wide && rows >= 16 { 30 } else { 0 };

    // The garden strip: one line per stone region budget; drops first.
    let mut garden = match mode {
        Mode::Tiny => 0,
        Mode::Wide => (rows / 4).clamp(0, 6),
        _ => (rows / 5).clamp(1, 5),
    };
    if rows < 8 {
        garden = 0;
    }

    let spent = header + gauges + status + footer + garden;
    let wire = rows.saturating_sub(spent).max(1);

    Regions { mode, width: cols, height: rows, header, gauges, status, wire, garden, footer, side }
}

// --- ANSI ------------------------------------------------------------------

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RED: &str = "\x1b[31m";
const AMBER: &str = "\x1b[33m";
const GREEN: &str = "\x1b[32m";
const CYAN: &str = "[36m";

fn paint_color(color: bool, code: &str, text: &str) -> String {
    if color { format!("{code}{text}{RESET}") } else { text.to_string() }
}

/// Fit a line to a width by dropping whole characters (color codes are
/// added AFTER fitting, so widths are honest).
pub fn fit(line: &str, width: usize) -> String {
    line.chars().take(width).collect()
}

/// A horizontal bar gauge with Firefly-shared thresholds. Falls back to
/// `LBL NN%` when the width cannot hold a bar.
pub fn gauge(label: &str, value: f64, width: usize, color: bool) -> String {
    let value = value.clamp(0.0, 100.0);
    let code = if value >= THRESHOLD_CRIT {
        RED
    } else if value >= THRESHOLD_WARN {
        AMBER
    } else {
        GREEN
    };
    let overhead = label.len() + 12;
    if width < overhead || width < 16 {
        return format!("{label} {:.0}", value);
    }
    let bar_width = (width - overhead + 4).min(40);
    let filled = ((value / 100.0) * bar_width as f64).round() as usize;
    let bar = format!(
        "[{}{}]",
        "=".repeat(filled),
        "-".repeat(bar_width.saturating_sub(filled))
    );
    let pct = format!("{:>3.0}%", value);
    paint_color(color, code, &format!("{label} {bar} {pct}"))
}

// --- the wire's memory ------------------------------------------------------

/// One wire line: a moment worth seeing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireLine {
    pub time: String,
    pub level: String,
    pub summary: String,
}

/// Per-second event buckets, newest last — the heartbeat's raw EKG.
#[derive(Debug, Default)]
pub struct Buckets {
    counts: VecDeque<u32>,
    open: Option<Instant>,
}

impl Buckets {
    pub fn count(&mut self, now: Instant) {
        const KEEP: usize = 60;
        let roll = match self.open {
            Some(t) => now.duration_since(t).as_secs(),
            None => 0,
        };
        match (self.open, roll) {
            (Some(_), 0) => {
                if let Some(b) = self.counts.back_mut() {
                    *b += 1;
                }
            }
            _ => {
                for _ in 0..roll.saturating_sub(1).min(KEEP as u64) {
                    self.counts.push_back(0);
                }
                self.counts.push_back(1);
                while self.counts.len() > KEEP {
                    self.counts.pop_front();
                }
                self.open = Some(now);
            }
        }
    }

    pub fn per_minute(&self) -> u32 {
        self.counts.iter().sum()
    }

    /// The heartbeat: unicode block sparkline over the last buckets,
    /// right-padded with calm (space) to 30 columns.
    pub fn sparkline(&self, unicode: bool) -> String {
        const RUNGS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        const RUNGS_ASCII: [char; 8] = ['_', '.', '-', '=', '+', '#', '%', '@'];
        let rungs = if unicode { RUNGS } else { RUNGS_ASCII };
        let max = self.counts.iter().copied().max().unwrap_or(0).max(1);
        let bars: String = self
            .counts
            .iter()
            .map(|c| {
                let idx = ((c * 8) / (max + 1)).clamp(0, 7) as usize;
                rungs[idx]
            })
            .collect();
        format!("{bars:>30}")
    }
}

// --- the wall's state -------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Connection {
    #[default]
    Connecting,
    Connected,
    Reconnecting { wait_secs: u64 },
}

#[derive(Debug, Default)]
pub struct WallState {
    pub stone_name: String,
    pub stones: Vec<serde_json::Value>,
    pub offerings: Vec<serde_json::Value>,
    pub jobs: Vec<serde_json::Value>,
    pub ring: VecDeque<WireLine>,
    /// Live work, pinned above the wire: (subject, last progress line).
    /// A working garden shows its working.
    pub progress: Vec<(String, String)>,
    pub buckets: Buckets,
    pub cpu: Option<f32>,
    pub memory: Option<f64>,
    pub event_count: u64,
    pub frame_no: u64,
    pub connection: Connection,
    pub connected_since: Option<Instant>,
}

fn hhmmss(ts: &str) -> String {
    if ts.len() >= 19 {
        ts[11..19].to_string()
    } else {
        chrono::Utc::now().format("%H:%M:%S").to_string()
    }
}

impl WallState {
    pub fn push_wire(&mut self, ts: &str, level: &str, summary: impl Into<String>) {
        if self.ring.len() >= MAX_RING {
            self.ring.pop_front();
        }
        self.ring.push_back(WireLine {
            time: hhmmss(ts),
            level: level.to_string(),
            summary: summary.into(),
        });
    }

    /// Absorb one pulse event. `now` is injected so tests are
    /// deterministic (the heartbeat buckets tick by it).
    pub fn apply(&mut self, ev: &PulseEvent, now: Instant) {
        self.event_count += 1;
        self.buckets.count(now);
        match ev.kind.as_str() {
            "snapshot" => {
                let data = ev.data.clone().unwrap_or_default();
                self.stones = data["stones"].as_array().cloned().unwrap_or_default();
                self.offerings = data["offerings"].as_array().cloned().unwrap_or_default();
                self.jobs = data["jobs"].as_array().cloned().unwrap_or_default();
                if let Some(name) = ev.stone.as_deref() {
                    self.stone_name = name.to_string();
                }
                self.connection = Connection::Connected;
                if self.connected_since.is_none() {
                    self.connected_since = Some(now);
                }
            }
            "topology.seen" => {
                if let Some(stone) = &ev.stone {
                    self.push_wire(&ev.ts, "info", format!("{stone} is here"));
                    if !self.stones.iter().any(|s| s["stone"]["name"] == *stone) {
                        // A stranger arrives: its full frame rides the next
                        // snapshot; the strip shows the name now.
                        self.stones.push(serde_json::json!({
                            "stone": { "name": stone },
                        }));
                    }
                }
            }
            "topology.goodbye" => {
                if let Some(stone) = &ev.stone {
                    self.push_wire(&ev.ts, "warn", format!("{stone} said goodbye - removed from the room"));
                    self.stones
                        .retain(|s| s["stone"]["name"] != *stone);
                }
            }
            "topology.expired" => {
                if let Some(stone) = &ev.stone {
                    self.push_wire(&ev.ts, "warn", format!("{stone} expired - silent past the threshold"));
                    for s in &mut self.stones {
                        if s["stone"]["name"] == *stone
                            && let Some(obj) = s.as_object_mut()
                        {
                            obj.insert("expired".into(), serde_json::json!(true));
                        }
                    }
                }
            }
            "offering.removed" => {
                self.push_wire(&ev.ts, "info", &ev.summary);
                if let Some(name) = &ev.offering {
                    self.offerings
                        .retain(|o| o["name"].as_str() != Some(name.as_str()));
                }
            }
            kind if kind.starts_with("offering.") => {
                self.push_wire(&ev.ts, if kind == "offering.degraded" { "warn" } else { "info" }, &ev.summary);
                if let (Some(name), Some(status)) =
                    (&ev.offering, kind.strip_prefix("offering."))
                {
                    for o in &mut self.offerings {
                        if o["name"].as_str() == Some(name.as_str())
                            && let Some(obj) = o.as_object_mut()
                        {
                            obj.insert("status".into(), serde_json::json!(status));
                        }
                    }
                }
            }
            "job.started" | "job.done" | "job.failed" | "job.interrupted" => {
                let level = match ev.kind.as_str() {
                    "job.failed" => "error",
                    "job.interrupted" => "warn",
                    _ => "info",
                };
                self.push_wire(&ev.ts, level, &ev.summary);
                // The work's pin leaves when the work ends.
                if let Some(subject) = ev.data.as_ref().and_then(|d| d["subject"].as_str()) {
                    self.progress.retain(|(s, _)| s != subject);
                }
            }
            "job.progress" => {
                let subject = ev.data.as_ref().and_then(|d| d["subject"].as_str());
                let line = ev.data.as_ref().and_then(|d| d["progress"].as_str());
                if let (Some(subject), Some(line)) = (subject, line) {
                    match self.progress.iter_mut().find(|(s, _)| s == subject) {
                        Some((_, existing)) => *existing = line.to_string(),
                        None => self.progress.push((subject.to_string(), line.to_string())),
                    }
                }
            }
            "storage.mounted" | "storage.ejected" | "storage.changed" => {
                self.push_wire(
                    &ev.ts,
                    if ev.kind == "storage.ejected" { "warn" } else { "info" },
                    &ev.summary,
                );
            }
            "stone.load" => {
                if let Some(data) = &ev.data {
                    self.cpu = data["cpu_percent"].as_f64().map(|v| v as f32);
                    self.memory = data["memory_percent"].as_f64();
                }
            }
            "wire.delta" | "pulse.lagged" => {}
            _ => {}
        }
    }

    /// The plain status line: states, not statistics.
    pub fn status_line(&self) -> String {
        match self.connection {
            Connection::Connecting => "connecting...".into(),
            Connection::Reconnecting { wait_secs } => {
                format!("reconnecting in {wait_secs}s...")
            }
            Connection::Connected => {
                let expired = self
                    .stones
                    .iter()
                    .filter(|s| s.get("expired").and_then(|e| e.as_bool()).unwrap_or(false))
                    .count();
                let reachable = self.stones.len() - expired;
                let running = self
                    .offerings
                    .iter()
                    .filter(|o| o["status"].as_str() == Some("running"))
                    .count();
                let degraded = self
                    .offerings
                    .iter()
                    .filter(|o| o["status"].as_str() == Some("degraded"))
                    .count();
                let mut line =
                    format!("{reachable} stones reachable · {running} offerings running");
                if degraded > 0 {
                    line.push_str(&format!(" · {degraded} degraded"));
                }
                if expired > 0 {
                    line.push_str(&format!(" · {expired} expired"));
                }
                line
            }
        }
    }

    fn uptime(&self, now: Instant) -> String {
        match self.connected_since {
            Some(t) => {
                let s = now.duration_since(t).as_secs();
                format!("up {}:{:02}", s / 60, s % 60)
            }
            None => String::new(),
        }
    }
}

// --- painting ---------------------------------------------------------------

/// Rendering options: the trash-hardware courtesies.
#[derive(Debug, Clone, Copy)]
pub struct Style {
    pub unicode: bool,
    pub color: bool,
}

fn separator(label: Option<&str>, width: usize, style: Style) -> String {
    let bar_char = if style.unicode { "─" } else { "-" };
    let prefix = match label {
        Some(l) => format!(" {l} "),
        None => String::new(),
    };
    let bar = bar_char.repeat(width.saturating_sub(prefix.len()));
    fit(&format!("{prefix}{bar}"), width)
}

trait WidthOf {
    fn width_of(&self) -> usize;
}
impl WidthOf for String {
    fn width_of(&self) -> usize {
        self.chars().count()
    }
}
impl WidthOf for &str {
    fn width_of(&self) -> usize {
        self.chars().count()
    }
}

/// Render one full frame: EXACTLY `regions.height` rows, each within
/// `regions.width` characters. Pure: state in, lines out.
pub fn render_frame(state: &WallState, r: &Regions, now: Instant, style: Style) -> Vec<String> {
    let w = r.width;
    let mut out: Vec<String> = Vec::with_capacity(r.height);

    // Header: the status speaks, the heartbeat breathes on the right.
    if r.header > 0 {
        let evt = state.buckets.per_minute();
        let head = if state.stone_name.is_empty() {
            format!("PULSE · {}", state.status_line())
        } else {
            format!("PULSE · {} · {}", state.stone_name, state.status_line())
        };
        let heart = state.buckets.sparkline(style.unicode);
        let tail = format!("{heart} {evt}/min");
        let pad = w.saturating_sub(head.width_of() + tail.width_of() + 2);
        out.push(fit(
            &paint_color(style.color, BOLD, &format!("{head}{}{tail}", " ".repeat(pad))),
            w,
        ));
    }

    // Gauges.
    if r.gauges > 0 {
        let half = (w / 2).saturating_sub(2);
        let cpu = state.cpu.unwrap_or(0.0) as f64;
        let mem = state.memory.unwrap_or(0.0);
        let row = if r.mode == Mode::Tiny || half < 16 {
            format!(
                "{} {}",
                gauge("CPU", cpu, half.max(8), style.color),
                gauge("MEM", mem, half.max(8), style.color)
            )
        } else {
            format!(
                "{}  {}",
                gauge("CPU", cpu, half, style.color),
                gauge("MEM", mem, w - half - 2, style.color)
            )
        };
        out.push(fit(&row, w));
    }

    // The status line stands on its own (it doubles as the header when
    // the header row is dropped).
    if r.status > 0 {
        let line = state.status_line();
        let code = if state.connection == Connection::Connected {
            GREEN
        } else {
            AMBER
        };
        out.push(fit(&paint_color(style.color, code, &line), w));
    }

    // The wire: live work pinned on top, moments newest-first below.
    if r.wire > 0 {
        if r.wire > 1 {
            out.push(fit(&separator(Some("the wire"), w, style), w));
        }
        let mut wire_rows = r.wire - usize::from(r.wire > 1);
        for (subject, line) in state.progress.iter().take(wire_rows) {
            let row = paint_color(
                style.color,
                CYAN,
                &format!("{subject} — {line}"),
            );
            out.push(fit(&row, w));
            wire_rows -= 1;
        }
        let skip = state.ring.len().saturating_sub(wire_rows);
        for line in state.ring.iter().skip(skip) {
            let code = match line.level.as_str() {
                "warn" => AMBER,
                "error" => RED,
                _ => "",
            };
            let text = format!("{} {}", line.time, line.summary);
            out.push(if code.is_empty() {
                fit(&text, w)
            } else {
                fit(&paint_color(style.color, code, &text), w)
            });
        }
        while out.len() < r.header + r.gauges + r.status + r.wire {
            out.push(String::new());
        }
    }

    // The garden strip: one line per stone, expired dimmed.
    if r.garden > 0 && r.height > out.len() + r.footer {
        out.push(fit(&separator(Some("the garden"), w, style), w));
        let garden_rows = r
            .garden
            .min(r.height - out.len() - r.footer)
            .saturating_sub(1);
        for stone in state.stones.iter().take(garden_rows) {
            let name = stone["stone"]["name"].as_str().unwrap_or("?");
            let health = stone["presence"]["health"].as_str().unwrap_or("?");
            let expired = stone.get("expired").and_then(|e| e.as_bool()).unwrap_or(false);
            let line = format!("{name:<22} {health}");
            out.push(if expired {
                fit(&paint_color(style.color, DIM, &line), w)
            } else {
                fit(&line, w)
            });
        }
        while out.len() < r.height - r.footer {
            out.push(String::new());
        }
    }

    // Footer: the almanac's plain cousin.
    if r.footer > 0 {
        let left = format!(
            "evt/min {} · {}",
            state.buckets.per_minute(),
            state.uptime(now)
        );
        let right = format!("{:?} · ctrl-c exits", r.mode).to_lowercase();
        let pad = w.saturating_sub(left.width_of() + right.width_of() + 2);
        out.push(fit(
            &paint_color(style.color, DIM, &format!("{left}{}{right}", " ".repeat(pad))),
            w,
        ));
    }

    out
}

// --- running ----------------------------------------------------------------

/// `rake pulse`: connect to a moss's pulse feed and paint until ctrl-c.
pub async fn run(cli: &Cli) -> Result<(), String> {
    let (cand, _) = cli
        .walk(
            false,
            "no moss answered - the room is out of reach",
            |c| moss_http::get_json(c.ip, c.http_port, "/health", crate::HTTP_TIMEOUT),
        )
        .await?;
    let tty = std::io::IsTerminal::is_terminal(&std::io::stdout());

    let mut state = WallState::default();
    let mut backoff = RECONNECT_MIN_MS;
    let mut stdout = std::io::stdout();

    loop {
        state.connection = Connection::Connecting;
        state.connected_since = None;
        draw(&mut stdout, &mut state, tty);

        match moss_http::open_stream(
            cand.ip,
            cand.http_port,
            garden_contract::faces::Face::PulseStream.path(),
            crate::HTTP_TIMEOUT,
        )
        .await
        {
            Ok(mut reader) => {
                backoff = RECONNECT_MIN_MS;
                state.connection = Connection::Connected;
                state.connected_since = Some(Instant::now());
                let reason = stream_loop(&mut state, &mut reader, &mut stdout, tty).await;
                match reason {
                    StreamEnd::Clean => {
                        if tty {
                            let (_, rows) = size();
                            let _ = writeln!(stdout, "\x1b[{rows};1H{RESET}");
                            let _ = stdout.flush();
                        }
                        return Ok(());
                    }
                    StreamEnd::Lost => {
                        state.connection = Connection::Reconnecting { wait_secs: backoff / 1000 };
                        state.push_wire("", "warn", "connection lost - reconnecting");
                    }
                }
            }
            Err(e) => {
                state.connection = Connection::Reconnecting { wait_secs: backoff / 1000 };
                state.push_wire("", "error", format!("feed unreachable: {e}"));
            }
        }

        draw(&mut stdout, &mut state, tty);
        tokio::time::sleep(Duration::from_millis(backoff)).await;
        backoff = (backoff * 2).min(RECONNECT_MAX_MS);
    }
}

enum StreamEnd {
    Clean,
    Lost,
}

async fn stream_loop(
    state: &mut WallState,
    reader: &mut moss_http::SseStream,
    stdout: &mut std::io::Stdout,
    tty: bool,
) -> StreamEnd {
    let mut last_render = Instant::now();
    let mut dirty = true;
    let mut resize = tokio::time::interval(Duration::from_secs(2));
    resize.tick().await;
    loop {
        tokio::select! {
            data = reader.next_data() => {
                match data {
                    Some(bytes) => {
                        if let Ok(ev) = serde_json::from_slice::<PulseEvent>(&bytes) {
                            state.apply(&ev, Instant::now());
                            dirty = true;
                        }
                    }
                    None => return StreamEnd::Lost,
                }
            }
            _ = resize.tick() => {
                dirty = true; // geometry may have changed; re-detect at draw
            }
            _ = tokio::signal::ctrl_c() => {
                return StreamEnd::Clean;
            }
        }
        if dirty && last_render.elapsed() >= Duration::from_millis(MIN_REDRAW_MS) {
            draw(&mut *stdout, state, tty);
            dirty = false;
            last_render = Instant::now();
        }
    }
}

/// Paint one frame to the sink: full-screen on a tty, sequential plain
/// frames otherwise (kiosk-loggable, witnessable without a terminal).
fn draw(stdout: &mut std::io::Stdout, state: &mut WallState, tty: bool) {
    let (cols, rows) = size();
    let style = Style { unicode: true, color: tty };
    let regions = layout(cols, rows);
    let frame = render_frame(state, &regions, Instant::now(), style);
    if tty {
        let mut out = String::from("\x1b[H");
        for (i, line) in frame.iter().enumerate() {
            out.push_str(&format!("\x1b[{};1H{line}\x1b[K", i + 1));
        }
        let _ = write!(stdout, "{out}");
        let _ = stdout.flush();
    } else {
        state.frame_no += 1;
        let mut out = format!("--- frame {} ---\n", state.frame_no);
        for line in &frame {
            out.push_str(line.trim_end());
            out.push('\n');
        }
        let _ = write!(stdout, "{out}");
        let _ = stdout.flush();
    }
}

/// Terminal size; (80, 24) when detection fails (piped output).
pub fn size() -> (usize, usize) {
    terminal_size::terminal_size()
        .map(|(w, h)| (w.0 as usize, h.0 as usize))
        .unwrap_or((80, 24))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    fn sample_state() -> WallState {
        let mut s = WallState::default();
        let snap = PulseEvent::new("snapshot", "pulse", "info", "the world").with_data(
            serde_json::json!({
                "stones": [
                    {"stone": {"name": "stone-a",
                        "presence": {"health": "thriving"}}},
                    {"stone": {"name": "stone-b",
                        "presence": {"health": "thriving"}}},
                ],
                "offerings": [
                    {"name": "redis::default", "status": "running"},
                    {"name": "ollama::default", "status": "running"},
                ],
                "jobs": [],
            }),
        );
        s.apply(&snap, Instant::now());
        s
    }

    /// THE GALLERY: the frame must be gorgeous at every canonical size —
    /// no overflow, no overlap, status and wire alive everywhere.
    #[test]
    fn the_wall_survives_every_screen() {
        let sizes: &[(usize, usize, Mode)] = &[
            (53, 120, Mode::Tall),   // portrait case screen
            (80, 24, Mode::Stacked), // ssh tty
            (120, 40, Mode::Wide),   // wide wall
            (200, 50, Mode::Wide),   // kiosk TV
            (26, 12, Mode::Tiny),    // OLED sidecar
            (40, 15, Mode::Narrow),  // narrow split
            (59, 80, Mode::Tall),    // tall medium
        ];
        for &(cols, rows, mode) in sizes {
            let regions = layout(cols, rows);
            assert_eq!(regions.mode, mode, "{cols}x{rows}");
            assert_eq!(regions.width, cols);
            let frame = render_frame(&sample_state(), &regions, Instant::now(),
                Style { unicode: true, color: false });
            assert!(frame.len() <= rows, "{cols}x{rows}: {} rows overflow {rows}", frame.len());
            for line in &frame {
                assert!(
                    line.chars().count() <= cols,
                    "{cols}x{rows}: line overflows: '{line}'"
                );
            }
            let joined = frame.join("\n");
            assert!(
                joined.contains("stones reachable") || joined.contains("connecting"),
                "{cols}x{rows}: the status line is missing"
            );
        }
    }

    /// Goodbye removes the stone's row; expired dims it. Two trust
    /// stories, two treatments.
    #[test]
    fn goodbye_removes_and_expired_marks() {
        let mut s = sample_state();
        assert_eq!(s.stones.len(), 2);
        s.apply(&PulseEvent::new(
            "topology.goodbye", "topology", "warn",
            "stone-b said goodbye - removed from the room",
        ).with_stone("stone-b"), Instant::now());
        assert_eq!(s.stones.len(), 1);

        s.apply(&PulseEvent::new(
            "topology.expired", "topology", "warn",
            "stone-a expired - silent past the threshold",
        ).with_stone("stone-a"), Instant::now());
        assert_eq!(
            s.stones[0].get("expired").and_then(|e| e.as_bool()),
            Some(true)
        );
        assert!(s.status_line().contains("1 expired"));
    }

    /// The wire never truncates mid-line: regions drop, words survive.
    #[test]
    fn wire_lines_fit_any_width() {
        let mut s = sample_state();
        s.apply(&PulseEvent::new(
            "topology.seen", "topology", "info",
            "a very long summary about something happening somewhere in the garden",
        ).with_stone("stone-c"), Instant::now());
        for cols in [26, 40, 59, 80, 200] {
            let regions = layout(cols, 30);
            let frame = render_frame(&s, &regions, Instant::now(),
                Style { unicode: false, color: false });
            for line in &frame {
                assert!(line.chars().count() <= cols, "{cols}: '{line}'");
            }
        }
    }

    /// The gauges speak the thresholds: green below, amber at warn, red
    /// at crit.
    #[test]
    fn gauges_change_color_at_the_thresholds() {
        let green = gauge("CPU", 30.0, 24, true);
        let amber = gauge("CPU", 70.0, 24, true);
        let red = gauge("CPU", 90.0, 24, true);
        assert!(green.contains("\x1b[32m"));
        assert!(amber.contains("\x1b[33m"));
        assert!(red.contains("\x1b[31m"));
        // Narrow: numbers survive without bars.
        assert_eq!(gauge("CPU", 42.0, 10, false), "CPU 42");
    }

    /// Live work pins above the wire and leaves when it ends — the
    /// docker-pull feel, one row per subject.
    #[test]
    fn progress_rows_pin_and_leave() {
        let mut s = sample_state();
        let mut ev = PulseEvent::new("job.progress", "job", "info",
            "ollama/model:llama3 - 45%").with_data(serde_json::json!(
            { "subject": "ollama/model:llama3", "progress": "45%" }));
        s.apply(&ev, Instant::now());
        ev = PulseEvent::new("job.progress", "job", "info",
            "ollama/model:llama3 - 78%").with_data(serde_json::json!(
            { "subject": "ollama/model:llama3", "progress": "78%" }));
        s.apply(&ev, Instant::now());
        assert_eq!(s.progress, vec![("ollama/model:llama3".to_string(), "78%".to_string())]);

        s.apply(&PulseEvent::new("job.done", "job", "info",
            "ollama/model:llama3 - done").with_data(serde_json::json!(
            { "subject": "ollama/model:llama3" })), Instant::now());
        assert!(s.progress.is_empty(), "the pin leaves when the work ends");
        assert!(s.ring.back().unwrap().summary.contains("done"));
    }

    /// The heartbeat counts events per minute and sparks.
    #[test]
    fn the_heartbeat_counts() {
        let mut s = WallState::default();
        let t0 = Instant::now();
        for i in 0..30u64 {
            s.apply(&PulseEvent::new("wire.delta", "wire", "info", "tick"), t0 + Duration::from_secs(i));
        }
        assert_eq!(s.buckets.per_minute(), 30);
        assert!(!s.buckets.sparkline(true).is_empty());
    }
}
