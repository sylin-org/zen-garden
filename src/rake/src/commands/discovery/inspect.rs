//! Inspect command — deep hardware topology view (ARCH-0014).
//!
//! Displays Tier 1 (core) + Tier 2 (topology) hardware capabilities for
//! a single stone, or aggregates across the garden.
//!
//! Options:
//!   --save <path>   Save raw JSON to file
//!   --json          Output raw JSON instead of formatted text

use crate::commands::{Command, CommandResult};
use crate::context::Context;
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
        ctx: &'a Context,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = CommandResult> + Send + 'a>> {
        Box::pin(async move {
            let api = ctx.api();
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

/// Garden-wide hardware inspection — fan-out to all stones.
pub struct InspectAllCommand {
    /// Output file path for JSON results.
    pub output_path: String,
    /// Output raw JSON to stdout instead of summary.
    pub json: bool,
    /// Show every stone individually — no grouping, no top-3 truncation.
    pub expanded: bool,
    pub quiet: bool,
}

impl InspectAllCommand {
    pub fn new(output_path: String, json: bool, expanded: bool, quiet: bool) -> Self {
        Self {
            output_path,
            json,
            expanded,
            quiet,
        }
    }
}

impl Command for InspectAllCommand {
    fn execute<'a>(
        &'a self,
        ctx: &'a Context,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = CommandResult> + Send + 'a>> {
        Box::pin(async move {
            let api = ctx.api();
            let inspection = api.garden().inspect().await?;

            // Always write JSON to file
            let json_output = serde_json::to_string_pretty(&inspection)?;
            std::fs::write(&self.output_path, &json_output)?;

            if self.json {
                println!("{}", json_output);
                return Ok(());
            }

            // Header
            println!(
                "{}",
                ui::section_header_v2("GARDEN INSPECTION", false, ctx.term.supports_color)
            );
            let ts = inspection.inspected_at.split('T').next().unwrap_or(&inspection.inspected_at);
            println!(
                "  {} inspected, {} unreachable -- {}",
                inspection.summary.inspected,
                inspection.summary.unreachable,
                ts,
            );
            println!();

            // Build per-stone summaries for leaderboards and fleet
            let summaries: Vec<StoneSummary> = inspection
                .stones
                .iter()
                .map(StoneSummary::from_inspection)
                .collect();

            render_all(&summaries, &inspection.unreachable, &ctx.term, self.expanded);

            println!();
            println!("  Saved to {}", self.output_path);

            Ok(())
        })
    }

    fn requires_endpoint(&self) -> bool {
        true
    }

    fn show_stone_header(&self) -> bool {
        false
    }

    fn name(&self) -> &'static str {
        "inspect all"
    }
}

// ============================================================================
// Stone summary for rendering
// ============================================================================

struct StoneSummary {
    name: String,
    model_label: String,
    cores: usize,
    ram_mb: u64,
    /// Memory type tag for RAM leaderboard. e.g., "DDR4-3200 SODIMM"
    ram_tag: String,
    disk_gb: u64,
    disk_type: String,
    gpu_name: Option<String>,
    vram_mb: u64,
    m2_free: usize,
    dimm_free: usize,
    network_speed: Option<String>,
    /// Notes column: CPU, GPU+VRAM, free slots — comma-separated.
    notes: String,
    /// Fingerprint for coalescing: all visible columns must match.
    fingerprint: String,
}

impl StoneSummary {
    fn from_inspection(
        stone: &garden_common::types::hardware_topology::StoneInspection,
    ) -> Self {
        let hw = &stone.capabilities.core.hardware;
        let topo = stone.capabilities.topology.as_ref();

        let cpu_model = hw.cpu.model.as_deref().unwrap_or("Unknown");
        let cpu_short = shorten_cpu(cpu_model);

        // System model: prefer product, fall back to board, then manufacturer + CPU
        let sys = topo.map(|t| &t.system);
        let model_label = derive_model_label(sys, &cpu_short);

        let disk_type = hw.disk.as_ref()
            .and_then(|d| d.disk_type.as_deref())
            .unwrap_or("?")
            .to_string();

        // GPU: first discrete GPU with VRAM
        let (gpu_name, vram_mb) = hw
            .gpus
            .iter()
            .find(|g| g.vram_mb.unwrap_or(0) > 0)
            .map(|g| {
                let name = shorten_gpu(&g.model);
                (Some(name), g.vram_mb.unwrap_or(0))
            })
            .unwrap_or((None, 0));

        // M.2 slots
        let m2_slots = topo
            .map(|t| &t.expansion.m2)
            .map(|slots| slots.as_slice())
            .unwrap_or(&[]);
        let m2_free = m2_slots.iter().filter(|s| !s.in_use).count();

        // Memory topology: DDR type, speed, form factor, free slots
        let mem_topo = topo.map(|t| &t.memory);
        let populated_slot = mem_topo
            .and_then(|m| m.slots.iter().find(|s| s.populated));
        let ram_tag = {
            let mut parts = Vec::new();
            if let Some(slot) = populated_slot {
                if let Some(ref mtype) = slot.memory_type {
                    let mut ddr = mtype.clone();
                    if let Some(speed) = slot.speed_mts {
                        ddr.push_str(&format!("-{}", speed));
                    }
                    parts.push(ddr);
                }
                if let Some(ref ff) = slot.form_factor {
                    parts.push(ff.clone());
                }
            }
            parts.join(" ")
        };
        let dimm_free = mem_topo
            .map(|m| m.slots.iter().filter(|s| !s.populated).count())
            .unwrap_or(0);

        // Network: fastest active link
        let network_speed = topo
            .and_then(|t| {
                t.network
                    .iter()
                    .filter_map(|n| n.speed_mbps)
                    .max()
            })
            .map(|s| {
                if s >= 1000 {
                    format!("{} Gbps", s / 1000)
                } else {
                    format!("{} Mbps", s)
                }
            });

        // Build notes: CPU short, GPU+VRAM, free slots
        let notes = build_notes(&cpu_short, &gpu_name, vram_mb, m2_free, dimm_free);

        // Fingerprint includes all visible columns + notes (which contains CPU)
        let fingerprint = format!(
            "{}|{}|{}|{}|{}|{}",
            model_label,
            hw.cpu.cores,
            hw.memory.total_mb,
            hw.disk.as_ref().map(|d| d.total_gb).unwrap_or(0),
            disk_type,
            notes,
        );

        Self {
            name: stone.name.clone(),
            model_label,
            cores: hw.cpu.cores,
            ram_mb: hw.memory.total_mb,
            ram_tag,
            disk_gb: hw.disk.as_ref().map(|d| d.total_gb).unwrap_or(0),
            disk_type,
            gpu_name,
            vram_mb,
            m2_free,
            dimm_free,
            network_speed,
            notes,
            fingerprint,
        }
    }

    fn ram_gb(&self) -> f64 {
        self.ram_mb as f64 / 1024.0
    }

    fn vram_gb(&self) -> f64 {
        self.vram_mb as f64 / 1024.0
    }
}

fn build_notes(
    cpu_short: &str,
    gpu_name: &Option<String>,
    vram_mb: u64,
    m2_free: usize,
    dimm_free: usize,
) -> String {
    let mut parts: Vec<String> = Vec::new();

    parts.push(cpu_short.to_string());

    if let Some(gpu) = gpu_name {
        if vram_mb > 0 {
            parts.push(format!("{} {:.1}G", gpu, vram_mb as f64 / 1024.0));
        } else {
            parts.push(gpu.clone());
        }
    }

    // Free expansion slots
    let mut free_slots = Vec::new();
    if m2_free > 0 {
        free_slots.push("M.2".to_string());
    }
    if dimm_free > 0 {
        free_slots.push(format!("{} DIMM", dimm_free));
    }
    if !free_slots.is_empty() {
        parts.push(format!("{} free", free_slots.join("+")));
    }

    parts.join(", ")
}

// ============================================================================
// 3-pass rendering: collect, measure, print
// ============================================================================

const BAR_WIDTH: usize = 30;
const SECTION_WIDTH: usize = 76;

fn section_line(label: &str) -> String {
    let rest = SECTION_WIDTH.saturating_sub(label.len() + 1);
    format!("{} {}", label, "\u{2501}".repeat(rest))
}

fn render_all(
    stones: &[StoneSummary],
    unreachable: &[garden_common::types::hardware_topology::UnreachableStone],
    term: &ui::TerminalInfo,
    expanded: bool,
) {
    // ── Pass 1: measure column widths across all data ──
    let w_name = stones
        .iter()
        .map(|s| s.name.len())
        .chain(unreachable.iter().map(|u| u.name.len()))
        .max()
        .unwrap_or(4)
        .max(4); // minimum "4x" width

    let w_model = stones
        .iter()
        .map(|s| s.model_label.len())
        .max()
        .unwrap_or(0);

    // ── Pass 2: render leaderboards ──

    // GPU
    let mut with_gpu: Vec<&StoneSummary> = stones.iter().filter(|s| s.gpu_name.is_some()).collect();
    with_gpu.sort_by(|a, b| b.vram_mb.cmp(&a.vram_mb));

    println!(
        "  {}",
        ui::colored_text(&section_line("GPU"), "dim", term)
    );
    if with_gpu.is_empty() {
        println!("  No discrete GPUs detected");
    } else {
        let max_vram = with_gpu[0].vram_mb.max(1);
        let show = if expanded { with_gpu.len() } else { 3.min(with_gpu.len()) };
        for s in with_gpu.iter().take(show) {
            let bar = render_bar(s.vram_mb, max_vram, BAR_WIDTH);
            println!(
                "  {:w_name$}  {} {:5.1}G  {}",
                s.name, bar, s.vram_gb(), s.gpu_name.as_deref().unwrap_or(""),
            );
        }
        let rest = stones.len() - with_gpu.len();
        if rest > 0 && !expanded {
            println!("  {} stones without discrete GPU", rest);
        }
    }
    println!();

    // RAM
    let mut by_ram: Vec<&StoneSummary> = stones.iter().collect();
    by_ram.sort_by(|a, b| b.ram_mb.cmp(&a.ram_mb));

    println!(
        "  {}",
        ui::colored_text(&section_line("RAM"), "dim", term)
    );
    let max_ram = by_ram.first().map(|s| s.ram_mb).unwrap_or(1).max(1);
    let show = if expanded { by_ram.len() } else { 3.min(by_ram.len()) };
    for s in by_ram.iter().take(show) {
        let bar = render_bar(s.ram_mb, max_ram, BAR_WIDTH);
        if s.ram_tag.is_empty() {
            println!("  {:w_name$}  {} {:5.1}G", s.name, bar, s.ram_gb());
        } else {
            println!(
                "  {:w_name$}  {} {:5.1}G  {}",
                s.name, bar, s.ram_gb(), s.ram_tag,
            );
        }
    }
    if !expanded && by_ram.len() > 3 {
        let lo = by_ram.last().unwrap().ram_gb();
        let hi = by_ram[3].ram_gb();
        println!(
            "  ...{} stones between {:.1}G and {:.1}G",
            by_ram.len() - 3,
            lo,
            hi,
        );
    }
    println!();

    // DISK
    let mut by_disk: Vec<&StoneSummary> = stones.iter().collect();
    by_disk.sort_by(|a, b| b.disk_gb.cmp(&a.disk_gb));

    println!(
        "  {}",
        ui::colored_text(&section_line("DISK"), "dim", term)
    );
    let show = if expanded { by_disk.len() } else { 3.min(by_disk.len()) };
    for s in by_disk.iter().take(show) {
        let dtype = if s.disk_type == "Unknown" { "NVMe" } else { &s.disk_type };
        println!("  {:w_name$}  {:>5}G  {}", s.name, s.disk_gb, dtype);
    }
    if !expanded && by_disk.len() > 3 {
        let lo = by_disk.last().unwrap().disk_gb;
        let hi = by_disk[3].disk_gb;
        println!(
            "  ...{} stones between {}G and {}G",
            by_disk.len() - 3,
            lo,
            hi,
        );
    }
    println!();

    // NETWORK
    println!(
        "  {}",
        ui::colored_text(&section_line("NETWORK"), "dim", term)
    );
    let mut speed_counts: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    for s in stones {
        let speed = s.network_speed.as_deref().unwrap_or("unknown");
        *speed_counts.entry(speed.to_string()).or_default() += 1;
    }
    for (speed, count) in speed_counts.iter().rev() {
        println!("  {} stones at {}", count, speed);
    }
    println!();

    // ── Pass 3: fleet table (coalesced) ──

    println!(
        "  {}",
        ui::colored_text(&section_line("FLEET"), "dim", term)
    );

    // Sort by cores desc, RAM desc, disk desc
    let mut sorted: Vec<&StoneSummary> = stones.iter().collect();
    sorted.sort_by(|a, b| {
        b.cores
            .cmp(&a.cores)
            .then(b.ram_mb.cmp(&a.ram_mb))
            .then(b.disk_gb.cmp(&a.disk_gb))
    });

    if expanded {
        // Expanded: every stone on its own row
        for s in &sorted {
            println!(
                "  {:w_name$}  {:w_model$}  {:>3}c {:5.1}G  {:>5}G {:4}  {}",
                s.name, s.model_label, s.cores, s.ram_gb(), s.disk_gb, s.disk_type, s.notes,
            );
        }
    } else {
        // Coalesce by fingerprint, preserving sort order
        let mut groups: Vec<Vec<&StoneSummary>> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for s in &sorted {
            if seen.contains(&s.fingerprint) {
                continue;
            }
            seen.insert(s.fingerprint.clone());
            let group: Vec<&StoneSummary> = sorted
                .iter()
                .filter(|x| x.fingerprint == s.fingerprint)
                .copied()
                .collect();
            groups.push(group);
        }

        for group in &groups {
            let s = group[0];
            let count = group.len();

            let name_col = if count == 1 {
                s.name.clone()
            } else {
                format!("{}x", count)
            };

            println!(
                "  {:w_name$}  {:w_model$}  {:>3}c {:5.1}G  {:>5}G {:4}  {}",
                name_col, s.model_label, s.cores, s.ram_gb(), s.disk_gb, s.disk_type, s.notes,
            );
        }
    }

    // Unreachable
    if !unreachable.is_empty() {
        println!();
        for u in unreachable {
            println!(
                "  {:w_name$}  {}",
                u.name,
                ui::colored_text("unreachable", "red", term),
            );
        }
    }

    // ── Totals ──
    let total_cores: usize = stones.iter().map(|s| s.cores).sum();
    let total_ram: f64 = stones.iter().map(|s| s.ram_gb()).sum();
    let total_vram: f64 = stones.iter().map(|s| s.vram_gb()).sum();
    let total_disk: u64 = stones.iter().map(|s| s.disk_gb).sum();
    let total_m2_free: usize = stones.iter().map(|s| s.m2_free).sum();
    let total_dimm_free: usize = stones.iter().map(|s| s.dimm_free).sum();

    println!();
    println!(
        "  TOTALS  {} cores  |  {:.0} GB RAM  |  {:.0} GB VRAM  |  {:.1} TB disk",
        total_cores, total_ram, total_vram, total_disk as f64 / 1000.0,
    );
    let mut free_parts = Vec::new();
    if total_m2_free > 0 {
        free_parts.push(format!("{} M.2", total_m2_free));
    }
    if total_dimm_free > 0 {
        free_parts.push(format!("{} DIMM", total_dimm_free));
    }
    if !free_parts.is_empty() {
        println!("  {} slots free", free_parts.join(" + "));
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn render_bar(value: u64, max: u64, width: usize) -> String {
    if max == 0 {
        return format!("[{:width$}]", "", width = width);
    }
    let filled = ((value as f64 / max as f64) * width as f64).round() as usize;
    let filled = filled.min(width);
    let empty = width - filled;
    format!(
        "[{}{}]",
        "\u{2588}".repeat(filled),
        " ".repeat(empty),
    )
}

fn shorten_cpu(model: &str) -> String {
    let s = model
        .replace("Intel(R) Core(TM) ", "")
        .replace("Intel(R) Pentium(R) Silver ", "Pentium ")
        .replace("Intel(R) Pentium(R) ", "Pentium ")
        .replace("Intel(R) Celeron(R) ", "Celeron ")
        .replace("12th Gen Intel(R) Core(TM) ", "")
        .replace("13th Gen Intel(R) Core(TM) ", "")
        .replace("14th Gen Intel(R) Core(TM) ", "")
        .replace("AMD Embedded G-Series ", "")
        .replace("AMD GX-", "GX-")
        .replace(" CPU", "")
        .replace(" Processor", "")
        .replace(" 8-Core", "")
        .replace(" 16-Core", "")
        .replace(" @ ", " ");
    // Trim trailing frequency like " 3.60GHz"
    if let Some(pos) = s.rfind(" with ") {
        s[..pos].trim().to_string()
    } else {
        s.trim().to_string()
    }
}

fn shorten_gpu(model: &str) -> String {
    model
        .replace("AMD Radeon ", "")
        .replace("NVIDIA GeForce ", "")
        .replace("Advanced Micro Devices, Inc. [AMD/ATI] ", "AMD ")
        .replace("Intel Corporation ", "Intel ")
        .trim()
        .to_string()
}

/// Derive a display label for the system model.
///
/// Priority: product name > board product > manufacturer + CPU.
/// Filters out generic BIOS placeholders.
fn derive_model_label(
    sys: Option<&garden_common::types::hardware_topology::SystemIdentity>,
    cpu_short: &str,
) -> String {
    let Some(sys) = sys else {
        return cpu_short.to_string();
    };

    let product = &sys.product;

    // Check if product is a real name (not a BIOS placeholder)
    let is_generic = product.is_empty()
        || product.contains("System Product Name")
        || product.contains("To Be Filled")
        || product.contains("Default string")
        || product.contains("To be filled");

    if !is_generic {
        return product.clone();
    }

    // Fall back to board product
    if let Some(ref board) = sys.board_product {
        let board_generic = board.contains("Default string")
            || board.contains("To Be Filled")
            || board.is_empty();
        if !board_generic {
            return board.clone();
        }
    }

    // Last resort: manufacturer + short CPU
    format!("{} {}", sys.manufacturer, cpu_short)
}

fn render_pcie(topo: &HardwareTopology, ctx: &Context) {
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

fn render_network(topo: &HardwareTopology, ctx: &Context) {
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

fn render_usb(topo: &HardwareTopology, ctx: &Context) {
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

fn render_firmware(topo: &HardwareTopology, ctx: &Context) {
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
