//! RP2040 5×5 LED matrix adapter.

use crate::animation::{start_animation, Animation, Health, Override};
use crate::firefly::{parse_color, Firefly};
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
use tokio::sync::{mpsc, RwLock};
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

pub struct MatrixAdapter {
    firefly: Arc<Firefly>,
    state_dir: Option<std::path::PathBuf>,
}

impl MatrixAdapter {
    pub fn new(firefly: Arc<Firefly>, state_dir: Option<std::path::PathBuf>) -> Self {
        Self { firefly, state_dir }
    }
}

impl Adapter for MatrixAdapter {
    fn info(&self) -> AdapterInfo {
        AdapterInfo {
            kind: "firefly.matrix",
            id: self.firefly.device.id().to_string(),
            device: Some(format!("RP2040-Matrix on {}", self.firefly.device.port())),
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
            let Self { firefly, state_dir } = *self;
            let port = firefly.device.port().to_string();
            let mut state_rx = firefly.device.state_changes();
            let _ = firefly.clear().await;

            let animation = Arc::new(RwLock::new(Animation::new(state_dir)));

            match moss.presence_snapshot().await {
                Ok(p) => {
                    let mut ctx = animation.write().await;
                    ctx.stone_name = Some(p.stone.name.clone());
                    ctx.health_label = p.stone.health.clone();
                    ctx.health = parse_health(&ctx.health_label);
                    ctx.cpu_percent = p.stone.cpu_percent as u8;
                    ctx.memory_percent = p.stone.memory_percent as u8;
                    ctx.disk_percent = p.stone.disk_percent as u8;
                    ctx.io_percent = p.stone.io_percent as u8;
                    ctx.gpu_percent = p.stone.gpu_percent as u8;
                    ctx.gpu_active = p.stone.gpu_active;
                    ctx.uptime_seconds = p.stone.uptime_seconds;
                    ctx.load = ((ctx.cpu_percent as f32 + ctx.memory_percent as f32) / 200.0)
                        .clamp(0.0, 1.0);
                    ctx.offering_count = p.offerings.len();
                    ctx.has_services = !p.offerings.is_empty();
                    ctx.has_seed_bank = p.stone.seed_bank.is_some();
                    trigger_health(&mut ctx);
                }
                Err(e) => {
                    tracing::warn!(port = %port, error = %e, "matrix hydrate failed");
                }
            }

            let engine = start_animation(Arc::clone(&firefly), animation.clone());

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
                        Some(event) => handle_event(&event, &firefly, &animation, &pulse).await,
                        None => break,
                    }
                }
            }

            engine.abort();
            let _ = firefly.clear().await;
        })
    }
}

