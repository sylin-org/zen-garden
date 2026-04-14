//! [`DeviceBus`] — the async runtime that ties the bus pieces together.
//!
//! Responsibilities (COMPANION-0012):
//!
//! - Poll the USB serial enumerator on a configurable interval.
//! - For each newly-attached port, run identity protocols sequentially
//!   against the opened port until one returns `Some(Identification)`.
//! - Use the identity cache as a hint (try the previously-bound
//!   registration first; fall back to full specificity-ordered
//!   matching on cache miss or mismatch).
//! - Spawn the winning registration's adapter via
//!   [`Adapters::spawn_external`].
//! - Track ownership by `DeviceHandle`; on detach, reap the owning
//!   adapter explicitly via [`Adapters::reap_id`].
//! - Emit telemetry events to [`Pulse`] on each failure mode:
//!   `core.companion.device.{unprovisioned, unclaimed, foreign}`.
//! - Apply per-port exponential backoff on probe or spawn failure.
//!
//! The identity-protocol step is the only place the bus opens ports.
//! The resulting `OpenedDevice` is handed to the winning adapter's
//! builder — no re-open, no compounding ESP32 resets.

use super::backoff::BackoffTracker;
use super::cache::DeviceCache;
use super::claim::{ClaimOutcome, pick_winner};
use super::descriptor::Identification;
use super::device::{Device, DeviceHandle, OpenedDevice, OpenedInner};
use super::identity::{IdentifyError, IdentityProtocol};
use super::registration::AdapterRegistration;
use super::resource::ResourceClass;
use super::telemetry::{DeviceForeign, DeviceUnclaimed, DeviceUnprovisioned};
use super::usb_serial::{UsbSerialEnumerator, UsbSerialPort};
use crate::adapters::Adapters;
use crate::garden::{Event, Pulse};

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub const DEFAULT_SCAN_INTERVAL: Duration = Duration::from_secs(5);
/// Per-port stabilization after opening (ESP devices auto-reset on
/// port open; we wait for the firmware to boot and emit its HELLO).
pub const DEFAULT_OPEN_STABILIZATION: Duration = Duration::from_millis(2500);

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Builder for [`DeviceBus`]. Configure identity protocols,
/// registrations, scan interval, and the cache path; call
/// [`DeviceBusBuilder::build`] to produce a ready-to-run bus.
pub struct DeviceBusBuilder {
    identity_protocols: Vec<Arc<dyn IdentityProtocol>>,
    registrations: Vec<AdapterRegistration>,
    scan_interval: Duration,
    stabilization: Duration,
    cache_path: Option<PathBuf>,
    port_filter: ResourceClass,
}

impl Default for DeviceBusBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceBusBuilder {
    pub fn new() -> Self {
        Self {
            identity_protocols: Vec::new(),
            registrations: Vec::new(),
            scan_interval: DEFAULT_SCAN_INTERVAL,
            stabilization: DEFAULT_OPEN_STABILIZATION,
            cache_path: None,
            port_filter: ResourceClass::UsbSerial {
                vid: None,
                pid: None,
            },
        }
    }

    pub fn with_identity_protocol(mut self, p: Arc<dyn IdentityProtocol>) -> Self {
        self.identity_protocols.push(p);
        self
    }

    pub fn with_registration(mut self, r: AdapterRegistration) -> Self {
        self.registrations.push(r);
        self
    }

    pub fn with_scan_interval(mut self, d: Duration) -> Self {
        self.scan_interval = d;
        self
    }

    pub fn with_stabilization(mut self, d: Duration) -> Self {
        self.stabilization = d;
        self
    }

    pub fn with_cache_path(mut self, p: PathBuf) -> Self {
        self.cache_path = Some(p);
        self
    }

    pub fn with_port_filter(mut self, c: ResourceClass) -> Self {
        self.port_filter = c;
        self
    }

    pub fn build(self, adapters: Arc<Adapters>, pulse: Arc<Pulse>) -> DeviceBus {
        let cache = match self.cache_path {
            Some(p) => DeviceCache::load(p),
            None => DeviceCache::memory(),
        };
        DeviceBus {
            enumerator: UsbSerialEnumerator::new(self.port_filter),
            identity_protocols: self.identity_protocols,
            registrations: self.registrations,
            adapters,
            pulse,
            cache,
            backoff: BackoffTracker::new(),
            owned: Mutex::new(HashMap::new()),
            scan_interval: self.scan_interval,
            stabilization: self.stabilization,
        }
    }
}

// ---------------------------------------------------------------------------
// DeviceBus
// ---------------------------------------------------------------------------

