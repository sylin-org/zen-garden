//! [`UsbRegistry`] — the USB devices aggregate.
//!
//! Subscribes to a [`Monitor`] and mediates device lifecycle:
//! opens ports on `Added`, emits `Appeared(Arc<UsbSerialDevice>)` so
//! the orchestrator can evaluate; disposes on `Removed`, emits
//! `Disappeared(Arc<UsbSerialDevice>)`.
//!
//! The registry is the canonical owner of every currently-present
//! USB serial device. Other holders (orchestrator, Firefly, adapter)
//! borrow via `Arc`.

use super::device::{DeviceId, UsbDescriptor, UsbSerialDevice};
use super::monitor::{Monitor, MonitorEvent};
use super::state::DeviceState;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

const REGISTRY_EVENT_CAPACITY: usize = 32;

/// How often the re-probe sweep runs. A device that fails its probe is never permanently abandoned:
/// it could be a firefly that was still settling/booting, or a port that later gets a firefly
/// plugged into the same device node, or one reflashed in place. The sweep re-opens (which resets
/// the device) and re-probes rejected-but-still-present devices.
const REPROBE_CHECK: Duration = Duration::from_secs(2);
/// First re-probe after a rejection — long enough for a slow ESP boot to settle.
const REPROBE_INITIAL: Duration = Duration::from_secs(5);
/// Backoff cap. After the initial fast retries, a confirmed non-firefly is re-probed at this steady
/// interval forever (it could become a firefly at any time) — gentle enough to barely disturb it.
const REPROBE_MAX: Duration = Duration::from_secs(60);

/// Domain event emitted by the registry. Carries the `Arc` so
/// subscribers never do a by-id lookup.
#[derive(Debug, Clone)]
pub enum RegistryEvent {
    /// A device has been opened and is ready for evaluation.
    Appeared(Arc<UsbSerialDevice>),
    /// A device has been disposed. Subscribers should release their
    /// `Arc` reference to free the underlying resources.
    Disappeared(Arc<UsbSerialDevice>),
}

pub struct UsbRegistry {
    devices: RwLock<HashMap<DeviceId, Arc<UsbSerialDevice>>>,
    events: broadcast::Sender<RegistryEvent>,
    baud: u32,
    /// Per-device re-probe schedule for rejected devices: `(next_attempt, current_backoff)`.
    reprobe: Mutex<HashMap<DeviceId, (Instant, Duration)>>,
}

