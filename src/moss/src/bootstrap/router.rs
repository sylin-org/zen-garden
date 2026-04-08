//! HTTP router configuration
//!
//! Defines all API routes for the Moss HTTP server.
//!
//! Route hierarchy:
//! - / and /health: Industry-standard health endpoints
//! - /api/v1/stone/*: THIS stone's local operations
//! - /api/v1/garden/*: Garden-wide operations (via tended stone)
//! - /api/v1/admin/*: Administrative operations (privileged)
//! - /api/v1/pond/*: Security/trust management
//! - /api/v1/console/*: Console control
//!
//! When pond security is active, routes are split across two listeners:
//! - HTTP :7185 → `configure_public()` (lobby: health, status, discovery, pond join)
//! - HTTPS :7183 → `configure()` (all routes, including public)
//!
//! When pond is not active, HTTP :7185 → `configure()` (all routes, backwards compatible).

use crate::{api, AppState};
use axum::{
    extract::State,
    http::header::HeaderValue,
    middleware::{self, Next},
    response::Response,
    routing::{any, delete, get, head, patch, post, put},
    Router,
};
use garden_common::constants::headers::{HEADER_STONE_ID, HEADER_STONE_NAME};
use tower_http::trace::TraceLayer;

// ── Middleware ───────────────────────────────────────────────────────────

