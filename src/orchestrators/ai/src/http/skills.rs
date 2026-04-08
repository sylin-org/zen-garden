//! HTTP endpoints for the skill subsystem (ORCH-0029 + ORCH-0030 §3).
//!
//! Endpoints:
//!
//! - `POST /v1/skills/{provider}/import` — accept either a JSON body
//!   with `{ "input": "<text>" }` or raw bytes (PNG upload with
//!   `Content-Type: image/png`). Runs the full import pipeline
//!   (input classify → extract → parse → param extract → resolve
//!   models → write draft), then fires AI naming asynchronously.
//!   Returns 202 with the moniker. The skill becomes visible to
//!   `GET /v1/skills` after the next ComfyUI hot-reload picks up
//!   the new file from disk.
//!
//! - `GET /v1/skills` — list every skill currently published by an
//!   enabled provider, optionally filtered by provider or primitive.
//!
//! - `GET /v1/skills/{skill_id}` — inspect a single skill by id.
//!   Returns every (provider, skill) match — typically one entry.
//!
//! - `DELETE /v1/skills/{skill_id}` — best-effort on-disk removal
//!   for the ComfyUI provider. Skills disappear from
//!   `GET /v1/skills` after the next ComfyUI hot-reload picks up
//!   the file deletion.
//!
//! # ORCH-0030 R2 M3 changes
//!
//! The legacy `Skills` aggregate has been deleted; ComfyUI owns its
//! loaded-skill state internally and publishes it via
//! `CapabilityAnnouncement.skills`. The HTTP surface reads from
//! [`crate::services::directory_subscriber::CapabilityDirectory`]
//! instead of the deleted aggregate. The import endpoint still
//! writes the draft to disk and runs the AI namer; immediate
//! in-memory registration is gone (a future commit will wire a
//! hot-reload signal from the import endpoint into ComfyUI).

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::app_state::AppState;
use crate::domain::ids::ProviderName;
use crate::domain::primitive::Primitive;
use crate::services::skills::import::{analyze, draft_builder, model_resolve, namer};

/// Request body for the JSON variant of `POST /v1/skills/{provider}/import`.
#[derive(Debug, Deserialize)]
pub struct ImportBody {
    /// Raw text input: a CivitAI URL, a raw workflow JSON, an A1111
    /// generation text dump, etc.
    pub input: String,
}

/// `POST /v1/skills/{provider}/import`
///
/// Two content-type modes:
/// - `application/json`: body is `{ "input": "<text>" }`
/// - `image/png` / `application/octet-stream`: raw PNG bytes
pub async fn post_import(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if provider != "comfyui" {
        return bail(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("skill import is only available for the `comfyui` provider (got `{provider}`)"),
        );
    }

    let content_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_lowercase();

    // Classify the input mode from the Content-Type header.
    let (input_text, input_bytes): (String, Option<Vec<u8>>) =
        if content_type.starts_with("application/json") {
            let parsed: ImportBody = match serde_json::from_slice(&body) {
                Ok(v) => v,
                Err(e) => {
                    return bail(
                        StatusCode::BAD_REQUEST,
                        "validation_failed",
                        format!("invalid JSON body: {e}"),
                    );
                }
            };
            (parsed.input, None)
        } else {
            // Binary — PNG uploads land here.
            (String::new(), Some(body.to_vec()))
        };

    // Data dir comes from the shared AppState — the same path the
    // loader scans and the cache + provisioner use.
    let data_dir = state.data_dir.clone();

    // Load the ComfyUI Manager registry once per import. A future
    // optimization would cache this globally on AppState.
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let manager = model_resolve::ManagerRegistry::fetch(&http).await;

    // ── Step 1: run the analyze pipeline ─────────────────────
    let result = match analyze::run(
        &http,
        &input_text,
        input_bytes.as_deref(),
        &data_dir,
        &manager,
        None, // civitai_token — Phase 4 wires this through AppState::secrets
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "skills import: analyze failed");
            // Publish a synthetic failure event with no moniker —
            // there's no skill to attach state to yet.
            state
                .events
                .publish(
                    "skills.import.failed",
                    &json!({
                        "stage": "analyze",
                        "reason": format!("{e:#}"),
                    }),
                )
                .await;
            return bail(
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation_failed",
                format!("{e:#}"),
            );
        }
    };

    let moniker_str = result.moniker.clone();
    let topic_state = format!("skills.{moniker_str}.state");

    // Publish: analyzing → confirmed (the analysis stage succeeded;
    // the next stages — draft write, naming — flow from here)
    state
        .events
        .publish(
            &topic_state,
            &json!({
                "moniker": moniker_str,
                "state": "analyzing",
                "primitive": result.primitive.dotted(),
                "display_name": result.display_name,
            }),
        )
        .await;

    // ── Step 2: write the draft to disk ──────────────────────
    let skills_dir = data_dir.join("skills");
    let draft_dir = match draft_builder::create_draft(&skills_dir, "comfyui", &result).await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "skills import: draft write failed");
            state
                .events
                .publish(
                    &topic_state,
                    &json!({
                        "moniker": moniker_str,
                        "state": "failed",
                        "stage": "draft_write",
                        "reason": format!("{e:#}"),
                    }),
                )
                .await;
            return bail(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                format!("draft write: {e:#}"),
            );
        }
    };

    // Publish: ready (the draft is on disk; ComfyUI hot-reload
    // will pick it up on the next sweep, or operator restart will
    // catch it. Naming may still finish in the background.)
    state
        .events
        .publish(
            &topic_state,
            &json!({
                "moniker": moniker_str,
                "state": "ready",
                "primitive": result.primitive.dotted(),
                "models": result.models.len(),
                "draft_dir": draft_dir.display().to_string(),
            }),
        )
        .await;

    // ── Step 3: fire AI naming in the background ─────────────
    //
    // Best-effort. The import response has already returned the
    // moniker; the namer rewrites the draft on disk and emits a
    // `skills.{moniker}.named` event when it finishes.
    spawn_async_namer(
        state.clone(),
        moniker_str.clone(),
        namer_context_from_result(&result),
    );

    // ── Step 4: return 202 + Location + thin body ───────────
    //
    // The body is intentionally minimal: clients fetch the full
    // metadata via `GET /v1/skills/{moniker}` and watch for
    // updates on `/v1/events?focus=skills.{moniker}.*`.
    let location = format!("/v1/skills/{moniker_str}");
    let topic_focus = format!("skills.{moniker_str}.*");
    let body = json!({
        "moniker": moniker_str,
        "primitive": result.primitive.dotted(),
        "draft_dir": draft_dir.display().to_string(),
        "links": {
            "self": &location,
            "events": format!("/v1/events?focus={topic_focus}"),
        }
    });
    let mut resp = (StatusCode::ACCEPTED, Json(body)).into_response();
    if let Ok(loc_value) = axum::http::HeaderValue::from_str(&location) {
        resp.headers_mut().insert(axum::http::header::LOCATION, loc_value);
    }
    resp
}

