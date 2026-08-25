//! `rake` — the gardener's CLI. Humans and agents walk the garden with it.
//!
//! Rake is a thin client (L21): it finds and validates a moss to attach
//! to, routes commands onto that moss's methods, and expects standard
//! return formats. It never computes garden truth.
//!
//! Attachment cascade (harvested from PoC rake, resolution.rs):
//!   1. `--stone` flag — explicit intent, never re-resolved (hard)
//!   2. `RAKE_STONE` env twin — operator intent, never re-resolved (hard)
//!   3. tending file — last successful attachment, optimistic, flushed on
//!      matching connection failure (soft)
//!   4. ask/tell discovery — whoever answers first (soft)

mod discover;
mod moss_http;
mod tending;

use clap::{Parser, Subcommand};
use garden_contract::chirp::ChirpBody;
use garden_contract::consts;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

/// How long rake listens for discovery answers after the call (ms).
const DEFAULT_TIMEOUT_MS: u64 = 2500;
/// How long rake waits for a moss's HTTP answer.
const HTTP_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Parser)]
#[command(
    name = "rake",
    about = "Walk a Zen Garden: see its stones, find what grows there.",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Attach to a specific moss: `ip:port`, or a stone name to pick from
    /// the room. Explicit intent — rake will not quietly go elsewhere.
    #[arg(long, global = true, env = "RAKE_STONE")]
    stone: Option<String>,

    /// Emit JSON instead of a human table.
    #[arg(long, global = true)]
    json: bool,

    /// Discovery UDP port (default is the v1 room).
    #[arg(long, env = "RAKE_DISCOVERY_PORT")]
    discovery_port: Option<u16>,

    /// Multicast group (default is the v1 room).
    #[arg(long, env = "RAKE_MCAST_GROUP")]
    mcast_group: Option<Ipv4Addr>,

    /// How long to wait for discovery answers (ms).
    #[arg(long, default_value_t = DEFAULT_TIMEOUT_MS)]
    timeout_ms: u64,
}

#[derive(Subcommand)]
enum Command {
    /// The garden as an attached moss sees it.
    Observe,
    /// Stones whose name contains the pattern, as the attached moss sees them.
    Find { pattern: String },
}

/// Where a candidate endpoint came from — provenance drives recovery
/// (PoC Origin): hard origins are user intent and never fall through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Origin {
    Flag,
    Env,
    Tending,
    Discovered,
}

impl Origin {
    fn is_soft(self) -> bool {
        matches!(self, Self::Tending | Self::Discovered)
    }
}

/// One place a moss might answer.
#[derive(Debug, Clone)]
struct Candidate {
    ip: IpAddr,
    http_port: u16,
    name_hint: Option<String>,
    origin: Origin,
}

impl Candidate {
    fn endpoint(&self) -> String {
        format!("{}:{}", self.ip, self.http_port)
    }
}

/// What the attached moss reports about its garden (standard formats, L21).
#[derive(Debug, Clone, Deserialize, Serialize)]
struct GardenStone {
    #[serde(flatten)]
    body: ChirpBody,
    chirps: u64,
}

impl Cli {
    fn room(&self) -> (u16, Ipv4Addr) {
        (
            self.discovery_port.unwrap_or(consts::DISCOVERY_PORT_V1),
            self.mcast_group.unwrap_or(consts::MULTICAST_GROUP_V1),
        )
    }