/// Inject `X-Stone-Id` and `X-Stone-Name` headers on every response.
///
/// This lets any client (rake, companion, browser) discover which stone
/// it is talking to without a dedicated `/capabilities` call — the
/// identity piggy-backs on every response for free.
async fn inject_stone_identity(
    State(state): State<AppState>,
    request: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Response {
    let mut response = next.run(request).await;
    if let Ok(v) = HeaderValue::from_str(&state.current.stone.id) {
        response.headers_mut().insert(HEADER_STONE_ID, v);
    }
    if let Ok(v) = HeaderValue::from_str(&state.current.stone.name) {
        response.headers_mut().insert(HEADER_STONE_NAME, v);
    }
    response
}

/// Configure the **public lobby** router for HTTP when pond security is active.
///
/// This subset of routes remains accessible over plain HTTP even after pond
/// initialization. It includes health checks, read-only stone/garden info,
/// discovery endpoints, and pond join/status/CA-cert — everything a
/// non-enrolled client needs to discover the garden and request membership.
///
/// Mutation endpoints, admin operations, and service management are only
/// available on the HTTPS listener via `configure()`.
pub fn configure_public(state: AppState) -> Router {
    Router::new()
        // ══════════════════════════════════════════════════════════════════
        // ROOT LEVEL - Industry standard endpoints
        // ══════════════════════════════════════════════════════════════════
        .route("/", get(api::v1::portrait::get_portrait_page))
        .route("/pulse", get(api::v1::pulse::get_pulse_page))
        .route("/greenhouse", get(api::v1::greenhouse::get_greenhouse_page))
        .route("/pond", get(api::v1::pond::get_pond_page))
        .route("/health", get(api::v1::health::get_health))
        // ══════════════════════════════════════════════════════════════════
        // Stone info (read-only)
        // ══════════════════════════════════════════════════════════════════
        .route("/api/v1/stone", get(api::v1::garden::get_local_stone_v1))
        .route("/api/v1/stone/info", get(api::v1::stone::get_stone_info_v1))
        .route(
            "/api/v1/stone/portrait",
            get(api::v1::portrait::get_portrait_data),
        )
        .route(
            "/api/v1/stone/portrait/guidance",
            get(api::v1::portrait::get_portrait_guidance),
        )
        .route(
            "/api/v1/stone/capabilities",
            get(api::v1::capabilities::get_capabilities),
        )
        .route(
            "/api/v1/stone/capabilities/core",
            get(api::v1::capabilities::get_capabilities_core),
        )
        .route(
            "/api/v1/stone/capabilities/topology",
            get(api::v1::capabilities::get_capabilities_topology),
        )
        .route(
            "/api/v1/stone/capabilities/refresh",
            post(api::v1::capabilities::refresh_capabilities),
        )
        .route("/api/v1/stone/metrics", get(api::v1::metrics::get_metrics))
        .route("/api/v1/stone/tasks", get(get_task_status))
        // ══════════════════════════════════════════════════════════════════
        // Read-only stone endpoints
        // ══════════════════════════════════════════════════════════════════
        .route(
            "/api/v1/stone/offerings",
            get(api::v1::offerings::list_offerings_v1),
        )
        .route(
            "/api/v1/stone/offerings/search",
            get(api::v1::offerings::search_offerings_v1),
        )
        .route(
            "/api/v1/stone/offerings/{name}",
            get(api::v1::offerings::get_offering_v1),
        )
        .route(
            "/api/v1/stone/offerings/{name}/manifest",
            get(api::v1::offerings::get_offering_manifest_v1),
        )
        .route(
            "/api/v1/stone/offerings/{name}/export",
            get(api::v1::offerings::export_offering_manifest_v1),
        )
        .route(
            "/api/v1/stone/services",
            get(api::v1::services::list_services_v1),
        )
        .route(
            "/api/v1/stone/services/manifests",
            get(api::v1::services::list_manifests_v1),
        )
        .route(
            "/api/v1/stone/services/{service}",
            get(api::v1::services::get_service_v1),
        )
        .route(
            "/api/v1/stone/services/{service}/config",
            get(api::v1::config::get_config_v1),
        )
        .route(
            "/api/v1/stone/storage",
            get(api::v1::storage::storage_overview_v1),
        )
        .route(
            "/api/v1/stone/storage/health",
            get(api::v1::storage::storage_health_v1),
        )
        .route(
            "/api/v1/stone/presence/stream",
            get(api::v1::presence::stream_stone_presence),
        )
        .route(
            "/api/v1/stone/pulse/stream",
            get(api::v1::pulse::stream_pulse),
        )
        .route("/api/v1/stone/updates", get(api::v1::updates::check_stone))
        .route(
            "/api/v1/stone/companions",
            get(api::v1::companions::get_companions),
        )
        // ══════════════════════════════════════════════════════════════════
        // Greenhouse (catalog + file read)
        // ══════════════════════════════════════════════════════════════════
        .route(
            "/api/v1/stone/greenhouse/catalog",
            get(api::v1::greenhouse::get_catalog),
        )
        .route(
            "/api/v1/stone/greenhouse/file",
            get(api::v1::greenhouse::get_file),
        )
        .route(
            "/api/v1/stone/greenhouse/export",
            get(api::v1::greenhouse::export_offering),
        )
        .route(
            "/api/v1/stone/greenhouse/containers",
            get(api::v1::greenhouse::list_containers_v1),
        )
        // ══════════════════════════════════════════════════════════════════
        // Garden topology (read-only discovery)
        // ══════════════════════════════════════════════════════════════════
        .route("/api/v1/garden", get(api::v1::garden::get_garden_v1))
        .route(
            "/api/v1/garden/topology",
            get(api::v1::garden::get_topology_v1),
        )
        .route(
            "/api/v1/garden/capabilities",
            get(api::v1::garden::get_garden_capabilities_v1),
        )
        .route(
            "/api/v1/garden/inspect",
            get(api::v1::garden::inspect_garden_v1),
        )
        .route(
            "/api/v1/garden/stones/{stone_name}",
            get(api::v1::garden::get_stone_v1),
        )
        .route(
            "/api/v1/garden/services",
            get(api::v1::services::find_services_v1),
        )
        .route(
            "/api/v1/garden/gateway/{offering}",
            put(api::v1::gateway::put_gateway).delete(api::v1::gateway::delete_gateway),
        )
        .route(
            "/api/v1/garden/tools",
            get(api::v1::tools::list_garden_tools_v1),
        )
        .route(
            "/api/v1/garden/tools/stream",
            get(api::v1::tools::stream_garden_tools_v1),
        )
        .route(
            "/api/v1/garden/updates",
            get(api::v1::updates::check_garden),
        )
        // ══════════════════════════════════════════════════════════════════
        // Jobs & manifests (read-only)
        // ══════════════════════════════════════════════════════════════════
        .route("/api/v1/jobs", get(api::v1::jobs::list_jobs))
        .route("/api/v1/jobs/{job_id}", get(api::v1::jobs::get_job_status))
        .route(
            "/api/v1/manifest",
            get(api::v1::manifest::get_api_manifest_v1),
        )
        // ══════════════════════════════════════════════════════════════════
        // Pond management — ALL routes in the HTTP lobby.
        // Pond operations are the bootstrap/recovery path for the trust
        // infrastructure itself. They are self-securing at the application
        // layer (passphrases, TOTP codes), so must always be reachable
        // over plain HTTP:
        //   init/join    → no HTTPS exists yet (creating CA or enrolling)
        //   unlock       → CA is locked after reboot, HTTPS may not work
        //   invite       → admin may not have CA cert in trust store
        //   remove/untrust/promote → recovery when HTTPS is broken
        // ══════════════════════════════════════════════════════════════════
        .route("/api/v1/pond/init", post(api::v1::pond::pond_init_v1))
        .route(
            "/api/v1/pond/ceremony",
            post(api::v1::pond::pond_ceremony_v1),
        )
        .route("/api/v1/pond", delete(api::v1::pond::pond_remove_v1))
        .route("/api/v1/pond/join", post(api::v1::pond::pond_join_v1))
        .route(
            "/api/v1/pond/enroll-client",
            post(api::v1::pond::pond_enroll_client_v1),
        )
        .route("/api/v1/pond/invite", post(api::v1::pond::pond_invite_v1))
        .route("/api/v1/pond/unlock", post(api::v1::pond::pond_unlock_v1))
        .route("/api/v1/pond/name", put(api::v1::pond::pond_rename_v1))
        .route("/api/v1/pond/promote", post(api::v1::pond::pond_promote_v1))
        .route(
            "/api/v1/pond/stones/{stone_name}",
            delete(api::v1::pond::pond_untrust_v1),
        )
        .route("/api/v1/pond/status", get(api::v1::pond::pond_status_v1))
        .route("/api/v1/pond/ca.pem", get(api::v1::pond::pond_ca_cert_v1))
        // ══════════════════════════════════════════════════════════════════
        // Stone deploy/upgrade - must work over HTTP for infrastructure
        // ══════════════════════════════════════════════════════════════════
        .route(
            "/api/v1/stone/upgrade",
            post(api::v1::stone::upgrade_stone_v1),
        )
        .route(
            "/api/v1/stone/deploy",
            post(api::v1::stone::deploy_stone_v1),
        )
        // ══════════════════════════════════════════════════════════════════
        // Console (read-only)
        // ══════════════════════════════════════════════════════════════════
        .route(
            "/api/v1/console/mode",
            get(api::v1::console::get_console_mode_v1),
        )
        // ══════════════════════════════════════════════════════════════════
        // Middleware
        // ══════════════════════════════════════════════════════════════════
        .layer(axum::extract::DefaultBodyLimit::max(200 * 1024 * 1024))
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            inject_stone_identity,
        ))
        .with_state(state)
}

