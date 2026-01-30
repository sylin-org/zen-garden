//! SSE event handler for Firefly
//!
//! Subscribes to Moss presence stream and updates animation context.
//! Events override the baseline animation temporarily.

use std::sync::Arc;

use garden_adapter_sdk::{async_trait, AdapterState, EventHandler, SseEvent};
use garden_common::presence::event_types;
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::animation::{AnimationContext, Health, Override};

/// Presence snapshot from Moss
#[derive(Debug, Deserialize)]
struct PresenceSnapshot {
    stone: StoneState,
    #[serde(default)]
    services: Vec<ServiceState>,
}

#[derive(Debug, Deserialize)]
struct StoneState {
    #[serde(default)]
    health: String,
    #[serde(default)]
    cpu_percent: f64,
    #[serde(default)]
    memory_percent: f64,
}

#[derive(Debug, Deserialize)]
struct ServiceState {
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    state: String,
}

/// Service event payload
#[derive(Debug, Deserialize)]
struct ServiceEvent {
    service: String,
}

/// Stone tended event payload
#[derive(Debug, Deserialize)]
struct TendedEvent {
    #[allow(dead_code)]
    by: Option<String>,
}

/// Firefly event handler - updates animation context based on SSE events
pub struct FireflyEventHandler {
    context: Arc<RwLock<AnimationContext>>,
    state: Arc<AdapterState>,
}

impl FireflyEventHandler {
    pub fn new(context: Arc<RwLock<AnimationContext>>, state: Arc<AdapterState>) -> Self {
        Self { context, state }
    }

    /// Map health string to Health enum
    fn parse_health(health: &str) -> Health {
        match health {
            "thriving" => Health::Thriving,
            "withering" => Health::Withering,
            "wilting" => Health::Wilting,
            _ => Health::Thriving,
        }
    }
}

#[async_trait]
impl EventHandler for FireflyEventHandler {
    async fn on_event(&self, event: SseEvent) {
        // Update enabled state in animation context
        {
            let mut ctx = self.context.write().await;
            ctx.enabled = self.state.is_enabled();
        }

        // Skip processing if disabled
        if !self.state.is_enabled() {
            tracing::trace!(
                event_type = %event.event_type,
                "Ignoring event - Firefly disabled"
            );
            return;
        }

        tracing::debug!(
            event_type = %event.event_type,
            data_len = event.data.len(),
            "Received presence event"
        );

        match event.event_type.as_str() {
            // Initial snapshot - update context with current state
            event_types::PRESENCE_SNAPSHOT => {
                if let Ok(snapshot) = serde_json::from_str::<PresenceSnapshot>(&event.data) {
                    tracing::info!(
                        health = %snapshot.stone.health,
                        offerings = snapshot.services.len(),
                        cpu = %snapshot.stone.cpu_percent,
                        memory = %snapshot.stone.memory_percent,
                        "Received presence snapshot"
                    );

                    let mut ctx = self.context.write().await;

                    // Update health
                    ctx.health = Self::parse_health(&snapshot.stone.health);

                    // Update load (average of CPU and memory)
                    ctx.load = ((snapshot.stone.cpu_percent + snapshot.stone.memory_percent) / 200.0) as f32;
                    ctx.load = ctx.load.clamp(0.0, 1.0);

                    // Update offering count (affects activity level)
                    ctx.offering_count = snapshot.services.len();

                    // Update service presence (for blue fireflies)
                    ctx.has_services = !snapshot.services.is_empty();

                    // Check for seed-bank (storage service)
                    ctx.has_seed_bank = snapshot.services.iter().any(|s| {
                        s.name.contains("seed-bank") || s.name.contains("storage")
                    });

                    // Trigger health override if not thriving
                    match ctx.health {
                        Health::Withering => ctx.trigger_override(Override::HealthWarning),
                        Health::Wilting => ctx.trigger_override(Override::HealthError),
                        Health::Thriving => ctx.clear_override(),
                    }
                }
            }

            // Service started - green bloom override
            event_types::SERVICE_STARTED => {
                if let Ok(evt) = serde_json::from_str::<ServiceEvent>(&event.data) {
                    tracing::info!(service = %evt.service, "Service started");

                    let mut ctx = self.context.write().await;
                    ctx.has_services = true;
                    ctx.trigger_override(Override::ServiceStarted);
                }
            }

            // Service stopped - brief dim override
            event_types::SERVICE_STOPPED => {
                if let Ok(evt) = serde_json::from_str::<ServiceEvent>(&event.data) {
                    tracing::info!(service = %evt.service, "Service stopped");

                    let mut ctx = self.context.write().await;
                    ctx.trigger_override(Override::ServiceStopped);
                }
            }

            // Stone health changed - update health and maybe trigger override
            event_types::STONE_HEALTH_CHANGED => {
                #[derive(Deserialize)]
                struct HealthEvent {
                    health: String,
                }
                if let Ok(evt) = serde_json::from_str::<HealthEvent>(&event.data) {
                    tracing::info!(health = %evt.health, "Stone health changed");

                    let mut ctx = self.context.write().await;
                    ctx.health = Self::parse_health(&evt.health);

                    // Trigger/clear override based on health
                    match ctx.health {
                        Health::Withering => ctx.trigger_override(Override::HealthWarning),
                        Health::Wilting => ctx.trigger_override(Override::HealthError),
                        Health::Thriving => ctx.clear_override(),
                    }
                }
            }

            // Stone load updated - update tempo
            event_types::STONE_LOAD_UPDATED => {
                #[derive(Deserialize)]
                struct LoadEvent {
                    #[serde(default)]
                    cpu: f64,
                    #[serde(default)]
                    memory: f64,
                }
                if let Ok(evt) = serde_json::from_str::<LoadEvent>(&event.data) {
                    let mut ctx = self.context.write().await;
                    ctx.load = ((evt.cpu + evt.memory) / 200.0) as f32;
                    ctx.load = ctx.load.clamp(0.0, 1.0);
                }
            }

            // Stone tended - sparkle override
            event_types::STONE_TENDED => {
                if let Ok(_evt) = serde_json::from_str::<TendedEvent>(&event.data) {
                    tracing::info!("Stone tended - showing appreciation");

                    let mut ctx = self.context.write().await;
                    ctx.trigger_override(Override::Tended);
                }
            }

            // Ignore other events
            _ => {
                tracing::trace!(event_type = %event.event_type, "Ignoring unhandled event");
            }
        }
    }
}
