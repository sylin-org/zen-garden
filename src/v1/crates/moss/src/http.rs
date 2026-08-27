//! The HTTP surface: observe the garden, describe thyself, tend offerings
//! (L1, L7, L22). Handlers are thin: parse → delegate to the application
//! service or the kernel aggregates → envelope. No domain logic lives here.
//!
//! Grammar (ADR-0004 §4): bare nouns name THIS stone's domain resources ·
//! `/garden/*` projects the room read-only · deeper paths hang off nouns.
//! The surface is declared ONCE — [`Face`] is the manifest, and the router
//! is built from it, so an unadvertised emission is structurally
//! impossible and an unrouted claim fails the manifest gates (L9, R4.7).

use crate::offerings::service::{CommandError, OfferingService};
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use garden_contract::consts::PROTO_V1;
use garden_kernel::announce::ChirpSource;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// Shared state behind the routes.
pub struct AppState {
    /// The offering application service (domain + worlds coordinated).
    pub garden: Arc<OfferingService>,
    pub topology: Arc<garden_kernel::topology::Topology>,
    pub dispatcher: Dispatcher,
    pub ingest_counters: Arc<IngestCounters>,
    /// This stone's voice — the SelfView's composer (self is a projection,
    /// never a stored peer; ADR-0004 §3).
    pub chirp_source: Arc<dyn ChirpSource>,
    pub stone_name: String,
    pub boot_id: Uuid,
    pub started_at: chrono::DateTime<chrono::Utc>,
}

use garden_kernel::dispatch::Dispatcher;
use garden_kernel::ingress::IngestCounters;

/// The surface, declared once (L9, R4.7): routes exist ONLY as rows of
/// [`Face::ALL`]. Adding a face means adding a variant — the compiler then
/// demands its method, path, summary, and wiring; removing one leaves
/// nowhere for a stale row to hide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Face {
    Health,
    /// The front door: this route table (ADR-0004 §4 — kills the
    /// `/manifest` offering-name collision).
    FrontDoor,
    /// Me: the SelfView.
    StoneSelf,
    /// Me, spelled explicitly.
    StoneThis,
    /// Any stone by name or id: mine answered, others redirected home.
    StoneRef,
    StonePosture,
    GardenStones,
    Catalog,
    OfferingPlant,
    OfferingShow,
    OfferingRest,
    OfferingWake,
    OfferingUproot,
}

impl Face {
    const ALL: [Face; 13] = [
        Face::Health,
        Face::FrontDoor,
        Face::StoneSelf,
        Face::StoneThis,
        Face::StoneRef,
        Face::StonePosture,
        Face::GardenStones,
        Face::Catalog,
        Face::OfferingPlant,
        Face::OfferingShow,
        Face::OfferingRest,
        Face::OfferingWake,
        Face::OfferingUproot,
    ];

