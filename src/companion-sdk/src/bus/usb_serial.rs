//! USB serial port enumerator.
//!
//! Wraps `serialport::available_ports()` with a delta tracker: every
//! call to [`UsbSerialEnumerator::scan`] returns the set of ports
//! that appeared or disappeared since the previous scan. The bus
//! drives this on its discovery interval and converts deltas into
//! `Attached` / `Detached` events for downstream consumers.
//!
//! The enumerator owns `Mutex<HashMap<port_name, UsbSerialPort>>` —
//! scans are cheap (one `available_ports()` call + set diff) and
//! safe to call from the bus's periodic task.

use super::device::{Device, DeviceHandle};
use super::resource::ResourceClass;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

/// Cached metadata about a USB serial port the enumerator has observed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbSerialPort {
    pub port_name: String,
    pub vid: u16,
    pub pid: u16,
    pub product: Option<String>,
}

impl UsbSerialPort {
    pub fn to_device(&self) -> Device {
        Device::usb_serial(
            self.port_name.clone(),
            self.vid,
            self.pid,
            self.product.clone(),
            self.port_name.clone(),
        )
    }
}

/// Delta between two scans of the system's USB serial ports.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ScanDelta {
    /// Ports that appeared since the previous scan.
    pub attached: Vec<UsbSerialPort>,
    /// Handles of ports that disappeared since the previous scan.
    pub detached: Vec<DeviceHandle>,
}

/// Tracks USB serial ports across successive scans, returning the set
/// of deltas on each tick.
pub struct UsbSerialEnumerator {
    /// Optional class filter applied before surfacing attached devices.
    /// Unmatched ports are still tracked internally (so detach works),
    /// but not emitted in `attached`.
    filter: ResourceClass,
    /// Last-observed port set, keyed by port name.
    known: Mutex<HashMap<String, UsbSerialPort>>,
}

impl UsbSerialEnumerator {
    /// New enumerator with the given class filter.
    pub fn new(filter: ResourceClass) -> Self {
        Self {
            filter,
            known: Mutex::new(HashMap::new()),
        }
    }

    /// Default: an unfiltered USB serial enumerator.
    pub fn unfiltered() -> Self {
        Self::new(ResourceClass::UsbSerial {
            vid: None,
            pid: None,
        })
    }

    /// Run one scan tick, returning the delta vs the previous call.
    pub fn scan(&self) -> anyhow::Result<ScanDelta> {
        let ports = serialport::available_ports()
            .map_err(|e| anyhow::anyhow!("serialport enumeration failed: {e}"))?;
        let current = extract_usb_ports(&ports);
        Ok(self.apply_current(current))
    }

    /// Feed a pre-computed port list in — used by tests to exercise
    /// delta logic without depending on real hardware.
    pub fn apply_current(&self, current: Vec<UsbSerialPort>) -> ScanDelta {
        let current_by_name: HashMap<String, UsbSerialPort> = current
            .into_iter()
            .map(|p| (p.port_name.clone(), p))
            .collect();

        let mut known = self.known.lock().unwrap();

        let current_keys: HashSet<&String> = current_by_name.keys().collect();
        let known_keys: HashSet<&String> = known.keys().collect();

        // Newly-appeared ports that pass the class filter.
        let attached: Vec<UsbSerialPort> = current_keys
            .difference(&known_keys)
            .filter_map(|name| current_by_name.get(*name).cloned())
            .filter(|p| self.filter.matches_usb(p.vid, p.pid))
            .collect();

        // Ports that vanished since last scan, regardless of filter
        // (we previously surfaced them, so emit detach on exit).
        let detached: Vec<DeviceHandle> = known_keys
            .difference(&current_keys)
            .map(|name| DeviceHandle::new((*name).clone()))
            .collect();

        *known = current_by_name;

        ScanDelta { attached, detached }
    }

    /// Number of ports currently tracked (post-filter or not; reflects
    /// the underlying known set).
    pub fn tracked_count(&self) -> usize {
        self.known.lock().unwrap().len()
    }

    /// Snapshot of currently-tracked ports that pass the class filter.
    /// Used by the bus to retry unowned ports on subsequent ticks —
    /// an attach delta fires only once per (port, scan) transition, so
    /// a port whose initial identity probe fails would otherwise be
    /// abandoned. Backoff-gated retry is the bus's responsibility;
    /// this method just exposes the candidate set.
    pub fn tracked_ports(&self) -> Vec<UsbSerialPort> {
        let known = self.known.lock().unwrap();
        known
            .values()
            .filter(|p| self.filter.matches_usb(p.vid, p.pid))
            .cloned()
            .collect()
    }
}

