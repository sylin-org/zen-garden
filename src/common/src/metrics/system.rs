use crate::{
    format_bytes, format_uptime, CpuMetrics, DiskMetrics, DiskType, GpuInfo,
    InterfaceMetrics, MemoryMetrics, NetworkMetrics, StoneResources, StorageMetrics,
};
use anyhow::{Context, Result};
#[cfg(target_os = "linux")]
use std::fs;
use std::process::Command;
use sysinfo::{Components, Networks, System};

/// Collect CPU model and features from /proc/cpuinfo (Linux) or WMI (Windows)
pub fn get_cpu_info() -> Result<(String, Vec<String>, String)> {
    #[cfg(target_os = "windows")]
    {
        get_cpu_info_windows()
    }

    #[cfg(target_os = "linux")]
    {
        get_cpu_info_linux()
    }
}

#[cfg(target_os = "linux")]
fn get_cpu_info_linux() -> Result<(String, Vec<String>, String)> {
    let cpuinfo = fs::read_to_string("/proc/cpuinfo").context("Failed to read /proc/cpuinfo")?;

    let mut model_name = String::from("Unknown");
    let mut features = Vec::new();
    let mut architecture = std::env::consts::ARCH.to_string();

    for line in cpuinfo.lines() {
        if line.starts_with("model name") {
            if let Some(value) = line.split(':').nth(1) {
                model_name = value.trim().to_string();
            }
        } else if line.starts_with("flags") || line.starts_with("Features") {
            if let Some(flags_str) = line.split(':').nth(1) {
                features = flags_str
                    .split_whitespace()
                    .map(|s| s.to_lowercase())
                    .collect();
            }
        }
    }

    // For ARM, try to get more specific arch info
    if architecture.starts_with("arm") || architecture.starts_with("aarch") {
        if let Ok(arch_info) = fs::read_to_string("/proc/device-tree/model") {
            architecture = arch_info.trim().to_string();
        }
    }

    Ok((model_name, features, architecture))
}

#[cfg(target_os = "windows")]
fn get_cpu_info_windows() -> Result<(String, Vec<String>, String)> {
    let mut model_name = String::from("Unknown");
    let architecture = std::env::consts::ARCH.to_string();
    let mut features = Vec::new();

    // Get CPU model name from WMI
    let output = Command::new("powershell")
        .args([
            "-Command",
            "Get-WmiObject -Class Win32_Processor | Select-Object -ExpandProperty Name",
        ])
        .output();

    if let Ok(output) = output
        && let Ok(name) = String::from_utf8(output.stdout)
    {
        model_name = name.trim().to_string();
    }

    // Detect CPU features using CPUID - is_x86_feature_detected! is safe
    {
        // Check basic features
        if is_x86_feature_detected!("sse") {
            features.push("sse".to_string());
        }
        if is_x86_feature_detected!("sse2") {
            features.push("sse2".to_string());
        }
        if is_x86_feature_detected!("sse3") {
            features.push("sse3".to_string());
        }
        if is_x86_feature_detected!("ssse3") {
            features.push("ssse3".to_string());
        }
        if is_x86_feature_detected!("sse4.1") {
            features.push("sse4_1".to_string());
        }
        if is_x86_feature_detected!("sse4.2") {
            features.push("sse4_2".to_string());
        }
        if is_x86_feature_detected!("avx") {
            features.push("avx".to_string());
        }
        if is_x86_feature_detected!("avx2") {
            features.push("avx2".to_string());
        }
        if is_x86_feature_detected!("fma") {
            features.push("fma".to_string());
        }
        if is_x86_feature_detected!("bmi1") {
            features.push("bmi1".to_string());
        }
        if is_x86_feature_detected!("bmi2") {
            features.push("bmi2".to_string());
        }
        if is_x86_feature_detected!("aes") {
            features.push("aes".to_string());
        }
        if is_x86_feature_detected!("avx512f") {
            features.push("avx512f".to_string());
        }
        if is_x86_feature_detected!("avx512bw") {
            features.push("avx512bw".to_string());
        }
        if is_x86_feature_detected!("avx512cd") {
            features.push("avx512cd".to_string());
        }
        if is_x86_feature_detected!("avx512dq") {
            features.push("avx512dq".to_string());
        }
        if is_x86_feature_detected!("avx512vl") {
            features.push("avx512vl".to_string());
        }
    }

    Ok((model_name, features, architecture))
}

