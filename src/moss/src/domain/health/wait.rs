//! Wait-for-health polling utility.
//!
//! Polls a [`HealthProbe`] until the named offering reports
//! [`ServiceHealthStatus::Healthy`] or a caller-specified timeout
//! elapses. Used by ceremony phases that need a passive
//! "is it up yet?" check during container start/restart flows —
//! the Water phase of nourish ceremonies today, and the plant
//! flow in [ORCH-0039] going forward.
//!
//! ## What this is *not*
//!
//! This is not [`Health::probe_offering`]. That method records
//! state transitions, mutates the offering through the Offerings
//! aggregate, and emits `HealthChanged` events. The wait helper
//! does none of that — it's a passive polling loop whose only
//! output is a boolean.
//!
//! Most callers want [`Health::wait_until_healthy`] (the method
//! form), which uses the aggregate's injected probe port. The
//! free [`poll_until_healthy`] function is exposed for tests and
//! for callers who already hold a `&dyn HealthProbe` directly.
//!
//! [ORCH-0039]: ../../../../../docs/decisions/ORCH-0039-seed-based-offering-replication.md

use std::time::{Duration, Instant};

use garden_common::ServiceHealthStatus;

use super::aggregate::Health;
use super::probe::HealthProbe;

/// Polling interval between health checks. 3 s matches the value
/// used by the Water phase since the original ceremony framework
/// — short enough to feel responsive, long enough not to hammer
/// the container runtime's health endpoint.
pub const HEALTH_POLL_INTERVAL: Duration = Duration::from_secs(3);

impl Health {
    /// Poll the injected probe until `name` reports `Healthy` or
    /// `timeout` elapses. Returns `true` on success, `false` on
    /// timeout. Errors and non-`Healthy` statuses log at debug
    /// and continue the loop.
    pub async fn wait_until_healthy(&self, name: &str, timeout: Duration) -> bool {
        poll_until_healthy(self.probe(), name, timeout, HEALTH_POLL_INTERVAL).await
    }
}