async fn handle_event(
    event: &Event,
    firefly: &Arc<Firefly>,
    animation: &Arc<RwLock<Animation>>,
    pulse: &Arc<Pulse>,
) {
    if event.kind == "core.command.invocation" {
        if let Some(inv) = event.payload::<CommandInvocation>() {
            let response = handle_command(&inv.raw_args, firefly, animation).await;
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
                from: "matrix".to_string(),
            }));
        }
        return;
    }

    let mut ctx = animation.write().await;
    match event.kind {
        "core.presence.snapshot" => {
            if let Some(p) = event.payload::<PresenceSnapshot>() {
                ctx.stone_name = Some(p.stone.name.clone());
                ctx.uptime_seconds = p.stone.uptime_seconds;
                ctx.health_label = p.stone.health.clone();
                ctx.health = parse_health(&p.stone.health);
                ctx.load = ((p.stone.cpu_percent + p.stone.memory_percent) / 200.0) as f32;
                ctx.load = ctx.load.clamp(0.0, 1.0);
                ctx.cpu_percent = p.stone.cpu_percent as u8;
                ctx.memory_percent = p.stone.memory_percent as u8;
                ctx.disk_percent = p.stone.disk_percent as u8;
                ctx.io_percent = p.stone.io_percent as u8;
                ctx.gpu_percent = p.stone.gpu_percent as u8;
                ctx.gpu_active = p.stone.gpu_active;
                ctx.has_gpu = p.stone.has_gpu;
                ctx.is_lantern = p.stone.is_lantern;
                ctx.has_cricket = p.stone.has_cricket;
                ctx.pond_active = p.stone.pond_active;
                ctx.hour = p.stone.hour;
                ctx.offering_count = p.offerings.len();
                ctx.has_services = !p.offerings.is_empty();
                ctx.has_seed_bank = p.stone.seed_bank.is_some();
                trigger_health(&mut ctx);
            }
        }
        "core.stone.health.changed" => {
            if let Some(p) = event.payload::<StoneHealthChangedPayload>() {
                ctx.health = parse_health(&p.health);
                ctx.health_label = p.health.clone();
                trigger_health(&mut ctx);
            }
        }
        "core.stone.load.updated" => {
            if let Some(p) = event.payload::<StoneLoadUpdatedPayload>() {
                ctx.load = ((p.cpu_percent + p.memory_percent) / 200.0) as f32;
                ctx.load = ctx.load.clamp(0.0, 1.0);
                ctx.cpu_percent = p.cpu_percent as u8;
                ctx.memory_percent = p.memory_percent as u8;
                ctx.disk_percent = p.disk_percent as u8;
                ctx.io_percent = p.io_percent as u8;
                ctx.gpu_percent = p.gpu_percent as u8;
                ctx.gpu_active = p.gpu_active;
            }
        }
        "core.stone.tended" => {
            if event.payload::<StoneTendedPayload>().is_some() {
                ctx.trigger_override(Override::Tended);
            }
        }
        "core.service.started" => {
            if event.payload::<ServiceStartedPayload>().is_some() {
                ctx.has_services = true;
                ctx.trigger_override(Override::ServiceStarted);
            }
        }
        "core.service.stopped" => {
            if event.payload::<ServiceStoppedPayload>().is_some() {
                ctx.trigger_override(Override::ServiceStopped);
            }
        }
        "core.storage.connected" => {
            if event.payload::<StorageConnectedPayload>().is_some() {
                ctx.has_seed_bank = true;
                ctx.trigger_override(Override::StorageDetected);
            }
        }
        "core.storage.removed" => {
            if event.payload::<StorageRemovedPayload>().is_some() {
                ctx.has_seed_bank = false;
                ctx.trigger_override(Override::StorageRemoved);
            }
        }
        _ => {}
    }
}

fn parse_health(s: &str) -> Health {
    match s {
        "withering" => Health::Withering,
        "wilting" => Health::Wilting,
        _ => Health::Thriving,
    }
}

fn trigger_health(ctx: &mut Animation) {
    match ctx.health {
        Health::Withering => ctx.trigger_override(Override::HealthWarning),
        Health::Wilting => ctx.trigger_override(Override::HealthError),
        Health::Thriving => ctx.clear_override(),
    }
}

// ---------------------------------------------------------------------------
// Command dispatch
// ---------------------------------------------------------------------------

async fn handle_command(
    args: &[String],
    firefly: &Arc<Firefly>,
    animation: &Arc<RwLock<Animation>>,
) -> CommandResponse {
    let Some((cmd, rest)) = args.split_first() else {
        return CommandResponse::error("No command provided");
    };
    let cmd = cmd.to_lowercase();
    match cmd.as_str() {
        "status" | "state" => cmd_status(rest, firefly).await,
        "pixel" | "px" => cmd_pixel(rest, firefly).await,
        "fill" => cmd_fill(rest, firefly).await,
        "clear" => cmd_clear(firefly).await,
        "brightness" | "bright" | "dim" => cmd_brightness(rest, animation).await,
        "animate" | "anim" | "animation" => cmd_animate(rest, firefly).await,
        "stop" => cmd_stop(firefly).await,
        "info" => cmd_info(firefly).await,
        "on" => cmd_on(animation).await,
        "off" => cmd_off(firefly, animation).await,
        _ => CommandResponse::error(format!("Unknown command: {}", cmd)),
    }
}

async fn cmd_status(args: &[String], firefly: &Firefly) -> CommandResponse {
    let Some(state) = args.first() else {
        return CommandResponse::error("Usage: status <healthy|warning|error|offline>");
    };
    let state = state.to_lowercase();
    if !["healthy", "warning", "error", "offline"].contains(&state.as_str()) {
        return CommandResponse::error(format!("Invalid status: {}", state));
    }
    match firefly.status(&state).await {
        Ok(_) => CommandResponse::success(format!("Status set to {state}")),
        Err(e) => CommandResponse::error(format!("Device error: {e}")),
    }
}

