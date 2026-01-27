//! Command handler for Cricket
//!
//! Implements the SDK's CommandHandler trait for Cricket-specific commands.

use garden_adapter_sdk::{async_trait, CommandHandler, CommandResponse};
use std::sync::Arc;

use crate::manifest::TuneManager;
use crate::mixer::{Channel, Mixer};

/// Cricket command handler
///
/// Implements the adapter SDK's CommandHandler trait.
pub struct CricketHandler {
    pub mixer: Arc<Mixer>,
    pub tune_manager: Arc<TuneManager>,
}

impl CricketHandler {
    /// Create a new Cricket command handler
    pub fn new(mixer: Arc<Mixer>, tune_manager: Arc<TuneManager>) -> Self {
        Self { mixer, tune_manager }
    }
}

#[async_trait]
impl CommandHandler for CricketHandler {
    async fn handle(&self, args: &[String]) -> CommandResponse {
        if args.is_empty() {
            return CommandResponse::error("No command provided")
                .with_suggestions([
                    "play <event>",
                    "stop [channel]",
                    "volume <0-100>",
                    "select <tune>",
                    "list",
                    "status",
                ]);
        }

        let cmd = args[0].to_lowercase();
        let cmd_args = &args[1..];

        match cmd.as_str() {
            // Advertised commands (from manifest)
            "select" => self.handle_select(cmd_args).await,
            "list" => self.handle_list().await,
            "show" => self.handle_show(cmd_args).await,
            "play" => self.handle_play(cmd_args).await,
            "stop" => self.handle_stop(cmd_args).await,
            "volume" | "vol" => self.handle_volume(cmd_args).await,
            // Internal/legacy commands
            "tune" => self.handle_select(cmd_args).await, // alias for select
            "status" => self.handle_status().await,
            "test" => self.handle_play(cmd_args).await, // alias for play
            _ => CommandResponse::error(format!("Unknown command: {}", cmd))
                .with_suggestions([
                    "select <tune>",
                    "list",
                    "show <tune>",
                    "play <event>",
                    "stop",
                    "volume <0-100>",
                ]),
        }
    }

    async fn on_shutdown(&self) {
        tracing::info!("Cricket shutting down, stopping all audio");
        // Stop all channels
        for channel in [
            Channel::Foreground,
            Channel::Midground,
            Channel::Ambient,
            Channel::Background,
        ] {
            self.mixer.stop(channel).await;
        }
    }
}

impl CricketHandler {
    /// Play an event's audio
    async fn handle_play(&self, args: &[String]) -> CommandResponse {
        if args.is_empty() {
            // Show available events from active tune
            let active_name = self
                .tune_manager
                .active_name()
                .unwrap_or_else(|| "(none)".to_string());
            let events = self.get_event_suggestions();

            if events.is_empty() {
                return CommandResponse::success_with_details(
                    format!("No events mapped in tune '{}'", active_name),
                    "No playable events. Try selecting a different tune with 'select <tune>'.",
                );
            }

            let mut output = format!("Playable events (tune: {}):\n\n", active_name);
            for event in &events {
                output.push_str(&format!("  {}\n", event));
            }

            return CommandResponse::success_with_details(
                format!("{} events available", events.len()),
                output,
            );
        }

        let event = &args[0];

        // Get mapping from active tune
        let Some(mapping) = self.tune_manager.get_event_mapping(event) else {
            return CommandResponse::not_found(format!("No mapping for event: {}", event))
                .with_suggestions(self.get_event_suggestions());
        };

        // Resolve channel and resource
        let Some(channel) = Channel::from_str(&mapping.channel) else {
            return CommandResponse::internal_error(format!("Invalid channel: {}", mapping.channel));
        };

        let active_name = self.tune_manager.active_name().unwrap_or_default();
        let Some(audio_data) = self
            .tune_manager
            .resolve_resource_bytes_with_fallback(&active_name, &mapping.resource)
        else {
            return CommandResponse::not_found(format!(
                "Audio file not found: {} (no fallback defined)",
                mapping.resource
            ));
        };

        // Play
        match self
            .mixer
            .play_bytes(channel, audio_data, mapping.looping)
            .await
        {
            Ok(()) => {
                CommandResponse::success(format!("Playing {} on {}", event, mapping.channel))
            }
            Err(e) => CommandResponse::internal_error(format!("Playback failed: {}", e)),
        }
    }

    /// Stop playback on channel(s)
    async fn handle_stop(&self, args: &[String]) -> CommandResponse {
        if args.is_empty() {
            // Stop all channels
            for channel in [
                Channel::Foreground,
                Channel::Midground,
                Channel::Ambient,
                Channel::Background,
            ] {
                self.mixer.stop(channel).await;
            }
            return CommandResponse::success("Stopped all channels");
        }

        let channel_name = &args[0];
        let Some(channel) = Channel::from_str(channel_name) else {
            return CommandResponse::error(format!("Invalid channel: {}", channel_name))
                .with_suggestions([
                    "foreground",
                    "midground",
                    "ambient",
                    "background",
                ]);
        };

        self.mixer.stop(channel).await;
        CommandResponse::success(format!("Stopped {}", channel_name))
    }

