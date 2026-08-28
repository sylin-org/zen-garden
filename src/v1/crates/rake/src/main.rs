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
//!
//! Stone ops (`offer`, `explain`, `rest`, `wake`, `uproot`) ride the same
//! cascade through one front door ([`Cli::stone_op`]). A mutation HALTS
//! the walk at the first moss that ANSWERS — refusals are real answers
//! about THAT stone, never reasons to quietly try somewhere else (L17).
//! Reads stay tolerant: observation walks past a sick answerer.

mod moss_http;
mod tending;

use clap::{Parser, Subcommand};
use garden_contract::chirp::ChirpFrame;
use garden_contract::consts;
use garden_kernel::probe;
use moss_http::AttachError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

/// How long rake listens for discovery answers after the call (ms).
const DEFAULT_TIMEOUT_MS: u64 = 2500;
/// How long rake waits for a moss's HTTP answer to a READ.
const HTTP_TIMEOUT: Duration = Duration::from_secs(3);
/// Mutations can move worlds: plant pulls images and starts containers
/// (place witnessed taking >3s even warm), so they carry a wider budget.
/// Rest/wake/uproot ride it too — one mutation budget is easier to reason
/// about than per-verb budgets (P1).
const MUTATION_HTTP_TIMEOUT: Duration = Duration::from_secs(120);

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

    /// Emit JSON instead of a human rendering.
    #[arg(long, global = true)]
    json: bool,

    /// Extract one value via dot notation (implies --json).
    /// Example: --field 'data.offering.identity.name'
    #[arg(long, global = true)]
    field: Option<String>,

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
    Find {
        pattern: String,
        /// Output format: human (default) or uri - one offering URI per
        /// line, nothing else. The connection promise as output.
        #[arg(long)]
        format: Option<String>,
    },
    /// Plant an offering by catalog name (or --image for ad-hoc placement).
    Offer {
        /// The offering's name; a catalog manifest wins when one exists.
        name: String,
        /// Raw image reference — ad-hoc planting without a catalog entry.
        #[arg(long)]
        image: Option<String>,
        /// Named port: NAME=CONTAINER_PORT, repeatable.
        #[arg(long = "port", value_name = "NAME=PORT")]
        ports: Vec<String>,
        /// Declared install-form input: KEY=VALUE, repeatable.
        #[arg(long = "input", value_name = "KEY=VALUE")]
        inputs: Vec<String>,
        /// Which world to place into; absent = this stone's default.
        #[arg(long)]
        runtime: Option<String>,
        /// Category override (catalog manifests carry their own).
        #[arg(long)]
        category: Option<String>,
    },
    /// The placed record rendered by hand: what runs and WHY it decided so.
    Explain { name: String },
    /// Rest a managed offering — stopped, and converge will keep it so.
    Rest { name: String },
    /// Wake a rested offering; resurrects from its stored spec if needed.
    Wake { name: String },
    /// Uproot — remove the workload and forget the offering entirely.
    Uproot { name: String },
    /// Storage banks: list this stone's, or adopt a removable volume.
    Storage {
        #[command(subcommand)]
        cmd: Option<StorageCmd>,
    },
    /// List what the attached stone hosts - with each offering's URI.
    /// The connection promise as output (J1).
    List,
    /// Run an offering's declared will (ADR-0005): imprint, pack, ferry,
    /// commit. `--last` reports the previous run instead.
    Capture {
        /// The offering's name (FQN or bare stem).
        name: String,
        /// Report the previous run instead of starting a new one.
        #[arg(long)]
        last: bool,
    },
    /// Replant an offering from its checkpoint: verify, restore, place.
    /// Same FQN, same connection strings - the incarnation returns.
    Replant {
        /// The offering's name (FQN or bare stem).
        name: String,
        /// The checkpoint run; absent = the newest.
        #[arg(long)]
        run: Option<String>,
    },
}