/// Collect fast metrics (CPU, memory, uptime)
///
/// Fast collection suitable for high-frequency polling (5s intervals).
/// Only accesses in-memory kernel structures, no I/O.
pub fn get_fast_metrics() -> Result<(CpuMetrics, MemoryMetrics, u64, String)> {
    let mut system = System::new();
    system.refresh_cpu_all();
    system.refresh_memory();

    // CPU metrics
    let usage_percent = system.global_cpu_usage();
    let cpu = CpuMetrics {
        cores: system.cpus().len(),
        usage_percent,
        usage_friendly: format!("{:.1}%", usage_percent),
    };

    // Memory metrics
    let total_bytes = system.total_memory();
    let used_bytes = system.used_memory();
    let available_bytes = system.available_memory();
    let used_percent = if total_bytes > 0 {
        (used_bytes as f64 / total_bytes as f64 * 100.0) as f32
    } else {
        0.0
    };
    let memory = MemoryMetrics {
        total_bytes,
        used_bytes,
        available_bytes,
        used_percent,
        total_friendly: format_bytes(total_bytes),
        used_friendly: format_bytes(used_bytes),
        available_friendly: format_bytes(available_bytes),
    };

    // System uptime
    let uptime_seconds = sysinfo::System::uptime();
    let uptime_friendly = format_uptime(uptime_seconds);

    Ok((cpu, memory, uptime_seconds, uptime_friendly))
}

/// Read the CPU package temperature from hardware thermal sensors.
///
/// Scans `sysinfo::Components` for labels containing "CPU", "Package",
/// "Tctl", or "coretemp" (common on x86 and ARM). Returns the first
/// matching sensor's temperature in degrees Celsius, or `None` if no
/// CPU thermal sensor is found (e.g. VMs, some Windows configurations).
///
/// Especially valuable for ARM stones with passive cooling where thermal
/// throttling is a real operational concern.
pub fn get_cpu_temperature() -> Option<f32> {
    let components = Components::new_with_refreshed_list();
    components
        .list()
        .iter()
        .find(|c| {
            let label = c.label().to_lowercase();
            label.contains("cpu")
                || label.contains("package")
                || label.contains("tctl")
                || label.contains("coretemp")
        })
        .and_then(|c| c.temperature())
}

/// Collect storage metrics for all mounted disks (slower, involves filesystem stat calls)
///
/// Suitable for lower-frequency polling (30s intervals).
/// Performs stat syscalls on mount points which may be slow on network mounts.
/// Returns complete storage inventory with live usage data.
pub fn get_storage_metrics() -> Result<Vec<StorageMetrics>> {
    let disks = sysinfo::Disks::new_with_refreshed_list();

    let mut storage = Vec::new();
    for disk in disks.iter() {
        let total = disk.total_space();
        let available = disk.available_space();
        let used = total - available;
        let used_percent = if total > 0 {
            (used as f64 / total as f64 * 100.0) as f32
        } else {
            0.0
        };

        let mount_point = disk.mount_point().to_string_lossy().to_string();

        // Detect disk type from mount point
        let disk_type = detect_disk_type_for_mount(&mount_point)
            .and_then(|s| match s.as_str() {
                "NVMe" => Some(DiskType::NVMe),
                "SSD" => Some(DiskType::SSD),
                "HDD" => Some(DiskType::HDD),
                _ => None,
            })
            .unwrap_or(DiskType::Unknown);

        // Extract identifier from mount point or use mount point
        let identifier = if let Some(name) = disk.name().to_str() {
            name.to_string()
        } else {
            mount_point.clone()
        };

        storage.push(StorageMetrics {
            identifier,
            mount_point,
            total_gb: total / 1024 / 1024 / 1024,
            used_gb: used / 1024 / 1024 / 1024,
            available_gb: available / 1024 / 1024 / 1024,
            used_percent,
            disk_type,
            filesystem: disk.file_system().to_string_lossy().to_string(),
        });
    }

    if storage.is_empty() {
        anyhow::bail!("No storage devices found");
    }

    Ok(storage)
}

/// Collect network metrics for all interfaces
///
/// Returns aggregate and per-interface statistics. For rate calculation,
/// call this function twice with a delay and compute the delta.
pub fn get_network_metrics() -> NetworkMetrics {
    let networks = Networks::new_with_refreshed_list();

    let mut interfaces = Vec::new();
    let mut total_rx: u64 = 0;
    let mut total_tx: u64 = 0;

    for (name, data) in networks.iter() {
        let rx_bytes = data.total_received();
        let tx_bytes = data.total_transmitted();

        // Skip loopback and virtual interfaces with no traffic
        if name.starts_with("lo") && rx_bytes == 0 && tx_bytes == 0 {
            continue;
        }

        total_rx = total_rx.saturating_add(rx_bytes);
        total_tx = total_tx.saturating_add(tx_bytes);

        interfaces.push(InterfaceMetrics {
            name: name.clone(),
            rx_bytes,
            tx_bytes,
            rx_friendly: format_bytes(rx_bytes),
            tx_friendly: format_bytes(tx_bytes),
        });
    }

    // Sort by name for consistent ordering
    interfaces.sort_by(|a, b| a.name.cmp(&b.name));

    NetworkMetrics {
        interfaces,
        total_rx_bytes: total_rx,
        total_tx_bytes: total_tx,
        rx_bytes_per_sec: None, // Requires delta calculation
        tx_bytes_per_sec: None,
        total_rx_friendly: format_bytes(total_rx),
        total_tx_friendly: format_bytes(total_tx),
    }
}

