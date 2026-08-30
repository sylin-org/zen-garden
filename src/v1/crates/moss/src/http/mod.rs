use crate::garden::service::{CommandError, OfferingService};

use axum::extract::{Path, Query, State};

use axum::response::{IntoResponse, Response};

use axum::routing::{get, post};

use axum::{Json, Router};

use garden_contract::consts::PROTO_V1;

use crate::room::announce::ChirpSource;

use serde::Deserialize;

use std::collections::HashMap;

use std::sync::Arc;

use uuid::Uuid;

/// How long a rehearsal container is held before the verdict (J2):
/// long enough for a restored service to reveal a crash-loop, short

pub(crate) mod offerings;
pub(crate) mod room;
pub(crate) mod storage;

use offerings::{
    capture_last, capture_offer, capability_add, capability_remove, capture_view,
    offering_capabilities, offering_logs_stream,
    offerings_list, plan_install, plant_offering, record_view, rehearse_offer, replant_offer,
    rest_offering, show_offering, update_check_face, update_face, wake_offering, uproot_offering,
};
use room::{
    catalog, front_door, garden_stones, garden_storage, health, html_escape, job_detail, job_list,
    portrait, posture, pulse_page, pulse_sse, pulse_stream, root, stone_ref, stone_self,
};
use storage::{
    content_type_for, files_err, gate_bank, parse_range, storage_adopt, storage_eject,
    storage_file_delete, storage_file_get, storage_files_list, storage_file_move, storage_file_put,
    storage_list, storage_roles,
};

pub struct AppState {
    /// The offering application service (domain + worlds coordinated).
    pub garden: Arc<OfferingService>,
    /// This stone's banks (ADR-0005 §8) — the storage MVP's state.
    pub storage: Arc<crate::garden::storage::Storage>,
    /// The living will's runner (ADR-0005 §2).
    pub capture: Arc<crate::garden::will::Runner>,
    /// The async operation tracker (the data plane's async contract).
    pub jobs: crate::jobs::JobTracker,
    /// The pulse bus (ADR-0013): typed, seq'd news for stream readers.
    pub pulse: Arc<crate::pulse::Bus>,
    pub topology: Arc<crate::room::topology::Topology>,
    pub dispatcher: Dispatcher,
    pub ingest_counters: Arc<IngestCounters>,
    /// This stone's voice — the SelfView's composer (self is a projection,
    /// never a stored peer; ADR-0004 §3).
    pub chirp_source: Arc<dyn ChirpSource>,
    pub stone_name: String,
    pub boot_id: Uuid,
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// The stone's shutdown token: stream faces (SSE) end on cancel so
    /// the graceful drain can finish and the farewell can be spoken.
    pub shutdown: tokio_util::sync::CancellationToken,
}


impl AppState {
    /// The Moss root's provenance mouth (ADR-0015): plan and install
    /// offerings by name — `state.provenance().plan_install("ollama")`
    /// answers can/cannot and why, touching nothing.
    pub fn provenance(&self) -> crate::garden::provenance::Provenance<'_> {
        crate::garden::provenance::Provenance::new(&self.garden)
    }
}


use crate::room::dispatch::Dispatcher;

use crate::room::ingress::IngestCounters;

/// The surface, declared once — now FROM THE CONTRACT (ADR-0009/B1):
/// the Face enum and its declarations live in garden_contract::faces;

use garden_contract::faces::Face;
/// The wiring: which handler answers each face. The declarations

