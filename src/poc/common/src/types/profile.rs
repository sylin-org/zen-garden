//! Stone hardware profile — classifies a stone by its capabilities and intended role.
//!
//! Profiles determine which offering modes are available and what workloads
//! are appropriate. Detection is automatic from hardware capabilities but
//! can be overridden via configuration.

use serde::{Deserialize, Serialize};

use super::hardware::HardwareCapabilities;

/// Stone hardware profile.
///
/// Each profile enables a subset of offering modes and carries
/// expectations about available resources and uptime.
///
/// | Profile | Offering Modes | Docker | Always-on | Typical Hardware |
/// |---------|---------------|--------|-----------|-----------------|
/// | Hearth | planted + borrowed | Required | Yes | Dedicated servers, NUCs, thin clients |
/// | Workbench | adopted + borrowed | Optional | No | Gaming PCs, laptops, desktops |
/// | Gateway | borrowed only | No | Yes | Raspberry Pi, routers, IoT |
/// | Full | all modes | Required | Configurable | Power users, dev machines |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StoneProfile {
    /// Always-on server with Docker. Runs planted + borrowed offerings.
    Hearth,
    /// Desktop/laptop without guaranteed uptime. Adopted + borrowed only.
    Workbench,
    /// Minimal device (no Docker). Borrowed services only.
    Gateway,
    /// All modes enabled. Operator manages configuration.
    Full,
}

impl StoneProfile {
    /// Detect the recommended profile from hardware capabilities.
    ///
    /// Heuristics:
    /// - <2 GB RAM or ARM with <4 cores → Gateway (too small for containers)
    /// - Has discrete GPU and >8 GB RAM → Workbench (likely a desktop)
    /// - Otherwise → Hearth (general-purpose server)
    pub fn detect(hw: &HardwareCapabilities) -> Self {
        let memory_mb = hw.hardware.memory.total_mb;
        let cores = hw.hardware.cpu.cores;
        let has_gpu = !hw.hardware.gpus.is_empty();
        let is_arm = hw.hardware.cpu.architecture.contains("aarch64")
            || hw.hardware.cpu.architecture.contains("arm");

        // Too small for Docker
        if memory_mb < 2048 || (is_arm && cores < 4) {
            return Self::Gateway;
        }

        // Desktop with discrete GPU — likely not a dedicated server
        if has_gpu && memory_mb > 8192 {
            return Self::Workbench;
        }

        // Default: dedicated server role
        Self::Hearth
    }

    /// Whether this profile supports planted (Docker-managed) offerings.
    pub fn supports_planted(&self) -> bool {
        matches!(self, Self::Hearth | Self::Full)
    }

    /// Whether this profile supports adopted (native service) offerings.
    pub fn supports_adopted(&self) -> bool {
        matches!(self, Self::Workbench | Self::Full)
    }

    /// Whether this profile supports borrowed (external) offerings.
    pub fn supports_borrowed(&self) -> bool {
        true // all profiles support borrowed
    }

    /// Whether Docker is expected to be available.
    pub fn expects_docker(&self) -> bool {
        matches!(self, Self::Hearth | Self::Full)
    }

    /// Human-readable description.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Hearth => "Always-on server with Docker (planted + borrowed)",
            Self::Workbench => "Desktop/laptop (adopted + borrowed)",
            Self::Gateway => "Minimal device, no Docker (borrowed only)",
            Self::Full => "All modes enabled (operator-configured)",
        }
    }
}

impl std::fmt::Display for StoneProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hearth => write!(f, "hearth"),
            Self::Workbench => write!(f, "workbench"),
            Self::Gateway => write!(f, "gateway"),
            Self::Full => write!(f, "full"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::hardware::*;

    fn minimal_hw(
        memory_mb: u64,
        cores: usize,
        arch: &str,
        gpus: Vec<GpuInfo>,
    ) -> HardwareCapabilities {
        HardwareCapabilities {
            stone_id: None,
            stone_name: "test".to_string(),
            hardware: HardwareInventory {
                cpu: CpuCapabilities {
                    model: None,
                    cores,
                    threads: None,
                    architecture: arch.to_string(),
                    features: None,
                },
                memory: MemoryCapabilities {
                    total_mb: memory_mb,
                },
                gpus,
                disk: None,
                swap_mb: None,
                ai_capabilities: None,
                system_manufacturer: None,
                system_product: None,
            },
            runtime: None,
            detection_status: DetectionStatus::Complete,
        }
    }

    #[test]
    fn low_memory_is_gateway() {
        let hw = minimal_hw(1024, 4, "x86_64", vec![]);
        assert_eq!(StoneProfile::detect(&hw), StoneProfile::Gateway);
    }

    #[test]
    fn arm_few_cores_is_gateway() {
        let hw = minimal_hw(4096, 2, "aarch64", vec![]);
        assert_eq!(StoneProfile::detect(&hw), StoneProfile::Gateway);
    }

    #[test]
    fn gpu_desktop_is_workbench() {
        let gpu = GpuInfo {
            vendor: "NVIDIA".into(),
            model: "RTX 4090".into(),
            vram_mb: Some(24576),
            capabilities: vec!["cuda".into()],
        };
        let hw = minimal_hw(32768, 16, "x86_64", vec![gpu]);
        assert_eq!(StoneProfile::detect(&hw), StoneProfile::Workbench);
    }

    #[test]
    fn server_no_gpu_is_hearth() {
        let hw = minimal_hw(8192, 8, "x86_64", vec![]);
        assert_eq!(StoneProfile::detect(&hw), StoneProfile::Hearth);
    }

    #[test]
    fn all_profiles_support_borrowed() {
        assert!(StoneProfile::Hearth.supports_borrowed());
        assert!(StoneProfile::Workbench.supports_borrowed());
        assert!(StoneProfile::Gateway.supports_borrowed());
        assert!(StoneProfile::Full.supports_borrowed());
    }
}
