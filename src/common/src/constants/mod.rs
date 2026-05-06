//! Zen Common Constants
//! Centralized constants for ports, names, paths, timeouts, limits, and error codes

pub mod categories;
pub mod channels;
pub mod headers;
pub mod limits;
pub mod orchestration;
pub mod paths;
pub mod reserved_names;
pub mod server;
pub mod storage;
pub mod timeouts;

/// Configuration directory path (platform-specific)
///
/// - Linux: `/etc/zen-garden`
/// - Windows: `.zen-garden` (relative to current working directory)
#[cfg(target_os = "windows")]
pub const CONFIG_DIR: &str = ".zen-garden";

#[cfg(target_os = "linux")]
pub const CONFIG_DIR: &str = "/etc/zen-garden";

// ============================================================================
// mDNS Discovery
// ============================================================================

/// mDNS service type for Koi HTTP API (no trailing `.local.`)
pub const MDNS_SERVICE_TYPE: &str = "_moss._tcp";

/// mDNS service type for native mdns-sd (fully qualified with `.local.`)
pub const MDNS_SERVICE_TYPE_LOCAL: &str = "_moss._tcp.local.";

/// mDNS service type for certmesh CA discovery (no trailing `.local.`)
pub const CERTMESH_SERVICE_TYPE: &str = "_certmesh._tcp";

/// mDNS service type for certmesh CA discovery (fully qualified with `.local.`)
pub const CERTMESH_SERVICE_TYPE_LOCAL: &str = "_certmesh._tcp.local.";

/// mDNS service type for HTTP web UIs (RFC 6763 standard).
///
/// Used by all zen-garden components that serve web interfaces:
/// Moss portrait, Lantern dashboard, and orchestrator dashboards.
pub const HTTP_SERVICE_TYPE: &str = "_http._tcp";

/// TXT key: URL path to the web UI (RFC 6763 standard for `_http._tcp`)
pub const TXT_PATH: &str = "path";

/// TXT key: zen-garden component type (e.g. "moss", "lantern", "orchestrator")
pub const TXT_COMPONENT: &str = "garden-component";

// ============================================================================
// Network Ports
// ============================================================================

/// HTTPS port for Moss API (when pond is active)
///
/// Below the baseline 7184 to avoid conflicts with the companion port range.
pub const MOSS_HTTPS: u16 = 7183;

/// UDP port for stone discovery broadcasts
pub const DISCOVERY_UDP: u16 = 7184;

/// HTTP port for Moss API (default)
pub const MOSS_HTTP: u16 = 7185;

/// HTTP port for Koi embedded API (mDNS, DNS, certmesh, UDP bridging)
///
/// "KOI" on a phone keypad = 5-6-4 → 564x.  Port 5641 chosen.
pub const KOI_HTTP: u16 = 5641;

/// DNS port for Koi local resolver (.zengarden zone)
///
/// Non-standard port — systemd-resolved owns port 53 and forwards
/// `.zengarden` queries to Koi via `resolvectl` routing domain.
pub const KOI_DNS: u16 = 5642;

/// HTTP port for Lantern API
pub const LANTERN_HTTP: u16 = 7186;

/// Base port for companion command servers (Cricket, Firefly, etc.)
///
/// Derived from ASCII sum of "moss Companion" (1187) + 6000 = 7187.
/// Range: 7187–7199 (13 companions max). Assigned by Moss via port ledger.
pub const COMPANION_PORT_BASE: u16 = 7187;

/// Maximum port for companion command servers
pub const COMPANION_PORT_MAX: u16 = 7199;

// ============================================================================
// Pond / mDNS TXT Keys
// ============================================================================

/// mDNS TXT key: pond state ("active" or absent)
pub const TXT_POND: &str = "pond";

/// mDNS TXT key: HTTPS port advertised when pond is active
pub const TXT_HTTPS_PORT: &str = "https_port";

/// mDNS TXT value: pond is active (certmesh CA initialized and unlocked)
pub const POND_ACTIVE: &str = "active";

// ============================================================================
// Component Names
// ============================================================================

/// Binary names
pub const MOSS_BINARY: &str = "garden-moss";
pub const RAKE_BINARY: &str = "garden-rake";
pub const LANTERN_BINARY: &str = "garden-lantern";

/// Config file names
pub const MOSS_CONFIG: &str = "garden-moss.toml";
pub const LANTERN_CONFIG: &str = "garden-lantern.toml";

/// Systemd service names
pub const MOSS_SERVICE: &str = "garden-moss.service";
pub const LANTERN_SERVICE: &str = "garden-lantern.service";