/// Ownership record: which adapter currently owns a given device,
/// plus the port metadata needed to re-identify it if the adapter
/// exits without a Detached event firing first.
#[derive(Debug, Clone)]
struct Owned {
    adapter_id: String,
    registration_name: &'static str,
    /// Original port descriptor — kept so that on
    /// [`AdapterExitReason::SelfExit`] / `Panicked` the bus can re-run
    /// `handle_attach` without going back through the enumerator.
    port: UsbSerialPort,
}

/// The bus runtime. Construct via [`DeviceBusBuilder`].
pub struct DeviceBus {
    enumerator: UsbSerialEnumerator,
    identity_protocols: Vec<Arc<dyn IdentityProtocol>>,
    registrations: Vec<AdapterRegistration>,
    adapters: Arc<Adapters>,
    pulse: Arc<Pulse>,
    cache: DeviceCache,
    backoff: BackoffTracker,
    owned: Mutex<HashMap<DeviceHandle, Owned>>,
    scan_interval: Duration,
    stabilization: Duration,
}

impl DeviceBus {
    pub fn builder() -> DeviceBusBuilder {
        DeviceBusBuilder::new()
    }

    /// Number of devices currently owned by spawned adapters.
    pub fn owned_count(&self) -> usize {
        self.owned.lock().unwrap().len()
    }

    /// Run the scan loop until `shutdown` is cancelled. On exit,
    /// detach + reap every owned adapter.
    pub async fn run(&self, shutdown: CancellationToken) {
        // Take the supervisor's adapter-exit channel. Single-consumer
        // — the bus is the canonical owner. If someone else already
        // subscribed (test harness, custom embedding), we degrade to
        // the no-event-driven-cleanup path (the loop still works for
        // detach-driven reclaim).
        let mut exits_rx = self.adapters.subscribe_exits();

        tracing::info!(
            identity_protocols = self.identity_protocols.len(),
            registrations = self.registrations.len(),
            scan_interval_ms = self.scan_interval.as_millis() as u64,
            exit_subscription = exits_rx.is_some(),
            "DeviceBus starting"
        );

        // First tick immediately so devices already plugged in don't
        // wait a full interval.
        self.tick(exits_rx.as_mut()).await;

        let mut interval = tokio::time::interval(self.scan_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        interval.tick().await; // consume the one that fires immediately

        loop {
            tokio::select! {
                _ = interval.tick() => self.tick(exits_rx.as_mut()).await,
                _ = shutdown.cancelled() => break,
            }
        }

        self.reap_all_owned().await;
        tracing::info!("DeviceBus stopped");
    }

    async fn tick(
        &self,
        exits: Option<&mut tokio::sync::mpsc::UnboundedReceiver<crate::adapters::AdapterExited>>,
    ) {
        // Drain pending adapter-exit events first. Adapter teardown
        // → port reclaim happens before any fresh scan so a SelfExit
        // can be re-identified within the same tick.
        if let Some(exits) = exits {
            while let Ok(exit) = exits.try_recv() {
                self.handle_adapter_exit(exit).await;
            }
        }

        let delta = match self.enumerator.scan() {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(error = %e, "DeviceBus scan failed");
                return;
            }
        };

        for handle in delta.detached {
            self.handle_detach(handle).await;
        }

        for port in delta.attached {
            if !self.backoff.is_eligible(&port.port_name) {
                continue;
            }
            self.handle_attach(port).await;
        }
    }