    fn method(self) -> &'static str {
        match self {
            Face::Health
            | Face::FrontDoor
            | Face::StoneSelf
            | Face::StoneThis
            | Face::StoneRef
            | Face::StonePosture
            | Face::GardenStones
            | Face::Catalog
            | Face::OfferingShow => "GET",
            Face::OfferingPlant | Face::OfferingRest | Face::OfferingWake => "POST",
            Face::OfferingUproot => "DELETE",
        }
    }

    fn path(self) -> &'static str {
        match self {
            Face::Health => "/health",
            Face::FrontDoor => "/api/v1",
            Face::StoneSelf => "/api/v1/stone",
            Face::StoneThis => "/api/v1/stone/this",
            Face::StoneRef => "/api/v1/stone/{ref}",
            Face::StonePosture => "/api/v1/stone/posture",
            Face::GardenStones => "/api/v1/garden/stones",
            Face::Catalog => "/api/v1/catalog",
            Face::OfferingPlant | Face::OfferingShow | Face::OfferingUproot => {
                "/api/v1/offerings/{fqn}"
            }
            Face::OfferingRest => "/api/v1/offerings/{fqn}/rest",
            Face::OfferingWake => "/api/v1/offerings/{fqn}/wake",
        }
    }

    fn summary(self) -> &'static str {
        match self {
            Face::Health => "Liveness probe of this stone and its wire protocol marker.",
            Face::FrontDoor => "This route table - every surface, described in place.",
            Face::StoneSelf => "Me: my frame, sung full-voice (the SelfView projection).",
            Face::StoneThis => "Me, spelled explicitly (same SelfView).",
            Face::StoneRef => {
                "A stone by name or id: mine answered here; others answer 404 with a \
                 Location to their home stone (the garden's only true redirect)."
            }
            Face::StonePosture => {
                "Local data (L22): this moss's live counters - ingest, dispatch, \
                 topology, offerings."
            }
            Face::GardenStones => {
                "Garden data (L22): the room as this moss sees it - self spliced \
                 among the peers, every row a canonical frame."
            }
            Face::Catalog => "The catalog this stone can place from (derived).",
            Face::OfferingPlant => {
                "Plant a managed offering {image?, ports:{name:container}, runtime?, \
                 inputs?}; catalog name wins when one exists."
            }
            Face::OfferingShow => "The placed record - plan, decisions, ports (OFFERINGS.md §5.3).",
            Face::OfferingRest => {
                "Rest a managed offering - stopped, and reconcile will keep it so."
            }
            Face::OfferingWake => {
                "Wake a rested offering; resurrects from its stored spec if reality lost it."
            }
            Face::OfferingUproot => "Uproot - remove the workload and forget the offering.",
        }
    }

    fn method_router(self) -> axum::routing::MethodRouter<Arc<AppState>> {
        match self {
            Face::Health => get(health),
            Face::FrontDoor => get(front_door),
            Face::StoneSelf | Face::StoneThis => get(stone_self),
            Face::StoneRef => get(stone_ref),
            Face::StonePosture => get(posture),
            Face::GardenStones => get(garden_stones),
            Face::Catalog => get(catalog),
            Face::OfferingPlant => post(plant_offering),
            Face::OfferingShow => get(show_offering),
            Face::OfferingRest => post(rest_offering),
            Face::OfferingWake => post(wake_offering),
            Face::OfferingUproot => axum::routing::delete(uproot_offering),
        }
    }
}

async fn health(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let uptime = chrono::Utc::now() - state.started_at;
    Json(serde_json::json!({
        "data": {
            "ok": true,
            "asset": "moss",
            "proto": PROTO_V1,
            "stone_name": state.stone_name,
            "boot_id": state.boot_id,
            "uptime_secs": uptime.num_seconds(),
        }
    }))
}

async fn posture(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let uptime = chrono::Utc::now() - state.started_at;
    let dispatch = state.dispatcher.stats();
    Json(serde_json::json!({
        "data": {
            "asset": "moss",
            "stone_name": state.stone_name,
            "boot_id": state.boot_id,
            "uptime_secs": uptime.num_seconds(),
            "ingest": {
                "parsed": state.ingest_counters.parsed(),
                "bad_json": state.ingest_counters.bad_json(),
                "deduped": state.ingest_counters.deduped(),
            },
            "dispatch": {
                "delivered": dispatch.delivered,
                "dropped": dispatch.dropped,
                "unclaimed": dispatch.unclaimed,
            },
            "topology": {
                "stones": state.topology.snapshot().len(),
                "candidates": state.topology.candidates().len(),
                "chirps_total": state.topology.chirps_total(),
            },
            "offerings": {
                "active": state.garden.counts().active,
                "candidates": state.garden.counts().candidates,
                "catalog": state.garden.catalog_size(),
            },
            "runtimes": state.garden.available_worlds(),
        }
    }))
}

/// The SelfView (ADR-0004 §3): self is rebuilt, never stored. The stone's
/// own frame, re-voiced with its full inventory — one composer, many
/// mouths (B1).
fn self_view(state: &AppState) -> serde_json::Value {
    let mut body = state.chirp_source.body();
    body.inventory =
        garden_contract::chirp::InventoryMap::from_pairs(state.chirp_source.song_blocks());
    serde_json::to_value(&body).unwrap_or_default()
}

async fn stone_self(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "data": self_view(&state) }))
}

