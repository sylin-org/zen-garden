//! The offering faces: plant and plan, rest and wake, capture and replant, nourish.

use super::*;
use serde::Deserialize;

const REHEARSE_WAIT_SECS: u64 = 15;

/// Shared state behind the routes.

pub async fn capture_offer(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult {
    use crate::garden::will::{readiness, Readiness};
    use crate::garden::will::RunInfo;

    let fqn = garden_glossary::fqn::canonicalize(&name)
        .map_err(|e| CommandError::Conflict(e.to_string()))?;
    let offering = state
        .garden
        .placed(&fqn)
        .ok_or(CommandError::NotFound(format!("'{}' is not planted here", fqn)))?;

    // The will lives in the catalog manifest (one machine-truth parse).
    let manifest = state.garden.catalog.get(&offering.offering).ok_or_else(|| {
        CommandError::Conflict(format!(
            "'{fqn}' grows from stem '{}' with no catalog manifest - its will cannot be read",
            offering.offering
        ))
    })?;
    match readiness(manifest) {
        Readiness::NothingToPreserve => {}
        Readiness::Trusted(_) => {}
        Readiness::Untrusted => {
            return Err(CommandError::Conflict(format!(
                "'{}' declares volumes but no capture policy - raw copy would be a lie;                  declare a `capture:` section in the manifest first",
                fqn
            ))
            .into())
        }
    }
    let Some(policy) = &manifest.capture else {
        return Err(CommandError::Conflict(format!(
            "'{}' declares no capture policy and no volumes - nothing to preserve",
            fqn
        ))
        .into())
    };

    let workload =
        crate::garden::will::workload_for(&offering, &state.garden.dirs_root);

    let runner = Arc::clone(&state.capture);
    let policy = policy.clone();
    let fqn_str = fqn.clone();
    let run_id = uuid::Uuid::now_v7().to_string();
    let mut announced = RunInfo {
        fqn: fqn_str.clone(),
        run_id: run_id.clone(),
        started_at: chrono::Utc::now(),
        phase: "imprint".into(),
        error: None,
        checkpoint: None,
        ferried_to: None,
    };
    state.capture.announce(announced.clone());

    // Track the capture as a job (the data plane's async contract).
    let job_id = state.jobs.start(crate::jobs::kind::CAPTURE, &fqn_str);
    let job_id_resp = job_id.clone();

    let task_fqn = fqn_str.clone();
    let task_run = run_id.clone();
    let jobs = state.jobs.clone();
    tokio::spawn(async move {
        match runner
            .execute_named(&task_fqn, &policy, &workload, &task_run)
            .await
        {
            Ok(checkpoint) => {
                jobs.complete(
                    &job_id,
                    serde_json::json!({
                        "checkpoint": checkpoint.display().to_string(),
                    }),
                );
            }
            Err(e) => {
                jobs.fail(&job_id, &e);
                tracing::warn!(offering = %task_fqn, error = %e, "capture run failed");
            }
        }
    });
    announced.phase = "accepted".into();
    Ok(Json(
        serde_json::json!({ "data": { "run": announced }, "job_id": job_id_resp }),
    ))
}

// ---- offering logs: the live voice of what was planted ---------------------

/// Which peer's song carries this offering — the logs redirect's
/// addressee. Offerings grow where they were planted; the room's cache
/// knows who sings about them.

pub fn service_holder(state: &AppState, fqn: &str) -> Option<String> {
    for peer in state.topology.snapshot() {
        let Some(services) = &peer.body.inventory.services else {
            continue;
        };
        if services.items.iter().any(|s| s.name == fqn) {
            let address = &peer.body.stone.network.address;
            return Some(format!(
                "http://{}:{}/api/v1/stone",
                address.ip, address.port
            ));
        }
    }
    None
}

/// The not-here answer for an offering that grows elsewhere (1:1 with
/// the bank files' redirect): 404, a Location header, `knows_at`.

pub fn offering_not_here(state: &AppState, fqn: &str) -> axum::response::Response {
    match service_holder(state, fqn) {
        Some(knows_at) => (
            axum::http::StatusCode::NOT_FOUND,
            [(axum::http::header::LOCATION, knows_at.clone())],
            Json(serde_json::json!({
                "error": {
                    "not_here": true,
                    "offering": fqn,
                    "knows_at": knows_at,
                    "message": "That offering does not grow here. Its home stone answers at \
                                `knows_at` - logs grow where the offering grows."
                }
            })),
        )
            .into_response(),
        None => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": {
                    "message": format!("'{fqn}' is not planted here, and the room's cache knows \
                                        no home for it - rake list shows what this stone hosts")
                }
            })),
        )
            .into_response(),
    }
}