    /// Phase 1 of the cascade: explicit intent, then tending. Optimistic —
    /// no discovery cost; 99% of invocations end here (PoC parity).
    fn immediate_candidates(&self) -> Result<Vec<Candidate>, String> {
        let mut out: Vec<Candidate> = Vec::new();
        let push = |c: Candidate, out: &mut Vec<Candidate>| {
            if !out.iter().any(|seen| seen.endpoint() == c.endpoint()) {
                out.push(c);
            }
        };

        // 1+2: explicit intent (flag wins over env for provenance label).
        if let Some(value) = self.stone.clone() {
            let origin = if std::env::var("RAKE_STONE").is_ok_and(|v| v == value) {
                Origin::Env
            } else {
                Origin::Flag
            };
            let mut c = self.resolve_explicit_now(&value)?;
            c.origin = origin;
            push(c, &mut out);
        }

        // 3: tending (soft).
        if let Some(path) = tending::default_path()
            && let Some(t) = tending::read_from(&path)
            && let Some((ip, port)) = parse_ip_port(&t.endpoint)
        {
            push(
                Candidate {
                    ip,
                    http_port: port,
                    name_hint: Some(t.stone_name),
                    origin: Origin::Tending,
                },
                &mut out,
            );
        }
        Ok(out)
    }

    /// Resolve an explicit `--stone`/env value WITHOUT the network when it
    /// is an `ip:port` literal; a name needs the room and is resolved by
    /// the caller via discovery.
    fn resolve_explicit_now(&self, value: &str) -> Result<Candidate, String> {
        if let Some((ip, port)) = parse_ip_port(value) {
            return Ok(Candidate { ip, http_port: port, name_hint: None, origin: Origin::Flag });
        }
        Err(format!("stone '{value}' is a name - resolving against the room"))
    }

    /// Ask/tell discovery, every answerer as a Discovered candidate.
    async fn discovered_candidates(&self) -> Vec<Candidate> {
        let (port, group) = self.room();
        let mut out = Vec::new();
        if let Ok(sightings) =
            discover::ask_the_room(port, Some(group), Duration::from_millis(self.timeout_ms)).await
        {
            for s in sightings {
                out.push(Candidate {
                    ip: s.ip,
                    http_port: s.http_port,
                    name_hint: Some(s.stone_name),
                    origin: Origin::Discovered,
                });
            }
        }
        out
    }

    /// Attach to the first moss that answers with a garden view (L21:
    /// rake renders; moss computes). Cascade: pinned/tended optimistically;
    /// on soft failure fall to discovery; hard failures abort loudly.
    async fn garden_view(&self) -> Result<Vec<GardenStone>, String> {
        // Name-shaped explicit intent resolves through discovery, so route
        // the whole walk through the discovered phase in that case.
        let explicit_is_name = self
            .stone
            .as_deref()
            .is_some_and(|v| parse_ip_port(v).is_none());

        if !explicit_is_name {
            let early = self.immediate_candidates()?;
            match self.try_candidates(&early).await? {
                Some(stones) => return Ok(stones),
                None => eprintln!("rake: tended stone unreachable - discovering..."),
            }
        }

        // Discovery phase (also resolves name intents).
        let mut candidates = self.discovered_candidates().await;
        if explicit_is_name {
            let value = self.stone.clone().unwrap_or_default();
            let needle = value.to_lowercase();
            let matches: Vec<Candidate> = candidates
                .iter()
                .filter(|c| {
                    c.name_hint
                        .as_deref()
                        .is_some_and(|n| n.to_lowercase().contains(&needle))
                })
                .cloned()
                .collect();
            match matches.len() {
                1 => candidates = vec![Candidate { origin: Origin::Flag, ..matches[0].clone() }],
                0 => {
                    return Err(format!("no stone answering matches '{value}'"));
                }
                _ => {
                    return Err(format!(
                        "'{value}' is ambiguous: {} stones match; use ip:port",
                        matches.len()
                    ));
                }
            }
        }

        if candidates.is_empty() {
            return Err("no moss found: nothing pinned, nothing tended, nobody answered".into());
        }
        match self.try_candidates(&candidates).await? {
            Some(stones) => Ok(stones),
            None => Err("nobody answered with a garden view".into()),
        }
    }

