// Legacy factory retained for backward compatibility during the bus
// migration window; bus_registrations() is the supported path.
#![allow(dead_code)]

//! ESP8266 OLED v2 adapter (dual-zone yellow/blue icon dashboard).
//!
//! Factory scans USB for CH340 devices, briefly opens each to probe the
//! `I` response, and only produces an adapter when the firmware reports
//! `firefly-oled-v2`. This disambiguates from OLED v1 (Ch2) and T-Display
//! (Ch4), which share VID 0x1a86.
//!
//! Subscriptions cover the full presence kind set; payload extraction
//! drives direct serial commands (S / H / D / WIPE-IN / WIPE-OUT).
//! Cached state is replayed on reconnect so the display recovers its
//! last-known dashboard when the adapter respawns after an unplug.

use crate::serial::{
    find_firefly_devices, FireflyConnection, FireflyDeviceType, FireflySerial,
};
use garden_common::command_manifest::CommandResponse;
use garden_common::presence::{
    PresenceSnapshot, StoneHealthChangedPayload, StoneLoadUpdatedPayload,
};
use garden_companion_sdk::adapters::{
    Adapter, AdapterFactory, AdapterInfo, AdapterProfile, DeliveryPolicy, adapter::BoxFuture,
};
use garden_companion_sdk::moss_client::MossLocalClient;
use garden_companion_sdk::garden::{
    CommandInvocation, CommandOutcome, CommandResult, Event, Pulse,
    ServiceStartedPayload, ServiceStoppedPayload, StoneTendedPayload, StorageConnectedPayload,
    StorageRemovedPayload,
};
use std::collections::HashSet;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::{Mutex, mpsc};
use tokio_util::sync::CancellationToken;

const OLED_V2_SUBSCRIPTIONS: &[&str] = &[
    "core.command.invocation",
    "core.presence.snapshot",
    "core.stone.health.changed",
    "core.stone.load.updated",
    "core.stone.tended",
    "core.service.started",
    "core.service.stopped",
    "core.storage.connected",
    "core.storage.removed",
];

// ---------------------------------------------------------------------------
// Cached dashboard state
// ---------------------------------------------------------------------------

/// Running snapshot of values that compose the OLED v2 `D,...` frame.
/// Load events update a subset; presence.snapshot rewrites the whole thing.
#[derive(Debug, Default, Clone)]
struct DashboardState {
    stone_name: Option<String>,
    health_label: String,
    cpu_percent: u8,
    memory_percent: u8,
    disk_percent: u8,
    uptime_seconds: u64,
    offering_count: usize,
    /// Aggregate net bytes/s (rx + tx). Moss emits counters; we sum them.
    net_bps: u64,
    has_seed_bank: bool,
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

pub struct OledV2Factory {
    preferred_port: Option<String>,
    /// Ports confirmed as v2 firmware on a previous discovery tick. See
    /// [`oled_v1::OledV1Factory::claimed`] for rationale — re-probing a
    /// port the adapter already owns fails with "access denied" and
    /// causes reap/respawn churn.
    claimed: StdMutex<HashSet<String>>,
}

impl OledV2Factory {
    pub fn new(preferred_port: Option<String>) -> Self {
        Self {
            preferred_port,
            claimed: StdMutex::new(HashSet::new()),
        }
    }
}

impl AdapterFactory for OledV2Factory {
    fn kind(&self) -> &'static str {
        "firefly.oled-v2"
    }

