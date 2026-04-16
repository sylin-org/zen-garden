//! [`UsbSerialDevice`] — the core entity of the USB devices domain.
//!
//! Owns the serial fd, spawns a blocking reader task that parses
//! incoming bytes into lines, and exposes:
//!
//! - [`UsbSerialDevice::send`] — synchronous write (holds a short lock).
//! - [`UsbSerialDevice::lines`] — `broadcast::Receiver<String>` of
//!   complete lines parsed by the reader.
//! - [`UsbSerialDevice::state_changes`] — `watch::Receiver<DeviceState>`
//!   for lifecycle observation.
//! - [`UsbSerialDevice::begin_evaluation`], [`accept`], [`reject`],
//!   [`dispose`] — state transitions.
//!
//! Instances are handed out as `Arc<UsbSerialDevice>`; callers hold
//! their reference permanently for the device's lifetime.

use super::state::{DeviceState, StateError};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

const LINES_BROADCAST_CAPACITY: usize = 64;
const READ_POLL_INTERVAL: Duration = Duration::from_millis(50);
const MAX_LINE_LEN: usize = 8192;
/// Sustained-EOF reads on a dangling Linux fd before self-dispose.
const MAX_ZERO_READS: u32 = 100;

// ---------------------------------------------------------------------------
// Value objects
// ---------------------------------------------------------------------------

/// Stable identity for a USB serial device. Preference at detection:
/// 1. `usb:{serial}:{vid:04x}:{pid:04x}` if the device exposes a
///    USB serial number — survives replug to any port.
/// 2. `sys:{syspath}` — stable across replug to the same port.
/// 3. `port:{name}` — last-resort fallback.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceId(String);

impl DeviceId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// OS-level port handle (`/dev/ttyUSB0`, `COM5`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PortPath(String);

impl PortPath {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PortPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// USB-level descriptor, set at attach time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbDescriptor {
    pub id: DeviceId,
    pub port: PortPath,
    pub vid: u16,
    pub pid: u16,
    pub product: Option<String>,
    pub serial_number: Option<String>,
}

// ---------------------------------------------------------------------------
// UsbSerialDevice
// ---------------------------------------------------------------------------

/// A USB serial device owned by the registry. The entity holds its
/// fd, its reader task, and its state; references flow to orchestrator,
/// wrapper entities (e.g. `Firefly`), and adapters via `Arc`.
pub struct UsbSerialDevice {
    descriptor: UsbDescriptor,
    port: Mutex<Option<Box<dyn serialport::SerialPort + Send>>>,
    state_tx: watch::Sender<DeviceState>,
    lines_tx: broadcast::Sender<String>,
    reader: Mutex<Option<JoinHandle<()>>>,
}

impl UsbSerialDevice {
    /// Open the OS port and construct an entity. Runs the blocking
    /// open + stabilization delay on a worker thread. Spawns the
    /// reader task before returning. Initial state is [`DeviceState::New`].
    pub async fn open(descriptor: UsbDescriptor, baud: u32) -> Result<Arc<Self>> {
        let port_name = descriptor.port.0.clone();
        let port = tokio::task::spawn_blocking(move || {
            serialport::new(&port_name, baud)
                .timeout(Duration::from_millis(2500))
                .data_bits(serialport::DataBits::Eight)
                .stop_bits(serialport::StopBits::One)
                .parity(serialport::Parity::None)
                .flow_control(serialport::FlowControl::None)
                .open()
                .map_err(|e| anyhow!("open {}: {e}", port_name))
        })
        .await
        .map_err(|e| anyhow!("spawn_blocking join: {e}"))??;

        // ESP devices auto-reset on open; give the firmware time to
        // boot before we start reading. Handled here so every
        // downstream reader sees a stable stream.
        tokio::time::sleep(Duration::from_millis(2500)).await;

        let (state_tx, _) = watch::channel(DeviceState::New);
        let (lines_tx, _) = broadcast::channel(LINES_BROADCAST_CAPACITY);

        let device = Arc::new(Self {
            descriptor,
            port: Mutex::new(Some(port)),
            state_tx,
            lines_tx,
            reader: Mutex::new(None),
        });

        let reader = spawn_reader(Arc::clone(&device));
        *device.reader.lock().unwrap() = Some(reader);

        Ok(device)
    }

    pub fn id(&self) -> &DeviceId {
        &self.descriptor.id
    }

    pub fn port(&self) -> &PortPath {
        &self.descriptor.port
    }

    pub fn descriptor(&self) -> &UsbDescriptor {
        &self.descriptor
    }

    pub fn state(&self) -> DeviceState {
        self.state_tx.borrow().clone()
    }

    pub fn state_changes(&self) -> watch::Receiver<DeviceState> {
        self.state_tx.subscribe()
    }

    pub fn lines(&self) -> broadcast::Receiver<String> {
        self.lines_tx.subscribe()
    }

    /// Write bytes to the device. Returns an error if the device is
    /// disposed or the write fails.
    pub fn send(&self, bytes: &[u8]) -> Result<()> {
        let mut guard = self
            .port
            .lock()
            .map_err(|_| anyhow!("port mutex poisoned"))?;
        let port = guard.as_mut().ok_or_else(|| anyhow!("device disposed"))?;
        port.write_all(bytes).map_err(|e| anyhow!("write: {e}"))?;
        port.flush().map_err(|e| anyhow!("flush: {e}"))?;
        Ok(())
    }