/// Storage verbs — each has its 1:1 API face (`/api/v1/storage*`).
#[derive(Subcommand)]
enum StorageCmd {
    /// The adopt ceremony: write the garden manifest onto a removable
    /// volume and announce it to the room (ADR-0005 §8).
    Adopt {
        /// The volume's mount point (what `rake storage` lists as device).
        device: String,
        /// The bank's logical name — FQN or bare stem (canonicalized).
        #[arg(long)]
        name: String,
    },
    /// Eject a bank: authoritative absence, sung to the room (§8.3).
    /// Safe-to-pull is the song's promise.
    Eject {
        /// The bank's name (FQN or bare stem).
        bank: String,
    },
    /// Declare a bank's roles (ADR-0005 §4): `--role sink` makes it a
    /// checkpoint sink - the will ferries to it.
    Roles {
        /// The bank's name (FQN or bare stem).
        bank: String,
        /// The complete role set, repeatable.
        #[arg(long = "role")]
        roles: Vec<String>,
    },
    /// List a bank's files (optional --path for a subdirectory).
    Files {
        /// The bank's name (FQN or bare stem).
        bank: String,
        /// A subdirectory of the bank to list.
        #[arg(long)]
        path: Option<String>,
    },
    /// Read one file from a bank: raw bytes to --out, or stdout.
    Get {
        /// The bank's name (FQN or bare stem).
        bank: String,
        /// The file's path, relative to the bank's root.
        path: String,
        /// Write the bytes here instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Write one file onto a bank — makes a sink a real storage
    /// destination. Parents are created on the bank.
    Put {
        /// The bank's name (FQN or bare stem).
        bank: String,
        /// The file's path, relative to the bank's root.
        path: String,
        /// The local file to send, or `-` for stdin.
        #[arg(long = "file")]
        file: String,
    },
    /// Delete one file from a bank. Directories refuse — wholesale
    /// removal is the operator's hand.
    Rm {
        /// The bank's name (FQN or bare stem).
        bank: String,
        /// The file's path, relative to the bank's root.
        path: String,
    },
    /// The room's banks — every stone's storage, from the one cache.
    Garden,
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

    /// What humans should call this attachment target.
    fn label(&self) -> String {
        self.name_hint.clone().unwrap_or_else(|| self.endpoint())
    }
}

/// What the attached moss reports about its garden (standard formats, L21).
/// The canonical frame — sections and all. Peer rows carry the reporter's
/// reception count; the spliced self row carries neither (it IS the chirp).
/// One shape: wire, cache, HTTP, CLI.
#[derive(Debug, Clone, Deserialize, Serialize)]
struct GardenStone {
    #[serde(flatten)]
    body: ChirpFrame,
    /// True for the reporter's own splice (ADR-0004 §3 — self is a
    /// projection, never a stored peer).
    #[serde(default, rename = "self")]
    is_self: bool,
    /// Frames the reporter has accepted from this peer; absent on self.
    #[serde(default)]
    chirps: Option<u64>,
}

// ---------------------------------------------------------------------------
// The attachment cascade (phases 1–4) and its single walker
// ---------------------------------------------------------------------------

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
            probe::ask_the_room(port, Some(group), Duration::from_millis(self.timeout_ms), "rake")
                .await
        {
            for s in sightings {
                out.push(Candidate {
                    ip: s.stone.network.address.ip,
                    http_port: s.stone.network.address.port,
                    name_hint: Some(s.stone.name),
                    origin: Origin::Discovered,
                });
            }
        }
        out
    }

    /// The ordered walk plan: what to try before discovery, then whom to
    /// discover. Name-shaped explicit intents skip straight to the room and
    /// narrow to exactly one answering stone (ambiguous = refuse loudly).
    async fn targets(&self) -> Result<(Vec<Candidate>, Vec<Candidate>), String> {
        let explicit_is_name = self
            .stone
            .as_deref()
            .is_some_and(|v| parse_ip_port(v).is_none());

        if explicit_is_name {
            let value = self.stone.clone().unwrap_or_default();
            let needle = value.to_lowercase();
            let candidates = self.discovered_candidates().await;
            let matches: Vec<Candidate> = candidates
                .into_iter()
                .filter(|c| {
                    c.name_hint
                        .as_deref()
                        .is_some_and(|n| n.to_lowercase().contains(&needle))
                })
                .collect();
            return match matches.len() {
                1 => Ok((vec![Candidate { origin: Origin::Flag, ..matches[0].clone() }], vec![])),
                0 => Err(format!("no stone answering matches '{value}'")),
                _ => Err(format!(
                    "'{value}' is ambiguous: {} stones match; use ip:port",
                    matches.len()
                )),
            };
        }

        let early = self.immediate_candidates()?;
        let late = self.discovered_candidates().await;
        Ok((early, late))
    }

    /// Walk candidates in order; the first accepted answer wins.
    ///
    /// `stop_when_answered` separates reads from writes: mutations halt at
    /// the first moss that ANSWERS with anything (its refusal binds), while
    /// observation may continue past a sick answerer to a healthier one.
    /// Connection failures stay retryable either way; explicitly pinned
    /// targets unreachable by connection abort loudly — intent is never
    /// silently redirected.
    ///
    /// Ok means "(candidate, answer)" and the CALLER remembers tending.
    async fn walk<T, F, Fut>(
        &self,
        stop_when_answered: bool,
        exhausted: &str,
        mut exec: F,
    ) -> Result<(Candidate, T), String>
    where
        F: FnMut(Candidate) -> Fut,
        Fut: Future<Output = Result<T, AttachError>>,
    {
        let (early, late) = self.targets().await?;
        let tried_early = !early.is_empty();

        for cand in early {
            if let Some(pair) = self.attempt(&cand, stop_when_answered, &mut exec).await? {
                return Ok(pair);
            }
        }
        if tried_early {
            eprintln!("rake: tended stone unreachable - discovering...");
        }
        for cand in late {
            if let Some(pair) = self.attempt(&cand, stop_when_answered, &mut exec).await? {
                return Ok(pair);
            }
        }
        Err(exhausted.to_string())
    }

    /// One round against one candidate. Shared refusal policy so both walk
    /// phases behave identically — one place encodes it (R3):
    ///   · success → attach,
    ///   · connection failure → tending flush; soft origins continue,
    ///     pinned ones abort loudly,
    ///   · any other answer → abort when mutations must bind to their
    ///     first answer, tolerate on tolerant reads.
    async fn attempt<T, F, Fut>(
        &self,
        cand: &Candidate,
        stop_when_answered: bool,
        exec: &mut F,
    ) -> Result<Option<(Candidate, T)>, String>
    where
        F: FnMut(Candidate) -> Fut,
        Fut: Future<Output = Result<T, AttachError>>,
    {
        match exec(cand.clone()).await {
            Ok(answer) => Ok(Some((cand.clone(), answer))),
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
                            cand.label()
                        ));
                    }
                    Ok(None)
                } else if stop_when_answered {
                    Err(format!(
                        "{} ({}) — {e}",
                        cand.endpoint(),
                        cand.origin_label()
                    ))
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// Attach to the first moss that answers with a garden view (L21:
    /// rake renders; moss computes).
    async fn garden_view(&self) -> Result<Vec<GardenStone>, String> {
        let (cand, stones) = self
            .walk(
                false,
                "no moss found: nothing pinned, nothing tended, nobody answered",
                |cand| async move {
                    let v =
                        moss_http::get_json(cand.ip, cand.http_port, "/api/v1/garden/stones", HTTP_TIMEOUT)
                            .await?;
                    parse_garden(&v).map_err(moss_http::AttachError::ProcessingError)
                },
            )
            .await?;
        remember_attachment(&cand, &stones);
        Ok(stones)
    }

    /// Run ONE request against the first moss that answers. The stone ops'
    /// shared front door: cascade, halt-on-refusal, tend-on-success all in
    /// one place; every verb command goes through this and nothing else.
    async fn stone_op(
        &self,
        method: &'static str,
        path: String,
        body: Option<&serde_json::Value>,
    ) -> Result<(Candidate, serde_json::Value), String> {
        let timeout = if method == "GET" { HTTP_TIMEOUT } else { MUTATION_HTTP_TIMEOUT };
        let body_owned = body.cloned();
        let (cand, value) = self
            .walk(true, "no moss answered; nothing was changed", move |cand| {
                let method = method.to_string();
                let path = path.clone();
                let body = body_owned.clone();
                async move {
                    moss_http::request_json(
                        &method,
                        cand.ip,
                        cand.http_port,
                        &path,
                        body.as_ref(),
                        timeout,
                    )
                    .await
                }
            })
            .await?;
        remember_attachment(&cand, &[]);
        Ok((cand, value))
    }

    /// Run ONE raw-bytes request against the first moss that answers —
    /// the file verbs' front door, the same cascade and halt law as
    /// [`Cli::stone_op`]. Files can be big: the mutation budget rides
    /// reads as well as writes (one wide budget, P1).
    async fn stone_bytes(
        &self,
        method: &'static str,
        path: String,
        body: Option<Vec<u8>>,
    ) -> Result<(Candidate, u16, Vec<u8>), String> {
        let owned = body;
        self.walk(true, "no moss answered; nothing was changed", move |cand| {
            let method = method.to_string();
            let path = path.clone();
            let body = owned.clone();
            async move {
                moss_http::request_bytes(
                    &method,
                    cand.ip,
                    cand.http_port,
                    &path,
                    Some("application/octet-stream"),
                    body.as_deref(),
                    MUTATION_HTTP_TIMEOUT,
                )
                .await
            }
        })
        .await
        .map(|(cand, (status, bytes))| (cand, status, bytes))
    }

    /// The file verbs' front door. A bank grows on ONE stone: when the
    /// attached moss answers not-here (the garden's only true redirect),
    /// this follows the `knows_at` way ONCE and re-binds there — reads
    /// delegate, writes bind at their authority. The home stone's answer
    /// is final: it owns the bank, so its refusals are the bank's truth,
    /// and a redirect loop is refused, never chased.
    async fn bank_bytes(
        &self,
        method: &'static str,
        path: String,
        body: Option<Vec<u8>>,
    ) -> Result<(Candidate, u16, Vec<u8>), String> {
        let (cand, status, bytes) =
            self.stone_bytes(method, path.clone(), body.clone()).await?;
        let Some(home) = not_here_home(status, &bytes) else {
            return Ok((cand, status, bytes));
        };
        eprintln!("rake: the bank grows elsewhere - asking {home}");
        let (status, bytes) = reissue_at_home(method, &home, &path, body.as_deref()).await?;
        Ok((cand, status, bytes))
    }
}

