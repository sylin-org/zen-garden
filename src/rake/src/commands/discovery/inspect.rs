//! Inspect command — deep hardware topology view (ARCH-0014).
//!
//! Displays Tier 1 (core) + Tier 2 (topology) hardware capabilities for
//! a single stone, or aggregates across the garden.
//!
//! Options:
//!   --save <path>   Save raw JSON to file
//!   --json          Output raw JSON instead of formatted text

use crate::commands::{Command, CommandResult};
use crate::context::Runtime;
use crate::ui::rendering as ui;
use garden_common::types::hardware_topology::{FullCapabilities, HardwareTopology, PcieDevice};

/// Display detailed hardware inspection for a stone.
pub struct InspectCommand {
    /// Save raw JSON to this path (if set).
    pub save_path: Option<String>,
    /// Output raw JSON instead of formatted text.
    pub json: bool,
    pub quiet: bool,
}

impl InspectCommand {
    pub fn new(save_path: Option<String>, json: bool, quiet: bool) -> Self {
        Self {
            save_path,
            json,
            quiet,
        }
    }
}

impl Command for InspectCommand {
    fn execute<'a>(
        &'a self,
        ctx: &'a Runtime,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = CommandResult> + Send + 'a>> {
        Box::pin(async move {
            let api = ctx.stone_api()?;
            let full: FullCapabilities = api.stone().capabilities().await?;

            // Save raw JSON if requested
            if let Some(ref path) = self.save_path {
                let json = serde_json::to_string_pretty(&full)?;
                std::fs::write(path, &json)?;
                println!("Saved to {}", path);
                if !self.json {
                    return Ok(());
                }
            }

            // Raw JSON output
            if self.json {
                println!("{}", serde_json::to_string_pretty(&full)?);
                return Ok(());
            }

            // === Formatted output ===
            let core = &full.core;

            // Header
            println!(
                "{}",
                ui::section_header_v2("HARDWARE INSPECTION", false, ctx.term.supports_color)
            );
            println!(
                "  Stone:  {}  ({})",
                core.stone_name,
                core.stone_id.as_deref().unwrap_or("?")
            );

            // System identity (Tier 2)
            if let Some(ref topo) = full.topology {
                let sys = &topo.system;
                println!(
                    "  System: {} {}",
                    sys.manufacturer, sys.product
                );
                if let Some(ref chassis) = sys.chassis_type {
                    print!("  Type:   {}", chassis);
                }
                if let Some(ref serial) = sys.serial {
                    print!("  Serial: {}", serial);
                }
                println!();
                if let Some(ref bios) = sys.bios_version {
                    let date = sys.bios_date.as_deref().unwrap_or("");
                    println!("  BIOS:   {} ({})", bios, date);
                }
            }
            println!();

            // CPU
            println!(
                "{}",
                ui::section_header_v2("CPU", false, ctx.term.supports_color)
            );
            let cpu = &core.hardware.cpu;
            println!(
                "  {}",
                cpu.model.as_deref().unwrap_or("Unknown")
            );
            println!(
                "  {} cores, {} threads, {}",
                cpu.cores,
                cpu.threads.unwrap_or(cpu.cores),
                cpu.architecture
            );
            if let Some(ref features) = cpu.features {
                let key_features: Vec<&str> = features
                    .iter()
                    .filter(|f| {
                        matches!(
                            f.as_str(),
                            "avx" | "avx2" | "avx512" | "sse4_2" | "fma" | "aes"
                        )
                    })
                    .map(|f| f.as_str())
                    .collect();
                if !key_features.is_empty() {
                    println!("  Features: {}", key_features.join(", "));
                }
            }
            println!();

            // Memory
            println!(
                "{}",
                ui::section_header_v2("MEMORY", false, ctx.term.supports_color)
            );
            let mem_gb = core.hardware.memory.total_mb as f64 / 1024.0;
            println!("  {:.1} GB", mem_gb);
            println!();

            // GPUs
            if !core.hardware.gpus.is_empty() {
                println!(
                    "{}",
                    ui::section_header_v2("GPU", false, ctx.term.supports_color)
                );
                for gpu in &core.hardware.gpus {
                    let vram = gpu
                        .vram_mb
                        .map(|v| format!("{:.1} GB", v as f64 / 1024.0))
                        .unwrap_or_else(|| "? VRAM".to_string());
                    println!("  {} ({}) [{}]", gpu.model, vram, gpu.capabilities.join(", "));
                }
                println!();
            }

            // Tier 2 sections
            if let Some(ref topo) = full.topology {
                render_pcie(topo, ctx);
                render_network(topo, ctx);
                render_usb(topo, ctx);
                render_firmware(topo, ctx);

                println!(
                    "  Fingerprint: {}",
                    &topo.fingerprint[..16.min(topo.fingerprint.len())]
                );
                println!("  Probed:      {}", topo.probed_at);
            } else {
                println!(
                    "  {}",
                    ui::colored_text(
                        "Tier 2 topology not yet available (probe in progress...)",
                        "yellow",
                        &ctx.term,
                    )
                );
                println!("  Trigger with: rake inspect --refresh");
            }

            Ok(())
        })
    }

