//! ESP8266 OLED v1 adapter (simpler stone/health/metric frames).

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
use garden_companion_usb::DeviceState;
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
struct V1State {
    stone_name: Option<String>,
    health_label: String,
    cpu_percent: u8,
    memory_percent: u8,
    uptime_seconds: u64,
}

pub struct OledV1Adapter {
    firefly: Arc<Firefly>,
}

impl OledV1Adapter {
    pub fn new(firefly: Arc<Firefly>) -> Self {
        Self { firefly }
    }
}

impl Adapter for OledV1Adapter {
    fn info(&self) -> AdapterInfo {
        AdapterInfo {
            kind: "firefly.oled-v1",
            id: self.firefly.device.id().to_string(),
            device: Some(format!("ESP8266-OLED on {}", self.firefly.device.port())),
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

            let state = Arc::new(Mutex::new(V1State::default()));

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
                    push_full_snapshot(&firefly, &snapshot).await;
                }
                Err(e) => {
                    tracing::warn!(port = %port, error = %e, "oled-v1 hydrate failed");
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
    state: &Arc<Mutex<V1State>>,
    pulse: &Arc<Pulse>,
) {
    if event.kind == "core.command.invocation" {
        if let Some(inv) = event.payload::<CommandInvocation>() {
            let response = handle_command(&inv.raw_args, firefly).await;
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
                let snapshot = {
                    let mut s = state.lock().await;
                    s.stone_name = Some(p.stone.name.clone());
                    s.health_label = p.stone.health.clone();
                    s.cpu_percent = p.stone.cpu_percent as u8;
                    s.memory_percent = p.stone.memory_percent as u8;
                    s.uptime_seconds = p.stone.uptime_seconds;
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
                    s.clone()
                };
                let _ = firefly
                    .oled_metrics(
                        snapshot.cpu_percent,
                        snapshot.memory_percent,
                        &format_uptime(snapshot.uptime_seconds),
                    )
                    .await;
            }
        }
        "core.stone.tended" => {
            if event.payload::<StoneTendedPayload>().is_some() {
                let _ = firefly.oled_wipe_in("ZEN GARDEN", "TENDING").await;
            }
        }
        "core.service.started" => {
            if let Some(p) = event.payload::<ServiceStartedPayload>() {
                let _ = firefly.oled_wipe_in(&p.service.to_uppercase(), "STARTED").await;
            }
        }
        "core.service.stopped" => {
            if let Some(p) = event.payload::<ServiceStoppedPayload>() {
                let _ = firefly.oled_wipe_out(&p.service.to_uppercase(), "STOPPED").await;
            }
        }
        "core.storage.connected" => {
            if event.payload::<StorageConnectedPayload>().is_some() {
                let _ = firefly.oled_wipe_in("STORAGE", "CONNECTED").await;
            }
        }
        "core.storage.removed" => {
            if event.payload::<StorageRemovedPayload>().is_some() {
                let _ = firefly.oled_wipe_out("SEED BANK", "REMOVED").await;
            }
        }
        _ => {}
    }
}

async fn push_full_snapshot(firefly: &Firefly, state: &V1State) {
    if let Some(name) = &state.stone_name {
        let _ = firefly.oled_stone_name(name).await;
    }
    let _ = firefly.oled_health(&state.health_label).await;
    let _ = firefly
        .oled_metrics(
            state.cpu_percent,
            state.memory_percent,
            &format_uptime(state.uptime_seconds),
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

async fn handle_command(args: &[String], firefly: &Arc<Firefly>) -> CommandResponse {
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
        other => CommandResponse::error(format!("Unknown command: {other}")),
    }
}
