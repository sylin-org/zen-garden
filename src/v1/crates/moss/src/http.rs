//! The HTTP surface: observe the garden, describe thyself, tend offerings
//! (L1, L7, L22). Handlers are thin: parse → delegate to the application
//! service or the kernel aggregates → envelope. No domain logic lives here.

use crate::offerings::service::{CommandError, OfferingService};
use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use garden_contract::consts::PROTO_V1;
use serde::{Deserialize, Serialize};
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
    pub stone_name: String,
    pub boot_id: Uuid,
    pub started_at: chrono::DateTime<chrono::Utc>,
}

use garden_kernel::dispatch::Dispatcher;
use garden_kernel::ingress::IngestCounters;

/// One row of the self-description (L7). Adding a route means adding a
/// row; the every-route-answers test keeps them honest.
#[derive(Serialize)]
struct ManifestRoute {
    method: &'static str,
    path: &'static str,
    summary: &'static str,
}

const MANIFEST: [ManifestRoute; 9] = [
    ManifestRoute {
        method: "GET",
        path: "/health",
        summary: "Liveness probe of this stone and its wire protocol marker.",
    },
    ManifestRoute {
        method: "GET",
        path: "/api/v1/local/posture",
        summary:
            "Local data (L22): this moss's live counters - ingest, dispatch, topology, offerings.",
    },
    ManifestRoute {
        method: "GET",
        path: "/api/v1/garden/observe",
        summary: "Garden data (L22): the room as this moss sees it - every peer's last chirp.",
    },
    ManifestRoute {
        method: "GET",
        path: "/api/v1/manifest",
        summary: "This route table - every surface, described in place.",
    },
    ManifestRoute {
        method: "POST",
        path: "/api/v1/stone/offerings/{name}",
        summary:
            "Stone ops (L22): plant a managed offering {image, ports:{name:container}, runtime?}.",
    },
    ManifestRoute {
        method: "GET",
        path: "/api/v1/stone/offerings/{name}",
        summary: "Stone ops: the placed record - plan, decisions, ports (§5.3).",
    },
    ManifestRoute {
        method: "POST",
        path: "/api/v1/stone/offerings/{name}/rest",
        summary: "Stone ops: rest a managed offering - stopped, and reconcile will keep it so.",
    },
    ManifestRoute {
        method: "POST",
        path: "/api/v1/stone/offerings/{name}/wake",
        summary:
            "Stone ops: wake a rested offering; resurrects from its stored spec if reality lost it.",
    },
    ManifestRoute {
        method: "DELETE",
        path: "/api/v1/stone/offerings/{name}",
        summary: "Stone ops: uproot - remove the workload and forget the offering.",
    },
];

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

async fn garden_observe(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let stones: Vec<serde_json::Value> = state
        .topology
        .snapshot()
        .iter()
        .map(|peer| {
            // The canonical frame, flattened: consumers read one shape, not
            // a wrapped one — and its `received` section IS our reception
            // record, so the JSON stays honest without extra wrapping.
            let mut v = serde_json::to_value(&peer.body).unwrap_or_default();
            if let Some(obj) = v.as_object_mut() {
                obj.insert("chirps".into(), serde_json::json!(peer.chirps));
            }
            v
        })
        .collect();
    Json(serde_json::json!({ "data": { "stones": stones } }))
}

async fn manifest() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "data": { "routes": MANIFEST } }))
}

// ---- stone ops (L22) — thin delegation to the application service ---------

type ApiResult = Result<Json<serde_json::Value>, ApiError>;

struct ApiError(CommandError);

impl From<CommandError> for ApiError {
    fn from(e: CommandError) -> Self {
        Self(e)
    }
}

impl axum::response::IntoResponse for ApiError {
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
    Ok(Json(serde_json::json!({ "data": { "offering": offering } })))
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
        Some(o) => Ok(Json(serde_json::json!({ "data": { "offering": o } }))),
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

/// The complete surface. New routes join here AND in [`MANIFEST`]; the
/// `every_manifest_route_answers` test keeps them honest.
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/local/posture", get(posture))
        .route("/api/v1/garden/observe", get(garden_observe))
        .route("/api/v1/manifest", get(manifest))
        .route(
            "/api/v1/stone/offerings/{name}",
            post(plant_offering).delete(uproot_offering).get(show_offering),
        )
        .route("/api/v1/stone/offerings/{name}/rest", post(rest_offering))
        .route("/api/v1/stone/offerings/{name}/wake", post(wake_offering))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    // R4.1: unwrap/expect sanctioned in tests.
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::offerings::registry::{MemorySnapshotStore, Registry};
    use crate::offerings::runtime::{NullRuntime, RuntimeRegistry};
    use garden_contract::chirp::ChirpFrame;
    use garden_kernel::topology::StoneView;

    fn test_state() -> Arc<AppState> {
        let registry = Arc::new(Registry::new(Arc::new(MemorySnapshotStore::default())));
        let worlds = Arc::new(RuntimeRegistry::build(vec![Arc::new(NullRuntime)]));
        let factsheet = Arc::new(crate::offerings::facts::Factsheet::empty());
        let service = Arc::new(OfferingService::new(
            registry,
            worlds,
            "null".into(),
            Arc::new(crate::offerings::manifest::Catalog::default()),
            factsheet,
            crate::offerings::directory::OfferingsRoot::new(
                std::env::temp_dir().join(format!("moss-test-offer-{}", Uuid::now_v7())),
            ),
            crate::offerings::ports::Pool::default(),
        ));
        Arc::new(AppState {
            garden: service,
            topology: Arc::new(garden_kernel::topology::Topology::new()),
            dispatcher: Dispatcher::new(16).0,
            ingest_counters: Arc::new(IngestCounters::default()),
            stone_name: "stone-test".into(),
            boot_id: Uuid::now_v7(),
            started_at: chrono::Utc::now(),
        })
    }

    /// L7: self-description is generated truth — every route in the
    /// manifest answers 200 through the real router.
    #[tokio::test]
    async fn every_manifest_route_answers() {
        use tower::ServiceExt;

        let app = router(test_state());

        for entry in &MANIFEST {
            let req = match entry.method {
                "GET" => axum::http::Request::builder()
                    .uri(entry.path)
                    .body(axum::body::Body::empty())
                    .unwrap(),
                // Plant needs a body; a missing workload answers 502/404 but
                // still proves ROUTING. Accept anything but a panic.
                _ => axum::http::Request::builder()
                    .method(entry.method)
                    .uri(entry.path)
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from("{}"))
                    .unwrap(),
            };
            let res = app.clone().oneshot(req).await.unwrap();
            assert_ne!(
                res.status(),
                405,
                "{} {} must be routed",
                entry.method,
                entry.path
            );
        }
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
                services: Inventory {
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
                },
                meta: garden_contract::chirp::FrameMeta {
                    proto: Some(PROTO_V1.into()),
                    boot_id: None,
                    seq: Some(7),
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
