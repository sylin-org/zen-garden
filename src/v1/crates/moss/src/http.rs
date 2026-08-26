//! The HTTP surface: observe the garden, describe thyself (L1, L7).
//!
//! One JSON envelope (`{"data": ...}`) everywhere — B1 makes envelope drift
//! unrepresentable. The manifest is a generated table, not prose (L7).

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use garden_contract::chirp::ChirpBody;
use garden_contract::consts::PROTO_V1;
use garden_kernel::dispatch::Dispatcher;
use garden_kernel::ingress::IngestCounters;
use garden_kernel::topology::StoneView;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::offerings::model::ModeData;
use std::sync::Arc;
use uuid::Uuid;

/// Shared state behind the routes.
pub struct AppState {
    pub topology: Arc<garden_kernel::topology::Topology>,
    pub dispatcher: Dispatcher,
    pub ingest_counters: Arc<IngestCounters>,
    pub offerings: Arc<crate::offerings::registry::Registry>,
    pub runtime: Arc<dyn crate::offerings::runtime::Runtime>,
    pub stone_name: String,
    pub boot_id: Uuid,
    pub started_at: chrono::DateTime<chrono::Utc>,
}

/// A peer as observe speaks it: the chirp it last said, plus our view.
#[derive(Serialize)]
struct ObserveStone {
    #[serde(flatten)]
    body: ChirpBody,
    /// How many chirps we have heard this boot.
    chirps: u64,
    /// When we last heard from it (our clock).
    seen_at: chrono::DateTime<chrono::Utc>,
}

impl From<&StoneView> for ObserveStone {
    fn from(peer: &StoneView) -> Self {
        Self { body: peer.body.clone(), chirps: peer.chirps, seen_at: peer.last_seen }
    }
}

/// One row of the self-description. Adding a route means adding a row;
/// the manifest test pins that the table matches the router.
#[derive(Serialize)]
struct ManifestRoute {
    method: &'static str,
    path: &'static str,
    summary: &'static str,
}

const MANIFEST: [ManifestRoute; 8] = [
    ManifestRoute {
        method: "GET",
        path: "/health",
        summary: "Liveness probe of this stone and its wire protocol marker.",
    },
    ManifestRoute {
        method: "GET",
        path: "/api/v1/local/posture",
        summary: "Local data (L22): this moss's live counters - ingest, dispatch, topology size.",
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
            "Stone ops (L22): plant a managed offering {image, ports:{name:container}} via the runtime.",
    },
    ManifestRoute {
        method: "POST",
        path: "/api/v1/stone/offerings/{name}/rest",
        summary: "Stone ops: rest a managed offering - stopped, and reconcile will keep it so.",
    },
    ManifestRoute {
        method: "POST",
        path: "/api/v1/stone/offerings/{name}/wake",
        summary: "Stone ops: wake a rested offering back to running.",
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
                "active": state.offerings.snapshot().len(),
                "candidates": state.offerings.candidate_count(),
            },
        }
    }))
}

async fn garden_observe(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let stones: Vec<ObserveStone> =
        state.topology.snapshot().iter().map(ObserveStone::from).collect();
    Json(serde_json::json!({ "data": { "stones": stones } }))
}

async fn manifest() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "data": { "routes": MANIFEST } }))
}

/// The complete surface. New routes join here AND in [`MANIFEST`]; the
/// `manifest_matches_router` test keeps them honest.
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/local/posture", get(posture))
        .route("/api/v1/garden/observe", get(garden_observe))
        .route("/api/v1/manifest", get(manifest))
        .route(
            "/api/v1/stone/offerings/{name}",
            post(plant_offering).delete(uproot_offering),
        )
        .route(
            "/api/v1/stone/offerings/{name}/rest",
            post(rest_offering),
        )
        .route(
            "/api/v1/stone/offerings/{name}/wake",
            post(wake_offering),
        )
        .with_state(state)
}

// ---- stone ops (L22) -------------------------------------------------------

#[derive(Debug, Deserialize)]
struct PlantRequest {
    image: String,
    /// Named ports: name → container port. Host mapping is the runtime's.
    #[serde(default)]
    ports: HashMap<String, u16>,
    #[serde(default = "default_category")]
    category: String,
}

fn default_category() -> String {
    "misc".into()
}

type ApiResult = Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<serde_json::Value>)>;

