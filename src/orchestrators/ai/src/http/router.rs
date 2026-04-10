//! Axum router assembly.

use axum::routing::{delete, get, post, put};
use axum::Router;

use crate::app_state::AppState;

use super::{
    actions_index, catalog, events, flush, health, ingress, introspect, jobs, media, metrics,
    preferences, requests, resources, sitemap, skills, static_files,
};

/// Build the full `/v1/*` router plus `/health`.
pub fn build(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health::get_health))
        .route("/metrics", get(metrics::get_metrics))
        // Sitemap
        .route("/v1/", get(sitemap::get_sitemap))
        .route("/v1", get(sitemap::get_sitemap))
        // Action dispatcher
        .route("/v1/do", post(ingress::post_do).get(actions_index::get_actions).options(ingress::options_do))
        // Hierarchical sugar (POST invokes, GET introspects — ORCH-0030 §R2.8.3)
        .route(
            "/v1/{modality}/{leaf}",
            post(ingress::post_primitive).get(introspect::get_primitive),
        )
        .route(
            "/v1/{modality}/{leaf}/{skill}",
            post(ingress::post_skill).get(introspect::get_skill),
        )
        // Catalog: two-view (ORCH-0030 R2 §R2.2.3)
        //   /v1/catalog                           → navigation summary
        //   /v1/catalog/{modality}/{leaf}          → full schema for a primitive
        //   /v1/catalog/{modality}/{leaf}/{skill}  → full schema for a skill
        // URL grammar mirrors dispatch: /v1/catalog/image/generate/flux-butterfly
        .route("/v1/catalog", get(catalog::get_catalog))
        .route("/v1/catalog/{modality}/{leaf}", get(catalog::get_catalog_primitive))
        .route("/v1/catalog/{modality}/{leaf}/{skill}", get(catalog::get_catalog_skill))
        // Events: unified bus (ORCH-0030 §1).
        // `/v1/catalog/events` is retired — clients migrate to
        // `/v1/events?focus=catalog.*,directory.*`.
        .route("/v1/events", get(events::get_events))
        // Media
        .route("/v1/media", post(media::post_upload).get(media::list_media))
        .route(
            "/v1/media/{id}",
            get(media::get_download)
                .head(media::head_media)
                .delete(media::delete_media),
        )
        .route("/v1/media/{id}/metadata", get(media::get_metadata))
        .route("/v1/media/flush", post(flush::flush_media))
        // Jobs
        .route("/v1/jobs", get(jobs::list_jobs))
        .route("/v1/jobs/{id}", get(jobs::get_job).delete(jobs::cancel_job))
        .route("/v1/jobs/{id}/result", get(jobs::get_job_result))
        // Recommendations endpoints removed in ORCH-0030 R2 M3 — the
        // recommendation engine is gone. Adapters that need scoring
        // (Ollama) own their own selector matrix and resolve `selectors.model`
        // inside `onboard`. The `recommended:*` moniker still works at
        // call sites; cloud adapters fall back to their default model.
        // Provider flush
        .route("/v1/providers/flush", post(flush::flush_all_providers))
        .route("/v1/providers/{name}/flush", post(flush::flush_one_provider))
        // Skills — import pipeline (ORCH-0029 Phase 3)
        .route("/v1/skills/{provider}/import", post(skills::post_import))
        // Skill noun surface (ORCH-0030 §3 commit 2)
        .route("/v1/skills", get(skills::list_skills))
        .route(
            "/v1/skills/{moniker}",
            get(skills::get_skill).delete(skills::delete_skill),
        )
        // Resources domain (ORCH-0030 §2 commit 4)
        .route("/v1/resources", get(resources::list_resources))
        .route(
            "/v1/resources/stones/{name}",
            get(resources::get_stone_resources),
        )
        .route(
            "/v1/resources/stones/{name}/pressure",
            get(resources::get_stone_pressure),
        )
        // Preferences (ORCH-0030 §8 commit 12)
        .route(
            "/v1/preferences",
            get(preferences::get_preferences).put(preferences::put_preferences),
        )
        .route(
            "/v1/preferences/{key}",
            delete(preferences::delete_preference),
        )
        // Request log (ORCH-0033) — persistent user interaction history
        .route(
            "/v1/requests",
            get(requests::list_requests).delete(requests::flush_requests),
        )
        .route("/v1/requests/{id}", get(requests::get_request))
        .route(
            "/v1/requests/{id}/pin",
            axum::routing::patch(requests::toggle_pin),
        )
        .route("/v1/requests/{id}/lineage", get(requests::get_lineage))
        // Dashboard SPA — serves embedded static files with SPA
        // fallback. Must be last so API routes take priority.
        .fallback(static_files::serve)
        .with_state(state)
}