    /// Transition New → Evaluating. Called by the orchestrator
    /// before running identity probes.
    pub fn begin_evaluation(&self) -> std::result::Result<(), StateError> {
        self.transition(|s| s.can_begin_evaluation(), DeviceState::Evaluating, "Evaluating")
    }

    /// Transition Evaluating → Accepted(kind). Called after a
    /// successful probe.
    pub fn accept(&self, kind: impl Into<String>) -> std::result::Result<(), StateError> {
        let next = DeviceState::Accepted { kind: kind.into() };
        self.transition(|s| s.can_accept(), next, "Accepted")
    }

    /// Transition Evaluating|New → Rejected(reason). Called after a
    /// failed probe or an unmatched device.
    pub fn reject(&self, reason: impl Into<String>) -> std::result::Result<(), StateError> {
        let next = DeviceState::Rejected {
            reason: reason.into(),
        };
        self.transition(|s| s.can_reject(), next, "Rejected")
    }

    /// Transition any → Disposed. Idempotent. Closes the fd and
    /// publishes the state change; the reader task observes and exits.
    pub fn dispose(&self) {
        let mut state = self.state_tx.borrow().clone();
        if state.is_disposed() {
            return;
        }
        state = DeviceState::Disposed;
        let _ = self.state_tx.send(state);
        // Close the fd synchronously so an outstanding reader.read()
        // unblocks promptly.
        if let Ok(mut guard) = self.port.lock() {
            *guard = None;
        }
        info!(device = %self.descriptor.id, "device disposed");
    }

    fn transition<F>(
        &self,
        guard: F,
        next: DeviceState,
        label: &'static str,
    ) -> std::result::Result<(), StateError>
    where
        F: FnOnce(&DeviceState) -> bool,
    {
        let current = self.state_tx.borrow().clone();
        if !guard(&current) {
            return Err(StateError::InvalidTransition {
                from: current,
                to: label,
            });
        }
        let _ = self.state_tx.send(next);
        Ok(())
    }
}

impl Drop for UsbSerialDevice {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.reader.lock()
            && let Some(handle) = guard.take()
        {
            handle.abort();
        }
    }
}

impl fmt::Debug for UsbSerialDevice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UsbSerialDevice")
            .field("id", &self.descriptor.id)
            .field("port", &self.descriptor.port)
            .field("state", &self.state())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Reader task
// ---------------------------------------------------------------------------

fn spawn_reader(device: Arc<UsbSerialDevice>) -> JoinHandle<()> {
    tokio::task::spawn_blocking(move || reader_loop(device))
}

fn reader_loop(device: Arc<UsbSerialDevice>) {
    let mut line_buf: Vec<u8> = Vec::with_capacity(512);
    let mut scratch = [0u8; 256];
    let mut zero_reads: u32 = 0;
    let port_name = device.descriptor.port.clone();

    loop {
        if device.state().is_disposed() {
            break;
        }

        // Hold the lock only for the duration of one read attempt so
        // `send()` callers aren't starved.
        let read_outcome: ReadOutcome = {
            let mut guard = match device.port.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            match guard.as_mut() {
                None => ReadOutcome::Disposed,
                Some(port) => {
                    let _ = port.set_timeout(READ_POLL_INTERVAL);
                    match port.read(&mut scratch) {
                        Ok(0) => ReadOutcome::Zero,
                        Ok(n) => ReadOutcome::Bytes(n),
                        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => ReadOutcome::Timeout,
                        Err(e) => ReadOutcome::IoError(e.to_string()),
                    }
                }
            }
        };

        match read_outcome {
            ReadOutcome::Bytes(n) => {
                zero_reads = 0;
                line_buf.extend_from_slice(&scratch[..n]);
                drain_lines(&mut line_buf, &device.lines_tx);
                if line_buf.len() > MAX_LINE_LEN {
                    line_buf.clear();
                }
            }
            ReadOutcome::Zero => {
                zero_reads += 1;
                if zero_reads > MAX_ZERO_READS {
                    info!(port = %port_name, "sustained EOF; self-disposing");
                    device.dispose();
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            ReadOutcome::Timeout => {
                // No data yet; this is normal polling, not an error.
            }
            ReadOutcome::IoError(msg) => {
                warn!(port = %port_name, error = %msg, "reader io error; self-disposing");
                device.dispose();
                break;
            }
            ReadOutcome::Disposed => {
                break;
            }
        }
    }

    debug!(port = %port_name, "reader loop exiting");
}

enum ReadOutcome {
    Bytes(usize),
    Zero,
    Timeout,
    IoError(String),
    Disposed,
}

fn drain_lines(buf: &mut Vec<u8>, tx: &broadcast::Sender<String>) {
    while let Some(nl) = buf.iter().position(|&b| b == b'\n') {
        let raw: Vec<u8> = buf.drain(..=nl).collect();
        let body = raw.strip_suffix(b"\n").unwrap_or(&raw);
        let body = body.strip_suffix(b"\r").unwrap_or(body);
        if let Ok(s) = std::str::from_utf8(body) {
            let _ = tx.send(s.to_string());
        }
    }
}