/// `/stone/{ref}` — the garden's only true redirect (ADR-0004 §4): mine
/// answered here; a peer's is a not-here answer carrying its home address
/// (Location header + `knows_at`), because reads delegate and writes bind
/// at their authority. Unknown names are a plain 404.
async fn stone_ref(State(state): State<Arc<AppState>>, Path(reference): Path<String>) -> Response {
    let my_frame = state.chirp_source.body();
    if reference == state.stone_name || reference == my_frame.stone.id {
        return Json(serde_json::json!({ "data": self_view(&state) })).into_response();
    }
    if let Some(peer) = state.topology.find(&reference) {
        let name = peer.body.stone.name.clone();
        let address = peer.body.stone.network.address.clone();
        let knows_at = format!("http://{}:{}/api/v1/stone", address.ip, address.port);
        return (
            axum::http::StatusCode::NOT_FOUND,
            [(axum::http::header::LOCATION, knows_at.clone())],
            Json(serde_json::json!({
                "error": {
                    "not_here": true,
                    "stone": name,
                    "knows_at": knows_at,
                    "message": "That stone does not grow here. Its home answers at \
                                `knows_at` - this stone only knows the way."
                }
            })),
        )
            .into_response();
    }
    (
        axum::http::StatusCode::NOT_FOUND,
        Json(serde_json::json!({
            "error": { "message": format!("No stone '{reference}' in this garden's ken.") }
        })),
    )
        .into_response()
}

async fn garden_stones(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    // Self spliced first — the room's projection obviously includes the
    // current stone (ADR-0004 §3).
    let mut self_row = self_view(&state);
    if let Some(obj) = self_row.as_object_mut() {
        obj.insert("self".into(), serde_json::json!(true));
    }
    let mut stones = vec![self_row];
    for peer in state.topology.snapshot() {
        let mut v = serde_json::to_value(&peer.body).unwrap_or_default();
        if let Some(obj) = v.as_object_mut() {
            obj.insert("chirps".into(), serde_json::json!(peer.chirps));
        }
        stones.push(v);
    }
    Json(serde_json::json!({ "data": { "stones": stones } }))
}

/// The derived catalog face: what this stone can place from.
async fn catalog(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let entries: Vec<serde_json::Value> = state
        .garden
        .catalog
        .names()
        .into_iter()
        .filter_map(|stem| {
            state.garden.catalog.get(&stem).map(|m| {
                serde_json::json!({
                    "stem": stem,
                    "category": m.category,
                    "description": m.description,
                })
            })
        })
        .collect();
    Json(serde_json::json!({ "data": { "catalog": entries } }))
}

async fn front_door() -> Json<serde_json::Value> {
    let routes: Vec<serde_json::Value> = Face::ALL
        .iter()
        .map(|face| {
            serde_json::json!({
                "method": face.method(),
                "path": face.path(),
                "summary": face.summary(),
            })
        })
        .collect();
    Json(serde_json::json!({ "data": { "routes": routes } }))
}

// ---- offerings (L22) — thin delegation to the application service ---------

type ApiResult = Result<Json<serde_json::Value>, ApiError>;

struct ApiError(CommandError);

