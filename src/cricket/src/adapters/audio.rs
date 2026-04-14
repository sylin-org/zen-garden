//! Cricket's audio adapter.
//!
//! A singleton [`Adapter`] that owns the mixer + tune manifest + on/off
//! state. Receives filtered events from the Pulse and either:
//!
//! - plays a mapped audio sample (for presence events), or
//! - executes a hey-tell command and publishes the correlated
//!   [`CommandResult`] back onto the Pulse.
//!
//! Subscriptions are a fixed, broad set covering every core kind that
//! a tune can currently map, plus [`CommandInvocation`]. Unmapped kinds
//! are dropped at the adapter; coalescing / debouncing are handled per
//! event via the tune's `debounce_ms`.

use crate::manifest::{EventMapping, Tunes};
use crate::mixer::{Channel, Mixer};
use garden_companion_sdk::adapters::{
    Adapter, AdapterFactory, AdapterInfo, AdapterProfile, DeliveryPolicy, adapter::BoxFuture,
};
use garden_companion_sdk::moss_client::MossLocalClient;
use garden_companion_sdk::garden::{
    CommandInvocation, CommandOutcome, CommandResult, Event, Pulse,
};
use garden_common::command_manifest::CommandResponse;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, mpsc};
use tokio_util::sync::CancellationToken;

/// The subscription set — every `core.*` kind a tune YAML may reference
/// (keys are matched without the `core.` prefix, see [`tune_key`]), plus
/// the command channel.
const AUDIO_SUBSCRIPTIONS: &[&str] = &[
    "core.command.invocation",
    "core.presence.snapshot",
    "core.stone.health.changed",
    "core.stone.load.updated",
    "core.stone.tended",
    "core.service.started",
    "core.service.stopped",
    "core.storage.connected",
    "core.storage.detected",
    "core.storage.removed",
];

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

/// Factory producing a single [`AudioAdapter`] instance per run.
pub struct AudioFactory {
    mixer: Arc<Mixer>,
    tunes: Arc<Tunes>,
    enabled: Arc<Mutex<bool>>,
    produced: std::sync::atomic::AtomicBool,
}

impl AudioFactory {
    pub fn new(mixer: Arc<Mixer>, tunes: Arc<Tunes>, enabled: Arc<Mutex<bool>>) -> Self {
        Self {
            mixer,
            tunes,
            enabled,
            produced: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

impl AdapterFactory for AudioFactory {
    fn kind(&self) -> &'static str {
        "cricket.audio"
    }

    fn discover(&self) -> Vec<Box<dyn Adapter>> {
        // Singleton: produce on first tick, never again. The supervisor
        // dedupes by (kind, id) anyway, but this is the honest signal.
        use std::sync::atomic::Ordering;
        if self.produced.swap(true, Ordering::SeqCst) {
            return Vec::new();
        }
        vec![Box::new(AudioAdapter {
            id: "default".to_string(),
            mixer: self.mixer.clone(),
            tunes: self.tunes.clone(),
            enabled: self.enabled.clone(),
        })]
    }
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// The audio adapter itself.
pub struct AudioAdapter {
    id: String,
    mixer: Arc<Mixer>,
    tunes: Arc<Tunes>,
    /// Shared on/off flag. Commands flip it; presence events check it.
    enabled: Arc<Mutex<bool>>,
}

impl Adapter for AudioAdapter {
    fn info(&self) -> AdapterInfo {
        AdapterInfo {
            kind: "cricket.audio",
            id: self.id.clone(),
            device: None,
        }
    }

    fn profile(&self) -> AdapterProfile {
        AdapterProfile {
            subscriptions: AUDIO_SUBSCRIPTIONS,
            delivery: DeliveryPolicy::All,
            persisted_state: false,
        }
    }

    fn run(
        self: Box<Self>,
        mut events: mpsc::Receiver<Event>,
        _moss: Arc<MossLocalClient>,
        pulse: Arc<Pulse>,
        shutdown: CancellationToken,
    ) -> BoxFuture<'static, ()> {
        Box::pin(async move {
            let mut debounce: HashMap<String, Instant> = HashMap::new();

            loop {
                tokio::select! {
                    maybe = events.recv() => match maybe {
                        Some(event) => self.handle_event(event, &mut debounce, &pulse).await,
                        None => break,
                    },
                    _ = shutdown.cancelled() => break,
                }
            }

            // Best-effort: silence every channel so we don't leak a
            // looping ambient sample across restart.
            for ch in [
                Channel::Foreground,
                Channel::Midground,
                Channel::Ambient,
                Channel::Background,
            ] {
                self.mixer.stop(ch).await;
            }
        })
    }
}

impl AudioAdapter {
    async fn handle_event(
        &self,
        event: Event,
        debounce: &mut HashMap<String, Instant>,
        pulse: &Arc<Pulse>,
    ) {
        if event.kind == "core.command.invocation" {
            if let Some(inv) = event.payload::<CommandInvocation>() {
                let response = self.handle_command(&inv.raw_args).await;
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
                    from: self.id.clone(),
                }));
            }
            return;
        }

