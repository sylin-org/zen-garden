// Garden Cricket - Ambient Audio Companion for Stone Presence
// Provides audio feedback for garden events and stone presence

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::sync::Arc;

use garden_companion_sdk::{
    check_dump_commands, CommandArg, CommandDef, CommandManifest, CompanionRuntime, CompanionState,
};

mod events;
mod handler;
mod manifest;
mod mixer;
mod test_mode;

use events::CricketEvents;
use handler::CricketCommands;
use manifest::Tunes;
use mixer::Mixer;

/// Build Cricket's command manifest
fn build_manifest() -> CommandManifest {
    CommandManifest::new(
        "cricket",
        "Garden Cricket",
        env!("CARGO_PKG_VERSION"),
        "Ambient audio Companion for Zen Garden stone presence events",
    )
    .command(
        CommandDef::new("select", "Switch active tune")
            .arg(CommandArg::required_string("tune", "Tune name to activate"))
            .example("Switch to mr-robot tune", "hey tell cricket select mr-robot")
            .example("Switch to zen-tech tune", "hey tell cricket select zen-tech")
            .see_also("list")
            .see_also("volume"),
    )
    .command(
        CommandDef::new("volume", "Set master volume")
            .arg(CommandArg::required_int("level", "Volume level (0-100)", 0, 100))
            .example("Set volume to 50%", "hey tell cricket volume 50")
            .example("Mute", "hey tell cricket volume 0")
            .see_also("select"),
    )
    .command(
        CommandDef::new("list", "List available tunes")
            .long_desc("Shows all tunes available from embedded assets and filesystem. Filesystem tunes override embedded tunes with the same name.")
            .example("List all tunes", "hey tell cricket list")
            .see_also("select")
            .see_also("show"),
    )
    .command(
        CommandDef::new("show", "Show tune details")
            .arg(CommandArg::required_string("tune", "Tune name to inspect"))
            .long_desc("Displays tune metadata, mapped events, and resource files.")
            .example("Show zen-tech tune details", "hey tell cricket show zen-tech")
            .see_also("list")
            .see_also("select"),
    )
    .command(
        CommandDef::new("play", "Play an event sound")
            .arg(CommandArg::required_string("event", "Event name to play"))
            .long_desc("Triggers the sound for a specific event from the current tune. Useful for testing.")
            .example("Play stone online sound", "hey tell cricket play stone-online")
            .example("Play service started", "hey tell cricket play service-started")
            .see_also("select"),
    )
    .command(
        CommandDef::new("stop", "Stop all playing sounds")
            .example("Stop all sounds", "hey tell cricket stop")
            .see_also("volume"),
    )
}

