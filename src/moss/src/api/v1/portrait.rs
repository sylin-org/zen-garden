//! Stone Portrait - Living landing page for Moss daemon
//!
//! Provides a single-page application that displays a stone's identity,
//! resources, offerings, adapters, and visible network topology.
//!
//! See: docs/decisions/PORTRAIT-0001-stone-landing-page.md

use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{Html, IntoResponse},
    Json,
};
use serde::Serialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use garden_common::storage::SeedBankRole;

use crate::app_state::AppState;
use crate::cli;
use crate::domain::topology;

/// Embedded HTML template (baked into binary at compile time)
const PORTRAIT_HTML: &str = include_str!("../../../assets/portrait.html");

/// Stone identity section
#[derive(Debug, Clone, Serialize)]
pub struct PortraitIdentity {
    pub id: String,
    pub name: String,
    pub role: String,
    pub version: String,
    pub color: String,
    pub endpoint: String,
    /// Operating system family ("windows", "linux", "macos")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_family: Option<String>,
    /// Friendly OS version/details (e.g., "Debian 13", "11 Pro")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_version: Option<String>,
    /// System (stone) uptime - how long the machine has been running
    pub uptime: String,
    /// Moss daemon uptime - how long the daemon has been running
    pub moss_uptime: String,
    /// Hardware manufacturer (e.g., "Dell Inc.")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    /// Hardware model (e.g., "Wyse 5070")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// CPU metrics for foundation
#[derive(Debug, Clone, Serialize)]
pub struct FoundationCpu {
    pub cores: usize,
    pub percent: f32,
}

/// Memory metrics for foundation
#[derive(Debug, Clone, Serialize)]
pub struct FoundationMemory {
    pub total_gb: f32,
    pub used_gb: f32,
    pub percent: f32,
}

/// Disk metrics for foundation
#[derive(Debug, Clone, Serialize)]
pub struct FoundationDisk {
    pub total_gb: u64,
    pub used_gb: u64,
    pub percent: f32,
}

/// Network metrics for foundation
#[derive(Debug, Clone, Serialize)]
pub struct FoundationNetwork {
    /// Total bytes received across all interfaces
    pub rx_bytes: u64,
    /// Total bytes transmitted across all interfaces
    pub tx_bytes: u64,
    /// Human-readable received bytes (e.g., "1.5 GB")
    pub rx_friendly: String,
    /// Human-readable transmitted bytes (e.g., "500 MB")
    pub tx_friendly: String,
}

/// Foundation metrics section
#[derive(Debug, Clone, Serialize)]
pub struct PortraitFoundation {
    pub cpu: FoundationCpu,
    pub memory: FoundationMemory,
    pub disk: FoundationDisk,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<FoundationNetwork>,
}

/// Offering entry
#[derive(Debug, Clone, Serialize)]
pub struct PortraitOffering {
    pub name: String,
    pub container: Option<String>,
    pub port: u16,
    pub status: String,
    pub health: String,
    /// Formatted capabilities string (e.g., "llama2, mistral +10")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<String>,
}

/// Companion (adapter) entry
#[derive(Debug, Clone, Serialize)]
pub struct PortraitCompanion {
    pub id: String,
    pub name: String,
    pub description: String,
    pub port: Option<u16>,
    pub status: String,
}

/// Seed bank entry
#[derive(Debug, Clone, Serialize)]
pub struct PortraitSeedBank {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub used_gb: f32,
    pub capacity_gb: f32,
    pub filesystem: String,
    pub visibility: String,
    pub role: SeedBankRole,
    pub pinned: bool,
    pub encrypted: bool,
    pub roaming: bool,
    pub online: bool,
}

/// Candidate device entry (USB drive ready for preparation)
#[derive(Debug, Clone, Serialize)]
pub struct PortraitCandidate {
    /// Device path (e.g., "/dev/sdb1")
    pub device: String,
    /// Device label if available (e.g., "SANDISK_32GB")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Capacity in GB
    pub capacity_gb: f32,
    /// Device state (e.g., "empty", "unformatted")
    pub state: String,
    /// Mount path if mounted
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mount_path: Option<String>,
}

/// Horizon stone entry
#[derive(Debug, Clone, Serialize)]
pub struct HorizonStone {
    pub name: String,
    pub endpoint: String,
    pub status: String,
    pub health: String,
    pub color: String,
    /// Operating system family ("windows", "linux", "macos")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_family: Option<String>,
    /// Friendly OS version/details (e.g., "Debian 13", "11 Pro")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_version: Option<String>,
    /// Number of CPU cores (if known)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_cores: Option<usize>,
    /// Total memory in GB (if known)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_gb: Option<u64>,
    /// Number of running services
    pub service_count: usize,
    /// Hardware manufacturer (e.g., "Dell Inc.")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    /// Hardware model (e.g., "Wyse 5070")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Notification tags for cross-stone awareness (opportunity, attention)
    /// Empty if stone has nothing noteworthy.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Whether the stone has at least one locally connected seed bank (remote view)
    pub has_seed_banks: bool,
}

/// Horizon section
#[derive(Debug, Clone, Serialize)]
pub struct PortraitHorizon {
    pub count: usize,
    pub seed_bank_count: usize,
    pub stones: Vec<HorizonStone>,
}

/// Pond status summary for portrait
#[derive(Debug, Clone, Serialize)]
pub struct PortraitPond {
    /// Whether a pond has been initialized on this stone
    pub active: bool,
    /// Whether the CA is currently locked
    pub locked: bool,
    /// Pond name (if active)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Number of enrolled stones
    pub stone_count: usize,
    /// Trust profile
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
}

/// Complete portrait response
#[derive(Debug, Clone, Serialize)]
pub struct PortraitResponse {
    pub identity: PortraitIdentity,
    pub foundation: PortraitFoundation,
    pub offerings: Vec<PortraitOffering>,
    pub seed_banks: Vec<PortraitSeedBank>,
    /// Candidate devices ready for seed bank preparation (hopeful state)
    pub candidates: Vec<PortraitCandidate>,
    pub companions: Vec<PortraitCompanion>,
    pub pond: PortraitPond,
    pub horizon: PortraitHorizon,
}

/// Derive a unique HSL color from stone ID (delegates to garden_common)
fn derive_stone_color(stone_id: &str) -> String {
    garden_common::utils::derive_stone_color(stone_id)
}

/// Parse normalized OS family and friendly version from runtime OS text.
///
/// Examples:
/// - "windows/Windows 11 Pro" -> "windows"
/// - "linux/Ubuntu 24.04" -> "linux"
/// - "darwin/macOS 15" -> "macos"
fn os_info_from_runtime(runtime_os: &str) -> (Option<String>, Option<String>) {
    let raw = runtime_os.trim();
    if raw.is_empty() {
        return (None, None);
    }

    let family_raw = raw
        .split('/')
        .next()
        .unwrap_or(raw)
        .trim()
        .to_ascii_lowercase();
    if family_raw.is_empty() {
        return (None, None);
    }

    let family = match family_raw.as_str() {
        "windows" | "win32" | "win" => "windows".to_string(),
        "linux" | "gnu/linux" => "linux".to_string(),
        "macos" | "darwin" | "osx" | "mac" => "macos".to_string(),
        other => other.to_string(),
    };

    let version = raw
        .split_once('/')
        .map(|(_, details)| details.trim())
        .filter(|details| !details.is_empty())
        .and_then(|details| normalize_os_details(&family, details));

    (Some(family), version)
}

fn normalize_os_details(family: &str, details: &str) -> Option<String> {
    let mut value = details.trim().to_string();
    if value.is_empty() {
        return None;
    }

    match family {
        "windows" => {
            let lower = value.to_ascii_lowercase();
            if lower.starts_with("microsoft ") {
                value = value[10..].trim().to_string();
            }
            let lower = value.to_ascii_lowercase();
            if lower.starts_with("windows ") {
                value = value[8..].trim().to_string();
            }

            if value.is_empty() {
                return None;
            }
            Some(value)
        }
        "linux" => {
            // Keep distro + version while removing noisy qualifiers.
            value = value
                .replace("GNU/Linux", "")
                .replace("gnu/linux", "")
                .replace("Linux", "")
                .replace("linux", "");

            if let Some(idx) = value.find('(') {
                value.truncate(idx);
            }

            let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
            if compact.is_empty() {
                None
            } else {
                Some(compact)
            }
        }
        _ => Some(value),
    }
}

/// Format sub-capabilities into a compact display string.
///
/// Shows up to 2 capability names, with overflow count.
/// Examples:
///   - ["llama2"] -> "llama2"
///   - ["llama2", "mistral"] -> "llama2, mistral"
///   - ["llama2", "mistral", "phi3", ...] -> "llama2, mistral +10"
fn format_capabilities(sub_capabilities: &[garden_common::SubCapability]) -> Option<String> {
    // Collect all items across all capability types
    let all_items: Vec<&str> = sub_capabilities
        .iter()
        .flat_map(|cap| cap.items.iter().map(|s| s.as_str()))
        .collect();

    if all_items.is_empty() {
        return None;
    }

    const MAX_VISIBLE: usize = 2;
    let total = all_items.len();

    if total <= MAX_VISIBLE {
        Some(all_items.join(", "))
    } else {
        let visible: Vec<&str> = all_items.into_iter().take(MAX_VISIBLE).collect();
        let overflow = total - MAX_VISIBLE;
        Some(format!("{} +{}", visible.join(", "), overflow))
    }
}

/// GET /
///
/// Returns the portrait SPA HTML page.
/// The page uses Alpine.js to poll /api/v1/stone/portrait for data.
pub async fn get_portrait_page() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        Html(PORTRAIT_HTML),
    )
}

