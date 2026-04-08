//! Hardware topology background probe task (ARCH-0014 Tier 2).
//!
//! Two distinct operations with different contracts:
//!
//! - **`restore_or_probe()`** — startup path. Loads cache, checks both
//!   hardware fingerprint and probe version, skips if both match.
//! - **`probe_now()`** — refresh path (`POST /capabilities/refresh`).
//!   Always runs a full probe unconditionally.
//!
//! Cache invalidation has two triggers:
//! - **Hardware fingerprint** (SHA-256 of PCI device IDs) — detects physical
//!   changes like eGPU hot-plug.
//! - **Probe version** (manually bumped constant) — detects code changes
//!   like new NIC filters, BDF parsing fixes, or added subsystems.

use crate::domain::Current;
use garden_common::console;
use garden_common::types::hardware_topology::HardwareTopology;
use std::path::PathBuf;
use std::sync::Arc;

/// Cache file name for topology data (stored in data_dir).
const TOPOLOGY_CACHE_FILE: &str = "hardware-topology.json";

/// Detection logic version. Bump this when probe behavior changes:
/// - New subsystem added
/// - Filtering logic changed (e.g., virtual NIC exclusion)
/// - Parser improvements (e.g., BDF address extraction)
/// - Field semantics changed
///
/// The cache is invalidated when this doesn't match the stored value,
/// even if hardware hasn't changed.
const PROBE_VERSION: u32 = 4;
// v1: initial implementation
// v2: physical-only NIC filter, Windows BDF addresses, multi-instance PCIe enum
// v3: Windows NIC detection rewritten from MSFT_NetAdapter to Win32_NetworkAdapter
// v4: SMBIOS Type 2 (baseboard) + Type 17 (memory slots: DDR type, speed, form factor)

/// Startup path: restore cached topology or probe if stale.
///
/// 1. Load cached topology from disk → immediately serve via API.
/// 2. Compute PCI fingerprint (~2ms Linux, ~10ms Windows).
/// 3. If fingerprint AND probe version both match cache → done.
/// 4. Otherwise → full probe, update cache.
pub async fn restore_or_probe(
    current: Arc<Current>,
    console: Arc<console::ConsolePrinter>,
) {
    tracing::info!(probe_version = PROBE_VERSION, "Topology probe: restore_or_probe starting");

    // Step 1: Load cache and serve immediately (stale-while-revalidate)
    let cached = load_topology_cache().await;
    match &cached {
        Some(topo) => {
            let mut guard = current.hardware_topology.write().await;
            *guard = Some(topo.clone());
            tracing::info!(
                probed_at = %topo.probed_at,
                cached_probe_version = topo.probe_version,
                fingerprint = &topo.fingerprint[..16.min(topo.fingerprint.len())],
                "Topology cache loaded into state"
            );
        }
        None => {
            tracing::info!("No topology cache found — will run full probe");
        }
    }

    // Step 2: Compute fingerprint
    tracing::debug!("Computing PCI fingerprint...");
    let fingerprint = crate::infra::topology::fingerprint::compute_fingerprint().await;
    tracing::info!(
        fingerprint = &fingerprint[..16.min(fingerprint.len())],
        "PCI fingerprint computed"
    );

    // Step 3: Check if cache is still valid
    if let Some(ref topo) = cached {
        let hardware_match = topo.fingerprint == fingerprint && !fingerprint.is_empty();
        let version_match = topo.probe_version == PROBE_VERSION;

        tracing::info!(
            hardware_match,
            version_match,
            cached_version = topo.probe_version,
            current_version = PROBE_VERSION,
            "Cache validation"
        );

        if hardware_match && version_match {
            console.emit(console::ConsoleEvent::new(
                console::EventCategory::Ops,
                console::EventStatus::Completed,
                "[TOPOLOGY] Cache valid — using cached topology".to_string(),
            ));
            tracing::info!("Topology probe: restore_or_probe complete (cache hit)");
            return;
        }

        if !hardware_match {
            tracing::info!("Hardware fingerprint changed — re-probing");
        }
        if !version_match {
            tracing::info!(
                cached = topo.probe_version,
                current = PROBE_VERSION,
                "Probe version changed — re-probing"
            );
        }
    }

    // Step 4: Cache miss or stale — run full probe
    run_full_probe(current, console, fingerprint).await;
    tracing::info!("Topology probe: restore_or_probe complete");
}

