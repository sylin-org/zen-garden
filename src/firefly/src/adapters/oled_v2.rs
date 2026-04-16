//! ESP8266 OLED v2 adapter — dual-zone yellow/blue icon dashboard.
//!
//! Holds an `Arc<Firefly>`. Hydrates initial state from moss's HTTP
//! API; then selects on shutdown + Pulse events + device state
//! changes. A `DeviceState::Disposed` transition exits the loop.

use crate::firefly::Firefly;
use garden_common::command_manifest::CommandResponse;
use garden_common::presence::{PresenceSnapshot, StoneHealthChangedPayload, StoneLoadUpdatedPayload};
use garden_companion_sdk::adapters::{
    adapter::BoxFuture, Adapter, AdapterInfo, AdapterProfile, DeliveryPolicy,
};
use garden_companion_sdk::garden::{
    CommandInvocation, CommandOutcome, CommandResult, Event, Pulse, ServiceStartedPayload,
    ServiceStoppedPayload, StoneTendedPayload, StorageConnectedPayload, StorageRemovedPayload,
};
use garden_companion_sdk::moss_client::MossLocalClient;
use garden_companion_sdk::usb_devices::DeviceState;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

const SUBSCRIPTIONS: &[&str] = &[
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

#[derive(Debug, Default, Clone)]
struct DashboardState {
    stone_name: Option<String>,
    health_label: String,
    cpu_percent: u8,
    memory_percent: u8,
    disk_percent: u8,
    uptime_seconds: u64,
    offering_count: usize,
    net_bps: u64,
    has_seed_bank: bool,
}

pub struct OledV2Adapter {
    firefly: Arc<Firefly>,
}

impl OledV2Adapter {
    pub fn new(firefly: Arc<Firefly>) -> Self {
        Self { firefly }
    }
}

impl Adapter for OledV2Adapter {
    fn info(&self) -> AdapterInfo {
        AdapterInfo {
            kind: "firefly.oled-v2",
            id: self.firefly.device.id().to_string(),
            device: Some(format!("ESP8266-OLED-v2 on {}", self.firefly.device.port())),
        }
    }

    fn profile(&self) -> AdapterProfile {
        AdapterProfile {
            subscriptions: SUBSCRIPTIONS,
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
            let firefly = self.firefly;
            let port = firefly.device.port().to_string();
            let mut state_rx = firefly.device.state_changes();
            let _ = firefly.clear().await;

            let state = Arc::new(Mutex::new(DashboardState::default()));

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
                    s.net_bps = p.stone.net_rx_bytes_per_sec + p.stone.net_tx_bytes_per_sec;
                    s.has_seed_bank = p.stone.seed_bank.is_some();
                    let snapshot = s.clone();
                    drop(s);
                    push_full_snapshot(&firefly, &snapshot).await;
                }
                Err(e) => {
                    tracing::warn!(
                        port = %port,
                        error = %e,
                        "oled-v2 hydrate from moss failed; will rely on live deltas"
                    );
                }
            }

            loop {
                tokio::select! {
                    _ = shutdown.cancelled() => break,
                    changed = state_rx.changed() => {
                        if changed.is_err() || matches!(*state_rx.borrow(), DeviceState::Disposed) {
                            tracing::info!(port = %port, "device disposed; exiting");
                            break;
                        }
                    }
                    maybe = events.recv() => match maybe {
                        Some(event) => handle_event(&event, &firefly, &state, &pulse).await,
                        None => break,
                    }
                }
            }

            let _ = firefly.clear().await;
        })
    }
}

// ---------------------------------------------------------------------------
// Event dispatch
// ---------------------------------------------------------------------------

