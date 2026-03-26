//! Health poller — generic periodic health-check driver for clustered services.
//!
//! Runs a configurable-interval loop that:
//! 1. Iterates logical sets
//! 2. Calls the adapter's `health_check()` for each set
//! 3. Detects membership changes (new/removed/health transitions)
//! 4. Emits `MembershipEvent`s via a broadcast channel
//! 5. Supports reactive wake-up via `Notify` (e.g. after discovery changes)
//!
//! Models MongoDB's conductor pattern but database-agnostic.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{broadcast, Notify};
use tokio::time::interval;
use tokio_util::sync::CancellationToken;

use super::adapter::MemberHealth;
use super::logical_set::{LogicalSet, MembershipEvent, SetPhase};

/// Configuration for the health poller.
pub struct HealthPollerConfig {
    /// How often to run periodic health checks.
    pub interval: Duration,
    /// How long to wait after startup before the first health check
    /// (allows discovery to populate instances first).
    pub initial_delay: Duration,
}

impl Default for HealthPollerConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(15),
            initial_delay: Duration::from_secs(15),
        }
    }
}

/// Health check function signature.
///
/// Each orchestrator provides a closure that, given a set name, returns
/// the current member health snapshot. The poller calls this on every tick.
pub type HealthCheckFn =
    Arc<dyn Fn(String) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<MemberHealth>> + Send>> + Send + Sync>;

/// Run the health poller loop.
///
/// This is a long-running task — spawn it in a background task and cancel
/// via the `shutdown` token.
///
/// # Arguments
///
/// * `check` — async function: set_name → member health snapshot
/// * `sets` — shared map of logical sets
/// * `reactive` — notify handle for immediate wake-up (e.g. after discovery)
/// * `events` — broadcast sender for membership events
/// * `config` — polling configuration
/// * `shutdown` — cancellation token
pub async fn run(
    check: HealthCheckFn,
    sets: Arc<tokio::sync::RwLock<HashMap<String, LogicalSet>>>,
    reactive: Arc<Notify>,
    events: broadcast::Sender<MembershipEvent>,
    config: HealthPollerConfig,
    shutdown: CancellationToken,
) {
    // Wait for initial discovery
    tokio::select! {
        _ = shutdown.cancelled() => return,
        _ = tokio::time::sleep(config.initial_delay) => {}
    }

    let mut timer = interval(config.interval);

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                tracing::info!("health poller shutting down");
                return;
            }

            _ = reactive.notified() => {
                tracing::debug!("health poller: reactive wake-up");
                poll_all_sets(&check, &sets, &events).await;
            }

            _ = timer.tick() => {
                poll_all_sets(&check, &sets, &events).await;
            }
        }
    }
}

/// Poll health for all logical sets and emit events on changes.
async fn poll_all_sets(
    check: &HealthCheckFn,
    sets: &tokio::sync::RwLock<HashMap<String, LogicalSet>>,
    events: &broadcast::Sender<MembershipEvent>,
) {
    let set_names: Vec<String> = {
        let guard = sets.read().await;
        guard.keys().cloned().collect()
    };

    for set_name in set_names {
        let members = check(set_name.clone()).await;
        let emitted = apply_health_results(sets, &set_name, &members).await;
        for event in emitted {
            let _ = events.send(event);
        }
    }
}