// ============================================================================
// Offering Naming
// ============================================================================

/// Separator for offering fully-qualified instance names (FQN v2).
/// Example: "ollama::dev"
pub(crate) const OFFERING_FQN_SEPARATOR: &str = "::";

/// Prefix for managed offering containers.
pub const OFFERING_CONTAINER_PREFIX: &str = "zen-offering-";

/// Container-safe separator for offering FQN instance names.
/// Example: "ollama:dev" -> "ollama--dev"
pub const OFFERING_FQN_CONTAINER_SEPARATOR: &str = "--";

/// Reserved instance name for adopted (native) offerings.
pub const OFFERING_ADOPTED_INSTANCE: &str = "adopted";

// ============================================================================
// File System Paths
// ============================================================================

/// Common paths (Linux-only)
pub const STONE_USER: &str = "stone";
pub const STONE_HOME: &str = "/home/stone";
pub const FIRST_RUN_FLAG: &str = "/etc/zen-garden/.first-run-complete";

/// Default stone password (matches STONE_USER by convention)
pub const STONE_PASSWORD: &str = "stone";

// ============================================================================
// Standard Error Codes
// ============================================================================

// Standard error codes for consistent API error responses
//
// Mapped to HTTP status codes:
// - 400 Bad Request: INVALID_REQUEST, TEMPLATE_NOT_FOUND, CONTAINER_NOT_RUNNING
// - 404 Not Found: SERVICE_NOT_FOUND, OFFERING_NOT_FOUND, NOT_FOUND, JOB_NOT_FOUND
// - 500 Internal Server Error: DOCKER_ERROR, INTERNAL_ERROR, REMOVE_FAILED, TEMPLATE_LOAD_FAILED, UPGRADE_FAILED
// - 503 Service Unavailable: DOCKER_UNAVAILABLE

// 400 Bad Request
pub const INVALID_REQUEST: &str = "INVALID_REQUEST";
pub const TEMPLATE_NOT_FOUND: &str = "TEMPLATE_NOT_FOUND";
pub const CONTAINER_NOT_RUNNING: &str = "CONTAINER_NOT_RUNNING";
pub const INVALID_COMPONENT: &str = "INVALID_COMPONENT";
pub const COMPATIBILITY_FAILED: &str = "COMPATIBILITY_FAILED";

// 404 Not Found
pub const SERVICE_NOT_FOUND: &str = "SERVICE_NOT_FOUND";
pub const OFFERING_NOT_FOUND: &str = "OFFERING_NOT_FOUND";
pub const NOT_FOUND: &str = "NOT_FOUND";
pub const JOB_NOT_FOUND: &str = "JOB_NOT_FOUND";

// 500 Internal Server Error
pub const DOCKER_ERROR: &str = "DOCKER_ERROR";
pub const INTERNAL_ERROR: &str = "INTERNAL_ERROR";
pub const REMOVE_FAILED: &str = "REMOVE_FAILED";
pub const TEMPLATE_LOAD_FAILED: &str = "TEMPLATE_LOAD_FAILED";
pub const UPGRADE_FAILED: &str = "UPGRADE_FAILED";
pub const INSUFFICIENT_RESOURCES: &str = "INSUFFICIENT_RESOURCES";

// 503 Service Unavailable
pub const DOCKER_UNAVAILABLE: &str = "DOCKER_UNAVAILABLE";

// ============================================================================
// Health Status Constants (Moss API)
// ============================================================================

/// Overall daemon health status - healthy
pub const HEALTH_HEALTHY: &str = "healthy";

/// Overall daemon health status - degraded (some components warn/degraded)
pub const HEALTH_DEGRADED: &str = "degraded";

/// Overall daemon health status - unhealthy (critical failure)
pub const HEALTH_UNHEALTHY: &str = "unhealthy";

/// Service health status - installing (setup in progress)
pub const HEALTH_INSTALLING: &str = "installing";

// ============================================================================
// Health Check Status Constants (HealthCheck struct)
// ============================================================================

/// Health check passed
pub const CHECK_PASS: &str = "pass";

/// Health check warning (degraded but functional)
pub const CHECK_WARN: &str = "warn";

/// Health check failed (critical)
pub const CHECK_FAIL: &str = "fail";

// ============================================================================
// Compatibility Decision Constants
// ============================================================================

/// Offering is fully compatible with stone hardware
pub const COMPAT_PASS: &str = "pass";

/// Offering can run but will use fallback configuration
pub const COMPAT_FALLBACK: &str = "fallback";

