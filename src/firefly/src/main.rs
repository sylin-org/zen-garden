// Garden Firefly — visual status indicator companion.
// Rewritten onto the companion-sdk event mesh per COMPANION-0009.
//
// Ch1 ships only the RP2040 matrix adapter; OLED v1/v2 and T-Display
// adapters land in subsequent chapters.

use anyhow::Result;
use clap::{Parser, Subcommand};
use garden_companion_sdk::bus::DeviceBus;
use garden_companion_sdk::garden::{CommandTransport, SseTransport};
use garden_companion_sdk::prelude::*;
use garden_companion_sdk::{
    check_dump_commands, CommandArg, CommandDef, CommandManifest,
};
use std::sync::Arc;
use std::time::Duration;

mod adapters;
mod animation;
mod identity;
mod serial;

use adapters::bus_registrations;
use identity::FireflyIdentityProtocol;
use serial::{
    detect_device_type, find_firefly_device, DetectedDevice, FireflyDeviceType, FireflySerial,
};

fn build_manifest() -> CommandManifest {
    CommandManifest::new(
        "firefly",
        "Garden Firefly",
        env!("CARGO_PKG_VERSION"),
        "Visual status indicator (RP2040-Matrix; OLED v1/v2 and T-Display in subsequent chapters)",
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
            .arg(CommandArg::required_string("color", "Color as hex (ff0000) or r,g,b"))
            .example("Red center pixel", "hey tell firefly pixel 2 2 ff0000"),
    )
    .command(
        CommandDef::new("fill", "Fill all pixels with color")
            .arg(CommandArg::required_string("color", "Color as hex (ff0000) or r,g,b"))
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

    /// Pin a specific serial port (otherwise auto-discover).
    #[arg(long, env = "FIREFLY_PORT")]
    serial_port: Option<String>,

    #[arg(long, env = "FIREFLY_HTTP_PORT")]
    port: Option<u16>,

    #[arg(long, env = "FIREFLY_STATE_DIR")]
    state_dir: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// List available serial ports
    Ports,
    /// Test mode: send a sequence of commands directly to the device
    Test {
        #[arg(long)]
        port: Option<String>,
    },
    /// Probe device and show info
    Probe {
        #[arg(long)]
        port: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    check_dump_commands(&build_manifest());
    garden_companion_sdk::init_tracing();

    let cli = Cli::parse();

    if let Some(cmd) = cli.command {
        return match cmd {
            Commands::Ports => list_ports(),
            Commands::Test { port } => test_mode(port).await,
            Commands::Probe { port } => probe_device(port),
        };
    }

    let stone = cli
        .stone
        .ok_or_else(|| anyhow::anyhow!("--stone endpoint required (or use a subcommand)"))?;
    let port = cli
        .port
        .ok_or_else(|| anyhow::anyhow!("--port required (assigned by Moss)"))?;

    let state_dir = cli.state_dir.as_deref().map(std::path::PathBuf::from);

    tracing::info!(
        stone = %stone,
        port = port,
        "Starting Garden Firefly (bus-driven: matrix + oled-v1 + oled-v2 + tdisplay)"
    );

    let mut companion = Companion::new("firefly")
        .with_transport(SseTransport::new(stone))
        .with_transport(CommandTransport::new(port));
    if let Some(dir) = cli.state_dir.as_deref() {
        companion = companion.with_state_dir(dir);
    }

    // Wire the device bus to the companion's pulse + adapters
    // supervisor. The bus runs alongside the companion's internal
    // tasks and exits on the same shutdown token.
    let pulse = companion.pulse();
    let adapter_supervisor = companion.adapters();
    let shutdown = companion.shutdown_token();

    let mut bus_builder = DeviceBus::builder()
        .with_identity_protocol(Arc::new(FireflyIdentityProtocol::new()));
    for reg in bus_registrations(state_dir.clone()) {
        bus_builder = bus_builder.with_registration(reg);
    }
    if let Some(dir) = &state_dir {
        bus_builder = bus_builder.with_cache_path(dir.join("device-bus-cache.json"));
    }
    let bus = bus_builder.build(adapter_supervisor, pulse);

    let bus_shutdown = shutdown.clone();
    let bus_handle = tokio::spawn(async move { bus.run(bus_shutdown).await });

    let result = companion.run().await;
    // Ensure the bus exits with the companion — it already listens on
    // the same shutdown token, so this is just a clean await.
    let _ = bus_handle.await;
    result
}

// ---------------------------------------------------------------------------
// Subcommands (unchanged from legacy firefly)
// ---------------------------------------------------------------------------

fn list_ports() -> Result<()> {
    let ports = serialport::available_ports()?;
    if ports.is_empty() {
        println!("No serial ports found");
        return Ok(());
    }
    println!("Available serial ports:\n");
    for port in &ports {
        let port_type = match &port.port_type {
            serialport::SerialPortType::UsbPort(info) => {
                let device_type = FireflyDeviceType::from_vid(info.vid);
                let tag = match device_type {
                    FireflyDeviceType::Rp2040Matrix => " [RP2040-Matrix]",
                    FireflyDeviceType::Esp8266Oled => " [ESP8266-OLED]",
                    FireflyDeviceType::Esp8266OledV2 => " [ESP8266-OLED-v2]",
                    FireflyDeviceType::Esp32TDisplay => " [ESP32-TDisplay]",
                    FireflyDeviceType::Unknown => "",
                };
                format!(
                    "USB {:04x}:{:04x} {}{}",
                    info.vid,
                    info.pid,
                    info.product.as_deref().unwrap_or("Unknown"),
                    tag
                )
            }
            other => format!("{:?}", other),
        };
        println!("  {} - {}", port.port_name, port_type);
    }
    Ok(())
}

async fn test_mode(port_override: Option<String>) -> Result<()> {
    let detected = match port_override {
        Some(p) => {
            let device_type = detect_device_type(&p).unwrap_or(FireflyDeviceType::Unknown);
            DetectedDevice {
                port_name: p,
                device_type,
                vid: 0,
                pid: 0,
            }
        }
        None => find_firefly_device()?,
    };

    println!("Firefly Test Mode\nPort: {}\nType: {}\n", detected.port_name, detected.device_type);

    let serial = FireflySerial::new(&detected.port_name, detected.device_type)?;

    let tests: Vec<(&str, &str)> = match detected.device_type {
        FireflyDeviceType::Rp2040Matrix | FireflyDeviceType::Unknown => vec![
            ("I", "Get device info"),
            ("C", "Clear display"),
            ("F,255,0,0", "Fill red"),
            ("F,0,255,0", "Fill green"),
            ("F,0,0,255", "Fill blue"),
            ("A,rainbow", "Rainbow animation"),
            ("T,healthy", "Status: healthy"),
            ("C", "Clear"),
        ],
        _ => vec![("I", "Get device info"), ("C", "Clear display")],
    };

    for (cmd, desc) in tests {
        println!("{}: {}", desc, cmd);
        match serial.send_command(cmd) {
            Ok(response) => println!("  -> {}", response),
            Err(e) => println!("  -> ERROR: {}", e),
        }
        tokio::time::sleep(Duration::from_millis(1500)).await;
    }
    println!("\nTest complete!");
    Ok(())
}

fn probe_device(port_override: Option<String>) -> Result<()> {
    let detected = match port_override {
        Some(p) => {
            let device_type = detect_device_type(&p).unwrap_or(FireflyDeviceType::Unknown);
            DetectedDevice {
                port_name: p,
                device_type,
                vid: 0,
                pid: 0,
            }
        }
        None => find_firefly_device()?,
    };

    println!("Probing Firefly device on {}\nDetected type: {}\n", detected.port_name, detected.device_type);

    let serial = FireflySerial::new(&detected.port_name, detected.device_type)?;
    match serial.send_command("I") {
        Ok(response) => println!("Device Info: {}", response),
        Err(e) => {
            println!("Failed to communicate: {}", e);
            return Err(e);
        }
    }
    Ok(())
}
