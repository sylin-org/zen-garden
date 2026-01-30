// Garden Firefly - Visual Status Indicator Companion
// Controls Waveshare RP2040-Matrix 5x5 RGB LED for system status display

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::sync::Arc;
use std::time::Duration;

use garden_companion_sdk::{
    check_dump_commands, CompanionRuntime, CompanionState, CommandArg, CommandDef, CommandManifest,
    SseClient,
    sse::SseClientConfig,
};

mod animation;
mod events;
mod handler;
mod serial;

use animation::{start_animation, AnimationContext};
use events::FireflyEventHandler;
use handler::FireflyHandler;
use serial::{find_firefly_port, FireflyConnection, FireflySerial};
use tokio::sync::RwLock;

/// Build Firefly's command manifest
fn build_manifest() -> CommandManifest {
    CommandManifest::new(
        "firefly",
        "Garden Firefly",
        env!("CARGO_PKG_VERSION"),
        "Visual status indicator using Waveshare RP2040-Matrix 5x5 RGB LED",
    )
    .command(
        CommandDef::new("status", "Show status indicator")
            .arg(CommandArg::required_string(
                "state",
                "Status state: healthy, warning, error, or offline",
            ))
            .example("Show healthy status", "hey tell firefly status healthy")
            .example("Show warning status", "hey tell firefly status warning")
            .example("Show error status", "hey tell firefly status error")
            .see_also("animate")
            .see_also("clear"),
    )
    .command(
        CommandDef::new("pixel", "Set single pixel color")
            .arg(CommandArg::required_int("x", "X coordinate (0-4)", 0, 4))
            .arg(CommandArg::required_int("y", "Y coordinate (0-4)", 0, 4))
            .arg(CommandArg::required_string("color", "Color as hex (ff0000) or r,g,b"))
            .example("Red center pixel", "hey tell firefly pixel 2 2 ff0000")
            .example("Blue corner", "hey tell firefly pixel 0 0 0000ff")
            .see_also("fill")
            .see_also("clear"),
    )
    .command(
        CommandDef::new("fill", "Fill all pixels with color")
            .arg(CommandArg::required_string("color", "Color as hex (ff0000) or r,g,b"))
            .example("Fill with green", "hey tell firefly fill 00ff00")
            .example("Fill with white", "hey tell firefly fill ffffff")
            .see_also("pixel")
            .see_also("clear"),
    )
    .command(
        CommandDef::new("clear", "Turn off all LEDs")
            .example("Clear display", "hey tell firefly clear")
            .see_also("fill")
            .see_also("stop"),
    )
    .command(
        CommandDef::new("brightness", "Set LED brightness")
            .arg(CommandArg::required_int("percent", "Brightness 0-100%", 0, 100))
            .example("Set to 50%", "hey tell firefly brightness 50")
            .example("Full brightness", "hey tell firefly brightness 100")
            .see_also("fill"),
    )
    .command(
        CommandDef::new("animate", "Start animation")
            .arg(CommandArg::required_string(
                "name",
                "Animation: rainbow, pulse, chase, or sparkle",
            ))
            .example("Rainbow cycle", "hey tell firefly animate rainbow")
            .example("Breathing pulse", "hey tell firefly animate pulse")
            .see_also("stop")
            .see_also("status"),
    )
    .command(
        CommandDef::new("stop", "Stop current animation")
            .example("Stop animation", "hey tell firefly stop")
            .see_also("animate")
            .see_also("clear"),
    )
    .command(
        CommandDef::new("info", "Show device information")
            .example("Get device info", "hey tell firefly info")
            .see_also("status"),
    )
}