/// Calculate network rate by comparing two snapshots
///
/// Takes the previous metrics, elapsed time in seconds, and returns updated metrics
/// with bytes_per_sec filled in.
pub fn calculate_network_rate(
    current: &NetworkMetrics,
    previous: &NetworkMetrics,
    elapsed_secs: f64,
) -> NetworkMetrics {
    if elapsed_secs <= 0.0 {
        return current.clone();
    }

    let rx_delta = current
        .total_rx_bytes
        .saturating_sub(previous.total_rx_bytes);
    let tx_delta = current
        .total_tx_bytes
        .saturating_sub(previous.total_tx_bytes);

    let rx_per_sec = (rx_delta as f64 / elapsed_secs) as u64;
    let tx_per_sec = (tx_delta as f64 / elapsed_secs) as u64;

    NetworkMetrics {
        interfaces: current.interfaces.clone(),
        total_rx_bytes: current.total_rx_bytes,
        total_tx_bytes: current.total_tx_bytes,
        rx_bytes_per_sec: Some(rx_per_sec),
        tx_bytes_per_sec: Some(tx_per_sec),
        total_rx_friendly: current.total_rx_friendly.clone(),
        total_tx_friendly: current.total_tx_friendly.clone(),
    }
}

/// Collect all host-level resource metrics (combined fast + slow)
///
/// Use this for one-shot collection. For continuous monitoring, prefer
/// separate `get_fast_metrics()` and `get_storage_metrics()` at different intervals.
pub fn collect_stone_resources() -> Result<StoneResources> {
    let (cpu, memory, uptime_seconds, uptime_friendly) = get_fast_metrics()?;
    let storage = get_storage_metrics()?;
    let cpu_temperature = get_cpu_temperature();

    Ok(StoneResources {
        cpu,
        memory,
        storage,
        uptime_seconds,
        uptime_friendly,
        cpu_temperature,
    })
}

/// Legacy alias for backward compatibility
pub fn get_stone_resources() -> Result<StoneResources> {
    collect_stone_resources()
}

#[expect(dead_code)]
fn collect_stone_resources_original() -> Result<StoneResources> {
    let mut system = System::new();
    system.refresh_cpu_all();
    system.refresh_memory();

    // CPU metrics
    let usage_percent = system.global_cpu_usage();
    let cpu = CpuMetrics {
        cores: system.cpus().len(),
        usage_percent,
        usage_friendly: format!("{:.1}%", usage_percent),
    };

    // Memory metrics
    let total_bytes = system.total_memory();
    let used_bytes = system.used_memory();
    let available_bytes = system.available_memory();
    let used_percent = if total_bytes > 0 {
        (used_bytes as f64 / total_bytes as f64 * 100.0) as f32
    } else {
        0.0
    };
    let memory = MemoryMetrics {
        total_bytes,
        used_bytes,
        available_bytes,
        used_percent,
        total_friendly: format_bytes(total_bytes),
        used_friendly: format_bytes(used_bytes),
        available_friendly: format_bytes(available_bytes),
    };

    // Disk metrics - focus on root filesystem or /var/lib/zen-garden if available
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let _disk = disks
        .iter()
        .find(|d| {
            let mount_point = d.mount_point().to_string_lossy();
            mount_point == "/var/lib/zen-garden" || mount_point == "/"
        })
        .or_else(|| disks.iter().next())
        .map(|d| {
            let total = d.total_space();
            let available = d.available_space();
            let used = total - available;
            let used_percent = if total > 0 {
                (used as f64 / total as f64 * 100.0) as f32
            } else {
                0.0
            };
            DiskMetrics {
                total_bytes: total,
                used_bytes: used,
                available_bytes: available,
                used_percent,
                path: d.mount_point().to_string_lossy().to_string(),
                total_friendly: format_bytes(total),
                used_friendly: format_bytes(used),
                available_friendly: format_bytes(available),
            }
        })
        .context("No disk information available")?;

    // System uptime
    let uptime_seconds = sysinfo::System::uptime();
    let uptime_friendly = format_uptime(uptime_seconds);

    Ok(StoneResources {
        cpu,
        memory,
        storage: vec![], // Legacy function - use get_storage_metrics() instead
        uptime_seconds,
        uptime_friendly,
        cpu_temperature: get_cpu_temperature(),
    })
}

/// Best-effort disk type detection for a mount point.
///
/// Returns one of: "NVMe", "SSD", "HDD" (or None if unknown).
///
/// On Linux, tries two strategies:
/// 1. `lsblk` with `--nodeps` to resolve device-mapper/LVM to physical type
/// 2. Fallback to `findmnt` + `/sys/block/*/queue/rotational` for direct devices
///
/// On Windows, always returns None (not implemented).
pub fn detect_disk_type_for_mount(mount_point: &str) -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        let _ = mount_point;
        None
    }

    #[cfg(target_os = "linux")]
    {
        // Strategy 1: Use lsblk which resolves LVM/device-mapper to physical device.
        // `lsblk -ndo ROTA,TYPE <source>` returns e.g. "0 disk" (SSD) or "1 disk" (HDD).
        // For LVM/DM, we find the underlying physical device first.
        if let Some(result) = detect_via_lsblk(mount_point) {
            return Some(result);
        }

        // Strategy 2: Direct sysfs probe (original approach, works for simple /dev/sdX).
        detect_via_sysfs(mount_point)
    }
}