async fn handle_event(
    event: &Event,
    firefly: &Arc<Firefly>,
    state: &Arc<Mutex<DashboardState>>,
    pulse: &Arc<Pulse>,
) {
    if event.kind == "core.command.invocation" {
        if let Some(inv) = event.payload::<CommandInvocation>() {
            let response = handle_command(&inv.raw_args, firefly, state).await;
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
                let snapshot = {
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
                    s.clone()
                };
                push_full_snapshot(firefly, &snapshot).await;
            }
        }
        "core.stone.health.changed" => {
            if let Some(p) = event.payload::<StoneHealthChangedPayload>() {
                state.lock().await.health_label = p.health.clone();
                let _ = firefly.oled_health(&p.health).await;
            }
        }
        "core.stone.load.updated" => {
            if let Some(p) = event.payload::<StoneLoadUpdatedPayload>() {
                let snapshot = {
                    let mut s = state.lock().await;
                    s.cpu_percent = p.cpu_percent as u8;
                    s.memory_percent = p.memory_percent as u8;
                    s.disk_percent = p.disk_percent as u8;
                    s.net_bps = p.net_rx_bytes_per_sec + p.net_tx_bytes_per_sec;
                    s.clone()
                };
                push_dashboard(firefly, &snapshot).await;
            }
        }
        "core.stone.tended" => {
            if event.payload::<StoneTendedPayload>().is_some() {
                let _ = firefly.oled_wipe_in("ZEN GARDEN", "TENDING");
            }
        }
        "core.service.started" => {
            if let Some(p) = event.payload::<ServiceStartedPayload>() {
                let label = p.service.to_uppercase();
                let _ = firefly.oled_wipe_in(&label, "STARTED");
                let snapshot = {
                    let mut s = state.lock().await;
                    s.offering_count = s.offering_count.saturating_add(1);
                    s.clone()
                };
                push_dashboard(firefly, &snapshot).await;
            }
        }
        "core.service.stopped" => {
            if let Some(p) = event.payload::<ServiceStoppedPayload>() {
                let label = p.service.to_uppercase();
                let _ = firefly.oled_wipe_out(&label, "STOPPED");
                let snapshot = {
                    let mut s = state.lock().await;
                    s.offering_count = s.offering_count.saturating_sub(1);
                    s.clone()
                };
                push_dashboard(firefly, &snapshot).await;
            }
        }
        "core.storage.connected" => {
            if event.payload::<StorageConnectedPayload>().is_some() {
                state.lock().await.has_seed_bank = true;
                let _ = firefly.oled_wipe_in("STORAGE", "CONNECTED");
                let snapshot = state.lock().await.clone();
                push_dashboard(firefly, &snapshot).await;
            }
        }
        "core.storage.removed" => {
            if event.payload::<StorageRemovedPayload>().is_some() {
                state.lock().await.has_seed_bank = false;
                let _ = firefly.oled_wipe_out("SEED BANK", "REMOVED");
                let snapshot = state.lock().await.clone();
                push_dashboard(firefly, &snapshot).await;
            }
        }
        _ => {}
    }
}

async fn push_full_snapshot(firefly: &Firefly, state: &DashboardState) {
    let Some(name) = &state.stone_name else {
        return;
    };
    let _ = firefly.oled_stone_name(name).await;
    let _ = firefly.oled_health(&state.health_label).await;
    let _ = firefly
        .oled_v2_dashboard(
            state.cpu_percent,
            state.memory_percent,
            state.disk_percent,
            &format_uptime(state.uptime_seconds),
            state.offering_count,
            0,
            state.net_bps,
            state.has_seed_bank,
        )
        .await;
}

async fn push_dashboard(firefly: &Firefly, state: &DashboardState) {
    let _ = firefly
        .oled_v2_dashboard(
            state.cpu_percent,
            state.memory_percent,
            state.disk_percent,
            &format_uptime(state.uptime_seconds),
            state.offering_count,
            0,
            state.net_bps,
            state.has_seed_bank,
        )
        .await;
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

async fn handle_command(
    args: &[String],
    firefly: &Arc<Firefly>,
    state: &Arc<Mutex<DashboardState>>,
) -> CommandResponse {
    let Some((cmd, rest)) = args.split_first() else {
        return CommandResponse::error("No command provided");
    };
    match cmd.to_lowercase().as_str() {
        "clear" => match firefly.clear().await {
            Ok(_) => CommandResponse::success("Display cleared"),
            Err(e) => CommandResponse::error(format!("Device error: {e}")),
        },
        "brightness" | "bright" | "dim" => {
            let Some(raw) = rest.first() else {
                return CommandResponse::error("Usage: brightness <0-100>");
            };
            let Ok(v) = raw.parse::<u8>() else {
                return CommandResponse::error("Brightness must be 0-100");
            };
            match firefly.brightness(v).await {
                Ok(_) => CommandResponse::success(format!("Brightness {}%", v)),
                Err(e) => CommandResponse::error(format!("Device error: {e}")),
            }
        }
        "info" => match firefly.info().await {
            Ok(r) => CommandResponse::success(r),
            Err(e) => CommandResponse::error(format!("Device error: {e}")),
        },
        "refresh" => {
            let snapshot = state.lock().await.clone();
            push_full_snapshot(firefly, &snapshot).await;
            CommandResponse::success("Dashboard refreshed")
        }
        other => CommandResponse::error(format!("Unknown command: {other}")),
    }
}

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
}
