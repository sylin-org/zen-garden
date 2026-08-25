//! `rake` — the gardener's CLI. Humans and agents walk the garden with it.
//!
//! Verbs come from the glossary (R1.1): observe, find, and more to come as
//! the garden grows. Rake speaks the room's ask/tell for discovery and
//! moss's HTTP surface for detail; it is a visitor, never a member.

mod discover;

use clap::{Parser, Subcommand};
use discover::Sighting;
use garden_contract::consts;
use std::net::Ipv4Addr;
use std::time::Duration;

/// How long rake listens for answers after the call (ms).
const DEFAULT_TIMEOUT_MS: u64 = 2500;

#[derive(Parser)]
#[command(
    name = "rake",
    about = "Walk a Zen Garden: see its stones, find what grows there.",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Emit JSON instead of a human table.
    #[arg(long, global = true)]
    json: bool,

    /// Discovery UDP port (default is the v1 room).
    #[arg(long, env = "RAKE_DISCOVERY_PORT")]
    discovery_port: Option<u16>,

    /// Multicast group (default is the v1 room).
    #[arg(long, env = "RAKE_MCAST_GROUP")]
    mcast_group: Option<Ipv4Addr>,

    /// How long to wait for answers.
    #[arg(long, default_value_t = DEFAULT_TIMEOUT_MS)]
    timeout_ms: u64,
}

#[derive(Subcommand)]
enum Command {
    /// Every stone that answers the call.
    Observe,
    /// Stones whose name contains the pattern.
    Find { pattern: String },
}

impl Cli {
    fn room(&self) -> (u16, Ipv4Addr) {
        (
            self.discovery_port.unwrap_or(consts::DISCOVERY_PORT_V1),
            self.mcast_group.unwrap_or(consts::MULTICAST_GROUP_V1),
        )
    }

    async fn sightings(&self) -> std::io::Result<Vec<Sighting>> {
        let (port, group) = self.room();
        discover::ask_the_room(port, Some(group), Duration::from_millis(self.timeout_ms)).await
    }
}

fn print_row(stone: &str, address: &str, version: &str, id: &str) {
    println!("{:<26} {:<21} {:<10} {}", stone, address, version, id);
}

fn print_table(sightings: &[Sighting]) {
    if sightings.is_empty() {
        println!("The garden is quiet - no stones answered.");
        return;
    }
    print_row("STONE", "ADDRESS", "VERSION", "ID");
    for s in sightings {
        let id_short = s.stone_id.as_deref().unwrap_or("-").get(..8).unwrap_or("-");
        print_row(
            &s.stone_name,
            &format!("{}:{}", s.ip, s.http_port),
            &s.moss_version,
            id_short,
        );
    }
}

fn print_json(sightings: &[Sighting]) -> serde_json::Result<String> {
    serde_json::to_string_pretty(sightings)
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let sightings = match &cli.command {
        Command::Observe => match cli.sightings().await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("rake could not listen on the room: {e}");
                std::process::exit(1);
            }
        },
        Command::Find { pattern } => {
            let pattern = pattern.to_lowercase();
            match cli.sightings().await {
                Ok(s) => s
                    .into_iter()
                    .filter(|s| s.stone_name.to_lowercase().contains(&pattern))
                    .collect(),
                Err(e) => {
                    eprintln!("rake could not listen on the room: {e}");
                    std::process::exit(1);
                }
            }
        }
    };

    if cli.json {
        match print_json(&sightings) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                eprintln!("could not render json: {e}");
                std::process::exit(1);
            }
        }
    } else {
        print_table(&sightings);
    }
}