/// Follow an offering's logs: history first (tail=N bounds it), then
/// live — SSE `log` events, one JSON LogLine each. The stream ends when
/// the client leaves or the container stops.

pub async fn offering_logs_stream(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> axum::response::Response {
    let fqn = garden_glossary::fqn::canonicalize(&name).unwrap_or_else(|_| name.clone());
    if state.garden.placed(&fqn).is_none() {
        return offering_not_here(&state, &fqn);
    }
    let tail = params.get("tail").and_then(|t| t.parse::<u64>().ok());
    let timestamps = params
        .get("timestamps")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);
    let Some(stream) = state.garden.logs_stream(&fqn, tail, timestamps) else {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": { "message": format!("'{fqn}' grows in a world that cannot stream logs") }
            })),
        )
            .into_response();
    };
    use futures::StreamExt as _;
    let events = stream.map(|item| {
        let event = match item {
            Ok(line) => axum::response::sse::Event::default()
                .event("log")
                .data(serde_json::to_string(&line).unwrap_or_default()),
            Err(e) => axum::response::sse::Event::default()
                .event("error")
                .data(
                    serde_json::to_string(&serde_json::json!({ "message": e }))
                        .unwrap_or_default(),
                ),
        };
        Ok::<_, std::convert::Infallible>(event)
    });
    // The stream ends on shutdown: the drain must finish for the farewell.
    let stop = state.shutdown.clone().cancelled_owned();
    axum::response::Sse::new(futures::StreamExt::take_until(events, stop))
        .keep_alive(axum::response::sse::KeepAlive::default())
        .into_response()
}

/// The last capture run of an offering.

pub async fn capture_last(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> axum::response::Response {
    let fqn = garden_glossary::fqn::canonicalize(&name).unwrap_or_else(|_| name.clone());
    // A foreign offering has no "no runs" answer to give — it answers the
    // garden's redirect like every other offering face (reads delegate).
    if state.garden.placed(&fqn).is_none() {
        return offering_not_here(&state, &fqn);
    }
    match state.capture.last_run(&fqn) {
        Some(run) => Json(serde_json::json!({ "data": { "run": run } })).into_response(),
        None => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": { "message": format!("'{fqn}' has run no capture on this stone") }
            })),
        )
            .into_response(),
    }
}

/// The collection: every offering placed on this stone.
pub async fn offerings_list(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let rows: Vec<serde_json::Value> =
        state.garden.snapshot().iter()
        .map(record_view).collect();
    Json(serde_json::json!({ "data": { "offerings": rows } }))
}

#[derive(Debug, Deserialize)]
pub struct ReplantRequest {
    /// The checkpoint run to restore; absent = the newest.
    #[serde(default)]
    run: Option<String>,
}

/// Replant (1:1 with `rake replant`): select -> verify -> restore the
/// directory -> place from the stored spec. The audit chain opens with
/// Replanted{predecessor_offering_id, final_hash}.

pub async fn replant_offer(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    body: Option<Json<ReplantRequest>>,
) -> ApiResult {
    let fqn = garden_glossary::fqn::canonicalize(&name)
        .map_err(|e| CommandError::Conflict(e.to_string()))?;
    let run = body.as_ref().and_then(|Json(req)| req.run.clone());
    // The pipeline is the will's; the face translates one call.
    let job_id = state.jobs.start(crate::jobs::kind::REPLANT, &fqn);
    let replanted = state.capture.replant_from(
        &fqn,
        run.as_deref(),
        &state.garden,
        &state.topology,
    ).await;
    let (checkpoint, count, final_hash, offering) = match replanted {
        Ok(v) => {
            state.jobs.complete(&job_id, serde_json::json!({ "fqn": fqn }));
            v
        }
        Err(e) => {
            state.jobs.fail(&job_id, &e);
            return Err(CommandError::Conflict(e).into());
        }
    };
    tracing::info!(offering = %fqn, from = %checkpoint.display(), files = count, "replanted");
    Ok(Json(
        serde_json::json!({ "data": { "offering": {
            "name": offering.name,
            "status": offering.status.as_str(),
            "replanted_from": checkpoint.display().to_string(),
            "final_hash": final_hash,
        } } }),
    ))
}