/// Detect disk type using `lsblk`, which handles LVM, device-mapper, and LUKS.
#[cfg(target_os = "linux")]
fn detect_via_lsblk(mount_point: &str) -> Option<String> {
    // Find the source device for this mount point
    let findmnt = Command::new("findmnt")
        .args(["-no", "SOURCE", "--target", mount_point])
        .output()
        .ok()?;

    if !findmnt.status.success() {
        return None;
    }

    let source = String::from_utf8_lossy(&findmnt.stdout).trim().to_string();
    if source.is_empty() {
        return None;
    }

    // Use lsblk to find the underlying physical disk(s) for this device.
    // -s = show parents (follow LVM → VG → PV → physical disk)
    // -ndo NAME,ROTA,TYPE = no header, device only, specific columns
    let lsblk = Command::new("lsblk")
        .args(["-sndo", "NAME,ROTA,TYPE", &source])
        .output()
        .ok()?;

    if !lsblk.status.success() {
        return None;
    }

    let output = String::from_utf8_lossy(&lsblk.stdout);

    // Look for the physical disk line (TYPE=disk) among the ancestors.
    // Example output for an LVM on NVMe:
    //   root      0 lvm
    //   nvme0n1p2 0 part
    //   nvme0n1   0 disk
    for line in output.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 && parts[2] == "disk" {
            let name = parts[0];
            if name.starts_with("nvme") {
                return Some("NVMe".to_string());
            }
            return match parts[1] {
                "0" => Some("SSD".to_string()),
                "1" => Some("HDD".to_string()),
                _ => None,
            };
        }
    }

    // If no "disk" type found in ancestry, try the device itself
    // (handles cases where lsblk -s doesn't traverse all the way)
    for line in output.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let name = parts[0];
            if name.starts_with("nvme") {
                return Some("NVMe".to_string());
            }
            return match parts[1] {
                "0" => Some("SSD".to_string()),
                "1" => Some("HDD".to_string()),
                _ => None,
            };
        }
    }

    None
}

/// Fallback: detect disk type via sysfs rotational flag.
/// Works for direct /dev/sdX and /dev/nvmeXnY devices only.
#[cfg(target_os = "linux")]
fn detect_via_sysfs(mount_point: &str) -> Option<String> {
    let output = Command::new("findmnt")
        .args(["-no", "SOURCE", "--target", mount_point])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let source = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if source.is_empty() {
        return None;
    }

    // Normalize /dev/<device><partition> to base block device name.
    let dev = source.rsplit('/').next().unwrap_or("");
    if dev.is_empty() {
        return None;
    }

    let base = if dev.starts_with("nvme") {
        dev.split('p').next().unwrap_or(dev)
    } else if dev.starts_with("mmcblk") {
        dev.split('p').next().unwrap_or(dev)
    } else {
        dev.trim_end_matches(|c: char| c.is_ascii_digit())
    };

    if base.is_empty() {
        return None;
    }

    if base.starts_with("nvme") {
        return Some("NVMe".to_string());
    }

    let rotational_path = format!("/sys/block/{base}/queue/rotational");
    let rotational = fs::read_to_string(rotational_path).ok()?;
    match rotational.trim() {
        "0" => Some("SSD".to_string()),
        "1" => Some("HDD".to_string()),
        _ => None,
    }
}

/// Detect GPU hardware
/// Collect GPU compute utilization across all detected GPUs (FIREFLY-0003).
///
/// Returns the maximum utilization percentage (0–100) if multiple GPUs are present.
/// Returns None if no GPU is detected or all queries fail.
///
/// Vendor dispatch:
/// - NVIDIA: `nvidia-smi --query-gpu=utilization.gpu`
/// - AMD: `/sys/class/drm/card*/device/gpu_busy_percent` or `rocm-smi --showuse`
/// - Intel: `/sys/class/drm/card*/gt_cur_freq_mhz` (heuristic)
///
/// Called on the fast metrics interval (5s). The shell-out is fast (~10ms).
pub fn get_gpu_utilization() -> Option<f32> {
    // Try NVIDIA first (most common for AI workloads)
    if let Some(pct) = query_nvidia_utilization() {
        return Some(pct);
    }
    // Try AMD
    if let Some(pct) = query_amd_utilization() {
        return Some(pct);
    }
    // No Intel utilization query yet — hardware needed for testing
    None
}

/// Query NVIDIA GPU utilization via nvidia-smi
fn query_nvidia_utilization() -> Option<f32> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=utilization.gpu",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // If multiple GPUs, take the max utilization
    stdout
        .lines()
        .filter_map(|line| line.trim().parse::<f32>().ok())
        .reduce(f32::max)
}

