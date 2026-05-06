//! Domain Events
//!
//! Unified event types for all domain changes. These events are emitted
//! by lifecycle operations and consumed by listeners for:
//! - Chirp announcements (UDP broadcast to garden)
//! - SSE events (real-time UI updates)
//! - Timer management (nurturing schedules)
//! - Companion notifications (Firefly, Cricket)
//! - Future: webhooks, audit logging, metrics

use chrono::{DateTime, Utc};
use garden_common::constants::{
    EVENT_DEPLOYED, EVENT_DESTROYED, EVENT_HEALTH_CHANGED, EVENT_REMOVED, EVENT_RENAMED,
    EVENT_ROLE_CHANGED, EVENT_STARTED, EVENT_STOPPED, EVENT_UPDATED,
};
use garden_common::{OfferingRole, presence::event_types};
use serde::{Deserialize, Serialize};

// ============================================================================
// Domain Event (unified wrapper)
// ============================================================================

/// Unified domain event dispatched through the EventBus
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "domain", rename_all = "snake_case")]
pub enum DomainEvent {
    /// Offering lifecycle events (deploy, start, stop, etc.)
    Offering(OfferingEvent),
    /// Storage events (seed bank detected, removed, etc.)
    Storage(StorageEvent),
    /// Stone-level events (tended, health, load)
    Stone(StoneEvent),
    /// Job events (installation/removal progress)
    Job(JobEvent),
    /// Pond security events (enrollment changes)
    Pond(PondEvent),
}

impl DomainEvent {
    /// Get the event type string for SSE
    pub fn event_type(&self) -> &str {
        match self {
            Self::Offering(e) => e.event_type(),
            Self::Storage(e) => e.event_type(),
            Self::Stone(e) => e.event_type(),
            Self::Job(e) => e.event_type(),
            Self::Pond(e) => e.event_type(),
        }
    }

    /// Create a human-readable message
    pub fn to_message(&self) -> String {
        match self {
            Self::Offering(e) => e.to_message(),
            Self::Storage(e) => e.to_message(),
            Self::Stone(e) => e.to_message(),
            Self::Job(e) => e.to_message(),
            Self::Pond(e) => e.to_message(),
        }
    }

    /// Check if this event should trigger a chirp announcement
    pub fn should_chirp(&self) -> bool {
        match self {
            Self::Offering(e) => e.should_chirp(),
            Self::Storage(_) => false, // Storage is local-only
            Self::Stone(_) => false,   // Stone events are local-only
            Self::Job(_) => false,     // Job progress is local-only
            Self::Pond(_) => false,    // Pond events are local-only
        }
    }
}

// Convenience conversions
impl From<OfferingEvent> for DomainEvent {
    fn from(e: OfferingEvent) -> Self {
        Self::Offering(e)
    }
}

impl From<StorageEvent> for DomainEvent {
    fn from(e: StorageEvent) -> Self {
        Self::Storage(e)
    }
}

impl From<StoneEvent> for DomainEvent {
    fn from(e: StoneEvent) -> Self {
        Self::Stone(e)
    }
}

impl From<JobEvent> for DomainEvent {
    fn from(e: JobEvent) -> Self {
        Self::Job(e)
    }
}

impl From<PondEvent> for DomainEvent {
    fn from(e: PondEvent) -> Self {
        Self::Pond(e)
    }
}

// ============================================================================
// Pond Events
// ============================================================================

/// Pond security events (enrollment state changes)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PondEvent {
    /// Enrollment state changed (enrolled or unenrolled)
    EnrollmentChanged {
        enrolled: bool,
        cornerstone: Option<String>,
        timestamp: DateTime<Utc>,
    },
}

impl PondEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::EnrollmentChanged { .. } => event_types::POND_ENROLLMENT_CHANGED,
        }
    }

    pub fn to_message(&self) -> String {
        match self {
            Self::EnrollmentChanged {
                enrolled: true,
                cornerstone,
                ..
            } => {
                if let Some(cs) = cornerstone {
                    format!("Stone enrolled in pond (cornerstone: {})", cs)
                } else {
                    "Stone enrolled in pond".to_string()
                }
            }
            Self::EnrollmentChanged {
                enrolled: false, ..
            } => "Stone unenrolled from pond".to_string(),
        }
    }

    /// Builder: enrollment changed
    pub fn enrollment_changed(enrolled: bool, cornerstone: Option<String>) -> Self {
        Self::EnrollmentChanged {
            enrolled,
            cornerstone,
            timestamp: Utc::now(),
        }
    }
}

