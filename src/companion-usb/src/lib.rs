//! USB serial device discovery and lifecycle.
//!
//! This module answers one question: *what USB serial devices does
//! the OS report, and what state is each one in?* It knows nothing
//! about firefly, identity probes, or adapters. Its output is a
//! registry that publishes `RegistryEvent`s carrying `Arc<UsbSerialDevice>`
//! instances — each one carries its own fd, reader task, and state
//! machine.
//!
//! See [COMPANION-0018].
//!
//! [COMPANION-0018]: https://github.com/zen-garden/zen-garden/blob/dev/docs/decisions/COMPANION-0018-three-domain-device-architecture.md

mod device;
mod monitor;
mod poll_monitor;
mod registry;
mod state;
#[cfg(target_os = "linux")]
mod udev_monitor;

pub use device::{DeviceId, PortPath, UsbDescriptor, UsbSerialDevice};
pub use monitor::{Monitor, MonitorEvent};
pub use poll_monitor::PollMonitor;
pub use registry::{RegistryEvent, UsbRegistry};
pub use state::{DeviceState, StateError};
#[cfg(target_os = "linux")]
pub use udev_monitor::UdevMonitor;