fn api_error(status: axum::http::StatusCode, message: impl Into<String>) -> ApiErrorShape {
    (status, Json(serde_json::json!({ "error": { "message": message.into() } })))
}
type ApiErrorShape = (axum::http::StatusCode, Json<serde_json::Value>);

async fn plant_offering(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<PlantRequest>,
) -> ApiResult {
    if state.offerings.get_by_name(&name).is_some() {
        return Err(api_error(axum::http::StatusCode::CONFLICT, format!("'{name}' already planted")));
    }
    let spec = crate::offerings::runtime::WorkloadSpec {
        image: req.image.clone(),
        named_ports: req.ports.clone(),
        ..Default::default()
    };
    let outcome = state
        .runtime
        .deploy(&name, &spec)
        .await
        .map_err(|e| api_error(axum::http::StatusCode::BAD_GATEWAY, e.to_string()))?;
    let raw_ports = state.runtime.host_ports(&name).await;
    let port_map: HashMap<String, u16> = req
        .ports
        .iter()
        .filter_map(|(n, &cp)| raw_ports.get(&format!("{cp}/tcp")).map(|&h| (n.clone(), h)))
        .collect();
    let now = chrono::Utc::now();
    let offering = crate::offerings::model::Offering {
        offering_id: Uuid::now_v7().to_string(),
        name: name.clone(),
        offering: name.clone(),
        category: req.category,
        status: crate::offerings::model::Status::Running,
        location: crate::offerings::model::Location {
            host: "localhost".into(),
            port: port_map.values().copied().next().unwrap_or(0),
            protocol: "http".into(),
        },
        mode_data: ModeData::Managed(crate::offerings::model::ManagedData {
            port_map,
            container_ports: req.ports.clone(),
            image: Some(req.image.clone()),
            volume_root: None,
        }),
        registered_at: now,
        updated_at: now,
    };
    state.offerings.upsert(offering.clone());
    let planted = matches!(outcome, crate::offerings::runtime::DeployOutcome::Created);
    Ok(Json(serde_json::json!({
        "data": { "offering": offering, "planted": planted }
    })))
}