// ============================================================================
// Job Events
// ============================================================================

/// Job-related events (installation, removal, updates)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JobEvent {
    /// Job started
    Started {
        job_id: String,
        offering: String,
        operation: String, // "install", "remove", "update"
        timestamp: DateTime<Utc>,
    },
    /// Job progress update.
    ///
    /// `step` / `total_steps` are populated by single-operation jobs
    /// (capture_snapshot, plant_snapshot) so wire consumers can render
    /// real progress without separately fetching the Job state. Batch
    /// jobs leave them `None` and rely on `completed.len() / targets.len()`
    /// against the Job snapshot.
    Progress {
        job_id: String,
        offering: String,
        message: String,
        level: String, // "info", "warn", "error", "debug"
        timestamp: DateTime<Utc>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        total_steps: Option<u32>,
    },
    /// Job completed successfully
    Completed {
        job_id: String,
        offering: String,
        timestamp: DateTime<Utc>,
    },
    /// Job failed
    Failed {
        job_id: String,
        offering: String,
        error: String,
        timestamp: DateTime<Utc>,
    },
}

impl JobEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::Started { .. } => event_types::JOB_STARTED,
            Self::Progress { .. } => event_types::JOB_PROGRESS,
            Self::Completed { .. } => event_types::JOB_COMPLETED,
            Self::Failed { .. } => event_types::JOB_FAILED,
        }
    }

    pub fn to_message(&self) -> String {
        match self {
            Self::Started {
                offering,
                operation,
                ..
            } => {
                format!("Job started: {} {}", operation, offering)
            }
            Self::Progress { message, .. } => message.clone(),
            Self::Completed { offering, .. } => {
                format!("Job completed: {}", offering)
            }
            Self::Failed {
                offering, error, ..
            } => {
                format!("Job failed: {} - {}", offering, error)
            }
        }
    }

    pub fn job_id(&self) -> &str {
        match self {
            Self::Started { job_id, .. } => job_id,
            Self::Progress { job_id, .. } => job_id,
            Self::Completed { job_id, .. } => job_id,
            Self::Failed { job_id, .. } => job_id,
        }
    }

    pub fn offering(&self) -> &str {
        match self {
            Self::Started { offering, .. } => offering,
            Self::Progress { offering, .. } => offering,
            Self::Completed { offering, .. } => offering,
            Self::Failed { offering, .. } => offering,
        }
    }

    pub fn level(&self) -> &str {
        match self {
            Self::Started { .. } => "info",
            Self::Progress { level, .. } => level,
            Self::Completed { .. } => "info",
            Self::Failed { .. } => "error",
        }
    }

    // Builder helpers
    pub fn started(
        job_id: impl Into<String>,
        offering: impl Into<String>,
        operation: impl Into<String>,
    ) -> Self {
        Self::Started {
            job_id: job_id.into(),
            offering: offering.into(),
            operation: operation.into(),
            timestamp: Utc::now(),
        }
    }

    pub fn progress(
        job_id: impl Into<String>,
        offering: impl Into<String>,
        message: impl Into<String>,
        level: impl Into<String>,
    ) -> Self {
        Self::Progress {
            job_id: job_id.into(),
            offering: offering.into(),
            message: message.into(),
            level: level.into(),
            timestamp: Utc::now(),
            step: None,
            total_steps: None,
        }
    }

    /// Progress builder for single-operation jobs that report step
    /// counters alongside the message. `step` is 1-indexed; `total`
    /// is 0 when not yet known (consumers render the message without
    /// a percent until total > 0).
    pub fn progress_with_step(
        job_id: impl Into<String>,
        offering: impl Into<String>,
        message: impl Into<String>,
        level: impl Into<String>,
        step: u32,
        total: u32,
    ) -> Self {
        Self::Progress {
            job_id: job_id.into(),
            offering: offering.into(),
            message: message.into(),
            level: level.into(),
            timestamp: Utc::now(),
            step: Some(step),
            total_steps: if total > 0 { Some(total) } else { None },
        }
    }

    pub fn completed(job_id: impl Into<String>, offering: impl Into<String>) -> Self {
        Self::Completed {
            job_id: job_id.into(),
            offering: offering.into(),
            timestamp: Utc::now(),
        }
    }

    pub fn failed(
        job_id: impl Into<String>,
        offering: impl Into<String>,
        error: impl Into<String>,
    ) -> Self {
        Self::Failed {
            job_id: job_id.into(),
            offering: offering.into(),
            error: error.into(),
            timestamp: Utc::now(),
        }
    }
}

