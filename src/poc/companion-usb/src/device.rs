//! [`UsbSerialDevice`] — owns its port via a single driver task.
//!
//! One serial port, one thread, one work queue. Writes enter the
//! queue and are executed by the driver; reads happen in the same
//! driver between queue drains and are broadcast as lines. No
//! mutex, no split fds, no contention. Callers hand in a write +
//! optional ack receiver; the driver does the work and replies.
//!
//! The entity exposes:
//!
//! - [`UsbSerialDevice::send`] — async, enqueues a write and awaits
//!   the driver's ack.
//! - [`UsbSerialDevice::lines`] — `broadcast::Receiver<String>` of
//!   complete lines the driver has read.
//! - [`UsbSerialDevice::state_changes`] — `watch::Receiver<DeviceState>`.
//! - [`UsbSerialDevice::begin_evaluation`], [`accept`], [`reject`],
//!   [`dispose`] — state transitions.

use super::state::{DeviceState, StateError};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::io::{Read, Write};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

const LINES_BROADCAST_CAPACITY: usize = 64;
const READ_POLL_INTERVAL: Duration = Duration::from_millis(20);
const MAX_LINE_LEN: usize = 8192;
/// Sustained-EOF reads on a dangling Linux fd before self-dispose.
const MAX_ZERO_READS: u32 = 100;
/// Driver queue depth. Commands beyond this await asynchronously
/// (backpressure). Small so we feel it in tests if the driver stalls.
const QUEUE_DEPTH: usize = 64;

// ---------------------------------------------------------------------------
// Value objects
// ---------------------------------------------------------------------------

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
// Driver command
// ---------------------------------------------------------------------------

/// One unit of work for the driver task.
enum DriverCommand {
    /// Write bytes to the port, then flush. Ack fires with the result.
    Write {
        bytes: Vec<u8>,
        ack: oneshot::Sender<Result<()>>,
    },
}

/// The driver's scoped dependency surface — the channels and port it
/// owns. Passed by value at spawn; never shared with the aggregate
/// outside of the `state` and `lines` clones.
struct DriverDeps {
    port_path: PortPath,
    port: Box<dyn serialport::SerialPort + Send>,
    queue: mpsc::Receiver<DriverCommand>,
    state: Arc<watch::Sender<DeviceState>>,
    lines: broadcast::Sender<String>,
}

// ---------------------------------------------------------------------------
// UsbSerialDevice
// ---------------------------------------------------------------------------

pub struct UsbSerialDevice {
    descriptor: UsbDescriptor,
    queue: mpsc::Sender<DriverCommand>,
    state: Arc<watch::Sender<DeviceState>>,
    lines: broadcast::Sender<String>,
    driver: JoinHandle<()>,
}

impl UsbSerialDevice {
    /// Open the OS port and construct an entity. Spawns the driver
    /// task before returning. Initial state is [`DeviceState::New`].
    pub async fn open(descriptor: UsbDescriptor, baud: u32) -> Result<Arc<Self>> {
        let port_name = descriptor.port.0.clone();
        let port = tokio::task::spawn_blocking(move || {
            serialport::new(&port_name, baud)
                .timeout(READ_POLL_INTERVAL)
                .data_bits(serialport::DataBits::Eight)
                .stop_bits(serialport::StopBits::One)
                .parity(serialport::Parity::None)
                .flow_control(serialport::FlowControl::None)
                .open()
                .map_err(|e| anyhow!("open {}: {e}", port_name))
        })
        .await
        .map_err(|e| anyhow!("spawn_blocking join: {e}"))??;

        // ESP devices auto-reset on open; wait for boot before the
        // driver starts reading.
        tokio::time::sleep(Duration::from_millis(2500)).await;

        let state = Arc::new(watch::channel(DeviceState::New).0);
        let (lines, _) = broadcast::channel(LINES_BROADCAST_CAPACITY);
        let (queue, queue_rx) = mpsc::channel(QUEUE_DEPTH);

        let driver = spawn_driver(DriverDeps {
            port_path: descriptor.port.clone(),
            port,
            queue: queue_rx,
            state: Arc::clone(&state),
            lines: lines.clone(),
        });

        Ok(Arc::new(Self {
            descriptor,
            queue,
            state,
            lines,
            driver,
        }))
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
        self.state.borrow().clone()
    }

    pub fn state_changes(&self) -> watch::Receiver<DeviceState> {
        self.state.subscribe()
    }

