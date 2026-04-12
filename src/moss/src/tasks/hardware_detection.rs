//! Hardware capability detection background task
//!
//! Progressive detection strategy:
//! 1. Fast Phase: CPU, memory, disk (~100ms) - results available immediately
//! 2. Slow Phase: GPU detection (2-6 seconds on Windows) - runs in background
//! 3. Final Phase: Storage, OS, kernel, swap - completes detection
//!
//! This non-blocking approach allows the daemon to start serving requests
//! while GPU detection completes in the background.

use crate::AppState;
use crate::infra::save_capabilities_cache;
use garden_common::console;
use garden_common::resources::system as resources;
use garden_common::{
    AiCapabilitiesSummary, CpuCapabilities, DetectionStatus, DiskCapabilities, GpuInfo,
    HardwareCapabilities, HardwareInventory, MemoryCapabilities, RuntimeInfo,
};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Background hardware capability detection with progressive results
///
/// This task should be spawned with tokio::spawn() at daemon startup.
/// It runs once, populating the capabilities cache in multiple phases:
///
/// # Progressive Detection Strategy
/// 1. **Phase 1: CPU (Fast, ~100ms)**
///    - Detects CPU model, cores, features, architecture
///    - Updates cache immediately with partial results
///    - Daemon can start serving requests
///
/// 2. **Phase 2: GPU (Slow, 2-6 seconds on Windows)**
///    - Runs DXDiag or equivalent GPU detection
///    - May take several seconds, especially on Windows
///    - Updates cache with complete GPU data
///
/// 3. **Phase 3: System Info**
///    - Storage, OS version, kernel, swap
///    - Completes detection
///    - Persists final results to disk
///
/// 4. **Post-Detection: Offerings Re-evaluation**
///    - Rebuilds offerings index with detected hardware
///    - Updates compatibility warnings (no GPU → no Ollama, etc.)
///
/// # Non-Blocking
/// This function is designed to run in the background. The daemon doesn't
/// wait for it to complete before starting the HTTP server.
///
/// # Parameters
/// - `stone_name`: Stone identifier
/// - `caps_arc`: Shared capabilities cache (updated progressively)
/// - `console`: Console printer for status updates
/// - `state`: Application state (for offerings re-evaluation)
///
/// # Example
/// ```rust,ignore
/// let stone_name = "stone-01".to_string();
/// let caps_arc = Arc::new(RwLock::new(None));
/// let console = Arc::new(ConsolePrinter::new());
/// let state_clone = state.clone();
///
/// tokio::spawn(async move {
///     detect_capabilities_background(
///         stone_name,
///         caps_arc,
///         console,
///         state_clone,
///     ).await;
/// });
/// // Daemon continues, capabilities populated progressively
/// ```
///
/// Build AI capabilities summary from GPU list
fn build_ai_capabilities_summary(
    gpus: &[GpuInfo],
    detection_complete: bool,
) -> AiCapabilitiesSummary {
    let mut runtimes: HashSet<String> = HashSet::new();
    let mut vendors: HashSet<String> = HashSet::new();
    let mut total_vram_mb: u64 = 0;
    let mut gpu_count: usize = 0;

    let ai_runtime_names: HashSet<&str> = ["cuda", "rocm", "directml", "openvino"]
        .into_iter()
        .collect();

    for gpu in gpus {
        // Derive runtimes from capabilities
        for cap in &gpu.capabilities {
            let lower = cap.to_lowercase();
            if ai_runtime_names.contains(lower.as_str()) {
                runtimes.insert(lower);
            }
        }

        // Collect unique vendors (lowercase for consistency)
        vendors.insert(gpu.vendor.to_lowercase());

        // Sum VRAM
        if let Some(vram) = gpu.vram_mb {
            total_vram_mb += vram;
            gpu_count += 1;
        }
    }

    // Convert HashSets to sorted Vecs for consistent output
    let mut runtimes_vec: Vec<String> = runtimes.into_iter().collect();
    runtimes_vec.sort();

    let mut vendors_vec: Vec<String> = vendors.into_iter().collect();
    vendors_vec.sort();

    AiCapabilitiesSummary {
        runtimes: runtimes_vec,
        vendors: vendors_vec,
        total_vram_mb,
        gpu_count,
        detection_complete,
    }
}

