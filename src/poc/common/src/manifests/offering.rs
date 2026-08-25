//! Offering Manifests (Unified Model)
//!
//! Single source of truth for all offering definitions. An offering can support
//! multiple deployment modes - managed (container), adopted (native), borrowed (external).
//!
//! # Mode Support
//!
//! Mode support is determined by which configurations are present:
//! - `managed.is_some()` → supports Managed mode (container deployment)
//! - `adopted.is_some()` → supports Adopted mode (native service detection)
//! - `borrowed.is_some()` → supports Borrowed mode (external service announcement)
//!
//! # Structure
//!
//! ```text
//! Offering
//! ├── name, category (identity)
//! ├── managed: Option<ManagedConfig>      # Container deployment
//! │   └── snippet_yaml, network, tasks
//! ├── adopted: Option<AdoptedConfig>      # Native service detection
//! │   └── detection, control
//! ├── borrowed: Option<BorrowedConfig>    # External service
//! │   └── location, health
//! ├── metadata: OfferingMetadata          # UI display info
//! ├── compatibility: Option<CompatibilityRules>
//! ├── guidance: Option<String>            # User documentation
//! └── connection: Option<ConnectionProfile> # Connection profile
//! ```

use crate::manifests::connection::ConnectionProfile;
use crate::manifests::connectivity::ConnectivityConfig;
use crate::manifests::detection::{
    ControlConfig, HealthConfig, HealthVerification, LocationConfig, OsDetectionRules,
    PortDetectionConfig, ProcessDetection,
};
use crate::manifests::ceremony::CeremonyPolicy;
use crate::types::AdoptedControlLevel;
use crate::{CompatibilityRule, CompatibilityRules, CoordinationMode, OfferingMode, TaskDefinition};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use super::discover_subdirectories;

// ============================================================================
// Network Requirements
// ============================================================================

/// Network requirements for an offering
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct NetworkRequirements {
    /// Static IP preference for this offering
    #[serde(default)]
    pub static_ip: StaticIpPreference,

    /// Human-readable reason (why this offering benefits from/requires static IP)
    #[serde(default)]
    pub static_ip_reason: Option<String>,
}

impl NetworkRequirements {
    pub fn wants_static_ip(&self) -> bool {
        !matches!(self.static_ip, StaticIpPreference::None)
    }

    pub fn requires_static_ip(&self) -> bool {
        matches!(self.static_ip, StaticIpPreference::Required)
    }
}

/// Static IP preference level
#[derive(Debug, Clone, Copy, Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StaticIpPreference {
    #[default]
    None,
    Preferred,
    Required,
}

impl StaticIpPreference {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Preferred => "preferred",
            Self::Required => "required",
        }
    }
}

// ============================================================================
// Mode-Specific Configurations
// ============================================================================

/// Managed mode: container-based deployment
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ManagedConfig {
    /// Raw Docker Compose snippet (template with Tera expressions)
    pub snippet_yaml: String,

    /// Network requirements (static IP preference)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub network: Option<NetworkRequirements>,

    /// Tasks to run during deployment lifecycle
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tasks: Option<Vec<TaskDefinition>>,
}

/// Adopted mode: native service detection and control
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdoptedConfig {
    /// OS-specific detection rules (legacy command-based).
    /// Deprecated: use `process` + `health` + `ports` instead.
    #[serde(default)]
    pub detection: OsDetectionRules,

    /// Process-based detection (DETECT-0001).
    /// Cross-platform: matches process executable + cmdline pattern.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub process: Option<ProcessDetection>,

    /// Health verification for process-based detection.
    /// HTTP probe on discovered port to confirm service identity.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub health: Option<HealthVerification>,

    /// Port configuration for process-based detection.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ports: Option<PortDetectionConfig>,

    /// Control commands (start/stop/restart)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub control: Option<ControlConfig>,

    /// Default control level when adopting
    #[serde(default)]
    pub default_control_level: AdoptedControlLevel,

    /// Health check for adopted service
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub health_check: Option<HealthConfig>,

    /// User-facing guidance for adopted mode
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub guidance: Option<String>,

    /// Connectivity enforcement rules (LAN access, firewall, binding)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub connectivity: Option<ConnectivityConfig>,
}

/// Borrowed mode: external service announcement
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BorrowedConfig {
    /// Default/suggested location
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub default_location: Option<LocationConfig>,

    /// Health check configuration
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub health: Option<HealthConfig>,

    /// Whether location is required (vs optional)
    #[serde(default)]
    pub location_required: bool,
}

/// UI and documentation metadata
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct OfferingMetadata {
    /// Human-readable description
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,

    /// Search/filter tags
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tags: Vec<String>,

    /// Icon identifier or URL
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub icon: Option<String>,

    /// Project homepage URL
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub homepage: Option<String>,

    /// Documentation URL
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub documentation: Option<String>,

    /// Primary port (for quick reference in UI)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub port: Option<u16>,
}

/// Manifest-declared environment variables that Moss may read and write.
///
/// Lives in `.frontmatter.json` as a cross-mode field: the same env vars
/// are meaningful regardless of whether the service is managed (container),
/// adopted (bare metal), or borrowed.
///
/// See ADR MOSS-0005 for the full design.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ManageableEnv {
    /// Platform service name for env-file / systemd / Windows Service lookup.
    /// If absent, derived from the offering name.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub service_name: Option<String>,

    /// Whether changes require a service restart to take effect.
    #[serde(default = "default_true")]
    pub restart_required: bool,

    /// Allowlist of environment variable names Moss may read and write.
    #[serde(default)]
    pub vars: Vec<String>,
}

fn default_true() -> bool {
    true
}

// ============================================================================
// Runtime Manifests Directory
// ============================================================================

/// Get runtime manifests directory (uses platform-aware paths)
pub fn runtime_manifests_dir() -> String {
    if let Ok(dir) = std::env::var("GARDEN_MANIFESTS_DIR") {
        return dir;
    }

    let production_dir = format!("{}/manifests", crate::constants::paths::data_dir());
    if std::path::Path::new(&production_dir).exists() {
        return production_dir;
    }

    if let Ok(current_dir) = std::env::current_dir() {
        let dev_paths = [
            "src/moss/embedded/manifests",
            "../moss/embedded/manifests",
            "../../moss/embedded/manifests",
            "../../../moss/embedded/manifests",
        ];

        for relative_path in dev_paths {
            let path = current_dir.join(relative_path);
            if path.exists() {
                return path.to_string_lossy().to_string();
            }
        }
    }

    production_dir
}