// ============================================================================
// Storage Events
// ============================================================================

/// Storage-related events (STORAGE-0010)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StorageEvent {
    /// Managed storage reconnected (has `.zen-garden/`, auto-mounted)
    StorageConnected {
        name: String,
        device: String,
        mount_path: String,
        capacity_gb: u64,
        roles: Vec<String>,
        timestamp: DateTime<Utc>,
    },
    /// Unmanaged device detected (needs `storage add`)
    StorageDetected {
        device: String,
        state: String,
        capacity_gb: u64,
        used_gb: u64,
        timestamp: DateTime<Utc>,
    },
    /// Storage removed/unmounted
    StorageRemoved {
        name: String,
        device: String,
        timestamp: DateTime<Utc>,
    },
    /// Storage released (unmounted, management cleared)
    StorageReleased {
        name: String,
        timestamp: DateTime<Utc>,
    },
    /// Storage device sensed — recognised and being measured
    StorageSensed {
        name: String,
        roles: Vec<String>,
        timestamp: DateTime<Utc>,
    },
    /// A replica set was renamed
    StorageRenamed {
        replica_set_id: String,
        new_name: String,
        timestamp: DateTime<Utc>,
    },
    /// A device's storage role changed (Primary, Replica, etc.)
    StorageRoleChanged {
        device_id: String,
        replica_set_id: String,
        new_role: String,
        timestamp: DateTime<Utc>,
    },
    /// A pin state changed on a device
    StoragePinChanged {
        device_id: String,
        replica_set_id: String,
        timestamp: DateTime<Utc>,
    },
    /// Volumes reclassified (broad change)
    StorageReclassified { timestamp: DateTime<Utc> },
    /// Sync started
    SyncStarted {
        name: String,
        timestamp: DateTime<Utc>,
    },
    /// Sync completed
    SyncCompleted {
        name: String,
        success: bool,
        timestamp: DateTime<Utc>,
    },
    /// Connectivity helper recovered a degraded candidate (STORAGE-0019).
    /// Fires once per device per coalescing window when SCSI rescan or
    /// USB re-authorization brought a 0-byte / offline device back to
    /// reachable. Carries the recovery summary so consumers can render
    /// "Recovered Realtek RTL9210C on USB port 2-3.4 in 4.4s" without
    /// re-querying.
    ConnectivityRecovered {
        device_id: String,
        model: Option<String>,
        size_bytes: u64,
        usb_port: Option<String>,
        recovered_via: String,
        attempts: u32,
        duration_ms: u64,
        timestamp: DateTime<Utc>,
    },
}

