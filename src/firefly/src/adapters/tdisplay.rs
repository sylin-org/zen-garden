//! ESP32 T-Display adapter (`firefly-tdisplay` firmware, 135×240 ST7789).
//!
//! Full-color TFT driven by a compact JSON push (single-char keys to
//! minimise serial transfer) plus narrow per-kind update frames.

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
use serde::Serialize;
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

pub struct TDisplayAdapter {
    firefly: Arc<Firefly>,
}

impl TDisplayAdapter {
    pub fn new(firefly: Arc<Firefly>) -> Self {
        Self { firefly }
    }
}

impl Adapter for TDisplayAdapter {
    fn info(&self) -> AdapterInfo {
        AdapterInfo {
            kind: "firefly.tdisplay",
            id: self.firefly.device.id().to_string(),
            device: Some(format!("ESP32-TDisplay on {}", self.firefly.device.port())),
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

            let state = Arc::new(Mutex::new(TDisplayState::default()));

            match moss.presence_snapshot().await {
                Ok(p) => {
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
                    push_full_snapshot(&firefly, &snapshot).await;
                }
                Err(e) => {
                    tracing::warn!(port = %port, error = %e, "tdisplay hydrate failed");
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

async fn handle_event(
    event: &Event,
    firefly: &Arc<Firefly>,
    state: &Arc<Mutex<TDisplayState>>,
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
                push_full_snapshot(firefly, &snapshot).await;
            }
        }
        "core.stone.health.changed" => {
            if let Some(p) = event.payload::<StoneHealthChangedPayload>() {
                state.lock().await.health = p.health.clone();
                let _ = firefly.tdisplay_health(&p.health).await;
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
                let _ = firefly
                    .tdisplay_load(cpu, mem, disk, io, gpu, gpu_active)
                    .await;
            }
        }
        "core.stone.tended" => {
            if let Some(p) = event.payload::<StoneTendedPayload>() {
                let by = if p.by.is_empty() { "unknown" } else { p.by.as_str() };
                let _ = firefly.tdisplay_tended(by, &p.from);
            }
        }
        "core.service.started" => {
            if let Some(p) = event.payload::<ServiceStartedPayload>() {
                {
                    let mut s = state.lock().await;
                    s.offerings = s.offerings.saturating_add(1);
                }
                let _ = firefly.tdisplay_service_started(&p.service, "healthy");
            }
        }
        "core.service.stopped" => {
            if let Some(p) = event.payload::<ServiceStoppedPayload>() {
                {
                    let mut s = state.lock().await;
                    s.offerings = s.offerings.saturating_sub(1);
                }
                let _ = firefly.tdisplay_service_stopped(&p.service);
            }
        }
        "core.storage.connected" => {
            if let Some(p) = event.payload::<StorageConnectedPayload>() {
                let _ = firefly.tdisplay_seed_bank_detected(&p.name, 0, p.capacity_gb);
            }
        }
        "core.storage.removed" => {
            if event.payload::<StorageRemovedPayload>().is_some() {
                let _ = firefly.tdisplay_seed_bank_removed();
            }
        }
        _ => {}
    }
}

async fn push_full_snapshot(firefly: &Firefly, state: &TDisplayState) {
    match serde_json::to_string(state) {
        Ok(json) => {
            let _ = firefly.tdisplay_json_push(&json).await;
        }
        Err(e) => {
            tracing::warn!(error = %e, "tdisplay: failed to serialize snapshot JSON");
        }
    }
}

fn bool_u8(b: bool) -> u8 {
    if b { 1 } else { 0 }
}

async fn handle_command(
    args: &[String],
    firefly: &Arc<Firefly>,
    state: &Arc<Mutex<TDisplayState>>,
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
    fn bool_u8_roundtrips() {
        assert_eq!(bool_u8(true), 1);
        assert_eq!(bool_u8(false), 0);
    }

    #[test]
    fn state_uses_single_char_keys() {
        let state = TDisplayState {
            stone_name: "stone-x".into(),
            health: "thriving".into(),
            cpu: 42,
            ..Default::default()
        };
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("\"n\":\"stone-x\""));
        assert!(json.contains("\"h\":\"thriving\""));
        assert!(json.contains("\"c\":42"));
    }
}
