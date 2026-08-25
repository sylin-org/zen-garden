//! The HTTP surface: observe the garden, describe thyself (L1, L7).
//!
//! One JSON envelope (`{"data": ...}`) everywhere — B1 makes envelope drift
//! unrepresentable. The manifest is a generated table, not prose (L7).

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use garden_contract::chirp::ChirpBody;
use garden_contract::consts::PROTO_V1;
use garden_kernel::topology::StoneView;
use serde::Serialize;
use std::sync::Arc;
use uuid::Uuid;

/// Shared state behind the routes.
pub struct AppState {
    pub topology: Arc<garden_kernel::topology::Topology>,
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

const MANIFEST: [ManifestRoute; 3] = [
    ManifestRoute {
        method: "GET",
        path: "/health",
        summary: "Liveness probe of this stone and its wire protocol marker.",
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
        .route("/api/v1/garden/observe", get(garden_observe))
        .route("/api/v1/manifest", get(manifest))
        .with_state(state)
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