/// Configure the HTTP router with all API endpoints
///
/// When pond is NOT active, this is served on HTTP :7185.
/// When pond IS active, this is served on HTTPS :7183 (the full set)
/// while HTTP :7185 serves the reduced `configure_public()` set.
pub fn configure(state: AppState) -> Router {
    Router::new()
        // ══════════════════════════════════════════════════════════════════
        // ROOT LEVEL - Industry standard endpoints
        // ══════════════════════════════════════════════════════════════════
        .route("/", get(api::v1::portrait::get_portrait_page))
        .route("/pulse", get(api::v1::pulse::get_pulse_page))
        .route("/greenhouse", get(api::v1::greenhouse::get_greenhouse_page))
        .route("/pond", get(api::v1::pond::get_pond_page))
        .route("/health", get(api::v1::health::get_health))
        // ══════════════════════════════════════════════════════════════════
        // /api/v1/stone/* - THIS stone's local operations
        // ══════════════════════════════════════════════════════════════════
        // Stone info & monitoring
        .route("/api/v1/stone", get(api::v1::garden::get_local_stone_v1))
        .route("/api/v1/stone/info", get(api::v1::stone::get_stone_info_v1))
        .route(
            "/api/v1/stone/portrait",
            get(api::v1::portrait::get_portrait_data),
        )
        .route(
            "/api/v1/stone/portrait/guidance",
            get(api::v1::portrait::get_portrait_guidance),
        )
        .route(
            "/api/v1/stone/capabilities",
            get(api::v1::capabilities::get_capabilities),
        )
        .route(
            "/api/v1/stone/capabilities/core",
            get(api::v1::capabilities::get_capabilities_core),
        )
        .route(
            "/api/v1/stone/capabilities/topology",
            get(api::v1::capabilities::get_capabilities_topology),
        )
        .route(
            "/api/v1/stone/capabilities/refresh",
            post(api::v1::capabilities::refresh_capabilities),
        )
        .route("/api/v1/stone/metrics", get(api::v1::metrics::get_metrics))
        .route("/api/v1/stone/tasks", get(get_task_status))
        .route(
            "/api/v1/stone/upgrade",
            post(api::v1::stone::upgrade_stone_v1),
        )
        .route(
            "/api/v1/stone/deploy",
            post(api::v1::stone::deploy_stone_v1),
        )
        // Stone offerings (catalog & deployment on THIS stone)
        .route(
            "/api/v1/stone/offerings",
            get(api::v1::offerings::list_offerings_v1),
        )
        .route(
            "/api/v1/stone/offerings",
            post(api::v1::offerings::plant_offering_v1),
        )
        .route(
            "/api/v1/stone/offerings/search",
            get(api::v1::offerings::search_offerings_v1),
        )
        .route(
            "/api/v1/stone/offerings/inspect",
            get(api::v1::offerings::inspect_image_v1),
        )
        .route(
            "/api/v1/stone/offerings/heal",
            post(api::v1::offerings::heal_garden_v1),
        )
        .route(
            "/api/v1/stone/offerings/refresh",
            post(api::v1::offerings::refresh_catalog_v1),
        )
        .route(
            "/api/v1/stone/offerings/adoptable",
            get(api::v1::adoption::list_adoptable_v1),
        )
        .route(
            "/api/v1/stone/offerings/adopted",
            get(api::v1::adoption::list_adopted_v1),
        )
        .route(
            "/api/v1/stone/offerings/borrowed",
            get(api::v1::adoption::list_borrowed_v1),
        )
        .route(
            "/api/v1/stone/offerings/{name}",
            get(api::v1::offerings::get_offering_v1),
        )
        .route(
            "/api/v1/stone/offerings/{name}",
            delete(api::v1::offerings::take_away_offering_v1),
        )
        .route(
            "/api/v1/stone/offerings/{name}/manifest",
            get(api::v1::offerings::get_offering_manifest_v1),
        )
        .route(
            "/api/v1/stone/offerings/{name}/export",
            get(api::v1::offerings::export_offering_manifest_v1),
        )
        .route(
            "/api/v1/stone/manifests/test",
            post(api::v1::offerings::test_manifest_v1),
        )
        // Offering volumes — file I/O in offering bind mounts (ORCH-0019)
        .route(
            "/api/v1/stone/offerings/{fqn}/volumes/{volume}/{*path}",
            put(api::v1::offering_volumes::put_volume_file),
        )
        .route(
            "/api/v1/stone/offerings/{fqn}/volumes/{volume}/{*path}",
            get(api::v1::offering_volumes::get_volume_file),
        )
        .route(
            "/api/v1/stone/offerings/{fqn}/volumes/{volume}/{*path}",
            head(api::v1::offering_volumes::head_volume_file),
        )
        // Greenhouse — manifest authoring + file CRUD
        .route(
            "/api/v1/stone/greenhouse/containers",
            get(api::v1::greenhouse::list_containers_v1),
        )
        .route(
            "/api/v1/stone/greenhouse/validate",
            post(api::v1::greenhouse::validate_manifest_v1),
        )
        .route(
            "/api/v1/stone/greenhouse/generate",
            post(api::v1::greenhouse::generate_manifest_v1),
        )
        .route(
            "/api/v1/stone/greenhouse/file",
            get(api::v1::greenhouse::get_file)
                .put(api::v1::greenhouse::put_file)
                .delete(api::v1::greenhouse::delete_file),
        )
        .route(
            "/api/v1/stone/greenhouse/catalog",
            get(api::v1::greenhouse::get_catalog),
        )
        .route(
            "/api/v1/stone/greenhouse/export",
            get(api::v1::greenhouse::export_offering),
        )
        .route(
            "/api/v1/stone/offerings/{name}/capabilities",
            get(api::v1::offering_capabilities::list_offering_capabilities_v1),
        )
        .route(
            "/api/v1/stone/offerings/{name}/capabilities",
            post(api::v1::offering_capabilities::add_offering_capability_v1),
        )
        .route(
            "/api/v1/stone/offerings/{name}/capabilities/refresh",
            post(api::v1::offering_capabilities::refresh_offering_capabilities_v1),
        )
        .route(
            "/api/v1/stone/offerings/{name}/capabilities/mirror",
            post(api::v1::offering_capabilities::mirror_offering_capabilities_v1),
        )
        .route(
            "/api/v1/stone/offerings/{name}/capabilities/{capability}",
            delete(api::v1::offering_capabilities::remove_offering_capability_v1),
        )
        .route(
            "/api/v1/stone/offerings/{offering}/adopt",
            post(api::v1::adoption::adopt_offering_v1),
        )
        .route(
            "/api/v1/stone/offerings/{offering}/adopt",
            delete(api::v1::adoption::unadopt_offering_v1),
        )
        .route(
            "/api/v1/stone/offerings/borrow",
            post(api::v1::adoption::borrow_service_v1),
        )
        .route(
            "/api/v1/stone/offerings/borrow/{name}",
            delete(api::v1::adoption::unborrow_service_v1),
        )
        // Stone services (running containers on THIS stone)
        .route(
            "/api/v1/stone/services",
            get(api::v1::services::list_services_v1),
        )
        .route(
            "/api/v1/stone/services",
            post(api::v1::services::create_service_v1),
        )
        .route(
            "/api/v1/stone/services/manifests",
            get(api::v1::services::list_manifests_v1),
        )
        .route(
            "/api/v1/stone/services/reconcile",
            post(api::v1::services::reconcile_inventory_v1),
        )
        .route(
            "/api/v1/stone/services/refresh",
            post(api::v1::services::refresh_manifests_v1),
        )
        .route(
            "/api/v1/stone/services/refresh-capabilities",
            post(api::v1::services::refresh_all_capabilities_v1),
        )
        .route(
            "/api/v1/stone/services/{name}/manifest",
            get(api::v1::services::get_manifest_v1),
        )
        .route(
            "/api/v1/stone/services/{service}/capabilities",
            get(api::v1::services::discover_service_capabilities_v1),
        )
        .route(
            "/api/v1/stone/services/{service}",
            get(api::v1::services::get_service_v1),
        )
        .route(
            "/api/v1/stone/services/{service}",
            delete(api::v1::services::delete_service_v1),
        )
        .route(
            "/api/v1/stone/services/{service}/logs",
            get(api::v1::services::stream_service_logs_v1),
        )
        .route(
            "/api/v1/stone/services/{service}/env",
            get(api::v1::services::get_service_env_v1)
                .patch(api::v1::services::patch_service_env_v1),
        )
        .route(
            "/api/v1/stone/services/{service}/restart",
            post(api::v1::services::restart_service_v1),
        )
        .route(
            "/api/v1/stone/services/{service}/rest",
            post(api::v1::services::rest_service_v1),
        )
        .route(
            "/api/v1/stone/services/{service}/wake",
            post(api::v1::services::wake_service_v1),
        )
        .route(
            "/api/v1/stone/services/{service}/upgrade",
            post(api::v1::services::nourish_service_v1),
        )
        .route(
            "/api/v1/stone/services/{service}/destroy",
            post(api::v1::services::destroy_service_v1),
        )
        .route(
            "/api/v1/stone/services/{service}/reassign",
            post(api::v1::services::reassign_service_v1),
        )
        .route(
            "/api/v1/stone/services/{service}/cordon",
            post(api::v1::services::cordon_service_v1),
        )
        // Service config patches (owned overlays)
        .route(
            "/api/v1/stone/services/{service}/config",
            get(api::v1::config::get_config_v1)
                .patch(api::v1::config::patch_config_v1)
                .delete(api::v1::config::delete_config_v1),
        )
        // Stone Companions (Cricket, Firefly, etc.)
        .route(
            "/api/v1/stone/companions",
            get(api::v1::companions::get_companions),
        )
        .route(
            "/api/v1/stone/companions/refresh",
            post(api::v1::companions::refresh_companions),
        )
        .route(
            "/api/v1/stone/companions/{id}",
            get(api::v1::companions::get_companion_manifest),
        )
        .route(
            "/api/v1/stone/companions/{id}/command",
            post(api::v1::companions::send_companion_command),
        )
        .route(
            "/api/v1/stone/companions/{id}/up",
            post(api::v1::companions::start_companion),
        )
        .route(
            "/api/v1/stone/companions/{id}/down",
            post(api::v1::companions::stop_companion),
        )
        // Garden storage — name-based, Primary-or-proxy (STORAGE-0009)
        .route(
            "/api/v1/garden/storage",
            get(api::v1::garden_storage::list_storages_v1),
        )
        .route(
            "/api/v1/garden/storage/{name}",
            get(api::v1::garden_storage::discover_v1),
        )
        // Garden storage: /fs namespace (user content at mount root)
        // Exact /fs — directory listing via ?path=&depth=N (S3/GCS model)
        // Wildcard /fs/{*path} — file content GET/PUT/DELETE/HEAD
        .route(
            "/api/v1/garden/storage/{name}/fs",
            get(api::v1::garden_storage::list_fs_v1),
        )
        .route(
            "/api/v1/garden/storage/{name}/fs/{*path}",
            get(api::v1::garden_storage::get_file_v1)
                .put(api::v1::garden_storage::put_file_v1)
                .delete(api::v1::garden_storage::delete_file_v1)
                .head(api::v1::garden_storage::head_file_v1),
        )
        // Garden storage: /objects/ namespace (S3 objects under .zen-garden/storage/)
        .route(
            "/api/v1/garden/storage/{name}/objects/{*path}",
            get(api::v1::garden_storage::get_object_v1),
        )
        .route(
            "/api/v1/garden/storage/{name}/objects/{*path}",
            put(api::v1::garden_storage::put_object_v1),
        )
        .route(
            "/api/v1/garden/storage/{name}/objects/{*path}",
            delete(api::v1::garden_storage::delete_object_v1),
        )
        .route(
            "/api/v1/garden/storage/{name}/objects/{*path}",
            head(api::v1::garden_storage::head_object_v1),
        )
        // Garden storage: /snapshots/ namespace (harvest artifacts)
        .route(
            "/api/v1/garden/storage/{name}/snapshots",
            get(api::v1::garden_storage::list_memories_v1),
        )
        .route(
            "/api/v1/garden/storage/{name}/snapshots/{offering_id}",
            get(api::v1::garden_storage::list_offering_snapshots_v1),
        )
        .route(
            "/api/v1/garden/storage/{name}/snapshots/{offering_id}/manifest",
            get(api::v1::garden_storage::get_offering_manifest_v1),
        )
        .route(
            "/api/v1/garden/storage/{name}/snapshots/{offering_id}/{harvest_id}",
            get(api::v1::garden_storage::download_snapshot_v1),
        )
        // WebDAV file access — STORAGE-0009 Phase 3
        // Accepts all HTTP methods (GET, PUT, DELETE, PROPFIND, MKCOL, MOVE, COPY, etc.)
        .route("/dav/{name}/{*path}", any(api::v1::webdav::handle_webdav))
        // Root collection (PROPFIND on /dav/{name}/ without trailing content)
        .route("/dav/{name}", any(api::v1::webdav::handle_webdav))
        // Stone storage (seed banks on THIS stone) — STORAGE-0009
        .route(
            "/api/v1/stone/storage",
            get(api::v1::storage::storage_overview_v1),
        )
        // S3 port catalog (STORAGE-0016)
        .route(
            "/api/v1/stone/storage/s3/ports",
            get(api::v1::storage::s3_port_catalog),
        )
        .route(
            "/api/v1/stone/storage/health",
            get(api::v1::storage::storage_health_v1),
        )
        .route(
            "/api/v1/stone/storage/candidates",
            get(api::v1::storage::list_candidates_v1),
        )
        .route(
            "/api/v1/stone/storage/add",
            post(api::v1::storage::add_storage_v1),
        )
        .route(
            "/api/v1/stone/storage/release-all",
            post(api::v1::storage::release_all_seed_banks_v1),
        )
        .route(
            "/api/v1/stone/storage/banks",
            get(api::v1::storage::list_banks_v1),
        )
        .route(
            "/api/v1/stone/storage/banks/{name}",
            get(api::v1::storage::get_bank_v1).delete(api::v1::storage::delete_bank_v1),
        )
        .route(
            "/api/v1/stone/storage/banks/{name}/visibility",
            patch(api::v1::storage::set_visibility_v1),
        )
        .route(
            "/api/v1/stone/storage/banks/{name}/rename",
            patch(api::v1::storage::rename_bank_v1),
        )
        .route(
            "/api/v1/stone/storage/banks/{name}/release",
            post(api::v1::storage::release_bank_v1),
        )
        .route(
            "/api/v1/stone/storage/banks/{name}/pin",
            post(api::v1::storage::pin_bank_v1),
        )
        .route(
            "/api/v1/stone/storage/banks/{name}/unpin",
            post(api::v1::storage::unpin_bank_v1),
        )
        .route(
            "/api/v1/stone/storage/banks/{name}/roles",
            patch(api::v1::storage::set_roles_v1),
        )
        .route(
            "/api/v1/stone/storage/banks/{name}/changes",
            get(api::v1::storage::bank_changes_v1),
        )
        // Storage replication SSE stream (STORAGE-0006 Phase 4)
        .route(
            "/api/v1/stone/storage/stream",
            get(api::v1::storage::stream_storage_v1),
        )
        // Stone presence (PRESENCE-0001)
        .route(
            "/api/v1/stone/presence/stream",
            get(api::v1::presence::stream_stone_presence),
        )
        .route(
            "/api/v1/stone/presence/notify",
            post(api::v1::presence::notify_presence),
        )
        // Stone pulse (full firehose: domain + transport events)
        .route(
            "/api/v1/stone/pulse/stream",
            get(api::v1::pulse::stream_pulse),
        )
        // Stone nourishment (updates for THIS stone)
        .route("/api/v1/stone/updates", get(api::v1::updates::check_stone))
        .route(
            "/api/v1/stone/updates/execute",
            post(api::v1::updates::execute_stone),
        )
        .route(
            "/api/v1/stone/updates/stream/{job_id}",
            get(api::v1::updates::stream_status),
        )
        // Stone logs (daemon log access)
        .route("/api/v1/stone/logs", get(api::v1::logs::get_recent_logs))
        .route("/api/v1/stone/logs/stream", get(api::v1::logs::stream_logs))
        // Stone maintenance (caretaking sweeps)
        .route(
            "/api/v1/stone/maintenance/history",
            get(api::v1::maintenance::get_sweep_history),
        )
        .route(
            "/api/v1/stone/maintenance/sweep",
            post(api::v1::maintenance::trigger_sweep),
        )
        // Stone nurturing (A/B local backup slots)
        .route(
            "/api/v1/stone/snapshots",
            get(api::v1::snapshots::list_nurturing),
        )
        .route(
            "/api/v1/stone/snapshots/{offering}",
            get(api::v1::snapshots::get_offering_slots),
        )
        .route(
            "/api/v1/stone/snapshots/{offering}",
            post(api::v1::snapshots::create_snapshot),
        )
        .route(
            "/api/v1/stone/snapshots/{offering}",
            delete(api::v1::snapshots::delete_nurturing),
        )
        .route(
            "/api/v1/stone/snapshots/{offering}/restore",
            post(api::v1::snapshots::restore_snapshot),
        )
        // Stone nurturing - timer triggers (for systemd/Task Scheduler)
        .route(
            "/api/v1/snapshots/{offering}/trigger",
            post(api::v1::snapshots::trigger_offering_nurturing),
        )
        .route(
            "/api/v1/snapshots/trigger-all",
            post(api::v1::snapshots::trigger_all_offerings_nurturing),
        )
        // Stone nurturing - seed bank integration (remote backup)
        .route(
            "/api/v1/stone/snapshots/{offering}/replicate",
            post(api::v1::snapshots::replicate_to_seed_bank),
        )
        .route(
            "/api/v1/stone/snapshots/{offering}/restore-remote",
            post(api::v1::snapshots::restore_from_seed_bank),
        )
        .route(
            "/api/v1/stone/snapshots/remote/{seed_bank}",
            get(api::v1::snapshots::list_remote_snapshots),
        )
        // ══════════════════════════════════════════════════════════════════
        // /api/v1/garden/* - Garden-wide operations (via tended stone)
        // ══════════════════════════════════════════════════════════════════
        // Garden overview & topology
        .route("/api/v1/garden", get(api::v1::garden::get_garden_v1))
        .route(
            "/api/v1/garden/topology",
            get(api::v1::garden::get_topology_v1),
        )
        .route(
            "/api/v1/garden/capabilities",
            get(api::v1::garden::get_garden_capabilities_v1),
        )
        .route(
            "/api/v1/garden/inspect",
            get(api::v1::garden::inspect_garden_v1),
        )
        .route(
            "/api/v1/garden/stones/{stone_name}",
            get(api::v1::garden::get_stone_v1),
        )
        .route(
            "/api/v1/garden/recommend",
            post(api::v1::garden::recommend_placement_v1),
        )
        // Garden services (find services across ALL stones)
        .route(
            "/api/v1/garden/services",
            get(api::v1::services::find_services_v1),
        )
        .route(
            "/api/v1/garden/gateway/{offering}",
            put(api::v1::gateway::put_gateway).delete(api::v1::gateway::delete_gateway),
        )
        .route(
            "/api/v1/garden/tools",
            get(api::v1::tools::list_garden_tools_v1),
        )
        .route(
            "/api/v1/garden/tools/stream",
            get(api::v1::tools::stream_garden_tools_v1),
        )
        // Garden nourishment (updates across ALL stones)
        .route(
            "/api/v1/garden/updates",
            get(api::v1::updates::check_garden),
        )
        .route(
            "/api/v1/garden/updates/execute",
            post(api::v1::updates::execute_garden),
        )
        // S3 gateway (STORAGE-0009 / STORAGE-0016)
        .route(
            "/api/v1/storage/s3/presign",
            post(api::v1::s3_presign::generate_presigned_url),
        )
        .route("/api/v1/storage/s3", get(api::v1::s3_gateway::list_buckets))
        .route(
            "/api/v1/storage/s3/{bucket}",
            get(api::v1::s3_gateway::list_objects)
                .put(api::v1::s3_gateway::create_bucket),
        )
        .route(
            "/api/v1/storage/s3/{bucket}/{*key}",
            put(api::v1::s3_gateway::put_object)
                .post(api::v1::s3_gateway::complete_or_initiate_multipart),
        )
        .route(
            "/api/v1/storage/s3/{bucket}/{*key}",
            get(api::v1::s3_gateway::get_object),
        )
        .route(
            "/api/v1/storage/s3/{bucket}/{*key}",
            head(api::v1::s3_gateway::head_object),
        )
        .route(
            "/api/v1/storage/s3/{bucket}/{*key}",
            delete(api::v1::s3_gateway::delete_object),
        )
        // ══════════════════════════════════════════════════════════════════
        // /api/v1/jobs - Job tracking
        // Note: Event streaming consolidated to /api/v1/stone/presence/stream
        // ══════════════════════════════════════════════════════════════════
        .route("/api/v1/jobs", get(api::v1::jobs::list_jobs))
        .route("/api/v1/jobs/{job_id}", get(api::v1::jobs::get_job_status))
        // ══════════════════════════════════════════════════════════════════
        // /api/v1/helpers/* - Internal utility endpoints
        // ══════════════════════════════════════════════════════════════════
        .route(
            "/api/v1/helpers/json-transform",
            post(api::v1::helpers::json_transform),
        )
        // ══════════════════════════════════════════════════════════════════
        // /api/v1/manifest - API documentation
        // ══════════════════════════════════════════════════════════════════
        .route(
            "/api/v1/manifest",
            get(api::v1::manifest::get_api_manifest_v1),
        )
        // ══════════════════════════════════════════════════════════════════
        // /api/v1/election - Election protocol (testing)
        // ══════════════════════════════════════════════════════════════════
        .route(
            "/api/v1/election/start",
            post(api::v1::election::start_election),
        )
        // ══════════════════════════════════════════════════════════════════
        // /api/v1/pond/* - Security & trust management
        // ══════════════════════════════════════════════════════════════════
        .route("/api/v1/pond/init", post(api::v1::pond::pond_init_v1))
        .route(
            "/api/v1/pond/ceremony",
            post(api::v1::pond::pond_ceremony_v1),
        )
        .route("/api/v1/pond", delete(api::v1::pond::pond_remove_v1))
        .route("/api/v1/pond/invite", post(api::v1::pond::pond_invite_v1))
        .route("/api/v1/pond/join", post(api::v1::pond::pond_join_v1))
        .route(
            "/api/v1/pond/enroll-client",
            post(api::v1::pond::pond_enroll_client_v1),
        )
        .route("/api/v1/pond/unlock", post(api::v1::pond::pond_unlock_v1))
        .route("/api/v1/pond/name", put(api::v1::pond::pond_rename_v1))
        .route("/api/v1/pond/promote", post(api::v1::pond::pond_promote_v1))
        .route(
            "/api/v1/pond/stones/{stone_name}",
            delete(api::v1::pond::pond_untrust_v1),
        )
        .route("/api/v1/pond/status", get(api::v1::pond::pond_status_v1))
        .route("/api/v1/pond/ca.pem", get(api::v1::pond::pond_ca_cert_v1))
        // ══════════════════════════════════════════════════════════════════
        // /api/v1/console/* - Console control
        // ══════════════════════════════════════════════════════════════════
        .route(
            "/api/v1/console/mode",
            get(api::v1::console::get_console_mode_v1),
        )
        .route(
            "/api/v1/console/mode",
            post(api::v1::console::set_console_mode_v1),
        )
        // ══════════════════════════════════════════════════════════════════
        // /api/v1/admin/* - Administrative operations (privileged)
        // ══════════════════════════════════════════════════════════════════
        .route(
            "/api/v1/admin/moss/shutdown",
            post(api::v1::admin::moss_shutdown),
        )
        .route(
            "/api/v1/admin/moss/take-root",
            post(api::v1::admin::moss_take_root),
        )
        .route(
            "/api/v1/admin/stone/shutdown",
            post(api::v1::admin::stone_shutdown),
        )
        .route(
            "/api/v1/admin/stone/reboot",
            post(api::v1::admin::stone_reboot),
        )
        .route(
            "/api/v1/admin/stone/{name}/wake",
            post(api::v1::admin::stone_wake),
        )
        // ══════════════════════════════════════════════════════════════════════
        // Middleware
        // ══════════════════════════════════════════════════════════════════════
        .layer(axum::extract::DefaultBodyLimit::max(200 * 1024 * 1024))
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            inject_stone_identity,
        ))
        .with_state(state)
}

