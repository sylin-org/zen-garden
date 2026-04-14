// Legacy factory (MatrixFactory) retained while the migration from
// factory-based to bus-based discovery settles. The bus path in
// `adapters::bus_registrations` is the supported entry.
#![allow(dead_code)]

//! RP2040 5×5 LED matrix adapter.
//!
//! One [`MatrixAdapter`] per detected RP2040 device. The adapter owns:
//!
//! - A [`FireflyConnection`] bound to a single port (legacy wrapper kept
//!   for its hot-unplug detection — auto-marks-disconnected on I/O fail).
//! - The mutable [`Animation`] state that the engine reads each frame.
//! - A spawned animation engine task whose handle is aborted on shutdown.
//!
//! Subscriptions cover every presence kind that a matrix-visible
//! override can react to, plus the command channel.

use crate::animation::{start_animation, Animation, Health, Override};
use crate::serial::{
    find_firefly_devices, parse_color, FireflyConnection, FireflyDeviceType,
};
use garden_common::command_manifest::CommandResponse;
use garden_common::presence::{
    PresenceSnapshot, StoneHealthChangedPayload, StoneLoadUpdatedPayload,
};
use garden_companion_sdk::adapters::{
    Adapter, AdapterFactory, AdapterInfo, AdapterProfile, DeliveryPolicy, adapter::BoxFuture,
};
use garden_companion_sdk::garden::{
    CommandInvocation, CommandOutcome, CommandResult, Event, Garden, Pulse,
    ServiceStartedPayload, ServiceStoppedPayload, StoneTendedPayload, StorageConnectedPayload,
    StorageRemovedPayload,
};
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use tokio_util::sync::CancellationToken;