// ============================================================================
// Service Template (Parsed Container Config)
// ============================================================================

/// Parsed service template ready for container creation
#[derive(Debug, Clone)]
pub struct ServiceTemplate {
    pub image: String,
    /// Manifest-level command override (e.g., Prometheus `--config.file=...`).
    pub command: Option<Vec<String>>,
    /// Config file mappings for file-based configuration injection.
    pub config_files: Vec<ConfigFileMapping>,
    pub ports: HashMap<String, (u16, u16)>,
    pub environment: Vec<String>,
    pub volumes: Vec<(String, String)>,
    pub compatibility: Option<CompatibilityRules>,
    pub tasks: HashMap<String, TaskDefinition>,
    pub network: NetworkRequirements,
    /// GPU device requests parsed from `deploy.resources.reservations.devices`.
    pub device_requests: Vec<GpuDeviceRequest>,
    /// Resource limits parsed from `deploy.resources.limits` (OFFER-0009).
    pub resource_limits: ResourceLimits,
    /// Container healthcheck parsed from the snippet `healthcheck:` block (OFFER-0009).
    pub healthcheck: Option<ContainerHealthcheck>,
}

impl ServiceTemplate {
    pub fn default_port(&self) -> Option<&(u16, u16)> {
        self.ports.get("default")
    }

    pub fn default_host_port(&self) -> u16 {
        self.default_port().map(|(host, _)| *host).unwrap_or(30000)
    }

    pub fn ports_vec(&self) -> Vec<(u16, u16)> {
        let mut ports = Vec::with_capacity(self.ports.len());
        if let Some(p) = self.ports.get("default") {
            ports.push(*p);
        }
        let mut other_ports: Vec<_> = self.ports.iter().filter(|(k, _)| *k != "default").collect();
        other_ports.sort_by_key(|(k, _)| *k);
        for (_, port) in other_ports {
            ports.push(*port);
        }
        ports
    }
}

/// Template listing info (for API responses)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateInfo {
    pub name: String,
    pub category: String,
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

// ============================================================================
// Config File Mapping (for managed offerings)
// ============================================================================

/// A config file that the software reads at startup.
///
/// The manifest declares where the file lives inside the container, what format
/// it uses, and how to trigger a reload after changes. This enables config
/// changes via file write + restart/signal instead of container recreation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFileMapping {
    /// Path inside the container where the software reads config.
    /// e.g., "/etc/mongod.conf"
    pub path: String,
    /// File format so Moss knows how to write/merge it.
    pub format: ConfigFormat,
    /// Command-line flag to add to Cmd so software reads this file.
    /// e.g., "--config /etc/mongod.conf"
    /// If absent, software reads it automatically from the default location.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flag: Option<String>,
    /// How to apply changes after writing the config file.
    #[serde(default)]
    pub reload: ReloadPolicy,
}

/// Config file format — determines how Moss writes empty/initial content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigFormat {
    Yaml,
    Toml,
    Ini,
    Json,
    Properties,
    Raw,
}

impl ConfigFormat {
    /// Minimal valid content for this format (empty config = all defaults).
    pub fn empty_content(&self) -> &'static str {
        match self {
            Self::Yaml => "# Managed by Zen Garden\n{}\n",
            Self::Json => "{}\n",
            Self::Toml => "# Managed by Zen Garden\n",
            Self::Ini => "; Managed by Zen Garden\n",
            Self::Properties => "# Managed by Zen Garden\n",
            Self::Raw => "",
        }
    }
}

/// How to apply config changes after writing the file.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReloadPolicy {
    /// `docker restart` — brief downtime, software re-reads config on startup.
    #[default]
    Restart,
    /// Send a Unix signal to the container process (e.g., "SIGHUP").
    /// Software reloads config without restarting — zero downtime.
    Signal(String),
}

// Internal: YAML parsing structures

/// Deserialize `command` from either a string or a list of strings.
/// Docker Compose supports both `command: "arg1 arg2"` and `command: ["arg1", "arg2"]`.
fn deserialize_command<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;

    struct CommandVisitor;

    impl<'de> de::Visitor<'de> for CommandVisitor {
        type Value = Option<Vec<String>>;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a string or list of strings")
        }

        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            Ok(Some(v.split_whitespace().map(String::from).collect()))
        }

        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut items = Vec::new();
            while let Some(item) = seq.next_element::<String>()? {
                items.push(item);
            }
            Ok(Some(items))
        }
    }

    deserializer.deserialize_any(CommandVisitor)
}

#[derive(Debug, Deserialize)]
struct ComposeFile {
    services: HashMap<String, ServiceConfig>,
}

#[derive(Debug, Deserialize, Clone)]
struct ServiceConfig {
    image: String,
    #[serde(default)]
    ports: HashMap<String, (u16, u16)>,
    #[serde(default)]
    environment: Option<serde_yml::Value>,
    #[serde(default)]
    volumes: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_command")]
    command: Option<Vec<String>>,
    #[serde(default)]
    config_files: Vec<ConfigFileMapping>,
    #[serde(default)]
    tasks: HashMap<String, TaskDefinition>,
    #[serde(default)]
    network: NetworkRequirements,
    #[serde(default)]
    deploy: Option<DeployConfig>,
    #[serde(default)]
    healthcheck: Option<HealthcheckConfig>,
}

/// Docker Compose `healthcheck:` block (raw string durations) — OFFER-0009.
#[derive(Debug, Deserialize, Clone)]
struct HealthcheckConfig {
    #[serde(default)]
    test: Vec<String>,
    #[serde(default)]
    interval: Option<String>,
    #[serde(default)]
    timeout: Option<String>,
    #[serde(default)]
    retries: Option<i64>,
    #[serde(default)]
    start_period: Option<String>,
}

/// Docker Compose `deploy` section — GPU device reservations.
#[derive(Debug, Deserialize, Clone, Default)]
struct DeployConfig {
    #[serde(default)]
    resources: Option<ResourcesConfig>,
}

#[derive(Debug, Deserialize, Clone, Default)]
struct ResourcesConfig {
    #[serde(default)]
    reservations: Option<ReservationsConfig>,
    #[serde(default)]
    limits: Option<LimitsConfig>,
}

/// Docker Compose `deploy.resources.limits` — memory/cpu caps (OFFER-0009).
#[derive(Debug, Deserialize, Clone, Default)]
struct LimitsConfig {
    #[serde(default)]
    memory: Option<String>,
    #[serde(default)]
    cpus: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Default)]