    pub fn lines(&self) -> broadcast::Receiver<String> {
        self.lines.subscribe()
    }

    /// Enqueue a write; await the driver's ack.
    pub async fn send(&self, bytes: &[u8]) -> Result<()> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.queue
            .send(DriverCommand::Write {
                bytes: bytes.to_vec(),
                ack: ack_tx,
            })
            .await
            .map_err(|_| anyhow!("driver queue closed (device disposed)"))?;
        ack_rx
            .await
            .map_err(|_| anyhow!("driver dropped write without ack"))?
    }

    pub fn begin_evaluation(&self) -> std::result::Result<(), StateError> {
        self.transition(
            |s| s.can_begin_evaluation(),
            DeviceState::Evaluating,
            "Evaluating",
        )
    }

    pub fn accept(&self, kind: impl Into<String>) -> std::result::Result<(), StateError> {
        let next = DeviceState::Accepted { kind: kind.into() };
        self.transition(|s| s.can_accept(), next, "Accepted")
    }

    pub fn reject(&self, reason: impl Into<String>) -> std::result::Result<(), StateError> {
        let next = DeviceState::Rejected {
            reason: reason.into(),
        };
        self.transition(|s| s.can_reject(), next, "Rejected")
    }

    /// Transition to Disposed. Idempotent. The driver task observes
    /// this on its next loop iteration and exits, closing the port.
    pub fn dispose(&self) {
        if self.state.borrow().is_disposed() {
            return;
        }
        self.state.send_replace(DeviceState::Disposed);
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
        let current = self.state.borrow().clone();
        if !guard(&current) {
            return Err(StateError::InvalidTransition {
                from: current,
                to: label,
            });
        }
        self.state.send_replace(next);
        Ok(())
    }
}

impl Drop for UsbSerialDevice {
    fn drop(&mut self) {
        self.driver.abort();
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
// Driver task — single owner of the port
// ---------------------------------------------------------------------------

fn spawn_driver(deps: DriverDeps) -> JoinHandle<()> {
    tokio::task::spawn_blocking(move || driver_loop(deps))
}

fn driver_loop(deps: DriverDeps) {
    let DriverDeps {
        port_path,
        mut port,
        mut queue,
        state,
        lines,
    } = deps;
    let mut line_buf: Vec<u8> = Vec::with_capacity(512);
    let mut scratch = [0u8; 256];
    let mut zero_reads: u32 = 0;

    loop {
        if state.borrow().is_disposed() {
            break;
        }

        // Drain every queued write before reading. Writes ack as they
        // complete, so callers block no longer than one read-poll
        // interval plus their own write time.
        loop {
            match queue.try_recv() {
                Ok(DriverCommand::Write { bytes, ack }) => {
                    let result = (|| {
                        port.write_all(&bytes)?;
                        port.flush()
                    })()
                    .map_err(|e| anyhow!("{e}"));
                    if let Err(ref e) = result {
                        warn!(port = %port_path, error = %e, "driver write failed");
                    }
                    if let Err(unsent) = ack.send(result) {
                        debug!(
                            port = %port_path,
                            write = if unsent.is_ok() { "succeeded" } else { "failed" },
                            "ack receiver dropped before driver completed write"
                        );
                    }
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    // All senders dropped — device is being torn down.
                    state.send_replace(DeviceState::Disposed);
                    return;
                }
            }
        }

        // Poll for bytes. The port is opened with `timeout(READ_POLL_INTERVAL)`,
        // so `port.read()` paces the loop itself: ~20ms on quiet lines
        // (returns TimedOut), immediate on data or EOF.
        match port.read(&mut scratch) {
            Ok(0) => {
                // Dangling Linux fd — dispose-fast signal, not a pacing hint.
                zero_reads += 1;
                if zero_reads > MAX_ZERO_READS {
                    info!(port = %port_path, "sustained EOF; self-disposing");
                    state.send_replace(DeviceState::Disposed);
                    break;
                }
            }
            Ok(n) => {
                zero_reads = 0;
                line_buf.extend_from_slice(&scratch[..n]);
                drain_lines(&mut line_buf, &lines);
                if line_buf.len() > MAX_LINE_LEN {
                    line_buf.clear();
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                // Polling heartbeat.
            }
            Err(e) => {
                warn!(port = %port_path, error = %e, "driver read failed; self-disposing");
                state.send_replace(DeviceState::Disposed);
                break;
            }
        }
    }

    debug!(port = %port_path, "driver loop exiting");
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