impl From<CommandError> for ApiError {
    fn from(e: CommandError) -> Self {
        Self(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        use CommandError::*;
        let status = match self.0 {            NotFound(_) => axum::http::StatusCode::NOT_FOUND,
            Conflict(_) => axum::http::StatusCode::CONFLICT,
            WorldUnavailable(_) => axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Runtime(_) => axum::http::StatusCode::BAD_GATEWAY,
        };
        (
            status,
            Json(serde_json::json!({ "error": { "message": self.0.to_string() } })),
        )
            .into_response()
    }
}

#[derive(Debug, Deserialize)]
struct PlantRequest {
    /// Required for ad-hoc placement; absent when planting from catalog.
    image: Option<String>,
    /// Named ports: name → container port. Host mapping is the world's.
    #[serde(default)]
    ports: HashMap<String, u16>,
    #[serde(default = "default_category")]
    category: String,
    /// Which world to place into; absent = this host's default.
    #[serde(default)]
    runtime: Option<String>,
    /// Declared install form values (OFFERINGS.md §5.1 `inputs`).
    #[serde(default)]
    inputs: std::collections::BTreeMap<String, String>,
}

fn default_category() -> String {
    "misc".into()
}

async fn plant_offering(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<PlantRequest>,
) -> ApiResult {
    let offering = state
        .garden
        .offer(&name, req.image, req.ports, Some(req.category), req.runtime.as_deref(), &req.inputs)
        .await?;
    Ok(Json(
        serde_json::json!({ "data": { "offering": record_view(&offering) } }),
    ))
}

/// Offerings render the sectioned record — disk and HTTP speak one shape
/// (R3.9, B1; S5.5).
fn record_view(offering: &crate::offerings::model::Offering) -> serde_json::Value {
    serde_json::to_value(crate::offerings::record::OfferingRecord::from_domain(offering))
        .unwrap_or_default()
}

/// §5.3: the placed record with its plan attached. Off-grammar names
/// refuse loudly here too — a tag-shaped read is an identity question,
/// not a quiet miss.
async fn show_offering(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult {
    let fqn = garden_glossary::fqn::canonicalize(&name)
        .map_err(|e| CommandError::Conflict(e.to_string()))?;
    match state.garden.placed(&fqn) {
        Some(o) => Ok(Json(
            serde_json::json!({ "data": { "offering": record_view(&o) } }),
        )),
        None => Err(CommandError::NotFound(fqn).into()),
    }
}

async fn rest_offering(State(state): State<Arc<AppState>>, Path(name): Path<String>) -> ApiResult {
    let offering = state.garden.rest(&name).await?;
    Ok(Json(serde_json::json!({
        "data": { "name": offering.name, "status": offering.status.as_str() }
    })))
}

async fn wake_offering(State(state): State<Arc<AppState>>, Path(name): Path<String>) -> ApiResult {
    let offering = state.garden.wake(&name).await?;
    let port_map = offering.managed().map(|m| m.port_map.clone()).unwrap_or_default();
    Ok(Json(serde_json::json!({
        "data": { "name": offering.name, "status": offering.status.as_str(), "port_map": port_map }
    })))
}

async fn uproot_offering(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult {
    state.garden.uproot(&name).await?;
    Ok(Json(serde_json::json!({ "data": { "name": name, "uprooted": true } })))
}

/// The complete surface, built FROM the manifest (L9, R4.7): the router's
/// routes are exactly [`Face::ALL`] — nothing emits unadvertised, nothing
/// advertises unrouted.
pub fn router(state: Arc<AppState>) -> Router {
    let router = Face::ALL
        .iter()
        .fold(Router::new(), |r, face| r.route(face.path(), face.method_router()));
    router.with_state(state)
}

#[cfg(test)]
mod tests {
    // R4.1: unwrap/expect sanctioned in tests.
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::offerings::registry::{MemorySnapshotStore, Registry};
    use crate::offerings::runtime::{NullRuntime, RuntimeRegistry};
    use crate::source::{DynamicChirpSource, Voice};
    use axum::http::StatusCode;
    use garden_contract::chirp::ChirpFrame;
    use garden_kernel::topology::StoneView;
    use tower::ServiceExt;

    fn test_state() -> Arc<AppState> {
        let registry = Arc::new(Registry::new(Arc::new(MemorySnapshotStore::default())));
        let worlds = Arc::new(RuntimeRegistry::build(vec![Arc::new(NullRuntime)]));
        let factsheet = Arc::new(crate::offerings::facts::Factsheet::empty());
        let service = Arc::new(OfferingService::new(
            registry.clone(),
            worlds,
            "null".into(),
            Arc::new(crate::offerings::manifest::Catalog::default()),
            factsheet,
            crate::offerings::directory::OfferingsRoot::new(
                std::env::temp_dir().join(format!("moss-test-offer-{}", Uuid::now_v7())),
            ),
            crate::offerings::ports::Pool::default(),
        ));
        let chirp_source = DynamicChirpSource::new(
            Voice {
                stone_id: "0198e0c7-0000-7000-8000-000000000001".into(),
                stone_name: "stone-test".into(),
                http_port: 7285,
                moss_version: "1.0.0".into(),
            },
            "boot-test".into(),
            registry,
        );
        Arc::new(AppState {
            garden: service,
            topology: Arc::new(garden_kernel::topology::Topology::new()),
            dispatcher: Dispatcher::new(16).0,
            ingest_counters: Arc::new(IngestCounters::default()),
            chirp_source,
            stone_name: "stone-test".into(),
            boot_id: Uuid::now_v7(),
            started_at: chrono::Utc::now(),
        })
    }

    async fn send(app: &Router, method: &str, path: &str) -> axum::http::Response<axum::body::Body> {
        let req = match method {
            "GET" => axum::http::Request::builder().uri(path).body(axum::body::Body::empty()),
            _ => axum::http::Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .body(axum::body::Body::from("{}")),
        }
        .unwrap();
        app.clone().oneshot(req).await.unwrap()
    }

    /// L7: self-description is generated truth — every manifest face
    /// answers through the real router. Static GET faces answer 200; the
    /// redirect face answers 404 by design ({ref} is nobody in a bare
    /// test state); every face must at least be ROUTED, never a method
    /// miss.
    #[tokio::test]
    async fn every_manifest_face_answers() {
        let app = router(test_state());

        for face in Face::ALL {
            let res = send(&app, face.method(), face.path()).await;
            assert_ne!(
                res.status(),
                StatusCode::METHOD_NOT_ALLOWED,
                "{} {} must be routed",
                face.method(),
                face.path()
            );
            match face {
                Face::StoneRef => {
                    assert_eq!(res.status(), StatusCode::NOT_FOUND, "nobody by that ref here");
                }
                _ if face.method() == "GET" && !face.path().contains('{') => {
                    assert_eq!(
                        res.status(),
                        StatusCode::OK,
                        "{} {} must answer",
                        face.method(),
                        face.path()
                    );
                }
                _ => {}
            }
        }
    }

    /// The grammar cut is CLEAN (ADR-0004 §4): no legacy aliases. The old
    /// spellings are dead — unrouted (404) or method-less (405), never a
    /// 200 wearing an old name.
    #[tokio::test]
    async fn legacy_spellings_are_dead() {
        let app = router(test_state());
        for path in [
            "/api/v1/manifest",
            "/api/v1/local/posture",
            "/api/v1/garden/observe",
            "/api/v1/stone/offerings/redis::default",
            "/api/v1/stone/offerings/redis::default/rest",
        ] {
            let res = send(&app, "GET", path).await;
            assert_ne!(
                res.status(),
                StatusCode::OK,
                "{path} must not answer under the old grammar"
            );
        }
    }

    /// The front door is the manifest, and the manifest is complete:
    /// the table lists every face, exactly once per (method, path).
    #[tokio::test]
    async fn front_door_lists_every_face_exactly_once() {
        let app = router(test_state());
        let res = send(&app, "GET", "/api/v1").await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1_000_000).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let routes = v["data"]["routes"].as_array().expect("routes array");
        assert_eq!(routes.len(), Face::ALL.len(), "every face advertised");

        let mut keys: Vec<(String, String)> = routes
            .iter()
            .map(|r| {
                (
                    r["method"].as_str().unwrap().into(),
                    r["path"].as_str().unwrap().into(),
                )
            })
            .collect();
        keys.sort();
        let before = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), before, "no duplicate (method, path) rows");
    }

