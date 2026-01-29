//! SSE event type constants for Stone Presence Protocol (PRESENCE-0001)
//!
//! Centralized constants to prevent drift between Moss and Rake.

// Event categories (for filtering)
pub const CATEGORY_SERVICE: &str = "service";
pub const CATEGORY_STONE: &str = "stone";

// Snapshot event
pub const PRESENCE_SNAPSHOT: &str = "presence.snapshot";

// Service lifecycle events
pub const SERVICE_STARTED: &str = "service.started";
pub const SERVICE_STOPPED: &str = "service.stopped";
pub const SERVICE_SPROUTED: &str = "service.sprouted";
pub const SERVICE_UPROOTED: &str = "service.uprooted";

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

// Storage events (seed bank lifecycle)
pub const CATEGORY_STORAGE: &str = "storage";
pub const STORAGE_DETECTED: &str = "storage.detected";
pub const STORAGE_PREPARED: &str = "storage.prepared";
pub const STORAGE_RELEASED: &str = "storage.released";
pub const STORAGE_REMOVED: &str = "storage.removed";
pub const STORAGE_POOL_CONFLICT: &str = "storage.pool_conflict";
pub const STORAGE_READONLY: &str = "storage.readonly_detected";
pub const STORAGE_PREPARE_PROGRESS: &str = "storage.prepare.progress";
pub const STORAGE_REDISCOVERED: &str = "storage.rediscovered";
/// Storage beacon received from another stone (STORAGE-0003)
pub const STORAGE_BEACON_RECEIVED: &str = "storage.beacon.received";