fn extract_usb_ports(ports: &[serialport::SerialPortInfo]) -> Vec<UsbSerialPort> {
    ports
        .iter()
        .filter_map(|info| match &info.port_type {
            serialport::SerialPortType::UsbPort(usb) => Some(UsbSerialPort {
                port_name: info.port_name.clone(),
                vid: usb.vid,
                pid: usb.pid,
                product: usb.product.clone(),
            }),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn port(name: &str, vid: u16, pid: u16) -> UsbSerialPort {
        UsbSerialPort {
            port_name: name.to_string(),
            vid,
            pid,
            product: None,
        }
    }

    #[test]
    fn first_scan_reports_all_as_attached() {
        let e = UsbSerialEnumerator::unfiltered();
        let delta = e.apply_current(vec![port("/dev/ttyUSB0", 0x1a86, 0x7523)]);
        assert_eq!(delta.attached.len(), 1);
        assert_eq!(delta.attached[0].port_name, "/dev/ttyUSB0");
        assert!(delta.detached.is_empty());
    }

    #[test]
    fn stable_set_reports_no_deltas() {
        let e = UsbSerialEnumerator::unfiltered();
        let initial = vec![port("/dev/ttyUSB0", 0x1a86, 0x7523)];
        e.apply_current(initial.clone());
        let delta = e.apply_current(initial);
        assert!(delta.attached.is_empty());
        assert!(delta.detached.is_empty());
    }

    #[test]
    fn removed_port_surfaces_as_detached() {
        let e = UsbSerialEnumerator::unfiltered();
        e.apply_current(vec![
            port("/dev/ttyUSB0", 0x1a86, 0x7523),
            port("/dev/ttyUSB1", 0x2e8a, 0x000a),
        ]);
        let delta = e.apply_current(vec![port("/dev/ttyUSB1", 0x2e8a, 0x000a)]);
        assert!(delta.attached.is_empty());
        assert_eq!(delta.detached.len(), 1);
        assert_eq!(delta.detached[0].as_str(), "/dev/ttyUSB0");
    }

    #[test]
    fn new_port_surfaces_as_attached() {
        let e = UsbSerialEnumerator::unfiltered();
        e.apply_current(vec![port("/dev/ttyUSB0", 0x1a86, 0x7523)]);
        let delta = e.apply_current(vec![
            port("/dev/ttyUSB0", 0x1a86, 0x7523),
            port("/dev/ttyACM0", 0x1a86, 0x55d4),
        ]);
        assert_eq!(delta.attached.len(), 1);
        assert_eq!(delta.attached[0].port_name, "/dev/ttyACM0");
        assert!(delta.detached.is_empty());
    }

    #[test]
    fn filter_excludes_non_matching_vid() {
        let e = UsbSerialEnumerator::new(ResourceClass::UsbSerial {
            vid: Some(0x1a86),
            pid: None,
        });
        let delta = e.apply_current(vec![
            port("/dev/ttyUSB0", 0x1a86, 0x7523),
            port("/dev/ttyUSB1", 0x2e8a, 0x000a),
        ]);
        assert_eq!(delta.attached.len(), 1);
        assert_eq!(delta.attached[0].vid, 0x1a86);
    }

    #[test]
    fn detach_fires_even_for_filtered_out_ports() {
        // A port that didn't match the filter still vanishes; we record
        // it internally so we can emit the detach cleanly. Per-port
        // filtering happens on emit, not on tracking.
        let e = UsbSerialEnumerator::new(ResourceClass::UsbSerial {
            vid: Some(0x1a86),
            pid: None,
        });
        e.apply_current(vec![
            port("/dev/ttyUSB0", 0x1a86, 0x7523),
            port("/dev/ttyUSB1", 0x2e8a, 0x000a),
        ]);
        let delta = e.apply_current(vec![]);
        // Both ports vanish; detached covers both even though only one
        // was surfaced as attached earlier.
        assert_eq!(delta.detached.len(), 2);
    }

    #[test]
    fn round_trip_device_metadata() {
        let p = UsbSerialPort {
            port_name: "/dev/ttyACM0".into(),
            vid: 0x1a86,
            pid: 0x55d4,
            product: Some("USB Single Serial".into()),
        };
        let d = p.to_device();
        assert_eq!(d.handle.as_str(), "/dev/ttyACM0");
        assert_eq!(d.vid, Some(0x1a86));
        assert_eq!(d.pid, Some(0x55d4));
        assert_eq!(d.product.as_deref(), Some("USB Single Serial"));
    }
}