impl StorageEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::StorageConnected { .. } => event_types::STORAGE_CONNECTED,
            Self::StorageDetected { .. } => event_types::STORAGE_DETECTED,
            Self::StorageRemoved { .. } => event_types::STORAGE_REMOVED,
            Self::StorageReleased { .. } => event_types::STORAGE_RELEASED,
            Self::StorageSensed { .. } => event_types::STORAGE_SENSED,
            Self::StorageRenamed { .. } => event_types::STORAGE_RENAMED,
            Self::StorageRoleChanged { .. } => event_types::STORAGE_ROLE_CHANGED,
            Self::StoragePinChanged { .. } => event_types::STORAGE_PIN_CHANGED,
            Self::StorageReclassified { .. } => event_types::STORAGE_RECLASSIFIED,
            Self::SyncStarted { .. } => event_types::STORAGE_SYNC_STARTED,
            Self::SyncCompleted { .. } => event_types::STORAGE_SYNC_COMPLETED,
            Self::ConnectivityRecovered { .. } => event_types::STORAGE_CONNECTIVITY_RECOVERED,
        }
    }

    pub fn to_message(&self) -> String {
        match self {
            Self::StorageConnected { name, .. } => {
                format!("Storage '{}' connected", name)
            }
            Self::StorageDetected { device, state, .. } => {
                format!("Storage device detected: {} ({})", device, state)
            }
            Self::StorageRemoved { name, .. } => {
                format!("Storage '{}' removed", name)
            }
            Self::StorageReleased { name, .. } => {
                format!("Storage '{}' released", name)
            }
            Self::StorageSensed { name, .. } => {
                format!("Storage '{}' sensed", name)
            }
            Self::StorageRenamed { new_name, .. } => {
                format!("Storage renamed to '{}'", new_name)
            }
            Self::StorageRoleChanged {
                replica_set_id,
                new_role,
                ..
            } => {
                format!("Storage '{}' role changed to {}", replica_set_id, new_role)
            }
            Self::StoragePinChanged { replica_set_id, .. } => {
                format!("Storage '{}' pin changed", replica_set_id)
            }
            Self::StorageReclassified { .. } => "Storage volumes reclassified".to_string(),
            Self::SyncStarted { name, .. } => {
                format!("Storage '{}' sync started", name)
            }
            Self::SyncCompleted { name, success, .. } => {
                if *success {
                    format!("Storage '{}' sync completed", name)
                } else {
                    format!("Storage '{}' sync failed", name)
                }
            }
            Self::ConnectivityRecovered {
                model,
                usb_port,
                duration_ms,
                ..
            } => {
                let model = model.as_deref().unwrap_or("device");
                let port = usb_port
                    .as_deref()
                    .map(|p| format!(" on USB port {p}"))
                    .unwrap_or_default();
                let secs = (*duration_ms as f64) / 1000.0;
                format!("Recovered {model}{port} in {secs:.1}s")
            }
        }
    }

    // Builder helpers

    /// Build a `ConnectivityRecovered` event from the connectivity
    /// helper's outcome (STORAGE-0019).
    pub fn connectivity_recovered(
        device_id: impl Into<String>,
        model: Option<String>,
        size_bytes: u64,
        usb_port: Option<String>,
        recovered_via: impl Into<String>,
        attempts: u32,
        duration_ms: u64,
    ) -> Self {
        Self::ConnectivityRecovered {
            device_id: device_id.into(),
            model,
            size_bytes,
            usb_port,
            recovered_via: recovered_via.into(),
            attempts,
            duration_ms,
            timestamp: Utc::now(),
        }
    }

    pub fn storage_connected(
        name: impl Into<String>,
        device: impl Into<String>,
        mount_path: impl Into<String>,
        capacity_gb: u64,
        roles: Vec<String>,
    ) -> Self {
        Self::StorageConnected {
            name: name.into(),
            device: device.into(),
            mount_path: mount_path.into(),
            capacity_gb,
            roles,
            timestamp: Utc::now(),
        }
    }

    pub fn storage_detected(
        device: impl Into<String>,
        state: impl Into<String>,
        capacity_gb: u64,
        used_gb: u64,
    ) -> Self {
        Self::StorageDetected {
            device: device.into(),
            state: state.into(),
            capacity_gb,
            used_gb,
            timestamp: Utc::now(),
        }
    }

    pub fn storage_removed(name: impl Into<String>, device: impl Into<String>) -> Self {
        Self::StorageRemoved {
            name: name.into(),
            device: device.into(),
            timestamp: Utc::now(),
        }
    }
}

