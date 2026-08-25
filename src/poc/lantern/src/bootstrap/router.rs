//! Axum router configuration
//!
//! All route registrations in one place, mirroring Moss's bootstrap/router.rs pattern.

use axum::routing::{delete, get, post};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::api::v1;
use crate::AppState;

/// Build the Lantern HTTP router with all routes registered.
pub fn configure(state: AppState) -> Router {
    Router::new()
        // ── Health ───────────────────────────────────────────────────
        .route("/health", get(v1::health::get_health))
        // ── Registry (Moss heartbeat) ────────────────────────────────
        .route("/api/v1/register", post(v1::registration::post_register))
        .route("/api/v1/resolve", get(v1::resolution::get_resolve))
        // ── Garden aggregation endpoints ─────────────────────────────
        .route("/api/v1/garden/stones", get(v1::stones::get_stones))
        .route(
            "/api/v1/garden/stones/{stone_id}",
            get(v1::stones::get_stone_detail),
        )
        .route("/api/v1/garden/topology", get(v1::stones::get_topology))
        .route(
            "/api/v1/garden/offerings",
            get(v1::offerings::get_offerings),
        )
        .route("/api/v1/garden/seeds", get(v1::seeds::get_seeds))
        .route("/api/v1/garden/pond", get(v1::pond::get_pond))
        .route("/api/v1/garden/activity", get(v1::activity::get_activity))
        // ── SSE presence stream ──────────────────────────────────────
        .route(
            "/api/v1/garden/presence/stream",
            get(v1::presence::get_presence_stream),
        )
        // ── Action proxying ──────────────────────────────────────────
        .route(
            "/api/v1/garden/stones/{stone_id}/services/{svc}/rest",
            post(v1::actions::post_service_rest),
        )
        .route(
            "/api/v1/garden/stones/{stone_id}/services/{svc}/wake",
            post(v1::actions::post_service_wake),
        )
        .route(
            "/api/v1/garden/stones/{stone_id}/offerings",
            post(v1::actions::post_deploy_offering),
        )
        .route(
            "/api/v1/garden/stones/{stone_id}/offerings/{name}",
            delete(v1::actions::delete_offering),
        )
        .route(
            "/api/v1/garden/stones/{stone_id}/companions",
            get(v1::companions::get_companions),
        )
        .route(
            "/api/v1/garden/stones/{stone_id}/companions/{cid}/command",
            post(v1::actions::post_companion_command),
        )
        // ── SPA static files ─────────────────────────────────────────
        .route("/", get(v1::spa::serve_index))
        .route("/assets/{*path}", get(v1::spa::serve_spa))
        // SPA fallback: any non-API path serves index.html for client-side routing
        .fallback(get(v1::spa::serve_index))
        // ── Middleware ────────────────────────────────────────────────
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
