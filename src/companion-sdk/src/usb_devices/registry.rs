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
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

const REGISTRY_EVENT_CAPACITY: usize = 32;

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
}

impl UsbRegistry {
    pub fn new(baud: u32) -> Arc<Self> {
        let (events, _) = broadcast::channel(REGISTRY_EVENT_CAPACITY);
        Arc::new(Self {
            devices: RwLock::new(HashMap::new()),
            events,
            baud,
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
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                next = monitor.next() => match next {
                    Some(MonitorEvent::Added(descriptor)) => {
                        let this = Arc::clone(&self);
                        // Opening the port is async (blocking syscall
                        // on a worker thread). Fan out so one slow
                        // open doesn't stall other events.
                        tokio::spawn(async move { this.handle_added(descriptor).await; });
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

    async fn handle_added(self: Arc<Self>, descriptor: UsbDescriptor) {
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
        let device = self.devices.write().unwrap().remove(&id);
        let Some(device) = device else {
            return;
        };
        device.dispose();
        info!(device = %id, "device removed");
        let _ = self.events.send(RegistryEvent::Disappeared(device));
    }
}