/// Apply health-check results to a logical set, returning any membership events.
async fn apply_health_results(
    sets: &tokio::sync::RwLock<HashMap<String, LogicalSet>>,
    set_name: &str,
    members: &[MemberHealth],
) -> Vec<MembershipEvent> {
    let mut events = Vec::new();
    let mut guard = sets.write().await;

    let set = match guard.get_mut(set_name) {
        Some(s) => s,
        None => return events,
    };

    // Track which endpoints the health check reported
    let reported_endpoints: std::collections::HashSet<&str> =
        members.iter().map(|m| m.endpoint.as_str()).collect();

    // Detect new members
    for member in members {
        let already_known = set
            .known_members
            .iter()
            .any(|km| km.endpoint == member.endpoint);
        if !already_known {
            set.upsert_member(super::logical_set::KnownMember {
                stone_name: member.stone_name.clone(),
                endpoint: member.endpoint.clone(),
                member_id: String::new(),
            });
            events.push(MembershipEvent::Added {
                endpoint: member.endpoint.clone(),
                stone_name: member.stone_name.clone(),
            });
        }
    }

    // Detect removed members (known but not in health check results)
    let removed: Vec<String> = set
        .known_members
        .iter()
        .filter(|km| !reported_endpoints.contains(km.endpoint.as_str()))
        .map(|km| km.endpoint.clone())
        .collect();

    for endpoint in &removed {
        set.remove_member(endpoint);
        events.push(MembershipEvent::Removed {
            endpoint: endpoint.clone(),
        });
    }

    // Update phase based on health
    let all_healthy = members.iter().all(|m| m.healthy);
    let any_healthy = members.iter().any(|m| m.healthy);
    let new_phase = if members.is_empty() {
        SetPhase::New
    } else if all_healthy {
        SetPhase::Healthy
    } else if any_healthy {
        SetPhase::Degraded
    } else {
        SetPhase::Degraded
    };

    if let Some(event) = set.set_phase(new_phase) {
        events.push(event);
    }

    events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn apply_detects_new_members() {
        let sets = Arc::new(tokio::sync::RwLock::new(HashMap::new()));
        {
            let mut guard = sets.write().await;
            guard.insert("test-set".into(), LogicalSet::new("test-set"));
        }

        let members = vec![MemberHealth {
            endpoint: "10.0.0.1:5432".into(),
            stone_name: "stone-a".into(),
            healthy: true,
            lag_seconds: None,
        }];

        let events = apply_health_results(&sets, "test-set", &members).await;
        assert_eq!(events.len(), 2); // Added + PhaseChanged (New→Healthy)

        assert!(matches!(&events[0], MembershipEvent::Added { endpoint, .. } if endpoint == "10.0.0.1:5432"));
    }

    #[tokio::test]
    async fn apply_detects_removed_members() {
        let sets = Arc::new(tokio::sync::RwLock::new(HashMap::new()));
        {
            let mut guard = sets.write().await;
            let mut set = LogicalSet::new("test-set");
            set.upsert_member(super::super::logical_set::KnownMember {
                stone_name: "stone-a".into(),
                endpoint: "10.0.0.1:5432".into(),
                member_id: String::new(),
            });
            set.set_phase(SetPhase::Healthy);
            guard.insert("test-set".into(), set);
        }

        // Health check returns empty — member gone
        let events = apply_health_results(&sets, "test-set", &[]).await;
        assert!(events.iter().any(|e| matches!(e, MembershipEvent::Removed { endpoint } if endpoint == "10.0.0.1:5432")));
    }

    #[tokio::test]
    async fn apply_transitions_phase() {
        let sets = Arc::new(tokio::sync::RwLock::new(HashMap::new()));
        {
            let mut guard = sets.write().await;
            guard.insert("test-set".into(), LogicalSet::new("test-set"));
        }

        let members = vec![
            MemberHealth {
                endpoint: "10.0.0.1:5432".into(),
                stone_name: "stone-a".into(),
                healthy: true,
                lag_seconds: None,
            },
            MemberHealth {
                endpoint: "10.0.0.2:5432".into(),
                stone_name: "stone-b".into(),
                healthy: false,
                lag_seconds: Some(30.0),
            },
        ];

        let events = apply_health_results(&sets, "test-set", &members).await;

        // Should transition to Degraded (not all healthy)
        let phase_event = events
            .iter()
            .find(|e| matches!(e, MembershipEvent::PhaseChanged { .. }));
        assert!(matches!(
            phase_event,
            Some(MembershipEvent::PhaseChanged { to: SetPhase::Degraded, .. })
        ));
    }
}