/// Query AMD GPU utilization via sysfs or rocm-smi
fn query_amd_utilization() -> Option<f32> {
    // Try sysfs first (Linux, no tool required)
    #[cfg(target_os = "linux")]
    {
        use std::fs;
        // Look for gpu_busy_percent in /sys/class/drm/card*/device/
        if let Ok(entries) = fs::read_dir("/sys/class/drm") {
            let mut max_util: Option<f32> = None;
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with("card") && !name_str.contains('-') {
                    let busy_path = entry.path().join("device/gpu_busy_percent");
                    if let Ok(content) = fs::read_to_string(&busy_path) {
                        if let Ok(pct) = content.trim().parse::<f32>() {
                            max_util = Some(max_util.map_or(pct, |m: f32| m.max(pct)));
                        }
                    }
                }
            }
            if max_util.is_some() {
                return max_util;
            }
        }
    }

    // Fallback: rocm-smi --showuse
    let output = Command::new("rocm-smi").arg("--showuse").output().ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Parse lines like "GPU[0] : GPU use (%): 42"
    stdout
        .lines()
        .filter(|line| line.contains("GPU use"))
        .filter_map(|line| {
            line.rsplit(':')
                .next()
                .and_then(|s| s.trim().parse::<f32>().ok())
        })
        .reduce(f32::max)
}

pub fn detect_gpus() -> Vec<GpuInfo> {
    let mut gpus = Vec::new();

    // Try NVIDIA detection first (most common for AI workloads)
    if let Ok(nvidia_gpus) = detect_nvidia_gpus() {
        gpus.extend(nvidia_gpus);
    }

    // Try AMD detection
    if let Ok(amd_gpus) = detect_amd_gpus() {
        gpus.extend(amd_gpus);
    }

    // Try Intel detection
    if let Ok(intel_gpus) = detect_intel_gpus() {
        gpus.extend(intel_gpus);
    }

    // If nothing detected but we're on Windows, try DirectX/DXGI detection
    #[cfg(target_os = "windows")]
    {
        if gpus.is_empty()
            && let Ok(dxgi_gpus) = detect_windows_gpus()
        {
            gpus.extend(dxgi_gpus);
        }
    }

    // ai_runtimes is legacy — capabilities is the source of truth.
    // No toolkit detection, no container scanning. Hardware capabilities
    // are set during GPU enumeration; the compatibility DSL reads them
    // directly via FactSource.

    gpus
}

fn detect_nvidia_gpus() -> Result<Vec<GpuInfo>> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .output()?;

    if !output.status.success() {
        anyhow::bail!("nvidia-smi failed");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let gpus = stdout
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
            if parts.len() >= 2 {
                let model = parts[0].to_string();
                let vram_mb = parts[1].parse::<u64>().ok();

                let mut capabilities = vec!["cuda".to_string(), "vulkan".to_string()];
                if cfg!(target_os = "windows") {
                    capabilities.push("directml".to_string());
                }

                Some(GpuInfo {
                    vendor: "NVIDIA".to_string(),
                    model,
                    vram_mb,
                    capabilities,
                    ai_runtimes: Vec::new(),
                })
            } else {
                None
            }
        })
        .collect();

    Ok(gpus)
}

fn detect_amd_gpus() -> Result<Vec<GpuInfo>> {
    // Try rocm-smi first
    if let Ok(output) = Command::new("rocm-smi").arg("--showproductname").output()
        && output.status.success()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let gpus: Vec<GpuInfo> = stdout
            .lines()
            .filter(|line| line.contains("Card series"))
            .filter_map(|line| {
                let model = line.split(':').nth(1)?.trim().to_string();

                let mut capabilities = vec!["rocm".to_string(), "vulkan".to_string()];
                if cfg!(target_os = "windows") {
                    capabilities.push("directml".to_string());
                }

                Some(GpuInfo {
                    vendor: "AMD".to_string(),
                    model,
                    vram_mb: None, // Would need additional query
                    capabilities,
                    ai_runtimes: Vec::new(),
                })
            })
            .collect();

        if !gpus.is_empty() {
            return Ok(gpus);
        }
    }

    // Fallback: check lspci on Linux
    #[cfg(target_os = "linux")]
    {
        if let Ok(output) = Command::new("lspci").output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let gpus: Vec<GpuInfo> = stdout
                .lines()
                .filter(|line| line.contains("VGA") || line.contains("3D"))
                .filter(|line| {
                    line.to_lowercase().contains("amd") || line.to_lowercase().contains("radeon")
                })
                .map(|line| {
                    let model = line
                        .split(':')
                        .last()
                        .unwrap_or("AMD GPU")
                        .trim()
                        .to_string();

                    GpuInfo {
                        vendor: "AMD".to_string(),
                        model,
                        vram_mb: None,
                        capabilities: vec!["vulkan".to_string()], // Unknown without rocm-smi
                        ai_runtimes: Vec::new(),
                    }
                })
                .collect();

            if !gpus.is_empty() {
                return Ok(gpus);
            }
        }
    }

    anyhow::bail!("No AMD GPUs detected")
}