/// Merge fresh GPU detection results with cached GPU data.
///
/// Matches GPUs by (vendor, model) pair (case-insensitive). For matched pairs,
/// preserves the richer value for optional fields (e.g., keeps cached VRAM when
/// fresh detection returns None). Fresh-only GPUs are added as-is. Cached-only
/// GPUs are dropped (hardware no longer present).
///
/// This prevents data regression on platforms where GPU detection tools
/// (e.g., rocm-smi on AMD Linux) don't report VRAM on every boot.
fn merge_gpus(cached: &[GpuInfo], fresh: &[GpuInfo]) -> Vec<GpuInfo> {
    let mut result = Vec::with_capacity(fresh.len());

    for fresh_gpu in fresh {
        let fresh_vendor = fresh_gpu.vendor.to_lowercase();
        let fresh_model = fresh_gpu.model.to_lowercase();

        // Find matching cached GPU by vendor+model
        let cached_match = cached.iter().find(|c| {
            c.vendor.to_lowercase() == fresh_vendor && c.model.to_lowercase() == fresh_model
        });

        if let Some(cached_gpu) = cached_match {
            // Merge: fresh values win, but preserve cached VRAM if fresh is None
            let vram_mb = fresh_gpu.vram_mb.or(cached_gpu.vram_mb);

            // Union capabilities (dedup, case-insensitive)
            let mut capabilities: Vec<String> = fresh_gpu.capabilities.clone();
            for cap in &cached_gpu.capabilities {
                if !capabilities.iter().any(|c| c.eq_ignore_ascii_case(cap)) {
                    capabilities.push(cap.clone());
                }
            }

            if fresh_gpu.vram_mb.is_none() && cached_gpu.vram_mb.is_some() {
                tracing::info!(
                    vendor = %fresh_gpu.vendor,
                    model = %fresh_gpu.model,
                    preserved_vram_mb = ?cached_gpu.vram_mb,
                    "Preserved cached VRAM (fresh detection returned None)"
                );
            }

            result.push(GpuInfo {
                vendor: fresh_gpu.vendor.clone(),
                model: fresh_gpu.model.clone(),
                vram_mb,
                capabilities,
            });
        } else {
            // New GPU not in cache, add as-is
            result.push(fresh_gpu.clone());
        }
    }

    result
}