    async fn handle_attach(&self, port: UsbSerialPort) {
        let device = port.to_device();
        let port_name = port.port_name.clone();

        // Open the port. Errors here are I/O-level (permissions, hot
        // unplug race between scan and open) — back off and move on.
        let opened = match open_usb_serial(&port, self.stabilization) {
            Ok(o) => o,
            Err(e) => {
                tracing::debug!(port = %port_name, error = %e, "open failed; backing off");
                self.backoff.note_failure(&port_name);
                return;
            }
        };

        // Try identity protocols in registration order.
        let mut opened = opened;
        let mut identification: Option<Identification> = None;
        let mut malformed_errors: Vec<String> = Vec::new();

        for proto in &self.identity_protocols {
            match proto.identify(&mut opened) {
                Ok(Some(id)) => {
                    identification = Some(id);
                    break;
                }
                Ok(None) => continue,
                Err(IdentifyError::Malformed(msg)) => {
                    malformed_errors.push(format!("{}: {msg}", proto.ecosystem()));
                }
                Err(IdentifyError::Io(msg)) => {
                    tracing::debug!(port = %port_name, protocol = %proto.ecosystem(), error = %msg, "identity probe I/O error");
                }
            }
        }

        let Some(id) = identification else {
            // No protocol claimed this device. Distinguish between
            // "malformed response" and "no response."
            if !malformed_errors.is_empty() {
                // Firmware emitted something but we couldn't parse it.
                // Log and backoff — but don't emit `foreign` (it's one
                // of ours, just broken).
                tracing::warn!(
                    port = %port_name,
                    errors = ?malformed_errors,
                    "identity protocols returned malformed; backing off"
                );
            } else {
                let _ = self.pulse.ingest(Event::new(DeviceForeign {
                    port: port_name.clone(),
                    vid: device.vid,
                    pid: device.pid,
                    product: device.product.clone(),
                }));
            }
            self.backoff.note_failure(&port_name);
            return;
        };

        // Guard against missing `device_id`. Descriptor parsed but no
        // identity means the firmware is running without provisioning.
        if id.device_id.is_empty() {
            let _ = self.pulse.ingest(Event::new(DeviceUnprovisioned {
                port: port_name.clone(),
                ecosystem: id.ecosystem.clone(),
                raw_descriptor: id.fields.clone(),
            }));
            self.backoff.note_failure(&port_name);
            return;
        }

        // Cache hint: try the previously-bound registration first.
        if let Some(hinted) = self.cache.lookup(&id.device_id) {
            let hinted_reg = self
                .registrations
                .iter()
                .find(|r| r.name == hinted)
                .cloned();
            if let Some(reg) = hinted_reg
                && reg.score(&id).is_some()
                && reg.resource.matches_usb(
                    device.vid.unwrap_or(0),
                    device.pid.unwrap_or(0),
                )
            {
                self.spawn_owned(&reg, device, port.clone(), opened, &id).await;
                return;
            }
            // Cached binding no longer applies. Invalidate and fall
            // through to the full dance.
            self.cache.invalidate(&id.device_id);
        }

        // Full dance: specificity-ordered claim.
        match pick_winner(&self.registrations, &device.class, &id) {
            ClaimOutcome::Claimed { index, .. } => {
                let reg = self.registrations[index].clone();
                self.spawn_owned(&reg, device, port.clone(), opened, &id).await;
            }
            ClaimOutcome::Unmatched => {
                let _ = self.pulse.ingest(Event::new(DeviceUnclaimed {
                    port: port_name.clone(),
                    device_id: id.device_id.clone(),
                    descriptor: id.fields.clone(),
                }));
                self.backoff.note_failure(&port_name);
            }
        }
    }

    async fn spawn_owned(
        &self,
        reg: &AdapterRegistration,
        device: Device,
        port: UsbSerialPort,
        opened: OpenedDevice,
        id: &Identification,
    ) {
        let handle = device.handle.clone();
        let adapter = (reg.build)(opened, id);
        let adapter_id = self.adapters.spawn_external(adapter);

        self.owned.lock().unwrap().insert(
            handle.clone(),
            Owned {
                adapter_id: adapter_id.clone(),
                registration_name: reg.name,
                port,
            },
        );
        self.cache.insert(&id.device_id, reg.name);
        self.backoff.note_success(handle.as_str());

        tracing::info!(
            port = %handle,
            device_id = %id.device_id,
            registration = reg.name,
            adapter_id = %adapter_id,
            "device claimed"
        );
    }

    async fn handle_detach(&self, handle: DeviceHandle) {
        let owned = self.owned.lock().unwrap().remove(&handle);
        self.backoff.clear(handle.as_str());
        let Some(owned) = owned else {
            return;
        };
        tracing::info!(
            port = %handle,
            registration = owned.registration_name,
            adapter_id = %owned.adapter_id,
            "device detached — reaping adapter"
        );
        self.adapters.reap_id(&owned.adapter_id).await;
    }