fn detect_intel_gpus() -> Result<Vec<GpuInfo>> {
    #[cfg(target_os = "linux")]
    {
        if let Ok(output) = Command::new("lspci").output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let gpus: Vec<GpuInfo> = stdout
                .lines()
                .filter(|line| line.contains("VGA") || line.contains("3D"))
                .filter(|line| line.to_lowercase().contains("intel"))
                .map(|line| {
                    let model = line
                        .split(':')
                        .last()
                        .unwrap_or("Intel GPU")
                        .trim()
                        .to_string();

                    GpuInfo {
                        vendor: "Intel".to_string(),
                        model,
                        vram_mb: None,
                        capabilities: vec!["vulkan".to_string()],
                        ai_runtimes: Vec::new(),
                    }
                })
                .collect();

            if !gpus.is_empty() {
                return Ok(gpus);
            }
        }
    }

    anyhow::bail!("No Intel GPUs detected")
}

#[cfg(target_os = "windows")]
fn detect_windows_gpus() -> Result<Vec<GpuInfo>> {
    // Query WMI for GPU info including PNPDeviceID to filter out non-PCIe devices
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "Get-WmiObject Win32_VideoController | Select-Object Name, CompanionRAM, PNPDeviceID, CompanionCompatibility | ConvertTo-Json"
        ])
        .output()?;

    if !output.status.success() {
        anyhow::bail!("PowerShell GPU query failed");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Get accurate VRAM using DXGI (native DirectX API - most reliable)
    // Falls back to WMI+Registry if DXGI fails
    let dxgi_vram = get_vram_from_dxgi();
    let wmi_vram = if dxgi_vram.is_empty() {
        tracing::info!("DXGI VRAM detection failed, falling back to WMI");
        get_vram_from_wmi()
    } else {
        tracing::info!("Using DXGI for accurate VRAM detection");
        dxgi_vram
    };

    // Parse JSON output
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
        let gpu_array: Vec<&serde_json::Value> = if json.is_array() {
            json.as_array().unwrap().iter().collect()
        } else {
            vec![&json]
        };

        let gpus: Vec<GpuInfo> = gpu_array
            .iter()
            .filter_map(|gpu| {
                let name = gpu["Name"].as_str()?.to_string();
                let pnp_id = gpu["PNPDeviceID"].as_str().unwrap_or("");
                let companion_compat = gpu["CompanionCompatibility"].as_str().unwrap_or("");

                // Filter out non-compute GPUs:
                // - USB devices (DisplayLink, etc.)
                // - Microsoft Basic Display Companion
                // - Virtual/software renderers
                if pnp_id.starts_with("USB\\") {
                    tracing::debug!("Skipping USB display Companion: {}", name);
                    return None;
                }
                if name.contains("DisplayLink")
                    || name.contains("Basic Display")
                    || name.contains("Microsoft Basic")
                {
                    tracing::debug!("Skipping virtual/display-only Companion: {}", name);
                    return None;
                }

                let vram_bytes = gpu["CompanionRAM"].as_u64();
                let mut vram_mb = vram_bytes.map(|b| b / 1024 / 1024);

                // If WMI CompanionRAM is unreliable, try to get from enhanced WMI detection
                // Trigger fallback if: None, < 1GB, or 4000-4096 MB (indicates 32-bit truncation at 4GB)
                // Try both exact and normalized name matching
                let needs_fallback = vram_mb.is_none()
                    || vram_mb.unwrap() < 1024
                    || (vram_mb.unwrap() >= 4000 && vram_mb.unwrap() <= 4096);

                if needs_fallback {
                    let normalized_name = normalize_gpu_name(&name);
                    if let Some(enhanced_vram) = wmi_vram
                        .get(&name)
                        .or_else(|| wmi_vram.get(&normalized_name))
                    {
                        vram_mb = Some(*enhanced_vram);
                    }
                }

                // Detect vendor and capabilities from name and compatibility
                let name_lower = name.to_lowercase();
                let compat_lower = companion_compat.to_lowercase();

                let (vendor, capabilities): (&str, Vec<String>) = if name_lower.contains("nvidia")
                    || name_lower.contains("geforce")
                    || name_lower.contains("quadro")
                    || name_lower.contains("rtx")
                {
                    (
                        "NVIDIA",
                        vec![
                            "cuda".to_string(),
                            "vulkan".to_string(),
                            "directml".to_string(),
                        ],
                    )
                } else if name_lower.contains("amd")
                    || name_lower.contains("radeon")
                    || compat_lower.contains("advanced micro devices")
                {
                    ("AMD", vec!["vulkan".to_string(), "directml".to_string()])
                } else if name_lower.contains("intel") && pnp_id.starts_with("PCI\\") {
                    ("Intel", vec!["vulkan".to_string(), "directml".to_string()])
                } else {
                    // Unknown but on PCIe bus - might still be usable
                    if pnp_id.starts_with("PCI\\") {
                        ("Unknown", vec!["vulkan".to_string()])
                    } else {
                        return None; // Not a real GPU
                    }
                };

                Some(GpuInfo {
                    vendor: vendor.to_string(),
                    model: name,
                    vram_mb,
                    capabilities,
                    ai_runtimes: Vec::new(),
                })
            })
            .collect();

        if !gpus.is_empty() {
            return Ok(gpus);
        }
    }

    anyhow::bail!("No compute-capable GPUs detected")
}