/// The garden's only true redirect, recognized: a 404 whose body says
/// `not_here` and names the holder's `knows_at`. Any other answer is the
/// answer.
fn not_here_home(status: u16, body: &[u8]) -> Option<String> {
    if status != 404 {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(body).ok()?;
    if v["error"]["not_here"] != true {
        return None;
    }
    v["error"]["knows_at"].as_str().map(str::to_string)
}

/// The holder's way (`http://ip:port/api/v1/stone`) as an endpoint.
fn parse_http_home(home: &str) -> Result<(IpAddr, u16), String> {
    let rest = home
        .strip_prefix("http://")
        .ok_or_else(|| format!("knows_at '{home}' is not a plain-http way"))?;
    let authority = rest.split('/').next().unwrap_or(rest);
    parse_ip_port(authority)
        .ok_or_else(|| format!("knows_at '{home}' carries an unreadable address"))
}

/// One raw re-request against the named home stone. An unreachable home
/// aborts loudly — the room named the holder, there is no softer target.
async fn reissue_at_home(
    method: &str,
    home: &str,
    path: &str,
    body: Option<&[u8]>,
) -> Result<(u16, Vec<u8>), String> {
    let (ip, port) = parse_http_home(home)?;
    moss_http::request_bytes(
        method,
        ip,
        port,
        path,
        Some("application/octet-stream"),
        body,
        MUTATION_HTTP_TIMEOUT,
    )
    .await
    .map_err(|e| format!("the bank's home stone at {home} did not answer: {e}"))
}

// ---------------------------------------------------------------------------
// Command dispatch and rendering
// ---------------------------------------------------------------------------

/// Exit codes (R3.3: the process answers with a code, not just a message).
/// The full vocabulary lands with the typed error refactor; GENERAL is
/// wired now, the rest are declared so callers can plan against them.
mod exit {
    pub const GENERAL: i32 = 1;
    /// Not yet wired: arrives with typed errors carrying their own code.
    #[allow(dead_code)]
    pub const NOT_FOUND: i32 = 2;
    /// Not yet wired: same.
    #[allow(dead_code)]
    pub const CONFLICT: i32 = 3;
    /// Not yet wired: same.
    #[allow(dead_code)]
    pub const UNAVAILABLE: i32 = 4;
}

#[tokio::main]
async fn main() {
    let mut cli = Cli::parse();
    // --field implies JSON mode: you cannot extract from a table.
    if cli.field.is_some() {
        cli.json = true;
    }
    if let Err(msg) = run(&cli).await {
        if cli.json {
            println!("{}", serde_json::json!({ "error": { "message": msg } }));
        } else {
            eprintln!("rake: {msg}");
        }
        std::process::exit(exit::GENERAL);
    }
}

async fn run(cli: &Cli) -> Result<(), String> {
    match &cli.command {
        Command::List => {
            let (_, v) = cli
                .stone_op("GET", paths::OFFERINGS.to_string(), None)
                .await?;
            if cli.json {
                return emit_output(&v, cli);
            }
            render_list(&envelope_plain(&v)?)
        }
        Command::Observe => {
            let stones = cli.garden_view().await?;
            if cli.json {
                let arr = serde_json::to_value(&stones)
                    .map_err(|e| format!("could not render json: {e}"))?;
                emit_output(&arr, cli)
            } else {
                print_table(&stones);
                Ok(())
            }
        }
        Command::Find { pattern, format } => {
            let needle = pattern.to_lowercase();
            let mut stones = cli.garden_view().await?;
            stones.retain(|s| s.body.stone.name.to_lowercase().contains(&needle));
            match format.as_deref() {
                Some("uri") | Some("uri-ip") => {
                    // The connection promise as output: one URI per line.
                    for s in &stones {
                        let ip = s.body.stone.network.address.ip;
                        for svc in s
                            .body
                            .inventory
                            .services
                            .as_ref()
                            .map(|b| b.items.as_slice())
                            .unwrap_or_default()
                        {
                            let port = svc.ports.values().next();
                            match port {
                                Some(p) => println!("{}://{}:{}", svc.stem, ip, p),
                                None => println!("{}://{}", svc.stem, ip),
                            }
                        }
                    }
                    Ok(())
                }
                _ => {
                    if cli.json {
                        let arr = serde_json::to_value(&stones)
                            .map_err(|e| format!("could not render json: {e}"))?;
                        emit_output(&arr, cli)
                    } else {
                        print_table(&stones);
                        Ok(())
                    }
                }
            }
        }
        Command::Offer { .. } | Command::Explain { .. } | Command::Rest { .. }
        | Command::Wake { .. } | Command::Uproot { .. } | Command::Capture { .. }
        | Command::Replant { .. } => cmd_stone_op(cli).await,
        Command::Storage { cmd } => cmd_storage(cli, cmd.as_ref()).await,
    }
}

/// The storage faces. Every verb here is the 1:1 client of one API face:
/// list -> GET /api/v1/storage; adopt -> POST /api/v1/storage/adopt.
async fn cmd_storage(cli: &Cli, cmd: Option<&StorageCmd>) -> Result<(), String> {
    match cmd {
        None => {
            let (_, v) = cli.stone_op("GET", paths::STORAGE.to_string(), None).await?;
            if cli.json {
                return emit_output(&v, cli);
            }
            render_storage(&envelope_plain(&v)?)
        }
        Some(StorageCmd::Roles { bank, roles }) => {
            let body = serde_json::json!({ "roles": roles });
            let (_, v) = cli
                .stone_op("POST", paths::storage_roles(bank), Some(&body))
                .await?;
            if cli.json {
                return emit_output(&v, cli);
            }
            let b = envelope(&v, "bank")?;
            println!(
                "{} now holds: {}",
                display_name(b["fqn"].as_str().unwrap_or("(unnamed)")),
                b["roles"]
                    .as_array()
                    .map(|r| r
                        .iter()
                        .filter_map(|x| x.as_str())
                        .collect::<Vec<_>>()
                        .join(", "))
                    .unwrap_or_else(|| "(none)".into()),
            );
            Ok(())
        }
        Some(StorageCmd::Garden) => {
            let (_, v) = cli
                .stone_op("GET", paths::STORAGE_GARDEN.to_string(), None)
                .await?;
            if cli.json {
                return emit_output(&v, cli);
            }
            render_garden_storage(&envelope_plain(&v)?)
        }
        Some(StorageCmd::Eject { bank }) => {
            let (_, v) = cli
                .stone_op("POST", paths::storage_eject(bank), None)
                .await?;
            if cli.json {
                return emit_output(&v, cli);
            }
            let b = envelope(&v, "bank")?;
            println!(
                "{} ejected — the garden hears the absence within one song",
                display_name(b["fqn"].as_str().unwrap_or("(unnamed)"))
            );
            Ok(())
        }
        Some(StorageCmd::Adopt { device, name }) => {
            let body = serde_json::json!({ "device": device, "name": name });
            let (_, v) = cli
                .stone_op("POST", paths::STORAGE_ADOPT.to_string(), Some(&body))
                .await?;
            if cli.json {
                return emit_output(&v, cli);
            }
            let bank = envelope(&v, "bank")?;
            println!(
                "{} adopted on {} — {}",
                display_name(bank["fqn"].as_str().unwrap_or("(unnamed)")),
                bank["mount_point"].as_str().unwrap_or("?"),
                bank["state"].as_str().unwrap_or("?"),
            );
            if let (Some(cap), Some(used)) = (bank["capacity_bytes"].as_u64(), bank["used_bytes"].as_u64())
            {
                println!("  capacity  {} ({} used)", human_bytes(cap), human_bytes(used));
            }
            println!("  the garden hears the news within one song");
            Ok(())
        }
        Some(StorageCmd::Files { bank, path }) => {
            let mut target = paths::storage_files(bank);
            if let Some(dir) = path {
                target = format!("{}?path={}", target, paths::encode_segment(dir));
            }
            let (_, status, body) = cli.bank_bytes("GET", target, None).await?;
            if status != 200 {
                return Err(raw_refusal(status, &body));
            }
            let v: serde_json::Value = serde_json::from_slice(&body)
                .map_err(|e| format!("moss answered unparsable: {e}"))?;
            if cli.json {
                return emit_output(&v, cli);
            }
            render_bank_files(&envelope_plain(&v)?)
        }
        Some(StorageCmd::Get { bank, path, out }) => {
            // The file IS the output — --json has nothing to re-render,
            // so the raw bytes ride to --out or stdout untouched.
            let (_, status, bytes) =
                cli.bank_bytes("GET", paths::storage_file(bank, path), None).await?;
            if status != 200 {
                return Err(raw_refusal(status, &bytes));
            }
            match out {
                Some(dest) => {
                    std::fs::write(dest, &bytes)
                        .map_err(|e| format!("could not write '{}': {e}", dest.display()))?;
                    println!("{} → {} ({} bytes)", path, dest.display(), bytes.len());
                }
                None => {
                    std::io::stdout()
                        .write_all(&bytes)
                        .map_err(|e| format!("could not write stdout: {e}"))?;
                }
            }
            Ok(())
        }
        Some(StorageCmd::Put { bank, path, file }) => {
            let bytes = read_local(file)?;
            let (_, status, body) = cli
                .bank_bytes("PUT", paths::storage_file(bank, path), Some(bytes))
                .await?;
            if status != 200 {
                return Err(raw_refusal(status, &body));
            }
            let v: serde_json::Value = serde_json::from_slice(&body)
                .map_err(|e| format!("moss answered unparsable: {e}"))?;
            if cli.json {
                return emit_output(&v, cli);
            }
            let written = v["data"]["size_bytes"].as_u64().unwrap_or(0);
            println!(
                "{} written onto {} ({} bytes)",
                path,
                display_name(bank),
                written
            );
            Ok(())
        }
        Some(StorageCmd::Rm { bank, path }) => {
            let (_, status, body) =
                cli.bank_bytes("DELETE", paths::storage_file(bank, path), None).await?;
            if status != 200 {
                return Err(raw_refusal(status, &body));
            }
            if cli.json {
                let v: serde_json::Value = serde_json::from_slice(&body)
                    .map_err(|e| format!("moss answered unparsable: {e}"))?;
                return emit_output(&v, cli);
            }
            println!("{} removed from {}", path, display_name(bank));
            Ok(())
        }
    }
}

/// A bank's directory listing: one row per entry, directories marked.
fn render_bank_files(v: &serde_json::Value) -> Result<(), String> {
    let rows = v["files"].as_array();
    match rows {
        Some(r) if !r.is_empty() => {
            println!("{:<40} {:<5} {:>10}  MODIFIED", "NAME", "KIND", "SIZE");
            for e in r {
                println!(
                    "{:<40} {:<5} {:>10}  {}",
                    e["name"].as_str().unwrap_or("?"),
                    e["kind"].as_str().unwrap_or("?"),
                    e["size_bytes"].as_u64().map(human_bytes).unwrap_or_else(|| "-".into()),
                    e["modified_at"].as_str().unwrap_or("-"),
                );
            }
        }
        _ => println!("The bank holds no files here."),
    }
    Ok(())
}

/// Read the `--file` side of `storage put`: a local path, or `-` for stdin.
fn read_local(source: &str) -> Result<Vec<u8>, String> {
    if source == "-" {
        let mut buf = Vec::new();
        std::io::stdin()
            .lock()
            .read_to_end(&mut buf)
            .map_err(|e| format!("could not read stdin: {e}"))?;
        return Ok(buf);
    }
    std::fs::read(source).map_err(|e| format!("could not read '{source}': {e}"))
}

/// A non-200 answer from a raw-bytes request: pull the moss's error
/// envelope message when one rides the body, else name the status.
fn raw_refusal(status: u16, body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body);
    match serde_json::from_str::<serde_json::Value>(text.trim()) {
        Ok(v) if v["error"]["message"].is_string() => {
            v["error"]["message"].as_str().unwrap_or_default().to_string()
        }
        _ => format!("HTTP {status}"),
    }
}