/// Refresh path: always run a full probe unconditionally.
///
/// Called from `POST /capabilities/refresh`. No fingerprint check,
/// no version check — just probe and replace.
pub async fn probe_now(
    current: Arc<Current>,
    console: Arc<console::ConsolePrinter>,
) {
    tracing::info!(probe_version = PROBE_VERSION, "Topology probe: probe_now starting (forced)");
    let fingerprint = crate::infra::topology::fingerprint::compute_fingerprint().await;
    tracing::info!(
        fingerprint = &fingerprint[..16.min(fingerprint.len())],
        "PCI fingerprint computed"
    );
    run_full_probe(current, console, fingerprint).await;
    tracing::info!("Topology probe: probe_now complete");
}

/// Execute the full topology probe, persist results, update shared state.
async fn run_full_probe(
    current: Arc<Current>,
    console: Arc<console::ConsolePrinter>,
    fingerprint: String,
) {
    tracing::info!("Full topology probe starting...");
    console.emit(console::ConsoleEvent::new(
        console::EventCategory::Ops,
        console::EventStatus::Active,
        "[TOPOLOGY] Probing hardware topology...".to_string(),
    ));

    match crate::infra::topology::probe_full_topology(fingerprint).await {
        Ok(mut topology) => {
            topology.probe_version = PROBE_VERSION;

            // Persist to disk
            match save_topology_cache(&topology).await {
                Ok(()) => tracing::info!("Topology cache persisted to disk"),
                Err(e) => tracing::warn!(error = %e, "Failed to persist topology cache"),
            }

            // Update shared state
            let mut guard = current.hardware_topology.write().await;
            *guard = Some(topology.clone());
            tracing::info!("Topology state updated");

            let summary = format!(
                "{} PCIe, {} M.2, {} NICs, {} firmware (v{})",
                topology.expansion.pcie.len(),
                topology.expansion.m2.len(),
                topology.network.len(),
                topology.firmware.len(),
                PROBE_VERSION,
            );

            console.emit(console::ConsoleEvent::new(
                console::EventCategory::Ops,
                console::EventStatus::Completed,
                format!("[TOPOLOGY] Complete — {}", summary),
            ));

            tracing::info!(
                pcie = topology.expansion.pcie.len(),
                m2 = topology.expansion.m2.len(),
                nics = topology.network.len(),
                firmware = topology.firmware.len(),
                usb_devices = topology.expansion.usb.connected_devices.len(),
                probe_version = PROBE_VERSION,
                manufacturer = %topology.system.manufacturer,
                product = %topology.system.product,
                "Full topology probe complete"
            );
        }
        Err(e) => {
            tracing::error!(error = %e, "Full topology probe FAILED");
            console.emit(console::ConsoleEvent::new(
                console::EventCategory::Ops,
                console::EventStatus::Completed,
                format!("[TOPOLOGY] Probe failed: {}", e),
            ));
        }
    }
}

/// Load cached topology from `{data_dir}/hardware-topology.json`.
async fn load_topology_cache() -> Option<HardwareTopology> {
    let path = PathBuf::from(garden_common::constants::paths::data_dir()).join(TOPOLOGY_CACHE_FILE);
    tracing::debug!(path = %path.display(), "Loading topology cache");

    let content = match tokio::fs::read_to_string(&path).await {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!(error = %e, path = %path.display(), "No topology cache file");
            return None;
        }
    };

    match serde_json::from_str(&content) {
        Ok(topo) => {
            tracing::debug!(path = %path.display(), bytes = content.len(), "Topology cache parsed");
            Some(topo)
        }
        Err(e) => {
            tracing::warn!(error = %e, path = %path.display(), "Topology cache corrupt — will re-probe");
            None
        }
    }
}

/// Persist topology to `{data_dir}/hardware-topology.json` atomically.
async fn save_topology_cache(topology: &HardwareTopology) -> anyhow::Result<()> {
    let dir = PathBuf::from(garden_common::constants::paths::data_dir());
    let path = dir.join(TOPOLOGY_CACHE_FILE);
    let content = serde_json::to_string_pretty(topology)?;

    // Ensure directory exists
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        tracing::warn!(error = %e, dir = %dir.display(), "Failed to create data dir for topology cache");
    }

    // Atomic write via temp file + rename
    let tmp = path.with_extension("json.tmp");
    tokio::fs::write(&tmp, &content).await?;
    tokio::fs::rename(&tmp, &path).await?;

    tracing::debug!(path = %path.display(), bytes = content.len(), "Topology cache written");
    Ok(())
}