/// Normalize GPU name for consistent matching between detection methods
#[cfg(target_os = "windows")]
fn normalize_gpu_name(name: &str) -> String {
    // Remove extra whitespace, lowercase, and normalize vendor prefixes
    name.to_lowercase()
        .split_whitespace()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Get accurate VRAM sizes using WMI + PowerShell (more reliable than DXDiag)
#[cfg(target_os = "windows")]
fn get_vram_from_wmi() -> std::collections::HashMap<String, u64> {
    use std::collections::HashMap;

    let mut vram_map = HashMap::new();

    // Use WMI to get video controller information with dedicated memory
    // This approach is faster and more reliable than DXDiag text parsing
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            r#"
            Get-CimInstance -ClassName Win32_VideoController | Where-Object {
                $_.PNPDeviceID -like "PCI\VEN*" -and
                $_.Name -notlike "*Basic*" -and
                $_.Name -notlike "*DisplayLink*"
            } | ForEach-Object {
                $vramBytes = $_.CompanionRAM

                # WMI CompanionRAM can be unreliable (32-bit field capped at ~4GB), try to get from device properties
                # Trigger fallback if: null, zero, < 1GB, or suspiciously close to 4GB (4000-4096 MB indicates truncation)
                $vramMB = [Math]::Round($vramBytes / 1MB)
                if ($vramBytes -eq $null -or $vramBytes -eq 0 -or $vramBytes -lt 1GB -or ($vramMB -ge 4000 -and $vramMB -le 4096)) {
                    # Try to read from registry for more accurate VRAM info
                    $pnpId = $_.PNPDeviceID
                    $regPath = "HKLM:\SYSTEM\CurrentControlSet\Enum\$pnpId"
                    if (Test-Path $regPath) {
                        try {
                            $hwInfo = Get-ItemProperty -Path $regPath -ErrorAction SilentlyContinue
                            if ($hwInfo.'HardwareInformation.qwMemorySize') {
                                $vramBytes = $hwInfo.'HardwareInformation.qwMemorySize'
                            }
                        } catch {}
                    }
                }

                # If still no valid VRAM and CompanionRAM is capped at 4GB, it's likely truncated
                # Use a heuristic: if it's exactly 4095 MB, the actual value is likely higher
                if ($vramBytes -gt 0) {
                    [PSCustomObject]@{
                        Name = $_.Name
                        VramMB = [Math]::Round($vramBytes / 1MB)
                    }
                }
            } | ConvertTo-Json
            "#
        ])
        .output();

    if let Ok(output) = output
        && let Ok(stdout) = String::from_utf8(output.stdout)
        && let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout)
    {
        // Parse JSON output
        let gpu_array: Vec<&serde_json::Value> = if json.is_array() {
            json.as_array().unwrap().iter().collect()
        } else {
            vec![&json]
        };

        for gpu in gpu_array {
            if let (Some(name), Some(vram)) = (gpu["Name"].as_str(), gpu["VramMB"].as_u64()) {
                // Only add if VRAM is reasonable (> 0)
                if vram > 0 {
                    let normalized = normalize_gpu_name(name);
                    vram_map.insert(normalized, vram);

                    // Also store original name for fallback matching
                    vram_map.insert(name.to_string(), vram);
                }
            }
        }
    }

    vram_map
}