/// Bytes for human eyes.
fn human_bytes(n: u64) -> String {
    let units = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < units.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} {}", units[0])
    } else {
        format!("{value:.1} {}", units[unit])
    }
}

/// The room's banks: one row per (stone, bank), self marked.
fn render_garden_storage(v: &serde_json::Value) -> Result<(), String> {
    let rows = v["banks"].as_array();
    match rows {
        Some(r) if !r.is_empty() => {
            println!("{:<22} {:<26} {:<10} CAPACITY", "STONE", "BANK", "STATE");
            for row in r {
                let marker = if row["self"] == true { " (me)" } else { "" };
                println!(
                    "{:<22} {:<26} {:<10} {}",
                    format!("{}{}", row["stone"].as_str().unwrap_or("?"), marker),
                    display_name(row["bank"]["fqn"].as_str().unwrap_or("?")),
                    row["bank"]["state"].as_str().unwrap_or("?"),
                    row["bank"]["capacity_bytes"]
                        .as_u64()
                        .map(human_bytes)
                        .unwrap_or_else(|| "-".into()),
                );
            }
        }
        _ => println!("The garden holds no banks yet."),
    }
    Ok(())
}

/// `rake list`: what the attached stone hosts, each with its URI —
/// `stem://host:home`. The connection promise as output (J1).
fn render_list(v: &serde_json::Value) -> Result<(), String> {
    let rows = v["offerings"].as_array();
    match rows {
        Some(r) if !r.is_empty() => {
            println!("{:<26} {:<10} {:<12} URI", "OFFERING", "STATUS", "HOME");
            for o in r {
                let stem = o["identity"]["stem"].as_str().unwrap_or("?");
                let home = o["mode"]["port_map"]
                    .as_object()
                    .and_then(|m| m.values().next())
                    .and_then(|p| p.as_u64())
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "-".into());
                println!(
                    "{:<26} {:<10} {:<12} {}://{}",
                    display_name(o["identity"]["name"].as_str().unwrap_or("?")),
                    o["state"]["status"].as_str().unwrap_or("?"),
                    home,
                    stem,
                    home,
                );
            }
        }
        _ => println!("Nothing planted on this stone yet. Try: rake offer <name>"),
    }
    Ok(())
}

/// The banks table: what this stone holds, and what it could adopt.
fn render_storage(v: &serde_json::Value) -> Result<(), String> {
    let banks = v["banks"].as_array();
    match banks {
        Some(b) if !b.is_empty() => {
            println!("{:<26} {:<10} {:<22} CAPACITY", "BANK", "STATE", "DEVICE");
            for bank in b {
                let cap = bank["capacity_bytes"]
                    .as_u64()
                    .map(human_bytes)
                    .unwrap_or_else(|| "-".into());
                println!(
                    "{:<26} {:<10} {:<22} {}",
                    display_name(bank["fqn"].as_str().unwrap_or("?")),
                    bank["state"].as_str().unwrap_or("?"),
                    bank["mount_point"].as_str().unwrap_or("?"),
                    cap,
                );
            }
        }
        _ => println!("No banks adopted yet on this stone."),
    }
    if let Some(adoptable) = v["adoptable"].as_array() {
        for vol in adoptable {
            println!(
                "ready to adopt: {} ({}) — rake storage adopt <device> --name <bank>",
                vol["device"].as_str().unwrap_or("?"),
                vol["capacity_bytes"]
                    .as_u64()
                    .map(human_bytes)
                    .unwrap_or_else(|| "unknown size".into()),
            );
        }
    }
    Ok(())
}

