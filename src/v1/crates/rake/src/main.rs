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
use garden_contract::chirp::ChirpBody;
use garden_contract::consts;
use garden_kernel::probe;
use moss_http::AttachError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
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
#[derive(Debug, Clone, Deserialize, Serialize)]
struct GardenStone {
    #[serde(flatten)]
    body: ChirpBody,
    chirps: u64,
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
                    ip: s.address.ip,
                    http_port: s.address.port,
                    name_hint: Some(s.stone_name),
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
                        moss_http::get_json(cand.ip, cand.http_port, "/api/v1/garden/observe", HTTP_TIMEOUT)
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
}

// ---------------------------------------------------------------------------
// Command dispatch and rendering
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(msg) = run(&cli).await {
        eprintln!("rake: {msg}");
        std::process::exit(1);
    }
}

async fn run(cli: &Cli) -> Result<(), String> {
    match &cli.command {
        Command::Observe => {
            let stones = cli.garden_view().await?;
            if cli.json {
                let arr = serde_json::to_value(&stones)
                    .map_err(|e| format!("could not render json: {e}"))?;
                emit_pretty(&arr)
            } else {
                print_table(&stones);
                Ok(())
            }
        }
        Command::Find { pattern } => {
            let needle = pattern.to_lowercase();
            let mut stones = cli.garden_view().await?;
            stones.retain(|s| s.body.stone_name.to_lowercase().contains(&needle));
            if cli.json {
                let arr = serde_json::to_value(&stones)
                    .map_err(|e| format!("could not render json: {e}"))?;
                emit_pretty(&arr)
            } else {
                print_table(&stones);
                Ok(())
            }
        }
        Command::Offer { .. } | Command::Explain { .. } | Command::Rest { .. }
        | Command::Wake { .. } | Command::Uproot { .. } => cmd_stone_op(cli).await,
    }
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
                return emit_pretty(&v);
            }
            render_offered(&target, envelope(&v, "offering")?)
        }
        Command::Explain { name } => {
            let (target, v) = cli.stone_op("GET", paths::record(name), None).await?;
            if cli.json {
                return emit_pretty(&v);
            }
            render_explain(&target, envelope(&v, "offering")?)
        }
        Command::Rest { name } => {
            let (_, v) = cli.stone_op("POST", paths::rest(name), None).await?;
            if cli.json {
                return emit_pretty(&v);
            }
            render_status("rested", &envelope_plain(&v)?)
        }
        Command::Wake { name } => {
            let (_, v) = cli.stone_op("POST", paths::wake(name), None).await?;
            if cli.json {
                return emit_pretty(&v);
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
                emit_pretty(&v)
            } else {
                println!("{name} uprooted");
                Ok(())
            }
        }
        _ => unreachable!("dispatch routes only stone ops here"),
    }
}

