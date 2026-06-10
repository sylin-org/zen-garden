//! Linux [`Monitor`] backed by udev/netlink via the `udev` crate.
//!
//! A dedicated blocking thread owns the non-`Send` udev handles and
//! blocks on the monitor fd with `mio::Poll`. Events are forwarded
//! through an unbounded mpsc; the async `next()` receives from it.
//!
//! Initial enumeration emits `Added` for every currently-plugged
//! device so the registry sees boot-time devices without waiting for
//! a subsequent plug cycle.

use super::device::{DeviceId, PortPath, UsbDescriptor};
use super::monitor::{Monitor, MonitorEvent};
use async_trait::async_trait;
use mio::unix::SourceFd;
use mio::{Events, Interest, Poll, Token};
use std::collections::HashMap;
use std::os::fd::AsRawFd;
use tokio::sync::mpsc;
use udev::{Enumerator, EventType, MonitorBuilder, MonitorSocket};

const SUBSYSTEM: &str = "tty";
const MONITOR_TOKEN: Token = Token(0);

pub struct UdevMonitor {
    rx: mpsc::UnboundedReceiver<MonitorEvent>,
    _reader: tokio::task::JoinHandle<()>,
}

impl UdevMonitor {
    pub fn new() -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::unbounded_channel();
        // `MonitorSocket`, `Enumerator`, and `Device` wrap raw C
        // pointers. Construct them inside the blocking worker so they
        // never cross a thread boundary.
        let reader = tokio::task::spawn_blocking(move || {
            if let Err(e) = run(tx) {
                tracing::error!(error = %e, "udev monitor failed");
            }
        });
        Ok(Self { rx, _reader: reader })
    }
}

impl Drop for UdevMonitor {
    fn drop(&mut self) {
        self._reader.abort();
    }
}

#[async_trait]
impl Monitor for UdevMonitor {
    async fn next(&mut self) -> Option<MonitorEvent> {
        self.rx.recv().await
    }
}

fn run(tx: mpsc::UnboundedSender<MonitorEvent>) -> anyhow::Result<()> {
    let monitor: MonitorSocket = MonitorBuilder::new()?
        .match_subsystem(SUBSYSTEM)?
        .listen()?;

    // syspath → id, used to resolve the id on Remove events (the
    // detached device no longer has queryable attributes).
    let mut by_syspath: HashMap<String, DeviceId> = HashMap::new();

    // Initial snapshot.
    let mut enumerator = Enumerator::new()?;
    enumerator.match_subsystem(SUBSYSTEM)?;
    for device in enumerator.scan_devices()? {
        if let Some(desc) = descriptor_from_udev(&device) {
            by_syspath.insert(
                device.syspath().to_string_lossy().into_owned(),
                desc.id.clone(),
            );
            if tx.send(MonitorEvent::Added(desc)).is_err() {
                return Ok(());
            }
        }
    }

    let mut poll = Poll::new()?;
    let fd = monitor.as_raw_fd();
    poll.registry()
        .register(&mut SourceFd(&fd), MONITOR_TOKEN, Interest::READABLE)?;
    let mut events = Events::with_capacity(16);

    loop {
        if tx.is_closed() {
            return Ok(());
        }
        if let Err(e) = poll.poll(&mut events, None) {
            if e.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(e.into());
        }
        for event in monitor.iter() {
            let syspath = event.syspath().to_string_lossy().into_owned();
            match event.event_type() {
                EventType::Add => {
                    if let Some(desc) = descriptor_from_udev(&event) {
                        by_syspath.insert(syspath, desc.id.clone());
                        if tx.send(MonitorEvent::Added(desc)).is_err() {
                            return Ok(());
                        }
                    }
                }
                EventType::Remove => {
                    if let Some(id) = by_syspath.remove(&syspath)
                        && tx.send(MonitorEvent::Removed(id)).is_err()
                    {
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
    }
}

fn descriptor_from_udev(device: &udev::Device) -> Option<UsbDescriptor> {
    let port = device.property_value("DEVNAME")?.to_str()?.to_string();
    if device.property_value("ID_BUS").and_then(|v| v.to_str()) != Some("usb") {
        return None;
    }
    let vid = parse_hex_u16(device.property_value("ID_VENDOR_ID")?.to_str()?)?;
    let pid = parse_hex_u16(device.property_value("ID_MODEL_ID")?.to_str()?)?;
    let product = device
        .property_value("ID_MODEL")
        .and_then(|v| v.to_str())
        .map(|s| s.to_string());
    let serial_number = device
        .property_value("ID_SERIAL_SHORT")
        .and_then(|v| v.to_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let id = match &serial_number {
        Some(s) => DeviceId::new(format!("usb:{}:{:04x}:{:04x}", s, vid, pid)),
        None => {
            let syspath = device.syspath().to_string_lossy();
            DeviceId::new(format!("sys:{}", syspath))
        }
    };

    Some(UsbDescriptor {
        id,
        port: PortPath::new(port),
        vid,
        pid,
        product,
        serial_number,
    })
}

fn parse_hex_u16(s: &str) -> Option<u16> {
    u16::from_str_radix(s.trim_start_matches("0x"), 16).ok()
}
