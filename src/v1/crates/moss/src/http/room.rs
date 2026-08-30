//! The room's faces: the stone among stones — presence, the garden strip, the pulse.

use super::*;

pub async fn health(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
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


pub async fn posture(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let uptime = chrono::Utc::now() - state.started_at;
    let dispatch = state.dispatcher.stats();
    let (capture_tracked, capture_failed) = state.capture.run_stats();
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
            "capture": {
                "tracked": capture_tracked,
                "failed": capture_failed
            },
        }
    }))
}

/// The SelfView (ADR-0004 §3): self is rebuilt, never stored. The stone's
/// own frame, re-voiced with its full inventory — one composer, many
/// mouths (B1).
pub(crate) fn self_view(state: &AppState) -> serde_json::Value {
    let mut body = state.chirp_source.body();
    body.inventory =
        garden_contract::chirp::InventoryMap::from_pairs(state.chirp_source.song_blocks());
    serde_json::to_value(&body).unwrap_or_default()
}


pub async fn stone_self(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "data": self_view(&state) }))
}

/// `/stone/{ref}` — the garden's only true redirect (ADR-0004 §4): mine
/// answered here; a peer's is a not-here answer carrying its home address
/// (Location header + `knows_at`), because reads delegate and writes bind
/// at their authority. Unknown names are a plain 404.

pub async fn stone_ref(State(state): State<Arc<AppState>>, Path(reference): Path<String>) -> Response {
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


pub async fn garden_stones(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
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

pub async fn catalog(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
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

/// Local storage (L22): this stone's banks and the volumes ready for
/// adoption.

pub async fn garden_storage(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let mut rows: Vec<serde_json::Value> = Vec::new();
    for bank in state.storage.banks() {
        rows.push(serde_json::json!({
            "self": true,
            "stone": state.stone_name,
            "bank": bank,
        }));
    }
    for peer in state.topology.snapshot() {
        let Some(banks) = &peer.body.inventory.banks else {
            continue; // absent key: the stone says nothing about banks
        };
        for bank in &banks.items {
            rows.push(serde_json::json!({
                "stone": peer.body.stone.name,
                "stone_id": peer.body.stone.id,
                "bank": bank,
            }));
        }
    }
    Json(serde_json::json!({ "data": { "banks": rows } }))
}

/// Run a will (1:1 with `rake capture {name}`): Phase A imprint, then
/// pack/ferry/commit in the background. The response carries the run; ask
/// again on the GET face for its progress.

pub async fn portrait(State(state): State<Arc<AppState>>) -> axum::response::Html<String> {
    let frame = state.chirp_source.body();
    let offerings = state.garden.snapshot();
    let peers = state.topology.snapshot();
    let uptime = chrono::Utc::now() - state.started_at;

    let mut offering_rows = String::new();
    if offerings.is_empty() {
        offering_rows.push_str("<div><em>nothing planted yet</em></div>");
    } else {
        offering_rows.push_str("<table><tr><th>offering</th><th>status</th><th>home</th></tr>");
        for o in &offerings {
            let home = o
                .managed()
                .and_then(|m| m.port_map.values().next())
                .map(|p| p.to_string())
                .unwrap_or_else(|| "-".into());
            offering_rows.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
                html_escape(&o.name),
                html_escape(o.status.as_str()),
                html_escape(&home)
            ));
        }
        offering_rows.push_str("</table>");
    }

    let banks = state.storage.banks();
    let bank_rows = if banks.is_empty() {
        "<div><em>no banks adopted</em></div>".to_string()
    } else {
        let mut s = String::from("<table><tr><th>bank</th><th>state</th></tr>");
        for b in &banks {
            s.push_str(&format!(
                "<tr><td>{}</td><td>{}</td></tr>",
                html_escape(&b.fqn),
                html_escape(&b.state)
            ));
        }
        s.push_str("</table>");
        s
    };

    let mut peer_rows = String::new();
    for p in &peers {
        peer_rows.push_str(&format!("<div>{}</div>", html_escape(&p.body.stone.name)));
    }
    if peer_rows.is_empty() {
        peer_rows.push_str("<div><em>the room is quiet</em></div>");
    }

    let page = include_str!("../../assets/portrait.html")
        .replace("__STONE_NAME__", &html_escape(&state.stone_name))
        .replace("__MOSS_VERSION__", &html_escape(&frame.stone.moss.version))
        .replace("__HEALTH__", &html_escape(&frame.presence.health))
        .replace("__STONE_ID__", &html_escape(&frame.stone.id))
        .replace(
            "__ADDRESS__",
            &html_escape(&format!(
                "{}:{}",
                frame.stone.network.address.ip, frame.stone.network.address.port
            )),
        )
        .replace("__BOOT_ID__", &html_escape(&state.boot_id.to_string()))
        .replace("__UPTIME__", &format!("{}s", uptime.num_seconds()))
        .replace("__OFFERING_COUNT__", &offerings.len().to_string())
        .replace("__OFFERINGS__", &offering_rows)
        .replace("__BANK_COUNT__", &banks.len().to_string())
        .replace("__BANKS__", &bank_rows)
        .replace("__PEER_COUNT__", &peers.len().to_string())
        .replace("__PEERS__", &peer_rows);
    axum::response::Html(page)
}

/// The root lands on the portrait (the stone's face is its front door).

pub async fn root() -> axum::response::Redirect {
    axum::response::Redirect::temporary("/portrait")
}


pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The pulse page: the live view. Connects to /pulse/stream, seeds itself
/// from /garden/stones.

pub async fn pulse_page() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("../../assets/pulse.html"))
}