    /// React to an adapter-lifecycle event published by the supervisor.
    ///
    /// - `Reaped`: the bus already drove the teardown via `handle_detach`
    ///   (Detached event from the enumerator). Nothing to do.
    /// - `SelfExit` / `Panicked`: the adapter ended without the bus
    ///   knowing. Drop the owned record, clear backoff, and re-run the
    ///   identification dance against the same port — synchronously,
    ///   so a new adapter is in place before this tick exits.
    async fn handle_adapter_exit(&self, exit: crate::adapters::AdapterExited) {
        use crate::adapters::AdapterExitReason;

        if matches!(exit.reason, AdapterExitReason::Reaped) {
            return;
        }

        // Find the port this adapter owned (if any) and pull both the
        // handle and the cached UsbSerialPort metadata.
        let entry: Option<(DeviceHandle, UsbSerialPort)> = {
            let owned = self.owned.lock().unwrap();
            owned
                .iter()
                .find(|(_, o)| o.adapter_id == exit.id)
                .map(|(h, o)| (h.clone(), o.port.clone()))
        };

        let Some((handle, port)) = entry else {
            // Adapter exit for an id we don't track — nothing to do.
            return;
        };

        match exit.reason {
            AdapterExitReason::SelfExit => tracing::warn!(
                port = %handle,
                adapter_id = %exit.id,
                "adapter exited unexpectedly — re-identifying port"
            ),
            AdapterExitReason::Panicked => tracing::error!(
                port = %handle,
                adapter_id = %exit.id,
                "adapter panicked — re-identifying port"
            ),
            AdapterExitReason::Reaped => unreachable!(),
        }

        // Release the bus-side bookkeeping. The supervisor's bookkeeping
        // entry is still present (run-task completed; reap_id removes
        // the active-map entry) — call reap_id to clean it up before
        // re-attaching to avoid a `(kind, id)` collision.
        self.owned.lock().unwrap().remove(&handle);
        self.adapters.reap_id(&exit.id).await;
        self.backoff.clear(handle.as_str());

        // Re-identify. handle_attach handles open + identity + claim.
        self.handle_attach(port).await;
    }

    async fn reap_all_owned(&self) {
        let to_reap: Vec<Owned> = {
            let mut guard = self.owned.lock().unwrap();
            let drained: Vec<Owned> = guard.drain().map(|(_, v)| v).collect();
            drained
        };
        for owned in to_reap {
            self.adapters.reap_id(&owned.adapter_id).await;
        }
    }

    // ----- Test hooks (exposed to testing::MockBus) --------------------

    /// Mirror of `handle_attach` that accepts a pre-opened device +
    /// pre-parsed identification, skipping enumeration and identity
    /// probing. Used by tests.
    #[doc(hidden)]
    pub async fn inject_attach(&self, device: Device, opened: OpenedDevice, id: Identification) {
        // Synthesize a UsbSerialPort from the Device so spawn_owned
        // can record it for re-attach on unexpected adapter exit.
        let synth_port = UsbSerialPort {
            port_name: device.handle.to_string(),
            vid: device.vid.unwrap_or(0),
            pid: device.pid.unwrap_or(0),
            product: device.product.clone(),
        };
        // Cache hint path
        if let Some(hinted) = self.cache.lookup(&id.device_id) {
            let hinted_reg = self
                .registrations
                .iter()
                .find(|r| r.name == hinted)
                .cloned();
            if let Some(reg) = hinted_reg
                && reg.score(&id).is_some()
                && reg.resource.matches_usb(
                    device.vid.unwrap_or(0),
                    device.pid.unwrap_or(0),
                )
            {
                self.spawn_owned(&reg, device, synth_port, opened, &id).await;
                return;
            }
            self.cache.invalidate(&id.device_id);
        }
        match pick_winner(&self.registrations, &device.class, &id) {
            ClaimOutcome::Claimed { index, .. } => {
                let reg = self.registrations[index].clone();
                self.spawn_owned(&reg, device, synth_port, opened, &id).await;
            }
            ClaimOutcome::Unmatched => {
                let _ = self.pulse.ingest(Event::new(DeviceUnclaimed {
                    port: device.handle.to_string(),
                    device_id: id.device_id.clone(),
                    descriptor: id.fields.clone(),
                }));
            }
        }
    }

    /// Test hook: mirror of `handle_detach`.
    #[doc(hidden)]
    pub async fn inject_detach(&self, handle: DeviceHandle) {
        self.handle_detach(handle).await;
    }
}

// ---------------------------------------------------------------------------
// USB serial port opening (real hardware path)
// ---------------------------------------------------------------------------

fn open_usb_serial(
    port: &UsbSerialPort,
    stabilization: Duration,
) -> Result<OpenedDevice, String> {
    let open = serialport::new(&port.port_name, 115_200)
        .timeout(Duration::from_millis(2500))
        .data_bits(serialport::DataBits::Eight)
        .stop_bits(serialport::StopBits::One)
        .parity(serialport::Parity::None)
        .flow_control(serialport::FlowControl::None)
        .open();

    let serial = open.map_err(|e| format!("open {}: {e}", port.port_name))?;

    // ESP devices reset on port open; give the firmware time to boot
    // and emit its HELLO frame before the identity protocol starts
    // reading.
    std::thread::sleep(stabilization);

    let device = port.to_device();
    let inner = OpenedInner::UsbSerial(Arc::new(Mutex::new(serial)));
    Ok(OpenedDevice::new(device, inner))
}