#[derive(Parser, Debug)]
#[command(name = "garden-firefly")]
#[command(about = "Visual status indicator Companion for Zen Garden")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Stone endpoint (e.g., http://10.0.0.5:7185)
    #[arg(short, long, env = "GARDEN_STONE")]
    stone: Option<String>,

    /// Serial port for RP2040-Matrix (auto-detects if not specified)
    #[arg(long, env = "FIREFLY_PORT")]
    serial_port: Option<String>,

    /// Command server port (assigned by Moss)
    #[arg(long, env = "FIREFLY_HTTP_PORT")]
    port: Option<u16>,

    /// State directory for persisting settings
    #[arg(long, env = "FIREFLY_STATE_DIR")]
    state_dir: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// List available serial ports
    Ports,

    /// Test mode: send commands directly to device
    Test {
        /// Serial port to use
        #[arg(long)]
        port: Option<String>,
    },

    /// Probe device and show info
    Probe {
        /// Serial port to use
        #[arg(long)]
        port: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Check for --dump-commands before any other processing
    check_dump_commands(&build_manifest());

    // Initialize tracing
    garden_companion_sdk::runtime::init_tracing();

    let cli = Cli::parse();

    // Handle subcommands
    if let Some(cmd) = cli.command {
        match cmd {
            Commands::Ports => {
                return list_ports();
            }
            Commands::Test { port } => {
                return test_mode(port).await;
            }
            Commands::Probe { port } => {
                return probe_device(port);
            }
        }
    }

    // Normal Companion mode: require stone and port
    let stone = cli.stone.ok_or_else(|| {
        anyhow::anyhow!("--stone endpoint required (or use 'test'/'probe' subcommand)")
    })?;

    let port = cli
        .port
        .ok_or_else(|| anyhow::anyhow!("--port required (assigned by Moss)"))?;

    // Create Companion state (handles on/off persistence)
    let state_dir = cli.state_dir.map(std::path::PathBuf::from);
    let companion_state = Arc::new(CompanionState::new(state_dir.clone()));

    // Create animation context (handles brightness persistence)
    let animation_context = Arc::new(RwLock::new(AnimationContext::new(state_dir)));

    // Create connection manager (doesn't require device to be present)
    let connection = Arc::new(FireflyConnection::new(cli.serial_port));

    // Try initial connection (non-fatal if it fails)
    match connection.try_connect() {
        Ok(()) => {
            tracing::info!("Firefly device connected on startup");
            // Clear display on startup for clean slate
            let _ = connection.with_device(|serial| serial.clear());
        }
        Err(e) => {
            tracing::info!(error = %e, "No Firefly device found on startup, will retry every 10s");
        }
    }

    tracing::info!(
        stone = %stone,
        port = port,
        connected = connection.is_connected(),
        "Starting Garden Firefly"
    );

    // Spawn background task to retry connection every 10 seconds
    let conn_for_retry = Arc::clone(&connection);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(10));
        loop {
            interval.tick().await;

            if !conn_for_retry.is_connected() {
                match conn_for_retry.try_connect() {
                    Ok(()) => {
                        tracing::info!("Firefly device connected");
                        // Clear display on reconnect for clean slate
                        let _ = conn_for_retry.with_device(|serial| serial.clear());
                    }
                    Err(_) => {
                        // Silent retry - don't spam logs
                    }
                }
            }
        }
    });

    // Start the animation engine (runs the baseline firefly animation)
    let _animation_handle = start_animation(
        Arc::clone(&connection),
        Arc::clone(&animation_context),
    );
    tracing::info!("Animation engine started");

    // Start SSE client to receive presence events from Moss
    let sse_config = SseClientConfig::new(&stone)
        .with_path("/api/v1/stone/presence/stream");
    let event_handler = Arc::new(FireflyEventHandler::new(
        Arc::clone(&animation_context),
        Arc::clone(&companion_state),
    ));
    let _sse_handle = SseClient::start(sse_config, event_handler);
    tracing::info!(endpoint = %stone, "SSE client started for presence events");

    // Create command handler
    let handler = FireflyHandler::new(
        Arc::clone(&connection),
        Arc::clone(&companion_state),
        Arc::clone(&animation_context),
    );

    // Build and run Companion
    let config = garden_companion_sdk::CompanionConfig {
        stone: Some(stone),
        port: Some(port),
        dump_commands: false,
    };

    // Clone connection for shutdown handler
    let conn_for_shutdown = Arc::clone(&connection);

    // Run Companion with graceful shutdown
    tokio::select! {
        result = CompanionRuntime::new(config, "firefly")
            .command_handler(handler)
            .run() => {
            // Companion stopped normally - clear display
            tracing::info!("Companion stopped, clearing display");
            let _ = conn_for_shutdown.with_device(|serial| serial.clear());
            result
        }
        _ = shutdown_signal() => {
            // Received shutdown signal - clear display
            tracing::info!("Shutdown signal received, clearing display");
            let _ = conn_for_shutdown.with_device(|serial| serial.clear());
            Ok(())
        }
    }
}

