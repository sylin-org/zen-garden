//! Garden hardware puller — polls Moss for stone hardware inventory
//! and feeds the [`crate::domain::resources::Resources`] domain.
//!
//! # Architecture (ORCH-0030 §2)
//!
//! The Resources domain is the single authority for physical hardware
//! contention in the garden. It needs per-stone topology (GPU count,
//! per-device VRAM, supported compute stacks, total system memory)
//! to do its job — but it never probes hardware itself. Someone has
//! to feed it. That "someone" is this service.
//!
//! # Data path
//!
//! ```text
//! Moss (tended stone)                    AI orchestrator
//! ───────────────────                    ────────────────────────
//! GET /api/v1/garden/capabilities ──▶ GardenHardwarePuller (this)
//!    │                                     │
//!    │   ApiResponse<                      │ map HardwareInventory
//!    │     Vec<FullCapabilities>           │    → StoneTopology
//!    │   >                                 │
//!    ▼                                     ▼
//! cached topology                    Resources::update_topology
//!                                          │
//!                                          ▼
//!                                    adapters query
//!                                    `stones_capable_of`,
//!                                    `could_host`, `claim`
//! ```
//!
//! # Cadence and freshness
//!
//! Hardware topology changes rarely in practice — boot, driver
//! reload, eGPU hotplug. 60 seconds between polls is well below any
//! human-noticeable latency for "I just plugged a GPU in" while being
//! gentle enough to not perturb Moss. An immediate first poll at
//! startup unblocks adapters on tick zero.
//!
//! # Endpoint choice
//!
//! `/api/v1/garden/capabilities` is the cheap cached read — Moss
//! serves it from its topology cache without fanning out to peers.
//! That's exactly right for us: we want total VRAM per GPU (which
//! doesn't change between boots), not a fresh fan-out probe. For
//! peers, Moss returns `core` (Tier 1 hardware) from the cache and
//! `topology` as `None` — we only need `core`, so the cheap read is
//! sufficient. If we ever need Tier 2 detail (PCIe layout, firmware)
//! we'd switch to `/api/v1/garden/inspect` which fan-outs live.
//!
//! # Failure modes
//!
//! - **Moss unreachable at startup**: log, back off, retry. Adapters
//!   see empty topology and the Resources domain defaults to its
//!   permissive "unknown total → don't filter" behavior — matches
//!   what happens today before this service existed.
//! - **Parse error**: log, skip this tick. Next tick may succeed.
//! - **Missing vram_mb on a GPU**: stored as `None` in TopologyGpu;
//!   the Resources domain treats it as opaque and forces exclusive
//!   holds on that device, same as before.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use garden_common::api_utils::responses::ApiResponse;
use garden_common::types::hardware::GpuInfo;
use garden_common::types::hardware_topology::FullCapabilities;
use reqwest::Client;
use tokio_util::sync::CancellationToken;

use crate::domain::resources::{
    ComputeStack, GpuVendor, Resources, StoneName, StoneTopology, TopologyGpu,
};

/// Interval between successive polls of `/api/v1/garden/capabilities`.
/// Topology changes are rare (boot, eGPU hotplug, driver reload) so
/// a minute is a comfortable middle ground between staleness and
/// load on Moss.
const POLL_INTERVAL: Duration = Duration::from_secs(60);

/// Timeout for a single HTTP round trip to Moss. The endpoint reads
/// from Moss's in-memory topology cache — if it takes longer than
/// this Moss is having a bad day and the next tick will retry.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Cooldown after an error before retrying. Short enough that a
/// transient network blip resolves quickly; long enough that a
/// genuinely-down Moss doesn't get hammered.
const ERROR_BACKOFF: Duration = Duration::from_secs(10);

pub struct GardenHardwarePuller {
    tended_stone: String,
    resources: Arc<Resources>,
    http: Client,
}

impl GardenHardwarePuller {
    /// Construct and spawn the background puller task. Returns
    /// immediately; the first poll happens inside the spawned task
    /// before the first `tick` of the interval so adapters see
    /// topology as soon as Moss is reachable.
    pub fn spawn(
        tended_stone: String,
        resources: Arc<Resources>,
        shutdown: CancellationToken,
    ) -> Arc<Self> {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("garden hardware puller http client");

        let this = Arc::new(Self {
            tended_stone,
            resources,
            http,
        });

        let task = this.clone();
        tokio::spawn(async move { task.run(shutdown).await });
        this
    }

