//! SSE event handler for Firefly
//!
//! Subscribes to Moss presence stream and updates animation context.
//! For RP2040 Matrix: Events override the baseline animation temporarily.
//! For ESP8266 OLED: Events send direct display commands.
//! For ESP32 T-Display: Events send compact serial commands (FIREFLY-0003).

use std::sync::Arc;

use garden_common::presence::{event_types, StoneHealthChangedPayload, StoneLoadUpdatedPayload};
use garden_companion_sdk::{async_trait, CompanionState, EventHandler, SseEvent};
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::animation::{Animation, Health, Override};
use crate::oled;
use crate::serial::{FireflyConnection, FireflyDeviceType};
use crate::tdisplay;

/// Presence snapshot from Moss
#[derive(Debug, Deserialize)]
pub(crate) struct PresenceSnapshot {
    pub(crate) stone: StoneState,
    #[serde(default)]
    pub(crate) offerings: Vec<OfferingState>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct StoneState {
    #[serde(default)]
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) health: String,
    #[serde(default)]
    pub(crate) cpu_percent: f64,
    #[serde(default)]
    pub(crate) memory_percent: f64,
    #[serde(default)]
    pub(crate) disk_percent: f64,
    #[serde(default)]
    pub(crate) uptime_seconds: u64,
    // FIREFLY-0003: Extended fields for T-Display diorama
    #[serde(default)]
    pub(crate) io_percent: f64,
    #[serde(default)]
    pub(crate) gpu_percent: f64,
    #[serde(default)]
    pub(crate) gpu_active: bool,
    #[serde(default)]
    pub(crate) has_gpu: bool,
    #[serde(default)]
    pub(crate) is_lantern: bool,
    #[serde(default)]
    pub(crate) has_cricket: bool,
    #[serde(default)]
    pub(crate) pond_active: bool,
    #[serde(default)]
    pub(crate) hour: f64,
    #[serde(default)]
    pub(crate) net_rx_bytes_per_sec: u64,
    #[serde(default)]
    pub(crate) net_tx_bytes_per_sec: u64,
    #[serde(default)]
    pub(crate) seed_bank: Option<StorageSummary>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct StorageSummary {
    pub(crate) name: String,
    pub(crate) used_gb: u64,
    pub(crate) total_gb: u64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OfferingState {
    pub(crate) name: String,
    #[allow(dead_code)]
    pub(crate) status: String,
    #[serde(default)]
    pub(crate) health: String,
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

/// Firefly event handler - updates animation context and sends OLED commands
pub struct FireflyEvents {
    context: Arc<RwLock<Animation>>,
    connection: Arc<FireflyConnection>,
    state: Arc<CompanionState>,
}

impl FireflyEvents {
    pub fn new(
        context: Arc<RwLock<Animation>>,
        connection: Arc<FireflyConnection>,
        state: Arc<CompanionState>,
    ) -> Self {
        Self {
            context,
            connection,
            state,
        }
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

    /// Send OLED-specific commands for a snapshot
    fn send_oled_snapshot(&self, snapshot: &PresenceSnapshot) {
        let _ = oled::send_oled_snapshot(
            self.connection.as_ref(),
            &snapshot.stone.name,
            &snapshot.stone.health,
            snapshot.stone.cpu_percent as u8,
            snapshot.stone.memory_percent as u8,
            snapshot.stone.uptime_seconds,
        );
    }

    /// Send OLED command for health change
    fn send_oled_health(&self, health: &str) {
        let _ = self
            .connection
            .with_device(|serial| serial.oled_health(health));
    }

    /// Send OLED command for metrics update
    fn send_oled_metrics(&self, cpu: f64, memory: f64, uptime_secs: u64) {
        let uptime = oled::format_uptime(uptime_secs);
        let _ = self
            .connection
            .with_device(|serial| serial.oled_metrics(cpu as u8, memory as u8, &uptime));
    }

    /// Send OLED wipe-in animation
    fn send_oled_wipe_in(&self, line1: &str, line2: &str) {
        let _ = self
            .connection
            .with_device(|serial| serial.oled_wipe_in(line1, line2));
    }

    /// Send OLED wipe-out animation
    fn send_oled_wipe_out(&self, line1: &str, line2: &str) {
        let _ = self
            .connection
            .with_device(|serial| serial.oled_wipe_out(line1, line2));
    }

    /// Send OLED blink animation
    #[allow(dead_code)] // API method for future use
    fn send_oled_blink(&self, count: u8) {
        let _ = self
            .connection
            .with_device(|serial| serial.oled_blink(count));
    }

    /// Send OLED pulse animation
    #[allow(dead_code)] // API method for future use
    fn send_oled_pulse(&self, count: u8) {
        let _ = self
            .connection
            .with_device(|serial| serial.oled_pulse(count));
    }
}

#[async_trait]
impl EventHandler for FireflyEvents {
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

        let device_type = self.connection.device_type();

        tracing::debug!(
            event_type = %event.event_type,
            device_type = %device_type,
            data_len = event.data.len(),
            "Received presence event"
        );

        match event.event_type.as_str() {
            // Initial snapshot - update context with current state
            event_types::PRESENCE_SNAPSHOT => {
                if let Ok(snapshot) = serde_json::from_str::<PresenceSnapshot>(&event.data) {
                    tracing::info!(
                        stone = %snapshot.stone.name,
                        health = %snapshot.stone.health,
                        offerings = snapshot.offerings.len(),
                        cpu = %snapshot.stone.cpu_percent,
                        memory = %snapshot.stone.memory_percent,
                        "Received presence snapshot"
                    );

                    // For OLED: Send display commands directly
                    if device_type == FireflyDeviceType::Esp8266Oled {
                        self.send_oled_snapshot(&snapshot);
                    }

                    // For T-Display: Send full JSON push (FIREFLY-0003)
                    if device_type == FireflyDeviceType::Esp32TDisplay {
                        let _ = tdisplay::send_snapshot(&self.connection, &snapshot);
                    }

                    // For Matrix: Update animation context
                    let mut ctx = self.context.write().await;

                    // Store stone info for OLED updates
                    ctx.stone_name = Some(snapshot.stone.name.clone());
                    ctx.uptime_seconds = snapshot.stone.uptime_seconds;
                    ctx.health_label = snapshot.stone.health.clone();

                    // Update health
                    ctx.health = Self::parse_health(&snapshot.stone.health);

                    // Update load (average of CPU and memory)
                    ctx.load = ((snapshot.stone.cpu_percent + snapshot.stone.memory_percent)
                        / 200.0) as f32;
                    ctx.load = ctx.load.clamp(0.0, 1.0);

                    // Store CPU/memory for OLED
                    ctx.cpu_percent = snapshot.stone.cpu_percent as u8;
                    ctx.memory_percent = snapshot.stone.memory_percent as u8;

                    // Update offering count (affects activity level)
                    ctx.offering_count = snapshot.offerings.len();

                    // Update service presence (for blue fireflies)
                    ctx.has_services = !snapshot.offerings.is_empty();

                    // Check for seed-bank (storage service)
                    ctx.has_seed_bank = snapshot
                        .offerings
                        .iter()
                        .any(|s| s.name.contains("seed-bank") || s.name.contains("storage"));

                    // Trigger health override if not thriving (Matrix only)
                    if device_type == FireflyDeviceType::Rp2040Matrix {
                        match ctx.health {
                            Health::Withering => ctx.trigger_override(Override::HealthWarning),
                            Health::Wilting => ctx.trigger_override(Override::HealthError),
                            Health::Thriving => ctx.clear_override(),
                        }
                    }
                }
            }

            // Service started - green bloom override / wipe-in
            event_types::SERVICE_STARTED => {
                if let Ok(evt) = serde_json::from_str::<ServiceEvent>(&event.data) {
                    tracing::info!(service = %evt.service, "Service started");

                    match device_type {
                        FireflyDeviceType::Esp8266Oled => {
                            self.send_oled_wipe_in(&evt.service.to_uppercase(), "STARTED");
                        }
                        FireflyDeviceType::Esp32TDisplay => {
                            let _ = self.connection.with_device(|s| {
                                s.tdisplay_service_started(&evt.service, "healthy")
                            });
                        }
                        FireflyDeviceType::Rp2040Matrix | FireflyDeviceType::Unknown => {
                            let mut ctx = self.context.write().await;
                            ctx.has_services = true;
                            ctx.trigger_override(Override::ServiceStarted);
                        }
                    }
                }
            }

            // Service stopped - brief dim override / wipe-out
            event_types::SERVICE_STOPPED => {
                if let Ok(evt) = serde_json::from_str::<ServiceEvent>(&event.data) {
                    tracing::info!(service = %evt.service, "Service stopped");

                    match device_type {
                        FireflyDeviceType::Esp8266Oled => {
                            self.send_oled_wipe_out(&evt.service.to_uppercase(), "STOPPED");
                        }
                        FireflyDeviceType::Esp32TDisplay => {
                            let _ = self.connection.with_device(|s| {
                                s.tdisplay_service_stopped(&evt.service)
                            });
                        }
                        FireflyDeviceType::Rp2040Matrix | FireflyDeviceType::Unknown => {
                            let mut ctx = self.context.write().await;
                            ctx.trigger_override(Override::ServiceStopped);
                        }
                    }
                }
            }

            // Stone health changed - update health and maybe trigger override
            event_types::STONE_HEALTH_CHANGED => {
                if let Ok(evt) = serde_json::from_str::<StoneHealthChangedPayload>(&event.data) {
                    tracing::info!(health = %evt.health, "Stone health changed");

                    // For OLED: Send health command
                    if device_type == FireflyDeviceType::Esp8266Oled {
                        self.send_oled_health(&evt.health);
                    }

                    // For T-Display: Send health command (FIREFLY-0003)
                    if device_type == FireflyDeviceType::Esp32TDisplay {
                        let _ = self.connection.with_device(|s| {
                            s.tdisplay_health(&evt.health)
                        });
                    }

                    let mut ctx = self.context.write().await;
                    ctx.health = Self::parse_health(&evt.health);
                    ctx.health_label = evt.health.clone();

                    // Trigger/clear override based on health (Matrix only)
                    if device_type == FireflyDeviceType::Rp2040Matrix {
                        match ctx.health {
                            Health::Withering => ctx.trigger_override(Override::HealthWarning),
                            Health::Wilting => ctx.trigger_override(Override::HealthError),
                            Health::Thriving => ctx.clear_override(),
                        }
                    }
                }
            }

            // Stone load updated - update tempo / metrics
            event_types::STONE_LOAD_UPDATED => {
                if let Ok(evt) = serde_json::from_str::<StoneLoadUpdatedPayload>(&event.data) {
                    // For OLED: Send metrics update
                    if device_type == FireflyDeviceType::Esp8266Oled {
                        let ctx = self.context.read().await;
                        self.send_oled_metrics(
                            evt.cpu_percent,
                            evt.memory_percent,
                            ctx.uptime_seconds,
                        );
                    }

                    // For T-Display: Send incremental load update (FIREFLY-0003)
                    if device_type == FireflyDeviceType::Esp32TDisplay {
                        let _ = self.connection.with_device(|s| {
                            s.tdisplay_load(
                                evt.cpu_percent as u8,
                                evt.memory_percent as u8,
                                evt.disk_percent as u8,
                                evt.io_percent as u8,
                                evt.gpu_percent as u8,
                                evt.gpu_active,
                            )
                        });
                    }

                    let mut ctx = self.context.write().await;
                    ctx.load = ((evt.cpu_percent + evt.memory_percent) / 200.0) as f32;
                    ctx.load = ctx.load.clamp(0.0, 1.0);
                    ctx.cpu_percent = evt.cpu_percent as u8;
                    ctx.memory_percent = evt.memory_percent as u8;
                }
            }

            // Stone tended - sparkle override / pulse
            event_types::STONE_TENDED => {
                if let Ok(evt) = serde_json::from_str::<TendedEvent>(&event.data) {
                    tracing::info!("Stone tended - showing appreciation");

                    match device_type {
                        FireflyDeviceType::Esp8266Oled => {
                            self.send_oled_wipe_in("ZEN GARDEN", "TENDING");
                        }
                        FireflyDeviceType::Esp32TDisplay => {
                            let by = evt.by.as_deref().unwrap_or("unknown");
                            let _ = self.connection.with_device(|s| {
                                s.tdisplay_tended(by, "")
                            });
                        }
                        FireflyDeviceType::Rp2040Matrix | FireflyDeviceType::Unknown => {
                            let mut ctx = self.context.write().await;
                            ctx.trigger_override(Override::Tended);
                        }
                    }
                }
            }

            // Managed storage connected (STORAGE-0010)
            event_types::STORAGE_CONNECTED => {
                #[derive(Deserialize)]
                struct ConnectedEvent {
                    name: String,
                    #[serde(default)]
                    capacity_gb: u64,
                }
                if let Ok(evt) = serde_json::from_str::<ConnectedEvent>(&event.data) {
                    tracing::info!(name = %evt.name, "Storage connected");

                    match device_type {
                        FireflyDeviceType::Esp8266Oled => {
                            self.send_oled_wipe_in("STORAGE", "CONNECTED");
                        }
                        FireflyDeviceType::Esp32TDisplay => {
                            let _ = self.connection.with_device(|s| {
                                s.tdisplay_seed_bank_detected(&evt.name, 0, evt.capacity_gb)
                            });
                        }
                        FireflyDeviceType::Rp2040Matrix | FireflyDeviceType::Unknown => {
                            let mut ctx = self.context.write().await;
                            ctx.has_seed_bank = true;
                            ctx.trigger_override(Override::StorageDetected);
                        }
                    }
                }
            }

            // Unmanaged device detected — informational only (STORAGE-0010)
            event_types::STORAGE_DETECTED => {
                tracing::debug!("Unmanaged storage detected (no LED action)");
            }

            // Storage removed
            event_types::STORAGE_REMOVED => {
                #[derive(Deserialize)]
                struct StorageEvent {
                    name: String,
                }
                if let Ok(evt) = serde_json::from_str::<StorageEvent>(&event.data) {
                    tracing::info!(name = %evt.name, "Seed bank removed");

                    match device_type {
                        FireflyDeviceType::Esp8266Oled => {
                            self.send_oled_wipe_out("SEED BANK", "REMOVED");
                        }
                        FireflyDeviceType::Esp32TDisplay => {
                            let _ = self.connection.with_device(|s| {
                                s.tdisplay_seed_bank_removed()
                            });
                        }
                        FireflyDeviceType::Rp2040Matrix | FireflyDeviceType::Unknown => {
                            let mut ctx = self.context.write().await;
                            ctx.has_seed_bank = false;
                            ctx.trigger_override(Override::StorageRemoved);
                        }
                    }
                }
            }

            // Ignore other events
            _ => {
                tracing::trace!(event_type = %event.event_type, "Ignoring unhandled event");
            }
        }
    }
}
