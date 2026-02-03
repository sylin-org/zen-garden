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

use axum::{
    routing::{get, post, delete, patch, put, head},
    Router,
};
use tower_http::trace::TraceLayer;
use crate::{api, AppState};

/// Configure the HTTP router with all API endpoints
pub fn configure(state: AppState) -> Router {
    Router::new()
        // ══════════════════════════════════════════════════════════════════
        // ROOT LEVEL - Industry standard endpoints
        // ══════════════════════════════════════════════════════════════════
        .route("/", get(api::v1::portrait::get_portrait_page))
        .route("/health", get(api::v1::health::get_health))

        // ══════════════════════════════════════════════════════════════════
        // /api/v1/stone/* - THIS stone's local operations
        // ══════════════════════════════════════════════════════════════════
        
        // Stone info & monitoring
        .route("/api/v1/stone", get(api::v1::garden::get_local_stone_v1))
        .route("/api/v1/stone/info", get(api::v1::stone::get_stone_info_v1))
        .route("/api/v1/stone/portrait", get(api::v1::portrait::get_portrait_data))
        .route("/api/v1/stone/portrait/guidance", get(api::v1::portrait::get_portrait_guidance))
        .route("/api/v1/stone/capabilities", get(api::v1::capabilities::get_capabilities))
        .route("/api/v1/stone/metrics", get(api::v1::metrics::get_metrics))
        .route("/api/v1/stone/upgrade", post(api::v1::stone::upgrade_stone_v1))
        .route("/api/v1/stone/deploy", post(api::v1::stone::deploy_stone_v1))

        // Stone offerings (catalog & deployment on THIS stone)
        .route("/api/v1/stone/offerings", get(api::v1::offerings::list_offerings_v1))
        .route("/api/v1/stone/offerings", post(api::v1::offerings::plant_offering_v1))
        .route("/api/v1/stone/offerings/search", get(api::v1::offerings::search_offerings_v1))
        .route("/api/v1/stone/offerings/heal", post(api::v1::offerings::heal_garden_v1))
        .route("/api/v1/stone/offerings/refresh", post(api::v1::offerings::refresh_catalog_v1))
        .route("/api/v1/stone/offerings/adoptable", get(api::v1::adoption::list_adoptable_v1))
        .route("/api/v1/stone/offerings/adopted", get(api::v1::adoption::list_adopted_v1))
        .route("/api/v1/stone/offerings/borrowed", get(api::v1::adoption::list_borrowed_v1))
        .route("/api/v1/stone/offerings/:name", get(api::v1::offerings::get_offering_v1))
        .route("/api/v1/stone/offerings/:name", delete(api::v1::offerings::take_away_offering_v1))
        .route("/api/v1/stone/offerings/:name/manifest", get(api::v1::offerings::get_offering_manifest_v1))
        .route("/api/v1/stone/offerings/:name/capabilities", get(api::v1::offering_capabilities::list_offering_capabilities_v1))
        .route("/api/v1/stone/offerings/:name/capabilities", post(api::v1::offering_capabilities::add_offering_capability_v1))
        .route("/api/v1/stone/offerings/:name/capabilities/:capability", delete(api::v1::offering_capabilities::remove_offering_capability_v1))
        .route("/api/v1/stone/offerings/:offering/adopt", post(api::v1::adoption::adopt_offering_v1))
        .route("/api/v1/stone/offerings/:offering/adopt", delete(api::v1::adoption::unadopt_offering_v1))
        .route("/api/v1/stone/offerings/borrow", post(api::v1::adoption::borrow_service_v1))
        .route("/api/v1/stone/offerings/borrow/:name", delete(api::v1::adoption::unborrow_service_v1))

        // Stone services (running containers on THIS stone)
        .route("/api/v1/stone/services", get(api::v1::services::list_services_v1))
        .route("/api/v1/stone/services", post(api::v1::services::create_service_v1))
        .route("/api/v1/stone/services/manifests", get(api::v1::services::list_manifests_v1))
        .route("/api/v1/stone/services/reconcile", post(api::v1::services::reconcile_inventory_v1))
        .route("/api/v1/stone/services/refresh", post(api::v1::services::refresh_manifests_v1))
        .route("/api/v1/stone/services/refresh-capabilities", post(api::v1::services::refresh_all_capabilities_v1))
        .route("/api/v1/stone/services/:name/manifest", get(api::v1::services::get_manifest_v1))
        .route("/api/v1/stone/services/:service/capabilities", get(api::v1::services::discover_service_capabilities_v1))
        .route("/api/v1/stone/services/:service", get(api::v1::services::get_service_v1))
        .route("/api/v1/stone/services/:service", delete(api::v1::services::delete_service_v1))
        .route("/api/v1/stone/services/:service/logs", get(api::v1::services::stream_service_logs_v1))
        .route("/api/v1/stone/services/:service/restart", post(api::v1::services::restart_service_v1))
        .route("/api/v1/stone/services/:service/rest", post(api::v1::services::rest_service_v1))
        .route("/api/v1/stone/services/:service/wake", post(api::v1::services::wake_service_v1))
        .route("/api/v1/stone/services/:service/nourish", post(api::v1::services::nourish_service_v1))
        .route("/api/v1/stone/services/:service/destroy", post(api::v1::services::destroy_service_v1))
        .route("/api/v1/stone/services/:service/cordon", post(api::v1::services::cordon_service_v1))

        // Stone Companions (Cricket, Firefly, etc.)
        .route("/api/v1/stone/companions", get(api::v1::companions::get_companions))
        .route("/api/v1/stone/companions/refresh", post(api::v1::companions::refresh_companions))
        .route("/api/v1/stone/companions/:id", get(api::v1::companions::get_companion_manifest))
        .route("/api/v1/stone/companions/:id/command", post(api::v1::companions::send_companion_command))
        .route("/api/v1/stone/companions/:id/up", post(api::v1::companions::start_companion))
        .route("/api/v1/stone/companions/:id/down", post(api::v1::companions::stop_companion))

        // Stone storage (seed banks on THIS stone)
        .route("/api/v1/stone/storage", get(api::v1::storage::storage_overview_v1))
        .route("/api/v1/stone/storage/candidates", get(api::v1::storage::list_candidates_v1))
        .route("/api/v1/stone/storage/prepare", post(api::v1::storage::prepare_seed_bank_v1))
        .route("/api/v1/stone/storage/release-all", post(api::v1::storage::release_all_seed_banks_v1))
        .route("/api/v1/stone/storage/bank", get(api::v1::storage::list_banks_v1))
        .route("/api/v1/stone/storage/bank/:id", get(api::v1::storage::get_bank_v1))
        .route("/api/v1/stone/storage/bank/:id", delete(api::v1::storage::delete_bank_v1))
        .route("/api/v1/stone/storage/bank/:id/visibility", patch(api::v1::storage::set_visibility_v1))
        .route("/api/v1/stone/storage/bank/:id/rename", patch(api::v1::storage::rename_bank_v1))
        .route("/api/v1/stone/storage/bank/:id/release", post(api::v1::storage::release_bank_v1))
        .route("/api/v1/stone/storage/bank/:id/*path", get(api::v1::storage::get_object_v1))
        .route("/api/v1/stone/storage/bank/:id/*path", put(api::v1::storage::put_object_v1))
        .route("/api/v1/stone/storage/bank/:id/*path", delete(api::v1::storage::delete_object_v1))
        .route("/api/v1/stone/storage/bank/:id/*path", head(api::v1::storage::head_object_v1))

        // Stone S3 gateway (S3-compatible interface)
        .route("/api/v1/stone/storage/s3", get(api::v1::s3_gateway::list_buckets))
        .route("/api/v1/stone/storage/s3/:bucket", get(api::v1::s3_gateway::list_objects))
        .route("/api/v1/stone/storage/s3/:bucket/*key", put(api::v1::s3_gateway::put_object))
        .route("/api/v1/stone/storage/s3/:bucket/*key", get(api::v1::s3_gateway::get_object))
        .route("/api/v1/stone/storage/s3/:bucket/*key", head(api::v1::s3_gateway::head_object))
        .route("/api/v1/stone/storage/s3/:bucket/*key", delete(api::v1::s3_gateway::delete_object))

        // Stone presence (PRESENCE-0001)
        .route("/api/v1/stone/presence/stream", get(api::v1::presence::stream_stone_presence))
        .route("/api/v1/stone/presence/notify", post(api::v1::presence::notify_presence))

        // Stone nourishment (updates for THIS stone)
        .route("/api/v1/stone/nourishment", get(api::v1::nourishment::check_stone))
        .route("/api/v1/stone/nourishment/execute", post(api::v1::nourishment::execute_stone))
        .route("/api/v1/stone/nourishment/stream/:job_id", get(api::v1::nourishment::stream_status))

        // Stone nurturing (A/B local backup slots)
        .route("/api/v1/stone/nurturing", get(api::v1::nurturing::list_nurturing))
        .route("/api/v1/stone/nurturing/:offering", get(api::v1::nurturing::get_offering_slots))
        .route("/api/v1/stone/nurturing/:offering", post(api::v1::nurturing::create_snapshot))
        .route("/api/v1/stone/nurturing/:offering", delete(api::v1::nurturing::delete_nurturing))
        .route("/api/v1/stone/nurturing/:offering/restore", post(api::v1::nurturing::restore_snapshot))
        // Stone nurturing - timer triggers (for systemd/Task Scheduler)
        .route("/api/v1/nurturing/:offering/trigger", post(api::v1::nurturing::trigger_offering_nurturing))
        .route("/api/v1/nurturing/trigger-all", post(api::v1::nurturing::trigger_all_offerings_nurturing))
        // Stone nurturing - seed bank integration (remote backup)
        .route("/api/v1/stone/nurturing/:offering/replicate", post(api::v1::nurturing::replicate_to_seed_bank))
        .route("/api/v1/stone/nurturing/:offering/restore-remote", post(api::v1::nurturing::restore_from_seed_bank))
        .route("/api/v1/stone/nurturing/remote/:seed_bank", get(api::v1::nurturing::list_remote_snapshots))

        // ══════════════════════════════════════════════════════════════════
        // /api/v1/garden/* - Garden-wide operations (via tended stone)
        // ══════════════════════════════════════════════════════════════════
        
        // Garden overview & topology
        .route("/api/v1/garden", get(api::v1::garden::get_garden_v1))
        .route("/api/v1/garden/topology", get(api::v1::garden::get_topology_v1))
        .route("/api/v1/garden/stones/:stone_name", get(api::v1::garden::get_stone_v1))
        .route("/api/v1/garden/recommend", post(api::v1::garden::recommend_placement_v1))

        // Garden services (find services across ALL stones)
        .route("/api/v1/garden/services", get(api::v1::services::find_services_v1))

        // Garden nourishment (updates across ALL stones)
        .route("/api/v1/garden/nourishment", get(api::v1::nourishment::check_garden))
        .route("/api/v1/garden/nourishment/execute", post(api::v1::nourishment::execute_garden))

        // ══════════════════════════════════════════════════════════════════
        // /api/v1/jobs - Job tracking
        // Note: Event streaming consolidated to /api/v1/stone/presence/stream
        // ══════════════════════════════════════════════════════════════════
        .route("/api/v1/jobs", get(api::v1::jobs::list_jobs))
        .route("/api/v1/jobs/:job_id", get(api::v1::jobs::get_job_status))

        // ══════════════════════════════════════════════════════════════════
        // /api/v1/helpers/* - Internal utility endpoints
        // ══════════════════════════════════════════════════════════════════
        .route("/api/v1/helpers/json-transform", post(api::v1::helpers::json_transform))

        // ══════════════════════════════════════════════════════════════════
        // /api/v1/manifest - API documentation
        // ══════════════════════════════════════════════════════════════════
        .route("/api/v1/manifest", get(api::v1::manifest::get_api_manifest_v1))

        // ══════════════════════════════════════════════════════════════════
        // /api/v1/election - Election protocol (testing)
        // ══════════════════════════════════════════════════════════════════
        .route("/api/v1/election/start", post(api::v1::election::start_election))

        // ══════════════════════════════════════════════════════════════════
        // /api/v1/pond/* - Security & trust management
        // ══════════════════════════════════════════════════════════════════
        .route("/api/v1/pond/init", post(api::v1::pond::pond_init_v1))
        .route("/api/v1/pond", delete(api::v1::pond::pond_remove_v1))
        .route("/api/v1/pond/invite", post(api::v1::pond::pond_invite_v1))
        .route("/api/v1/pond/join", post(api::v1::pond::pond_join_v1))
        .route("/api/v1/pond/stones/:stone_name", delete(api::v1::pond::pond_untrust_v1))
        .route("/api/v1/pond/status", get(api::v1::pond::pond_status_v1))

        // ══════════════════════════════════════════════════════════════════
        // /api/v1/console/* - Console control
        // ══════════════════════════════════════════════════════════════════
        .route("/api/v1/console/mode", get(api::v1::console::get_console_mode_v1))
        .route("/api/v1/console/mode", post(api::v1::console::set_console_mode_v1))

        // ══════════════════════════════════════════════════════════════════
        // /api/v1/admin/* - Administrative operations (privileged)
        // ══════════════════════════════════════════════════════════════════
        .route("/api/v1/admin/moss/shutdown", post(api::v1::admin::moss_shutdown))
        .route("/api/v1/admin/moss/take-root", post(api::v1::admin::moss_take_root))
        .route("/api/v1/admin/stone/shutdown", post(api::v1::admin::stone_shutdown))
        .route("/api/v1/admin/stone/reboot", post(api::v1::admin::stone_reboot))
        .route("/api/v1/admin/stone/:name/wake", post(api::v1::admin::stone_wake))

        // ══════════════════════════════════════════════════════════════════
        // Middleware
        // ══════════════════════════════════════════════════════════════════
        .layer(axum::extract::DefaultBodyLimit::max(200 * 1024 * 1024))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
