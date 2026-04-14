//! [`Device`] — a physical thing the bus has discovered.
//!
//! Every device the bus surfaces flows through three states:
//!
//! 1. [`Device`]: pre-identification metadata (port path, VID/PID,
//!    product string). All the enumerator can tell us without opening.
//! 2. [`OpenedDevice`]: the bus has opened the port; identity protocols
//!    read/write against it.
//! 3. A claimed adapter owns the `OpenedDevice` for the remainder of
//!    the device's attached life.
//!
//! [`DeviceHandle`] is the stable key the bus uses for lookup and
//! detach matching. For USB devices it is currently the OS port path
//! (`/dev/ttyUSB0`, `COM3`, …); stable device identity via `device_id`
//! is layered on top once identification succeeds.

use super::resource::ResourceClass;
use std::sync::{Arc, Mutex};

/// Stable key used by the bus to correlate attach / detach events and
/// adapter ownership.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeviceHandle(pub String);

impl DeviceHandle {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for DeviceHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Pre-identification metadata about a device the enumerator just
/// observed. The bus hasn't opened the port yet.
#[derive(Debug, Clone)]
pub struct Device {
    /// The key the bus will use for this device across its lifetime.
    pub handle: DeviceHandle,
    /// Which enumerator surfaced this device.
    pub class: ResourceClass,
    /// USB Vendor ID, when the class has one. `None` for non-USB.
    pub vid: Option<u16>,
    /// USB Product ID, when the class has one.
    pub pid: Option<u16>,
    /// OS product string from the USB descriptor, for telemetry.
    pub product: Option<String>,
    /// Platform-specific location hint (`/dev/ttyACM0`, `COM3`, …).
    pub location: String,
}

impl Device {
    /// Build a USB serial device record.
    pub fn usb_serial(
        handle: impl Into<String>,
        vid: u16,
        pid: u16,
        product: Option<String>,
        location: impl Into<String>,
    ) -> Self {
        Self {
            handle: DeviceHandle::new(handle),
            class: ResourceClass::UsbSerial {
                vid: Some(vid),
                pid: Some(pid),
            },
            vid: Some(vid),
            pid: Some(pid),
            product,
            location: location.into(),
        }
    }
}

/// An opened, readable/writeable handle to a device. Passed to identity
/// protocols for the probe step, then handed to the claiming adapter.
///
/// For USB serial devices the inner value is the crate's
/// `Box<dyn SerialPort>`. Wrapped in an `Arc<Mutex<_>>` so the probe
/// step and the adapter can both hold it through the claim handoff
/// without dropping the OS-level port open.
pub struct OpenedDevice {
    pub device: Device,
    inner: OpenedInner,
}

/// Internal storage for opened devices. Kept `pub(crate)` so the bus
/// modules can construct variants without exposing them publicly.
#[allow(dead_code)] // UsbSerial constructed by bus in Ch2; Mock by tests now.
pub(crate) enum OpenedInner {
    UsbSerial(Arc<Mutex<Box<dyn serialport::SerialPort>>>),
    Mock(Arc<Mutex<Vec<u8>>>),
}

impl OpenedDevice {
    #[allow(dead_code)] // Constructed by the bus in Ch2 when it opens a port.
    pub(crate) fn new(device: Device, inner: OpenedInner) -> Self {
        Self { device, inner }
    }

    /// Build a mock opened device carrying a byte buffer — used by
    /// integration tests to simulate identity-protocol I/O without
    /// touching a serial port.
    pub fn mock(device: Device, buffer: Vec<u8>) -> Self {
        Self {
            device,
            inner: OpenedInner::Mock(Arc::new(Mutex::new(buffer))),
        }
    }

    /// Access the inner USB serial port, if this is one.
    pub fn as_usb_serial(&self) -> Option<Arc<Mutex<Box<dyn serialport::SerialPort>>>> {
        match &self.inner {
            OpenedInner::UsbSerial(p) => Some(p.clone()),
            _ => None,
        }
    }

    /// Read from the mock buffer. Only valid for `OpenedDevice::mock`-
    /// constructed instances; returns `None` otherwise.
    pub fn mock_buffer(&self) -> Option<Arc<Mutex<Vec<u8>>>> {
        match &self.inner {
            OpenedInner::Mock(b) => Some(b.clone()),
            _ => None,
        }
    }
}

impl std::fmt::Debug for OpenedDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenedDevice")
            .field("device", &self.device)
            .field(
                "inner",
                &match &self.inner {
                    OpenedInner::UsbSerial(_) => "UsbSerial(<port>)",
                    OpenedInner::Mock(_) => "Mock(<buffer>)",
                },
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_handle_roundtrips_through_string() {
        let h = DeviceHandle::new("/dev/ttyUSB0");
        assert_eq!(h.as_str(), "/dev/ttyUSB0");
        assert_eq!(h.to_string(), "/dev/ttyUSB0");
    }

    #[test]
    fn mock_opened_device_exposes_buffer() {
        let device = Device::usb_serial("test", 0x1a86, 0x7523, None, "/dev/ttyUSB0");
        let opened = OpenedDevice::mock(device, b"hello".to_vec());
        let buf = opened.mock_buffer().unwrap();
        assert_eq!(buf.lock().unwrap().as_slice(), b"hello");
        assert!(opened.as_usb_serial().is_none());
    }
}