/// Wait for shutdown signal (SIGTERM on Unix, Ctrl+C everywhere)
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
        let mut sigint = signal(SignalKind::interrupt()).expect("failed to install SIGINT handler");
        tokio::select! {
            _ = sigterm.recv() => {}
            _ = sigint.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    }
}

/// List available serial ports
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
                let vid_pid = format!("{:04x}:{:04x}", info.vid, info.pid);
                let product = info
                    .product
                    .as_ref()
                    .map(|s| s.as_str())
                    .unwrap_or("Unknown");
                let manufacturer = info.manufacturer.as_ref().map(|s| s.as_str()).unwrap_or("");

                let is_rp2040 = info.vid == 0x2e8a; // Raspberry Pi VID

                format!(
                    "USB {} {} {}{}",
                    vid_pid,
                    product,
                    manufacturer,
                    if is_rp2040 { " [RP2040]" } else { "" }
                )
            }
            serialport::SerialPortType::PciPort => "PCI".to_string(),
            serialport::SerialPortType::BluetoothPort => "Bluetooth".to_string(),
            serialport::SerialPortType::Unknown => "Unknown".to_string(),
        };

        println!("  {} - {}", port.port_name, port_type);
    }

    Ok(())
}

/// Test mode: interactive serial communication
async fn test_mode(port_override: Option<String>) -> Result<()> {
    let port = match port_override {
        Some(p) => p,
        None => find_firefly_port()?,
    };

    println!("Firefly Test Mode");
    println!("Port: {}", port);
    println!();

    let serial = FireflySerial::new(&port)?;

    // Test sequence
    let tests = [
        ("I", "Get device info"),
        ("C", "Clear display"),
        ("F,255,0,0", "Fill red"),
        ("F,0,255,0", "Fill green"),
        ("F,0,0,255", "Fill blue"),
        ("A,rainbow", "Rainbow animation"),
        ("T,healthy", "Status: healthy"),
        ("C", "Clear"),
    ];

    for (cmd, desc) in tests {
        println!("{}: {}", desc, cmd);

        match serial.send_command(cmd) {
            Ok(response) => println!("  -> {}", response),
            Err(e) => println!("  -> ERROR: {}", e),
        }

        tokio::time::sleep(Duration::from_millis(1000)).await;
    }

    println!();
    println!("Test complete!");

    Ok(())
}

/// Probe device and show info
fn probe_device(port_override: Option<String>) -> Result<()> {
    let port = match port_override {
        Some(p) => p,
        None => find_firefly_port()?,
    };

    println!("Probing Firefly device on {}", port);

    let serial = FireflySerial::new(&port)?;

    // Get info
    match serial.send_command("I") {
        Ok(response) => {
            println!("Device Info: {}", response);

            // Parse response: OK,firefly-v0,rp2040-matrix,5x5
            let parts: Vec<&str> = response.split(',').collect();
            if parts.len() >= 4 {
                println!();
                println!("  Firmware: {}", parts.get(1).unwrap_or(&"unknown"));
                println!("  Hardware: {}", parts.get(2).unwrap_or(&"unknown"));
                println!("  Matrix:   {}", parts.get(3).unwrap_or(&"unknown"));
            }
        }
        Err(e) => {
            println!("Failed to communicate: {}", e);
            return Err(e);
        }
    }

    // Get help
    match serial.send_command("?") {
        Ok(response) => {
            println!("  Commands: {}", response.trim_start_matches("OK,"));
        }
        Err(_) => {}
    }

    Ok(())
}
