//! [`FireflyOrchestrator`] — wires USB registry → probe → adapter.
//!
//! Subscribes to the SDK's `UsbRegistry`. For every `Appeared` event:
//!
//! 1. Transition the device to `Evaluating`.
//! 2. Run `FireflyProbe::evaluate`. On success, construct `Firefly`
//!    and accept the device; on failure, reject it.
//! 3. Pick the adapter constructor for this `FireflyKind`; spawn it
//!    through the SDK's adapter supervisor.
//!
//! On `Disappeared` events there is nothing to do — the adapter
//! observes `device.state_changes()` going to `Disposed` and exits
//! its own run loop. The `Arc<Firefly>` releases naturally as the
//! adapter task returns.

use crate::adapters::{MatrixAdapter, OledV1Adapter, OledV2Adapter, TDisplayAdapter};
use crate::firefly::{Firefly, FireflyKind};
use crate::probe::FireflyProbe;
use garden_companion_sdk::adapters::{Adapter, Adapters};
use garden_companion_usb::{RegistryEvent, UsbRegistry, UsbSerialDevice};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

pub struct FireflyOrchestrator {
    registry: Arc<UsbRegistry>,
    adapters: Arc<Adapters>,
    state_dir: Option<PathBuf>,
}

impl FireflyOrchestrator {
    pub fn new(
        registry: Arc<UsbRegistry>,
        adapters: Arc<Adapters>,
        state_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            registry,
            adapters,
            state_dir,
        }
    }

    pub async fn run(self, shutdown: CancellationToken) {
        info!("FireflyOrchestrator starting");
        let mut rx = self.registry.subscribe();
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                event = rx.recv() => match event {
                    Ok(RegistryEvent::Appeared(device)) => {
                        let state_dir = self.state_dir.clone();
                        let adapters = Arc::clone(&self.adapters);
                        tokio::spawn(async move {
                            evaluate_and_spawn(device, adapters, state_dir).await;
                        });
                    }
                    Ok(RegistryEvent::Disappeared(device)) => {
                        debug!(device = %device.id(), "disappeared — adapter will observe state change");
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!(skipped = n, "orchestrator lagged on registry events");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!("registry event stream closed");
                        break;
                    }
                }
            }
        }
        info!("FireflyOrchestrator stopped");
    }
}

async fn evaluate_and_spawn(
    device: Arc<UsbSerialDevice>,
    adapters: Arc<Adapters>,
    state_dir: Option<PathBuf>,
) {
    let id = device.id().clone();
    let port = device.port().clone();

    if let Err(e) = device.begin_evaluation() {
        debug!(device = %id, error = ?e, "not new; skipping evaluation");
        return;
    }

    info!(device = %id, port = %port, "evaluating device");
    match FireflyProbe::evaluate(Arc::clone(&device)).await {
        Ok(firefly) => {
            let kind = firefly.kind;
            let Some(adapter) = build_adapter(Arc::clone(&firefly), state_dir) else {
                warn!(device = %id, kind = ?kind, "firefly kind has no adapter; rejecting");
                let _ = device.reject(format!("no adapter for {kind:?}"));
                return;
            };
            let adapter_kind = adapter.info().kind.to_string();
            if let Err(e) = device.accept(adapter_kind.clone()) {
                warn!(device = %id, error = ?e, "accept transition refused");
                return;
            }
            let adapter_id = adapters.spawn_external(adapter);
            info!(
                device = %id,
                firmware_id = %firefly.identity.device_id,
                kind = ?kind,
                adapter = %adapter_id,
                "firefly claimed"
            );
        }
        Err(e) => {
            let reason = e.to_string();
            info!(device = %id, reason = %reason, "not a firefly; rejecting");
            let _ = device.reject(reason);
        }
    }
}

fn build_adapter(
    firefly: Arc<Firefly>,
    state_dir: Option<PathBuf>,
) -> Option<Box<dyn Adapter>> {
    match firefly.kind {
        FireflyKind::Rp2040Matrix => Some(Box::new(MatrixAdapter::new(firefly, state_dir))),
        FireflyKind::Esp8266OledV2 => Some(Box::new(OledV2Adapter::new(firefly))),
        FireflyKind::Esp8266Oled => Some(Box::new(OledV1Adapter::new(firefly))),
        FireflyKind::Esp32TDisplay => Some(Box::new(TDisplayAdapter::new(firefly))),
        FireflyKind::Unknown => None,
    }
}
