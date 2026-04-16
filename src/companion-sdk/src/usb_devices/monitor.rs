//! [`Monitor`] — abstracts the OS-level source of USB serial
//! attach/detach notifications. The [`super::UsbRegistry`] consumes
//! a `MonitorEvent` stream from whatever `Monitor` the caller supplies.

use super::device::UsbDescriptor;
use super::DeviceId;
use async_trait::async_trait;

/// One observation from the OS.
#[derive(Debug, Clone)]
pub enum MonitorEvent {
    /// A device appeared. Carries the descriptor needed for the
    /// registry to open the port.
    Added(UsbDescriptor),
    /// A device was removed. Carries only the id — the registry has
    /// its own handle to the entity.
    Removed(DeviceId),
}

#[async_trait]
pub trait Monitor: Send + Sync {
    /// Pull the next monitor event. `None` means the stream is
    /// permanently exhausted (shutdown).
    async fn next(&mut self) -> Option<MonitorEvent>;
}