/// GET /api/v1/stone/portrait
///
/// Returns JSON data for the portrait SPA.
/// Aggregates identity, foundation metrics, offerings, adapters, and topology.
///
/// PERF: This endpoint MUST only read from cached AppState data - NO I/O operations.
/// All metrics are collected by background tasks and cached in AppState.
/// Target latency: <10ms. Any I/O here will cause latency regression.
pub async fn get_portrait_data(
    State(state): State<AppState>,
) -> Result<Json<PortraitResponse>, StatusCode> {
    // === Identity ===
    let stone_color = derive_stone_color(&state.stone_id);

    // Role is always STONE for now — multi-role (LANTERN, CORNERSTONE) requires
    // a role field in AppState once Pond/elections are implemented.
    let role = "STONE".to_string();

    // Build endpoint URL
    let endpoint = format!("http://{}:{}", state.stone_name, state.api_port);

    // Get uptime from resources
    let uptime = {
        let resources = state.system_resources.read().await;
        resources
            .as_ref()
            .map(|r| r.uptime_friendly.clone())
            .unwrap_or_else(|| "–".into())
    };

    // Get Moss daemon uptime
    let moss_uptime = {
        let secs = state.start_time.elapsed().as_secs();
        garden_common::utils::format_uptime(secs)
    };

    // Get hardware manufacturer/model and local OS info from capabilities
    let (manufacturer, model, os_family, os_version) = {
        let caps = state.capabilities.read().await;
        if let Some(ref c) = *caps {
            let (os_family, os_version) = c
                .runtime
                .as_ref()
                .map(|runtime| os_info_from_runtime(&runtime.os))
                .unwrap_or((None, None));
            (
                c.hardware.system_manufacturer.clone(),
                c.hardware.system_product.clone(),
                os_family,
                os_version,
            )
        } else {
            (None, None, None, None)
        }
    };

    let identity = PortraitIdentity {
        id: state.stone_id.clone(),
        name: state.stone_name.clone(),
        role,
        version: cli::VERSION.to_string(),
        color: stone_color,
        endpoint,
        os_family,
        os_version,
        uptime,
        moss_uptime,
        manufacturer,
        model,
    };

    // === Foundation (system resources) ===
    // NOTE: All metrics read from cache - no I/O allowed here
    let foundation = {
        let resources = state.system_resources.read().await;

        // Read network metrics from cache (populated by health_monitor task)
        let network = {
            let cached = state.network_metrics_cache.read().await;
            cached.as_ref().map(|m| FoundationNetwork {
                rx_bytes: m.total_rx_bytes,
                tx_bytes: m.total_tx_bytes,
                rx_friendly: m.total_rx_friendly.clone(),
                tx_friendly: m.total_tx_friendly.clone(),
            })
        };

        if let Some(ref res) = *resources {
            // Find primary disk (root mount or first available)
            let primary_disk = res
                .storage
                .iter()
                .find(|d| d.mount_point == "/" || d.mount_point == "C:\\")
                .or_else(|| res.storage.first());

            let disk = if let Some(d) = primary_disk {
                FoundationDisk {
                    total_gb: d.total_gb,
                    used_gb: d.used_gb,
                    percent: d.used_percent,
                }
            } else {
                FoundationDisk {
                    total_gb: 0,
                    used_gb: 0,
                    percent: 0.0,
                }
            };

            PortraitFoundation {
                cpu: FoundationCpu {
                    cores: res.cpu.cores,
                    percent: res.cpu.usage_percent,
                },
                memory: FoundationMemory {
                    total_gb: res.memory.total_bytes as f32 / 1024.0 / 1024.0 / 1024.0,
                    used_gb: res.memory.used_bytes as f32 / 1024.0 / 1024.0 / 1024.0,
                    percent: res.memory.used_percent,
                },
                disk,
                network,
            }
        } else {
            // No metrics yet - return placeholder
            PortraitFoundation {
                cpu: FoundationCpu {
                    cores: 0,
                    percent: 0.0,
                },
                memory: FoundationMemory {
                    total_gb: 0.0,
                    used_gb: 0.0,
                    percent: 0.0,
                },
                disk: FoundationDisk {
                    total_gb: 0,
                    used_gb: 0,
                    percent: 0.0,
                },
                network,
            }
        }
    };

    // === Offerings (managed containers + adopted native services) ===
    let offerings = {
        let offerings_guard = state.offerings.read().await;
        offerings_guard
            .iter()
            .map(|o| {
                let status_str = match o.status {
                    garden_common::OfferingStatus::Running => "running",
                    garden_common::OfferingStatus::Stopped => "stopped",
                    garden_common::OfferingStatus::Installing => "installing",
                    garden_common::OfferingStatus::Maintenance => "maintenance",
                    garden_common::OfferingStatus::Degraded => "degraded",
                    garden_common::OfferingStatus::Unknown => "unknown",
                };
                let health_str = match o.health {
                    garden_common::ServiceHealthStatus::Healthy => "healthy",
                    garden_common::ServiceHealthStatus::Degraded => "degraded",
                    garden_common::ServiceHealthStatus::Offline => "offline",
                };

                PortraitOffering {
                    name: o.name.clone(),
                    // Managed offerings have containers, adopted/borrowed don't
                    container: if o.is_managed() {
                        Some(o.offering.clone())
                    } else {
                        None
                    },
                    port: o.location.port,
                    status: status_str.to_string(),
                    health: health_str.to_string(),
                    capabilities: format_capabilities(&o.sub_capabilities),
                }
            })
            .collect()
    };

    // === Seed Banks ===
    // STORAGE-0007: Read from unified lifecycle objects (single source of truth).
    let seed_banks = {
        let banks = state.seed_banks.read().await;
        banks
            .values()
            .map(|bank| PortraitSeedBank {
                id: bank.id.clone(),
                short_id: bank.short_id.clone(),
                name: bank.name.clone(),
                used_gb: bank.storage.used_bytes as f32 / 1024.0 / 1024.0 / 1024.0,
                capacity_gb: bank.storage.capacity_bytes as f32 / 1024.0 / 1024.0 / 1024.0,
                filesystem: bank.storage.filesystem.clone(),
                visibility: bank.visibility.to_string(),
                role: bank.role,
                pinned: bank.is_pinned(),
                encrypted: bank.encrypted,
                roaming: bank.roaming,
                online: bank.storage.health.is_usable(),
            })
            .collect()
    };

    // === Candidates (hopeful state - devices ready to become seed banks) ===
    // NOTE: Read from cache - populated by metrics_collector task + storage events
    let candidates = {
        let cached = state.candidates_cache.read().await;
        cached
            .iter()
            .map(|c| PortraitCandidate {
                device: c.device.clone(),
                label: c.label.clone(),
                capacity_gb: c.capacity_bytes as f32 / 1024.0 / 1024.0 / 1024.0,
                state: format!("{:?}", c.state).to_lowercase(),
                mount_path: c.mount_path.clone(),
            })
            .collect()
    };

    // === Companions (adapters) ===
    let companions = {
        let adapters = state.companion_registry.list().await;
        let mut result = Vec::new();
        for adapter in adapters {
            let running = state.companion_registry.is_running(&adapter.id).await;
            result.push(PortraitCompanion {
                id: adapter.manifest.id.clone(),
                name: adapter.manifest.name.clone(),
                description: adapter.manifest.description.clone(),
                port: adapter.port(),
                status: if running {
                    "running".into()
                } else {
                    "stopped".into()
                },
            });
        }
        result
    };

    // === Horizon (visible stones) ===
    let horizon = {
        let visible_stones = topology::get_all_stones(&state.topology_cache).await;
        let storage_cache = state.storage_cache.read().await;
        let stones: Vec<HorizonStone> = visible_stones
            .iter()
            .filter(|entry| entry.stone_id != state.stone_id) // Exclude self
            .map(|entry| {
                // Extract resource hints from capabilities
                let caps = entry.capabilities.as_ref();
                let cpu_cores = caps.map(|c| c.hardware.cpu.cores);
                let memory_gb = caps.map(|c| c.hardware.memory.total_mb / 1024);
                let (os_family, os_version) = caps
                    .and_then(|c| c.runtime.as_ref())
                    .map(|runtime| os_info_from_runtime(&runtime.os))
                    .unwrap_or((None, None));
                let manufacturer = caps.and_then(|c| c.hardware.system_manufacturer.clone());
                let model = caps.and_then(|c| c.hardware.system_product.clone());
                let service_count = entry.services.len();
                let has_seed_banks = storage_cache
                    .get_beacon(&entry.stone_id)
                    .map(|b| !b.seed_banks.is_empty())
                    .unwrap_or(false);

                HorizonStone {
                    name: entry.stone_name.clone(),
                    endpoint: entry.address.http_base(),
                    status: entry.status.to_string(),
                    health: entry.health.clone(),
                    color: derive_stone_color(&entry.stone_id),
                    os_family,
                    os_version,
                    cpu_cores,
                    memory_gb,
                    service_count,
                    manufacturer,
                    model,
                    tags: entry.tags.clone(),
                    has_seed_banks,
                }
            })
            .collect();

        let seed_bank_count = visible_stones
            .iter()
            .filter(|entry| entry.stone_id != state.stone_id)
            .map(|entry| {
                storage_cache
                    .get_beacon(&entry.stone_id)
                    .map(|b| b.seed_banks.len())
                    .unwrap_or(0)
            })
            .sum();

        PortraitHorizon {
            count: stones.len(),
            seed_bank_count,
            stones,
        }
    };

    // === Pond ===
    let pond = {
        let active = state.pond_active.load(std::sync::atomic::Ordering::Relaxed);
        let name = state.pond.name().await;
        // Stone count from horizon (discovered peers + self if enrolled)
        let stone_count = if active { horizon.count.max(1) } else { 0 };
        PortraitPond {
            active,
            locked: !active && state.pond.enrolled(),
            name,
            stone_count,
            profile: None, // requires certmesh I/O, omitted for portrait
        }
    };

    Ok(Json(PortraitResponse {
        identity,
        foundation,
        offerings,
        seed_banks,
        candidates,
        companions,
        pond,
        horizon,
    }))
}