struct ReservationsConfig {
    #[serde(default)]
    devices: Vec<DeviceReservation>,
}

#[derive(Debug, Deserialize, Clone)]
struct DeviceReservation {
    #[serde(default)]
    driver: Option<String>,
    #[serde(default)]
    count: Option<serde_yml::Value>, // "all" or a number
    #[serde(default)]
    capabilities: Vec<Vec<String>>,
}

/// Parsed GPU device request for container creation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GpuDeviceRequest {
    /// Driver name (e.g., "nvidia")
    pub driver: String,
    /// Number of devices: -1 = all, N = specific count
    pub count: i64,
    /// Required capabilities (e.g., [["gpu"]])
    pub capabilities: Vec<Vec<String>>,
}

/// Parsed container resource limits (OFFER-0009).
///
/// Maps to bollard `HostConfig.memory` (bytes) and `HostConfig.nano_cpus`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Hard memory limit in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_bytes: Option<i64>,
    /// CPU limit in nano-CPUs (1.5 cores = 1_500_000_000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nano_cpus: Option<i64>,
}

impl ResourceLimits {
    /// True when no limit is set.
    pub fn is_empty(&self) -> bool {
        self.memory_bytes.is_none() && self.nano_cpus.is_none()
    }
}

/// Parsed container healthcheck (OFFER-0009).
///
/// Maps to bollard `ContainerCreateBody.healthcheck`. Durations are nanoseconds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContainerHealthcheck {
    /// Test command, e.g. `["CMD", "curl", "-f", "http://localhost:8191/health"]`.
    pub test: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval_ns: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ns: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retries: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_period_ns: Option<i64>,
}

/// Parse a Docker-style size string ("512m", "2g", "1073741824") to bytes.
fn parse_memory_bytes(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let last = s.chars().last()?;
    // Strip the suffix on a char boundary (robust if a future arm matches a
    // multi-byte char); the current k/m/g/b arms are all single-byte ASCII.
    let stripped = &s[..s.len() - last.len_utf8()];
    let (num_str, mult): (&str, i64) = match last.to_ascii_lowercase() {
        'k' => (stripped, 1024),
        'm' => (stripped, 1024 * 1024),
        'g' => (stripped, 1024 * 1024 * 1024),
        'b' => (stripped, 1),
        c if c.is_ascii_digit() => (s, 1),
        _ => return None,
    };
    let value: f64 = num_str.trim().parse().ok()?;
    if !value.is_finite() {
        return None;
    }
    let bytes = (value * mult as f64) as i64;
    (bytes > 0).then_some(bytes)
}

/// Parse a decimal CPU count ("1.5") to nano-CPUs.
fn parse_cpus_nano(s: &str) -> Option<i64> {
    let value: f64 = s.trim().parse().ok()?;
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    Some((value * 1_000_000_000.0).round() as i64)
}

/// Parse a Go/Compose duration ("30s", "1m30s", "500ms") to nanoseconds.
fn parse_duration_ns(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(secs) = s.parse::<f64>() {
        if !secs.is_finite() {
            return None;
        }
        let ns = (secs * 1_000_000_000.0) as i64;
        return (ns > 0).then_some(ns);
    }
    let mut total: i64 = 0;
    let mut num = String::new();
    let mut chars = s.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() || c == '.' {
            num.push(c);
            chars.next();
        } else {
            let mut unit = String::new();
            while let Some(&u) = chars.peek() {
                if u.is_ascii_alphabetic() {
                    unit.push(u);
                    chars.next();
                } else {
                    break;
                }
            }
            let value: f64 = num.parse().ok()?;
            if !value.is_finite() {
                return None;
            }
            num.clear();
            let mult = match unit.as_str() {
                "ns" => 1.0,
                "us" => 1_000.0,
                "ms" => 1_000_000.0,
                "s" => 1_000_000_000.0,
                "m" => 60.0 * 1_000_000_000.0,
                "h" => 3_600.0 * 1_000_000_000.0,
                _ => return None,
            };
            total += (value * mult) as i64;
        }
    }
    if !num.is_empty() {
        return None;
    }
    (total > 0).then_some(total)
}

// ============================================================================
// Offering (Core Type)
// ============================================================================

/// Single source of truth for an offering definition
///
/// Mode support is determined by which configurations are present:
/// - `managed.is_some()` → supports Managed mode (container deployment)
/// - `adopted.is_some()` → supports Adopted mode (native service detection)
/// - `borrowed.is_some()` → supports Borrowed mode (external service)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Offering {
    // ═══════════════════════════════════════════════════════════════════════
    // IDENTITY
    // ═══════════════════════════════════════════════════════════════════════
    /// Offering name (e.g., "mongodb", "ollama")
    pub name: String,

    /// Category (e.g., "data", "ai", "network")
    pub category: String,

    // ═══════════════════════════════════════════════════════════════════════
    // MODE CONFIGURATIONS (at least one must be present)
    // ═══════════════════════════════════════════════════════════════════════
    /// Managed mode: container deployment
    pub managed: Option<ManagedConfig>,

    /// Adopted mode: native service detection & control
    pub adopted: Option<AdoptedConfig>,

    /// Borrowed mode: external service announcement
    pub borrowed: Option<BorrowedConfig>,

    // ═══════════════════════════════════════════════════════════════════════
    // CROSS-MODE FIELDS
    // ═══════════════════════════════════════════════════════════════════════
    /// UI metadata (description, tags, icon, etc.)
    pub metadata: OfferingMetadata,

    /// Hardware compatibility rules
    pub compatibility: Option<CompatibilityRules>,

    /// User-facing guidance documentation (markdown)
    pub guidance: Option<String>,

    /// Connection profile for dependent services
    pub connection: Option<ConnectionProfile>,

    /// Manifest-declared manageable environment variables (MOSS-0005).
    pub manageable_env: Option<ManageableEnv>,

    // ═══════════════════════════════════════════════════════════════════════
    // ORCHESTRATION (ORCH-0006)
    // ═══════════════════════════════════════════════════════════════════════
    /// How instances coordinate across stones.
    /// `Independent` (default) = no election. `Elected` = Primary/Replica roles.
    #[serde(default)]
    pub coordination: CoordinationMode,

    /// Snapshot ceremony policy (quiesce/resume) for managed offerings (ORCH-0041).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ceremony: Option<CeremonyPolicy>,
}

