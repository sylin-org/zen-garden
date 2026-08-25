//! Bridge koi trust-lifecycle events into zen's domain event bus (felt-safety).
//!
//! koi emits [`KoiEvent`]s on a broadcast stream — posture transitions (identity
//! gained/lost) and certificate renewal lifecycle. This task subscribes after
//! Moss is built, maps the pond-relevant variants to [`PondEvent`], and emits them
//! onto the [`EventBus`] so the pulse bridge / SSE / companions can surface them.
//!
//! Discovery, DNS, proxy, and runtime `KoiEvent`s are ignored here — other
//! subsystems own those. Lagged receivers warn and continue (never break the
//! stream), per the domain-event-subscription convention (code-standards §13).

use crate::domain::events::PondEvent;
use crate::infra::event_bus::EventBus;
use koi_embedded::{KoiEvent, KoiHandle};
use std::sync::Arc;
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;

/// Spawn the koi-event → event-bus bridge. Runs until `shutdown` is cancelled or
/// the koi event stream closes.
pub fn spawn(koi: Arc<KoiHandle>, event_bus: EventBus, shutdown: CancellationToken) {
    tokio::spawn(async move {
        let mut events = koi.events();
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                next = events.next() => match next {
                    Some(Ok(event)) => {
                        if let Some(pond) = map_event(event) {
                            event_bus.emit(pond);
                        }
                    }
                    Some(Err(e)) => {
                        tracing::warn!(error = %e, "koi event stream lagged — continuing");
                    }
                    None => break,
                },
            }
        }
        tracing::debug!("koi event bridge stopped");
    });
}

/// Map a koi event to its pond-domain counterpart, or `None` for events this
/// bridge does not surface (discovery, dns, proxy, runtime).
fn map_event(event: KoiEvent) -> Option<PondEvent> {
    match event {
        KoiEvent::PostureChanged { to, .. } => Some(PondEvent::posture_changed(to.signed)),
        KoiEvent::CertRenewed { expires_at } => Some(PondEvent::cert_renewed(expires_at)),
        KoiEvent::CertExpiringSoon { days_left } => Some(PondEvent::cert_expiring(days_left)),
        KoiEvent::CertRenewalFailed {
            reason,
            consecutive_failures,
        } => Some(PondEvent::cert_renewal_failed(reason, consecutive_failures)),
        _ => None,
    }
}
