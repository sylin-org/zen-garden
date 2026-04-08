//! HTTP endpoints for the skill subsystem (ORCH-0029 + ORCH-0030 §3).
//!
//! Endpoints:
//!
//! - `POST /v1/skills/{provider}/import` — accept either a JSON body
//!   with `{ "input": "<text>" }` or raw bytes (PNG upload with
//!   `Content-Type: image/png`). Runs the full import pipeline
//!   (input classify → extract → parse → param extract → resolve
//!   models → write draft), then fires AI naming asynchronously if
//!   a chat provider is available. Returns the full `AnalyzeResult`.
//!
//! - `GET /v1/skills` — list every loaded skill, optionally filtered
//!   by provider or primitive query parameter.
//!
//! - `GET /v1/skills/{moniker}` — inspect a single skill by moniker.
//!
//! - `DELETE /v1/skills/{moniker}` — remove a loaded skill (drops
//!   it from the aggregate; on-disk removal is left to a future
//!   commit because the disk path includes the provider segment).

use std::collections::HashMap;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::app_state::AppState;
use crate::domain::ids::ProviderName;
use crate::domain::moniker::Moniker;
use crate::domain::primitive::Primitive;
use crate::services::skills::import::{
    analyze, draft_builder, model_resolve, namer,
};
use crate::services::skills::registry::SkillKey;

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
            return bail(
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation_failed",
                format!("{e:#}"),
            );
        }
    };

    // ── Step 2: write the draft to disk ──────────────────────
    let skills_dir = data_dir.join("skills");
    let draft_dir = match draft_builder::create_draft(&skills_dir, "comfyui", &result).await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "skills import: draft write failed");
            return bail(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                format!("draft write: {e:#}"),
            );
        }
    };

    // ── Step 3: fire AI naming in the background ─────────────
    //
    // Best-effort. The import response returns immediately; the
    // namer updates the Skills aggregate (and, eventually, the
    // on-disk `skill.json`) when it finishes. Phase 4 wires the
    // name update back to disk; Phase 3 only updates the in-memory
    // aggregate via a `rename` call.
    spawn_async_namer(
        state.clone(),
        result.moniker.clone(),
        namer_context_from_result(&result),
    );

    // ── Step 4: return the analysis result ──────────────────
    let moniker = result.moniker.clone();
    let body = json!({
        "moniker": moniker,
        "draft_dir": draft_dir.display().to_string(),
        "result": result,
    });
    (StatusCode::CREATED, Json(body)).into_response()
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
            return;
        };
        // Locate the skill in the aggregate and update its metadata.
        // The key requires a typed Moniker; if the moniker string
        // fails validation (unlikely — it came from our own
        // generator) we just log and drop the update.
        let Ok(moniker_typed) = crate::domain::moniker::Moniker::new(&moniker) else {
            tracing::warn!(moniker, "skills import: generated moniker is invalid");
            return;
        };
        let key = crate::services::skills::registry::SkillKey::new(
            crate::domain::ids::ProviderName::new("comfyui"),
            moniker_typed,
        );
        state
            .skills
            .rename(&key, naming.name, naming.description)
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