impl Offering {
    /// Get supported modes (derived from which configs are present)
    pub fn modes(&self) -> Vec<OfferingMode> {
        let mut modes = Vec::new();
        if self.managed.is_some() {
            modes.push(OfferingMode::Managed);
        }
        if self.adopted.is_some() {
            modes.push(OfferingMode::Adopted);
        }
        if self.borrowed.is_some() {
            modes.push(OfferingMode::Borrowed);
        }
        modes
    }

    /// Check if offering supports a specific mode
    pub fn supports_mode(&self, mode: &OfferingMode) -> bool {
        match mode {
            OfferingMode::Managed => self.managed.is_some(),
            OfferingMode::Adopted => self.adopted.is_some(),
            OfferingMode::Borrowed => self.borrowed.is_some(),
        }
    }

    /// Get detection rules for the current OS
    pub fn get_detection_rules(&self) -> Vec<crate::manifests::DetectionRule> {
        self.adopted
            .as_ref()
            .map(|a| a.detection.get_current_os_rules())
            .unwrap_or_default()
    }

    /// Get control config for adopted mode
    pub fn get_control_config(&self) -> Option<&ControlConfig> {
        self.adopted.as_ref().and_then(|a| a.control.as_ref())
    }

    /// Get connectivity config for adopted mode
    pub fn get_connectivity_config(&self) -> Option<&ConnectivityConfig> {
        self.adopted.as_ref().and_then(|a| a.connectivity.as_ref())
    }

    /// Get description
    pub fn description(&self) -> String {
        self.metadata
            .description
            .clone()
            .unwrap_or_else(|| format!("{} service", self.name))
    }

    /// Get tags (normalized to lowercase)
    pub fn tags(&self) -> Vec<String> {
        self.metadata
            .tags
            .iter()
            .map(|t| t.trim().to_lowercase())
            .filter(|t| !t.is_empty())
            .collect()
    }

    /// Get default host port from any available source
    pub fn default_host_port(&self) -> u16 {
        // Try metadata port first
        if let Some(port) = self.metadata.port {
            return port;
        }
        // Try parsing managed config for port
        if let Some(ref managed) = self.managed
            && let Ok(template) = self.parse_managed_template(managed, &self.name)
        {
            let port = template.default_host_port();
            if port != 30000 {
                return port;
            }
        }
        8080 // Generic default
    }

    /// Convert to TemplateInfo for API responses
    pub fn to_template_info(&self) -> TemplateInfo {
        TemplateInfo {
            name: self.name.clone(),
            category: self.category.clone(),
            description: self.description(),
            tags: self.tags(),
        }
    }

    /// Parse managed config snippet into ServiceTemplate.
    ///
    /// Volumes are namespaced under the offering name. For FQN-specific isolation
    /// (e.g., `comfyui::prod` gets its own volume directories), use
    /// [`parse_template_for_fqn`] instead.
    pub fn parse_template(&self) -> Result<ServiceTemplate> {
        self.parse_template_with_namespace(&self.name)
    }

    /// Parse managed config snippet with FQN-specific volume isolation.
    ///
    /// Volumes are namespaced under the FQN's encoded name so that
    /// `comfyui` and `comfyui::prod` get separate data directories:
    /// - `comfyui`       → `{volumes_dir}/comfyui/comfyui-models`
    /// - `comfyui::prod` → `{volumes_dir}/comfyui--prod/comfyui-models`
    pub fn parse_template_for_fqn(
        &self,
        fqn: &crate::offerings::OfferingFqn,
    ) -> Result<ServiceTemplate> {
        self.parse_template_with_namespace(&fqn.encoded_for_container())
    }

    fn parse_template_with_namespace(&self, volume_namespace: &str) -> Result<ServiceTemplate> {
        let managed = self
            .managed
            .as_ref()
            .with_context(|| format!("Offering '{}' has no managed config", self.name))?;
        self.parse_managed_template(managed, volume_namespace)
    }

    fn parse_managed_template(
        &self,
        managed: &ManagedConfig,
        volume_namespace: &str,
    ) -> Result<ServiceTemplate> {
        let yaml = managed.snippet_yaml.replace("\r\n", "\n");

        // Try parsing as snippet format first (direct service config)
        if let Ok(service_config) = serde_yml::from_str::<ServiceConfig>(&yaml) {
            return Ok(self.service_config_to_template(service_config, volume_namespace));
        }

        // Fallback: try parsing as compose file (services: wrapper)
        let compose: ComposeFile = serde_yml::from_str(&yaml).with_context(|| {
            format!(
                "Failed to parse YAML for '{}'. First 100 chars: {}",
                self.name,
                &yaml[..yaml.len().min(100)]
            )
        })?;

        let service_config = compose
            .services
            .get(&self.name)
            .with_context(|| format!("Service '{}' not found in compose file", self.name))?
            .clone();

        Ok(self.service_config_to_template(service_config, volume_namespace))
    }

