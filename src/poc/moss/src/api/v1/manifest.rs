//! API manifest endpoint - returns structured API documentation
//!
//! GET /api/v1/manifest - Returns complete API manifest

use crate::domain::Current;
use axum::{Json, extract::State, http::StatusCode};
use garden_common::api_manifest::{ApiManifest, EndpointSpec};
use std::sync::Arc;

/// GET /api/v1/manifest - Return complete API manifest
pub async fn get_api_manifest_v1(
    State(current): State<Arc<Current>>,
) -> Result<Json<ApiManifest>, StatusCode> {
    // Build base URL from stone name and API port
    let base_url = format!("http://{}:{}", current.stone.name, current.api_port);

    // Generate manifest from registry
    let manifest = build_manifest(&base_url);

    Ok(Json(manifest))
}

/// Build complete API manifest with all endpoints
#[expect(clippy::vec_init_then_push)]
fn build_manifest(base_url: &str) -> ApiManifest {
    let mut endpoints = Vec::new();

    // Health & Monitoring
    endpoints.push(
        EndpointSpec::new("GET", "/health", "health")
            .description("Health check - daemon and component status")
            .response_type("HealthStatus")
            .example(
                "Check health",
                "curl http://stone-01:7185/health",
                r#"{"status": "healthy", "components": {"docker": {"status": "healthy"}}}"#,
            ),
    );

    endpoints.push(
        EndpointSpec::new("GET", "/capabilities", "health")
            .description("Hardware capabilities - CPU, memory, GPU, AI runtimes")
            .response_type("HardwareCapabilities")
            .example(
                "Get capabilities",
                "curl http://stone-01:7185/capabilities",
                r#"{"data": {"hardware": {"cpu": {"cores": 8}, "gpus": []}}}"#,
            ),
    );

    endpoints.push(
        EndpointSpec::new("GET", "/resources", "health")
            .description("Stone hardware resource snapshot — CPU, memory, disk, network, uptime")
            .response_type("ResourcesSnapshot")
            .example(
                "Get resources",
                "curl http://stone-01:7185/api/v1/stone/resources",
                r#"{"data": {"cpu": {"cores": 8, "usage_percent": 42.0}, "memory": {"total_bytes": 16000000000}}}"#,
            ),
    );

    // Metrics aggregate (ARCH-0018) — software observability
    endpoints.push(
        EndpointSpec::new("GET", "/api/v1/stone/metrics", "observability")
            .description("Full software observability snapshot — per-domain event counters, mutation latency histograms, per-task timing and lag counters, global totals")
            .response_type("MetricsSnapshot")
            .example(
                "Get full snapshot",
                "curl http://stone-01:7185/api/v1/stone/metrics",
                r#"{"data": {"global": {"events_total": 42}, "domains": [...], "tasks": [...]}}"#,
            ),
    );

    endpoints.push(
        EndpointSpec::new("GET", "/api/v1/stone/metrics/global", "observability")
            .description("Process-wide counters: uptime, total events across all domains, total subscriber lag across all tasks")
            .response_type("GlobalSnapshot"),
    );

    endpoints.push(
        EndpointSpec::new("GET", "/api/v1/stone/metrics/domains", "observability")
            .description("All registered domains' observability data (event counters by kind, mutation latency histogram)")
            .response_type("Array<DomainSnapshot>"),
    );

    endpoints.push(
        EndpointSpec::new(
            "GET",
            "/api/v1/stone/metrics/domains/{name}",
            "observability",
        )
        .description("One domain's observability data. 404 if the domain is not registered.")
        .response_type("DomainSnapshot"),
    );

    endpoints.push(
        EndpointSpec::new("GET", "/api/v1/stone/metrics/tasks", "observability")
            .description("All registered tasks' observability data (timing, event counts, subscriber lag). Complementary to /api/v1/stone/tasks which returns lifecycle state.")
            .response_type("Array<TaskSnapshot>"),
    );

    endpoints.push(
        EndpointSpec::new("GET", "/api/v1/stone/metrics/tasks/{name}", "observability")
            .description(
                "One task's observability data. 404 if the task is not registered with Metrics.",
            )
            .response_type("TaskSnapshot"),
    );

    endpoints.push(
        EndpointSpec::new("GET", "/api/v1/stone/metrics/stream", "observability")
            .description("Live SSE stream of interesting transitions (task state changes, subscriber lag detection, registration). Counter increments are NOT streamed — poll /metrics for current counter values.")
            .response_type("text/event-stream"),
    );

    endpoints.push(
        EndpointSpec::new("GET", "/api/v1/stone/tasks/{name}", "observability")
            .description("Single background task lifecycle status (Waiting/Running/Completed/Failed). Complementary to /api/v1/stone/metrics/tasks/{name} which returns observability data.")
            .response_type("TaskStatus"),
    );

    // Offerings (Human Layer)
    endpoints.push(
        EndpointSpec::new("GET", "/api/v1/offerings", "offerings")
            .description("List all offerings (installed and available)")
            .query_param("state", "Filter: installed, available, all", false)
            .response_type("Array<OfferingInfo>")
            .example(
                "List all offerings",
                "curl http://stone-01:7185/api/v1/offerings",
                r#"{"data": [{"name": "mongodb", "state": "installed"}]}"#,
            ),
    );

    endpoints.push(
        EndpointSpec::new("GET", "/api/v1/offerings/search", "offerings")
            .description("Search offerings with intelligent ranking")
            .query_param("q", "Search query (fuzzy, taxonomy-aware)", true)
            .query_param("prefer", "Hardware preferences (e.g., ssd,nvme)", false)
            .query_param("limit", "Max results (default 5, max 50)", false)
            .response_type("OfferingSearchResponse")
            .example(
                "Search for databases",
                "curl 'http://stone-01:7185/api/v1/offerings/search?q=nosql%20database&limit=3'",
                r#"{"data": {"results": [{"name": "mongodb", "score": 95}]}}"#,
            ),
    );

    endpoints.push(
        EndpointSpec::new("POST", "/api/v1/offerings", "offerings")
            .description("Plant (install) an offering")
            .body_schema("PlantOfferingRequest { name: string, config?: object }")
            .response_type("PlantOfferingResponse")
            .example(
                "Install MongoDB",
                r#"curl -X POST http://stone-01:7185/api/v1/offerings -H "Content-Type: application/json" -d '{"name": "mongodb"}'"#,
                r#"{"data": {"name": "mongodb", "state": "installing", "job_id": "job_01936e8b..."}}"#
            )
            .note("Returns 202 Accepted for async operations. Poll job_id for status.")
    );

    endpoints.push(
        EndpointSpec::new("DELETE", "/api/v1/offerings/:name", "offerings")
            .description("Take away (uninstall) an offering")
            .path_param("name", "Offering name", "string")
            .response_type("TakeAwayResponse")
            .example(
                "Remove MongoDB",
                "curl -X DELETE http://stone-01:7185/api/v1/offerings/mongodb",
                r#"{"data": {"name": "mongodb", "state": "removed"}}"#,
            ),
    );

    // Services (Technical Layer)
    endpoints.push(
        EndpointSpec::new("GET", "/api/v1/services", "services")
            .description("List all services (container-level details)")
            .query_param("q", "Search query (name, category, tag)", false)
            .query_param("fresh", "Force network scan", false)
            .response_type("Array<ServiceInfo>")
            .example(
                "List services",
                "curl http://stone-01:7185/api/v1/services",
                r#"{"data": [{"name": "mongodb", "status": "Running", "resources": {"cpu_percent": 2.5}}]}"#
            )
    );

    endpoints.push(
        EndpointSpec::new("GET", "/api/v1/services/:service/logs", "services")
            .description("Stream service logs (SSE)")
            .path_param("service", "Service name", "string")
            .query_param("tail", "Number of lines (default 100)", false)
            .query_param("timestamps", "Include timestamps", false)
            .response_type("text/event-stream")
            .example(
                "Tail MongoDB logs",
                "curl -N http://stone-01:7185/api/v1/services/mongodb/logs?tail=50",
                "data: [2026-01-27T12:00:00Z] MongoDB starting...\n\n",
            ),
    );

    // Stone Operations
    endpoints.push(
        EndpointSpec::new("GET", "/api/v1/stone/info", "stone")
            .description("Stone information (name, version, uptime, resources)")
            .response_type("StoneInfo")
            .example(
                "Get stone info",
                "curl http://stone-01:7185/api/v1/stone/info",
                r#"{"data": {"stone_name": "stone-01", "moss_version": "0.1.0.312"}}"#,
            ),
    );

    endpoints.push(
        EndpointSpec::new("GET", "/api/v1/stone/companions", "stone")
            .description("List registered Companions (Cricket, Firefly, OLED)")
            .response_type("Array<CompanionInfo>")
            .example(
                "List Companions",
                "curl http://stone-01:7185/api/v1/stone/companions",
                r#"{"data": {"companions": [{"id": "cricket", "port": 7187, "running": true}]}}"#,
            ),
    );

    endpoints.push(
        EndpointSpec::new("POST", "/api/v1/stone/companions/:id/command", "stone")
            .description("Send command to Companion (forwarded to Companion HTTP port)")
            .path_param("id", "Companion ID (e.g., cricket)", "string")
            .body_schema("CompanionCommandRequest { args: string[] }")
            .response_type("CompanionCommandResponse")
            .example(
                "Play audio via Cricket",
                r#"curl -X POST http://stone-01:7185/api/v1/stone/companions/cricket/command -H "Content-Type: application/json" -d '{"args": ["play", "stone-online"]}'"#,
                r#"{"data": {"success": true, "output": "Playing: stone-online.mp3"}}"#
            )
            .note("Timeout: 5 seconds. Forwarded to http://127.0.0.1:{companion_port}/command")
    );

    endpoints.push(
        EndpointSpec::new("GET", "/api/v1/stone/presence/stream", "stone")
            .description("Stream Stone presence events (SSE - chirps, service changes)")
            .response_type("text/event-stream")
            .example(
                "Monitor presence",
                "curl -N http://stone-01:7185/api/v1/stone/presence/stream",
                "event: stone_chirp\ndata: {\"stone_id\": \"...\", \"services\": []}\n\n",
            )
            .note("Long-running connection. Events broadcast every 30s + on service state change."),
    );

    endpoints.push(
        EndpointSpec::new("GET", "/api/v1/stone/storage/health", "stone")
            .description("Stone seed-bank storage health (mount readiness, layout, and banks)")
            .response_type("StorageHealth")
            .example(
                "Check storage health",
                "curl http://stone-01:7185/api/v1/stone/storage/health",
                r#"{"data": {"ready": true, "mount_path": "/garden/storage", "bank_count": 1}}"#,
            ),
    );

    // Garden Topology
    endpoints.push(
        EndpointSpec::new("GET", "/api/v1/garden/topology", "garden")
            .description("Garden topology - all stones and their services")
            .response_type("TopologyResponse")
            .example(
                "Get topology",
                "curl http://stone-01:7185/api/v1/garden/topology",
                r#"{"data": {"stones": [{"stone_id": "...", "services": []}]}}"#,
            ),
    );

    endpoints.push(
        EndpointSpec::new("GET", "/api/v1/garden/updates", "garden")
            .description("Check garden-wide updates (software + firmware)")
            .response_type("NourishmentCheckResponse")
            .example(
                "Check updates",
                "curl http://stone-01:7185/api/v1/garden/updates",
                r#"{"data": {"offerings": {"available": 3}, "firmware": {"available": 1}}}"#,
            )
            .note("Orchestrated: tended Moss queries all stones and aggregates results"),
    );

    endpoints.push(
        EndpointSpec::new("POST", "/api/v1/garden/updates/execute", "garden")
            .description("Execute garden-wide updates")
            .body_schema(r#"NourishmentExecuteRequest { scope: "all" | "offerings" | "firmware" }"#)
            .response_type("NourishmentExecuteResponse")
            .example(
                "Update all offerings",
                r#"curl -X POST http://stone-01:7185/api/v1/garden/updates/execute -H "Content-Type: application/json" -d '{"scope": "offerings"}'"#,
                r#"{"data": {"stones": [{"stone_name": "stone-01", "job_id": "job_..."}]}}"#
            )
            .note("Orchestrated: tended Moss dispatches to each affected stone")
    );

    // Events & Jobs
    endpoints.push(
        EndpointSpec::new("GET", "/api/v1/stone/presence/stream", "events")
            .description("Stream stone presence events (SSE - installs, upgrades, state changes)")
            .response_type("text/event-stream")
            .example(
                "Watch events",
                "curl -N http://stone-01:7185/api/v1/stone/presence/stream",
                "event: offering_install\ndata: {\"offering\": \"mongodb\", \"status\": \"started\"}\n\n"
            )
    );

    endpoints.push(
        EndpointSpec::new("GET", "/api/v1/jobs/:job_id", "events")
            .description("Get job status (async operation tracking)")
            .path_param("job_id", "Job ID (GUIDv7)", "string")
            .response_type("JobStatus")
            .example(
                "Check job",
                "curl http://stone-01:7185/api/v1/jobs/job_01936e8b-1234-7def-8123-456789abcdef",
                r#"{"data": {"job_id": "job_...", "status": "completed", "progress": 100}}"#,
            ),
    );

    // Admin Operations
    endpoints.push(
        EndpointSpec::new("POST", "/api/v1/admin/moss/shutdown", "admin")
            .description("Graceful Moss daemon shutdown")
            .response_type("ShutdownResponse")
            .example(
                "Shutdown Moss",
                "curl -X POST http://stone-01:7185/api/v1/admin/moss/shutdown",
                r#"{"data": {"status": "shutting_down"}}"#,
            )
            .note("Stops Companions, flushes logs, closes connections. Stone remains on."),
    );

    endpoints.push(
        EndpointSpec::new("POST", "/api/v1/admin/stone/reboot", "admin")
            .description("Reboot the Stone (machine-level)")
            .response_type("RebootResponse")
            .example(
                "Reboot stone",
                "curl -X POST http://stone-01:7185/api/v1/admin/stone/reboot",
                r#"{"data": {"status": "rebooting"}}"#,
            )
            .note("Requires sudo. Stone will be offline during reboot (~30-60s)."),
    );

    ApiManifest {
        version: env!("CARGO_PKG_VERSION").into(),
        base_url: base_url.into(),
        categories: vec![
            garden_common::api_manifest::ApiCategory {
                name: "health".into(),
                description: "Health checks and hardware resource snapshots".into(),
                endpoints: vec![
                    "/health".into(),
                    "/capabilities".into(),
                    "/resources".into(),
                ],
            },
            garden_common::api_manifest::ApiCategory {
                name: "observability".into(),
                description: "Software observability — per-domain counters, per-task timing, mutation latency, subscriber lag (ARCH-0018)".into(),
                endpoints: vec![
                    "/api/v1/stone/metrics".into(),
                    "/api/v1/stone/metrics/global".into(),
                    "/api/v1/stone/metrics/domains".into(),
                    "/api/v1/stone/metrics/domains/{name}".into(),
                    "/api/v1/stone/metrics/tasks".into(),
                    "/api/v1/stone/metrics/tasks/{name}".into(),
                    "/api/v1/stone/metrics/stream".into(),
                    "/api/v1/stone/tasks".into(),
                    "/api/v1/stone/tasks/{name}".into(),
                ],
            },
            garden_common::api_manifest::ApiCategory {
                name: "offerings".into(),
                description: "Human-layer service management (plant/remove)".into(),
                endpoints: vec![
                    "/api/v1/offerings".into(),
                    "/api/v1/offerings/search".into(),
                ],
            },
            garden_common::api_manifest::ApiCategory {
                name: "services".into(),
                description: "Technical-layer container operations".into(),
                endpoints: vec![
                    "/api/v1/services".into(),
                    "/api/v1/services/:service/logs".into(),
                ],
            },
            garden_common::api_manifest::ApiCategory {
                name: "stone".into(),
                description: "Stone-level operations".into(),
                endpoints: vec![
                    "/api/v1/stone/info".into(),
                    "/api/v1/stone/companions".into(),
                    "/api/v1/stone/presence/stream".into(),
                    "/api/v1/stone/storage/health".into(),
                ],
            },
            garden_common::api_manifest::ApiCategory {
                name: "garden".into(),
                description: "Cross-stone topology and orchestration".into(),
                endpoints: vec![
                    "/api/v1/garden/topology".into(),
                    "/api/v1/garden/updates".into(),
                ],
            },
            garden_common::api_manifest::ApiCategory {
                name: "events".into(),
                description: "Event streams and job tracking".into(),
                endpoints: vec![
                    "/api/v1/stone/presence/stream".into(),
                    "/api/v1/jobs/:job_id".into(),
                ],
            },
            garden_common::api_manifest::ApiCategory {
                name: "admin".into(),
                description: "Administrative operations".into(),
                endpoints: vec![
                    "/api/v1/admin/moss/shutdown".into(),
                    "/api/v1/admin/stone/reboot".into(),
                ],
            },
        ],
        endpoints,
    }
}