/// The SSE firehose (L18 at the edge): topology events and offering
/// changes, merged into one stream. Each connection holds its own
/// receivers; events are JSON, keep-alives keep proxies honest.
/// Pulse stream query: an optional comma-separated category filter
/// ("offering,topology,job,storage,stone,wire").
#[derive(serde::Deserialize, Default)]

pub struct PulseQuery {
    categories: Option<String>,
}

/// Frame one pulse event as SSE; the category filter drops silently.

pub fn pulse_sse(
    ev: &garden_contract::pulse::PulseEvent,
    filter: Option<&Vec<String>>,
) -> Option<axum::response::sse::Event> {
    if let Some(allowed) = filter
        && !allowed.iter().any(|c| c == &ev.category)
    {
        return None;
    }
    let data = serde_json::to_string(ev).ok()?;
    Some(
        axum::response::sse::Event::default()
            .id(ev.seq.to_string())
            .event(ev.kind.clone())
            .data(data),
    )
}

/// The pulse (ADR-0013): snapshot first — the world as this stone sees
/// it — then typed, seq'd events. A gap between the SSE `id`s is missed
/// news, said out loud via `pulse.lagged`.

pub async fn pulse_stream(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PulseQuery>,
) -> axum::response::Response {
    let filter: Option<Vec<String>> = query.categories.map(|c| {
        c.split(',').map(str::trim).map(String::from).collect()
    });
    let rx = state.pulse.subscribe();
    // Subscribe BEFORE the snapshot: nothing is missed, and the snapshot
    // is the newer truth. seq 0 marks it as the opener.
    let mut snapshot = garden_contract::pulse::PulseEvent::new(
        "snapshot",
        "pulse",
        garden_contract::pulse::LEVEL_INFO,
        "the world as this stone sees it",
    )
    .with_data(crate::pulse::snapshot(
        &state.garden,
        &state.topology,
        &state.jobs,
        self_view(&state),
    ));
    snapshot.seq = 0;

    let stream = futures::stream::unfold(
        (rx, filter, Some(snapshot)),
        |(mut rx, filter, mut snapshot)| async move {
            if let Some(ev) = snapshot.take()
                && let Some(frame) = pulse_sse(&ev, filter.as_ref())
            {
                return Some((Ok::<_, std::convert::Infallible>(frame), (rx, filter, None)));
            }
            loop {
                match rx.recv().await {
                    Ok(ev) => {
                        if let Some(frame) = pulse_sse(&ev, filter.as_ref()) {
                            return Some((Ok(frame), (rx, filter, None)));
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        let lag = garden_contract::pulse::PulseEvent::new(
                            "pulse.lagged",
                            "pulse",
                            garden_contract::pulse::LEVEL_WARN,
                            format!("the stream ran {n} events behind - some news was dropped"),
                        );
                        if let Some(frame) = pulse_sse(&lag, filter.as_ref()) {
                            return Some((Ok(frame), (rx, filter, None)));
                        }
                    }
                    Err(_) => continue,
                }
            }
        },
    );

    // The stream ends on shutdown: an open SSE connection must not hold
    // the graceful drain hostage — the farewell waits on nothing.
    let stop = state.shutdown.clone().cancelled_owned();
    axum::response::IntoResponse::into_response(
        axum::response::Sse::new(futures::StreamExt::take_until(stream, stop))
            .keep_alive(axum::response::sse::KeepAlive::default()),
    )
}

/// Every tracked async operation, newest first.

pub async fn job_list(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let jobs = state.jobs.list();
    Json(serde_json::json!({ "data": { "jobs": jobs } }))
}

/// One job by id.

pub async fn job_detail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .jobs
        .get(&id)
        .map(|j| Json(serde_json::json!({ "data": { "job": j } })))
        .ok_or_else(|| {
            ApiError(CommandError::NotFound(format!("no job '{id}' on this stone")))
        })
}


pub async fn front_door() -> Json<serde_json::Value> {
    let routes: Vec<serde_json::Value> = garden_contract::faces::FACES
        .iter()
        .map(|face| {
            serde_json::json!({
                "method": face.method,
                "path": face.path,
                "summary": face.summary,
            })
        })
        .collect();
    Json(serde_json::json!({ "data": { "routes": routes } }))
}

// ---- offerings (L22) — thin delegation to the application service ---------