    fn service_config_to_template(
        &self,
        config: ServiceConfig,
        volume_namespace: &str,
    ) -> ServiceTemplate {
        let environment = match &config.environment {
            Some(serde_yml::Value::Sequence(list)) => list
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect(),
            Some(serde_yml::Value::Mapping(map)) => map
                .iter()
                .filter_map(|(k, v)| {
                    let key = k.as_str()?;
                    let value = v.as_str().unwrap_or("");
                    Some(format!("{}={}", key, value))
                })
                .collect(),
            _ => Vec::new(),
        };

        let volumes = config
            .volumes
            .iter()
            .filter_map(|v| {
                let parts: Vec<&str> = v.split(':').collect();
                if parts.len() >= 2 {
                    let host_path = if parts[0].starts_with('/') || parts[0].contains('\\') {
                        // Absolute path specified in manifest — use as-is
                        parts[0].to_string()
                    } else {
                        // Relative volume name — resolve per-FQN:
                        //   {volumes_dir}/{volume_namespace}/{volume_name}
                        let base = crate::constants::paths::volumes_dir();
                        format!("{}/{}/{}", base, volume_namespace, parts[0])
                    };
                    Some((host_path, parts[1].to_string()))
                } else {
                    None
                }
            })
            .collect();

        // Parse GPU device requests from deploy.resources.reservations.devices
        let device_requests = config
            .deploy
            .as_ref()
            .and_then(|d| d.resources.as_ref())
            .and_then(|r| r.reservations.as_ref())
            .map(|res| {
                res.devices
                    .iter()
                    .filter(|dev| {
                        dev.capabilities
                            .iter()
                            .any(|caps| caps.iter().any(|c| c == "gpu"))
                    })
                    .map(|dev| {
                        let count = match &dev.count {
                            Some(serde_yml::Value::String(s)) if s == "all" => -1,
                            Some(serde_yml::Value::Number(n)) => n.as_i64().unwrap_or(1),
                            _ => -1, // default: all GPUs
                        };
                        GpuDeviceRequest {
                            driver: dev.driver.clone().unwrap_or_default(),
                            count,
                            capabilities: dev.capabilities.clone(),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Parse resource limits from deploy.resources.limits (OFFER-0009).
        let resource_limits = config
            .deploy
            .as_ref()
            .and_then(|d| d.resources.as_ref())
            .and_then(|r| r.limits.as_ref())
            .map(|lim| ResourceLimits {
                memory_bytes: lim.memory.as_deref().and_then(parse_memory_bytes),
                nano_cpus: lim.cpus.as_deref().and_then(parse_cpus_nano),
            })
            .unwrap_or_default();

        // Parse the container healthcheck block (OFFER-0009).
        let healthcheck = config.healthcheck.as_ref().and_then(|hc| {
            if hc.test.is_empty() {
                None
            } else {
                Some(ContainerHealthcheck {
                    test: hc.test.clone(),
                    interval_ns: hc.interval.as_deref().and_then(parse_duration_ns),
                    timeout_ns: hc.timeout.as_deref().and_then(parse_duration_ns),
                    retries: hc.retries,
                    start_period_ns: hc.start_period.as_deref().and_then(parse_duration_ns),
                })
            }
        });

        ServiceTemplate {
            image: config.image,
            command: config.command,
            config_files: config.config_files,
            ports: config.ports,
            environment,
            volumes,
            compatibility: self.compatibility.clone(),
            tasks: config.tasks,
            network: config.network,
            device_requests,
            resource_limits,
            healthcheck,
        }
    }
}

// ============================================================================
// OfferingRegistry
// ============================================================================

/// Collection of all offerings
#[derive(Debug)]
pub struct OfferingRegistry {
    /// All offerings keyed by name
    pub entries: HashMap<String, Offering>,
    /// Discovered category names, sorted alphabetically
    pub categories: Vec<String>,
}

impl OfferingRegistry {
    /// Create empty registry
    pub fn empty() -> Self {
        Self {
            entries: HashMap::new(),
            categories: Vec::new(),
        }
    }

    /// Get an offering by name
    pub fn get(&self, name: &str) -> Option<&Offering> {
        self.entries.get(name)
    }

    /// Get mutable offering by name
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Offering> {
        self.entries.get_mut(name)
    }

    /// Check if an offering exists
    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// Get all offerings in a specific category
    pub fn by_category(&self, category: &str) -> Vec<&Offering> {
        self.entries
            .values()
            .filter(|e| e.category == category)
            .collect()
    }

    /// Get all offerings that support a specific mode
    pub fn by_mode(&self, mode: &OfferingMode) -> Vec<&Offering> {
        self.entries
            .values()
            .filter(|e| e.supports_mode(mode))
            .collect()
    }

    /// Get count of offerings
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Insert or update an offering
    pub fn upsert(&mut self, offering: Offering) -> bool {
        let existed = self.entries.contains_key(&offering.name);

        if !self.categories.contains(&offering.category) {
            self.categories.push(offering.category.clone());
            self.categories.sort();
        }

        self.entries.insert(offering.name.clone(), offering);
        existed
    }

    /// Load offerings from directory
    ///
    /// Scans for `.snippet.yaml` and `.manifest.yaml` files.
    pub fn load(dir: &Path) -> Result<Self> {
        let mut registry = Self::empty();

        if !dir.exists() {
            tracing::warn!(path = %dir.display(), "Manifests directory not found");
            return Ok(registry);
        }

        let categories = discover_subdirectories(dir);

        for category in &categories {
            let category_dir = dir.join(category);
            Self::load_category(&mut registry, &category_dir, category)?;
        }

        // Check root level
        Self::load_category(&mut registry, dir, "uncategorized")?;

        Ok(registry)
    }

    fn load_category(registry: &mut Self, dir: &Path, category: &str) -> Result<()> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Ok(());
        };

        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };

            // Load .snippet.yaml (managed-only)
            if name.ends_with(".snippet.yaml") {
                let offering_name = name.trim_end_matches(".snippet.yaml");
                if let Ok(offering) = Self::load_snippet_offering(dir, category, offering_name) {
                    registry.upsert(offering);
                }
            }

            // Load .manifest.yaml (full unified format)
            if name.ends_with(".manifest.yaml") {
                let offering_name = name.trim_end_matches(".manifest.yaml");
                if let Ok(offering) = Self::load_manifest_offering(dir, category, offering_name) {
                    registry.upsert(offering);
                }
            }

            // Load .adopted.yaml (adopted-only)
            if name.ends_with(".adopted.yaml") {
                let offering_name = name.trim_end_matches(".adopted.yaml");
                if let Ok(offering) = Self::load_adopted_offering(dir, category, offering_name) {
                    // Merge with existing or insert new
                    if let Some(existing) = registry.get_mut(offering_name) {
                        existing.adopted = offering.adopted;
                        existing.connection = offering.connection.or(existing.connection.clone());
                    } else {
                        registry.upsert(offering);
                    }
                }
            }
        }

        Ok(())
    }

    /// Load from .snippet.yaml (managed mode only)
    fn load_snippet_offering(dir: &Path, category: &str, name: &str) -> Result<Offering> {
        let snippet_path = dir.join(format!("{}.snippet.yaml", name));
        let snippet_yaml_raw = std::fs::read_to_string(&snippet_path)
            .with_context(|| format!("Failed to read: {}", snippet_path.display()))?;
        let snippet_yaml = crate::utils::strings::strip_bom(&snippet_yaml_raw).to_string();

        // Load optional files
        let mut compatibility = Self::load_compatibility(dir, name);
        let fm = Self::load_metadata(dir, name).unwrap_or_default();
        if let Some(gb) = fm.minimum_memory_gb {
            compatibility = Some(append_min_memory_rule(compatibility, gb));
        }
        let guidance = Self::load_guidance(dir, name);

        Ok(Offering {
            name: name.to_string(),
            category: category.to_string(),
            managed: Some(ManagedConfig {
                snippet_yaml,
                network: None,
                tasks: None,
            }),
            adopted: None,
            borrowed: None,
            metadata: fm.metadata,
            compatibility,
            guidance,
            connection: fm.connection,
            manageable_env: fm.manageable_env,
            coordination: fm.coordination,
            ceremony: fm.ceremony,
        })
    }

    /// Load from .manifest.yaml (full unified format)
    fn load_manifest_offering(dir: &Path, category: &str, name: &str) -> Result<Offering> {
        let manifest_path = dir.join(format!("{}.manifest.yaml", name));
        let content_raw = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("Failed to read: {}", manifest_path.display()))?;
        let content = crate::utils::strings::strip_bom(&content_raw);

        // Parse the manifest YAML
        let manifest: ManifestFile = serde_yml::from_str(content)
            .with_context(|| format!("Failed to parse: {}", manifest_path.display()))?;

        Ok(Offering {
            name: manifest.name.unwrap_or_else(|| name.to_string()),
            category: manifest.category.unwrap_or_else(|| category.to_string()),
            managed: manifest.managed,
            adopted: manifest.adopted,
            borrowed: manifest.borrowed,
            metadata: manifest.metadata.unwrap_or_default(),
            compatibility: manifest.compatibility,
            guidance: manifest.guidance,
            connection: manifest.connection,
            manageable_env: manifest.manageable_env,
            coordination: manifest.coordination,
            ceremony: manifest.ceremony,
        })
    }

