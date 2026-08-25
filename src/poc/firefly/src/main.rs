// Garden Firefly — visual status indicator companion.
// Three-domain architecture per COMPANION-0018.

use anyhow::Result;
use clap::{Parser, Subcommand};
use garden_companion_sdk::garden::{CommandTransport, SseTransport};
use garden_companion_sdk::prelude::*;
use garden_companion_usb::{Monitor, UsbRegistry};
use garden_companion_sdk::{check_dump_commands, CommandArg, CommandDef, CommandManifest};

mod adapters;
mod animation;
mod firefly;
mod orchestrator;
mod probe;

use orchestrator::FireflyOrchestrator;

fn build_manifest() -> CommandManifest {
    CommandManifest::new(
        "firefly",
        "Garden Firefly",
        env!("CARGO_PKG_VERSION"),
        "Visual status indicator (RP2040-Matrix + OLED v1/v2 + T-Display)",
    )
    .command(
        CommandDef::new("status", "Show status indicator")
            .arg(CommandArg::required_string(
                "state",
                "Status state: healthy, warning, error, or offline",
            ))
            .example("Show healthy status", "hey tell firefly status healthy"),
    )
    .command(
        CommandDef::new("pixel", "Set single pixel color")
            .arg(CommandArg::required_int("x", "X coordinate (0-4)", 0, 4))
            .arg(CommandArg::required_int("y", "Y coordinate (0-4)", 0, 4))
            .arg(CommandArg::required_string("color", "Color as hex or r,g,b"))
            .example("Red center pixel", "hey tell firefly pixel 2 2 ff0000"),
    )
    .command(
        CommandDef::new("fill", "Fill all pixels with color")
            .arg(CommandArg::required_string("color", "Color as hex or r,g,b"))
            .example("Fill with green", "hey tell firefly fill 00ff00"),
    )
    .command(
        CommandDef::new("clear", "Turn off all LEDs")
            .example("Clear display", "hey tell firefly clear"),
    )
    .command(
        CommandDef::new("brightness", "Set LED brightness")
            .arg(CommandArg::required_int("percent", "Brightness 0-100%", 0, 100))
            .example("Set to 50%", "hey tell firefly brightness 50"),
    )
    .command(
        CommandDef::new("animate", "Start animation")
            .arg(CommandArg::required_string(
                "name",
                "Animation: rainbow, pulse, chase, or sparkle",
            ))
            .example("Rainbow cycle", "hey tell firefly animate rainbow"),
    )
    .command(CommandDef::new("stop", "Stop current animation"))
    .command(CommandDef::new("info", "Show device information"))
}

#[derive(Parser, Debug)]
#[command(name = "garden-firefly")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[arg(short, long, env = "GARDEN_STONE")]
    stone: Option<String>,

    #[arg(long, env = "FIREFLY_HTTP_PORT")]
    port: Option<u16>,

    #[arg(long, env = "FIREFLY_STATE_DIR")]
    state_dir: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// List available serial ports.
    Ports,
}

const BAUD: u32 = 115_200;

#[tokio::main]
async fn main() -> Result<()> {
    check_dump_commands(&build_manifest());
    garden_companion_sdk::init_tracing();

    let cli = Cli::parse();

    if let Some(Commands::Ports) = cli.command {
        return list_ports();
    }

    let stone = cli
        .stone
        .ok_or_else(|| anyhow::anyhow!("--stone endpoint required"))?;
    let port = cli
        .port
        .ok_or_else(|| anyhow::anyhow!("--port required (assigned by Moss)"))?;

    let state_dir = cli.state_dir.as_deref().map(std::path::PathBuf::from);

    tracing::info!(
        stone = %stone,
        port = port,
        "Starting Garden Firefly (three-domain architecture; COMPANION-0018)"
    );

    let mut companion = Companion::new("firefly")
        .with_transport(SseTransport::new(stone))
        .with_transport(CommandTransport::new(port));
    if let Some(dir) = cli.state_dir.as_deref() {
        companion = companion.with_state_dir(dir);
    }

    let adapters = companion.adapters();
    let shutdown = companion.shutdown_token();

    // USB devices domain.
    let registry = UsbRegistry::new(BAUD);
    let monitor: Box<dyn Monitor> = new_monitor()?;
    let registry_handle = {
        let registry = std::sync::Arc::clone(&registry);
        let shutdown = shutdown.clone();
        tokio::spawn(async move { registry.run(monitor, shutdown).await })
    };

    // Firefly orchestrator.
    let orchestrator = FireflyOrchestrator::new(registry, adapters, state_dir);
    let orchestrator_handle = {
        let shutdown = shutdown.clone();
        tokio::spawn(async move { orchestrator.run(shutdown).await })
    };

    let result = companion.run().await;
    let _ = registry_handle.await;
    let _ = orchestrator_handle.await;
    result
}

// Event-driven libudev hotplug only on glibc Linux. musl (the Android/arm64 Stone) has no libudev,
// so it uses the cross-platform PollMonitor — the same path Windows/macOS use.
#[cfg(all(target_os = "linux", not(target_env = "musl")))]
fn new_monitor() -> Result<Box<dyn Monitor>> {
    use garden_companion_usb::UdevMonitor;
    Ok(Box::new(UdevMonitor::new()?))
}

#[cfg(not(all(target_os = "linux", not(target_env = "musl"))))]
fn new_monitor() -> Result<Box<dyn Monitor>> {
    use garden_companion_usb::PollMonitor;
    Ok(Box::new(PollMonitor::new()))
}

fn list_ports() -> Result<()> {
    let ports = serialport::available_ports()?;
    if ports.is_empty() {
        println!("No serial ports found");
        return Ok(());
    }
    for port in &ports {
        let label = match &port.port_type {
            serialport::SerialPortType::UsbPort(info) => format!(
                "USB {:04x}:{:04x} {}",
                info.vid,
                info.pid,
                info.product.as_deref().unwrap_or("Unknown")
            ),
            other => format!("{other:?}"),
        };
        println!("  {} - {}", port.port_name, label);
    }
    Ok(())
}
