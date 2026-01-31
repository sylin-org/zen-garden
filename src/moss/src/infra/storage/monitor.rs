//! USB storage device monitoring via udev
//!
//! Linux-only module that uses udev to detect USB storage device events.
//! Emits StorageEvents via EventBus when eligible devices are detected.

use anyhow::{Context, Result};
use garden_common::storage::StorageDetectedInfo;
use tracing::{debug, error, info, warn};

use crate::domain::StorageEvent;
use crate::infra::EventBus;
use super::analyze_device;

/// Monitors USB storage devices using udev
pub struct StorageMonitor {
    /// Event bus for domain events
    event_bus: EventBus,
}

impl StorageMonitor {
    /// Create a new storage monitor
    pub fn new(event_bus: EventBus) -> Self {
        Self { event_bus }
    }

    /// Start monitoring for USB storage devices
    ///
    /// This runs in a blocking thread since udev is synchronous.
    /// Spawns a tokio task internally.
    pub fn start(&self) -> Result<()> {
        let event_bus = self.event_bus.clone();

        // Spawn blocking task for udev monitoring
        std::thread::spawn(move || {
            if let Err(e) = run_udev_monitor(event_bus) {
                error!("udev monitor failed: {}", e);
            }
        });

        info!("Storage monitor started");
        Ok(())
    }

    /// Scan for currently attached eligible devices
    pub async fn scan_existing(&self) -> Result<Vec<StorageDetectedInfo>> {
        scan_existing_devices().await
    }
}

/// Run the udev monitor loop (blocking, runs in dedicated thread)
fn run_udev_monitor(event_bus: EventBus) -> Result<()> {
    // Create udev context
    let mut enumerator = udev::Enumerator::new()
        .context("Failed to create udev enumerator")?;

    // Filter for block devices only
    enumerator.match_subsystem("block")
        .context("Failed to set udev subsystem filter")?;

    // Create monitor socket
    let socket = udev::MonitorBuilder::new()
        .context("Failed to create udev monitor")?
        .match_subsystem("block")
        .context("Failed to set monitor subsystem filter")?
        .listen()
        .context("Failed to start udev monitor")?;

    info!("udev monitor listening for block device events");

    // Poll for events
    loop {
        // Use poll to wait for events with timeout
        let mut pollfd = libc::pollfd {
            fd: socket.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };

        // Poll with 5 second timeout
        let ret = unsafe { libc::poll(&mut pollfd, 1, 5000) };

        if ret < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(err).context("poll failed");
        }

        if ret == 0 {
            // Timeout, continue polling
            continue;
        }

        // Process available events
        while let Some(event) = socket.iter().next() {
            let event_type = event.event_type();
            let devnode = match event.devnode() {
                Some(node) => node.to_string_lossy().to_string(),
                None => continue,
            };

            match event_type {
                udev::EventType::Add => {
                    debug!("Block device added: {}", devnode);

                    // Analyze the device (handles both partitions like /dev/sdb1
                    // and whole-disk devices like /dev/sdd for manifest-first discovery)
                    match analyze_device(&devnode) {
                        Ok(info) => {
                            // Emit events for both:
                            // 1. Devices eligible for preparation (empty, unformatted, etc.)
                            // 2. Already-prepared seed banks (state = Prepared)
                            let is_prepared = info.state == garden_common::storage::DeviceState::Prepared;

                            if info.eligible || is_prepared {
                                info!(
                                    device = %devnode,
                                    label = ?info.label,
                                    capacity = info.capacity_bytes,
                                    state = ?info.state,
                                    prepared = is_prepared,
                                    "USB storage detected"
                                );

                                // Print TTY ribbon
                                if let Err(e) = garden_common::console::print_storage_detected_ribbon(&info) {
                                    warn!("Failed to print TTY ribbon: {}", e);
                                }

                                // Emit StorageEvent via EventBus
                                let storage_event = StorageEvent::seed_bank_detected(
                                    info.label.as_deref().unwrap_or(&info.device),
                                    &info.device,
                                    info.mount_path.as_deref().unwrap_or(""),
                                    info.capacity_bytes / (1024 * 1024 * 1024), // Convert to GB
                                );
                                event_bus.emit(storage_event);
                            } else {
                                debug!(
                                    device = %devnode,
                                    state = ?info.state,
                                    reason = ?info.ineligible_reason,
                                    "Device not eligible for seed bank"
                                );
                            }
                        }
                        Err(e) => {
                            warn!("Failed to analyze device {}: {}", devnode, e);
                        }
                    }
                }
                udev::EventType::Remove => {
                    debug!("Block device removed: {}", devnode);

                    // Mark seed bank offline in registry if this was a registered device
                    if let Err(e) = handle_device_removal(&devnode) {
                        warn!("Failed to handle device removal for {}: {}", devnode, e);
                    }

                    // Emit removal event via EventBus
                    let storage_event = StorageEvent::seed_bank_removed(&devnode, &devnode);
                    event_bus.emit(storage_event);
                }
                _ => {
                    // Change events, etc. - ignore for now
                }
            }
        }
    }
}

/// Handle device removal by marking seed bank offline in registry
fn handle_device_removal(device: &str) -> anyhow::Result<()> {
    // With live-scan architecture, there's nothing to persist.
    // The device manifest IS the source of truth.
    // Next scan will automatically not include the removed device.
    info!(device = %device, "Device removed - seed bank will be absent from next scan");
    Ok(())
}

/// Scan for existing USB storage devices
async fn scan_existing_devices() -> Result<Vec<StorageDetectedInfo>> {
    // Use our robust USB detection that doesn't rely on the unreliable RM flag
    tokio::task::spawn_blocking(|| {
        super::list_usb_partitions()
    })
    .await
    .context("Failed to spawn blocking task")?
}

use std::os::unix::io::AsRawFd;