async fn cmd_stone_op(cli: &Cli) -> Result<(), String> {
    match &cli.command {
        Command::Offer { name, image, ports, inputs, runtime, category } => {
            let ports = parse_u16_pairs(ports)?;
            let inputs_map = parse_input_map(inputs)?;
            // Thin client honesty: only keys the operator set ride along.
            let mut body = serde_json::Map::new();
            let ports_json = serde_json::to_value(ports)
                .map_err(|e| format!("could not encode --port values: {e}"))?;
            body.insert("ports".into(), ports_json);
            let inputs_json = serde_json::to_value(inputs_map)
                .map_err(|e| format!("could not encode --input values: {e}"))?;
            body.insert("inputs".into(), inputs_json);
            if let Some(v) = image { body.insert("image".into(), serde_json::json!(v)); }
            if let Some(v) = runtime { body.insert("runtime".into(), serde_json::json!(v)); }
            if let Some(v) = category { body.insert("category".into(), serde_json::json!(v)); }

            let (target, v) = cli.stone_op("POST", paths::record(name), Some(&body.into())).await?;
            if cli.json {
                return emit_output(&v, cli);
            }
            render_offered(&target, envelope(&v, "offering")?)
        }
        Command::Explain { name } => {
            let (target, v) = cli.stone_op("GET", paths::record(name), None).await?;
            if cli.json {
                return emit_output(&v, cli);
            }
            let data = envelope_plain(&v)?;
            render_explain(&target, data["offering"].clone(), &data)
        }
        Command::Rest { name } => {
            let (_, v) = cli.stone_op("POST", paths::rest(name), None).await?;
            if cli.json {
                return emit_output(&v, cli);
            }
            render_status("rested", &envelope_plain(&v)?)
        }
        Command::Capture { name, last } => {
            if *last {
                let (_, v) = cli
                    .stone_op("GET", paths::capture_last(name), None)
                    .await?;
                if cli.json {
                    return emit_output(&v, cli);
                }
                let run = envelope_plain(&v)?;
                println!(
                    "{} — last capture: {} ({}){}",
                    display_name(name),
                    run["phase"].as_str().unwrap_or("?"),
                    run["started_at"].as_str().unwrap_or("?"),
                    run["error"]
                        .as_str()
                        .map(|e| format!(" — {e}"))
                        .unwrap_or_default(),
                );
                if let Some(cp) = run["checkpoint"].as_str() {
                    println!("  checkpoint  {cp}");
                }
                if let Some(sinks) = run["ferried_to"].as_array()
                    && !sinks.is_empty()
                {
                    let names: Vec<&str> = sinks.iter().filter_map(|s| s.as_str()).collect();
                    println!("  ferried to  {}", names.join(", "));
                }
                return Ok(());
            }
            let (_, v) = cli
                .stone_op("POST", paths::capture(name), None)
                .await?;
            if cli.json {
                return emit_output(&v, cli);
            }
            let run = envelope(&v, "run")?;
            println!(
                "{} — capture accepted, run {}",
                display_name(name),
                run["run_id"].as_str().unwrap_or("?"),
            );
            println!("  the will executes in the background; `rake capture {name} --last` reports progress");
            Ok(())
        }
        Command::Replant { name, run } => {
            let mut body = serde_json::Map::new();
            if let Some(r) = run {
                body.insert("run".into(), serde_json::json!(r));
            }
            let (_, v) = cli
                .stone_op(
                    "POST",
                    paths::replant(name),
                    Some(&serde_json::Value::Object(body)),
                )
                .await?;
            if cli.json {
                return emit_output(&v, cli);
            }
            let o = envelope_plain(&v)?;
            println!(
                "{} replanted — {}",
                display_name(name),
                o["offering"]["status"].as_str().unwrap_or("?"),
            );
            if let Some(from) = o["offering"]["replanted_from"].as_str() {
                println!("  from      {from}");
            }
            if let Some(h) = o["offering"]["final_hash"].as_str() {
                println!("  hash      {h}");
            }
            Ok(())
        }
        Command::Wake { name } => {
            let (_, v) = cli.stone_op("POST", paths::wake(name), None).await?;
            if cli.json {
                return emit_output(&v, cli);
            }
            let data = envelope_plain(&v)?;
            render_status("awake", &data)?;
            if let Some(port_map) = data.get("port_map")
                && port_map.as_object().is_some_and(|m| !m.is_empty())
            {
                println!("  ports {}", named_pairs(port_map, ", "));
            }
            Ok(())
        }
        Command::Uproot { name } => {
            let (_, v) = cli.stone_op("DELETE", paths::record(name), None).await?;
            if cli.json {
                emit_output(&v, cli)
            } else {
                // Echo what was ACTUALLY uprooted, moniker-displayed.
                let canonical = v["data"]["name"].as_str().unwrap_or(name);
                println!("{} uprooted", display_name(canonical));
                Ok(())
            }
        }
        _ => unreachable!("dispatch routes only stone ops here"),
    }
}

/// Request paths with their commands (R2.2): built where used, kept nowhere.
mod paths {
    pub const OFFERINGS: &str = "/api/v1/offerings";

    /// Local storage (L22): banks and adoptable volumes.
    pub const STORAGE: &str = "/api/v1/storage";
    /// The adopt ceremony's face.
    pub const STORAGE_ADOPT: &str = "/api/v1/storage/adopt";
    /// The room's banks (grid law, ADR-0004 §4).
    pub const STORAGE_GARDEN: &str = "/api/v1/garden/storage";
    /// The eject verb's face.
    pub fn storage_eject(bank: &str) -> String {
        format!("{STORAGE}/{}/eject", encode_segment(bank))
    }

    /// The roles declaration's face.
    pub fn storage_roles(bank: &str) -> String {
        format!("{STORAGE}/{}/roles", encode_segment(bank))
    }

    /// The bank-files list face.
    pub fn storage_files(bank: &str) -> String {
        format!("{STORAGE}/{}/files", encode_segment(bank))
    }

    /// One file's face on a bank. The path is wire-encoded whole — `/`
    /// inside a name nests on the bank, everything unsafe escapes.
    pub fn storage_file(bank: &str, path: &str) -> String {
        format!(
            "{STORAGE}/{}/files/{}",
            encode_segment(bank),
            encode_segment(path)
        )
    }

    /// Percent-encode one wire path segment: everything outside the
    /// unreserved set escapes, so names with spaces, `#` or `/` ride
    /// correctly. Zero deps (P5) — one table of safe bytes.
    pub fn encode_segment(s: &str) -> String {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        let mut out = String::with_capacity(s.len());
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                    out.push(b as char)
                }
                other => {
                    out.push('%');
                    out.push(HEX[usize::from(other >> 4)] as char);
                    out.push(HEX[usize::from(other & 15)] as char);
                }
            }
        }
        out
    }

    /// The living will's faces (ADR-0005 §2).
    pub fn capture(name: &str) -> String {
        format!("{OFFERINGS}/{name}/capture")
    }

    /// The last-run report face.
    pub fn capture_last(name: &str) -> String {
        format!("{OFFERINGS}/{name}/capture")
    }

    /// The replant face.
    pub fn replant(name: &str) -> String {
        format!("{OFFERINGS}/{name}/replant")
    }

    pub fn record(name: &str) -> String {
        format!("{OFFERINGS}/{name}")
    }

    pub fn rest(name: &str) -> String {
        format!("{OFFERINGS}/{name}/rest")
    }

    pub fn wake(name: &str) -> String {
        format!("{OFFERINGS}/{name}/wake")
    }
}

