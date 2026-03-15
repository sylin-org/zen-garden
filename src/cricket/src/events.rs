//! SSE event handler for Cricket
//!
//! Handles presence events from Moss and triggers audio playback.

use garden_companion_sdk::{async_trait, CompanionState, EventHandler, SseEvent};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use crate::manifest::Tunes;
use crate::mixer::{Channel, Mixer};

/// Debounce tracker for events
struct DebounceState {
    last_fired: HashMap<String, Instant>,
}

impl DebounceState {
    fn new() -> Self {
        Self {
            last_fired: HashMap::new(),
        }
    }

    /// Check if event can fire (respects debounce)
    fn can_fire(&mut self, event_type: &str, debounce_ms: u64) -> bool {
        if debounce_ms == 0 {
            return true;
        }

        let now = Instant::now();
        let debounce = Duration::from_millis(debounce_ms);

        if let Some(last) = self.last_fired.get(event_type) {
            if now.duration_since(*last) < debounce {
                return false;
            }
        }

        self.last_fired.insert(event_type.to_string(), now);
        true
    }
}

/// Cricket's SSE event handler
///
/// Receives presence events from Moss and plays corresponding audio.
pub struct CricketEvents {
    mixer: Arc<Mixer>,
    tunes: Arc<Tunes>,
    state: Arc<CompanionState>,
    debounce: Arc<RwLock<DebounceState>>,
}

impl CricketEvents {
    /// Create a new event handler
    pub fn new(
        mixer: Arc<Mixer>,
        tunes: Arc<Tunes>,
        state: Arc<CompanionState>,
    ) -> Self {
        Self {
            mixer,
            tunes,
            state,
            debounce: Arc::new(RwLock::new(DebounceState::new())),
        }
    }
}

#[async_trait]
impl EventHandler for CricketEvents {
    async fn on_event(&self, event: SseEvent) {
        // Skip processing if disabled (user ran "off" command)
        if !self.state.is_enabled() {
            tracing::trace!(
                event_type = %event.event_type,
                "Ignoring event - Cricket disabled"
            );
            return;
        }

        // Get mapping from active tune
        let Some(mapping) = self.tunes.get_event_mapping(&event.event_type) else {
            tracing::trace!(event = %event.event_type, "No mapping for event");
            return;
        };

        // Check debounce
        {
            let mut db = self.debounce.write().await;
            if !db.can_fire(&event.event_type, mapping.debounce_ms) {
                tracing::trace!(event = %event.event_type, "Debounced");
                return;
            }
        }

        // Resolve channel
        let Some(channel) = Channel::from_str(&mapping.channel) else {
            tracing::warn!(channel = %mapping.channel, "Invalid channel in mapping");
            return;
        };

        // Resolve resource (works for both embedded and filesystem, with fallback)
        let active_name = self.tunes.active_name().unwrap_or_default();
        let Some(audio_data) = self
            .tunes
            .resolve_resource_bytes_with_fallback(&active_name, &mapping.resource)
        else {
            tracing::warn!(
                tune = %active_name,
                resource = %mapping.resource,
                "Audio resource not found (no fallback defined)"
            );
            return;
        };

        // Play audio
        tracing::debug!(
            event = %event.event_type,
            resource = %mapping.resource,
            channel = %mapping.channel,
            "Playing audio"
        );

        if let Err(e) = self
            .mixer
            .play_bytes(channel, audio_data, mapping.looping)
            .await
        {
            tracing::error!(error = %e, "Failed to play audio");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_debounce() {
        let mut state = DebounceState::new();

        // First fire should succeed
        assert!(state.can_fire("test", 1000));

        // Immediate second fire should fail
        assert!(!state.can_fire("test", 1000));

        // Different event should succeed
        assert!(state.can_fire("other", 1000));

        // Zero debounce always succeeds
        assert!(state.can_fire("test", 0));
    }
}