/// Convert a `StorageChanged` (infra broadcast) into a `StorageEvent` (domain event bus).
///
/// This bridges the two storage event systems: `StorageChanged` drives dedicated
/// infra subscribers (beacon, cloud filter, watcher), while `StorageEvent` feeds
/// the EventBus so PulseDomainBridge can translate them for SSE consumers.
impl From<&garden_common::storage::StorageChanged> for StorageEvent {
    fn from(changed: &garden_common::storage::StorageChanged) -> Self {
        let now = Utc::now();
        match changed {
            garden_common::storage::StorageChanged::Added {
                device_id,
                replica_set_id,
            } => Self::StorageConnected {
                name: replica_set_id.clone(),
                device: device_id.clone(),
                mount_path: String::new(),
                capacity_gb: 0,
                roles: vec![],
                timestamp: now,
            },
            garden_common::storage::StorageChanged::Removed {
                device_id,
                replica_set_id,
            } => Self::StorageRemoved {
                name: replica_set_id.clone(),
                device: device_id.clone(),
                timestamp: now,
            },
            garden_common::storage::StorageChanged::RoleChanged {
                device_id,
                replica_set_id,
                new_role,
            } => Self::StorageRoleChanged {
                device_id: device_id.clone(),
                replica_set_id: replica_set_id.clone(),
                new_role: format!("{:?}", new_role),
                timestamp: now,
            },
            garden_common::storage::StorageChanged::Renamed {
                replica_set_id,
                new_name,
            } => Self::StorageRenamed {
                replica_set_id: replica_set_id.clone(),
                new_name: new_name.clone(),
                timestamp: now,
            },
            garden_common::storage::StorageChanged::PinChanged {
                device_id,
                replica_set_id,
            } => Self::StoragePinChanged {
                device_id: device_id.clone(),
                replica_set_id: replica_set_id.clone(),
                timestamp: now,
            },
            garden_common::storage::StorageChanged::Reclassified => {
                Self::StorageReclassified { timestamp: now }
            }
            garden_common::storage::StorageChanged::Sensed { name, roles } => Self::StorageSensed {
                name: name.clone(),
                roles: roles.clone(),
                timestamp: now,
            },
            garden_common::storage::StorageChanged::Connected {
                name,
                roles,
                used_bytes: _,
                capacity_bytes,
            } => Self::StorageConnected {
                name: name.clone(),
                device: String::new(),
                mount_path: String::new(),
                capacity_gb: capacity_bytes / (1024 * 1024 * 1024),
                roles: roles.clone(),
                timestamp: now,
            },
            garden_common::storage::StorageChanged::Released { name } => Self::StorageReleased {
                name: name.clone(),
                timestamp: now,
            },
        }
    }
}

// ============================================================================
// Stone Events
// ============================================================================

/// Stone-level events (tended, health, load, network)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StoneEvent {
    /// Stone was tended (user interaction)
    Tended {
        by: String,
        from: String,
        message: Option<String>,
        timestamp: DateTime<Utc>,
    },
    /// Stone health changed
    HealthChanged {
        health: String,
        cpu_percent: f64,
        memory_percent: f64,
        timestamp: DateTime<Utc>,
    },
    /// Stone load updated (FIREFLY-0003: extended with disk, io, gpu, network)
    LoadUpdated {
        cpu_percent: f64,
        memory_percent: f64,
        disk_percent: f64,
        io_percent: f64,
        gpu_percent: f64,
        gpu_active: bool,
        net_rx_bytes_per_sec: u64,
        net_tx_bytes_per_sec: u64,
        timestamp: DateTime<Utc>,
    },
    /// Network became ready (valid LAN IP detected)
    /// Triggers immediate chirp announcement and mDNS registration.
    NetworkReady {
        ip: String,
        interface: Option<String>,
        timestamp: DateTime<Utc>,
    },
}

