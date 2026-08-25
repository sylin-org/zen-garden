//! `moss` — the resident service of a Zen Garden stone. M0: presence only.
//!
//! Moss is the quiet green layer on every stone: it announces, listens,
//! and answers. Humans and agents walk the garden with `rake`.
//!
//! Startup is the typed pipeline (R0.4/L17): config -> ingress bind ->
//! dispatch -> topology claim -> expiry sweep → announcer → HTTP. Every
//! step's output feeds the next; failure aborts loudly by step name.
//! Shutdown speaks goodbye before the light goes out.

mod http;
mod identity;
mod source;

use clap::Parser;
use garden_kernel::announce::{self, ChirpSource};
use garden_kernel::config::{DiscoveryConfig, HttpConfig};
use garden_kernel::dispatch::Dispatcher;
use garden_kernel::ingress::Ingress;
use garden_kernel::pipeline;
use garden_kernel::topology::Topology;
use garden_kernel::responder;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Capacity of the queue between ingress and dispatch (datagrams).
const INGRESS_QUEUE: usize = 1024;
/// Grace period after cancellation for in-flight sends before goodbye.
const SHUTDOWN_GRACE_MS: u64 = 200;
#[derive(Parser)]
#[command(
    name = "moss",
    about = "A Zen Garden stone's resident service: announces itself, observes its peers.",
    version
)]
struct Cli {
    /// Stone name. Absent: a poetical name is minted on first boot and kept
    /// forever. Present: operator rename intent (the stone_id never changes).
    #[arg(long, env = "MOSS_STONE_NAME")]
    stone_name: Option<String>,

    /// HTTP port for this stone's surface.
    #[arg(long, env = "MOSS_HTTP_PORT", default_value_t = HttpConfig::DEFAULT_PORT)]
    http_port: u16,

    /// Discovery UDP port override (default is the v1 room).
    #[arg(long, env = "MOSS_DISCOVERY_PORT")]
    discovery_port: Option<u16>,

    /// Multicast group override (default is the v1 room).
    #[arg(long, env = "MOSS_MCAST_GROUP")]
    mcast_group: Option<Ipv4Addr>,
}

impl Cli {
    /// CLI > env > defaults (R3.7): v1 topology by default, then env twins,
    /// then CLI overrides. All deployment config lives here, at the binary —
    /// the kernel ships pure defaults only.
    fn discovery_config(&self) -> DiscoveryConfig {
        let mut cfg = DiscoveryConfig::default();
        if let Some(p) = self.discovery_port {
            cfg.port = p;
        }
        if let Some(g) = self.mcast_group {
            cfg.group = g;
        }
        cfg
    }
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[tokio::main]
async fn main() {
    init_tracing();
    let cli = Cli::parse();
    let token = CancellationToken::new();
    let boot_id = Uuid::now_v7();

    // ---- startup pipeline: build-up is sacred (R0.4, L17) -----------------
    let discovery =
        pipeline::step("config", async { Ok::<_, String>(cli.discovery_config()) }).await;
    let http_port = cli.http_port;

    // Identity: minted once, persistent and immutable (D6); poetic default
    // name collision-checked against the room; explicit flag = rename intent.
    let identity = pipeline::step("identity", {
        let discovery = discovery.clone();
        async move { identity::load_or_mint(cli.stone_name.as_deref(), &discovery).await }
    })
    .await;

    let chirp_body = source::static_body(
        identity.stone_id.clone(),
        identity.stone_name.clone(),
        boot_id.to_string(),
        http_port,
        env!("CARGO_PKG_VERSION").to_string(),
    );
    let chirp_source = source::StaticChirpSource::new(chirp_body.clone());

    let ingress = pipeline::step("ingress-bind", {
        let discovery = discovery.clone();
        async move { Ingress::bind(&discovery, Some(discovery.group)).await }
    })
    .await;

    let (dispatcher, dispatcher_handle) = Dispatcher::new(INGRESS_QUEUE);
    let topology = Arc::new(Topology::new());

    // The topology cache claims its types from the dispatcher (R2.9, L22); expiry sweeps
    // on protocol time (the threshold IS the protocol — R2.8).
    topology.claim(&dispatcher, token.clone());
    tokio::spawn(Arc::clone(&topology).run_expiry(
        token.clone(),
        discovery.offline_threshold_secs,
    ));

    // The announcer speaks through the same bound port number: boot chirp,
    // then ask the room who's here; heartbeats and change-chirps after.
    let announce_socket = ingress.socket();
    tokio::spawn(announce::run(
        Arc::clone(&announce_socket),
        discovery.group,
        discovery.port,
        chirp_source.clone() as Arc<dyn ChirpSource>,
        identity.stone_name.clone(),
        token.clone(),
    ));

    // Answer others' asks — tell half of ask/tell.
    responder::claim(
        &dispatcher,
        ingress.socket(),
        discovery.group,
        discovery.port,
        chirp_source.clone() as Arc<dyn ChirpSource>,
        token.clone(),
    );

    // Ingestion and dispatch run until cancelled. The counters handle is
    // cloned before the move so posture can serve live numbers (D7).
    let ingest_counters = ingress.counters();
    let dispatch_tx = dispatcher.ingest_tx();
    let ingest_token = token.clone();
    tokio::spawn(async move { ingress.run(ingest_token, dispatch_tx).await });
    tokio::spawn(dispatcher_handle.run(token.clone()));

    // HTTP surface, last: the garden answers once it can hear.
    let state = Arc::new(http::AppState {
        topology: Arc::clone(&topology),
        dispatcher: dispatcher.clone(),
        ingest_counters,
        stone_name: identity.stone_name.clone(),
        boot_id,
        started_at: chrono::Utc::now(),
    });
    let app = http::router(state);
    let listener = pipeline::step("http-listen", async move {
        tokio::net::TcpListener::bind((Ipv4Addr::UNSPECIFIED, http_port))
            .await
            .map_err(|e| format!("bind 0.0.0.0:{http_port}: {e}"))
    })
    .await;

    tracing::info!(
        stone = %identity.stone_name,
        group = %discovery.group,
        discovery_port = discovery.port,
        http_port,
        "moss awake"
    );

    // ---- run until signalled ----------------------------------------------
    let shutdown_token = token.clone();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            match tokio::signal::ctrl_c().await {
                Ok(()) => tracing::info!("shutdown signal received"),
                Err(e) => {
                    // No signal channel: graceful shutdown becomes impossible.
                    // Refuse to fake it by cancelling at once; park instead.
                    tracing::error!(error = %e, "signal listener failed; kill to stop");
                    std::future::pending::<()>().await;
                }
            }
            shutdown_token.cancel();
        })
        .await
        .unwrap_or_else(|e| eprintln!("http server error: {e}"));

    // ---- farewell ----------------------------------------------------------
    // Let cancelled tasks wind down, then speak goodbye so peers drop us
    // without waiting out the threshold (PoC parity: 3 copies, debounce-free).
    tokio::time::sleep(Duration::from_millis(SHUTDOWN_GRACE_MS)).await;
    let final_body = chirp_source.body();
    if let Err(e) =
        announce::send_goodbye(&announce_socket, discovery.group, discovery.port, final_body).await
    {
        tracing::warn!(error = %e, "goodbye send failed");
    }
    tracing::info!("goodbye spoken; garden rests");
}