    fn requires_endpoint(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "inspect"
    }
}

fn render_pcie(topo: &HardwareTopology, ctx: &Runtime) {
    let active: Vec<&PcieDevice> = topo
        .expansion
        .pcie
        .iter()
        .filter(|d| d.bandwidth_gbps > 0.0)
        .collect();

    if active.is_empty() && topo.expansion.thunderbolt.is_empty() {
        return;
    }

    println!(
        "{}",
        ui::section_header_v2("PCIe DEVICES", false, ctx.term.supports_color)
    );

    for dev in &active {
        let name = dev
            .device_name
            .as_deref()
            .unwrap_or(dev.device_id.as_str());
        println!(
            "  {:14}  {:44}  x{} Gen{}  {:.0} Gbps",
            dev.address, name, dev.negotiated_width, dev.generation, dev.bandwidth_gbps,
        );
    }

    let bridge_count = topo.expansion.pcie.len() - active.len();
    if bridge_count > 0 {
        println!("  + {} chipset/bridge devices", bridge_count);
    }

    // Thunderbolt
    for tb in &topo.expansion.thunderbolt {
        println!(
            "  {} v{}  {:.0} Gbps",
            tb.kind, tb.version, tb.bandwidth_gbps
        );
    }

    // M.2 slots
    if !topo.expansion.m2.is_empty() {
        println!();
        for slot in &topo.expansion.m2 {
            let status = if slot.in_use { "occupied" } else { "EMPTY" };
            let occupant = slot
                .occupant
                .as_deref()
                .map(|o| format!("  ({})", o))
                .unwrap_or_default();
            println!(
                "  M.2 {:12}  Key {:4}  {}{}",
                slot.designation, slot.key, status, occupant
            );
        }
    }

    println!();
}

fn render_network(topo: &HardwareTopology, ctx: &Runtime) {
    if topo.network.is_empty() {
        return;
    }

    println!(
        "{}",
        ui::section_header_v2("NETWORK", false, ctx.term.supports_color)
    );

    for nic in &topo.network {
        let speed = nic
            .speed_mbps
            .map(|s| {
                if s >= 1000 {
                    format!("{} Gbps", s / 1000)
                } else {
                    format!("{} Mbps", s)
                }
            })
            .unwrap_or_else(|| "down".to_string());
        let mac = nic.mac.as_deref().unwrap_or("");
        let fw = nic
            .firmware_version
            .as_deref()
            .map(|f| format!("  fw={}", f))
            .unwrap_or_default();
        println!(
            "  {:15}  {:10}  {:>10}  {:17}{}",
            nic.name, nic.kind, speed, mac, fw
        );
    }
    println!();
}

fn render_usb(topo: &HardwareTopology, ctx: &Runtime) {
    let usb = &topo.expansion.usb;
    if usb.ports.is_empty() && usb.connected_devices.is_empty() {
        return;
    }

    println!(
        "{}",
        ui::section_header_v2("USB", false, ctx.term.supports_color)
    );

    for group in &usb.ports {
        println!("  USB {:8}  {} port(s)", group.version, group.count);
    }
    println!(
        "  {} device(s) connected",
        usb.connected_devices.len()
    );
    println!();
}

fn render_firmware(topo: &HardwareTopology, ctx: &Runtime) {
    if topo.firmware.is_empty() {
        return;
    }

    println!(
        "{}",
        ui::section_header_v2("FIRMWARE", false, ctx.term.supports_color)
    );

    for fw in &topo.firmware {
        let name = fw
            .device_name
            .as_deref()
            .unwrap_or(fw.component.as_str());
        println!("  {:40}  {}", name, fw.version);
    }
    println!();
}
