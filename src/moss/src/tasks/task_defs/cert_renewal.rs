//! BackgroundTask: cert-renewal (ARCH-0015, koi authz plane Stage 3C/3D)
//!
//! Periodically renews this stone's pond leaf before it expires. zen drives
//! renewal itself (it does not run koi's background loop), so this timer is the
//! trigger. Each tick does both role paths — each a cheap no-op on the wrong role:
//!
//! - **cornerstone**: [`renewal::renew_cornerstone_self_leaf_if_due`] re-issues the
//!   CA's own self leaf locally when due. koi emits its lifecycle events, forwarded
//!   by the `koi_events` bridge, so the task only logs a failure here.
//! - **member**: [`renewal::renew_member_identity`] rotates the leaf over the clear
//!   plane when due. The member's koi stream is silent during a clear-plane
//!   renewal, so the task emits the felt-safety PondEvents itself: `CertRenewed` on
//!   success, a warm `RejoinRequired` when past grace (retrying never helps — the
//!   operator rejoins), `CertRenewalFailed` on a transient failure (retry next tick).
//!
//! First check 60s after boot (let topology populate so the cornerstone is
//! reachable), then hourly.

use std::future::Future;
use std::pin::Pin;

use crate::domain::PondEvent;
use crate::domain::security::renewal::{self, RenewOutcome};
use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct CertRenewalTask;

impl BackgroundTask for CertRenewalTask {
    fn name(&self) -> &'static str {
        "cert-renewal"
    }

    fn run(self: Box<Self>, ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            ctx.ready.signal();

            // Wait 60s after boot before the first check (topology needs to
            // populate so the cornerstone is discoverable), or exit on shutdown.
            tokio::select! {
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(60)) => {}
                _ = ctx.token.cancelled() => {
                    tracing::debug!("Cert renewal cancelled during startup delay");
                    return TaskOutcome::Cancelled;
                }
            }

            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(3600));
            // Consecutive transient failures, carried across ticks for the event.
            let mut consecutive_failures: u32 = 0;

            loop {
                tokio::select! {
                    _ = interval.tick() => {}
                    _ = ctx.token.cancelled() => {
                        tracing::debug!("Cert renewal shutting down");
                        break;
                    }
                }

                // Keep the CA's own self leaf fresh first (a no-op on members).
                // koi emits its own lifecycle events (forwarded by the koi_events
                // bridge), so we only log a failure here — never re-emit.
                if let Err(e) = renewal::renew_cornerstone_self_leaf_if_due(&ctx.state).await {
                    tracing::warn!(
                        error = %format!("{e:#}"),
                        "Cornerstone CA self-leaf renewal failed — will retry on the next tick"
                    );
                }

                // Member clear-plane renewal (a no-op / Skipped on the cornerstone).
                match renewal::renew_member_identity(&ctx.state).await {
                    Ok(RenewOutcome::Renewed { expires }) => {
                        consecutive_failures = 0;
                        match chrono::DateTime::parse_from_rfc3339(&expires) {
                            Ok(dt) => ctx
                                .state
                                .event_bus
                                .emit(PondEvent::cert_renewed(dt.with_timezone(&chrono::Utc))),
                            Err(e) => tracing::warn!(
                                error = %e,
                                expires = %expires,
                                "Renewed leaf has an unparseable expiry — skipping cert_renewed event"
                            ),
                        }
                    }
                    Ok(RenewOutcome::NotDue { expires_in_days }) => {
                        consecutive_failures = 0;
                        tracing::debug!(expires_in_days, "Pond leaf not yet due for renewal");
                    }
                    Ok(RenewOutcome::Skipped { reason }) => {
                        consecutive_failures = 0;
                        tracing::debug!(reason, "Pond renewal skipped");
                    }
                    Ok(RenewOutcome::RejoinRequired { reason }) => {
                        // Not a transient failure — retrying never recovers a
                        // past-grace identity. Surface warmly and stop counting it
                        // as a failure streak.
                        consecutive_failures = 0;
                        tracing::warn!(%reason, "Pond identity past grace — rejoin required");
                        ctx.state.event_bus.emit(PondEvent::rejoin_required(reason));
                    }
                    Err(e) => {
                        consecutive_failures = consecutive_failures.saturating_add(1);
                        let reason = format!("{e:#}");
                        tracing::warn!(
                            error = %reason,
                            consecutive_failures,
                            "Pond renewal attempt failed — will retry on the next tick"
                        );
                        ctx.state
                            .event_bus
                            .emit(PondEvent::cert_renewal_failed(reason, consecutive_failures));
                    }
                }
            }

            TaskOutcome::Cancelled
        })
    }
}
