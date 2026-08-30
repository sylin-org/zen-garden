// The Converger loop wires in with main.rs's background steps (O2 finish).
#![allow(dead_code)]

//! The Converger (OFFERINGS.md §3.1/§6): drives reality toward the stored
//! plan on a protocol floor. Rules, in order of authority:
//!   missing + desired running  → place (heal)
//!   present + stopped          → left alone if rested (§3.2), started if
//!                                the registry says it should run
//!   failures accumulate        → degraded after MAX_ATTEMPTS, then quiet
//!   observed running           → heals a degraded marking (external rescue)

use crate::garden::model::Status;
use crate::garden::service::OfferingService;
use std::collections::HashMap;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Protocol floor between convergence sweeps.
pub const INTERVAL_SECS: u64 = 30;
/// Failed placements before an offering is marked degraded.
pub const MAX_ATTEMPTS: u32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Healthy,
    Healed,
    Started,
    RestedMissing,
    Degraded,
}

/// One sweep across every active managed offering.
pub async fn converge_once(service: &OfferingService) -> Vec<(String, Outcome)> {
    let mut results = Vec::new();
    for offering in service.registry().snapshot() {
        let Some(m) = offering.managed() else { continue };
        let Ok(world) = service.world_for(&offering) else {
            results.push((offering.name.to_string(), Outcome::Degraded));
            continue;
        };
        let name = offering.name.clone();
        let observed = world.observe(name.as_str()).await;
        match (observed, offering.status) {
            // Healthy: nothing to do.
            (Some(o), Status::Running) if o.running => {
                results.push((name, Outcome::Healthy));
            }
            // Registry says running but the world disagrees — start it.
            (Some(o), Status::Running) if !o.running && !restarting(&o) => {
                match world.start(name.as_str()).await {
                    Ok(()) => results.push((name, Outcome::Started)),
                    Err(e) => fail(service, &offering.offering_id, &name, e, &mut results),
                }
            }
            // Missing while desired running: heal by re-placing the plan.
            // The stored spec carries the ledgered allocations (ADR-0002) —
            // identity rides along; residence is chosen at the create edge.
            (None, Status::Running | Status::Degraded) => {
                let spec = m.spec.clone();
                match world.place(name.as_str(), &spec).await {
                    Ok(placement) => {
                        refresh_ports(service, &offering, placement.named_host_ports);
                        clear_failure(service, &offering.offering_id);
                        service.audit_healed(&name);
                        results.push((name, Outcome::Healed));
                    }
                    Err(e) => fail(service, &offering.offering_id, &name, e, &mut results),
                }
            }
            // Rested offerings stay rested, even when their workload is
            // absent (§3.2). Dormant by intent is not an error.
            _ => results.push((name, Outcome::RestedMissing)),
        }
    }
    results
}

fn restarting(o: &crate::garden::runtime::Observed) -> bool {
    // Observed carries only running; a restarting container reports
    // running=false in most states we care about here — counted as failure
    // signal via repeated non-running sweeps rather than special-cased.
    let _ = o;
    false
}

fn fail(
    service: &OfferingService,
    id: &str,
    name: &str,
    e: crate::garden::runtime::RuntimeError,
    out: &mut Vec<(String, Outcome)>,
) {
    let count = service.bump_failure(id);
    tracing::warn!(offering = %name, attempt = count, error = %e, "converge failed");
    if count >= MAX_ATTEMPTS {
        service.mark_degraded(id);
        out.push((name.to_string(), Outcome::Degraded));
    } else {
        out.push((name.to_string(), Outcome::Healed)); // retry scheduled next sweep
    }
}

fn refresh_ports(
    service: &OfferingService,
    offering: &crate::garden::model::Offering,
    named: HashMap<String, u16>,
) {
    if named.is_empty() {
        return;
    }
    if let Some(mut o) = service.registry().get(&offering.offering_id)
        && let crate::garden::model::ModeData::Managed(m) = &mut o.mode_data
        && m.port_map != named
    {
        m.port_map = named;
        o.updated_at = chrono::Utc::now();
        service.registry().replace(o);
        tracing::info!(offering = %offering.name, "ports remapped during converge");
    }
}

fn clear_failure(service: &OfferingService, id: &str) {
    service.clear_failure(id);
}

/// The background loop: converge on the floor until cancelled.
pub async fn run(service: Arc<OfferingService>, token: CancellationToken) {
    let mut ticker =
        tokio::time::interval(std::time::Duration::from_secs(INTERVAL_SECS));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ticker.tick().await; // immediate first tick consumed — boot already placed
    loop {
        tokio::select! {
            _ = token.cancelled() => return,
            _ = ticker.tick() => {
                for (name, outcome) in converge_once(&service).await {
                    tracing::debug!(%name, ?outcome, "converge");
                }
                // The detection domain rides the same protocol floor
                // (OFFERINGS.md §1 adopted mode): recognize, confirm,
                // observe — never operate.
                let report = super::detect::detect_once(&service).await;
                if !report.is_empty() {
                    tracing::info!(
                        minted = report.minted.len(),
                        confirmed = report.confirmed.len(),
                        moved = report.observed.len(),
                        "detection sweep"
                    );
                }
                // Capability caches ride the same floor so the room's
                // wishes answer against fresh truth (W1).
                let refreshed = super::capabilities::refresh_once(&service).await;
                if refreshed > 0 {
                    tracing::debug!(refreshed, "capability sweep");
                }
            }
        }
    }
}
