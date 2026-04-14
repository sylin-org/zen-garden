// Garden Cricket — ambient audio companion.
// Rewritten onto the companion-sdk event mesh per COMPANION-0009 Ch5.

use anyhow::Result;
use clap::{Parser, Subcommand};
use garden_companion_sdk::garden::{CommandTransport, SseTransport};
use garden_companion_sdk::prelude::*;
use garden_companion_sdk::{
    check_dump_commands, CommandArg, CommandDef, CommandManifest,
};
use std::sync::Arc;
use tokio::sync::Mutex;

mod adapters;
mod manifest;
mod mixer;
mod test_mode;

use adapters::AudioFactory;
use manifest::Tunes;
use mixer::Mixer;

/// Build Cricket's command manifest (consumed by moss's companion registry
/// via the `--dump-commands` flag).
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
            .example("Switch to zen-tech tune", "hey tell cricket select zen-tech")
            .see_also("list")
            .see_also("volume"),
    )
    .command(
        CommandDef::new("volume", "Set master volume")
            .arg(CommandArg::required_int("level", "Volume level (0-100)", 0, 100))
            .example("Set volume to 50%", "hey tell cricket volume 50")
            .example("Mute", "hey tell cricket volume 0"),
    )
    .command(
        CommandDef::new("list", "List available tunes")
            .example("List all tunes", "hey tell cricket list")
            .see_also("select")
            .see_also("show"),
    )
    .command(
        CommandDef::new("show", "Show tune details")
            .arg(CommandArg::required_string("tune", "Tune name to inspect"))
            .example("Show zen-tech tune", "hey tell cricket show zen-tech"),
    )
    .command(
        CommandDef::new("play", "Play an event sound")
            .arg(CommandArg::required_string("event", "Event name to play"))
            .example("Play stone online sound", "hey tell cricket play stone.tended"),
    )
    .command(CommandDef::new("stop", "Stop all playing sounds"))
}

#[derive(Parser, Debug)]
#[command(name = "garden-cricket")]
#[command(about = "Ambient audio companion for Zen Garden")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[arg(short, long, env = "GARDEN_STONE")]
    stone: Option<String>,

    #[arg(long, env = "CRICKET_TUNES_DIR")]
    tunes_dir: Option<String>,

    #[arg(long, env = "CRICKET_TUNE", default_value = "zen-tech")]
    tune: String,

    #[arg(long, env = "CRICKET_VOLUME", default_value = "50")]
    volume: u8,

    #[arg(long, env = "CRICKET_PORT")]
    port: Option<u16>,

    #[arg(long, env = "CRICKET_STATE_DIR")]
    state_dir: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// List available tunes
    Load {
        #[arg(long)]
        tunes_dir: Option<String>,
    },
    /// Interactive test mode with keyboard-driven events
    Test {
        tune: String,
        #[arg(long)]
        tunes_dir: Option<String>,
    },
    /// Show tune details
    Show {
        tune: String,
        #[arg(long)]
        tunes_dir: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    check_dump_commands(&build_manifest());
    garden_companion_sdk::init_tracing();

    let cli = Cli::parse();
    let tunes_dir = resolve_tunes_dir(cli.tunes_dir.as_deref());

    if let Some(cmd) = cli.command {
        return match cmd {
            Commands::Load { tunes_dir: d } => list_tunes(&resolve_tunes_dir(d.as_deref())),
            Commands::Test { tune, tunes_dir: d } => {
                test_mode::run(&tune, &resolve_tunes_dir(d.as_deref())).await
            }
            Commands::Show { tune, tunes_dir: d } => {
                show_tune(&tune, &resolve_tunes_dir(d.as_deref()))
            }
        };
    }

    let stone = cli
        .stone
        .ok_or_else(|| anyhow::anyhow!("--stone endpoint required (or use a subcommand)"))?;
    let port = cli
        .port
        .ok_or_else(|| anyhow::anyhow!("--port required (assigned by Moss)"))?;

    mixer::ensure_audio_dependencies()?;
    mixer::init_system_audio(cli.volume)?;

    let mixer = Arc::new(Mixer::new(cli.volume as f32 / 100.0)?);
    let tunes = Arc::new(Tunes::new(Some(&tunes_dir))?);
    tunes.select(&cli.tune)?;

    let enabled = Arc::new(Mutex::new(load_enabled(cli.state_dir.as_deref())));

    tracing::info!(
        stone = %stone,
        tune = %cli.tune,
        volume = cli.volume,
        port = port,
        "Starting Garden Cricket"
    );

    let factory = AudioFactory::new(mixer.clone(), tunes.clone(), enabled.clone());

    let mut companion = Companion::new("cricket")
        .with_transport(SseTransport::new(stone))
        .with_transport(CommandTransport::new(port))
        .with_adapter_factory(factory);
    if let Some(dir) = cli.state_dir.as_deref() {
        companion = companion.with_state_dir(dir);
    }
    companion.run().await
}

// ---------------------------------------------------------------------------
// Subcommand impls (unchanged from legacy cricket)
// ---------------------------------------------------------------------------

fn resolve_tunes_dir(override_path: Option<&str>) -> String {
    if let Some(path) = override_path {
        return path.to_string();
    }
    for candidate in [
        "./tunes",
        "/usr/share/garden-cricket/tunes",
        "/etc/zen-garden/cricket/tunes",
    ] {
        if std::path::Path::new(candidate).exists() {
            return candidate.to_string();
        }
    }
    "./tunes".to_string()
}

fn list_tunes(tunes_dir: &str) -> Result<()> {
    let tunes = Tunes::new(Some(tunes_dir))?.list_tunes();
    if tunes.is_empty() {
        println!("No tunes found in {}", tunes_dir);
        return Ok(());
    }
    println!("Available Tunes:\n");
    for tune in tunes {
        let source = if tune.embedded { "[embedded]" } else { "[filesystem]" };
        println!("  {} (v{}) {}", tune.name, tune.version, source);
        println!("    {}", tune.description);
        println!("    Events: {}\n", tune.event_count);
    }
    Ok(())
}

fn show_tune(name: &str, tunes_dir: &str) -> Result<()> {
    let tune = Tunes::new(Some(tunes_dir))?
        .get_tune(name)
        .ok_or_else(|| anyhow::anyhow!("Tune '{}' not found", name))?;
    println!("{}", tune.name);
    println!("  Version:     {}", tune.version);
    println!("  Description: {}", tune.description);
    println!("  Author:      {}", tune.author);
    println!("  License:     {}\n", tune.license);
    println!("Event Mappings:");
    for (event, mapping) in &tune.events {
        println!("  {} → {} ({})", event, mapping.resource, mapping.channel);
    }
    Ok(())
}

/// Load the persisted on/off flag from the state directory, defaulting
/// to `true` when no state dir is configured. Mirrors the legacy
/// `CompanionState` behaviour so an operator who disabled cricket
/// keeps that preference across the rewrite.
fn load_enabled(state_dir: Option<&str>) -> bool {
    let Some(dir) = state_dir else { return true };
    let path = std::path::Path::new(dir).join("enabled");
    match std::fs::read_to_string(&path) {
        Ok(s) => s.trim() != "off",
        Err(_) => true,
    }
}
