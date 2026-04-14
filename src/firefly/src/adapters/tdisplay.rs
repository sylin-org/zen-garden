// Legacy factory retained for backward compatibility during the bus
// migration window; bus_registrations() is the supported path.
#![allow(dead_code)]

//! ESP32 T-Display adapter (`firefly-tdisplay` firmware, 135x240 ST7789).
//!
//! Full-color TFT drawn from two complementary event shapes:
//!
//! - `core.presence.snapshot` → single `J,<json>` push rebuilding every
//!   field the firmware tracks (stone name, health, load bars, offering
//!   counter, lantern / cricket / pond / GPU flags, wall clock hour,
//!   seed-bank state).
//! - Per-kind events → narrow `L` / `H` / `+` / `-` / `T` / `SD` / `SR`
//!   updates so the firmware can animate transitions instead of redoing
//!   a full redraw per frame.
//!
//! VID 0x1a86 is shared with CH340 (OLED v1/v2); [`TDisplayFactory`]
//! probes the `I` response and only claims ports that report
//! `firefly-tdisplay`. The same probe-cache discipline as the OLED
//! factories stops the supervisor from reaping a live adapter.

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
use serde::Serialize;
use std::collections::HashSet;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::{Mutex, mpsc};
use tokio_util::sync::CancellationToken;

const TDISPLAY_SUBSCRIPTIONS: &[&str] = &[
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
// Cached state (mirrors the firmware's tracked fields)
// ---------------------------------------------------------------------------

/// Compact JSON payload for the `J,<json>` full-push frame. Field names
/// are single-char to minimize serial transfer time — the firmware
/// parses them directly.
#[derive(Debug, Default, Clone, Serialize)]
struct TDisplayState {
    #[serde(rename = "n")]
    stone_name: String,
    #[serde(rename = "h")]
    health: String,
    #[serde(rename = "c")]
    cpu: u8,
    #[serde(rename = "m")]
    mem: u8,
    #[serde(rename = "d")]
    disk: u8,
    #[serde(rename = "i")]
    io: u8,
    #[serde(rename = "g")]
    gpu: u8,
    #[serde(rename = "ga")]
    gpu_active: u8,
    #[serde(rename = "up")]
    uptime: u64,
    #[serde(rename = "sv")]
    offerings: usize,
    #[serde(rename = "hg")]
    has_gpu: u8,
    #[serde(rename = "il")]
    is_lantern: u8,
    #[serde(rename = "hc")]
    has_cricket: u8,
    #[serde(rename = "pa")]
    pond_active: u8,
    #[serde(rename = "hr")]
    hour: f64,
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

pub struct TDisplayFactory {
    preferred_port: Option<String>,
    /// Ports already confirmed as T-Display firmware. See the OLED
    /// factories for the rationale — re-probing a port the adapter
    /// already holds open would cause reap/respawn churn.
    claimed: StdMutex<HashSet<String>>,
}

impl TDisplayFactory {
    pub fn new(preferred_port: Option<String>) -> Self {
        Self {
            preferred_port,
            claimed: StdMutex::new(HashSet::new()),
        }
    }
}

impl AdapterFactory for TDisplayFactory {
    fn kind(&self) -> &'static str {
        "firefly.tdisplay"
    }

    fn discover(&self) -> Vec<Box<dyn Adapter>> {
        let devices = match find_firefly_devices() {
            Ok(d) => d,
            Err(e) => {
                tracing::debug!(error = %e, "tdisplay discovery: USB scan failed");
                return Vec::new();
            }
        };

        // CH9102 shares the CH340 VID, so candidates initially classify
        // as Esp8266Oled. Collect that slice and probe each port.
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
                        Box::new(TDisplayAdapter::new(d.port_name)) as Box<dyn Adapter>,
                    );
                }
                match probe_is_tdisplay(&d.port_name) {
                    Ok(true) => {
                        self.claimed.lock().unwrap().insert(d.port_name.clone());
                        Some(Box::new(TDisplayAdapter::new(d.port_name)) as Box<dyn Adapter>)
                    }
                    Ok(false) => None,
                    Err(e) => {
                        tracing::debug!(
                            port = %d.port_name,
                            error = %e,
                            "tdisplay probe failed — leaving for another factory"
                        );
                        None
                    }
                }
            })
            .collect()
    }
}