/// Offering can run but has potential issues (proceed with caution)
pub const COMPAT_WARNING: &str = "warning";

/// Offering is incompatible and cannot run on this stone
pub const COMPAT_FAIL: &str = "fail";

// ============================================================================
// Vitality Language Constants (Rake UI)
// ============================================================================

/// Stone/service is healthy and fully operational
pub const VITALITY_THRIVING: &str = "thriving";

/// Stone/service has warnings or degraded performance
pub const VITALITY_NEEDS_ATTENTION: &str = "needs attention";

/// Stone/service is critically unhealthy
pub const VITALITY_WITHERING: &str = "withering";

/// Stone/service is critically failing (terminal)
pub const VITALITY_WILTING: &str = "wilting";

/// Stone/service is offline or unreachable
pub const VITALITY_DORMANT: &str = "dormant";

// ============================================================================
// Tool Category Constants (GardenTool / ToolIdentity)
// ============================================================================

/// Tool category: managed or adopted offering (e.g., mongodb, ollama)
pub const CATEGORY_OFFERING: &str = "offering";

/// Tool category: orchestrator gateway (e.g., mongodb orchestrator)
pub const CATEGORY_ORCHESTRATOR: &str = "orchestrator";

/// Tool category: storage / seed-bank
pub const CATEGORY_STORAGE: &str = "storage";

/// Tool category: companion (e.g., cricket, firefly)
pub const CATEGORY_COMPANION: &str = "companion";

// ============================================================================
// Storage Role & Visibility Constants
// ============================================================================

/// Replication role: this stone owns the seed bank
pub const ROLE_PRIMARY: &str = "primary";

/// Replication role: active replica, pulls changes from primary and may serve reads
pub const ROLE_REPLICA: &str = "replica";

/// Composable role: receives offering harvests from nurturing cycles
pub const ROLE_SEED_BANK: &str = "seed-bank";

/// Storage visibility: accessible by all stones in the garden
pub const VISIBILITY_OPEN: &str = "open";

/// Storage visibility: only accessible locally
pub const VISIBILITY_CLOSED: &str = "closed";

/// Storage visibility: visible but read-only (degraded state)
pub const VISIBILITY_READ_ONLY: &str = "read-only";

// ============================================================================
// Storage Protocol Constants
// ============================================================================

/// Protocol identifier for S3-compatible access
pub const PROTOCOL_S3: &str = "s3";

/// Protocol identifier for garden storage (file-level) access
pub const PROTOCOL_STORAGE: &str = "storage";

/// Tool type for seed banks in ToolIdentity
pub const TOOL_TYPE_SEED_BANK: &str = "seed-bank";

// ============================================================================
// Stone Boot Health Progression
// ============================================================================

/// Stone is starting up (Phase 0: basic identity loaded)
pub const STONE_STARTING: &str = "starting";

/// Stone is initializing (Phase 2-3: network + basic hardware detected)
pub const STONE_INITIALIZING: &str = "initializing";

/// Stone is thriving (all services healthy, complete inventory)
pub const STONE_THRIVING: &str = "thriving";

/// Stone is degraded (some service errors detected)
pub const STONE_DEGRADED: &str = "degraded";

/// Stone is being nourished (updates in progress)
pub const STONE_NOURISHING: &str = "nourishing";

// ============================================================================
// Service Status Constants
// ============================================================================

/// Service is running
pub const SERVICE_RUNNING: &str = "running";

/// Service is stopped
pub const SERVICE_STOPPED: &str = "stopped";

/// Service is being installed
pub const SERVICE_INSTALLING: &str = "installing";

/// Service is being updated
pub const SERVICE_UPDATING: &str = "updating";

/// Service is in maintenance mode
pub const SERVICE_MAINTENANCE: &str = "maintenance";

/// Service is degraded
pub const SERVICE_DEGRADED: &str = "degraded";

/// Service cordoned (non-schedulable)
pub const SERVICE_CORDONED: &str = "cordoned";

/// Service status unknown
pub const SERVICE_UNKNOWN: &str = "unknown";

// ============================================================================
// Environment Variable Names
// ============================================================================

/// Environment variable for stone endpoint override (Rake client)
pub const ENV_GARDEN_STONE: &str = "GARDEN_STONE";

/// Stone name identifier (Moss daemon)
pub const ENV_STONE_NAME: &str = "STONE_NAME";

/// Stone host address (Moss daemon)
pub const ENV_STONE_HOST: &str = "STONE_HOST";