// --- small shared machinery -------------------------------------------------

/// Standard envelope lives at `data` (L21); all stone ops answer inside it.
fn envelope_plain(v: &serde_json::Value) -> Result<serde_json::Value, String> {
    v.get("data")
        .cloned()
        .ok_or_else(|| "response lacked the standard 'data' envelope".to_string())
}

fn envelope(v: &serde_json::Value, key: &str) -> Result<serde_json::Value, String> {
    envelope_plain(v)?
        .get(key)
        .cloned()
        .ok_or_else(|| format!("response lacked data.{key}"))
}

/// Extract one value via dot notation with array indexing.
/// `"services[0].connection.uris[0]"` walks objects and arrays.
/// Returns the value as a string (objects/arrays serialize as JSON).
fn extract_json_field(value: &serde_json::Value, path: &str) -> Option<String> {
    let mut current = value;
    for segment in path.split('.') {
        if let Some(bracket_pos) = segment.find('[') {
            let field_name = &segment[..bracket_pos];
            let rest = &segment[bracket_pos..];
            if !field_name.is_empty() {
                current = current.get(field_name)?;
            }
            let mut chars = rest.chars().peekable();
            while chars.peek() == Some(&'[') {
                chars.next();
                let mut index_str = String::new();
                while let Some(&c) = chars.peek() {
                    if c == ']' {
                        chars.next();
                        break;
                    }
                    index_str.push(c);
                    chars.next();
                }
                let index: usize = index_str.parse().ok()?;
                current = current.get(index)?;
            }
        } else {
            current = current.get(segment)?;
        }
    }
    match current {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        other => Some(other.to_string()),
    }
}

/// Emit the JSON output, or extract one field if --field is set.
fn emit_output(v: &serde_json::Value, cli: &Cli) -> Result<(), String> {
    if let Some(path) = &cli.field {
        let extracted = extract_json_field(v, path).ok_or_else(|| {
            format!("field '{path}' not found in output")
        })?;
        println!("{extracted}");
        return Ok(());
    }
    emit_pretty(v)
}

fn emit_pretty(v: &serde_json::Value) -> Result<(), String> {
    serde_json::to_string_pretty(v)
        .map(|s| println!("{s}"))
        .map_err(|e| format!("could not render json: {e}"))
}

/// Parse repeated NAME=NUMBER flags into an ordered map.
fn parse_u16_pairs(raw: &[String]) -> Result<HashMap<String, u16>, String> {
    raw.iter()
        .map(|s| match s.split_once('=') {
            Some((k, v)) => v.parse::<u16>().map(|n| (k.to_string(), n)).map_err(|_| {
                format!("--port '{s}' must look like NAME=NUMBER (e.g. --port default=6379)")
            }),
            None => Err(format!(
                "--port '{s}' must look like NAME=PORT (e.g. --port default=6379)"
            )),
        })
        .collect()
}

/// Parse repeated KEY=VALUE flags; values may contain '=' themselves.
fn parse_input_map(raw: &[String]) -> Result<std::collections::BTreeMap<String, String>, String> {
    raw.iter()
        .map(|s| match s.split_once('=') {
            Some((k, v)) => Ok((k.trim().to_string(), v.to_string())),
            None => Err(format!(
                "--input '{s}' must look like KEY=VALUE (e.g. --input password=hunter2)"
            )),
        })
        .collect()
}

/// Delight of continuity: on success, tend toward this moss (unless the
/// attachment was explicitly pinned — those are already remembered).
fn remember_attachment(cand: &Candidate, stones: &[GardenStone]) {
    if !cand.origin.is_soft() {
        return;
    }
    let name = stones
        .iter()
        .find(|s| s.body.stone.network.address.ip == cand.ip && s.body.stone.network.address.port == cand.http_port)
        .map(|s| s.body.stone.name.clone())
        .or_else(|| cand.name_hint.clone())
        .unwrap_or_else(|| "unknown".into());
    if let Some(path) = tending::default_path() {
        let _ = tending::write_to(&path, &tending::Tending::now(name, cand.endpoint()));
    }
}