async fn cmd_pixel(args: &[String], firefly: &Firefly) -> CommandResponse {
    if args.len() < 3 {
        return CommandResponse::error("Usage: pixel <x> <y> <color>");
    }
    let Ok(x) = args[0].parse::<u8>() else {
        return CommandResponse::error("x must be 0-4");
    };
    let Ok(y) = args[1].parse::<u8>() else {
        return CommandResponse::error("y must be 0-4");
    };
    if x > 4 || y > 4 {
        return CommandResponse::error("x and y must be 0-4");
    }
    let (r, g, b) = match parse_color(&args[2]) {
        Ok(c) => c,
        Err(e) => return CommandResponse::error(format!("Invalid color: {e}")),
    };
    match firefly.pixel(x, y, r, g, b).await {
        Ok(_) => CommandResponse::success(format!("Pixel ({x},{y}) set to RGB({r},{g},{b})")),
        Err(e) => CommandResponse::error(format!("Device error: {e}")),
    }
}

async fn cmd_fill(args: &[String], firefly: &Firefly) -> CommandResponse {
    let Some(color) = args.first() else {
        return CommandResponse::error("Usage: fill <color>");
    };
    let (r, g, b) = match parse_color(color) {
        Ok(c) => c,
        Err(e) => return CommandResponse::error(format!("Invalid color: {e}")),
    };
    match firefly.fill(r, g, b).await {
        Ok(_) => CommandResponse::success(format!("Filled with RGB({r},{g},{b})")),
        Err(e) => CommandResponse::error(format!("Device error: {e}")),
    }
}

async fn cmd_clear(firefly: &Firefly) -> CommandResponse {
    match firefly.clear().await {
        Ok(_) => CommandResponse::success("Display cleared"),
        Err(e) => CommandResponse::error(format!("Device error: {e}")),
    }
}

async fn cmd_brightness(args: &[String], animation: &Arc<RwLock<Animation>>) -> CommandResponse {
    let Some(raw) = args.first() else {
        let ctx = animation.read().await;
        return CommandResponse::success(format!("Current brightness: {}%", ctx.brightness));
    };
    let Ok(v) = raw.parse::<u8>() else {
        return CommandResponse::error("Brightness must be 0-100");
    };
    if v > 100 {
        return CommandResponse::error("Brightness must be 0-100");
    }
    animation.write().await.set_brightness(v);
    CommandResponse::success(format!("Brightness set to {v}% (saved)"))
}

async fn cmd_animate(args: &[String], firefly: &Firefly) -> CommandResponse {
    let Some(name) = args.first() else {
        return CommandResponse::error("Usage: animate <rainbow|pulse|chase|sparkle>");
    };
    let name = name.to_lowercase();
    if !["rainbow", "pulse", "chase", "sparkle"].contains(&name.as_str()) {
        return CommandResponse::error(format!("Unknown animation: {name}"));
    }
    match firefly.animate(&name).await {
        Ok(_) => CommandResponse::success(format!("Playing animation: {name}")),
        Err(e) => CommandResponse::error(format!("Device error: {e}")),
    }
}

async fn cmd_stop(firefly: &Firefly) -> CommandResponse {
    match firefly.stop().await {
        Ok(_) => CommandResponse::success("Animation stopped"),
        Err(e) => CommandResponse::error(format!("Device error: {e}")),
    }
}

async fn cmd_info(firefly: &Firefly) -> CommandResponse {
    match firefly.info().await {
        Ok(response) => CommandResponse::success(response),
        Err(e) => CommandResponse::error(format!("Communication error: {e}")),
    }
}

async fn cmd_on(animation: &Arc<RwLock<Animation>>) -> CommandResponse {
    animation.write().await.enabled = true;
    CommandResponse::success("Firefly enabled — animation running")
}

async fn cmd_off(firefly: &Firefly, animation: &Arc<RwLock<Animation>>) -> CommandResponse {
    animation.write().await.enabled = false;
    let _ = firefly.clear().await;
    CommandResponse::success("Firefly disabled — display cleared")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_health_maps_known_values() {
        assert_eq!(parse_health("thriving"), Health::Thriving);
        assert_eq!(parse_health("withering"), Health::Withering);
        assert_eq!(parse_health("wilting"), Health::Wilting);
        assert_eq!(parse_health("unknown"), Health::Thriving);
    }
}