// ── Task supervisor status (ARCH-0015) ──────────────────────────────────

/// GET /api/v1/stone/tasks — Background task status.
async fn get_task_status(
    State(state): State<AppState>,
) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    let guard = state.task_supervisor.read().await;
    match guard.as_ref() {
        Some(handle) => {
            let status = handle.status().await;
            axum::Json(crate::api::responses::ApiResponse::new(status)).into_response()
        }
        None => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

// ============================================================================
// Routing tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    /// Verify that `/fs` (exact, listing) and `/fs/{*path}` (wildcard, content)
    /// coexist without conflict in Axum's matchit router.
    #[tokio::test]
    async fn fs_exact_and_wildcard_coexist() {
        async fn listing(axum::extract::Path(name): axum::extract::Path<String>) -> String {
            format!("list:{name}")
        }
        async fn content(
            axum::extract::Path((name, path)): axum::extract::Path<(String, String)>,
        ) -> String {
            format!("content:{name}:{path}")
        }

        let app: Router<()> = Router::new()
            .route("/storage/{name}/fs", get(listing))
            .route(
                "/storage/{name}/fs/{*path}",
                get(content).put(content).delete(content).head(content),
            );

        // Exact route → listing handler
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/storage/mystore/fs")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let resp: axum::http::Response<Body> = resp;
        assert_eq!(resp.status(), 200, "exact /fs should match listing");
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), b"list:mystore");

        // Wildcard route → content handler
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/storage/mystore/fs/photos/sunset.jpg")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200, "wildcard /fs/path should match content");
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();
        assert!(body_str.starts_with("content:mystore:"), "body={body_str}");
    }
}
