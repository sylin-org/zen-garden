//! SSE event type constants for Stone Presence Protocol (PRESENCE-0001)
//!
//! Centralized constants to prevent drift between Moss, Rake, and Companions.

// API path for presence stream (used by Companions and Rake)
pub const PRESENCE_STREAM_PATH: &str = "/api/v1/stone/presence/stream";

// API path for pulse stream (full firehose: domain + transport events)
pub const PULSE_STREAM_PATH: &str = "/api/v1/stone/pulse/stream";

// Event categories (for filtering)
pub const CATEGORY_SERVICE: &str = "service";
pub const CATEGORY_STONE: &str = "stone";

// Category prefixes (for event type matching)
pub const PREFIX_SERVICE: &str = "service.";
pub const PREFIX_STONE: &str = "stone.";
pub const PREFIX_STORAGE: &str = "storage.";

// Snapshot event
pub const PRESENCE_SNAPSHOT: &str = "presence.snapshot";

// Service lifecycle events
pub const SERVICE_STARTED: &str = "service.started";
pub const SERVICE_STOPPED: &str = "service.stopped";
pub const SERVICE_SPROUTED: &str = "service.sprouted";
pub const SERVICE_UPROOTED: &str = "service.uprooted";
pub const SERVICE_UPDATED: &str = "service.updated";
pub const SERVICE_RENAMED: &str = "service.renamed";
pub const SERVICE_HEALTH_CHANGED: &str = "service.health.changed";

// Offering status events (for visual feedback)
pub const OFFERING_STATUS_UP: &str = "offering.status.up";
pub const OFFERING_STATUS_DOWN: &str = "offering.status.down";
pub const OFFERING_STATUS_MAINTENANCE: &str = "offering.status.maintenance";
pub const OFFERING_REMOVED: &str = "offering.removed";
pub const OFFERING_ADOPTED: &str = "offering.adopted";

// Stone health events
pub const STONE_LOAD_UPDATED: &str = "stone.load.updated";
pub const STONE_HEALTH_CHANGED: &str = "stone.health.changed";
pub const STONE_TENDED: &str = "stone.tended";
pub const STONE_NETWORK_READY: &str = "stone.network.ready";

// Pond security events
pub const POND_ENROLLMENT_CHANGED: &str = "pond.enrollment.changed";

// Job events (installation/removal progress)
pub const CATEGORY_JOB: &str = "job";
pub const PREFIX_JOB: &str = "job.";
pub const JOB_STARTED: &str = "job.started";
pub const JOB_PROGRESS: &str = "job.progress";
pub const JOB_COMPLETED: &str = "job.completed";
pub const JOB_FAILED: &str = "job.failed";

// Storage events (STORAGE-0010)
pub const CATEGORY_STORAGE: &str = "storage";
/// Managed storage reconnected (has `.zen-garden/`, auto-mounted)
pub const STORAGE_CONNECTED: &str = "storage.connected";
/// Unmanaged device detected (empty or has files — needs `storage add`)
pub const STORAGE_DETECTED: &str = "storage.detected";
pub const STORAGE_RELEASED: &str = "storage.released";
pub const STORAGE_REMOVED: &str = "storage.removed";
pub const STORAGE_POOL_CONFLICT: &str = "storage.pool_conflict";
pub const STORAGE_READONLY: &str = "storage.readonly_detected";
pub const STORAGE_ADD_PROGRESS: &str = "storage.add.progress";
pub const STORAGE_REDISCOVERED: &str = "storage.rediscovered";
pub const STORAGE_SENSED: &str = "storage.sensed";
pub const STORAGE_RENAMED: &str = "storage.renamed";
pub const STORAGE_ROLE_CHANGED: &str = "storage.role_changed";
pub const STORAGE_PIN_CHANGED: &str = "storage.pin_changed";
pub const STORAGE_RECLASSIFIED: &str = "storage.reclassified";
pub const STORAGE_SYNC_STARTED: &str = "storage.sync_started";
pub const STORAGE_SYNC_COMPLETED: &str = "storage.sync_completed";
/// Storage beacon received from another stone (STORAGE-0003)
pub const STORAGE_BEACON_RECEIVED: &str = "storage.beacon.received";

// Orchestration events (ORCH-0001)
/// Offering election started for primary selection
pub const OFFERING_ELECTION_STARTED: &str = "offering.election.started";
/// Offering promoted to primary role
pub const OFFERING_ROLE_PROMOTED: &str = "offering.role.promoted";
/// Offering demoted from primary role
pub const OFFERING_ROLE_DEMOTED: &str = "offering.role.demoted";
/// Offering sync completed (dormant replica pull)
pub const OFFERING_SYNC_COMPLETED: &str = "offering.sync.completed";
/// Offering health degraded (consecutive failures)
pub const OFFERING_HEALTH_DEGRADED: &str = "offering.health.degraded";

// Server lifecycle events
/// Server shutting down (sent as final SSE event before stream close)
pub const SERVER_SHUTDOWN: &str = "server.shutdown";
