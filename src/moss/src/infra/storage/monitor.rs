//! USB storage device monitoring via udev
//!
//! Linux-only module that uses udev to detect USB storage device events.
//! Emits SSE events and TTY ribbons when eligible devices are detected.

use anyhow::{Context, Result};
use garden_common::presence::event_types;
use garden_common::storage::StorageDetectedInfo;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use crate::app_state::MossEvent;
use super::analyze_device;

/// Storage event for internal broadcasting
#[derive(Debug, Clone)]
pub enum StorageEvent {
    /// A new eligible device was detected
    DeviceDetected(StorageDetectedInfo),
    /// A device was removed
    DeviceRemoved { device: String },
}

/// Monitors USB storage devices using udev
pub struct StorageMonitor {
    /// Broadcast channel for storage events
    event_tx: broadcast::Sender<StorageEvent>,
    /// Sender for MossEvents (SSE)
    moss_event_tx: broadcast::Sender<MossEvent>,
}

impl StorageMonitor {
    /// Create a new storage monitor
    pub fn new(moss_event_tx: broadcast::Sender<MossEvent>) -> Self {
        let (event_tx, _) = broadcast::channel(32);
        Self { event_tx, moss_event_tx }
    }
    
    /// Subscribe to storage events
    pub fn subscribe(&self) -> broadcast::Receiver<StorageEvent> {
        self.event_tx.subscribe()
    }
    
    /// Start monitoring for USB storage devices
    /// 
    /// This runs in a blocking thread since udev is synchronous.
    /// Spawns a tokio task internally.
    pub fn start(&self) -> Result<()> {
        let event_tx = self.event_tx.clone();
        let moss_event_tx = self.moss_event_tx.clone();
        
        // Spawn blocking task for udev monitoring
        std::thread::spawn(move || {
            if let Err(e) = run_udev_monitor(event_tx, moss_event_tx) {
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
fn run_udev_monitor(
    event_tx: broadcast::Sender<StorageEvent>,
    moss_event_tx: broadcast::Sender<MossEvent>,
) -> Result<()> {
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
                    
                    // Skip non-partition devices (we want /dev/sdb1, not /dev/sdb)
                    if !devnode.chars().last().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                        continue;
                    }
                    
                    // Analyze the device
                    match analyze_device(&devnode) {
                        Ok(info) => {
                            if info.eligible {
                                info!(
                                    device = %devnode,
                                    label = ?info.label,
                                    capacity = info.capacity_bytes,
                                    "Eligible USB storage detected"
                                );
                                
                                // Print TTY ribbon
                                if let Err(e) = garden_common::console::print_storage_detected_ribbon(&info) {
                                    warn!("Failed to print TTY ribbon: {}", e);
                                }
                                
                                // Emit SSE event
                                emit_storage_detected_event(&moss_event_tx, &info);
                                
                                // Broadcast internal event
                                let _ = event_tx.send(StorageEvent::DeviceDetected(info));
                            } else {
                                debug!(
                                    device = %devnode,
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
                    
                    // Emit removal event
                    emit_storage_removed_event(&moss_event_tx, &devnode);
                    
                    // Broadcast internal event
                    let _ = event_tx.send(StorageEvent::DeviceRemoved { device: devnode });
                }
                _ => {
                    // Change events, etc. - ignore for now
                }
            }
        }
    }
}

/// Emit SSE event for storage detected
fn emit_storage_detected_event(tx: &broadcast::Sender<MossEvent>, info: &StorageDetectedInfo) {
    let _data = serde_json::json!({
        "type": event_types::STORAGE_DETECTED,
        "data": info
    });
    
    let event = MossEvent {
        timestamp: chrono::Utc::now().to_rfc3339(),
        level: "info".to_string(),
        message: format!("[STORAGE] Detected: {} ({} bytes)", 
            info.label.as_deref().unwrap_or(&info.device),
            info.capacity_bytes
        ),
        job_id: None,
    };
    
    let _ = tx.send(event);
}

/// Emit SSE event for storage removed
fn emit_storage_removed_event(tx: &broadcast::Sender<MossEvent>, device: &str) {
    let event = MossEvent {
        timestamp: chrono::Utc::now().to_rfc3339(),
        level: "warn".to_string(),
        message: format!("[STORAGE] Removed: {}", device),
        job_id: None,
    };
    
    let _ = tx.send(event);
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