/// Request paths with their commands (R2.2): built where used, kept nowhere.
mod paths {
    pub const OFFERINGS: &str = "/api/v1/stone/offerings";

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
        .find(|s| s.body.address.ip == cand.ip && s.body.address.port == cand.http_port)
        .map(|s| s.body.stone_name.clone())
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

/// The moment after planting: what grew, where, listening on what.
fn render_offered(target: &Candidate, offering: serde_json::Value) -> Result<(), String> {
    let name = offering["name"].as_str().unwrap_or("(unnamed)");
    let status = offering["status"].as_str().unwrap_or("?");
    println!("{name} planted — {status}");
    println!("  on      {} ({})", target.label(), target.origin_label());
    if let Some(image) = offering["spec"]["image"].as_str() {
        println!("  image   {image}");
    }
    let container_ports = offering["spec"]["named_ports"].clone();
    let host_ports = offering.get("port_map").cloned().unwrap_or(serde_json::Value::Null);
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
fn render_explain(target: &Candidate, offering: serde_json::Value) -> Result<(), String> {
    let name = offering["name"].as_str().unwrap_or("(unnamed)");
    let status = offering["status"].as_str().unwrap_or("?");
    let mode = offering["mode"].as_str().unwrap_or("?");

    println!("{name} — {} ({})", target.label(), target.endpoint());
    println!(
        "  status     {status} · {mode}{}",
        offering["runtime_kind"]
            .as_str()
            .map(|k| format!(" · {k}"))
            .unwrap_or_default()
    );
    println!(
        "  identity   category {}{}",
        offering["category"].as_str().unwrap_or("?"),
        offering["offering_id"]
            .as_str()
            .map(|id| format!(" · id {id}"))
            .unwrap_or_default()
    );
    if let Some(image) = offering["spec"]["image"].as_str() {
        let restart = offering["spec"]["restart"].as_str().unwrap_or("unless-stopped");
        println!("  spec       image {image} · restart {restart}");
        let named = offering["spec"]["named_ports"].clone();
        let mapped = offering["port_map"].clone();
        if named.is_object() {
            println!("  ports      {}", describe_ports(&named, &mapped));
        }
    } else {
        println!("  spec       ({mode}; no workload container)");
    }

    match offering.get("plan") {
        Some(plan) => {
            let decisions = plan["decisions"].as_array();
            println!(
                "  decisions  {} recorded{}",
                decisions.map(Vec::len).unwrap_or(0),
                plan["facts_generation"]
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
            if let Some(hash) = plan["plan_hash"].as_u64() {
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
    let name = data["name"].as_str().unwrap_or("(unnamed)");
    let status = data["status"].as_str().unwrap_or("?");
    println!("{name} {verb_past} — {status}");
    Ok(())
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

    #[test]
    fn garden_parse_accepts_flattened_standard_format() {
        let v = serde_json::json!({
            "data": { "stones": [ {
                "stone_id": "0198e0c7-0000-7000-8000-000000000001",
                "stone_name": "stone-a",
                "address": { "ip": "192.168.1.9", "port": 7285, "tls_port": null },
                "moss_version": "0.1.0",
                "services": [],
                "health": "thriving",
                "status": "online",
                "discovered_at": "2026-08-26T00:00:00Z",
                "last_seen": "2026-08-26T00:00:00Z",
                "chirps": 3
            } ] }
        });
        let stones = parse_garden(&v).unwrap();
        assert_eq!(stones[0].body.stone_name, "stone-a");
        assert_eq!(stones[0].chirps, 3);
    }

    /// §5.3's promise: explain renders the decision log from the plan the
    /// record carries — same document reality was built from.
    #[test]
    fn explain_renders_plan_decisions_and_hash() {
        let offering = serde_json::json!({
            "name": "redis", "status": "running", "mode": "managed",
            "category": "data", "offering_id": "id-1", "runtime_kind": "oci",
            "spec": { "image": "redis:7-alpine", "restart": "unless-stopped",
                       "named_ports": { "default": 6379 } },
            "port_map": { "default": 63001 },
            "plan": {
                "facts_generation": 3,
                "plan_hash": 5709041973811721503_u64,
                "decisions": [ { "rule": "arch-x86", "chose": "place",
                                  "because": "x86_64 fits", "source": "manifest" } ]
            }
        });
        render_explain(&pinned(), offering).unwrap();
    }

    #[test]
    fn renderers_survive_sparse_records() {
        // Sparse/foreign shapes must degrade gracefully, never panic.
        render_offered(&pinned(), serde_json::json!({ "name": "weird" })).unwrap();
        render_explain(
            &pinned(),
            serde_json::json!({ "mode": "adopted", "plan": { "decisions": [ {"rule": "r"} ] } }),
        )
        .unwrap();
        render_status("awake", &serde_json::json!({})).unwrap();
        assert!(describe_ports(&serde_json::json!({}), &serde_json::json!({})).is_empty());
    }

    #[test]
    fn named_pairs_render_in_human_form() {
        let s = named_pairs(&serde_json::json!({"default": 63001}), ", ");
        assert_eq!(s, "default → 63001");
    }
}