    /// The SelfView: /stone speaks MY frame, full-voiced (B1 — the pull
    /// face renders the same canonical shape).
    #[tokio::test]
    async fn stone_self_is_my_frame() {
        let app = router(test_state());
        for path in ["/api/v1/stone", "/api/v1/stone/this", "/api/v1/stone/stone-test"] {
            let res = send(&app, "GET", path).await;
            assert_eq!(res.status(), StatusCode::OK, "{path} is me");
            let body = axum::body::to_bytes(res.into_body(), 1_000_000).await.unwrap();
            let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(v["data"]["stone"]["name"], "stone-test", "{path}");
            assert_eq!(v["data"]["meta"]["boot_id"], "boot-test", "{path}");
        }
    }

    /// Land a peer frame in the topology through the real claim path —
    /// the same door the wire uses (R4.5: test the promise, not the guts).
    async fn wire_peer(topology: &Arc<garden_kernel::topology::Topology>, peer: StoneView) {
        let (dispatcher, handle) = Dispatcher::new(16);
        let token = tokio_util::sync::CancellationToken::new();
        topology.claim(&dispatcher, token.clone());
        tokio::spawn(handle.run(token.clone()));
        dispatcher
            .ingest(garden_kernel::ingress::Ingested {
                announcement: garden_contract::wire::Announcement::new(
                    garden_contract::consts::announcement::STONE_CHIRP,
                    serde_json::to_value(&peer.body).unwrap(),
                ),
                source: std::net::SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, 51000)),
                received_at: chrono::Utc::now(),
            })
            .await;
        let mut version = topology.version();
        tokio::time::timeout(std::time::Duration::from_secs(2), version.changed())
            .await
            .expect("cache must settle")
            .expect("watch alive");
        token.cancel();
    }

    /// The delight face (ADR-0004 §4): asking for a peer by name is a
    /// not-here answer that teaches — 404, a Location header, and a
    /// knows_at field naming where the stone answers.
    #[tokio::test]
    async fn asking_for_a_peer_teaches_the_way() {
        let state = test_state();
        wire_peer(&state.topology, sample_peer()).await;
        let app = router(state);

        let res = send(&app, "GET", "/api/v1/stone/stone-peer").await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let location = res
            .headers()
            .get(axum::http::header::LOCATION)
            .expect("the way is named")
            .to_str()
            .unwrap()
            .to_string();
        assert_eq!(location, "http://192.168.1.50:7285/api/v1/stone");

        let body = axum::body::to_bytes(res.into_body(), 1_000_000).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["error"]["not_here"], true);
        assert_eq!(v["error"]["stone"], "stone-peer");
        assert_eq!(v["error"]["knows_at"], location);

        // An unknown name: a plain 404, no way to offer.
        let res = send(&app, "GET", "/api/v1/stone/nobody").await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        assert!(res.headers().get(axum::http::header::LOCATION).is_none());
    }

    /// The splice (ADR-0004 §3): /garden/stones obviously includes the
    /// current stone, among the peers, every row a canonical frame.
    #[tokio::test]
    async fn garden_stones_splices_self_among_peers() {
        let state = test_state();
        wire_peer(&state.topology, sample_peer()).await;
        let app = router(state);

        let res = send(&app, "GET", "/api/v1/garden/stones").await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1_000_000).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let stones = v["data"]["stones"].as_array().expect("stones array");
        assert_eq!(stones.len(), 2, "self + one peer");
        assert_eq!(stones[0]["self"], true, "self spliced first");
        assert_eq!(stones[0]["stone"]["name"], "stone-test");
        assert_eq!(stones[1]["stone"]["name"], "stone-peer");
        assert_eq!(stones[1]["chirps"], 1, "one accepted frame through the real door");
    }

    fn sample_peer() -> StoneView {
        use garden_contract::chirp::{
            Inventory, Moss, Network, PeerAddress, Presence, Reception, ServiceEntry, ServiceState,
            Stone,
        };
        let now = chrono::Utc::now();
        StoneView {
            body: ChirpFrame {
                stone: Stone {
                    id: "0198e0c7-0000-7000-8000-0000000000ef".into(),
                    name: "stone-peer".into(),
                    moss: Moss { version: "0.1.0".into() },
                    network: Network {
                        address: PeerAddress {
                            ip: std::net::IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 1, 50)),
                            port: 7285,
                            tls_port: None,
                        },
                        mac: None,
                    },
                },
                presence: Presence {
                    health: garden_glossary::health::THRIVING.into(),
                    status: garden_glossary::presence::ONLINE.into(),
                },
                inventory: garden_contract::chirp::InventoryMap {
                    services: Some(Inventory {
                        rev: Some(1),
                        total: None,
                        items: vec![ServiceEntry {
                            offering_id: String::new(),
                            name: "mongodb::default".into(),
                            stem: "mongodb".into(),
                            category: "data".into(),
                            state: ServiceState { status: "running".into(), role: None },
                            ports: Default::default(),
                        }],
                    }),
                    ..Default::default()
                },
                meta: garden_contract::chirp::FrameMeta {
                    proto: Some(PROTO_V1.into()),
                    boot_id: None,
                    seq: Some(7),
                    part: None,
                },
                received: Reception { discovered_at: now, last_seen: now },
            },
            last_seen: now,
            chirps: 3,
        }
    }

    /// B1: the cache, HTTP, and the wire render ONE canonical shape — the
    /// sectioned frame — with reception facts filled by the listener.
    #[test]
    fn observe_stone_renders_the_canonical_shape() {
        let peer = sample_peer();
        let mut v = serde_json::to_value(&peer.body).unwrap();
        v.as_object_mut().unwrap().insert("chirps".into(), serde_json::json!(3));
        assert_eq!(v["stone"]["name"], "stone-peer");
        assert_eq!(v["stone"]["network"]["address"]["port"], 7285);
        assert_eq!(v["meta"]["proto"], PROTO_V1);
        assert_eq!(v["chirps"], 3);
    }
}
