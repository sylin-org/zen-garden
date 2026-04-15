// Legacy factory retained for backward compatibility during the bus
// migration window; bus_registrations() is the supported path.
#![allow(dead_code)]

//! ESP8266 OLED v1 adapter (`firefly-oled` firmware, dual-zone 128x64).
//!
//! Protocol is simpler than v2: individual S (stone name), H (health),
//! M (cpu, mem, uptime) frames rather than a packed D dashboard. No
//! disk bar, seed-bank icon, or offering counter — those land in v2.
//!
//! Factory shares VID 0x1a86 with OLED v2 and T-Display; discrimination
//! happens via the `I` probe. Only ports whose firmware reports
//! `firefly-oled` (without `-v2`) are claimed here.

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

const OLED_V1_SUBSCRIPTIONS: &[&str] = &[
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

/// Cached values needed to recover the display after reconnect. v1
/// only surfaces stone name + health + cpu/mem/uptime on its metric
/// frame.
#[derive(Debug, Default, Clone)]
struct V1State {
    stone_name: Option<String>,
    health_label: String,
    cpu_percent: u8,
    memory_percent: u8,
    uptime_seconds: u64,
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

pub struct OledV1Factory {
    preferred_port: Option<String>,
    /// Ports confirmed as v1 firmware on a previous discovery tick.
    /// Keeps the adapter present across ticks even while it holds the
    /// port open (re-probing would fail with "access denied" and the
    /// supervisor would reap + respawn — causing visible blank/refill
    /// churn on the display).
    claimed: StdMutex<HashSet<String>>,
}

impl OledV1Factory {
    pub fn new(preferred_port: Option<String>) -> Self {
        Self {
            preferred_port,
            claimed: StdMutex::new(HashSet::new()),
        }
    }
}

impl AdapterFactory for OledV1Factory {
    fn kind(&self) -> &'static str {
        "firefly.oled-v1"
    }

    fn discover(&self) -> Vec<Box<dyn Adapter>> {
        let devices = match find_firefly_devices() {
            Ok(d) => d,
            Err(e) => {
                tracing::debug!(error = %e, "oled-v1 discovery: USB scan failed");
                return Vec::new();
            }
        };

        let current_ports: HashSet<String> = devices
            .iter()
            .filter(|d| d.device_type == FireflyDeviceType::Esp8266Oled)
            .map(|d| d.port_name.clone())
            .collect();

        // Prune claimed entries whose ports have physically vanished.
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
                        Box::new(OledV1Adapter::new(d.port_name)) as Box<dyn Adapter>
                    );
                }
                match probe_is_v1(&d.port_name) {
                    Ok(true) => {
                        self.claimed.lock().unwrap().insert(d.port_name.clone());
                        Some(Box::new(OledV1Adapter::new(d.port_name)) as Box<dyn Adapter>)
                    }
                    Ok(false) => None,
                    Err(e) => {
                        tracing::debug!(
                            port = %d.port_name,
                            error = %e,
                            "oled-v1 probe failed — leaving for another factory"
                        );
                        None
                    }
                }
            })
            .collect()
    }
}