fn parse_garden(v: &serde_json::Value) -> Result<Vec<GardenStone>, String> {
    let stones = envelope_plain(v)?.get("stones").cloned().ok_or_else(|| "garden view lacked data.stones".to_string())?;
    serde_json::from_value::<Vec<GardenStone>>(stones)
        .map_err(|e| format!("garden view did not match standard format: {e}"))
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

/// Render `"key": number` pairs in map order for human eyes.
fn named_pairs(map: &serde_json::Value, sep: &str) -> String {
    map.as_object()
        .map(|pairs| {
            pairs
                .iter()
                .filter_map(|(k, v)| v.as_u64().map(|n| format!("{k} → {n}")))
                .collect::<Vec<_>>()
                .join(sep)
        })
        .unwrap_or_default()
}

// --- human renderings for stone ops -----------------------------------------

/// Surfaces suppress `::default` (infrastructure noise); foreign instances
/// stay in full (`ollama::adopted` is honest on the wire AND to humans).
fn display_name(fqn: &str) -> String {
    garden_glossary::fqn::moniker(fqn)
}

/// The moment after planting: what grew, where, listening on what.
fn render_offered(target: &Candidate, offering: serde_json::Value) -> Result<(), String> {
    let name = display_name(offering["identity"]["name"].as_str().unwrap_or("(unnamed)"));
    let status = offering["state"]["status"].as_str().unwrap_or("?");
    println!("{name} planted — {status}");
    println!("  on      {} ({})", target.label(), target.origin_label());
    if let Some(image) = offering["mode"]["spec"]["image"].as_str() {
        println!("  image   {image}");
    }
    let container_ports = offering["mode"]["spec"]["named_ports"].clone();
    let host_ports = offering["mode"]["port_map"].clone();
    let host_has_names = host_ports.as_object().is_some_and(|m| !m.is_empty());
    if container_ports.is_object() {
        println!("  ports   {}", describe_ports(&container_ports, &host_ports));
    } else if host_has_names {
        println!("  ports   {}", named_pairs(&host_ports, ", "));
    }
    Ok(())
}

/// Container intention meets host reality, briefly.
fn describe_ports(container: &serde_json::Value, host: &serde_json::Value) -> String {
    let mut parts = Vec::new();
    if let Some(names) = container.as_object() {
        for (role, n) in names {
            match host.get(role).and_then(|h| h.as_u64()) {
                Some(host_port) => parts.push(format!("{role}: {n} → :{host_port}")),
                None => parts.push(format!("{role}: {n}")),
            }
        }
    }
    if parts.is_empty() {
        parts.push(named_pairs(host, ", "));
    }
    parts.join(", ")
}

/// §5.3's placed record rendered by hand — the delightful reference.
fn render_explain(
    target: &Candidate,
    offering: serde_json::Value,
    data: &serde_json::Value,
) -> Result<(), String> {
    let name = display_name(offering["identity"]["name"].as_str().unwrap_or("(unnamed)"));
    let status = offering["state"]["status"].as_str().unwrap_or("?");
    let mode = offering["mode"]["mode"].as_str().unwrap_or("?");

    println!("{name} — {} ({})", target.label(), target.endpoint());
    println!(
        "  status     {status} · {mode}{}",
        offering["mode"]["runtime_kind"]
            .as_str()
            .map(|k| format!(" · {k}"))
            .unwrap_or_default()
    );
    println!(
        "  identity   category {}{}",
        offering["identity"]["category"].as_str().unwrap_or("?"),
        offering["identity"]["offering_id"]
            .as_str()
            .map(|id| format!(" · id {id}"))
            .unwrap_or_default()
    );
    if let Some(image) = offering["mode"]["spec"]["image"].as_str() {
        let restart = offering["mode"]["spec"]["restart"]
            .as_str()
            .unwrap_or("unless-stopped");
        println!("  spec       image {image} · restart {restart}");
        let named = offering["mode"]["spec"]["named_ports"].clone();
        let mapped = offering["mode"]["port_map"].clone();
        if named.is_object() {
            println!("  ports      {}", describe_ports(&named, &mapped));
        }
    } else {
        println!("  spec       ({mode}; no workload container)");
    }

    // The living will, surfaced honestly (L3: never silent about gaps).
    if let Some(capture) = data.get("capture") {
        let readiness = capture["readiness"].as_str().unwrap_or("?");
        match readiness {
            "trusted" => {
                let mode = capture["mode"].as_str().unwrap_or("?");
                println!("  capture    {mode} (trusted)");
            }
            "untrusted" => println!(
                "  capture    UNTRUSTED - volumes exist but no will is declared;\n             raw copy would be a lie. Declare `capture:` in the manifest."
            ),
            _ => {}
        }
    }

    match offering["mode"].get("plan") {
        Some(plan) => {
            let decisions = plan["decisions"].as_array();
            println!(
                "  decisions  {} recorded{}",
                decisions.map(Vec::len).unwrap_or(0),
                plan["meta"]["facts_generation"]
                    .as_u64()
                    .map(|g| format!(" against facts generation {g}"))
                    .unwrap_or_default()
            );
            for d in decisions.into_iter().flatten() {
                let rule = d["rule"].as_str().unwrap_or("?");
                let chose = d["chose"].as_str().unwrap_or("?");
                let because = d["because"].as_str().unwrap_or("");
                let source = d["source"]
                    .as_str()
                    .map(|s| format!(" [{s}]"))
                    .unwrap_or_default();
                println!("    · {rule}: {chose} — {because}{source}");
            }
            if let Some(hash) = plan["meta"]["plan_hash"].as_u64() {
                println!("  plan       hash {:016x}", hash);
            }
        }
        None => {
            println!("  plan       none — placed ad hoc, without a catalog manifest");
        }
    }
    Ok(())
}

/// rest/wake speak in the past tense with the resulting status as truth.
fn render_status(verb_past: &str, data: &serde_json::Value) -> Result<(), String> {
    let name = display_name(data["name"].as_str().unwrap_or("(unnamed)"));
    let status = data["status"].as_str().unwrap_or("?");
    println!("{name} {verb_past} — {status}");
    Ok(())
}

fn print_row(stone: &str, health: &str, address: &str, offerings: &str, version: &str) {
    println!("{:<26} {:<12} {:<21} {:<10} {}", stone, health, address, offerings, version);
}

/// What the stone claims to host: the declared total when the song was
/// capped, the visible items otherwise, silence when nothing is said.
fn offerings_count(s: &GardenStone) -> String {
    s.body
        .inventory
        .services
        .as_ref()
        .map(|svc| {
            svc.total
                .map(|t| t.to_string())
                .unwrap_or_else(|| svc.items.len().to_string())
        })
        .unwrap_or_else(|| "-".into())
}

fn print_table(stones: &[GardenStone]) {
    if stones.is_empty() {
        println!("The garden is quiet - the moss sees nobody.");
        return;
    }
    print_row("STONE", "HEALTH", "ADDRESS", "OFFERINGS", "VERSION");
    for s in stones {
        let name = if s.is_self {
            format!("{} (me)", s.body.stone.name)
        } else {
            s.body.stone.name.clone()
        };
        print_row(
            &name,
            &s.body.presence.health,
            &format!("{}:{}", s.body.stone.network.address.ip, s.body.stone.network.address.port),
            &offerings_count(s),
            &s.body.stone.moss.version,
        );
    }
}

/// `ip:port` literal → parts; names and bare ports are not endpoints.
fn parse_ip_port(s: &str) -> Option<(IpAddr, u16)> {
    let (ip, port) = s.rsplit_once(':')?;
    let ip = ip.parse::<IpAddr>().ok()?;
    let port = port.parse().ok()?;
    Some((ip, port))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn pinned() -> Candidate {
        Candidate {
            ip: Ipv4Addr::LOCALHOST.into(),
            http_port: 7285,
            name_hint: Some("stone-test".into()),
            origin: Origin::Flag,
        }
    }

    #[test]
    fn parses_well_formed_ports() {
        let got = parse_u16_pairs(&["default=6379".to_string(), "ui=8080".to_string()])
            .unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got["default"], 6379);
    }

    #[test]
    fn malformed_port_names_the_flag_back() {
        let err = parse_u16_pairs(&["default".to_string()]).unwrap_err();
        assert!(err.contains("--port"), "{err}");
        let err = parse_u16_pairs(&["default=http".to_string()]).unwrap_err();
        assert!(err.contains("NAME=NUMBER"), "{err}");
    }

    #[test]
    fn input_values_may_contain_equals() {
        let got = parse_input_map(&["password=a=b".to_string()]).unwrap();
        assert_eq!(got["password"], "a=b");
        assert!(parse_input_map(&["noequals".to_string()]).is_err());
    }

    #[test]
    fn envelope_errors_name_the_missing_thing() {
        let err = envelope(&serde_json::json!({"data":{}}), "offering").unwrap_err();
        assert_eq!(err, "response lacked data.offering");
        let err = envelope_plain(&serde_json::json!({})).unwrap_err();
        assert!(err.contains("data"));
    }

    /// The wire encoder: safe bytes ride raw, everything unsafe escapes —
    /// including `/` (which would otherwise nest the path on the bank)
    /// and the `::` of an FQN.
    #[test]
    fn path_encoding_escapes_everything_unsafe() {
        assert_eq!(paths::encode_segment("seed-vault::default"), "seed-vault%3A%3Adefault");
        assert_eq!(paths::encode_segment("dumps/notes.txt"), "dumps%2Fnotes.txt");
        assert_eq!(paths::encode_segment("my file#1.txt"), "my%20file%231.txt");
        assert_eq!(paths::encode_segment("plain-._~name"), "plain-._~name");

        let face = paths::storage_file("seed-vault::default", "dumps/notes.txt");
        assert_eq!(face, "/api/v1/storage/seed-vault%3A%3Adefault/files/dumps%2Fnotes.txt");
        assert_eq!(
            paths::storage_files("seed-vault"),
            "/api/v1/storage/seed-vault/files",
            "a bare stem has nothing to escape"
        );
    }

    /// The listing renders one row per entry; sparse rows cannot crash it.
    #[test]
    fn bank_files_render_tolerates_the_sparse() {
        render_bank_files(&serde_json::json!({
            "bank": "seed-vault::default",
            "path": "",
            "files": [
                { "name": "dumps", "kind": "dir", "size_bytes": null, "modified_at": "2026-08-28T10:00:00Z" },
                { "name": "notes.txt", "kind": "file", "size_bytes": 10 }
            ]
        }))
        .unwrap();
        // The empty and malformed answers render too (L3: never a crash).
        render_bank_files(&serde_json::json!({ "files": [] })).unwrap();
        render_bank_files(&serde_json::json!({})).unwrap();
    }

    /// The raw refusal: envelope messages surface verbatim, foreign
    /// bodies degrade to the status (L21 — rake speaks moss's refusals).
    #[test]
    fn raw_refusals_speak_the_moss_envelope() {
        let body = br#"{"error":{"message":"nothing answers at 'x' on this bank"}}"#;
        assert_eq!(
            raw_refusal(404, body),
            "nothing answers at 'x' on this bank"
        );
        assert_eq!(raw_refusal(502, b"<html>gateway</html>"), "HTTP 502");
    }

    /// The garden's only true redirect, client-side: a 404 that says
    /// `not_here` names the holder, anything else is the answer, and the
    /// way parses into an endpoint.
    #[test]
    fn not_here_answers_are_recognized_and_the_way_parses() {
        let body = br#"{"error":{"not_here":true,"bank":"seed-vault::default","knows_at":"http://192.168.1.50:7285/api/v1/stone","message":"That bank does not grow here."}}"#;
        let home = not_here_home(404, body).unwrap();
        assert_eq!(home, "http://192.168.1.50:7285/api/v1/stone");

        assert_eq!(not_here_home(200, body), None, "only a 404 redirects");
        assert_eq!(
            not_here_home(404, br#"{"error":{"message":"a plain miss"}}"#),
            None,
            "a plain 404 is not a redirect"
        );

        let (ip, port) = parse_http_home(&home).unwrap();
        assert_eq!(ip.to_string(), "192.168.1.50");
        assert_eq!(port, 7285);
        assert!(parse_http_home("ftp://192.168.1.50:7285/stone").is_err());
        assert!(parse_http_home("http://unreadably-broken").is_err());
    }

    /// The follow itself over a real socket: the re-request speaks HTTP
    /// to the holder and the bytes ride home.
    #[tokio::test]
    async fn reissue_speaks_to_the_home_stone() {
        use std::net::Ipv4Addr;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let n = sock.read(&mut buf).await.unwrap();
            let got = String::from_utf8_lossy(&buf[..n]).into_owned();
            sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello")
                .await
                .unwrap();
            got
        });
        let home = format!("http://127.0.0.1:{port}/api/v1/stone");
        let (status, bytes) = reissue_at_home(
            "GET",
            &home,
            "/api/v1/storage/seed-vault%3A%3Adefault/files/x.txt",
            None,
        )
        .await
        .unwrap();
        let seen = server.await.unwrap();
        assert_eq!(status, 200);
        assert_eq!(bytes, b"hello");
        assert!(
            seen.starts_with("GET /api/v1/storage/seed-vault%3A%3Adefault/files/x.txt HTTP/1.1"),
            "the wire path arrives as spelled: {seen}"
        );
    }

    #[test]
    fn garden_parse_accepts_the_canonical_frame() {
        let v = serde_json::json!({
            "data": { "stones": [ {
                "stone": {
                    "id": "0198e0c7-0000-7000-8000-000000000001",
                    "name": "stone-a",
                    "moss": { "version": "0.1.0" },
                    "network": { "address": { "ip": "192.168.1.9", "port": 7285 } }
                },
                "presence": { "health": "thriving", "status": "online" },
                "services": { "rev": 1, "items": [] },
                "meta": { "proto": "zg/1", "seq": 7 },
                "received": {
                    "discovered_at": "2026-08-26T00:00:00Z",
                    "last_seen": "2026-08-26T00:00:00Z"
                },
                "chirps": 3
            } ] }
        });
        let stones = parse_garden(&v).unwrap();
        assert_eq!(stones[0].body.stone.name, "stone-a");
        assert_eq!(stones[0].body.stone.network.address.port, 7285);
        assert_eq!(stones[0].chirps, Some(3));
        assert!(!stones[0].is_self);
    }

    /// The spliced self row (ADR-0004 §3): `"self": true`, no chirp count,
    /// same canonical frame. Rake marks it and renders it among peers.
    #[test]
    fn garden_parse_accepts_the_self_splice() {
        let v = serde_json::json!({
            "data": { "stones": [ {
                "self": true,
                "stone": {
                    "id": "0198e0c7-0000-7000-8000-000000000001",
                    "name": "stone-a",
                    "moss": { "version": "1.0.0" },
                    "network": { "address": { "ip": "192.168.1.9", "port": 7285 } }
                },
                "presence": { "health": "thriving", "status": "online" },
                "inventory": { "services": { "rev": 4, "items": [] } },
                "meta": { "proto": "zg/1" },
                "received": {
                    "discovered_at": "2026-08-26T00:00:00Z",
                    "last_seen": "2026-08-26T00:00:00Z"
                }
            } ] }
        });
        let stones = parse_garden(&v).unwrap();
        assert!(stones[0].is_self, "the splice is marked");
        assert_eq!(stones[0].chirps, None, "self does not count chirps");
        assert_eq!(stones[0].body.inventory.services.as_ref().and_then(|s| s.rev), Some(4));
    }

    /// §5.3's promise: explain renders the decision log from the plan the
    /// record carries — same document reality was built from.
    #[test]
    fn explain_renders_plan_decisions_and_hash() {
        let offering = serde_json::json!({
            "identity": { "offering_id": "id-1", "name": "redis::default",
                          "stem": "redis", "category": "data" },
            "state": { "status": "running" },
            "location": { "host": "localhost", "port": 63001, "protocol": "http" },
            "mode": { "mode": "managed", "runtime_kind": "oci",
                      "spec": { "image": "redis:7-alpine", "restart": "unless-stopped",
                                "named_ports": { "default": 6379 } },
                      "port_map": { "default": 63001 },
                      "plan": {
                          "workload": {},
                          "decisions": [ { "rule": "arch-x86", "chose": "place",
                                           "because": "x86_64 fits", "source": "manifest" } ],
                          "meta": { "facts_generation": 3,
                                    "plan_hash": 5709041973811721503_u64 }
                      } },
            "registered_at": "2026-08-26T00:00:00Z",
            "updated_at": "2026-08-26T00:00:00Z"
        });
        render_explain(&pinned(), offering, &serde_json::json!({})).unwrap();
    }

    #[test]
    fn renderers_survive_sparse_records() {
        // Sparse/foreign shapes must degrade gracefully, never panic.
        render_offered(&pinned(), serde_json::json!({ "identity": { "name": "weird" } })).unwrap();
        render_explain(
            &pinned(),
            serde_json::json!({ "mode": "adopted", "plan": { "decisions": [ {"rule": "r"} ] } }),
            &serde_json::json!({}),
        )
        .unwrap();
        render_status("awake", &serde_json::json!({})).unwrap();
        render_garden_storage(&serde_json::json!({})).unwrap();
        render_garden_storage(&serde_json::json!({ "banks": [
            { "stone": "stone-a", "self": true,
              "bank": { "fqn": "seed-vault::default", "state": "mounted",
                        "capacity_bytes": 5709041973811721503_u64 } }
        ] }))
        .unwrap();
        assert!(describe_ports(&serde_json::json!({}), &serde_json::json!({})).is_empty());
    }

    #[test]
    fn named_pairs_render_in_human_form() {
        let s = named_pairs(&serde_json::json!({"default": 63001}), ", ");
        assert_eq!(s, "default → 63001");
    }
}

