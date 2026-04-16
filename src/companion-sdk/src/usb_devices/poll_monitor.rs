//! Portable polling [`Monitor`] — fallback for platforms without a
//! native kernel-event source. Diffs successive snapshots of
//! `serialport::available_ports()` and yields the resulting events.
//!
//! On Linux, prefer [`super::UdevMonitor`] — udev reports events in
//! milliseconds; polling is bounded by the scan interval.

use super::device::{DeviceId, PortPath, UsbDescriptor};
use super::monitor::{Monitor, MonitorEvent};
use async_trait::async_trait;
use std::collections::{HashMap, VecDeque};
use std::time::Duration;

const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(2);

pub struct PollMonitor {
    known: HashMap<DeviceId, PortPath>,
    pending: VecDeque<MonitorEvent>,
    interval: Duration,
    first_scan: bool,
}

impl PollMonitor {
    pub fn new() -> Self {
        Self::with_interval(DEFAULT_POLL_INTERVAL)
    }

    pub fn with_interval(interval: Duration) -> Self {
        Self {
            known: HashMap::new(),
            pending: VecDeque::new(),
            interval,
            first_scan: true,
        }
    }

    fn refresh(&mut self) {
        let current = snapshot();
        let current_ids: std::collections::HashSet<&DeviceId> = current.keys().collect();
        let detached_ids: Vec<DeviceId> = self
            .known
            .keys()
            .filter(|id| !current_ids.contains(id))
            .cloned()
            .collect();
        for id in detached_ids {
            self.known.remove(&id);
            self.pending.push_back(MonitorEvent::Removed(id));
        }
        for (id, desc) in &current {
            if !self.known.contains_key(id) {
                self.known.insert(id.clone(), desc.port.clone());
                self.pending.push_back(MonitorEvent::Added(desc.clone()));
            }
        }
    }
}

impl Default for PollMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Monitor for PollMonitor {
    async fn next(&mut self) -> Option<MonitorEvent> {
        loop {
            if let Some(event) = self.pending.pop_front() {
                return Some(event);
            }
            if self.first_scan {
                self.first_scan = false;
            } else {
                tokio::time::sleep(self.interval).await;
            }
            self.refresh();
        }
    }
}

fn snapshot() -> HashMap<DeviceId, UsbDescriptor> {
    let mut map = HashMap::new();
    let ports = match serialport::available_ports() {
        Ok(p) => p,
        Err(_) => return map,
    };
    for info in ports {
        let serialport::SerialPortType::UsbPort(usb) = &info.port_type else {
            continue;
        };
        let port = PortPath::new(info.port_name.clone());
        let serial_number = usb
            .serial_number
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let id = match &serial_number {
            Some(s) => DeviceId::new(format!("usb:{}:{:04x}:{:04x}", s, usb.vid, usb.pid)),
            None => DeviceId::new(format!("port:{}", info.port_name)),
        };
        map.insert(
            id.clone(),
            UsbDescriptor {
                id,
                port,
                vid: usb.vid,
                pid: usb.pid,
                product: usb.product.clone(),
                serial_number,
            },
        );
    }
    map
}
