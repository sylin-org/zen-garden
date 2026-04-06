//! Status command - display stone system information
//!
//! Shows detailed system information including hardware, storage, AI devices, and health.

use crate::command_manifest::cmd;
use crate::commands::{Command, CommandResult};
use crate::context::Context;
use crate::suggestions;
use crate::ui::rendering as ui;
use garden_common::DetectionStatus;

/// Display stone system status
pub struct StatusCommand {
    pub quiet: bool,
}

impl StatusCommand {
    pub fn new(quiet: bool) -> Self {
        Self { quiet }
    }
}

impl Command for StatusCommand {
    fn execute<'a>(&'a self, ctx: &'a Context) -> std::pin::Pin<Box<dyn std::future::Future<Output = CommandResult> + Send + 'a>> {
        Box::pin(async move {
            // Info notice (unless quiet mode)
            if !self.quiet {
                eprintln!(
                    "{}ℹ️  Tip: Use 'observe' for garden overview or 'tend' to set default stone",
                    " ".repeat(ui::constants::DEFAULT_INDENT)
                );
                eprintln!();
            }

            let api = ctx.api();
            let endpoint = ctx.endpoint();

            let caps = api
                .stone()
                .capabilities_core()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to get capabilities: {}", e.display_message()))?;

            let health_resp = api
                .stone()
                .health()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to get health: {}", e.display_message()))?;
            let health_text = health_resp.text().await.unwrap_or_default();

            // Parse health response
            let health_json: serde_json::Value =
                serde_json::from_str(&health_text).unwrap_or_else(|_| serde_json::json!({}));

            // Show detection status if not complete
            if caps.detection_status != DetectionStatus::Complete {
                let status_msg = match caps.detection_status {
                    DetectionStatus::Scanning => "    ⚙️  Hardware detection in progress...",
                    DetectionStatus::Partial => "    ⚙️  GPU detection in progress (CPU data ready)...",
                    DetectionStatus::Complete => "", // Won't reach here
                };
                println!("{}\n", ui::colored_text(status_msg, "yellow", &ctx.term));
            }

            // === ACCESS SECTION ===
            println!(
                "{}",
                ui::section_header_v2("ACCESS", false, ctx.term.supports_color)
            );
            // Parse endpoint to extract IP and port
            let endpoint_clean = endpoint.trim_start_matches("http://").trim_end_matches('/');
            let default_port = garden_common::constants::MOSS_HTTP.to_string();
            let (ip_addr, port) = if let Some(colon_pos) = endpoint_clean.rfind(':') {
                (
                    &endpoint_clean[..colon_pos],
                    &endpoint_clean[colon_pos + 1..],
                )
            } else {
                (endpoint_clean, default_port.as_str())
            };

            // mDNS name is stone_name.local (lowercase)
            let mdns_name = format!("{}.local", caps.stone_name.to_lowercase());

            let indent = " ".repeat(ui::constants::DEFAULT_INDENT);
            println!(
                "{}{}",
                indent,
                ui::place_value("HTTP", &format!("http://{}:{}", ip_addr, port))
            );
            println!("{}{}", indent, ui::place_value("mDNS", &mdns_name));
            println!("{}{}", indent, ui::place_value("IP", ip_addr));

            // === SYSTEM SECTION ===
            println!();
            println!(
                "{}",
                ui::section_header_v2("SYSTEM", false, ctx.term.supports_color)
            );
            println!(
                "{}{}",
                indent,
                ui::place_value("ARCH", &caps.hardware.cpu.architecture)
            );
            println!(
                "{}{}",
                indent,
                ui::place_value("CPU", &format!("{} cores", caps.hardware.cpu.cores))
            );

            // CPU flags - filter to important ones for AI/performance
            if let Some(features) = &caps.hardware.cpu.features {
                let important_flags = ["avx", "avx2", "avx512f", "sse4_2", "fma", "f16c"];
                let relevant: Vec<&str> = features
                    .iter()
                    .filter_map(|f| {
                        let lower = f.to_lowercase();
                        if important_flags.iter().any(|flag| lower.contains(flag)) {
                            Some(f.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();
                if !relevant.is_empty() {
                    println!(
                        "{}{}",
                        indent,
                        ui::place_value("FLAGS", &relevant.join(", "))
                    );
                }
            }

            println!(
                "{}{}",
                indent,
                ui::place_value(
                    "MEMORY",
                    &format!("{} GB", caps.hardware.memory.total_mb / 1024)
                )
            );

            // OS and Kernel (from RuntimeInfo)
            if let Some(ref runtime) = caps.runtime {
                let os_display = if runtime.os.contains('/') {
                    let parts: Vec<&str> = runtime.os.split('/').collect();
                    parts.get(1).unwrap_or(&runtime.os.as_str()).to_string()
                } else {
                    match runtime.os.as_str() {
                        "windows" => "Windows".to_string(),
                        "linux" => "Linux".to_string(),
                        "macos" => "macOS".to_string(),
                        other => other.to_string(),
                    }
                };
                println!("{}{}", indent, ui::place_value("OS", &os_display));

                if let Some(ref kernel_ver) = runtime.kernel {
                    println!("{}{}", indent, ui::place_value("KERNEL", kernel_ver));
                }
            }

            // === AI SECTION ===
            let ai_devices: Vec<&garden_common::GpuInfo> = caps
                .hardware
                .gpus
                .iter()
                .filter(|gpu| {
                    gpu.capabilities
                        .iter()
                        .any(|c| c == "cuda" || c == "rocm" || c == "vulkan" || c == "directml")
                })
                .collect();

            if !ai_devices.is_empty() {
                println!();
                println!(
                    "{}",
                    ui::section_header_v2("AI", false, ctx.term.supports_color)
                );
                for gpu in ai_devices {
                    let mut device_name = gpu.model.clone();
                    let prefixes = [
                        "AMD ", "NVIDIA ", "Intel ", "RADEON ", "Radeon ", "GeForce ", "GTX ", "RTX ",
                    ];
                    for prefix in &prefixes {
                        if device_name.starts_with(prefix) {
                            device_name = device_name[prefix.len()..].to_string();
                        }
                    }
                    let device_name = device_name.trim().to_string();

                    let vram_str = if let Some(vram_mb) = gpu.vram_mb {
                        format!("{} GB", vram_mb / 1024)
                    } else {
                        "Unknown".to_string()
                    };

                    let runtime_str = {
                        let formatted: Vec<String> = gpu
                            .capabilities
                            .iter()
                            .map(|c| match c.as_str() {
                                "cuda" => "CUDA".to_string(),
                                "rocm" => "ROCm".to_string(),
                                "directml" => "DirectML".to_string(),
                                "openvino" => "OpenVINO".to_string(),
                                "vulkan" => "Vulkan".to_string(),
                                "opencl" => "OpenCL".to_string(),
                                other => other.to_uppercase(),
                            })
                            .collect();
                        formatted.join(", ")
                    };

                    let value = format!("{} - {}", vram_str, runtime_str);
                    println!("{}{}", indent, ui::place_value(&device_name, &value));
                }
            }

            // === HEALTH SECTION ===
            if let Some(checks) = health_json.get("checks").and_then(|v| v.as_object()) {
                println!();
                println!(
                    "{}",
                    ui::section_header_v2("HEALTH", false, ctx.term.supports_color)
                );

                let mut passing = Vec::new();
                let mut failing = Vec::new();

                for (check_name, check_data) in checks {
                    let status = check_data
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let proper_case = if !check_name.is_empty() {
                        let mut chars = check_name.chars();
                        chars.next().unwrap().to_uppercase().collect::<String>() + chars.as_str()
                    } else {
                        check_name.to_string()
                    };

                    match status {
                        "pass" => passing.push(proper_case),
                        _ => failing.push(proper_case),
                    }
                }

                if !passing.is_empty() {
                    println!("{}{}", indent, ui::place_value("PASS", &passing.join(", ")));
                }
                if !failing.is_empty() {
                    println!("{}{}", indent, ui::place_value("FAIL", &failing.join(", ")));
                }
            }

            // Self-teaching suggestions
            suggestions::print_suggestions(cmd::STATUS, self.quiet);

            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        cmd::STATUS
    }
}