pub async fn detect_capabilities_background(
    stone_name: String,
    caps_arc: Arc<RwLock<Option<HardwareCapabilities>>>,
    console: Arc<console::ConsolePrinter>,
    state: AppState,
) {
    tracing::info!("Starting background hardware capability detection...");

    // === PHASE 1: CPU Detection (fast, <100ms) ===
    console.emit(console::ConsoleEvent::new(
        console::EventCategory::Ops,
        console::EventStatus::Active,
        "[CAPABILITY DETECTION] Detecting CPU features".to_string(),
    ));

    let (cpu_model, cpu_features, architecture) = match resources::get_cpu_info() {
        Ok(result) => result,
        Err(e) => {
            tracing::error!(error = ?e, "Failed to get CPU info");
            (
                "Unknown".to_string(),
                vec![],
                std::env::consts::ARCH.to_string(),
            )
        }
    };

    let resources = resources::collect_stone_resources().ok();
    let cpu_cores = resources.as_ref().map(|r| r.cpu.cores).unwrap_or(1);
    let total_memory_mb = resources
        .as_ref()
        .map(|r| r.memory.total_bytes / 1024 / 1024)
        .unwrap_or(0);

    let disk = resources.as_ref().map(|r| DiskCapabilities {
        // Use primary storage mount for disk capabilities summary
        total_gb: r
            .storage
            .iter()
            .find(|s| s.mount_point == "/" || s.mount_point == "C:\\")
            .or_else(|| r.storage.iter().max_by_key(|s| s.total_gb))
            .map(|s| s.total_gb)
            .unwrap_or(0),
        disk_type: r
            .storage
            .iter()
            .find(|s| s.mount_point == "/" || s.mount_point == "C:\\")
            .or_else(|| r.storage.iter().max_by_key(|s| s.total_gb))
            .map(|s| match &s.disk_type {
                garden_common::DiskType::NVMe => "NVMe".to_string(),
                garden_common::DiskType::SSD => "SSD".to_string(),
                garden_common::DiskType::HDD => "HDD".to_string(),
                garden_common::DiskType::Unknown => "Unknown".to_string(),
            }),
    });

    tracing::info!("CPU detection complete: {} cores", cpu_cores);
    console.emit(console::ConsoleEvent::new(
        console::EventCategory::Ops,
        console::EventStatus::Active,
        format!(
            "[CAPABILITY DETECTION] CPU: {} cores, {} features",
            cpu_cores,
            cpu_features.len()
        ),
    ));

    // Update CPU data incrementally (preserve existing data, update CPU fields)
    let updated_caps = {
        let mut guard = caps_arc.write().await;
        let mut caps = guard.take().unwrap_or_else(|| HardwareCapabilities {
            stone_id: None, // Will be set from AppState
            stone_name: stone_name.clone(),
            hardware: HardwareInventory {
                cpu: CpuCapabilities {
                    model: None,
                    cores: 0,
                    threads: None,
                    architecture: std::env::consts::ARCH.to_string(),
                    features: None,
                },
                memory: MemoryCapabilities { total_mb: 0 },
                gpus: vec![],
                disk: None,
                swap_mb: None,
                ai_capabilities: None,
                system_manufacturer: None,
                system_product: None,
            },
            runtime: Some(RuntimeInfo {
                docker_version: None,
                os: std::env::consts::OS.to_string(),
                kernel: None,
            }),
            detection_status: DetectionStatus::Scanning,
        });

        // Update CPU fields only
        caps.hardware.cpu = CpuCapabilities {
            model: if cpu_model == "Unknown" {
                None
            } else {
                Some(cpu_model.clone())
            },
            cores: cpu_cores,
            threads: None,
            architecture: architecture.clone(),
            features: if cpu_features.is_empty() {
                None
            } else {
                Some(cpu_features.clone())
            },
        };
        caps.hardware.memory = MemoryCapabilities {
            total_mb: total_memory_mb,
        };
        caps.hardware.disk = disk.clone();

        // Upgrade status if needed (Scanning → Partial, but preserve Complete)
        if caps.detection_status == DetectionStatus::Scanning {
            caps.detection_status = DetectionStatus::Partial;
        }

        let cloned = caps.clone();
        *guard = Some(caps);
        cloned
    };

    // Persist updated CPU data to disk (non-blocking for consumers)
    if let Err(e) = save_capabilities_cache(&updated_caps).await {
        tracing::warn!(error = ?e, "Failed to save updated capabilities after CPU detection");
    }
    console.emit(console::ConsoleEvent::new(
        console::EventCategory::System,
        console::EventStatus::Updated,
        "Hardware capabilities (CPU ready)".to_string(),
    ));

    // === PHASE 2: GPU Detection (slow, 2-6 seconds on Windows) ===
    tracing::info!("Starting GPU detection (may take 2-6 seconds on Windows)...");
    console.emit(console::ConsoleEvent::new(
        console::EventCategory::Ops,
        console::EventStatus::Active,
        "[CAPABILITY DETECTION] Detecting GPUs (DXDiag, 2-6 sec)".to_string(),
    ));

    let gpus = resources::detect_gpus();
    let gpu_count = gpus.len();
    tracing::info!(gpu_count = gpus.len(), "GPU detection complete");
    console.emit(console::ConsoleEvent::new(
        console::EventCategory::Ops,
        console::EventStatus::Completed,
        format!("[CAPABILITY DETECTION] Found {} GPU(s)", gpu_count),
    ));

    // === PHASE 3: OS, Kernel, Swap Detection ===
    // Note: Storage inventory moved to live resources (METRICS-0001)
    tracing::info!("Detecting system information...");
    let os_version = resources::detect_os_version();
    let kernel_version = resources::detect_kernel_version();
    let swap_mb = resources::detect_swap();
    tracing::info!("System information detection complete");

    // Update complete capabilities incrementally (update GPU + system info fields)
    let complete_caps = {
        let mut guard = caps_arc.write().await;
        let mut caps = guard
            .take()
            .expect("capabilities should exist after CPU phase");

        // Merge fresh GPU data with cached, preserving VRAM from prior detection
        caps.hardware.gpus = merge_gpus(&caps.hardware.gpus, &gpus);

        // Build AI summary from the merged list (includes preserved VRAM values)
        caps.hardware.ai_capabilities = Some(build_ai_capabilities_summary(
            &caps.hardware.gpus,
            true, // detection_complete = true
        ));

        // Update swap (storage moved to live resources)
        caps.hardware.swap_mb = swap_mb;

        // Update runtime info with OS version and kernel
        if let Some(ref mut runtime) = caps.runtime {
            // Enhance OS string with version
            if let Some(ref os_ver) = os_version {
                let os_family = runtime.os.split('/').next().unwrap_or(&runtime.os);
                runtime.os = format!("{}/{}", os_family, os_ver);
            }
            runtime.kernel = kernel_version;
        }

        // Mark detection as complete
        caps.detection_status = DetectionStatus::Complete;

        let cloned = caps.clone();
        *guard = Some(caps);
        cloned
    };

    // Persist complete data to disk
    match save_capabilities_cache(&complete_caps).await {
        Ok(_) => {
            tracing::info!("Complete capabilities saved to disk");
            console.emit(console::ConsoleEvent::new(
                console::EventCategory::Ops,
                console::EventStatus::Completed,
                "[CAPABILITY DETECTION] Cache persisted to disk".to_string(),
            ));
        }
        Err(e) => tracing::warn!(error = ?e, "Failed to save complete capabilities"),
    }

    tracing::info!("Hardware capability detection complete");

    // Sync updated capabilities to self_entry so chirps carry fresh data.
    // This closes the gap where Phase 9.5 copies stale/skeleton capabilities
    // but background detection never pushes updates to self_entry.
    crate::domain::topology::composition::sync_capabilities(&state, true).await;

    // Write MOTD with complete hardware capabilities now that detection is done.
    #[cfg(target_os = "linux")]
    {
        use garden_common::console::{BankSummary, MotdInfo, StorageSetSummary, write_motd};
        use garden_common::storage::DEFAULT_REPLICA_SET_DISPLAY;

        let caps = state.current.capabilities.read().await.clone();
        let stone_name = state.current.stone.name.clone();
        let ip = state.current.address.read().await.ip_str();
        let port = state.current.api_port;
        let version = crate::version_string();
        let pond_name = state.security.pond_name().await;

        let (cpu_cores, ram_mb, gpu) = match &caps {
            Some(c) => {
                let cores = Some(c.hardware.cpu.cores);
                let ram = Some(c.hardware.memory.total_mb);
                let first_gpu = c
                    .hardware
                    .gpus
                    .first()
                    .map(|g| (g.model.clone(), g.vram_mb));
                (cores, ram, first_gpu)
            }
            None => (None, None, None),
        };

        let storage_sets = {
            let volumes = state.current.storage.volumes.read().await;
            let mut sets: std::collections::BTreeMap<String, Vec<BankSummary>> =
                std::collections::BTreeMap::new();
            for volume in volumes.values() {
                if *volume.state() != crate::domain::storage::VolumeState::Online {
                    continue;
                }
                if let Some(mgmt) = volume.management() {
                    let set_name = if mgmt.replica_set_name.is_empty() {
                        DEFAULT_REPLICA_SET_DISPLAY.to_string()
                    } else {
                        mgmt.replica_set_name.clone()
                    };
                    sets.entry(set_name).or_default().push(BankSummary {
                        name: mgmt.name.clone(),
                        used_bytes: volume.used_bytes(),
                        capacity_bytes: volume.capacity_bytes(),
                    });
                }
            }
            sets.into_iter()
                .map(|(replica_set_name, banks)| StorageSetSummary {
                    replica_set_name,
                    banks,
                })
                .collect::<Vec<_>>()
        };

        let info = MotdInfo {
            stone_name,
            ip,
            port,
            version,
            pond_name,
            cpu_cores,
            ram_mb,
            gpu,
            storage_sets,
        };
        if let Err(e) = write_motd(&info) {
            tracing::warn!(error = %e, "Failed to write MOTD after hardware detection");
        }
    }

    // Re-evaluate offerings index now that complete hardware is known
    // This ensures compatibility warnings update (e.g., no AI → no Ollama, no AVX → MongoDB warning)
    tracing::info!("Re-evaluating offerings compatibility with detected hardware...");
    if let Err(e) = state.catalog.rebuild().await {
        tracing::warn!(error = ?e, "Failed to rebuild offerings index after detection");
    } else {
        console.emit(console::ConsoleEvent::new(
            console::EventCategory::Ops,
            console::EventStatus::Completed,
            "[OFFERINGS] Compatibility re-evaluated".to_string(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(vendor: &str, model: &str, vram: Option<u64>) -> GpuInfo {
        GpuInfo {
            vendor: vendor.to_string(),
            model: model.to_string(),
            vram_mb: vram,
            capabilities: vec![],
        }
    }

    fn gpu_with_caps(vendor: &str, model: &str, vram: Option<u64>, caps: Vec<&str>) -> GpuInfo {
        GpuInfo {
            vendor: vendor.to_string(),
            model: model.to_string(),
            vram_mb: vram,
            capabilities: caps.into_iter().map(String::from).collect(),
        }
    }

    #[test]
    fn merge_preserves_cached_vram_when_fresh_is_none() {
        let cached = vec![gpu("AMD", "Radeon RX 7900 XTX", Some(24517))];
        let fresh = vec![gpu("AMD", "Radeon RX 7900 XTX", None)];

        let result = merge_gpus(&cached, &fresh);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].vram_mb, Some(24517));
    }

    #[test]
    fn merge_fresh_vram_wins_when_both_present() {
        let cached = vec![gpu("AMD", "Radeon RX 7900 XTX", Some(24517))];
        let fresh = vec![gpu("AMD", "Radeon RX 7900 XTX", Some(25000))];

        let result = merge_gpus(&cached, &fresh);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].vram_mb, Some(25000));
    }

    #[test]
    fn merge_new_gpu_added_as_is() {
        let cached = vec![gpu("AMD", "Radeon RX 7900 XTX", Some(24517))];
        let fresh = vec![
            gpu("AMD", "Radeon RX 7900 XTX", None),
            gpu("NVIDIA", "RTX 4090", Some(24576)),
        ];

        let result = merge_gpus(&cached, &fresh);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].vram_mb, Some(24517)); // preserved
        assert_eq!(result[1].vram_mb, Some(24576)); // new, as-is
    }

    #[test]
    fn merge_removed_gpu_dropped() {
        let cached = vec![
            gpu("AMD", "Radeon RX 7900 XTX", Some(24517)),
            gpu("NVIDIA", "RTX 3080", Some(10240)),
        ];
        let fresh = vec![gpu("AMD", "Radeon RX 7900 XTX", None)];

        let result = merge_gpus(&cached, &fresh);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].vendor, "AMD");
    }

    #[test]
    fn merge_capabilities_union() {
        let cached = vec![gpu_with_caps(
            "AMD",
            "Radeon RX 7900 XTX",
            Some(24517),
            vec!["vulkan", "directml", "rocm"],
        )];
        let fresh = vec![gpu_with_caps(
            "AMD",
            "Radeon RX 7900 XTX",
            None,
            vec!["vulkan", "opencl"],
        )];

        let result = merge_gpus(&cached, &fresh);

        assert_eq!(result.len(), 1);
        assert!(result[0].capabilities.contains(&"vulkan".to_string()));
        assert!(result[0].capabilities.contains(&"opencl".to_string()));
        assert!(result[0].capabilities.contains(&"directml".to_string()));
        assert!(result[0].capabilities.contains(&"rocm".to_string()));
    }

    #[test]
    fn merge_case_insensitive_matching() {
        let cached = vec![gpu("nvidia", "rtx 4090", Some(24576))];
        let fresh = vec![gpu("NVIDIA", "RTX 4090", None)];

        let result = merge_gpus(&cached, &fresh);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].vram_mb, Some(24576));
        // Fresh vendor/model casing is used
        assert_eq!(result[0].vendor, "NVIDIA");
        assert_eq!(result[0].model, "RTX 4090");
    }

    #[test]
    fn merge_empty_cached() {
        let cached: Vec<GpuInfo> = vec![];
        let fresh = vec![gpu("AMD", "Radeon RX 7900 XTX", Some(24517))];

        let result = merge_gpus(&cached, &fresh);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].vram_mb, Some(24517));
    }

    #[test]
    fn merge_empty_fresh() {
        let cached = vec![gpu("AMD", "Radeon RX 7900 XTX", Some(24517))];
        let fresh: Vec<GpuInfo> = vec![];

        let result = merge_gpus(&cached, &fresh);

        assert!(result.is_empty());
    }
}