    /// Load from .adopted.yaml (adopted mode only)
    fn load_adopted_offering(dir: &Path, category: &str, name: &str) -> Result<Offering> {
        let adopted_path = dir.join(format!("{}.adopted.yaml", name));
        let content_raw = std::fs::read_to_string(&adopted_path)
            .with_context(|| format!("Failed to read: {}", adopted_path.display()))?;
        let content = crate::utils::strings::strip_bom(&content_raw);

        let adopted_file: AdoptedFile = serde_yml::from_str(content)
            .with_context(|| format!("Failed to parse: {}", adopted_path.display()))?;
        let guidance = Self::load_adopted_guidance(dir, name);

        // Adopted offerings share frontmatter with their managed counterpart
        // (e.g., ollama.frontmatter.json applies to both ollama.snippet.yaml
        // and ollama.adopted.yaml).
        let fm = Self::load_metadata(dir, name).unwrap_or_default();

        // Adopted YAML fields override frontmatter where both exist
        let metadata = OfferingMetadata {
            description: adopted_file.description.or(fm.metadata.description),
            tags: if adopted_file.tags.as_ref().is_none_or(|t| t.is_empty()) {
                fm.metadata.tags
            } else {
                adopted_file.tags.unwrap_or_default()
            },
            icon: fm.metadata.icon,
            homepage: fm.metadata.homepage,
            documentation: fm.metadata.documentation,
            port: fm.metadata.port,
        };

        Ok(Offering {
            name: adopted_file.name.unwrap_or_else(|| name.to_string()),
            category: adopted_file
                .category
                .unwrap_or_else(|| category.to_string()),
            managed: None,
            adopted: Some(AdoptedConfig {
                detection: adopted_file.detection,
                process: adopted_file.process,
                health: adopted_file.health,
                ports: adopted_file.ports,
                control: adopted_file.control,
                default_control_level: adopted_file.default_control_level.unwrap_or_default(),
                health_check: adopted_file.health_check,
                guidance: adopted_file.guidance.or(guidance),
                connectivity: adopted_file.connectivity,
            }),
            borrowed: None,
            metadata,
            compatibility: None,
            guidance: None,
            connection: adopted_file.connection.or(fm.connection),
            manageable_env: fm.manageable_env,
            coordination: adopted_file.coordination,
            ceremony: None,
        })
    }

    fn load_compatibility(dir: &Path, name: &str) -> Option<CompatibilityRules> {
        let path = dir.join(format!("{}.compatibility.yaml", name));
        if !path.exists() {
            return None;
        }
        std::fs::read_to_string(&path).ok().and_then(|yaml| {
            let yaml = crate::utils::strings::strip_bom(&yaml);
            serde_yml::from_str(yaml).ok()
        })
    }

    fn load_metadata(dir: &Path, name: &str) -> Option<ParsedFrontmatter> {
        let path = dir.join(format!("{}.frontmatter.json", name));
        if !path.exists() {
            return None;
        }
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|json| {
                let json = crate::utils::strings::strip_bom(&json);
                serde_json::from_str::<FrontmatterFile>(json).ok()
            })
            .map(ParsedFrontmatter::from)
    }

    fn load_guidance(dir: &Path, name: &str) -> Option<String> {
        let path = dir.join(format!("{}.guidance.md", name));
        if !path.exists() {
            return None;
        }
        std::fs::read_to_string(&path).ok().map(|md| {
            let md = crate::utils::strings::strip_bom(&md);
            strip_markdown_frontmatter(md)
        })
    }

    fn load_adopted_guidance(dir: &Path, name: &str) -> Option<String> {
        let path = dir.join(format!("{}.adopted.guidance.md", name));
        if !path.exists() {
            return None;
        }
        std::fs::read_to_string(&path).ok().map(|md| {
            let md = crate::utils::strings::strip_bom(&md);
            strip_markdown_frontmatter(md)
        })
    }

    /// Load offering from raw content (for embedded assets)
    pub fn load_from_content(
        relative_path: &str,
        snippet_content: &str,
        compatibility_content: Option<&str>,
        frontmatter_content: Option<&str>,
        guidance_content: Option<&str>,
    ) -> Result<Offering> {
        let parts: Vec<&str> = relative_path.split('/').collect();
        if parts.len() < 3 {
            anyhow::bail!("Invalid manifest path: {}", relative_path);
        }

        let category = parts[1].to_string();
        let filename = parts.last().unwrap();
        let name = filename.trim_end_matches(".snippet.yaml").to_string();

        // Strip BOM from all input content
        let snippet_content = crate::utils::strings::strip_bom(snippet_content);

        let mut compatibility = compatibility_content
            .map(crate::utils::strings::strip_bom)
            .and_then(|yaml| serde_yml::from_str(yaml).ok());

        let fm = frontmatter_content
            .map(crate::utils::strings::strip_bom)
            .and_then(|json| serde_json::from_str::<FrontmatterFile>(json).ok())
            .map(ParsedFrontmatter::from)
            .unwrap_or_default();

        if let Some(gb) = fm.minimum_memory_gb {
            compatibility = Some(append_min_memory_rule(compatibility, gb));
        }

        let guidance = guidance_content
            .map(crate::utils::strings::strip_bom)
            .map(strip_markdown_frontmatter);

        Ok(Offering {
            name,
            category,
            managed: Some(ManagedConfig {
                snippet_yaml: snippet_content.to_string(),
                network: None,
                tasks: None,
            }),
            adopted: None,
            borrowed: None,
            metadata: fm.metadata,
            compatibility,
            guidance,
            connection: fm.connection,
            manageable_env: fm.manageable_env,
            coordination: fm.coordination,
            ceremony: fm.ceremony,
        })
    }
}

