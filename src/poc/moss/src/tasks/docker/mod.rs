//! Docker daemon monitoring background task
//!
//! Monitors Docker daemon availability and broadcasts changes to subscribers.
//! Handles disconnections with configurable retry intervals.
//!
//! ## Architecture
//! - Background task polls Docker ping endpoint
//! - When disconnected, retries every N seconds (tunable, default 5s)
//! - When connected, polls less frequently (default 30s)
//! - Broadcasts Event when state changes
//! - Updates subsystems "docker" readiness via ARCH-0023 aggregate

use crate::docker::ContainerRuntime;
use crate::domain::Subsystems;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

/// Default retry interval when disconnected (Docker daemon unavailable)
pub const DEFAULT_DISCONNECT_RETRY_SECS: u64 = 5;

/// Default poll interval when connected (Docker daemon healthy)
pub const DEFAULT_CONNECTED_POLL_SECS: u64 = 30;

/// Docker events broadcast by the monitor
#[derive(Debug, Clone)]
pub enum Event {
    /// Docker daemon became available (initial connection)
    Connected,
    /// Docker daemon became unavailable
    Disconnected {
        /// Reason for disconnection
        reason: String,
    },
    /// Reconnected after being disconnected
    Reconnected,
}

/// Configuration for the Docker monitor
#[derive(Debug, Clone)]
pub struct Config {
    /// Retry interval when disconnected (seconds)
    pub disconnect_retry_secs: u64,
    /// Poll interval when connected (seconds)
    pub connected_poll_secs: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            disconnect_retry_secs: DEFAULT_DISCONNECT_RETRY_SECS,
            connected_poll_secs: DEFAULT_CONNECTED_POLL_SECS,
        }
    }
}

impl Config {
    /// Create config with custom disconnect retry interval
    pub fn with_disconnect_retry(mut self, secs: u64) -> Self {
        self.disconnect_retry_secs = secs;
        self
    }

    /// Create config with custom connected poll interval
    pub fn with_connected_poll(mut self, secs: u64) -> Self {
        self.connected_poll_secs = secs;
        self
    }
}

/// Docker monitor that tracks daemon availability and broadcasts changes
#[derive(Clone)]
pub struct Monitor {
    /// Docker manager reference (stored for potential future methods)
    _docker: Arc<ContainerRuntime>,
    tx: broadcast::Sender<Event>,
    /// Subsystems aggregate reference (ARCH-0023)
    subsystems: Arc<Subsystems>,
}

impl Monitor {
    /// Start background Docker monitoring with default config
    pub async fn start(docker: Arc<ContainerRuntime>, subsystems: Arc<Subsystems>) -> Self {
        Self::start_with_config(docker, Config::default(), subsystems).await
    }

    /// Start background Docker monitoring with custom config
    pub async fn start_with_config(
        docker: Arc<ContainerRuntime>,
        config: Config,
        subsystems: Arc<Subsystems>,
    ) -> Self {
        let (tx, _) = broadcast::channel(garden_common::constants::channels::MONITOR_EVENT);

        // Check initial state and set via Subsystems aggregate
        let initially_healthy = docker.is_healthy().await;
        if initially_healthy {
            subsystems.mark_ready("docker").await;
        }

        let monitor = Self {
            _docker: docker.clone(),
            tx: tx.clone(),
            subsystems: subsystems.clone(),
        };

        // Log initial state
        if !initially_healthy {
            tracing::warn!(
                retry_secs = config.disconnect_retry_secs,
                "Monitor started in disconnected state, will retry"
            );
        } else {
            tracing::info!(
                poll_secs = config.connected_poll_secs,
                docker_ready = true,
                "Monitor started with healthy Docker daemon"
            );
        }

        // Spawn monitor task
        tokio::spawn(docker_monitor_task(docker, tx, config, subsystems));

        monitor
    }

    /// Check if Docker is currently available
    pub fn is_ready(&self) -> bool {
        self.subsystems.is_ready("docker")
    }

    /// Subscribe to Docker events
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }
}

/// Background task that monitors Docker daemon health
async fn docker_monitor_task(
    docker: Arc<ContainerRuntime>,
    tx: broadcast::Sender<Event>,
    config: Config,
    subsystems: Arc<Subsystems>,
) {
    let mut was_disconnected = !docker.is_healthy().await;

    loop {
        // Determine poll interval based on current state
        let interval = if was_disconnected {
            Duration::from_secs(config.disconnect_retry_secs)
        } else {
            Duration::from_secs(config.connected_poll_secs)
        };

        tokio::time::sleep(interval).await;

        // Check Docker health
        let is_healthy = docker.is_healthy().await;
        let now_disconnected = !is_healthy;

        if was_disconnected != now_disconnected {
            // State changed — update via Subsystems aggregate
            if now_disconnected {
                subsystems
                    .mark_unready("docker", "Docker ping failed")
                    .await;
            } else {
                subsystems.mark_ready("docker").await;
            }

            let event = if was_disconnected && !now_disconnected {
                // Reconnected
                tracing::info!(docker_ready = true, "Docker daemon reconnected");
                Event::Reconnected
            } else if !was_disconnected && now_disconnected {
                // Disconnected
                tracing::warn!(
                    docker_ready = false,
                    retry_secs = config.disconnect_retry_secs,
                    "Docker daemon disconnected, will retry"
                );
                Event::Disconnected {
                    reason: "Docker ping failed".to_string(),
                }
            } else {
                // This shouldn't happen given the if condition, but handle it
                continue;
            };

            // Broadcast event (ignore if no receivers)
            let _ = tx.send(event);

            was_disconnected = now_disconnected;
        } else if was_disconnected {
            // Still disconnected, log at debug level
            tracing::debug!(
                retry_secs = config.disconnect_retry_secs,
                "Docker still disconnected, retrying..."
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = Config::default();
        assert_eq!(config.disconnect_retry_secs, DEFAULT_DISCONNECT_RETRY_SECS);
        assert_eq!(config.connected_poll_secs, DEFAULT_CONNECTED_POLL_SECS);
    }

    #[test]
    fn test_config_builder() {
        let config = Config::default()
            .with_disconnect_retry(10)
            .with_connected_poll(60);
        assert_eq!(config.disconnect_retry_secs, 10);
        assert_eq!(config.connected_poll_secs, 60);
    }
}