fn method_router(face: Face) -> axum::routing::MethodRouter<Arc<AppState>> {
        match face {
            Face::Health => get(health),
            Face::FrontDoor => get(front_door),
            Face::StoneSelf | Face::StoneThis => get(stone_self),
            Face::StoneRef => get(stone_ref),
            Face::StonePosture => get(posture),
            Face::GardenStones => get(garden_stones),
            Face::PlanInstall => post(plan_install),
            Face::Catalog => get(catalog),
            Face::StorageList => get(storage_list),
            Face::StorageAdopt => post(storage_adopt),
            Face::StorageRoles => post(storage_roles),
            Face::StorageEject => post(storage_eject),
            Face::StorageFileList => get(storage_files_list),
            Face::StorageFileGet => get(storage_file_get),
            Face::StorageFilePut => axum::routing::put(storage_file_put),
            Face::StorageFileMove => axum::routing::patch(storage_file_move),
            Face::StorageFileDelete => axum::routing::delete(storage_file_delete),
            Face::GardenStorage => get(garden_storage),
            Face::OfferingList => get(offerings_list),
            Face::OfferingPlant => post(plant_offering),
            Face::OfferingCapture => post(capture_offer),
            Face::OfferingLogsStream => get(offering_logs_stream),
            Face::OfferingCaptureLast => get(capture_last),
            Face::OfferingReplant => post(replant_offer),
            Face::OfferingRehearse => post(rehearse_offer),
            Face::OfferingUpdateCheck => get(update_check_face),
            Face::OfferingUpdate => post(update_face),
            Face::Portrait => get(portrait),
            Face::Root => get(root),
            Face::JobList => get(job_list),
            Face::JobDetail => get(job_detail),
            Face::PulsePage => get(pulse_page),
            Face::PulseStream => get(pulse_stream),
            Face::Mcp => post(crate::mcp::handle),
            Face::OfferingShow => get(show_offering),
            Face::OfferingRest => post(rest_offering),
            Face::OfferingWake => post(wake_offering),
            Face::OfferingUproot => axum::routing::delete(uproot_offering),
            Face::OfferingCapabilities => get(offering_capabilities),
            Face::OfferingCapabilityAdd => post(capability_add),
            Face::OfferingCapabilityRemove => axum::routing::delete(capability_remove),
        }
    }



pub(crate) type ApiResult = Result<Json<serde_json::Value>, ApiError>;


pub(crate) struct ApiError(CommandError);


impl From<CommandError> for ApiError {
    fn from(e: CommandError) -> Self {
        Self(e)
    }
}


impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        use CommandError::*;
        let status = match self.0 {
            NotFound(_) => axum::http::StatusCode::NOT_FOUND,
            Conflict(_) => axum::http::StatusCode::CONFLICT,
            BadRequest(_) => axum::http::StatusCode::BAD_REQUEST,
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


pub fn router(state: Arc<AppState>) -> Router {
    let router = garden_contract::faces::FACES
        .iter()
        .fold(Router::new(), |r, face| {
            r.route(face.path, method_router(face.face))
        });
    router.with_state(state)
}


#[cfg(test)]
pub(crate) mod tests {
    // R4.1: unwrap/expect sanctioned in tests.
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::garden::registry::{MemorySnapshotStore, Registry};
    use crate::garden::runtime::{NullRuntime, RuntimeRegistry};
    use crate::room::voice::{DynamicChirpSource, Voice};
    use axum::http::StatusCode;
    use garden_contract::chirp::ChirpFrame;
    use crate::room::topology::StoneView;
    use tower::ServiceExt;

    pub(crate) fn test_state() -> Arc<AppState> {
        let registry = Arc::new(Registry::new(Arc::new(MemorySnapshotStore::default())));
        let worlds = Arc::new(RuntimeRegistry::build(vec![Arc::new(NullRuntime)]));
        let factsheet = Arc::new(crate::garden::facts::Factsheet::empty());
        let service = Arc::new(OfferingService::new(
            registry.clone(),
            worlds,
            "null".into(),
            Arc::new(crate::garden::manifest::Catalog::default()),
            factsheet,
            crate::garden::directory::OfferingsRoot::new(
                std::env::temp_dir().join(format!("moss-test-offer-{}", Uuid::now_v7())),
            ),
            crate::garden::ports::Pool::default(),
            None,
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
            Arc::new(crate::garden::storage::Storage::new()),
        );
        Arc::new(AppState {
            garden: service,
            storage: Arc::new(crate::garden::storage::Storage::new()),
            capture: Arc::new(crate::garden::will::Runner::new(
                Arc::new(crate::garden::storage::Storage::new()),
                Arc::new(crate::garden::will::NullHooks),
            )),
            jobs: crate::jobs::JobTracker::new(),
            pulse: Arc::new(crate::pulse::Bus::new()),
            topology: Arc::new(crate::room::topology::Topology::new()),
            dispatcher: Dispatcher::new(16).0,
            ingest_counters: Arc::new(IngestCounters::default()),
            chirp_source,
            stone_name: "stone-test".into(),
            boot_id: Uuid::now_v7(),
            started_at: chrono::Utc::now(),
            shutdown: tokio_util::sync::CancellationToken::new(),
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

        for face in garden_contract::faces::FACES.iter().map(|d| d.face) {
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
                    // Root redirects to the portrait; every other static GET answers.
                    let want = if face == Face::Root {
                        StatusCode::TEMPORARY_REDIRECT
                    } else {
                        StatusCode::OK
                    };
                    assert_eq!(
                        res.status(),
                        want,
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
        assert_eq!(routes.len(), garden_contract::faces::FACES.len(), "every face advertised");

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
    async fn wire_peer(topology: &Arc<crate::room::topology::Topology>, peer: StoneView) {
        let (dispatcher, handle) = Dispatcher::new(16);
        let token = tokio_util::sync::CancellationToken::new();
        topology.claim(&dispatcher, token.clone());
        tokio::spawn(handle.run(token.clone()));
        dispatcher
            .ingest(crate::room::ingress::Ingested {
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

    /// The adopt face routes and validates: a device no scan reports is a
    /// loud Conflict naming the problem (R3.3), never a silent empty.
    #[tokio::test]
    async fn adopt_refuses_unknown_devices_loudly() {
        let app = router(test_state());
        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/api/v1/storage/adopt")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(
                r#"{"device": "Q:", "name": "seed-vault"}"#,
            ))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::CONFLICT);
        let body = axum::body::to_bytes(res.into_body(), 100_000).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            v["error"]["message"]
                .as_str()
                .unwrap()
                .contains("no removable volume"),
            "the refusal teaches: {}",
            v["error"]["message"]
        );
    }

    /// The room's banks (ADR-0004 §4 grid): self spliced first, then the
    /// peer's banks as the cache heard them — end-to-end from song merge
    /// to surface.
    #[tokio::test]
    async fn garden_storage_projects_the_room() {
        let state = test_state();
        // Self holds a bank; the peer holds another (via its song frame).
        state
            .storage
            .adopt(
                &crate::garden::storage::VolumeFact {
                    roles: Vec::new(),
                    mount_point: {
                        let d = std::env::temp_dir()
                            .join(format!("zg-tmp-adopt-{}", std::process::id()));
                        std::fs::create_dir_all(&d).unwrap();
                        d
                    },
                    device_id: None,
                    fqn: None,
                    capacity_bytes: 4000,
                    available_bytes: 3000,
                },
                "local-vault",
                "0198e0c7-0000-7000-8000-000000000001",
            )
            .unwrap();
        wire_peer(&state.topology, sample_peer()).await;
        let app = router(state);

        let res = send(&app, "GET", "/api/v1/garden/storage").await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1_000_000).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let rows = v["data"]["banks"].as_array().expect("rows");
        assert_eq!(rows.len(), 2, "self + peer");
        assert_eq!(rows[0]["self"], true);
        assert_eq!(rows[0]["bank"]["fqn"], "local-vault::default");
        assert_eq!(rows[1]["stone"], "stone-peer");
        assert_eq!(rows[1]["bank"]["fqn"], "seed-vault::default");
        assert_eq!(rows[1]["bank"]["state"], "mounted");
    }

    /// The eject verb's happy path: adopted banks eject, the state sings,
    /// and the refusal cases stay loud (R3.3).
    #[tokio::test]
    async fn eject_announces_authoritative_absence() {
        let state = test_state();
        state
            .storage
            .adopt(
                &crate::garden::storage::VolumeFact {
                    roles: Vec::new(),
                    mount_point: {
                        let d = std::env::temp_dir()
                            .join(format!("zg-tmp-eject-{}", std::process::id()));
                        std::fs::create_dir_all(&d).unwrap();
                        d
                    },
                    device_id: None,
                    fqn: None,
                    capacity_bytes: 1000,
                    available_bytes: 900,
                },
                "seed-vault",
                "0198e0c7-0000-7000-8000-000000000001",
            )
            .unwrap();
        let app = router(state);

        let res = send(&app, "POST", "/api/v1/storage/seed-vault/eject").await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 100_000).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["data"]["bank"]["state"], "ejected");

        // Ejecting twice is a conflict; ejecting a ghost is a 404.
        let res = send(&app, "POST", "/api/v1/storage/seed-vault/eject").await;
        assert_eq!(res.status(), StatusCode::CONFLICT);
        let res = send(&app, "POST", "/api/v1/storage/nobody/eject").await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    // ---- bank files: the storage data plane over the real router -------

    /// A test state whose stone holds a bank on a real temp volume.
    async fn state_with_bank() -> (Arc<AppState>, std::path::PathBuf) {
        let state = test_state();
        let tmp = std::env::temp_dir().join(format!("zg-http-files-{}", Uuid::now_v7()));
        std::fs::create_dir_all(&tmp).unwrap();
        state
            .storage
            .adopt(
                &crate::garden::storage::VolumeFact {
                    roles: Vec::new(),
                    mount_point: tmp.clone(),
                    device_id: None,
                    fqn: None,
                    capacity_bytes: 1_000_000,
                    available_bytes: 900_000,
                },
                "seed-vault",
                "0198e0c7-0000-7000-8000-000000000001",
            )
            .unwrap();
        (state, tmp)
    }

    /// Like `send`, but carrying a raw body (the file verbs' payloads).
    async fn send_bytes(
        app: &Router,
        method: &str,
        path: &str,
        body: &[u8],
    ) -> axum::http::Response<axum::body::Body> {
        let req = axum::http::Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/octet-stream")
            .body(axum::body::Body::from(body.to_vec()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap()
    }

    /// Like `send`, but carrying a JSON body and saying so.
    async fn send_json(
        app: &Router,
        method: &str,
        path: &str,
        body: serde_json::Value,
    ) -> axum::http::Response<axum::body::Body> {
        let req = axum::http::Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json")
            .body(axum::body::Body::from(body.to_string()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap()
    }

    /// `send` plus one extra header (Range tests ride this).
    async fn send_with(
        app: &Router,
        method: &str,
        path: &str,
        name: &str,
        value: &str,
    ) -> axum::http::Response<axum::body::Body> {
        let req = axum::http::Request::builder()
            .method(method)
            .uri(path)
            .header(name, value)
            .body(axum::body::Body::empty())
            .unwrap();
        app.clone().oneshot(req).await.unwrap()
    }

    async fn body_json(res: &mut axum::http::Response<axum::body::Body>) -> serde_json::Value {
        let bytes = axum::body::to_bytes(std::mem::take(res).into_body(), 1_000_000)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    /// The CRUD roundtrip: put creates parents and reports size, get rides
    /// the raw bytes with a guessed type, list shows files but never the
    /// adoption record, delete removes, and the gone file answers 404.
    #[tokio::test]
    async fn bank_files_crud_over_http() {
        let (state, tmp) = state_with_bank().await;
        let app = router(state);
        let base = "/api/v1/storage/seed-vault/files";

        let mut res =
            send_bytes(&app, "PUT", &format!("{base}/dumps/notes.txt"), b"hello bank").await;
        assert_eq!(res.status(), StatusCode::OK);
        let v = body_json(&mut res).await;
        assert_eq!(v["data"]["size_bytes"], 10, "the write is sized honestly");
        assert_eq!(v["data"]["bank"], "seed-vault::default");

        let res = send(&app, "GET", &format!("{base}/dumps/notes.txt")).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(
            res.headers()[axum::http::header::CONTENT_TYPE],
            "text/plain; charset=utf-8",
            "the extension guesses the type"
        );
        let bytes = axum::body::to_bytes(res.into_body(), 1_000_000).await.unwrap();
        assert_eq!(&bytes[..], b"hello bank", "the raw bytes ride alone");

        let mut res = send(&app, "GET", base).await;
        assert_eq!(res.status(), StatusCode::OK);
        let v = body_json(&mut res).await;
        let rows = v["data"]["files"].as_array().unwrap();
        assert_eq!(rows.len(), 1, "only dumps shows — the adoption record is invisible");
        assert_eq!(rows[0]["name"], "dumps");
        assert_eq!(rows[0]["kind"], "dir");

        let mut res = send(&app, "GET", &format!("{base}?path=dumps")).await;
        assert_eq!(res.status(), StatusCode::OK);
        let v = body_json(&mut res).await;
        let rows = v["data"]["files"].as_array().unwrap();
        assert_eq!(rows[0]["name"], "notes.txt");
        assert_eq!(rows[0]["size_bytes"], 10);

        let mut res = send(&app, "DELETE", &format!("{base}/dumps/notes.txt")).await;
        assert_eq!(res.status(), StatusCode::OK);
        let v = body_json(&mut res).await;
        assert_eq!(v["data"]["deleted"], true);

        let res = send(&app, "GET", &format!("{base}/dumps/notes.txt")).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND, "gone is gone");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The escape laws hold at the wire: `..` (raw or percent-spelled)
    /// and the adoption record refuse with a 400 that teaches.
    #[tokio::test]
    async fn file_paths_refuse_escapes_and_the_manifest() {
        let (state, tmp) = state_with_bank().await;
        let app = router(state);
        let base = "/api/v1/storage/seed-vault/files";

        for path in [
            format!("{base}/..%2Fsecret"),
            format!("{base}/ok/../../secret"),
            format!("{base}/.zen-garden/manifest.json"),
        ] {
            let mut res = send(&app, "GET", &path).await;
            assert_eq!(res.status(), StatusCode::BAD_REQUEST, "{path}");
            let v = body_json(&mut res).await;
            assert!(
                !v["error"]["message"].as_str().unwrap_or("").is_empty(),
                "{path} must teach"
            );
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The gates before the filesystem: unknown banks 404, ejected banks
    /// 409 — even before a byte is asked for.
    #[tokio::test]
    async fn file_faces_refuse_unknown_and_ejected_banks() {
        let (state, tmp) = state_with_bank().await;
        let mut res = send(
            &router(state.clone()),
            "GET",
            "/api/v1/storage/ghost/files",
        )
        .await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let v = body_json(&mut res).await;
        assert!(
            v["error"]["message"].as_str().unwrap().contains("no bank"),
            "the refusal names the miss: {}",
            v["error"]["message"]
        );

        state.storage.eject("seed-vault").unwrap();
        let mut res = send(
            &router(state),
            "GET",
            "/api/v1/storage/seed-vault/files/dumps/notes.txt",
        )
        .await;
        assert_eq!(res.status(), StatusCode::CONFLICT, "ejected: no volume");
        let v = body_json(&mut res).await;
        assert!(
            v["error"]["message"].as_str().unwrap().contains("ejected"),
            "{}",
            v["error"]["message"]
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Verbs and kinds agree at the wire: reading or deleting a directory
    /// conflicts — the path is real, the verb does not apply.
    #[tokio::test]
    async fn directories_refuse_the_file_verbs() {
        let (state, tmp) = state_with_bank().await;
        let app = router(state);
        let base = "/api/v1/storage/seed-vault/files";
        send_bytes(&app, "PUT", &format!("{base}/dumps/a.txt"), b"x")
            .await;

        let res = send(&app, "GET", &format!("{base}/dumps")).await;
        assert_eq!(res.status(), StatusCode::CONFLICT);
        let res = send(&app, "DELETE", &format!("{base}/dumps")).await;
        assert_eq!(res.status(), StatusCode::CONFLICT);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// A bank grows on ONE stone: the files faces landing where the
    /// volume is not answer the garden's only true redirect — 404, a
    /// Location, and `knows_at` naming the holder (1:1 with the stone
    /// face). Writes bind the same way; a bank nobody holds is a plain
    /// 404; local presence beats the room's stale claim.
    #[tokio::test]
    async fn a_peers_bank_teaches_the_way() {
        let state = test_state();
        // The peer's song carries seed-vault::default at 192.168.1.50.
        wire_peer(&state.topology, sample_peer()).await;
        let app = router(state);
        let base = "/api/v1/storage/seed-vault/files";

        let res = send(&app, "GET", base).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let location = res
            .headers()
            .get(axum::http::header::LOCATION)
            .expect("the way is named")
            .to_str()
            .unwrap()
            .to_string();
        assert_eq!(location, "http://192.168.1.50:7285/api/v1/stone");

        let mut res = send(&app, "GET", base).await;
        let v = body_json(&mut res).await;
        assert_eq!(v["error"]["not_here"], true);
        assert_eq!(v["error"]["bank"], "seed-vault::default");
        assert_eq!(v["error"]["knows_at"], location);

        let res = send_bytes(&app, "PUT", &format!("{base}/notes.txt"), b"x").await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND, "writes redirect too");
        assert_eq!(
            res.headers().get(axum::http::header::LOCATION).unwrap(),
            "http://192.168.1.50:7285/api/v1/stone"
        );

        let res = send(&app, "GET", "/api/v1/storage/ghost/files").await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND, "nobody holds it");
        assert!(res.headers().get(axum::http::header::LOCATION).is_none());

        // Local presence wins: the same FQN adopted HERE answers HERE,
        // even though the peer's song claims the name too.
        let state = test_state();
        let tmp = std::env::temp_dir().join(format!("zg-local-wins-{}", Uuid::now_v7()));
        std::fs::create_dir_all(&tmp).unwrap();
        state
            .storage
            .adopt(
                &crate::garden::storage::VolumeFact {
                    roles: Vec::new(),
                    mount_point: tmp.clone(),
                    device_id: None,
                    fqn: None,
                    capacity_bytes: 1,
                    available_bytes: 1,
                },
                "seed-vault",
                "0198e0c7-0000-7000-8000-000000000001",
            )
            .unwrap();
        wire_peer(&state.topology, sample_peer()).await;
        let app = router(state);
        let res = send(&app, "GET", base).await;
        assert_eq!(res.status(), StatusCode::OK, "the volume is in MY slot");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// One call trees the bank: depth bounds the walk, names below level
    /// one are full relative paths, and the record stays invisible
    /// (objective: an agent trees a bank in one call).
    #[tokio::test]
    async fn depth_lists_the_whole_tree_in_one_call() {
        let (state, tmp) = state_with_bank().await;
        let app = router(state);
        let base = "/api/v1/storage/seed-vault/files";
        send_bytes(&app, "PUT", &format!("{base}/a/b/c.txt"), b"deep").await;
        send_bytes(&app, "PUT", &format!("{base}/x.txt"), b"top").await;

        let names_at = |v: serde_json::Value| -> Vec<String> {
            v["data"]["files"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| e["name"].as_str().unwrap().to_string())
                .collect()
        };

        let mut res = send(&app, "GET", &format!("{base}?depth=2")).await;
        let v = body_json(&mut res).await;
        assert_eq!(names_at(v), vec!["a", "a/b", "x.txt"], "c.txt is beyond depth 2");

        let mut res = send(&app, "GET", &format!("{base}?depth=all")).await;
        let v = body_json(&mut res).await;
        assert_eq!(
            names_at(v),
            vec!["a", "a/b", "a/b/c.txt", "x.txt"],
            "the whole tree, flat, `/`-joined"
        );

        let mut res = send(&app, "GET", base).await;
        let v = body_json(&mut res).await;
        assert_eq!(names_at(v), vec!["a", "x.txt"], "default depth is 1");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// A standard client can stream: single-range 206 with Content-Range,
    /// suffix and open-ended ranges, 416 past EOF, and malformed specs
    /// degrade to the full 200 (RFC 7233).
    #[tokio::test]
    async fn range_requests_serve_partial_content() {
        let (state, tmp) = state_with_bank().await;
        let app = router(state);
        let url = "/api/v1/storage/seed-vault/files/notes.txt";
        send_bytes(&app, "PUT", url, b"hello bank").await; // 10 bytes

        let res = send_with(&app, "GET", url, "range", "bytes=0-4").await;
        assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(res.headers()[axum::http::header::CONTENT_RANGE], "bytes 0-4/10");
        let bytes = axum::body::to_bytes(res.into_body(), 1000).await.unwrap();
        assert_eq!(&bytes[..], b"hello");

        let res = send_with(&app, "GET", url, "range", "bytes=-4").await;
        assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(res.headers()[axum::http::header::CONTENT_RANGE], "bytes 6-9/10");
        let bytes = axum::body::to_bytes(res.into_body(), 1000).await.unwrap();
        assert_eq!(&bytes[..], b"bank");

        let res = send_with(&app, "GET", url, "range", "bytes=5-").await;
        assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
        let bytes = axum::body::to_bytes(res.into_body(), 1000).await.unwrap();
        assert_eq!(&bytes[..], b" bank");

        let res = send_with(&app, "GET", url, "range", "bytes=100-200").await;
        assert_eq!(res.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(res.headers()[axum::http::header::CONTENT_RANGE], "bytes */10");

        let res = send_with(&app, "GET", url, "range", "bytes=nonsense").await;
        assert_eq!(res.status(), StatusCode::OK, "malformed spec is ignored");
        let bytes = axum::body::to_bytes(res.into_body(), 1000).await.unwrap();
        assert_eq!(&bytes[..], b"hello bank");

        let res = send(&app, "GET", url).await;
        assert_eq!(
            res.headers()[axum::http::header::ACCEPT_RANGES],
            "bytes",
            "full answers advertise resumability"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Rename without re-upload: content rides, the old path vanishes,
    /// overwrites refuse (a move never clobbers), and the escape gate
    /// guards both endpoints.
    #[tokio::test]
    async fn move_renames_within_the_bank() {
        let (state, tmp) = state_with_bank().await;
        let app = router(state);
        let base = "/api/v1/storage/seed-vault/files";
        send_bytes(&app, "PUT", &format!("{base}/dumps/a.txt"), b"payload").await;

        let mut res = send_json(
            &app,
            "PATCH",
            &format!("{base}/dumps/a.txt"),
            serde_json::json!({ "move_to": "dumps/b.txt" }),
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
        let v = body_json(&mut res).await;
        assert_eq!(v["data"]["moved"], true);

        let res = send(&app, "GET", &format!("{base}/dumps/b.txt")).await;
        assert_eq!(res.status(), StatusCode::OK, "the content rides");
        let res = send(&app, "GET", &format!("{base}/dumps/a.txt")).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND, "the old path is gone");

        // A real collision: keep.txt exists, so b.txt may not take its name.
        send_bytes(&app, "PUT", &format!("{base}/dumps/keep.txt"), b"keep").await;
        let res = send_json(
            &app,
            "PATCH",
            &format!("{base}/dumps/b.txt"),
            serde_json::json!({ "move_to": "dumps/keep.txt" }),
        )
        .await;
        assert_eq!(res.status(), StatusCode::CONFLICT, "never overwrites");
        let res = send(&app, "GET", &format!("{base}/dumps/keep.txt")).await;
        let bytes = axum::body::to_bytes(res.into_body(), 100).await.unwrap();
        assert_eq!(&bytes[..], b"keep", "the clobbered file is untouched");

        let res = send_json(
            &app,
            "PATCH",
            &format!("{base}/dumps/b.txt"),
            serde_json::json!({ "move_to": "../out.txt" }),
        )
        .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST, "escape still refuses");

        let res = send_json(
            &app,
            "PATCH",
            &format!("{base}/dumps/none.txt"),
            serde_json::json!({ "move_to": "elsewhere.txt" }),
        )
        .await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND, "nothing to move");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// HEAD rides the GET face: 200, no body — existence checks cost
    /// nothing (the transport strips it; the test pins the promise).
    #[tokio::test]
    async fn head_exists_answers_without_a_body() {
        let (state, tmp) = state_with_bank().await;
        let app = router(state);
        let base = "/api/v1/storage/seed-vault/files";
        send_bytes(&app, "PUT", &format!("{base}/a.txt"), b"xyz").await;

        let res = send(&app, "HEAD", &format!("{base}/a.txt")).await;
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(res.into_body(), 100).await.unwrap();
        assert!(bytes.is_empty(), "HEAD carries no body");
        let res = send(&app, "HEAD", &format!("{base}/none.txt")).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let _ = std::fs::remove_dir_all(&tmp);
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
                            state: ServiceState { status: "running".into(), role: None, mode: None },
                            ports: Default::default(),
                capabilities: Default::default(),
                        }],
                    }),
                    banks: Some(Inventory {
                        rev: Some(2),
                        total: None,
                        items: vec![garden_contract::chirp::BankEntry {
                            fqn: "seed-vault::default".into(),
                            device_id: "dev-peer".into(),
                            state: "mounted".into(),
                            roles: vec![garden_glossary::bank::role::SINK.into()],
                            capacity_bytes: Some(1_000_000),
                            used_bytes: Some(10),
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