/// GET /api/v1/stone/portrait/guidance
///
/// Returns compiled markdown containing all offering guidance.
/// Each offering's guidance is separated by a header with the offering name.
/// Supports HTTP caching via ETag header.
///
/// Returns 204 No Content if no offerings have guidance.
pub async fn get_portrait_guidance(State(state): State<AppState>) -> axum::response::Response {
    use axum::body::Body;
    use axum::response::Response;

    // Collect all guidance from installed offerings (managed + adopted)
    let guidance_sections: Vec<(String, String)> = {
        let offerings = state.offerings.read().await;
        offerings
            .iter()
            .filter_map(|o| {
                o.managed_data()
                    .and_then(|m| m.guidance.as_ref())
                    .or_else(|| o.adopted_data().and_then(|a| a.guidance.as_ref()))
                    .map(|g| (o.name.clone(), g.content.clone()))
            })
            .collect()
    };

    // Return 204 if no guidance available
    if guidance_sections.is_empty() {
        return Response::builder()
            .status(StatusCode::NO_CONTENT)
            .header(header::CONTENT_TYPE, "text/markdown; charset=utf-8")
            .body(Body::empty())
            .unwrap();
    }

    // Build combined markdown document
    let mut markdown = String::new();
    for (i, (name, content)) in guidance_sections.iter().enumerate() {
        if i > 0 {
            markdown.push_str("\n\n---\n\n");
        }
        // Use offering name as section header (capitalize first letter)
        let display_name = name
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_default()
            + &name[1..];
        markdown.push_str(&format!("# {}\n\n", display_name));
        markdown.push_str(content);
    }

    // Generate ETag from content hash
    let mut hasher = DefaultHasher::new();
    markdown.hash(&mut hasher);
    let etag = format!("\"{}\"", hasher.finish());

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/markdown; charset=utf-8")
        .header(header::ETAG, etag)
        .body(Body::from(markdown))
        .unwrap()
}