    /// Background loop: one poll on startup, then one every
    /// `POLL_INTERVAL`. Exits on shutdown.
    async fn run(self: Arc<Self>, shutdown: CancellationToken) {
        tracing::info!(
            tended = %self.tended_stone,
            interval_s = POLL_INTERVAL.as_secs(),
            "garden hardware puller starting"
        );

        // Immediate first poll — adapters shouldn't wait a minute
        // for their first dose of topology.
        if let Err(e) = self.poll_once().await {
            tracing::warn!(error = %e, "initial garden capabilities poll failed; will retry");
        }

        let mut ticker = tokio::time::interval(POLL_INTERVAL);
        // Skip the "immediate" tick that tokio::time::interval fires
        // on the first call — we just did it manually.
        ticker.tick().await;

        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    tracing::info!("garden hardware puller shutdown");
                    return;
                }
                _ = ticker.tick() => {
                    if let Err(e) = self.poll_once().await {
                        tracing::warn!(error = %e, "garden capabilities poll failed");
                        tokio::select! {
                            _ = shutdown.cancelled() => return,
                            _ = tokio::time::sleep(ERROR_BACKOFF) => {}
                        }
                    }
                }
            }
        }
    }

    /// Execute one poll cycle: fetch from Moss, parse, fold each
    /// stone's topology into the Resources domain.
    async fn poll_once(&self) -> Result<()> {
        let url = format!(
            "{}/api/v1/garden/capabilities",
            self.tended_stone.trim_end_matches('/')
        );

        let response = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;

        if !response.status().is_success() {
            anyhow::bail!("GET {url} returned {}", response.status());
        }

        let wrapped: ApiResponse<Vec<FullCapabilities>> = response
            .json()
            .await
            .context("parse garden capabilities response")?;

        let stones = wrapped.data;
        let count = stones.len();
        let mut total_gpus = 0usize;
        let mut total_vram_mb: u64 = 0;

        for caps in stones {
            let stone_name = StoneName::new(caps.core.stone_name.clone());
            let topology = map_to_topology(&caps);

            total_gpus += topology.gpus.len();
            total_vram_mb += topology
                .gpus
                .iter()
                .filter_map(|g| g.total_vram_mb)
                .sum::<u64>();

            tracing::debug!(
                stone = %stone_name,
                gpu_count = topology.gpus.len(),
                memory_mb = ?topology.memory_total_mb,
                "updating stone topology"
            );

            self.resources
                .update_topology(stone_name, topology)
                .await;
        }

        // INFO so the milestone is observable at default log level.
        // One line per poll cycle, rollup only — per-stone details
        // stay at DEBUG so production logs don't get noisy.
        tracing::info!(
            stones = count,
            gpus = total_gpus,
            total_vram_mb,
            "garden hardware poll complete"
        );
        Ok(())
    }
}

/// Map a Moss `FullCapabilities` to the Resources domain's
/// `StoneTopology`. Reads Tier 1 `core.hardware` only — Tier 2
/// topology is richer (PCIe layout, firmware) but we don't need it
/// for VRAM fit decisions.
fn map_to_topology(caps: &FullCapabilities) -> StoneTopology {
    let memory_total_mb = Some(caps.core.hardware.memory.total_mb);

    let gpus: Vec<TopologyGpu> = caps
        .core
        .hardware
        .gpus
        .iter()
        .enumerate()
        .map(|(index, gpu)| TopologyGpu {
            index: index as u32,
            name: gpu.model.clone(),
            vendor: parse_vendor(&gpu.vendor),
            compute_stack: parse_compute_stacks(&gpu.capabilities),
            total_vram_mb: gpu.vram_mb,
        })
        .collect();

    StoneTopology {
        gpus,
        memory_total_mb,
    }
}

fn parse_vendor(s: &str) -> GpuVendor {
    match s.to_ascii_lowercase().as_str() {
        "nvidia" => GpuVendor::Nvidia,
        "amd" | "radeon" => GpuVendor::Amd,
        "intel" => GpuVendor::Intel,
        "apple" => GpuVendor::Apple,
        _ => GpuVendor::Unknown,
    }
}

/// Parse Moss's `GpuInfo.capabilities` strings (`"cuda"`, `"rocm"`,
/// `"vulkan"`, `"directml"`, `"opencl"`, possibly version-suffixed
/// like `"cuda:12.2"`) into the Resources domain's `ComputeStack`
/// enum. Strings we don't recognize are silently dropped — adapters
/// only care about the subset the Resources domain models.
fn parse_compute_stacks(strings: &[String]) -> Vec<ComputeStack> {
    let mut out = Vec::new();
    for raw in strings {
        let base = raw
            .split_once(':')
            .map(|(base, _)| base)
            .unwrap_or(raw.as_str())
            .to_ascii_lowercase();
        let stack = match base.as_str() {
            "cuda" => Some(ComputeStack::Cuda),
            "rocm" => Some(ComputeStack::Rocm),
            "oneapi" => Some(ComputeStack::OneApi),
            "metal" => Some(ComputeStack::Metal),
            "vulkan" => Some(ComputeStack::Vulkan),
            // "directml" and "opencl" aren't first-class stacks in
            // the Resources domain today; drop them.
            _ => None,
        };
        if let Some(s) = stack {
            if !out.contains(&s) {
                out.push(s);
            }
        }
    }
    out
}

