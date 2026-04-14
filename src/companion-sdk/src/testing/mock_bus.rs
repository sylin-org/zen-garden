//! [`MockBus`] — in-memory device bus for integration tests.
//!
//! Wraps a real [`DeviceBus`] but bypasses the USB enumerator + port
//! opener + identity-protocol path. Tests inject fake `(Device,
//! Identification)` pairs via [`MockBus::attach`] and drive detach
//! via [`MockBus::detach`]. Useful for exercising the claim dance,
//! cache behaviour, and adapter spawn/reap lifecycle without real
//! hardware.
//!
//! Paired with [`super::MockTransport`] and
//! [`super::RecordingAdapter`] this closes the discovery-flow gap
//! that the Book IX integration tests deferred.

use crate::bus::{Device, DeviceBus, DeviceHandle, Identification, OpenedDevice};

/// Test-scope wrapper around a [`DeviceBus`] that supports synthetic
/// attach/detach events.
///
/// Tests typically:
///
/// 1. Build the bus with registrations.
/// 2. Wrap it in a `MockBus`.
/// 3. Call `bus.attach(fake_device, fake_identification)`.
/// 4. Yield to the runtime; the bus runs the claim dance and spawns
///    the matching adapter.
/// 5. Assert on outcomes (active adapter count, received events, etc.).
/// 6. Call `bus.detach(handle)` to simulate unplug.
///
/// There is no scan loop — tests drive events directly.
pub struct MockBus {
    inner: DeviceBus,
}

impl MockBus {
    pub fn new(bus: DeviceBus) -> Self {
        Self { inner: bus }
    }

    /// Inject an attach event. The bus runs the same claim dance it
    /// would on real hardware, minus the port open + identity probe
    /// (the caller supplies an already-opened device and
    /// identification).
    pub async fn attach(&self, device: Device, opened: OpenedDevice, id: Identification) {
        self.inner.inject_attach(device, opened, id).await;
    }

    /// Inject a detach event. Reaps the adapter owning the handle.
    pub async fn detach(&self, handle: DeviceHandle) {
        self.inner.inject_detach(handle).await;
    }

    /// Number of devices currently owned by spawned adapters.
    pub fn owned_count(&self) -> usize {
        self.inner.owned_count()
    }

    /// Expose the wrapped `DeviceBus` if a test needs to drive
    /// something not in the mock API.
    pub fn inner(&self) -> &DeviceBus {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{Adapter, AdapterInfo, AdapterProfile, Adapters, adapter::BoxFuture};
    use crate::bus::{AdapterRegistration, Device, Predicate, ResourceClass};
    use crate::garden::{Event, Pulse};
    use crate::moss_client::MossLocalClient;
    use serde_json::json;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    struct FlagAdapter {
        id: String,
        started: Arc<AtomicBool>,
    }
    impl Adapter for FlagAdapter {
        fn info(&self) -> AdapterInfo {
            AdapterInfo {
                kind: "test.flag",
                id: self.id.clone(),
                device: None,
            }
        }
        fn profile(&self) -> AdapterProfile {
            AdapterProfile::default()
        }
        fn run(
            self: Box<Self>,
            mut events: mpsc::Receiver<Event>,
            _g: Arc<MossLocalClient>,
            _p: Arc<Pulse>,
            shutdown: CancellationToken,
        ) -> BoxFuture<'static, ()> {
            let flag = self.started.clone();
            Box::pin(async move {
                flag.store(true, Ordering::Relaxed);
                loop {
                    tokio::select! {
                        maybe = events.recv() => { if maybe.is_none() { break; } }
                        _ = shutdown.cancelled() => break,
                    }
                }
            })
        }
    }

    fn sample_registration(flag: Arc<AtomicBool>) -> AdapterRegistration {
        AdapterRegistration::new(
            "test.flag",
            ResourceClass::UsbSerial {
                vid: Some(0x1a86),
                pid: None,
            },
            Predicate::eq("family", "firefly"),
            move |_opened, id| {
                Box::new(FlagAdapter {
                    id: id.device_id.clone(),
                    started: flag.clone(),
                })
            },
        )
    }