    /// Set volume
    async fn handle_volume(&self, args: &[String]) -> CommandResponse {
        if args.is_empty() {
            return CommandResponse::error("Usage: volume <0-100>");
        }

        let volume: u8 = match args[0].parse() {
            Ok(v) => v,
            Err(_) => return CommandResponse::error("Volume must be 0-100"),
        };

        if volume > 100 {
            return CommandResponse::error("Volume must be 0-100");
        }

        self.mixer.set_master_volume(volume as f32 / 100.0).await;
        CommandResponse::success(format!("Volume set to {}%", volume))
    }

    /// Select/switch tune
    async fn handle_select(&self, args: &[String]) -> CommandResponse {
        if args.is_empty() {
            return CommandResponse::error("Usage: select <tune>").with_suggestions(
                self.tune_manager
                    .list_tunes()
                    .into_iter()
                    .map(|t| format!("select {}", t.name))
                    .collect::<Vec<_>>(),
            );
        }

        let tune_name = &args[0];

        match self.tune_manager.select(tune_name) {
            Ok(()) => CommandResponse::success(format!("Switched to tune: {}", tune_name)),
            Err(_) => CommandResponse::not_found(format!("Tune not found: {}", tune_name))
                .with_suggestions(
                    self.tune_manager
                        .list_tunes()
                        .into_iter()
                        .map(|t| format!("select {}", t.name))
                        .collect::<Vec<_>>(),
                ),
        }
    }

    /// List available tunes
    async fn handle_list(&self) -> CommandResponse {
        let tunes = self.tune_manager.list_tunes();
        let active = self.tune_manager.active_name();

        if tunes.is_empty() {
            return CommandResponse::success_with_details(
                "No tunes available",
                "No tunes found. Add tunes to the tunes directory.",
            );
        }

        let mut output = String::new();
        for tune in &tunes {
            let marker = if Some(&tune.name) == active.as_ref() {
                "→"
            } else {
                " "
            };
            let source = if tune.embedded {
                "[embedded]"
            } else {
                "[filesystem]"
            };
            output.push_str(&format!(
                "{} {} (v{}) {}\n",
                marker, tune.name, tune.version, source
            ));
        }

        CommandResponse::success_with_details(format!("{} tune(s) available", tunes.len()), output)
    }

    /// Show tune details
    async fn handle_show(&self, args: &[String]) -> CommandResponse {
        if args.is_empty() {
            return CommandResponse::error("Usage: show <tune>").with_suggestions(
                self.tune_manager
                    .list_tunes()
                    .into_iter()
                    .map(|t| format!("show {}", t.name))
                    .collect::<Vec<_>>(),
            );
        }

        let tune_name = &args[0];

        let Some(tune) = self.tune_manager.get_tune(tune_name) else {
            return CommandResponse::not_found(format!("Tune not found: {}", tune_name))
                .with_suggestions(
                    self.tune_manager
                        .list_tunes()
                        .into_iter()
                        .map(|t| format!("show {}", t.name))
                        .collect::<Vec<_>>(),
                );
        };

        let mut output = format!("{}\n", tune.name);
        output.push_str(&format!("  Version:     {}\n", tune.version));
        output.push_str(&format!("  Description: {}\n", tune.description));
        output.push_str(&format!("  Author:      {}\n", tune.author));
        output.push_str(&format!("  License:     {}\n", tune.license));
        output.push_str(&format!("\nEvent Mappings ({}):\n", tune.events.len()));

        for (event, mapping) in &tune.events {
            output.push_str(&format!(
                "  {} → {} ({})\n",
                event, mapping.resource, mapping.channel
            ));
        }

        CommandResponse::success_with_details(format!("Tune: {}", tune_name), output)
    }

    /// Show status
    async fn handle_status(&self) -> CommandResponse {
        let active = self
            .tune_manager
            .active_name()
            .unwrap_or_else(|| "(none)".to_string());
        let tune = self.tune_manager.active();

        let mut output = format!("Active tune: {}\n", active);

        if let Some(t) = tune {
            output.push_str(&format!("Version: {}\n", t.version));
            output.push_str(&format!("Events mapped: {}\n", t.events.len()));
        }

        CommandResponse::success_with_details("Cricket status", output)
    }

    /// Get event suggestions from active tune
    fn get_event_suggestions(&self) -> Vec<String> {
        self.tune_manager
            .active()
            .map(|t| t.events.keys().cloned().collect())
            .unwrap_or_default()
    }
}