impl StoneEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::Tended { .. } => event_types::STONE_TENDED,
            Self::HealthChanged { .. } => event_types::STONE_HEALTH_CHANGED,
            Self::LoadUpdated { .. } => event_types::STONE_LOAD_UPDATED,
            Self::NetworkReady { .. } => event_types::STONE_NETWORK_READY,
        }
    }

    pub fn to_message(&self) -> String {
        match self {
            Self::Tended { by, .. } => format!("Stone tended by {}", by),
            Self::HealthChanged { health, .. } => format!("Stone health: {}", health),
            Self::LoadUpdated {
                cpu_percent,
                memory_percent,
                ..
            } => {
                format!(
                    "Stone load: CPU {:.0}%, Memory {:.0}%",
                    cpu_percent, memory_percent
                )
            }
            Self::NetworkReady { ip, .. } => format!("Network ready: {}", ip),
        }
    }

    // Builder helpers
    pub fn tended(by: impl Into<String>, from: impl Into<String>, message: Option<String>) -> Self {
        Self::Tended {
            by: by.into(),
            from: from.into(),
            message,
            timestamp: Utc::now(),
        }
    }

    pub fn health_changed(
        health: impl Into<String>,
        cpu_percent: f64,
        memory_percent: f64,
    ) -> Self {
        Self::HealthChanged {
            health: health.into(),
            cpu_percent,
            memory_percent,
            timestamp: Utc::now(),
        }
    }

    #[expect(clippy::too_many_arguments)]
    pub fn load_updated(
        cpu_percent: f64,
        memory_percent: f64,
        disk_percent: f64,
        io_percent: f64,
        gpu_percent: f64,
        gpu_active: bool,
        net_rx_bytes_per_sec: u64,
        net_tx_bytes_per_sec: u64,
    ) -> Self {
        Self::LoadUpdated {
            cpu_percent,
            memory_percent,
            disk_percent,
            io_percent,
            gpu_percent,
            gpu_active,
            net_rx_bytes_per_sec,
            net_tx_bytes_per_sec,
            timestamp: Utc::now(),
        }
    }

    pub fn network_ready(ip: impl Into<String>, interface: Option<String>) -> Self {
        Self::NetworkReady {
            ip: ip.into(),
            interface,
            timestamp: Utc::now(),
        }
    }
}

// ============================================================================
// Offering Events (existing)
// ============================================================================

/// Offering lifecycle event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OfferingEvent {
    /// Offering deployed (container created and started)
    Deployed {
        offering_id: String,
        name: String,
        stone_id: String,
        image: String,
        timestamp: DateTime<Utc>,
    },

    /// Offering started (container started)
    Started {
        offering_id: String,
        name: String,
        stone_id: String,
        timestamp: DateTime<Utc>,
    },

    /// Offering stopped (container stopped)
    Stopped {
        offering_id: String,
        name: String,
        stone_id: String,
        timestamp: DateTime<Utc>,
    },

    /// Offering removed (container deleted, data preserved)
    Removed {
        offering_id: String,
        name: String,
        stone_id: String,
        timestamp: DateTime<Utc>,
    },

    /// Offering destroyed (container + data deleted)
    Destroyed {
        offering_id: String,
        name: String,
        stone_id: String,
        timestamp: DateTime<Utc>,
    },

    /// Offering updated (new image version)
    Updated {
        offering_id: String,
        name: String,
        stone_id: String,
        from_image: String,
        to_image: String,
        timestamp: DateTime<Utc>,
    },

    /// Offering renamed
    Renamed {
        offering_id: String,
        old_name: String,
        new_name: String,
        stone_id: String,
        timestamp: DateTime<Utc>,
    },

    /// Offering health changed
    HealthChanged {
        offering_id: String,
        name: String,
        stone_id: String,
        status: String,
        timestamp: DateTime<Utc>,
    },

    /// Offering orchestration role changed (ORCH-0001).
    /// Emitted once; downstream listeners (chirp, presence, tools) react automatically.
    RoleChanged {
        offering_id: String,
        name: String,
        stone_id: String,
        old_role: OfferingRole,
        new_role: OfferingRole,
        timestamp: DateTime<Utc>,
    },
}

impl OfferingEvent {
    /// Get the offering_id from any event variant
    pub fn offering_id(&self) -> &str {
        match self {
            Self::Deployed { offering_id, .. } => offering_id,
            Self::Started { offering_id, .. } => offering_id,
            Self::Stopped { offering_id, .. } => offering_id,
            Self::Removed { offering_id, .. } => offering_id,
            Self::Destroyed { offering_id, .. } => offering_id,
            Self::Updated { offering_id, .. } => offering_id,
            Self::Renamed { offering_id, .. } => offering_id,
            Self::HealthChanged { offering_id, .. } => offering_id,
            Self::RoleChanged { offering_id, .. } => offering_id,
        }
    }