    /// Walk candidates in order; the first standard-format answer wins.
    /// Ok(None) = every candidate failed softly; Err = hard intent failed
    /// (explicit pins are never silently redirected).
    async fn try_candidates(
        &self,
        candidates: &[Candidate],
    ) -> Result<Option<Vec<GardenStone>>, String> {
        for cand in candidates {
            match moss_http::get_json(cand.ip, cand.http_port, "/api/v1/garden/observe", HTTP_TIMEOUT)
                .await
            {
                Ok(v) => {
                    let stones = parse_garden(&v)?;
                    remember_attachment(cand, &stones);
                    return Ok(Some(stones));
                }
                Err(e) => {
                    eprintln!("note: {} ({}) — {}", cand.endpoint(), cand.origin_label(), e);
                    if e.is_connection_failed() {
                        // Soft memory of a dead stone: flush and move on
                        // (PoC parity — stale tending never survives contact).
                        if cand.origin == Origin::Tending
                            && let Some(path) = tending::default_path()
                        {
                            let _ = tending::clear_at(&path);
                        }
                        if !cand.origin.is_soft() {
                            return Err(format!(
                                "{} was pinned explicitly but is unreachable; refusing to guess",
                                cand.name_hint.as_deref().unwrap_or(&cand.endpoint())
                            ));
                        }
                    }
                }
            }
        }
        Ok(None)
    }
}

fn parse_garden(v: &serde_json::Value) -> Result<Vec<GardenStone>, String> {
    let stones = v
        .get("data")
        .and_then(|d| d.get("stones"))
        .cloned()
        .ok_or_else(|| "response lacked data.stones".to_string())?;
    serde_json::from_value::<Vec<GardenStone>>(stones)
        .map_err(|e| format!("garden view did not match standard format: {e}"))
}

/// Delight of continuity: on success, tend toward this moss (unless the
/// attachment was explicitly pinned — those are already remembered).
fn remember_attachment(cand: &Candidate, stones: &[GardenStone]) {
    if !cand.origin.is_soft() {
        return;
    }
    let name = stones
        .iter()
        .find(|s| s.body.address.ip == cand.ip && s.body.address.port == cand.http_port)
        .map(|s| s.body.stone_name.clone())
        .or_else(|| cand.name_hint.clone())
        .unwrap_or_else(|| "unknown".into());
    if let Some(path) = tending::default_path() {
        let _ = tending::write_to(&path, &tending::Tending::now(name, cand.endpoint()));
    }
}

fn parse_ip_port(s: &str) -> Option<(IpAddr, u16)> {
    let (ip, port) = s.rsplit_once(':')?;
    let ip = ip.parse::<IpAddr>().ok()?;
    let port = port.parse().ok()?;
    Some((ip, port))
}

impl Candidate {
    fn origin_label(&self) -> &'static str {
        match self.origin {
            Origin::Flag => "pinned by --stone",
            Origin::Env => "pinned by RAKE_STONE",
            Origin::Tending => "tended",
            Origin::Discovered => "discovered",
        }
    }
}

fn print_row(stone: &str, health: &str, address: &str, version: &str) {
    println!("{:<26} {:<12} {:<21} {}", stone, health, address, version);
}

fn print_table(stones: &[GardenStone]) {
    if stones.is_empty() {
        println!("The garden is quiet - the moss sees nobody.");
        return;
    }
    print_row("STONE", "HEALTH", "ADDRESS", "VERSION");
    for s in stones {
        print_row(
            &s.body.stone_name,
            &s.body.health,
            &format!("{}:{}", s.body.address.ip, s.body.address.port),
            &s.body.moss_version,
        );
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result: Result<Vec<GardenStone>, String> = async {
        let mut stones = cli.garden_view().await?;
        if let Command::Find { pattern } = &cli.command {
            let needle = pattern.to_lowercase();
            stones.retain(|s| s.body.stone_name.to_lowercase().contains(&needle));
        }
        Ok(stones)
    }
    .await;

    match result {
        Ok(stones) => {
            if cli.json {
                match serde_json::to_string_pretty(&stones) {
                    Ok(json) => println!("{json}"),
                    Err(e) => {
                        eprintln!("could not render json: {e}");
                        std::process::exit(1);
                    }
                }
            } else {
                print_table(&stones);
            }
        }
        Err(msg) => {
            eprintln!("rake: {msg}");
            std::process::exit(1);
        }
    }
}