/// `GET /v1/skills` — list loaded skills.
///
/// Optional query parameters:
/// - `provider` — exact provider name match (e.g. `comfyui`)
/// - `primitive` — dotted primitive id (e.g. `image.generate`)
pub async fn list_skills(
    State(state): State<AppState>,
    Query(q): Query<ListSkillsQuery>,
) -> Response {
    let snapshot = state.skills.snapshot();

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

    let mut entries: Vec<Value> = Vec::new();
    for entry in snapshot.skills.values() {
        if let Some(p) = &provider_filter {
            if &entry.meta.provider != p {
                continue;
            }
        }
        if let Some(p) = primitive_filter {
            if entry.meta.primitive != p {
                continue;
            }
        }
        entries.push(serde_json::to_value(entry).unwrap_or_else(|_| json!({})));
    }

    // Stable order: provider asc, then moniker asc.
    entries.sort_by(|a, b| {
        let aa = a
            .get("meta")
            .and_then(|m| m.get("provider"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let bb = b
            .get("meta")
            .and_then(|m| m.get("provider"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let primary = aa.cmp(bb);
        if primary != std::cmp::Ordering::Equal {
            return primary;
        }
        let am = a
            .get("meta")
            .and_then(|m| m.get("moniker"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let bm = b
            .get("meta")
            .and_then(|m| m.get("moniker"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        am.cmp(bm)
    });

    let body = json!({
        "version": snapshot.version,
        "count": entries.len(),
        "skills": entries,
    });
    (StatusCode::OK, Json(body)).into_response()
}

/// `GET /v1/skills/{moniker}` — inspect a single skill.
///
/// Skills are uniquely keyed by `(provider, moniker)`. When the same
/// moniker is registered under multiple providers (rare in practice
/// today; ComfyUI is the only loader) the response includes every
/// matching entry, with the consumer expected to disambiguate via
/// the `meta.provider` field. The most common case (one match)
/// returns a single entry.
pub async fn get_skill(
    State(state): State<AppState>,
    Path(moniker_str): Path<String>,
) -> Response {
    let moniker = match Moniker::new(&moniker_str) {
        Ok(m) => m,
        Err(e) => {
            return bail(
                StatusCode::BAD_REQUEST,
                "validation_failed",
                format!("invalid moniker `{moniker_str}`: {e}"),
            );
        }
    };

    let snapshot = state.skills.snapshot();
    let mut matches: Vec<Value> = Vec::new();
    for (key, entry) in snapshot.skills.iter() {
        if key.moniker == moniker {
            matches.push(serde_json::to_value(entry).unwrap_or_else(|_| json!({})));
        }
    }

    if matches.is_empty() {
        return bail(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("no skill registered with moniker `{moniker_str}`"),
        );
    }

    if matches.len() == 1 {
        return (StatusCode::OK, Json(matches.into_iter().next().unwrap()))
            .into_response();
    }

    // Multi-match (different providers): return both as a list.
    let body = json!({
        "moniker": moniker_str,
        "matches": matches,
    });
    (StatusCode::OK, Json(body)).into_response()
}

/// `DELETE /v1/skills/{moniker}` — remove a loaded skill from every
/// provider that has registered it.
///
/// Returns the list of `(provider, moniker)` tuples that were removed.
/// On-disk removal of `{data_dir}/skills/{provider}/{moniker}/` is
/// best-effort and only happens for ComfyUI (the only provider with a
/// disk-backed skill loader today).
pub async fn delete_skill(
    State(state): State<AppState>,
    Path(moniker_str): Path<String>,
) -> Response {
    let moniker = match Moniker::new(&moniker_str) {
        Ok(m) => m,
        Err(e) => {
            return bail(
                StatusCode::BAD_REQUEST,
                "validation_failed",
                format!("invalid moniker `{moniker_str}`: {e}"),
            );
        }
    };

    // Collect every key matching this moniker (across providers).
    let snapshot = state.skills.snapshot();
    let keys_to_remove: Vec<SkillKey> = snapshot
        .skills
        .keys()
        .filter(|k| k.moniker == moniker)
        .cloned()
        .collect();

    if keys_to_remove.is_empty() {
        return bail(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("no skill registered with moniker `{moniker_str}`"),
        );
    }

    let mut removed: Vec<HashMap<&'static str, String>> = Vec::new();
    for key in &keys_to_remove {
        // Remove from in-memory aggregate
        state.skills.unregister(key).await;

        // Best-effort on-disk removal: scan for the directory under
        // `{data_dir}/skills/{provider}/{moniker}` and remove if
        // present. We log failures but don't fail the request — the
        // in-memory removal is the source of truth.
        let disk_dir = state
            .data_dir
            .join("skills")
            .join(key.provider.as_str())
            .join(key.moniker.as_str());
        if disk_dir.exists() {
            if let Err(e) = tokio::fs::remove_dir_all(&disk_dir).await {
                tracing::warn!(
                    error = %e,
                    path = %disk_dir.display(),
                    "skills delete: on-disk removal failed (in-memory removal succeeded)"
                );
            }
        }

        let mut h = HashMap::new();
        h.insert("provider", key.provider.as_str().to_string());
        h.insert("moniker", key.moniker.as_str().to_string());
        removed.push(h);
    }

    let body = json!({
        "moniker": moniker_str,
        "removed": removed,
    });
    (StatusCode::OK, Json(body)).into_response()
}