/// Return true when the port reports `firefly-oled` firmware (v1),
/// explicitly excluding `firefly-oled-v2`.
fn probe_is_v1(port_name: &str) -> anyhow::Result<bool> {
    let serial = FireflySerial::new(port_name, FireflyDeviceType::Esp8266Oled)?;
    let response = serial.info()?;
    let refined = FireflyDeviceType::refine_from_info(FireflyDeviceType::Esp8266Oled, &response);
    Ok(refined == FireflyDeviceType::Esp8266Oled && response.contains("firefly-oled"))
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

pub struct OledV1Adapter {
    port_name: String,
    prebuilt: Option<Arc<FireflyConnection>>,
}

impl OledV1Adapter {
    pub fn new(port_name: String) -> Self {
        Self {
            port_name,
            prebuilt: None,
        }
    }

    /// Construct from a pre-built bus connection (skips try_connect).
    pub fn from_connection(connection: Arc<FireflyConnection>, port_name: String) -> Self {
        Self {
            port_name,
            prebuilt: Some(connection),
        }
    }
}

impl Adapter for OledV1Adapter {
    fn info(&self) -> AdapterInfo {
        AdapterInfo {
            kind: "firefly.oled-v1",
            id: self.port_name.clone(),
            device: Some(format!("ESP8266-OLED on {}", self.port_name)),
        }
    }

    fn profile(&self) -> AdapterProfile {
        AdapterProfile {
            subscriptions: OLED_V1_SUBSCRIPTIONS,
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
                            "oled-v1 adapter could not open device"
                        );
                        return;
                    }
                    conn
                }
            };
            let _ = connection.with_device(|s| s.clear());

            let state = Arc::new(Mutex::new(V1State::default()));

            // Hydrate from moss's HTTP API (COMPANION-0014).
            match moss.presence_snapshot().await {
                Ok(p) => {
                    let mut s = state.lock().await;
                    s.stone_name = Some(p.stone.name.clone());
                    s.health_label = p.stone.health.clone();
                    s.cpu_percent = p.stone.cpu_percent as u8;
                    s.memory_percent = p.stone.memory_percent as u8;
                    s.uptime_seconds = p.stone.uptime_seconds;
                    let snapshot = s.clone();
                    drop(s);
                    push_full_snapshot(&connection, &snapshot);
                }
                Err(e) => {
                    tracing::warn!(
                        port = %self.port_name,
                        error = %e,
                        "oled-v1 hydrate from moss failed; will rely on live deltas"
                    );
                }
            }

            let mut health = tokio::time::interval(std::time::Duration::from_secs(5));
            health.tick().await; // consume immediate tick

            loop {
                tokio::select! {
                    _ = shutdown.cancelled() => break,
                    _ = health.tick() => {
                        // COMPANION-0015: detect silent replug (fd alive,
                        // device gone). Self-exit → bus re-identifies.
                        if connection.is_lost() {
                            tracing::warn!(
                                port = ?connection.port_name(),
                                "connection appears lost — self-exiting for re-identification"
                            );
                            break;
                        }
                    }
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
    state: &Arc<Mutex<V1State>>,
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
                from: "oled-v1".to_string(),
            }));
        }
        return;
    }

    match event.kind {
        "core.presence.snapshot" => {
            if let Some(p) = event.payload::<PresenceSnapshot>() {
                {
                    let mut s = state.lock().await;
                    s.stone_name = Some(p.stone.name.clone());
                    s.health_label = p.stone.health.clone();
                    s.cpu_percent = p.stone.cpu_percent as u8;
                    s.memory_percent = p.stone.memory_percent as u8;
                    s.uptime_seconds = p.stone.uptime_seconds;
                }
                let snapshot = state.lock().await.clone();
                push_full_snapshot(connection, &snapshot);
            }
        }
        "core.stone.health.changed" => {
            if let Some(p) = event.payload::<StoneHealthChangedPayload>() {
                state.lock().await.health_label = p.health.clone();
                let _ = connection.with_device(|s| s.oled_health(&p.health));
            }
        }
        "core.stone.load.updated" => {
            if let Some(p) = event.payload::<StoneLoadUpdatedPayload>() {
                let snapshot = {
                    let mut s = state.lock().await;
                    s.cpu_percent = p.cpu_percent as u8;
                    s.memory_percent = p.memory_percent as u8;
                    s.clone()
                };
                push_metrics(connection, &snapshot);
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
            }
        }
        "core.service.stopped" => {
            if let Some(p) = event.payload::<ServiceStoppedPayload>() {
                let label = p.service.to_uppercase();
                let _ = connection.with_device(|s| s.oled_wipe_out(&label, "STOPPED"));
            }
        }
        "core.storage.connected" => {
            if event.payload::<StorageConnectedPayload>().is_some() {
                let _ = connection.with_device(|s| s.oled_wipe_in("STORAGE", "CONNECTED"));
            }
        }
        "core.storage.removed" => {
            if event.payload::<StorageRemovedPayload>().is_some() {
                let _ = connection.with_device(|s| s.oled_wipe_out("SEED BANK", "REMOVED"));
            }
        }
        _ => {}
    }
}

fn push_full_snapshot(connection: &FireflyConnection, state: &V1State) {
    if let Some(name) = &state.stone_name {
        let uptime = format_uptime(state.uptime_seconds);
        let _ = connection.with_device(|s| {
            s.oled_stone_name(name)?;
            s.oled_health(&state.health_label)?;
            s.oled_metrics(state.cpu_percent, state.memory_percent, &uptime)
        });
    }
}

fn push_metrics(connection: &FireflyConnection, state: &V1State) {
    let uptime = format_uptime(state.uptime_seconds);
    let _ = connection.with_device(|s| {
        s.oled_metrics(state.cpu_percent, state.memory_percent, &uptime)
    });
}

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
    state: &Arc<Mutex<V1State>>,
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
            CommandResponse::success("Display refreshed")
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
        assert_eq!(format_uptime(30), "30s");
        assert_eq!(format_uptime(3600), "1h");
    }

    #[test]
    fn subscriptions_include_command_and_presence() {
        assert!(OLED_V1_SUBSCRIPTIONS.contains(&"core.command.invocation"));
        assert!(OLED_V1_SUBSCRIPTIONS.contains(&"core.presence.snapshot"));
    }
}