#[derive(Parser, Debug)]
#[command(name = "garden-cricket")]
#[command(about = "Ambient audio Companion for Zen Garden stone presence")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Stone endpoint (e.g., http://10.0.0.5:7185)
    #[arg(short, long, env = "GARDEN_STONE")]
    stone: Option<String>,

    /// Tunes directory (contains tune.yaml files)
    #[arg(long, env = "CRICKET_TUNES_DIR")]
    tunes_dir: Option<String>,

    /// Active tune name
    #[arg(long, env = "CRICKET_TUNE", default_value = "zen-tech")]
    tune: String,

    /// Master volume (0-100)
    #[arg(long, env = "CRICKET_VOLUME", default_value = "50")]
    volume: u8,

    /// Command server port (for receiving hey-tell commands)
    /// Default is computed from Companion ID (7188-7199 range)
    #[arg(long, env = "CRICKET_PORT")]
    port: Option<u16>,

    /// State directory for persisting settings
    #[arg(long, env = "CRICKET_STATE_DIR")]
    state_dir: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// List available tunes
    Load {
        /// Tunes directory
        #[arg(long)]
        tunes_dir: Option<String>,
    },

    /// Test mode: interactively trigger tune events with keyboard
    Test {
        /// Tune name to test
        tune: String,

        /// Tunes directory
        #[arg(long)]
        tunes_dir: Option<String>,
    },

    /// Show tune details
    Show {
        /// Tune name
        tune: String,

        /// Tunes directory
        #[arg(long)]
        tunes_dir: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Check for --dump-commands before any other processing
    // This is used by Moss Companion registry to discover Cricket's commands
    check_dump_commands(&build_manifest());

    // Initialize tracing (from SDK)
    garden_companion_sdk::runtime::init_tracing();

    let cli = Cli::parse();

    // Resolve tunes directory
    let tunes_dir = resolve_tunes_dir(cli.tunes_dir.as_deref());

    // Handle subcommands
    if let Some(cmd) = cli.command {
        match cmd {
            Commands::Load {
                tunes_dir: override_dir,
            } => {
                let dir = resolve_tunes_dir(override_dir.as_deref());
                return list_tunes(&dir);
            }
            Commands::Test {
                tune,
                tunes_dir: override_dir,
            } => {
                let dir = resolve_tunes_dir(override_dir.as_deref());
                return test_mode::run(&tune, &dir).await;
            }
            Commands::Show {
                tune,
                tunes_dir: override_dir,
            } => {
                let dir = resolve_tunes_dir(override_dir.as_deref());
                return show_tune(&tune, &dir);
            }
        }
    }

    // Normal mode: require stone endpoint and port
    let stone = cli
        .stone
        .ok_or_else(|| anyhow::anyhow!("--stone endpoint required (or use 'test' subcommand)"))?;

    // Port is assigned by Moss and passed via --port
    let port = cli.port.ok_or_else(|| {
        anyhow::anyhow!("--port required (assigned by Moss when starting Companion)")
    })?;

    // Create Companion state (handles on/off persistence)
    let state_dir = cli.state_dir.map(std::path::PathBuf::from);
    let companion_state = Arc::new(CompanionState::new(state_dir));

    // Ensure audio dependencies are installed (alsa-utils on Linux)
    mixer::ensure_audio_dependencies()?;

    // Initialize system audio (unmute, set volume) on Linux
    mixer::init_system_audio(50)?;

    // Initialize domain components
    let mixer = Arc::new(Mixer::new(cli.volume as f32 / 100.0)?);
    let tunes = Arc::new(Tunes::new(Some(&tunes_dir))?);

    // Select initial tune
    tunes.select(&cli.tune)?;

    tracing::info!(
        stone = %stone,
        tune = %cli.tune,
        volume = cli.volume,
        port = port,
        "Starting Garden Cricket"
    );

    // Create handlers
    let commands = CricketCommands::new(
        Arc::clone(&mixer),
        Arc::clone(&tunes),
        Arc::clone(&companion_state),
    );
    let events = CricketEvents::new(
        Arc::clone(&mixer),
        Arc::clone(&tunes),
        Arc::clone(&companion_state),
    );

    // Build and run Companion using SDK runtime
    let config = garden_companion_sdk::CompanionConfig {
        stone: Some(stone),
        port: Some(port),
        dump_commands: false,
    };

    CompanionRuntime::new(config, "cricket")
        .command_handler(commands)
        .event_handler(events)
        .run()
        .await
}

/// Resolve tunes directory with fallbacks
fn resolve_tunes_dir(override_path: Option<&str>) -> String {
    if let Some(path) = override_path {
        return path.to_string();
    }

    // Check standard locations
    let candidates = [
        "./tunes",
        "/usr/share/garden-cricket/tunes",
        "/etc/zen-garden/cricket/tunes",
    ];

    for candidate in candidates {
        if std::path::Path::new(candidate).exists() {
            return candidate.to_string();
        }
    }

    // Default
    "./tunes".to_string()
}

/// List available tunes
fn list_tunes(tunes_dir: &str) -> Result<()> {
    let tunes = Tunes::new(Some(tunes_dir))?.list_tunes();

    if tunes.is_empty() {
        println!("No tunes found in {}", tunes_dir);
        println!();
        println!("Create a tune by adding a tune.yaml file:");
        println!("  {}/my-tune/tune.yaml", tunes_dir);
        return Ok(());
    }

    println!("Available Tunes:");
    println!();

    for tune in tunes {
        let source = if tune.embedded {
            "[embedded]"
        } else {
            "[filesystem]"
        };
        println!("  {} (v{}) {}", tune.name, tune.version, source);
        println!("    {}", tune.description);
        println!("    Events: {}", tune.event_count);
        println!();
    }

    Ok(())
}

/// Show tune details
fn show_tune(name: &str, tunes_dir: &str) -> Result<()> {
    let tune = Tunes::new(Some(tunes_dir))?
        .get_tune(name)
        .ok_or_else(|| anyhow::anyhow!("Tune '{}' not found", name))?;

    println!("{}", tune.name);
    println!("  Version:     {}", tune.version);
    println!("  Description: {}", tune.description);
    println!("  Author:      {}", tune.author);
    println!("  License:     {}", tune.license);
    println!();
    println!("Event Mappings:");

    for (event, mapping) in &tune.events {
        println!("  {} → {} ({})", event, mapping.resource, mapping.channel);
    }

    Ok(())
}