    /// Get the offering name from any event variant
    pub fn name(&self) -> &str {
        match self {
            Self::Deployed { name, .. } => name,
            Self::Started { name, .. } => name,
            Self::Stopped { name, .. } => name,
            Self::Removed { name, .. } => name,
            Self::Destroyed { name, .. } => name,
            Self::Updated { name, .. } => name,
            Self::Renamed { new_name, .. } => new_name,
            Self::HealthChanged { name, .. } => name,
            Self::RoleChanged { name, .. } => name,
        }
    }

    /// Get the stone_id from any event variant
    pub fn stone_id(&self) -> &str {
        match self {
            Self::Deployed { stone_id, .. } => stone_id,
            Self::Started { stone_id, .. } => stone_id,
            Self::Stopped { stone_id, .. } => stone_id,
            Self::Removed { stone_id, .. } => stone_id,
            Self::Destroyed { stone_id, .. } => stone_id,
            Self::Updated { stone_id, .. } => stone_id,
            Self::Renamed { stone_id, .. } => stone_id,
            Self::HealthChanged { stone_id, .. } => stone_id,
            Self::RoleChanged { stone_id, .. } => stone_id,
        }
    }

    /// Get event type as a string for logging/display
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::Deployed { .. } => EVENT_DEPLOYED,
            Self::Started { .. } => EVENT_STARTED,
            Self::Stopped { .. } => EVENT_STOPPED,
            Self::Removed { .. } => EVENT_REMOVED,
            Self::Destroyed { .. } => EVENT_DESTROYED,
            Self::Updated { .. } => EVENT_UPDATED,
            Self::Renamed { .. } => EVENT_RENAMED,
            Self::HealthChanged { .. } => EVENT_HEALTH_CHANGED,
            Self::RoleChanged { .. } => EVENT_ROLE_CHANGED,
        }
    }

    /// Check if this event should trigger a chirp announcement
    pub fn should_chirp(&self) -> bool {
        match self {
            // These change the garden topology
            Self::Deployed { .. } => true,
            Self::Removed { .. } => true,
            Self::Destroyed { .. } => true,
            Self::Renamed { .. } => true,
            Self::Updated { .. } => true,
            Self::HealthChanged { .. } => true,
            // Role changes affect topology (role field on TopologyServiceEntry)
            Self::RoleChanged { .. } => true,
            // Start/stop don't change topology (service exists but state changes)
            Self::Started { .. } => false,
            Self::Stopped { .. } => false,
        }
    }

    /// Check if this event should trigger timer management
    pub fn should_manage_timers(&self) -> bool {
        match self {
            // Deploy creates timer, remove/destroy deletes it
            Self::Deployed { .. } => true,
            Self::Removed { .. } => true,
            Self::Destroyed { .. } => true,
            Self::Renamed { .. } => true, // May need to rename timer
            // Others don't affect timers
            _ => false,
        }
    }

    /// Create a human-readable message for SSE/logging
    pub fn to_message(&self) -> String {
        match self {
            Self::Deployed { name, .. } => format!("Service {} deployed", name),
            Self::Started { name, .. } => format!("Service {} started", name),
            Self::Stopped { name, .. } => format!("Service {} stopped", name),
            Self::Removed { name, .. } => format!("Service {} removed", name),
            Self::Destroyed { name, .. } => format!("Service {} destroyed", name),
            Self::Updated {
                name,
                from_image,
                to_image,
                ..
            } => {
                format!(
                    "Service {} updated from {} to {}",
                    name, from_image, to_image
                )
            }
            Self::Renamed {
                old_name, new_name, ..
            } => {
                format!("Service {} renamed to {}", old_name, new_name)
            }
            Self::HealthChanged { name, status, .. } => {
                format!("Service {} health: {}", name, status)
            }
            Self::RoleChanged {
                name,
                old_role,
                new_role,
                ..
            } => {
                format!("Service {} role: {} → {}", name, old_role, new_role)
            }
        }
    }
}

/// Builder helpers for creating events with current timestamp
impl OfferingEvent {
    pub fn deployed(
        offering_id: impl Into<String>,
        name: impl Into<String>,
        stone_id: impl Into<String>,
        image: impl Into<String>,
    ) -> Self {
        Self::Deployed {
            offering_id: offering_id.into(),
            name: name.into(),
            stone_id: stone_id.into(),
            image: image.into(),
            timestamp: Utc::now(),
        }
    }