/// Build the naming context from the analyze result.
fn namer_context_from_result(result: &analyze::AnalyzeResult) -> namer::NamingContext {
    let (prompt, negative_prompt, steps, cfg_scale, sampler, width, height) = result
        .generation
        .as_ref()
        .map(|g| {
            (
                g.prompt.clone(),
                g.negative_prompt.clone(),
                g.steps,
                g.cfg_scale,
                g.sampler.clone(),
                None, // width/height aren't on GenerationSummary
                None,
            )
        })
        .unwrap_or_default();

    let model_names: Vec<String> = result
        .models
        .iter()
        .map(|m| match m {
            model_resolve::ModelResolution::Resolved { filename, .. }
            | model_resolve::ModelResolution::Cached { filename, .. }
            | model_resolve::ModelResolution::AuthRequired { filename, .. }
            | model_resolve::ModelResolution::Unresolved { filename, .. } => filename.clone(),
        })
        .collect();

    namer::NamingContext {
        prompt,
        negative_prompt,
        model_names,
        steps,
        cfg_scale,
        sampler,
        width,
        height,
    }
}

fn spawn_async_namer(
    state: AppState,
    moniker: String,
    ctx: namer::NamingContext,
) {
    tokio::spawn(async move {
        let Some(naming) = namer::generate_name(&state.dispatcher, &ctx).await else {
            tracing::debug!(
                moniker = %moniker,
                "skills import: AI naming skipped (no chat provider available or request failed)"
            );
            // Publish a soft notice so dashboards stop spinning the
            // "naming…" indicator without falsely claiming a name.
            state
                .events
                .publish(
                    format!("skills.{moniker}.named"),
                    &json!({
                        "moniker": moniker,
                        "skipped": true,
                        "reason": "no chat provider available or request failed",
                    }),
                )
                .await;
            return;
        };

        // Publish the lifecycle event. Subscribers focused on
        // `skills.{moniker}.named` get a real-time push.
        //
        // ORCH-0030 R2 M3: the legacy `Skills::rename` call is gone.
        // The naming is recorded in the event payload; future hot-
        // reload work will rewrite the on-disk draft to make the new
        // name persistent.
        state
            .events
            .publish(
                format!("skills.{moniker}.named"),
                &json!({
                    "moniker": moniker,
                    "display_name": naming.name,
                    "description": naming.description,
                }),
            )
            .await;
    });
}

fn bail(status: StatusCode, code: &str, message: String) -> Response {
    let body: Value = json!({
        "error": {
            "code": code,
            "message": message,
        }
    });
    (status, Json(body)).into_response()
}

// ── Skill noun surface (ORCH-0030 §3) ─────────────────────────

#[derive(Debug, Deserialize)]
pub struct ListSkillsQuery {
    pub provider: Option<String>,
    pub primitive: Option<String>,
}