async fn rest_offering(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult {
    let Some(offering) = state.offerings.get_by_name(&name) else {
        return Err(api_error(axum::http::StatusCode::NOT_FOUND, format!("'{name}' is not planted here")));
    };
    state
        .runtime
        .stop(&name)
        .await
        .map_err(|e| api_error(axum::http::StatusCode::BAD_GATEWAY, e.to_string()))?;
    state.offerings.set_status(&offering.offering_id, crate::offerings::model::Status::Stopped);
    Ok(Json(serde_json::json!({ "data": { "name": name, "status": "stopped" } })))
}

async fn wake_offering(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult {
    let Some(offering) = state.offerings.get_by_name(&name) else {
        return Err(api_error(axum::http::StatusCode::NOT_FOUND, format!("'{name}' is not planted here")));
    };

    // Self-heal (PoC wake parity): if the workload vanished behind our back,
    // resurrect it from stored knowledge before starting.
    if state.runtime.inspect(&name).await.is_none() {
        let ModeData::Managed(m) = &offering.mode_data else {
            return Err(api_error(axum::http::StatusCode::CONFLICT, "'{name}' is not managed"));
        };
        let Some(image) = &m.image else {
            return Err(api_error(axum::http::StatusCode::CONFLICT, "managed offering lacks stored image"));
        };
        tracing::warn!(offering = %name, "workload missing - resurrecting");
        let spec = crate::offerings::runtime::WorkloadSpec {
            image: image.clone(),
            named_ports: m.container_ports.clone(),
            ..Default::default()
        };
        state
            .runtime
            .deploy(&name, &spec)
            .await
            .map_err(|e| api_error(axum::http::StatusCode::BAD_GATEWAY, e.to_string()))?;
    } else {
        state
            .runtime
            .start(&name)
            .await
            .map_err(|e| api_error(axum::http::StatusCode::BAD_GATEWAY, e.to_string()))?;
    }
    state.offerings.set_status(&offering.offering_id, crate::offerings::model::Status::Running);

    // Auto-assigned host ports may differ after a restart — there is no
    // ledger yet (O2), so detect the remap and keep the registry honest.
    let mut remapped = false;
    let mut updated = offering.clone();
    if let ModeData::Managed(m) = &mut updated.mode_data {
        let raw = state.runtime.host_ports(&name).await;
        let new_map: HashMap<String, u16> = m
            .container_ports
            .iter()
            .filter_map(|(n, &cp)| raw.get(&format!("{cp}/tcp")).map(|&h| (n.clone(), h)))
            .collect();
        if !new_map.is_empty() && new_map != m.port_map {
            m.port_map = new_map.clone();
            updated.location.port = new_map.values().copied().next().unwrap_or(0);
            state.offerings.upsert(updated);
            remapped = true;
        }
    }
    Ok(Json(
        serde_json::json!({ "data": { "name": name, "status": "running", "ports_remapped": remapped } }),
    ))
}

async fn uproot_offering(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult {
    let Some(offering) = state.offerings.get_by_name(&name) else {
        return Err(api_error(axum::http::StatusCode::NOT_FOUND, format!("'{name}' is not planted here")));
    };
    // Managed only for now; adopted release / borrowed return come with O3.
    if !matches!(offering.mode_data, crate::offerings::model::ModeData::Managed(_)) {
        return Err(api_error(
            axum::http::StatusCode::CONFLICT,
            format!("'{name}' is not managed; uproot applies to managed offerings"),
        ));
    }
    state
        .runtime
        .remove(&name)
        .await
        .map_err(|e| api_error(axum::http::StatusCode::BAD_GATEWAY, e.to_string()))?;
    state.offerings.remove(&offering.offering_id);
    Ok(Json(serde_json::json!({ "data": { "name": name, "uprooted": true } })))
}

#[cfg(test)]
mod tests {
    // R4.1: unwrap_used is denied in domain code but sanctioned in tests.
    #![allow(clippy::unwrap_used)]
    use super::*;

    /// L7: self-description is generated truth — every route in the
    /// manifest answers 200. Requests go through the real router.
    #[tokio::test]
    async fn every_manifest_route_answers() {
        use tower::ServiceExt;

        let state = Arc::new(AppState {
            topology: Arc::new(garden_kernel::topology::Topology::new()),
            dispatcher: garden_kernel::dispatch::Dispatcher::new(16).0,
            ingest_counters: Arc::new(garden_kernel::ingress::IngestCounters::default()),
            offerings: Arc::new(crate::offerings::registry::Registry::load(
                std::env::temp_dir().join(format!("moss-test-offerings-{}.json", Uuid::now_v7())),
            )),
            runtime: Arc::new(crate::offerings::runtime::NullRuntime),
            stone_name: "stone-test".into(),
            boot_id: Uuid::now_v7(),
            started_at: chrono::Utc::now(),
        });
        let app = router(state);

        for entry in &MANIFEST {
            let req = axum::http::Request::builder()
                .uri(entry.path)
                .body(axum::body::Body::empty())
                .unwrap();
            let res = app.clone().oneshot(req).await.unwrap();
            assert_eq!(res.status(), 200, "{} must answer", entry.path);
        }
    }

    fn sample_stone() -> StoneView {
        use garden_contract::chirp::{PeerAddress, ServiceEntry};
        let now = chrono::Utc::now();
        StoneView {
            body: ChirpBody {
                stone_id: "0198e0c7-0000-7000-8000-0000000000ef".into(),
                stone_name: "stone-peer".into(),
                address: PeerAddress {
                    ip: std::net::IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 1, 50)),
                    port: 7285,
                    tls_port: None,
                },
                moss_version: "0.1.0".into(),
                services: vec![ServiceEntry {
                    offering_id: String::new(),
                    name: "mongodb".into(),
                    offering: "mongodb".into(),
                    category: "data".into(),
                    status: "running".into(),
                    role: None,
                    ports: Default::default(),
                }],
                health: garden_glossary::health::THRIVING.into(),
                status: garden_glossary::presence::ONLINE.into(),
                discovered_at: now,
                last_seen: now,
                mac: None,
                proto: Some(PROTO_V1.into()),
                boot_id: None,
                seq: Some(7),
            },
            last_seen: now,
            chirps: 3,
        }
    }

    /// B1/L1: observe hoists the peer's chirp fields to the top level —
    /// consumers read one flat shape, not a wrapped one.
    #[test]
    fn observe_stone_flattens_chirp_body() {
        let view = ObserveStone::from(&sample_stone());
        let v = serde_json::to_value(&view).unwrap();
        assert_eq!(v["stone_name"], "stone-peer", "flatten must hoist body fields");
        assert_eq!(v["chirps"], 3);
        assert_eq!(v["proto"], PROTO_V1);
    }
}
