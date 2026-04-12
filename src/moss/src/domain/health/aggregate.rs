//! Health aggregate — stateless command facade for per-offering health
//! probing and transition detection.
//!
//! The Health aggregate does NOT hold per-offering health state (that
//! lives on the `Offering` struct). Instead it orchestrates the
//! probe→compare→mutate→emit pipeline, delegating:
//!
//! - Probe execution to the [`HealthProbe`] port
//! - Offering mutation to the `Offerings` aggregate's `update` method
//! - Event emission through its own `broadcast::Sender<HealthChanged>`
//! - Metrics recording through the injected `Arc<Metrics>`
//!
//! This is the "Ephemeral aggregates" pattern deviation (Book I
//! precedent): no `RwLock<State>`, no `finalize` pipeline, no `Store` port.

use std::sync::Arc;

use garden_common::notifications::{NOTIF_SOURCE_OFFERINGS_DEGRADED, NotificationTag};
use garden_common::{OfferingStatus, ServiceHealthStatus};
use tokio::sync::broadcast;

use super::event::{HealthChangeKind, HealthChanged, classify_transition};
use super::probe::{HealthProbe, HealthProbeResult};
use crate::domain::Metrics;
use crate::domain::Offerings;

/// The Health bounded context's aggregate root.
pub struct Health {
    metrics: Arc<Metrics>,
    probe: Arc<dyn HealthProbe>,
    events: broadcast::Sender<HealthChanged>,
}

impl Health {
    /// Stable context name — used for metrics domain registration.
    pub const NAME: &'static str = "health";

    /// Construct a new Health aggregate.
    pub async fn new(metrics: Arc<Metrics>, probe: Arc<dyn HealthProbe>) -> Self {
        let (events, _) = broadcast::channel(64);
        metrics
            .register_domain(Self::NAME, HealthChangeKind::ALL_NAMES)
            .await;
        Self {
            metrics,
            probe,
            events,
        }
    }

    // =========================================================================
    // Commands
    // =========================================================================

    /// Probe a single offering's health status via the injected port.
    ///
    /// Compares the probe result with the offering's current status/health.
    /// If there is a change, mutates the offering through the Offerings
    /// aggregate and emits a `HealthChanged` event on interesting transitions.
    ///
    /// Returns `true` if the offering's status or health changed.
    #[tracing::instrument(level = "debug", skip(self, offerings), fields(health.offering = %name))]
    pub async fn probe_offering(
        &self,
        offerings: &Arc<Offerings>,
        name: &str,
        offering_id: &str,
        old_status: OfferingStatus,
        old_health: &ServiceHealthStatus,
    ) -> ProbeOutcome {
        let start = std::time::Instant::now();

        let result = match self.probe.probe(name).await {
            Ok(r) => r,
            Err(e) => {
                // Probe failed — check if container exists
                tracing::debug!(
                    offering = %name,
                    error = ?e,
                    "Health probe failed"
                );
                HealthProbeResult {
                    status: OfferingStatus::Stopped,
                    health: ServiceHealthStatus::Offline,
                }
            }
        };

        self.metrics
            .record_mutation_latency(Self::NAME, start.elapsed())
            .await;

        let status_changed = result.status != old_status;
        let health_changed = result.health != *old_health;

        if !status_changed && !health_changed {
            return ProbeOutcome::Unchanged;
        }

        // Detect interesting health transition
        let transition = classify_transition(old_health, &result.health);

        // Mutate offering through the Offerings aggregate
        let new_health = result.health.clone();
        let new_status = result.status;
        offerings
            .update(offering_id, |o| {
                o.status = new_status;
                o.health = new_health.clone();
                true
            })
            .await;

        // Record metrics and emit event for interesting transitions
        if let Some(kind) = transition {
            self.metrics
                .record_domain_event(Self::NAME, kind.name())
                .await;

            let event = HealthChanged {
                kind,
                offering: name.to_string(),
                old_health: old_health.clone(),
                new_health: result.health.clone(),
                timestamp: chrono::Utc::now(),
            };
            // Ignore send failure (no subscribers is fine)
            let _ = self.events.send(event);
        }

        self.metrics.record_domain_event(Self::NAME, "probed").await;

        ProbeOutcome::Changed {
            new_status: result.status,
            new_health: result.health,
        }
    }

    /// Apply a Docker event's status/health to an offering.
    ///
    /// Called by the docker-events task when a container start/stop/die/
    /// health_status event is received. Delegates mutation to the
    /// Offerings aggregate and emits a `HealthChanged` event on
    /// interesting transitions.
    ///
    /// Returns `true` if the offering's status or health changed.
    #[tracing::instrument(level = "debug", skip(self, offerings), fields(health.offering = %name))]
    pub async fn apply_docker_event(
        &self,
        offerings: &Arc<Offerings>,
        offering_id: &str,
        name: &str,
        old_health: &ServiceHealthStatus,
        new_status: OfferingStatus,
        new_health: ServiceHealthStatus,
    ) -> bool {
        let transition = classify_transition(old_health, &new_health);

        let health_clone = new_health.clone();
        let changed = offerings
            .update(offering_id, |o| {
                let s_changed = o.status != new_status;
                let h_changed = o.health != health_clone;
                if s_changed || h_changed {
                    o.status = new_status;
                    o.health = health_clone.clone();
                    true
                } else {
                    false
                }
            })
            .await;

        if changed && let Some(kind) = transition {
            self.metrics
                .record_domain_event(Self::NAME, kind.name())
                .await;

            let event = HealthChanged {
                kind,
                offering: name.to_string(),
                old_health: old_health.clone(),
                new_health,
                timestamp: chrono::Utc::now(),
            };
            let _ = self.events.send(event);
        }

        changed
    }

    /// Scan all offerings and set/clear the degraded-offerings notification.
    ///
    /// Called at the end of each health monitoring cycle.
    #[tracing::instrument(level = "trace", skip(self, offerings, notifications))]
    pub async fn update_notification(
        &self,
        offerings: &Arc<Offerings>,
        notifications: &garden_common::notifications::NotificationRegistry,
    ) {
        let has_degraded = offerings
            .with_active(|active| {
                active.iter().any(|o| {
                    matches!(
                        o.health,
                        ServiceHealthStatus::Degraded | ServiceHealthStatus::Offline
                    )
                })
            })
            .await;

        notifications.set_if(
            NOTIF_SOURCE_OFFERINGS_DEGRADED,
            NotificationTag::Attention,
            has_degraded,
        );
    }

    // =========================================================================
    // Queries
    // =========================================================================

    /// Subscribe to health change events.
    pub fn changes(&self) -> broadcast::Receiver<HealthChanged> {
        self.events.subscribe()
    }
}

/// Outcome of a health probe.
#[derive(Debug, Clone, PartialEq)]
pub enum ProbeOutcome {
    /// No change detected.
    Unchanged,
    /// Status or health changed.
    Changed {
        new_status: OfferingStatus,
        new_health: ServiceHealthStatus,
    },
}

impl ProbeOutcome {
    /// Whether the offering's state changed.
    pub fn is_changed(&self) -> bool {
        matches!(self, Self::Changed { .. })
    }
}