fn probe_is_tdisplay(port_name: &str) -> anyhow::Result<bool> {
    let serial = FireflySerial::new(port_name, FireflyDeviceType::Esp8266Oled)?;
    let response = serial.info()?;
    let refined = FireflyDeviceType::refine_from_info(FireflyDeviceType::Esp8266Oled, &response);
    Ok(refined == FireflyDeviceType::Esp32TDisplay)
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

pub struct TDisplayAdapter {
    port_name: String,
    prebuilt: Option<Arc<FireflyConnection>>,
}

impl TDisplayAdapter {
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

impl Adapter for TDisplayAdapter {
    fn info(&self) -> AdapterInfo {
        AdapterInfo {
            kind: "firefly.tdisplay",
            id: self.port_name.clone(),
            device: Some(format!("ESP32-TDisplay on {}", self.port_name)),
        }
    }

    fn profile(&self) -> AdapterProfile {
        AdapterProfile {
            subscriptions: TDISPLAY_SUBSCRIPTIONS,
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
                            "tdisplay adapter could not open device"
                        );
                        return;
                    }
                    conn
                }
            };
            let _ = connection.with_device(|s| s.clear());

            let state = Arc::new(Mutex::new(TDisplayState::default()));

            // Hydrate from moss's HTTP API (COMPANION-0014).
            match moss.presence_snapshot().await {
                Ok(p) => {
                    let mut s = state.lock().await;
                    s.stone_name = p.stone.name.clone();
                    s.health = p.stone.health.clone();
                    s.cpu = p.stone.cpu_percent as u8;
                    s.mem = p.stone.memory_percent as u8;
                    s.disk = p.stone.disk_percent as u8;
                    s.io = p.stone.io_percent as u8;
                    s.gpu = p.stone.gpu_percent as u8;
                    s.gpu_active = bool_u8(p.stone.gpu_active);
                    s.uptime = p.stone.uptime_seconds;
                    s.offerings = p.offerings.len();
                    s.has_gpu = bool_u8(p.stone.has_gpu);
                    s.is_lantern = bool_u8(p.stone.is_lantern);
                    s.has_cricket = bool_u8(p.stone.has_cricket);
                    s.pond_active = bool_u8(p.stone.pond_active);
                    s.hour = p.stone.hour;
                    let snapshot = s.clone();
                    drop(s);
                    push_full_snapshot(&connection, &snapshot);
                }
                Err(e) => {
                    tracing::warn!(
                        port = %self.port_name,
                        error = %e,
                        "tdisplay hydrate from moss failed; will rely on live deltas"
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
    state: &Arc<Mutex<TDisplayState>>,
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
                from: "tdisplay".to_string(),
            }));
        }
        return;
    }

    match event.kind {
        "core.presence.snapshot" => {
            if let Some(p) = event.payload::<PresenceSnapshot>() {
                let snapshot = {
                    let mut s = state.lock().await;
                    s.stone_name = p.stone.name.clone();
                    s.health = p.stone.health.clone();
                    s.cpu = p.stone.cpu_percent as u8;
                    s.mem = p.stone.memory_percent as u8;
                    s.disk = p.stone.disk_percent as u8;
                    s.io = p.stone.io_percent as u8;
                    s.gpu = p.stone.gpu_percent as u8;
                    s.gpu_active = bool_u8(p.stone.gpu_active);
                    s.uptime = p.stone.uptime_seconds;
                    s.offerings = p.offerings.len();
                    s.has_gpu = bool_u8(p.stone.has_gpu);
                    s.is_lantern = bool_u8(p.stone.is_lantern);
                    s.has_cricket = bool_u8(p.stone.has_cricket);
                    s.pond_active = bool_u8(p.stone.pond_active);
                    s.hour = p.stone.hour;
                    s.clone()
                };
                push_full_snapshot(connection, &snapshot);
            }
        }
        "core.stone.health.changed" => {
            if let Some(p) = event.payload::<StoneHealthChangedPayload>() {
                state.lock().await.health = p.health.clone();
                let _ = connection.with_device(|s| s.tdisplay_health(&p.health));
            }
        }
        "core.stone.load.updated" => {
            if let Some(p) = event.payload::<StoneLoadUpdatedPayload>() {
                let (cpu, mem, disk, io, gpu, gpu_active) = {
                    let mut s = state.lock().await;
                    s.cpu = p.cpu_percent as u8;
                    s.mem = p.memory_percent as u8;
                    s.disk = p.disk_percent as u8;
                    s.io = p.io_percent as u8;
                    s.gpu = p.gpu_percent as u8;
                    s.gpu_active = bool_u8(p.gpu_active);
                    (s.cpu, s.mem, s.disk, s.io, s.gpu, p.gpu_active)
                };
                let _ = connection.with_device(|s| {
                    s.tdisplay_load(cpu, mem, disk, io, gpu, gpu_active)
                });
            }
        }
        "core.stone.tended" => {
            if let Some(p) = event.payload::<StoneTendedPayload>() {
                let by = if p.by.is_empty() { "unknown" } else { p.by.as_str() };
                let _ = connection.with_device(|s| s.tdisplay_tended(by, &p.from));
            }
        }
        "core.service.started" => {
            if let Some(p) = event.payload::<ServiceStartedPayload>() {
                state.lock().await.offerings =
                    state.lock().await.offerings.saturating_add(1);
                let _ = connection
                    .with_device(|s| s.tdisplay_service_started(&p.service, "healthy"));
            }
        }
        "core.service.stopped" => {
            if let Some(p) = event.payload::<ServiceStoppedPayload>() {
                state.lock().await.offerings =
                    state.lock().await.offerings.saturating_sub(1);
                let _ = connection.with_device(|s| s.tdisplay_service_stopped(&p.service));
            }
        }
        "core.storage.connected" => {
            if let Some(p) = event.payload::<StorageConnectedPayload>() {
                let _ = connection.with_device(|s| {
                    s.tdisplay_seed_bank_detected(&p.name, 0, p.capacity_gb)
                });
            }
        }
        "core.storage.removed" => {
            if event.payload::<StorageRemovedPayload>().is_some() {
                let _ = connection.with_device(|s| s.tdisplay_seed_bank_removed());
            }
        }
        _ => {}
    }
}