    pub fn started(
        offering_id: impl Into<String>,
        name: impl Into<String>,
        stone_id: impl Into<String>,
    ) -> Self {
        Self::Started {
            offering_id: offering_id.into(),
            name: name.into(),
            stone_id: stone_id.into(),
            timestamp: Utc::now(),
        }
    }

    pub fn stopped(
        offering_id: impl Into<String>,
        name: impl Into<String>,
        stone_id: impl Into<String>,
    ) -> Self {
        Self::Stopped {
            offering_id: offering_id.into(),
            name: name.into(),
            stone_id: stone_id.into(),
            timestamp: Utc::now(),
        }
    }

    pub fn removed(
        offering_id: impl Into<String>,
        name: impl Into<String>,
        stone_id: impl Into<String>,
    ) -> Self {
        Self::Removed {
            offering_id: offering_id.into(),
            name: name.into(),
            stone_id: stone_id.into(),
            timestamp: Utc::now(),
        }
    }

    pub fn destroyed(
        offering_id: impl Into<String>,
        name: impl Into<String>,
        stone_id: impl Into<String>,
    ) -> Self {
        Self::Destroyed {
            offering_id: offering_id.into(),
            name: name.into(),
            stone_id: stone_id.into(),
            timestamp: Utc::now(),
        }
    }

    pub fn updated(
        offering_id: impl Into<String>,
        name: impl Into<String>,
        stone_id: impl Into<String>,
        from_image: impl Into<String>,
        to_image: impl Into<String>,
    ) -> Self {
        Self::Updated {
            offering_id: offering_id.into(),
            name: name.into(),
            stone_id: stone_id.into(),
            from_image: from_image.into(),
            to_image: to_image.into(),
            timestamp: Utc::now(),
        }
    }

    pub fn renamed(
        offering_id: impl Into<String>,
        old_name: impl Into<String>,
        new_name: impl Into<String>,
        stone_id: impl Into<String>,
    ) -> Self {
        Self::Renamed {
            offering_id: offering_id.into(),
            old_name: old_name.into(),
            new_name: new_name.into(),
            stone_id: stone_id.into(),
            timestamp: Utc::now(),
        }
    }

    pub fn health_changed(
        offering_id: impl Into<String>,
        name: impl Into<String>,
        stone_id: impl Into<String>,
        status: impl Into<String>,
    ) -> Self {
        Self::HealthChanged {
            offering_id: offering_id.into(),
            name: name.into(),
            stone_id: stone_id.into(),
            status: status.into(),
            timestamp: Utc::now(),
        }
    }

    pub fn role_changed(
        offering_id: impl Into<String>,
        name: impl Into<String>,
        stone_id: impl Into<String>,
        old_role: OfferingRole,
        new_role: OfferingRole,
    ) -> Self {
        Self::RoleChanged {
            offering_id: offering_id.into(),
            name: name.into(),
            stone_id: stone_id.into(),
            old_role,
            new_role,
            timestamp: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_builders() {
        let event = OfferingEvent::deployed("id-1", "mongodb", "stone-01", "mongo:7");
        assert_eq!(event.offering_id(), "id-1");
        assert_eq!(event.name(), "mongodb");
        assert_eq!(event.stone_id(), "stone-01");
        assert_eq!(event.event_type(), "deployed");
        assert!(event.should_chirp());
        assert!(event.should_manage_timers());
    }

    #[test]
    fn test_started_stopped_no_chirp() {
        let started = OfferingEvent::started("id-1", "mongodb", "stone-01");
        let stopped = OfferingEvent::stopped("id-1", "mongodb", "stone-01");

        assert!(!started.should_chirp());
        assert!(!stopped.should_chirp());
        assert!(!started.should_manage_timers());
        assert!(!stopped.should_manage_timers());
    }

    #[test]
    fn test_to_message() {
        let event = OfferingEvent::updated("id-1", "mongodb", "stone-01", "mongo:6", "mongo:7");
        assert_eq!(
            event.to_message(),
            "Service mongodb updated from mongo:6 to mongo:7"
        );
    }
}