    fn discover(&self) -> Vec<Box<dyn Adapter>> {
        let devices = match find_firefly_devices() {
            Ok(d) => d,
            Err(e) => {
                tracing::debug!(error = %e, "oled-v2 discovery: USB scan failed");
                return Vec::new();
            }
        };

        let current_ports: HashSet<String> = devices
            .iter()
            .filter(|d| d.device_type == FireflyDeviceType::Esp8266Oled)
            .map(|d| d.port_name.clone())
            .collect();

        {
            let mut claimed = self.claimed.lock().unwrap();
            claimed.retain(|p| current_ports.contains(p));
        }

        devices
            .into_iter()
            // VID 0x1a86 — classified as Esp8266Oled by from_vid; refine
            // via a live probe below.
            .filter(|d| d.device_type == FireflyDeviceType::Esp8266Oled)
            .filter(|d| {
                self.preferred_port
                    .as_ref()
                    .is_none_or(|p| p.eq_ignore_ascii_case(&d.port_name))
            })
            .filter_map(|d| {
                let already_claimed = self
                    .claimed
                    .lock()
                    .unwrap()
                    .contains(&d.port_name);
                if already_claimed {
                    return Some(
                        Box::new(OledV2Adapter::new(d.port_name)) as Box<dyn Adapter>
                    );
                }
                match probe_is_v2(&d.port_name) {
                    Ok(true) => {
                        self.claimed.lock().unwrap().insert(d.port_name.clone());
                        Some(Box::new(OledV2Adapter::new(d.port_name)) as Box<dyn Adapter>)
                    }
                    Ok(false) => None,
                    Err(e) => {
                        tracing::debug!(
                            port = %d.port_name,
                            error = %e,
                            "oled-v2 probe failed — leaving for another factory"
                        );
                        None
                    }
                }
            })
            .collect()
    }
}

/// Open the port, send `I`, classify, close. Returns true only for
/// firmware `firefly-oled-v2`.
fn probe_is_v2(port_name: &str) -> anyhow::Result<bool> {
    let serial = FireflySerial::new(port_name, FireflyDeviceType::Esp8266Oled)?;
    let response = serial.info()?;
    let refined = FireflyDeviceType::refine_from_info(FireflyDeviceType::Esp8266Oled, &response);
    Ok(refined == FireflyDeviceType::Esp8266OledV2)
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

pub struct OledV2Adapter {
    port_name: String,
    prebuilt: Option<Arc<FireflyConnection>>,
}

impl OledV2Adapter {
    pub fn new(port_name: String) -> Self {
        Self {
            port_name,
            prebuilt: None,
        }
    }

    /// Construct from a pre-built bus connection.
    pub fn from_connection(connection: Arc<FireflyConnection>, port_name: String) -> Self {
        Self {
            port_name,
            prebuilt: Some(connection),
        }
    }
}

impl Adapter for OledV2Adapter {
    fn info(&self) -> AdapterInfo {
        AdapterInfo {
            kind: "firefly.oled-v2",
            id: self.port_name.clone(),
            device: Some(format!("ESP8266-OLED-v2 on {}", self.port_name)),
        }
    }

    fn profile(&self) -> AdapterProfile {
        AdapterProfile {
            subscriptions: OLED_V2_SUBSCRIPTIONS,
            delivery: DeliveryPolicy::All,
            persisted_state: false,
        }
    }

    fn run(
        self: Box<Self>,
        mut events: mpsc::Receiver<Event>,
        moss: Arc<MossLocalClient>,
        pulse: Arc<Pulse>,
        shutdown: CancellationToken,
    ) -> BoxFuture<'static, ()> {
        Box::pin(async move {
            let connection = match self.prebuilt {
                Some(conn) => conn,
                None => {
                    let conn = Arc::new(FireflyConnection::new(Some(self.port_name.clone())));
                    if let Err(e) = conn.try_connect() {
                        tracing::warn!(
                            port = %self.port_name,
                            error = %e,
                            "oled-v2 adapter could not open device"
                        );
                        return;
                    }
                    conn
                }
            };
            let _ = connection.with_device(|s| s.clear());

            let state = Arc::new(Mutex::new(DashboardState::default()));

            // Hydrate from moss's HTTP API (COMPANION-0014).
            // Deterministic: request → response. No race against SSE
            // subscription timing.
            match moss.presence_snapshot().await {
                Ok(p) => {
                    let mut s = state.lock().await;
                    s.stone_name = Some(p.stone.name.clone());
                    s.health_label = p.stone.health.clone();
                    s.cpu_percent = p.stone.cpu_percent as u8;
                    s.memory_percent = p.stone.memory_percent as u8;
                    s.disk_percent = p.stone.disk_percent as u8;
                    s.uptime_seconds = p.stone.uptime_seconds;
                    s.offering_count = p.offerings.len();
                    s.net_bps = p.stone.net_rx_bytes_per_sec
                        + p.stone.net_tx_bytes_per_sec;
                    s.has_seed_bank = p.stone.seed_bank.is_some();
                    let snapshot = s.clone();
                    drop(s);
                    push_full_snapshot(&connection, &snapshot);
                }
                Err(e) => {
                    tracing::warn!(
                        port = %self.port_name,
                        error = %e,
                        "oled-v2 hydrate from moss failed; will rely on live deltas"
                    );
                }
            }

            loop {
                tokio::select! {
                    _ = shutdown.cancelled() => break,
                    maybe = events.recv() => match maybe {
                        Some(event) => {
                            // Device lifecycle is a bus concern; we exit
                            // only on shutdown/closed-channel.
                            handle_event(&event, &connection, &state, &pulse).await;
                        }
                        None => break,
                    },
                }
            }

            let _ = connection.with_device(|s| s.clear());
        })
    }
}