fn push_full_snapshot(connection: &FireflyConnection, state: &TDisplayState) {
    match serde_json::to_string(state) {
        Ok(json) => {
            let _ = connection.with_device(|s| s.tdisplay_json_push(&json));
        }
        Err(e) => {
            tracing::warn!(error = %e, "tdisplay: failed to serialize snapshot JSON");
        }
    }
}

fn bool_u8(b: bool) -> u8 {
    if b { 1 } else { 0 }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

async fn handle_command(
    args: &[String],
    connection: &Arc<FireflyConnection>,
    state: &Arc<Mutex<TDisplayState>>,
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
    fn bool_u8_rounds_trip() {
        assert_eq!(bool_u8(true), 1);
        assert_eq!(bool_u8(false), 0);
    }

    #[test]
    fn state_serializes_with_single_char_keys() {
        let state = TDisplayState {
            stone_name: "stone-x".into(),
            health: "thriving".into(),
            cpu: 42,
            mem: 55,
            ..Default::default()
        };
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("\"n\":\"stone-x\""), "stone_name key should be 'n'");
        assert!(json.contains("\"h\":\"thriving\""), "health key should be 'h'");
        assert!(json.contains("\"c\":42"), "cpu key should be 'c'");
        assert!(json.contains("\"m\":55"), "mem key should be 'm'");
    }

    #[test]
    fn subscriptions_include_command_and_presence() {
        assert!(TDISPLAY_SUBSCRIPTIONS.contains(&"core.command.invocation"));
        assert!(TDISPLAY_SUBSCRIPTIONS.contains(&"core.presence.snapshot"));
    }
}