/// The stone's portrait: the landing page, rendered from the SelfView and
#[derive(Debug, Deserialize)]
pub struct PlantRequest {
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


pub fn default_category() -> String {
    "misc".into()
}


pub async fn plant_offering(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<PlantRequest>,
) -> ApiResult {
    // Install runs as a JOB (ADR-0015): plan, place, start — steps and
    // progress ride the pulse; the job id rides the answer.
    let (offering, job_id) = state
        .provenance()
        .install(
            &name,
            req.image,
            req.ports.into_iter().collect(),
            Some(req.category),
            req.runtime.as_deref(),
            &req.inputs,
            Some(&state.jobs),
        )
        .await?;
    Ok(Json(serde_json::json!({
        "data": { "offering": record_view(&offering) },
        "job_id": job_id,
    })))
}

/// The dry twin of plant (ADR-0015): same decision path, nothing
/// placed. The answer says can/cannot and WHY — the decision trail.

pub async fn plan_install(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<PlantRequest>,
) -> ApiResult {
    let plan = state.provenance().plan_install(&name, req.image, &req.inputs)?;
    Ok(Json(serde_json::json!({ "data": { "plan": plan } })))
}

/// Offerings render the sectioned record — disk and HTTP speak one shape
/// (R3.9, B1; S5.5).

pub fn record_view(offering: &crate::garden::model::Offering) -> serde_json::Value {
    serde_json::to_value(crate::garden::record::OfferingRecord::from_domain(offering))
        .unwrap_or_default()
}

/// §5.3: the placed record with its plan attached. Off-grammar names
/// refuse loudly here too — a tag-shaped read is an identity question,
/// not a quiet miss.
/// What the offering holds (W1): observed LIVE through its manifest's
/// list channel, and the record refreshed so chirps answer wishes from
/// fresh truth. Read-only — discovery never operates on the workload.

pub async fn offering_capabilities(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult {
    let fqn = garden_glossary::fqn::canonicalize(&name)
        .map_err(|e| CommandError::Conflict(e.to_string()))?;
    let offering = state.garden.placed(&fqn).ok_or(CommandError::NotFound(fqn.clone()))?;
    match crate::garden::capabilities::discover(&state.garden, &offering).await {
        Ok(map) => {
            if map != offering.sub_capabilities {
                let mut fresh = offering.clone();
                fresh.sub_capabilities = map.clone();
                fresh.updated_at = chrono::Utc::now();
                state.garden.registry().replace(fresh);
            }
            Ok(Json(serde_json::json!({
                "data": { "offering": fqn, "capabilities": map },
            })))
        }
        Err(crate::garden::capabilities::DiscoverError::Unsupported(m)) => {
            Err(ApiError(CommandError::Conflict(m)))
        }
        Err(e) => Err(ApiError(CommandError::Conflict(e.to_string()))),
    }
}

/// Grow one capability item (W2): validated and journaled by the domain;
/// the answer is the job id — check /api/v1/jobs/{id} or just re-ask the
/// wish when it goes green.

pub async fn capability_add(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    body: Option<Json<serde_json::Value>>,
) -> ApiResult {
    let Json(body) = body.ok_or_else(|| {
        ApiError(CommandError::BadRequest(
            "grow one item: {\"type\": \"model\", \"item\": \"llama3\"}".into(),
        ))
    })?;
    let kind = body["type"].as_str().ok_or_else(|| {
        ApiError(CommandError::BadRequest("body needs a \"type\" (the capability type) and an \"item\" (its name)".into()))
    })?;
    let item = body["item"].as_str().ok_or_else(|| {
        ApiError(CommandError::BadRequest("body needs a \"type\" and an \"item\"".into()))
    })?;
    let job_id = crate::garden::capabilities::grow(
        Arc::clone(&state.garden),
        state.jobs.clone(),
        &name,
        kind,
        item,
    )
    .map_err(|e| ApiError(CommandError::Conflict(e.to_string())))?;
    Ok(Json(
        serde_json::json!({ "data": { "accepted": true, "job_id": job_id } }),
    ))
}

/// Remove one capability item (W2): the trust law and journaling are the
/// domain's; this face only carries the wire.

pub async fn capability_remove(
    State(state): State<Arc<AppState>>,
    Path((name, kind, item)): Path<(String, String, String)>,
) -> ApiResult {
    let job_id = crate::garden::capabilities::prune(
        Arc::clone(&state.garden),
        state.jobs.clone(),
        &name,
        &kind,
        &item,
    )
    .map_err(|e| ApiError(CommandError::Conflict(e.to_string())))?;
    Ok(Json(
        serde_json::json!({ "data": { "accepted": true, "job_id": job_id } }),
    ))
}


pub async fn show_offering(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult {
    let fqn = garden_glossary::fqn::canonicalize(&name)
        .map_err(|e| CommandError::Conflict(e.to_string()))?;
    match state.garden.placed(&fqn) {
        Some(o) => {
            let capture = capture_view(&state, &o);
            Ok(Json(
                serde_json::json!({ "data": { "offering": record_view(&o), "capture": capture } }),
            ))
        }
        None => Err(CommandError::NotFound(fqn).into()),
    }
}

/// Restore rehearsal (J2): boot the newest checkpoint in isolation and
/// report green/red. The proof never touches the live offering, never
/// publishes a port, and never lingers — container and scratch removed
/// whatever the verdict.

pub async fn rehearse_offer(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult {
    let fqn = garden_glossary::fqn::canonicalize(&name)
        .map_err(|e| CommandError::Conflict(e.to_string()))?;
    let offering = state
        .garden
        .placed(&fqn)
        .ok_or(CommandError::NotFound(format!("'{}' is not planted here", fqn)))?;
    let Some(managed) = offering.managed() else {
        return Err(ApiError(CommandError::Conflict(format!(
            "'{fqn}' is not managed by the garden - rehearsal replays a capture, and adopted work has none"
        ))));
    };

    let world = state.garden.world_for(&offering)?;
    let spec = managed.spec.clone();
    let deps = crate::garden::rehearse::RehearsalDeps {
        world,
        select_checkpoint: {
            let runner = Arc::clone(&state.capture);
            let fqn = fqn.clone();
            Box::new(move |_| runner.select_checkpoint(&fqn, None))
        },
        restore_into: {
            let runner = Arc::clone(&state.capture);
            Box::new(move |cp, dir| runner.restore_into(cp, dir))
        },
    };
    let scratch_root = state.capture.workspace_root().join("rehearsals");
    let report = crate::garden::rehearse::rehearse(
        &fqn, &spec, deps, &scratch_root, REHEARSE_WAIT_SECS,
    ).await;
    Ok(Json(serde_json::json!({ "data": { "rehearsal": report } })))
}

/// The nourish check (J3): refresh the image reference and say whether
/// the tag would now run something different.

pub async fn update_check_face(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult {
    let fqn = garden_glossary::fqn::canonicalize(&name)
        .map_err(|e| CommandError::Conflict(e.to_string()))?;
    let refresh = state.garden.update_check(&fqn).await?;
    Ok(Json(serde_json::json!({ "data": {
        "name": fqn,
        "changed": refresh.changed,
        "image_id": refresh.id,
    } })))
}

/// The nourish apply (J3): pull the newer image and rebuild the container
/// from the stored spec. Volumes persist — data never moves. Refused
/// placements revert to the pre-pull image automatically.

pub async fn update_face(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult {
    let fqn = garden_glossary::fqn::canonicalize(&name)
        .map_err(|e| CommandError::Conflict(e.to_string()))?;
    let refresh = state.garden.update_offering(&fqn).await?;
    Ok(Json(serde_json::json!({ "data": {
        "name": fqn,
        "updated": refresh.changed,
        "image_id": refresh.id,
    } })))
}

/// The living will's surfacing for one offering (L3: never silent).
/// Readiness comes from the catalog manifest's declared policy; volumes
/// without a will are UNTRUSTED and say so.

pub fn capture_view(
    state: &AppState,
    offering: &crate::garden::model::Offering,
) -> serde_json::Value {
    let manifest = state.garden.catalog.get(&offering.offering);
    let declared = manifest.and_then(|m| m.capture.as_ref());
    let readiness = match (declared, offering.managed()) {
        (Some(_), _) => "trusted",
        (None, Some(m)) if !m.spec.volumes.is_empty() => "untrusted",
        _ => "nothing-to-preserve",
    };
    let mut v = serde_json::json!({ "readiness": readiness });
    if let Some(policy) = declared {
        v["mode"] = serde_json::json!(policy.mode.as_str());
        if policy.mode == crate::garden::will::CaptureMode::LockAndCopy {
            v["max_locked_s"] = serde_json::json!(policy.max_locked_s);
        }
    }
    if let Some(run) = state.capture.last_run(&offering.name) {
        v["last_run"] = serde_json::json!(run);
    }
    v
}


pub async fn rest_offering(State(state): State<Arc<AppState>>, Path(name): Path<String>) -> ApiResult {
    let offering = state.garden.rest(&name).await?;
    Ok(Json(serde_json::json!({
        "data": { "name": offering.name, "status": offering.status.as_str() }
    })))
}


pub async fn wake_offering(State(state): State<Arc<AppState>>, Path(name): Path<String>) -> ApiResult {
    let offering = state.garden.wake(&name).await?;
    let port_map = offering.managed().map(|m| m.port_map.clone()).unwrap_or_default();
    Ok(Json(serde_json::json!({
        "data": { "name": offering.name, "status": offering.status.as_str(), "port_map": port_map }
    })))
}


pub async fn uproot_offering(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult {
    state.garden.uproot(&name).await?;
    Ok(Json(serde_json::json!({ "data": { "name": name, "uprooted": true } })))
}