// ---------------------------------------------------------------------------
// Event dispatch
// ---------------------------------------------------------------------------

async fn handle_event(
    event: &Event,
    connection: &Arc<FireflyConnection>,
    state: &Arc<Mutex<DashboardState>>,
    pulse: &Arc<Pulse>,
) {
    if event.kind == "core.command.invocation" {
        if let Some(inv) = event.payload::<CommandInvocation>() {
            let response = handle_command(&inv.raw_args, connection, state).await;
            let outcome = if response.is_success() {
                CommandOutcome::Success {
                    output: Some(response.message.clone()),
                }
            } else {
                CommandOutcome::Error {
                    message: response.message.clone(),
                }
            };
            let _ = pulse.ingest(Event::new(CommandResult {
                correlation_id: inv.correlation_id,
                outcome,
                from: "oled-v2".to_string(),
            }));
        }
        return;
    }

    match event.kind {
        "core.presence.snapshot" => {
            if let Some(p) = event.payload::<PresenceSnapshot>() {
                let mut s = state.lock().await;
                s.stone_name = Some(p.stone.name.clone());
                s.health_label = p.stone.health.clone();
                s.cpu_percent = p.stone.cpu_percent as u8;
                s.memory_percent = p.stone.memory_percent as u8;
                s.disk_percent = p.stone.disk_percent as u8;
                s.uptime_seconds = p.stone.uptime_seconds;
                s.offering_count = p.offerings.len();
                s.net_bps = p.stone.net_rx_bytes_per_sec + p.stone.net_tx_bytes_per_sec;
                s.has_seed_bank = p.stone.seed_bank.is_some();
                drop(s);
                let s = state.lock().await.clone();
                push_full_snapshot(connection, &s);
            }
        }
        "core.stone.health.changed" => {
            if let Some(p) = event.payload::<StoneHealthChangedPayload>() {
                state.lock().await.health_label = p.health.clone();
                let _ = connection.with_device(|serial| serial.oled_health(&p.health));
            }
        }
        "core.stone.load.updated" => {
            if let Some(p) = event.payload::<StoneLoadUpdatedPayload>() {
                let mut s = state.lock().await;
                s.cpu_percent = p.cpu_percent as u8;
                s.memory_percent = p.memory_percent as u8;
                s.disk_percent = p.disk_percent as u8;
                s.net_bps = p.net_rx_bytes_per_sec + p.net_tx_bytes_per_sec;
                let snapshot = s.clone();
                drop(s);
                push_dashboard(connection, &snapshot);
            }
        }
        "core.stone.tended" => {
            if event.payload::<StoneTendedPayload>().is_some() {
                let _ = connection.with_device(|s| s.oled_wipe_in("ZEN GARDEN", "TENDING"));
            }
        }
        "core.service.started" => {
            if let Some(p) = event.payload::<ServiceStartedPayload>() {
                let label = p.service.to_uppercase();
                let _ = connection.with_device(|s| s.oled_wipe_in(&label, "STARTED"));
                let mut s = state.lock().await;
                s.offering_count = s.offering_count.saturating_add(1);
                let snapshot = s.clone();
                drop(s);
                push_dashboard(connection, &snapshot);
            }
        }
        "core.service.stopped" => {
            if let Some(p) = event.payload::<ServiceStoppedPayload>() {
                let label = p.service.to_uppercase();
                let _ = connection.with_device(|s| s.oled_wipe_out(&label, "STOPPED"));
                let mut s = state.lock().await;
                s.offering_count = s.offering_count.saturating_sub(1);
                let snapshot = s.clone();
                drop(s);
                push_dashboard(connection, &snapshot);
            }
        }
        "core.storage.connected" => {
            if event.payload::<StorageConnectedPayload>().is_some() {
                state.lock().await.has_seed_bank = true;
                let _ = connection.with_device(|s| s.oled_wipe_in("STORAGE", "CONNECTED"));
                let snapshot = state.lock().await.clone();
                push_dashboard(connection, &snapshot);
            }
        }
        "core.storage.removed" => {
            if event.payload::<StorageRemovedPayload>().is_some() {
                state.lock().await.has_seed_bank = false;
                let _ = connection.with_device(|s| s.oled_wipe_out("SEED BANK", "REMOVED"));
                let snapshot = state.lock().await.clone();
                push_dashboard(connection, &snapshot);
            }
        }
        _ => {}
    }
}