// ============================================================================
// File Parsing Structures
// ============================================================================

/// Full manifest file format (.manifest.yaml)
#[derive(Debug, Deserialize)]
struct ManifestFile {
    name: Option<String>,
    category: Option<String>,
    managed: Option<ManagedConfig>,
    adopted: Option<AdoptedConfig>,
    borrowed: Option<BorrowedConfig>,
    metadata: Option<OfferingMetadata>,
    compatibility: Option<CompatibilityRules>,
    guidance: Option<String>,
    connection: Option<ConnectionProfile>,
    manageable_env: Option<ManageableEnv>,
    #[serde(default)]
    coordination: CoordinationMode,
    #[serde(default)]
    ceremony: Option<CeremonyPolicy>,
}

/// Adopted-only file format (.adopted.yaml)
#[derive(Debug, Deserialize)]
struct AdoptedFile {
    name: Option<String>,
    category: Option<String>,
    description: Option<String>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    detection: OsDetectionRules,
    // DETECT-0001: process-based detection (new format)
    process: Option<ProcessDetection>,
    health: Option<HealthVerification>,
    ports: Option<PortDetectionConfig>,
    control: Option<ControlConfig>,
    default_control_level: Option<AdoptedControlLevel>,
    health_check: Option<HealthConfig>,
    guidance: Option<String>,
    connectivity: Option<ConnectivityConfig>,
    connection: Option<ConnectionProfile>,
    #[serde(default)]
    coordination: CoordinationMode,
}

/// Frontmatter file format (.frontmatter.json)
#[derive(Debug, Deserialize)]
struct FrontmatterFile {
    description: Option<String>,
    tags: Option<Vec<String>>,
    icon: Option<String>,
    homepage: Option<String>,
    documentation: Option<String>,
    port: Option<u16>,
    connection: Option<ConnectionProfile>,
    #[serde(default)]
    coordination: CoordinationMode,
    manageable_env: Option<ManageableEnv>,
    /// Snapshot ceremony policy (ORCH-0041).
    #[serde(default)]
    ceremony: Option<CeremonyPolicy>,
    /// Recommended minimum RAM in GB; synthesized into a warn-only rule (COMPAT-0003).
    #[serde(default)]
    minimum_memory_gb: Option<u32>,
}

/// Frontmatter resolved into the fields an Offering consumes.
#[derive(Default)]
struct ParsedFrontmatter {
    metadata: OfferingMetadata,
    connection: Option<ConnectionProfile>,
    coordination: CoordinationMode,
    manageable_env: Option<ManageableEnv>,
    ceremony: Option<CeremonyPolicy>,
    minimum_memory_gb: Option<u32>,
}

impl From<FrontmatterFile> for ParsedFrontmatter {
    fn from(fm: FrontmatterFile) -> Self {
        Self {
            metadata: OfferingMetadata {
                description: fm.description,
                tags: fm.tags.unwrap_or_default(),
                icon: fm.icon,
                homepage: fm.homepage,
                documentation: fm.documentation,
                port: fm.port,
            },
            connection: fm.connection,
            coordination: fm.coordination,
            manageable_env: fm.manageable_env,
            ceremony: fm.ceremony,
            minimum_memory_gb: fm.minimum_memory_gb,
        }
    }
}

/// Append a synthesized `warn_only` RAM rule from a `minimum_memory_gb` hint (COMPAT-0003).
///
/// Appended last so a hand-authored deny still takes first-match precedence.
fn append_min_memory_rule(existing: Option<CompatibilityRules>, gb: u32) -> CompatibilityRules {
    let mut rules = existing.unwrap_or(CompatibilityRules {
        compatibility_rules: Vec::new(),
        post_install_healthcheck: None,
    });
    rules.compatibility_rules.push(CompatibilityRule {
        name: "minimum-memory".to_string(),
        when: vec![format!("host.ram.total.mb < {}", gb as u64 * 1024)],
        reason: format!("Recommended minimum is {gb}GB RAM"),
        suggestion: Some("Increase stone memory for reliable operation".to_string()),
        fallback: None,
        warn_only: true,
        continue_eval: false,
    });
    rules
}