    fn sample_device(handle: &str) -> Device {
        Device::usb_serial(handle, 0x1a86, 0x55d4, None, handle)
    }

    fn sample_id(device_id: &str) -> Identification {
        Identification::from_json(
            "firefly",
            json!({
                "device_id": device_id,
                "family": "firefly",
                "variant": "tdisplay",
            }),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn attach_spawns_owned_adapter() {
        let pulse = Arc::new(Pulse::with_defaults());
        let moss = Arc::new(crate::moss_client::MossLocalClient::new("http://127.0.0.1:0"));
        let adapters = Arc::new(Adapters::new(moss, pulse.clone()));

        let flag = Arc::new(AtomicBool::new(false));
        let bus = DeviceBus::builder()
            .with_registration(sample_registration(flag.clone()))
            .build(adapters.clone(), pulse.clone());
        let mock = MockBus::new(bus);

        let device = sample_device("/dev/ttyACM0");
        let opened = OpenedDevice::mock(device.clone(), b"ignored".to_vec());
        mock.attach(device, opened, sample_id("dev-1")).await;

        // Allow the filter task + run task to start.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        assert_eq!(mock.owned_count(), 1);
        assert!(flag.load(Ordering::Relaxed), "adapter.run never observed");
        assert_eq!(adapters.active_count(), 1);
    }

    #[tokio::test]
    async fn detach_reaps_adapter() {
        let pulse = Arc::new(Pulse::with_defaults());
        let moss = Arc::new(crate::moss_client::MossLocalClient::new("http://127.0.0.1:0"));
        let adapters = Arc::new(Adapters::new(moss, pulse.clone()));

        let flag = Arc::new(AtomicBool::new(false));
        let bus = DeviceBus::builder()
            .with_registration(sample_registration(flag))
            .build(adapters.clone(), pulse.clone());
        let mock = MockBus::new(bus);

        let device = sample_device("/dev/ttyACM0");
        let opened = OpenedDevice::mock(device.clone(), b"ignored".to_vec());
        mock.attach(device.clone(), opened, sample_id("dev-2")).await;
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert_eq!(mock.owned_count(), 1);

        mock.detach(device.handle).await;
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert_eq!(mock.owned_count(), 0);
        assert_eq!(adapters.active_count(), 0);
    }

    #[tokio::test]
    async fn unmatched_descriptor_emits_unclaimed_telemetry() {
        let pulse = Arc::new(Pulse::with_defaults());
        // Telemetry events share the `core` namespace with presence events.
        pulse.register_namespace("core");
        let moss = Arc::new(crate::moss_client::MossLocalClient::new("http://127.0.0.1:0"));
        let adapters = Arc::new(Adapters::new(moss, pulse.clone()));

        let mut rx = pulse.subscribe();

        let bus = DeviceBus::builder()
            .with_registration(AdapterRegistration::new(
                "picky",
                ResourceClass::UsbSerial {
                    vid: Some(0x1a86),
                    pid: None,
                },
                // Requires a variant the test device won't advertise.
                Predicate::eq("variant", "nonexistent"),
                |_, _| {
                    panic!("predicate should not match");
                },
            ))
            .build(adapters.clone(), pulse.clone());
        let mock = MockBus::new(bus);

        let device = sample_device("/dev/ttyACM0");
        let opened = OpenedDevice::mock(device.clone(), b"ignored".to_vec());
        mock.attach(device, opened, sample_id("dev-3")).await;

        // Wait for the telemetry event to fan out through pulse.
        let mut observed = false;
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(200);
        while std::time::Instant::now() < deadline {
            match tokio::time::timeout(std::time::Duration::from_millis(20), rx.recv()).await {
                Ok(Ok(event)) if event.kind == "core.companion.device.unclaimed" => {
                    observed = true;
                    break;
                }
                _ => {}
            }
        }
        assert!(observed, "DeviceUnclaimed telemetry not observed");
        assert_eq!(mock.owned_count(), 0);
    }
}