fn push_full_snapshot(connection: &FireflyConnection, state: &DashboardState) {
    if let Some(name) = &state.stone_name {
        let _ = connection.with_device(|s| {
            s.oled_stone_name(name)?;
            s.oled_health(&state.health_label)?;
            s.oled_v2_dashboard(
                state.cpu_percent,
                state.memory_percent,
                state.disk_percent,
                &format_uptime(state.uptime_seconds),
                state.offering_count,
                0,
                state.net_bps,
                state.has_seed_bank,
            )
        });
    }
}

fn push_dashboard(connection: &FireflyConnection, state: &DashboardState) {
    let _ = connection.with_device(|s| {
        s.oled_v2_dashboard(
            state.cpu_percent,
            state.memory_percent,
            state.disk_percent,
            &format_uptime(state.uptime_seconds),
            state.offering_count,
            0,
            state.net_bps,
            state.has_seed_bank,
        )
    });
}

/// Human-readable uptime. Copied from legacy `oled.rs` — unit-scale up to days.
fn format_uptime(seconds: u64) -> String {
    if seconds < 60 {
        format!("{}s", seconds)
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else if seconds < 86400 {
        format!("{}h", seconds / 3600)
    } else {
        format!("{}d", seconds / 86400)
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

async fn handle_command(
    args: &[String],
    connection: &Arc<FireflyConnection>,
    state: &Arc<Mutex<DashboardState>>,
) -> CommandResponse {
    let Some((cmd, rest)) = args.split_first() else {
        return CommandResponse::error("No command provided");
    };
    match cmd.to_lowercase().as_str() {
        "clear" => match connection.with_device(|s| s.clear()) {
            Ok(_) => CommandResponse::success("Display cleared"),
            Err(e) => CommandResponse::error(format!("Device error: {}", e)),
        },
        "brightness" | "bright" | "dim" => {
            let Some(raw) = rest.first() else {
                return CommandResponse::error("Usage: brightness <0-100>");
            };
            let Ok(v) = raw.parse::<u8>() else {
                return CommandResponse::error("Brightness must be 0-100");
            };
            match connection.with_device(|s| s.brightness(v)) {
                Ok(_) => CommandResponse::success(format!("Brightness {}%", v)),
                Err(e) => CommandResponse::error(format!("Device error: {}", e)),
            }
        }
        "info" => match connection.with_device(|s| s.info()) {
            Ok(r) => CommandResponse::success(r),
            Err(e) => CommandResponse::error(format!("Device error: {}", e)),
        },
        "refresh" => {
            let snapshot = state.lock().await.clone();
            push_full_snapshot(connection, &snapshot);
            CommandResponse::success("Dashboard refreshed")
        }
        other => CommandResponse::error(format!("Unknown command: {}", other)),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_uptime_scales_with_magnitude() {
        assert_eq!(format_uptime(45), "45s");
        assert_eq!(format_uptime(90), "1m");
        assert_eq!(format_uptime(3700), "1h");
        assert_eq!(format_uptime(90_000), "1d");
    }

    #[test]
    fn subscriptions_include_presence_and_command() {
        assert!(OLED_V2_SUBSCRIPTIONS.contains(&"core.command.invocation"));
        assert!(OLED_V2_SUBSCRIPTIONS.contains(&"core.presence.snapshot"));
    }
}