/// All kinds the matrix adapter cares about.
const MATRIX_SUBSCRIPTIONS: &[&str] = &[
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
// Factory
// ---------------------------------------------------------------------------

/// Discovers connected RP2040 matrix devices and produces a
/// [`MatrixAdapter`] per device. The supervisor's `(kind, id)` dedup
/// keeps a single adapter alive per port.
pub struct MatrixFactory {
    /// Optional user-pinned port. When set, only that port is considered.
    preferred_port: Option<String>,
    /// State directory for animation persistence.
    state_dir: Option<std::path::PathBuf>,
}

impl MatrixFactory {
    pub fn new(preferred_port: Option<String>, state_dir: Option<std::path::PathBuf>) -> Self {
        Self {
            preferred_port,
            state_dir,
        }
    }
}

impl AdapterFactory for MatrixFactory {
    fn kind(&self) -> &'static str {
        "firefly.matrix"
    }

    fn discover(&self) -> Vec<Box<dyn Adapter>> {
        let devices = match find_firefly_devices() {
            Ok(d) => d,
            Err(e) => {
                tracing::debug!(error = %e, "matrix discovery: USB scan failed");
                return Vec::new();
            }
        };
        devices
            .into_iter()
            .filter(|d| d.device_type == FireflyDeviceType::Rp2040Matrix)
            .filter(|d| {
                self.preferred_port
                    .as_ref()
                    .is_none_or(|p| p.eq_ignore_ascii_case(&d.port_name))
            })
            .map(|d| {
                Box::new(MatrixAdapter::new(d.port_name, self.state_dir.clone()))
                    as Box<dyn Adapter>
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// Single-device matrix adapter. Lives for the duration the device is
/// plugged in; exits its run loop when the device disconnects so the
/// supervisor reaps and respawns on next discovery.
pub struct MatrixAdapter {
    port_name: String,
    state_dir: Option<std::path::PathBuf>,
    /// Pre-built connection supplied by the device bus. When `Some`,
    /// `run()` skips the open-then-try_connect path and adopts the
    /// bus's already-identified port directly.
    prebuilt: Option<Arc<FireflyConnection>>,
}

impl MatrixAdapter {
    pub fn new(port_name: String, state_dir: Option<std::path::PathBuf>) -> Self {
        Self {
            port_name,
            state_dir,
            prebuilt: None,
        }
    }

    /// Construct from a pre-built connection (bus integration path).
    pub fn from_connection(
        connection: Arc<FireflyConnection>,
        port_name: String,
        state_dir: Option<std::path::PathBuf>,
    ) -> Self {
        Self {
            port_name,
            state_dir,
            prebuilt: Some(connection),
        }
    }
}

impl Adapter for MatrixAdapter {
    fn info(&self) -> AdapterInfo {
        AdapterInfo {
            kind: "firefly.matrix",
            id: self.port_name.clone(),
            device: Some(format!("RP2040-Matrix on {}", self.port_name)),
        }
    }

    fn profile(&self) -> AdapterProfile {
        AdapterProfile {
            subscriptions: MATRIX_SUBSCRIPTIONS,
            delivery: DeliveryPolicy::All,
            persisted_state: false,
        }
    }

    fn run(
        self: Box<Self>,
        mut events: mpsc::Receiver<Event>,
        garden: Arc<Garden>,
        pulse: Arc<Pulse>,
        shutdown: CancellationToken,
    ) -> BoxFuture<'static, ()> {
        Box::pin(async move {
            let connection = match self.prebuilt {
                Some(conn) => conn,
                None => {
                    let conn = Arc::new(FireflyConnection::new(Some(self.port_name.clone())));
                    if let Err(e) = conn.try_connect() {
                        tracing::warn!(port = %self.port_name, error = %e, "matrix adapter could not open device");
                        return;
                    }
                    conn
                }
            };
            let _ = connection.with_device(|s| s.clear());

            let animation = Arc::new(RwLock::new(Animation::new(self.state_dir.clone())));

            // Rehydrate the animation context from Garden's read-model
            // before starting the engine, so the very first frame
            // reflects current health/load instead of defaulted state.
            if garden.is_ready() {
                let gs = garden.snapshot();
                let mut ctx = animation.write().await;
                ctx.stone_name = gs.stone_name.clone();
                ctx.health_label = gs.health.to_string();
                ctx.health = parse_health(&ctx.health_label);
                ctx.cpu_percent = gs.load.cpu.as_u8();
                ctx.memory_percent = gs.load.memory.as_u8();
                ctx.disk_percent = gs.load.disk.as_u8();
                ctx.io_percent = gs.load.io.as_u8();
                ctx.gpu_percent = gs.load.gpu.as_u8();
                ctx.gpu_active = gs.load.gpu_active;
                ctx.load = ((ctx.cpu_percent as f32 + ctx.memory_percent as f32) / 200.0)
                    .clamp(0.0, 1.0);
                ctx.offering_count = gs.offerings.len();
                ctx.has_services = !gs.offerings.is_empty();
                ctx.has_seed_bank = gs.seed_bank.is_some();
                trigger_health(&mut ctx);
            }

            let engine = start_animation(connection.clone(), animation.clone());

            loop {
                tokio::select! {
                    _ = shutdown.cancelled() => break,
                    maybe = events.recv() => match maybe {
                        Some(event) => {
                            // Device lifecycle is the bus's concern. We
                            // exit only on shutdown or closed channel;
                            // transient serial errors are logged inside
                            // the connection and the loop continues.
                            handle_event(&event, &connection, &animation, &pulse).await;
                        }
                        None => break,
                    },
                }
            }

            engine.abort();
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
    animation: &Arc<RwLock<Animation>>,
    pulse: &Arc<Pulse>,
) {
    if event.kind == "core.command.invocation" {
        if let Some(inv) = event.payload::<CommandInvocation>() {
            let response = handle_command(&inv.raw_args, connection, animation).await;
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
    connection: &Arc<FireflyConnection>,
    animation: &Arc<RwLock<Animation>>,
) -> CommandResponse {
    let Some((cmd, rest)) = args.split_first() else {
        return CommandResponse::error("No command provided");
    };
    let cmd = cmd.to_lowercase();
    match cmd.as_str() {
        "status" | "state" => cmd_status(rest, connection),
        "pixel" | "px" => cmd_pixel(rest, connection),
        "fill" => cmd_fill(rest, connection),
        "clear" => cmd_clear(connection),
        "brightness" | "bright" | "dim" => cmd_brightness(rest, animation).await,
        "animate" | "anim" | "animation" => cmd_animate(rest, connection),
        "stop" => cmd_stop(connection),
        "info" => cmd_info(connection),
        "on" => cmd_on(animation).await,
        "off" => cmd_off(connection, animation).await,
        _ => CommandResponse::error(format!("Unknown command: {}", cmd)),
    }
}

fn cmd_status(args: &[String], connection: &FireflyConnection) -> CommandResponse {
    let Some(state) = args.first() else {
        return CommandResponse::error("Usage: status <healthy|warning|error|offline>");
    };
    let state = state.to_lowercase();
    if !["healthy", "warning", "error", "offline"].contains(&state.as_str()) {
        return CommandResponse::error(format!("Invalid status: {}", state));
    }
    match connection.with_device(|s| s.status(&state)) {
        Ok(_) => CommandResponse::success(format!("Status set to {}", state)),
        Err(e) => CommandResponse::error(format!("Device error: {}", e)),
    }
}

fn cmd_pixel(args: &[String], connection: &FireflyConnection) -> CommandResponse {
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
        Err(e) => return CommandResponse::error(format!("Invalid color: {}", e)),
    };
    match connection.with_device(|s| s.pixel(x, y, r, g, b)) {
        Ok(_) => CommandResponse::success(format!("Pixel ({},{}) set to RGB({},{},{})", x, y, r, g, b)),
        Err(e) => CommandResponse::error(format!("Device error: {}", e)),
    }
}

fn cmd_fill(args: &[String], connection: &FireflyConnection) -> CommandResponse {
    let Some(color) = args.first() else {
        return CommandResponse::error("Usage: fill <color>");
    };
    let (r, g, b) = match parse_color(color) {
        Ok(c) => c,
        Err(e) => return CommandResponse::error(format!("Invalid color: {}", e)),
    };
    match connection.with_device(|s| s.fill(r, g, b)) {
        Ok(_) => CommandResponse::success(format!("Filled with RGB({},{},{})", r, g, b)),
        Err(e) => CommandResponse::error(format!("Device error: {}", e)),
    }
}

fn cmd_clear(connection: &FireflyConnection) -> CommandResponse {
    match connection.with_device(|s| s.clear()) {
        Ok(_) => CommandResponse::success("Display cleared"),
        Err(e) => CommandResponse::error(format!("Device error: {}", e)),
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
    CommandResponse::success(format!("Brightness set to {}% (saved)", v))
}

fn cmd_animate(args: &[String], connection: &FireflyConnection) -> CommandResponse {
    let Some(name) = args.first() else {
        return CommandResponse::error("Usage: animate <rainbow|pulse|chase|sparkle>");
    };
    let name = name.to_lowercase();
    if !["rainbow", "pulse", "chase", "sparkle"].contains(&name.as_str()) {
        return CommandResponse::error(format!("Unknown animation: {}", name));
    }
    match connection.with_device(|s| s.animate(&name)) {
        Ok(_) => CommandResponse::success(format!("Playing animation: {}", name)),
        Err(e) => CommandResponse::error(format!("Device error: {}", e)),
    }
}

fn cmd_stop(connection: &FireflyConnection) -> CommandResponse {
    match connection.with_device(|s| s.stop()) {
        Ok(_) => CommandResponse::success("Animation stopped"),
        Err(e) => CommandResponse::error(format!("Device error: {}", e)),
    }
}

fn cmd_info(connection: &FireflyConnection) -> CommandResponse {
    let status = connection.status_info();
    match connection.with_device(|s| s.info()) {
        Ok(response) => CommandResponse::success(format!("{}\n{}", status, response)),
        Err(e) => CommandResponse::error(format!("Communication error: {}", e)),
    }
}

async fn cmd_on(animation: &Arc<RwLock<Animation>>) -> CommandResponse {
    animation.write().await.enabled = true;
    CommandResponse::success("Firefly enabled — animation running")
}

async fn cmd_off(connection: &FireflyConnection, animation: &Arc<RwLock<Animation>>) -> CommandResponse {
    animation.write().await.enabled = false;
    let _ = connection.with_device(|s| s.clear());
    CommandResponse::success("Firefly disabled — display cleared")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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

    #[test]
    fn subscriptions_cover_command_and_presence() {
        assert!(MATRIX_SUBSCRIPTIONS.contains(&"core.command.invocation"));
        assert!(MATRIX_SUBSCRIPTIONS.contains(&"core.presence.snapshot"));
        assert!(MATRIX_SUBSCRIPTIONS.contains(&"core.stone.health.changed"));
    }
}