/// Generic polling loop. Returns `true` if `probe` ever yields
/// `Ok(result)` with `result.health == Healthy` before `timeout`
/// elapses. Other results (non-healthy statuses, errors) are
/// debug-logged and the loop continues.
///
/// `probe.probe(name)` is called repeatedly with `poll_interval`
/// sleep between calls; a fast probe with a fast `Healthy` reply
/// returns immediately without sleeping.
pub async fn poll_until_healthy(
    probe: &dyn HealthProbe,
    name: &str,
    timeout: Duration,
    poll_interval: Duration,
) -> bool {
    let start = Instant::now();
    loop {
        match probe.probe(name).await {
            Ok(result) if result.health == ServiceHealthStatus::Healthy => return true,
            Ok(result) => tracing::debug!(
                offering = name,
                health = ?result.health,
                status = ?result.status,
                elapsed = ?start.elapsed(),
                "Waiting for health...",
            ),
            Err(e) => tracing::debug!(
                offering = name,
                error = %e,
                "Health check error, retrying...",
            ),
        }
        if start.elapsed() >= timeout {
            return false;
        }
        tokio::time::sleep(poll_interval).await;
        if start.elapsed() >= timeout {
            return false;
        }
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the polling loop, using a small fake probe.
    //! The existing `FakeHealthProbe` in `health/tests.rs` is
    //! private to that module's tests, so we duplicate a minimal
    //! version here. Tests use real (short) wall-clock durations
    //! rather than paused virtual time — moss's tokio doesn't
    //! enable the `test-util` feature, and the sub-second
    //! real-time waits are acceptable in CI.

    use super::*;
    use crate::domain::health::probe::HealthProbeResult;
    use anyhow::Result;
    use garden_common::{OfferingStatus, ServiceHealthStatus};
    use std::future::Future;
    use std::pin::Pin;
    use tokio::sync::Mutex;

    struct ScriptedProbe {
        responses: Mutex<Vec<Result<HealthProbeResult>>>,
    }

    impl ScriptedProbe {
        fn new(responses: Vec<Result<HealthProbeResult>>) -> Self {
            Self {
                responses: Mutex::new(responses),
            }
        }

        fn ok(health: ServiceHealthStatus) -> Result<HealthProbeResult> {
            Ok(HealthProbeResult {
                status: OfferingStatus::Running,
                health,
            })
        }

        fn err(msg: &str) -> Result<HealthProbeResult> {
            Err(anyhow::anyhow!("{msg}"))
        }
    }

    impl HealthProbe for ScriptedProbe {
        fn probe<'a>(
            &'a self,
            _name: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<HealthProbeResult>> + Send + 'a>> {
            Box::pin(async move {
                let mut responses = self.responses.lock().await;
                if responses.is_empty() {
                    // Once script exhausted, keep returning the
                    // last shape — `Offline` so no test
                    // accidentally trips the Healthy short-circuit
                    // by overrunning its scripted ending.
                    Ok(HealthProbeResult {
                        status: OfferingStatus::Stopped,
                        health: ServiceHealthStatus::Offline,
                    })
                } else {
                    responses.remove(0)
                }
            })
        }
    }

    #[tokio::test]
    async fn returns_true_immediately_on_first_healthy_probe() {
        let probe = ScriptedProbe::new(vec![ScriptedProbe::ok(ServiceHealthStatus::Healthy)]);
        let started = Instant::now();
        let ok = poll_until_healthy(
            &probe,
            "svc",
            Duration::from_secs(120),
            Duration::from_secs(3),
        )
        .await;
        assert!(ok, "must return true on first Healthy probe");
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "must not sleep when first probe is Healthy"
        );
    }

    #[tokio::test]
    async fn returns_true_after_a_few_unhealthy_probes() {
        let probe = ScriptedProbe::new(vec![
            ScriptedProbe::ok(ServiceHealthStatus::Offline),
            ScriptedProbe::ok(ServiceHealthStatus::Degraded),
            ScriptedProbe::ok(ServiceHealthStatus::Healthy),
        ]);
        let ok = poll_until_healthy(
            &probe,
            "svc",
            Duration::from_secs(5),
            Duration::from_millis(50),
        )
        .await;
        assert!(ok, "Healthy after non-healthy results must succeed");
    }

    #[tokio::test]
    async fn returns_false_when_timeout_expires() {
        // Probe always returns Unhealthy. Loop must give up at
        // timeout and return false. With paused virtual time the
        // sleep advances instantly so this is fast.
        let probe = ScriptedProbe::new(vec![]);
        let ok = poll_until_healthy(
            &probe,
            "svc",
            Duration::from_millis(50),
            Duration::from_millis(10),
        )
        .await;
        assert!(!ok, "exhausted timeout with no Healthy must return false");
    }

    #[tokio::test]
    async fn keeps_polling_through_probe_errors() {
        let probe = ScriptedProbe::new(vec![
            ScriptedProbe::err("connection refused"),
            ScriptedProbe::err("not ready yet"),
            ScriptedProbe::ok(ServiceHealthStatus::Healthy),
        ]);
        let ok = poll_until_healthy(
            &probe,
            "svc",
            Duration::from_secs(5),
            Duration::from_millis(50),
        )
        .await;
        assert!(ok, "transient errors must not abort the wait");
    }

    #[tokio::test]
    async fn does_not_block_indefinitely_when_probe_returns_offline() {
        // ScriptedProbe.responses is empty → the fallback Offline
        // shape kicks in. Must time out.
        let probe = ScriptedProbe::new(vec![]);
        let ok = poll_until_healthy(
            &probe,
            "svc",
            Duration::from_millis(50),
            Duration::from_millis(10),
        )
        .await;
        assert!(!ok);
    }
}