/// Strip YAML frontmatter from markdown content
pub fn strip_markdown_frontmatter(content: &str) -> String {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return content.to_string();
    }
    let after_first = &trimmed[3..];
    if let Some(end_pos) = after_first.find("\n---") {
        after_first[end_pos + 4..]
            .trim_start_matches('\n')
            .to_string()
    } else {
        content.to_string()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_offering(dir: &Path, category: &str, name: &str) {
        let cat_dir = dir.join(category);
        fs::create_dir_all(&cat_dir).unwrap();

        fs::write(
            cat_dir.join(format!("{}.snippet.yaml", name)),
            format!("image: {}:latest\nports:\n  default: [8080, 8080]", name),
        )
        .unwrap();

        fs::write(
            cat_dir.join(format!("{}.frontmatter.json", name)),
            format!(
                r#"{{"description": "Test {} service", "tags": ["test"]}}"#,
                name
            ),
        )
        .unwrap();
    }

    #[test]
    fn test_empty_registry() {
        let registry = OfferingRegistry::empty();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_load_offerings() {
        let temp = TempDir::new().unwrap();
        create_test_offering(temp.path(), "data", "mongodb");
        create_test_offering(temp.path(), "cache", "redis");

        let registry = OfferingRegistry::load(temp.path()).unwrap();

        assert_eq!(registry.len(), 2);
        assert!(registry.contains("mongodb"));
        assert!(registry.contains("redis"));
    }

    #[test]
    fn test_mode_support() {
        let offering = Offering {
            name: "test".to_string(),
            category: "data".to_string(),
            managed: Some(ManagedConfig {
                snippet_yaml: "image: test".to_string(),
                network: None,
                tasks: None,
            }),
            adopted: None,
            borrowed: None,
            metadata: OfferingMetadata::default(),
            compatibility: None,
            guidance: None,
            connection: None,
            manageable_env: None,
            coordination: CoordinationMode::default(),
            ceremony: None,
        };

        assert!(offering.supports_mode(&OfferingMode::Managed));
        assert!(!offering.supports_mode(&OfferingMode::Adopted));
        assert!(!offering.supports_mode(&OfferingMode::Borrowed));
        assert_eq!(offering.modes().len(), 1);
    }

    #[test]
    fn test_by_mode() {
        let mut registry = OfferingRegistry::empty();

        registry.upsert(Offering {
            name: "mongodb".to_string(),
            category: "data".to_string(),
            managed: Some(ManagedConfig {
                snippet_yaml: "image: mongo".to_string(),
                network: None,
                tasks: None,
            }),
            adopted: None,
            borrowed: None,
            metadata: OfferingMetadata::default(),
            compatibility: None,
            guidance: None,
            connection: None,
            manageable_env: None,
            coordination: CoordinationMode::Elected,
            ceremony: None,
        });

        registry.upsert(Offering {
            name: "ollama".to_string(),
            category: "ai".to_string(),
            managed: None,
            adopted: Some(AdoptedConfig {
                detection: OsDetectionRules {
                    windows: None,
                    linux: None,
                    macos: None,
                },
                process: None,
                health: None,
                ports: None,
                control: None,
                default_control_level: AdoptedControlLevel::default(),
                health_check: None,
                guidance: None,
                connectivity: None,
            }),
            borrowed: None,
            metadata: OfferingMetadata::default(),
            compatibility: None,
            guidance: None,
            connection: None,
            manageable_env: None,
            coordination: CoordinationMode::default(),
            ceremony: None,
        });

        assert_eq!(registry.by_mode(&OfferingMode::Managed).len(), 1);
        assert_eq!(registry.by_mode(&OfferingMode::Adopted).len(), 1);
        assert_eq!(registry.by_mode(&OfferingMode::Borrowed).len(), 0);
    }

    #[test]
    fn test_frontmatter_connection_propagates() {
        let temp = TempDir::new().unwrap();
        let cat_dir = temp.path().join("data");
        fs::create_dir_all(&cat_dir).unwrap();

        fs::write(
            cat_dir.join("mongodb.snippet.yaml"),
            "image: mongodb:latest\nports:\n  default: [27017, 27017]",
        )
        .unwrap();
        fs::write(
            cat_dir.join("mongodb.frontmatter.json"),
            r#"{"description":"MongoDB","port":27017,"connection":{"uri_template":"mongodb://{host}:{port}"}}"#,
        )
        .unwrap();

        let registry = OfferingRegistry::load(temp.path()).unwrap();
        let mongo = registry.get("mongodb").unwrap();
        assert_eq!(
            mongo
                .connection
                .as_ref()
                .and_then(|c| c.uri_template.as_deref()),
            Some("mongodb://{host}:{port}")
        );
    }

    #[test]
    fn parse_memory_sizes() {
        assert_eq!(parse_memory_bytes("512m"), Some(512 * 1024 * 1024));
        assert_eq!(parse_memory_bytes("2g"), Some(2 * 1024 * 1024 * 1024));
        assert_eq!(parse_memory_bytes("1073741824"), Some(1_073_741_824));
        assert_eq!(parse_memory_bytes("nonsense"), None);
        assert_eq!(parse_memory_bytes(""), None);
    }

    #[test]
    fn parse_cpu_counts() {
        assert_eq!(parse_cpus_nano("1.5"), Some(1_500_000_000));
        assert_eq!(parse_cpus_nano("2"), Some(2_000_000_000));
        assert_eq!(parse_cpus_nano("0"), None);
        assert_eq!(parse_cpus_nano("x"), None);
        assert_eq!(parse_cpus_nano("inf"), None);
        assert_eq!(parse_cpus_nano("nan"), None);
    }

    #[test]
    fn parse_durations() {
        assert_eq!(parse_duration_ns("30s"), Some(30_000_000_000));
        assert_eq!(parse_duration_ns("500ms"), Some(500_000_000));
        assert_eq!(parse_duration_ns("1m30s"), Some(90_000_000_000));
        assert_eq!(parse_duration_ns("bad"), None);
    }

    #[test]
    fn snippet_parses_limits_and_healthcheck() {
        let snippet = "image: ghcr.io/flaresolverr/flaresolverr:v3.5.0\n\
ports:\n  default: [8191, 8191]\n\
healthcheck:\n  test: [CMD, curl, -fsS, \"http://localhost:8191/health\"]\n  interval: 30s\n  timeout: 10s\n  retries: 5\n  start_period: 30s\n\
deploy:\n  resources:\n    limits:\n      memory: 2g\n      cpus: \"1.5\"\n";
        let offering = Offering {
            name: "flaresolverr".to_string(),
            category: "proxy".to_string(),
            managed: Some(ManagedConfig {
                snippet_yaml: snippet.to_string(),
                network: None,
                tasks: None,
            }),
            adopted: None,
            borrowed: None,
            metadata: OfferingMetadata::default(),
            compatibility: None,
            guidance: None,
            connection: None,
            manageable_env: None,
            coordination: CoordinationMode::default(),
            ceremony: None,
        };
        let template = offering.parse_template().unwrap();
        assert_eq!(
            template.resource_limits.memory_bytes,
            Some(2 * 1024 * 1024 * 1024)
        );
        assert_eq!(template.resource_limits.nano_cpus, Some(1_500_000_000));
        let hc = template.healthcheck.expect("healthcheck parsed");
        assert_eq!(hc.test.first().map(String::as_str), Some("CMD"));
        assert_eq!(hc.interval_ns, Some(30_000_000_000));
        assert_eq!(hc.retries, Some(5));
    }

    #[test]
    fn minimum_memory_gb_synthesizes_warn_rule() {
        let temp = TempDir::new().unwrap();
        let cat = temp.path().join("proxy");
        fs::create_dir_all(&cat).unwrap();
        fs::write(
            cat.join("svc.snippet.yaml"),
            "image: x:1\nports:\n  default: [80, 80]",
        )
        .unwrap();
        fs::write(
            cat.join("svc.frontmatter.json"),
            r#"{"description":"x","minimum_memory_gb":2}"#,
        )
        .unwrap();
        let registry = OfferingRegistry::load(temp.path()).unwrap();
        let svc = registry.get("svc").unwrap();
        let rules = svc.compatibility.as_ref().expect("compat synthesized");
        assert!(rules.compatibility_rules.iter().any(|r| {
            r.name == "minimum-memory" && r.warn_only && r.when.iter().any(|w| w.contains("2048"))
        }));
    }
}