impl UsbRegistry {
    pub fn new(baud: u32) -> Arc<Self> {
        let (events, _) = broadcast::channel(REGISTRY_EVENT_CAPACITY);
        Arc::new(Self {
            devices: RwLock::new(HashMap::new()),
            events,
            baud,
            reprobe: Mutex::new(HashMap::new()),
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<RegistryEvent> {
        self.events.subscribe()
    }

    pub fn get(&self, id: &DeviceId) -> Option<Arc<UsbSerialDevice>> {
        self.devices.read().ok()?.get(id).cloned()
    }

    pub fn len(&self) -> usize {
        self.devices.read().map(|g| g.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Drive the registry from a monitor stream until `shutdown`
    /// fires. On shutdown, disposes every held device so reader
    /// tasks wind down promptly.
    pub async fn run<M: Monitor + ?Sized>(
        self: Arc<Self>,
        mut monitor: Box<M>,
        shutdown: CancellationToken,
    ) {
        info!("UsbRegistry starting");
        let mut reprobe_tick = tokio::time::interval(REPROBE_CHECK);
        reprobe_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = reprobe_tick.tick() => {
                    self.reprobe_due().await;
                }
                next = monitor.next() => match next {
                    Some(MonitorEvent::Added(descriptor)) => {
                        // Serialize inline so a second udev ADD for the
                        // same device can't race past the `contains_key`
                        // check before the first open completes.
                        self.handle_added(descriptor).await;
                    }
                    Some(MonitorEvent::Removed(id)) => {
                        self.handle_removed(id);
                    }
                    None => {
                        info!("monitor stream ended");
                        break;
                    }
                }
            }
        }

        // Dispose every device on shutdown so reader tasks exit.
        let to_dispose: Vec<Arc<UsbSerialDevice>> = {
            let mut guard = self.devices.write().unwrap();
            guard.drain().map(|(_, v)| v).collect()
        };
        for device in to_dispose {
            device.dispose();
        }
        info!("UsbRegistry stopped");
    }

    async fn handle_added(&self, descriptor: UsbDescriptor) {
        // If an id is already present (noisy re-ADD from udev or a
        // stuck-Rejected carry-over), skip. The disposal path is the
        // only way a device leaves the registry.
        if self.devices.read().unwrap().contains_key(&descriptor.id) {
            debug!(device = %descriptor.id, "ignoring duplicate Added; already tracked");
            return;
        }
        let id = descriptor.id.clone();
        let port = descriptor.port.clone();
        match UsbSerialDevice::open(descriptor, self.baud).await {
            Ok(device) => {
                info!(device = %id, port = %port, "device opened");
                self.devices
                    .write()
                    .unwrap()
                    .insert(id.clone(), Arc::clone(&device));
                let _ = self.events.send(RegistryEvent::Appeared(device));
            }
            Err(e) => {
                warn!(device = %id, port = %port, error = %e, "failed to open device; ignoring");
            }
        }
    }

    fn handle_removed(&self, id: DeviceId) {
        self.reprobe.lock().unwrap().remove(&id);
        let device = self.devices.write().unwrap().remove(&id);
        let Some(device) = device else {
            return;
        };
        device.dispose();
        info!(device = %id, "device removed");
        let _ = self.events.send(RegistryEvent::Disappeared(device));
    }

    /// Re-probe sweep: re-open one rejected-but-still-present device whose backoff has elapsed.
    ///
    /// A failed probe is never permanent — re-opening resets the device (clearing a stuck-on-open
    /// state and giving a still-booting ESP another chance) and yields a fresh `New` device, so the
    /// orchestrator evaluates it again. Accepted (claimed) devices are never touched. One device per
    /// sweep keeps a re-open's ~3s open/boot from stalling monitor handling.
    async fn reprobe_due(&self) {
        let now = Instant::now();

        // Snapshot currently-rejected devices.
        let rejected: Vec<(DeviceId, Arc<UsbSerialDevice>)> = {
            let devices = self.devices.read().unwrap();
            devices
                .iter()
                .filter(|(_, d)| matches!(d.state(), DeviceState::Rejected { .. }))
                .map(|(id, d)| (id.clone(), Arc::clone(d)))
                .collect()
        };

        // Pick the first device whose re-probe is due; advance its backoff. Drop schedule entries
        // for devices no longer rejected (claimed) or gone.
        let due = {
            let mut reprobe = self.reprobe.lock().unwrap();
            let rejected_ids: std::collections::HashSet<&DeviceId> =
                rejected.iter().map(|(id, _)| id).collect();
            reprobe.retain(|id, _| rejected_ids.contains(id));

            let mut pick = None;
            for (id, device) in &rejected {
                let entry = reprobe
                    .entry(id.clone())
                    .or_insert((now + REPROBE_INITIAL, REPROBE_INITIAL));
                if now >= entry.0 {
                    let next_backoff = (entry.1 * 2).min(REPROBE_MAX);
                    *entry = (now + next_backoff, next_backoff);
                    pick = Some((id.clone(), Arc::clone(device)));
                    break;
                }
            }
            pick
        };

        let Some((id, old)) = due else {
            return;
        };

        // Close the old port (resets the device), then open a fresh handle on the same descriptor
        // and re-announce it for evaluation.
        let descriptor = old.descriptor().clone();
        old.dispose();
        tokio::time::sleep(Duration::from_millis(300)).await;
        match UsbSerialDevice::open(descriptor, self.baud).await {
            Ok(device) => {
                info!(device = %id, "re-probe: re-opened rejected device");
                self.devices
                    .write()
                    .unwrap()
                    .insert(id.clone(), Arc::clone(&device));
                let _ = self.events.send(RegistryEvent::Appeared(device));
            }
            Err(e) => {
                warn!(device = %id, error = %e, "re-probe: re-open failed; will retry");
            }
        }
    }
}