/// Lantern service registry endpoint (Moss discovery)
pub const ENV_LANTERN_ENDPOINT: &str = "LANTERN_ENDPOINT";

/// Environment variable to disable color output (universal standard)
pub const ENV_NO_COLOR: &str = "NO_COLOR";

/// Environment variable to enable Unicode support (Windows override)
pub const ENV_GARDEN_UNICODE: &str = "GARDEN_UNICODE";

// ============================================================================
// Common Default Values
// ============================================================================

/// Unknown value placeholder
pub const VALUE_UNKNOWN: &str = "unknown";

/// Default stone name when no configuration is provided
pub const DEFAULT_STONE_NAME: &str = "stone-01";

// ============================================================================
// HTTP Headers and Authentication
// ============================================================================

/// HTTP Authorization header name (lowercase per HTTP/2 spec)
pub const HEADER_AUTHORIZATION: &str = "authorization";

/// Bearer token authentication scheme prefix
pub const AUTH_BEARER_PREFIX: &str = "Bearer ";

// ============================================================================
// API Endpoint Paths
// ============================================================================

/// Health check endpoint path (used by Moss, Lantern, and Rake)
pub const ENDPOINT_HEALTH: &str = "/health";

/// Hardware capabilities endpoint path (Moss)
pub const ENDPOINT_CAPABILITIES: &str = "/capabilities";

// ============================================================================
// Job/Operation Status Constants
// ============================================================================

/// Job or operation completed successfully
pub const STATUS_COMPLETED: &str = "completed";

/// Operation succeeded (alternative to COMPLETED)
pub const STATUS_SUCCESS: &str = "success";

/// Job or operation failed
pub const STATUS_FAILED: &str = "failed";

/// Error occurred during operation
pub const STATUS_ERROR: &str = "error";

// ============================================================================
// Offering Lifecycle Event Types
// ============================================================================

/// Offering was deployed (container created and started)
pub const EVENT_DEPLOYED: &str = "deployed";

/// Offering was started (container started)
pub const EVENT_STARTED: &str = "started";

/// Offering was stopped (container stopped)
pub const EVENT_STOPPED: &str = "stopped";

/// Offering was removed (container deleted, data preserved)
pub const EVENT_REMOVED: &str = "removed";

/// Offering was destroyed (container + data deleted)
pub const EVENT_DESTROYED: &str = "destroyed";

/// Offering was updated to a new image version
pub const EVENT_UPDATED: &str = "updated";

/// Offering was renamed
pub const EVENT_RENAMED: &str = "renamed";

/// Offering health status changed
pub const EVENT_HEALTH_CHANGED: &str = "health_changed";

/// Offering orchestration role changed (ORCH-0001)
pub const EVENT_ROLE_CHANGED: &str = "role_changed";

// ============================================================================
// UDP Announcement Types
// ============================================================================

/// Stone chirp - periodic presence/service announcement
pub const ANNOUNCEMENT_STONE_CHIRP: &str = "STONE_CHIRP";

/// Stone goodbye - graceful shutdown notification
pub const ANNOUNCEMENT_STONE_GOODBYE: &str = "STONE_GOODBYE";

/// Storage detected - new seed bank or storage device found
pub const ANNOUNCEMENT_STORAGE_DETECTED: &str = "STORAGE_DETECTED";

/// Storage removed - seed bank or storage device removed
pub const ANNOUNCEMENT_STORAGE_REMOVED: &str = "STORAGE_REMOVED";

// ============================================================================
// SSE Event Levels
// ============================================================================

/// Informational event
pub const SSE_LEVEL_INFO: &str = "info";

/// Warning event
pub const SSE_LEVEL_WARN: &str = "warn";

/// Error event
pub const SSE_LEVEL_ERROR: &str = "error";

/// Debug event
pub const SSE_LEVEL_DEBUG: &str = "debug";

// ============================================================================
// Standard Error Codes Documentation
// ============================================================================
// Mapped to HTTP status codes:
// - 400 Bad Request: INVALID_REQUEST, TEMPLATE_NOT_FOUND, CONTAINER_NOT_RUNNING
// - 404 Not Found: SERVICE_NOT_FOUND, OFFERING_NOT_FOUND, NOT_FOUND, JOB_NOT_FOUND
// - 500 Internal Server Error: DOCKER_ERROR, INTERNAL_ERROR, REMOVE_FAILED, TEMPLATE_LOAD_FAILED, UPGRADE_FAILED
// - 503 Service Unavailable: DOCKER_UNAVAILABLE