// Keep GpuInfo import live even if future refactors drop it from the
// mapping body — it's part of the wire contract and we want breakage
// to show up at compile time, not runtime.
#[allow(dead_code)]
fn _type_witness(_: &GpuInfo) {}

#[cfg(test)]
mod tests {
    use super::*;
    use garden_common::types::hardware::{
        DetectionStatus, GpuInfo, HardwareCapabilities, HardwareInventory, MemoryCapabilities,
        CpuCapabilities,
    };

    fn caps_with_gpus(stone_name: &str, gpus: Vec<GpuInfo>) -> FullCapabilities {
        FullCapabilities {
            core: HardwareCapabilities {
                stone_id: None,
                stone_name: stone_name.to_string(),
                hardware: HardwareInventory {
                    cpu: CpuCapabilities {
                        model: None,
                        cores: 8,
                        threads: None,
                        architecture: "x86_64".to_string(),
                        features: None,
                    },
                    memory: MemoryCapabilities { total_mb: 32_000 },
                    gpus,
                    disk: None,
                    swap_mb: None,
                    ai_capabilities: None,
                    system_manufacturer: None,
                    system_product: None,
                },
                runtime: None,
                detection_status: DetectionStatus::Complete,
            },
            topology: None,
        }
    }

    fn gpu(vendor: &str, model: &str, vram_mb: Option<u64>, caps: &[&str]) -> GpuInfo {
        GpuInfo {
            vendor: vendor.to_string(),
            model: model.to_string(),
            vram_mb,
            capabilities: caps.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn maps_nvidia_gpu_to_cuda_stack() {
        let caps = caps_with_gpus(
            "stone-alpha",
            vec![gpu("nvidia", "RTX 4090", Some(24_000), &["cuda", "vulkan"])],
        );
        let topology = map_to_topology(&caps);

        assert_eq!(topology.memory_total_mb, Some(32_000));
        assert_eq!(topology.gpus.len(), 1);
        let g = &topology.gpus[0];
        assert_eq!(g.index, 0);
        assert_eq!(g.name, "RTX 4090");
        assert!(matches!(g.vendor, GpuVendor::Nvidia));
        assert_eq!(g.total_vram_mb, Some(24_000));
        assert!(g.compute_stack.contains(&ComputeStack::Cuda));
        assert!(g.compute_stack.contains(&ComputeStack::Vulkan));
    }

    #[test]
    fn maps_amd_gpu_to_rocm_stack() {
        let caps = caps_with_gpus(
            "stone-beta",
            vec![gpu("AMD", "RX 7900 XTX", Some(24_000), &["rocm", "vulkan"])],
        );
        let topology = map_to_topology(&caps);
        let g = &topology.gpus[0];
        assert!(matches!(g.vendor, GpuVendor::Amd));
        assert!(g.compute_stack.contains(&ComputeStack::Rocm));
    }

    #[test]
    fn unknown_vram_stays_none() {
        let caps = caps_with_gpus("stone-gamma", vec![gpu("nvidia", "Tesla K80", None, &["cuda"])]);
        let topology = map_to_topology(&caps);
        assert_eq!(topology.gpus[0].total_vram_mb, None);
    }

    #[test]
    fn versioned_capability_strings_parse() {
        let caps = caps_with_gpus(
            "stone-delta",
            vec![gpu("nvidia", "L40S", Some(48_000), &["cuda:12.2", "vulkan:1.3"])],
        );
        let topology = map_to_topology(&caps);
        let g = &topology.gpus[0];
        assert!(g.compute_stack.contains(&ComputeStack::Cuda));
        assert!(g.compute_stack.contains(&ComputeStack::Vulkan));
    }

    #[test]
    fn multiple_gpus_indexed_sequentially() {
        let caps = caps_with_gpus(
            "stone-epsilon",
            vec![
                gpu("nvidia", "RTX 3090", Some(24_000), &["cuda"]),
                gpu("nvidia", "RTX 4090", Some(24_000), &["cuda"]),
            ],
        );
        let topology = map_to_topology(&caps);
        assert_eq!(topology.gpus.len(), 2);
        assert_eq!(topology.gpus[0].index, 0);
        assert_eq!(topology.gpus[1].index, 1);
    }

    #[test]
    fn cpu_only_stone_has_empty_gpu_list() {
        let caps = caps_with_gpus("stone-cpu", vec![]);
        let topology = map_to_topology(&caps);
        assert!(topology.gpus.is_empty());
        assert_eq!(topology.memory_total_mb, Some(32_000));
    }

    #[test]
    fn directml_and_opencl_are_silently_dropped() {
        let caps = caps_with_gpus(
            "stone-zeta",
            vec![gpu("nvidia", "GTX 1060", Some(6_000), &["cuda", "directml", "opencl"])],
        );
        let topology = map_to_topology(&caps);
        let stacks = &topology.gpus[0].compute_stack;
        assert!(stacks.contains(&ComputeStack::Cuda));
        // directml and opencl are not modeled.
        assert_eq!(stacks.len(), 1);
    }
}