        let enabled = *self.enabled.lock().await;
        if !enabled {
            tracing::trace!(kind = event.kind, "ignoring event — cricket disabled");
            return;
        }

        let Some(mapping) = self.tunes.get_event_mapping(&tune_key(event.kind)) else {
            tracing::trace!(kind = event.kind, "no tune mapping");
            return;
        };

        if mapping.debounce_ms > 0 && !can_fire(debounce, event.kind, mapping.debounce_ms) {
            tracing::trace!(kind = event.kind, "debounced");
            return;
        }

        self.play_mapping(&mapping).await;
    }

    async fn play_mapping(&self, mapping: &EventMapping) {
        let Some(channel) = Channel::from_str(&mapping.channel) else {
            tracing::warn!(channel = %mapping.channel, "invalid channel in tune mapping");
            return;
        };
        let Some(tune_name) = self.tunes.active_name() else {
            tracing::trace!("no active tune");
            return;
        };
        let Some(data) = self
            .tunes
            .resolve_resource_bytes_with_fallback(&tune_name, &mapping.resource)
        else {
            tracing::warn!(
                tune = %tune_name,
                resource = %mapping.resource,
                "audio resource not found (no fallback)"
            );
            return;
        };
        if let Err(e) = self.mixer.play_bytes(channel, data, mapping.looping).await {
            tracing::error!(error = %e, "audio playback failed");
        }
    }

    async fn handle_command(&self, args: &[String]) -> CommandResponse {
        if args.is_empty() {
            return CommandResponse::error("No command provided");
        }
        let cmd = args[0].to_lowercase();
        let rest = &args[1..];
        match cmd.as_str() {
            "select" | "tune" => self.cmd_select(rest).await,
            "list" => self.cmd_list(),
            "show" => self.cmd_show(rest),
            "play" | "test" => self.cmd_play(rest).await,
            "stop" => self.cmd_stop(rest).await,
            "volume" | "vol" => self.cmd_volume(rest).await,
            "on" => self.cmd_on().await,
            "off" => self.cmd_off().await,
            "status" => self.cmd_status().await,
            _ => CommandResponse::error(format!("Unknown command: {}", cmd)),
        }
    }

    async fn cmd_select(&self, args: &[String]) -> CommandResponse {
        let Some(name) = args.first() else {
            return CommandResponse::error("Usage: select <tune>");
        };
        match self.tunes.select(name) {
            Ok(()) => CommandResponse::success(format!("Switched to tune: {}", name)),
            Err(e) => CommandResponse::error(format!("Tune not found: {}", e)),
        }
    }

    fn cmd_list(&self) -> CommandResponse {
        let tunes = self.tunes.list_tunes();
        if tunes.is_empty() {
            return CommandResponse::success("No tunes available");
        }
        let active = self.tunes.active_name();
        let mut out = String::new();
        for t in &tunes {
            let marker = if Some(&t.name) == active.as_ref() { "→" } else { " " };
            out.push_str(&format!(
                "{} {} (v{}) {}\n",
                marker,
                t.name,
                t.version,
                if t.embedded { "[embedded]" } else { "[filesystem]" }
            ));
        }
        CommandResponse::success(out)
    }

    fn cmd_show(&self, args: &[String]) -> CommandResponse {
        let Some(name) = args.first() else {
            return CommandResponse::error("Usage: show <tune>");
        };
        let Some(tune) = self.tunes.get_tune(name) else {
            return CommandResponse::error(format!("Tune not found: {}", name));
        };
        let mut out = format!("{} (v{})\n", tune.name, tune.version);
        out.push_str(&format!("  {}\n", tune.description));
        out.push_str(&format!("  Author: {}\n", tune.author));
        out.push_str(&format!("  License: {}\n\n", tune.license));
        out.push_str("Event Mappings:\n");
        for (ev, m) in &tune.events {
            out.push_str(&format!("  {} → {} ({})\n", ev, m.resource, m.channel));
        }
        CommandResponse::success(out)
    }

    async fn cmd_play(&self, args: &[String]) -> CommandResponse {
        let Some(event) = args.first() else {
            return CommandResponse::error("Usage: play <event>");
        };
        let Some(mapping) = self.tunes.get_event_mapping(event) else {
            return CommandResponse::error(format!("No mapping for event: {}", event));
        };
        self.play_mapping(&mapping).await;
        CommandResponse::success(format!("Playing {} on {}", event, mapping.channel))
    }

    async fn cmd_stop(&self, args: &[String]) -> CommandResponse {
        match args.first() {
            None => {
                for ch in [
                    Channel::Foreground,
                    Channel::Midground,
                    Channel::Ambient,
                    Channel::Background,
                ] {
                    self.mixer.stop(ch).await;
                }
                CommandResponse::success("Stopped all channels")
            }
            Some(name) => match Channel::from_str(name) {
                Some(ch) => {
                    self.mixer.stop(ch).await;
                    CommandResponse::success(format!("Stopped {}", name))
                }
                None => CommandResponse::error(format!("Invalid channel: {}", name)),
            },
        }
    }

    async fn cmd_volume(&self, args: &[String]) -> CommandResponse {
        let Some(raw) = args.first() else {
            return CommandResponse::error("Usage: volume <0-100>");
        };
        let Ok(v) = raw.parse::<u8>() else {
            return CommandResponse::error("Volume must be 0-100");
        };
        if v > 100 {
            return CommandResponse::error("Volume must be 0-100");
        }
        self.mixer.set_master_volume(v as f32 / 100.0).await;
        CommandResponse::success(format!("Volume set to {}%", v))
    }

    async fn cmd_on(&self) -> CommandResponse {
        *self.enabled.lock().await = true;
        CommandResponse::success("Cricket enabled — responding to events")
    }

    async fn cmd_off(&self) -> CommandResponse {
        *self.enabled.lock().await = false;
        for ch in [
            Channel::Foreground,
            Channel::Midground,
            Channel::Ambient,
            Channel::Background,
        ] {
            self.mixer.stop(ch).await;
        }
        CommandResponse::success("Cricket disabled — ignoring events")
    }

    async fn cmd_status(&self) -> CommandResponse {
        let active = self.tunes.active_name().unwrap_or_else(|| "(none)".into());
        let enabled = if *self.enabled.lock().await { "on" } else { "off" };
        let mapped = self
            .tunes
            .active()
            .map(|t| t.events.len())
            .unwrap_or(0);
        CommandResponse::success(format!(
            "tune: {}\nstate: {}\nevents mapped: {}",
            active, enabled, mapped
        ))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Strip the `core.` prefix from a wire kind so it matches the
/// historically two-level tune YAML keys (`stone.health.changed`, ...).
fn tune_key(kind: &str) -> String {
    kind.strip_prefix("core.").unwrap_or(kind).to_string()
}

fn can_fire(tracker: &mut HashMap<String, Instant>, key: &str, debounce_ms: u64) -> bool {
    let now = Instant::now();
    let window = Duration::from_millis(debounce_ms);
    if let Some(last) = tracker.get(key)
        && now.duration_since(*last) < window
    {
        return false;
    }
    tracker.insert(key.to_string(), now);
    true
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tune_key_strips_core_prefix() {
        assert_eq!(tune_key("core.stone.health.changed"), "stone.health.changed");
        assert_eq!(tune_key("core.service.started"), "service.started");
        assert_eq!(tune_key("stone.tended"), "stone.tended"); // idempotent
    }

    #[test]
    fn debounce_blocks_rapid_fires() {
        let mut t: HashMap<String, Instant> = HashMap::new();
        assert!(can_fire(&mut t, "x", 1000));
        assert!(!can_fire(&mut t, "x", 1000));
        assert!(can_fire(&mut t, "y", 1000));
        assert!(can_fire(&mut t, "x", 0)); // zero debounce always fires
    }

    #[test]
    fn subscriptions_include_command_channel() {
        assert!(AUDIO_SUBSCRIPTIONS.contains(&"core.command.invocation"));
    }
}