/// Get accurate VRAM using native DXGI API (most reliable on Windows)
///
/// Returns HashMap of GPU description -> VRAM in MB
/// Uses DirectX Graphics Infrastructure to query dedicated video memory directly
#[cfg(target_os = "windows")]
fn get_vram_from_dxgi() -> std::collections::HashMap<String, u64> {
    use std::collections::HashMap;
    use std::mem::MaybeUninit;
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory, IDXGIFactory, DXGI_ADAPTER_DESC, DXGI_ERROR_NOT_FOUND,
    };

    let mut vram_map = HashMap::new();

    // Create DXGI Factory
    // SAFETY: CreateDXGIFactory takes no arguments and is safe to call on any Windows platform
    // that has the DXGI runtime installed (guaranteed on Windows 7+).
    let factory: Result<IDXGIFactory, _> = unsafe { CreateDXGIFactory() };

    let Ok(factory) = factory else {
        tracing::debug!("Failed to create DXGI factory");
        return vram_map;
    };

    // Enumerate adapters
    let mut adapter_index = 0;
    loop {
        // SAFETY: `factory` is a valid IDXGIFactory obtained above. `adapter_index` starts at 0
        // and increments; EnumAdapters returns DXGI_ERROR_NOT_FOUND when the index is exhausted.
        let adapter = unsafe { factory.EnumAdapters(adapter_index) };

        match adapter {
            Ok(adapter) => {
                // Get adapter description using mutable pointer
                let mut desc = MaybeUninit::<DXGI_ADAPTER_DESC>::uninit();
                // SAFETY: `adapter` is a valid IDXGIAdapter. `desc.as_mut_ptr()` is a valid
                // pointer to an uninitialized DXGI_ADAPTER_DESC that GetDesc will fully initialize.
                let result = unsafe { adapter.GetDesc(desc.as_mut_ptr()) };

                if result.is_ok() {
                    // SAFETY: GetDesc returned Ok, which means it has fully initialized `desc`.
                    let desc = unsafe { desc.assume_init() };

                    // Convert wide string to Rust String
                    let description = String::from_utf16_lossy(&desc.Description)
                        .trim_end_matches('\0')
                        .to_string();

                    // Get dedicated video memory (in bytes, 64-bit value - no truncation!)
                    let vram_bytes = desc.DedicatedVideoMemory as u64;
                    let vram_mb: u64 = vram_bytes / (1024 * 1024);

                    if vram_mb > 0 {
                        tracing::debug!(
                            "DXGI adapter {}: {} - {} MB VRAM",
                            adapter_index,
                            description,
                            vram_mb
                        );

                        // Store both original and normalized names
                        vram_map.insert(description.clone(), vram_mb);
                        vram_map.insert(normalize_gpu_name(&description), vram_mb);
                    }
                }

                adapter_index += 1;
            }
            Err(e) => {
                // DXGI_ERROR_NOT_FOUND means we've enumerated all adapters
                if e.code() == DXGI_ERROR_NOT_FOUND {
                    break;
                }
                tracing::debug!("Error enumerating DXGI adapter {}: {:?}", adapter_index, e);
                break;
            }
        }
    }

    if !vram_map.is_empty() {
        tracing::info!(
            "DXGI detected {} GPU(s) with accurate VRAM data",
            vram_map.len() / 2
        );
    }

    vram_map
}

/// Detect OS version
pub fn detect_os_version() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        let output = Command::new("powershell")
            .args([
                "-Command",
                "(Get-WmiObject -Class Win32_OperatingSystem).Caption",
            ])
            .output()
            .ok()?;
        String::from_utf8(output.stdout)
            .ok()
            .map(|s| s.trim().to_string())
    }

    #[cfg(target_os = "linux")]
    {
        // Try /etc/os-release first
        if let Ok(content) = fs::read_to_string("/etc/os-release") {
            for line in content.lines() {
                if line.starts_with("PRETTY_NAME=") {
                    return Some(line.split('=').nth(1)?.trim_matches('"').to_string());
                }
            }
        }
        None
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        None
    }
}

/// Detect kernel version
pub fn detect_kernel_version() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        let output = Command::new("powershell")
            .args([
                "-Command",
                "(Get-WmiObject -Class Win32_OperatingSystem).Version",
            ])
            .output()
            .ok()?;
        String::from_utf8(output.stdout)
            .ok()
            .map(|s| s.trim().to_string())
    }

    #[cfg(target_os = "linux")]
    {
        let output = Command::new("uname").arg("-r").output().ok()?;
        String::from_utf8(output.stdout)
            .ok()
            .map(|s| s.trim().to_string())
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        None
    }
}

/// Detect swap space in MB
pub fn detect_swap() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        if let Ok(content) = fs::read_to_string("/proc/meminfo") {
            for line in content.lines() {
                if line.starts_with("SwapTotal:") {
                    return line
                        .split_whitespace()
                        .nth(1)
                        .and_then(|s| s.parse::<u64>().ok())
                        .map(|kb| kb / 1024); // Convert kB to MB
                }
            }
        }
        None
    }

    #[cfg(target_os = "windows")]
    {
        let output = Command::new("powershell")
            .args(["-Command", 
                "(Get-WmiObject -Class Win32_PageFileUsage | Measure-Object -Property AllocatedBaseSize -Sum).Sum"])
            .output()
            .ok()?;
        String::from_utf8(output.stdout)
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        None
    }
}

#[cfg(target_os = "linux")]
#[expect(dead_code)]
fn detect_windows_gpus() -> Result<Vec<GpuInfo>> {
    anyhow::bail!("Windows GPU detection not available on this platform")
}

/// Detect which container runtime is available (Docker or Podman)
pub fn detect_container_runtime() -> Option<String> {
    // Try Docker first (most common)
    if Command::new("docker")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return Some("docker".to_string());
    }

    // Try Podman
    if Command::new("podman")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return Some("podman".to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_stone_resources() {
        let resources = collect_stone_resources().unwrap();
        assert!(resources.cpu.cores > 0);
        assert!(resources.memory.total_bytes > 0);
    }
}