/// `GET /v1/skills` — list skills currently published by an enabled
/// provider via the `CapabilityDirectory`.
///
/// Optional query parameters:
/// - `provider` — exact provider name match (e.g. `comfyui`)
/// - `primitive` — dotted primitive id (e.g. `image.generate`)
pub async fn list_skills(
    State(state): State<AppState>,
    Query(q): Query<ListSkillsQuery>,
) -> Response {
    let primitive_filter = match q.primitive.as_deref() {
        None => None,
        Some(s) => match Primitive::parse_dotted(s) {
            Ok(p) => Some(p),
            Err(_) => {
                return bail(
                    StatusCode::BAD_REQUEST,
                    "validation_failed",
                    format!("unknown primitive `{s}`"),
                );
            }
        },
    };

    let provider_filter = q.provider.as_deref().map(ProviderName::new);

    let all = state.capability_directory.all_skills().await;
    let mut entries: Vec<Value> = Vec::new();
    for (provider, skill) in all {
        if let Some(p) = &provider_filter {
            if &provider != p {
                continue;
            }
        }
        if let Some(p) = primitive_filter {
            if skill.primitive != p {
                continue;
            }
        }
        let entry = json!({
            "provider": provider.as_str(),
            "id": skill.id,
            "primitive": skill.primitive.dotted(),
            "display": {
                "name": skill.display.name,
                "description": skill.display.description,
                "tags": skill.display.tags,
                "preview_image": skill.display.preview_image,
            },
            "parameters": skill.parameters,
        });
        entries.push(entry);
    }

    // Stable order: provider asc, then id asc.
    entries.sort_by(|a, b| {
        let ap = a.get("provider").and_then(|v| v.as_str()).unwrap_or("");
        let bp = b.get("provider").and_then(|v| v.as_str()).unwrap_or("");
        let primary = ap.cmp(bp);
        if primary != std::cmp::Ordering::Equal {
            return primary;
        }
        let ai = a.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let bi = b.get("id").and_then(|v| v.as_str()).unwrap_or("");
        ai.cmp(bi)
    });

    let body = json!({
        "version": state.capability_directory.version(),
        "count": entries.len(),
        "skills": entries,
    });
    (StatusCode::OK, Json(body)).into_response()
}

/// `GET /v1/skills/{skill_id}` — inspect a single skill.
///
/// Skills are uniquely keyed by `(provider, skill_id)`. When the
/// same id is published under multiple providers (rare in practice
/// today; ComfyUI is the only loader) the response includes every
/// matching entry, with the consumer expected to disambiguate via
/// the `provider` field. The most common case (one match) returns
/// a single entry.
pub async fn get_skill(
    State(state): State<AppState>,
    Path(skill_id): Path<String>,
) -> Response {
    let all = state.capability_directory.all_skills().await;
    let mut matches: Vec<Value> = Vec::new();
    for (provider, skill) in all {
        if skill.id == skill_id {
            matches.push(json!({
                "provider": provider.as_str(),
                "id": skill.id,
                "primitive": skill.primitive.dotted(),
                "display": {
                    "name": skill.display.name,
                    "description": skill.display.description,
                    "tags": skill.display.tags,
                    "preview_image": skill.display.preview_image,
                },
                "parameters": skill.parameters,
            }));
        }
    }

    if matches.is_empty() {
        return bail(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("no skill registered with id `{skill_id}`"),
        );
    }

    if matches.len() == 1 {
        return (StatusCode::OK, Json(matches.into_iter().next().unwrap()))
            .into_response();
    }

    // Multi-match (different providers): return both as a list.
    let body = json!({
        "id": skill_id,
        "matches": matches,
    });
    (StatusCode::OK, Json(body)).into_response()
}

/// `DELETE /v1/skills/{skill_id}` — best-effort on-disk removal of
/// the ComfyUI skill directory at
/// `{data_dir}/skills/comfyui/{skill_id}/`. ComfyUI's hot-reload
/// will drop the skill from its in-memory state on the next sweep
/// (or operator restart will catch it).
///
/// Returns the list of paths that were removed.
pub async fn delete_skill(
    State(state): State<AppState>,
    Path(skill_id): Path<String>,
) -> Response {
    // Validate the path segment is sane (no traversal).
    if skill_id.contains('/') || skill_id.contains('\\') || skill_id.contains("..") {
        return bail(
            StatusCode::BAD_REQUEST,
            "validation_failed",
            format!("invalid skill id `{skill_id}`"),
        );
    }

    let disk_dir = state
        .data_dir
        .join("skills")
        .join("comfyui")
        .join(&skill_id);

    if !disk_dir.exists() {
        return bail(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("no skill directory found at `{}`", disk_dir.display()),
        );
    }

    if let Err(e) = tokio::fs::remove_dir_all(&disk_dir).await {
        tracing::warn!(
            error = %e,
            path = %disk_dir.display(),
            "skills delete: on-disk removal failed"
        );
        return bail(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            format!("on-disk removal failed: {e}"),
        );
    }

    // Publish lifecycle event so dashboards refresh.
    state
        .events
        .publish(
            format!("skills.{skill_id}.state"),
            &json!({
                "id": skill_id,
                "state": "removed",
                "path": disk_dir.display().to_string(),
            }),
        )
        .await;

    let body = json!({
        "id": skill_id,
        "removed_path": disk_dir.display().to_string(),
        "note": "in-memory state will update after the next ComfyUI hot-reload or restart",
    });
    (StatusCode::OK, Json(body)).into_response()
}
