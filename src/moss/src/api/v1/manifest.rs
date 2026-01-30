//! API manifest endpoint - returns structured API documentation
//!
//! GET /api/v1/manifest - Returns complete API manifest

use axum::{extract::State, http::StatusCode, Json};
use garden_common::api_manifest::{ApiManifest, EndpointSpec};
use crate::AppState;

/// GET /api/v1/manifest - Return complete API manifest
pub async fn get_api_manifest_v1(
    State(state): State<AppState>,
) -> Result<Json<ApiManifest>, StatusCode> {
    // Build base URL from stone name and API port
    let base_url = format!("http://{}:{}", state.stone_name(), state.api_port);
    
    // Generate manifest from registry
    let manifest = build_manifest(&base_url);
    
    Ok(Json(manifest))
}

/// Build complete API manifest with all endpoints
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
                r#"{"status": "healthy", "components": {"docker": {"status": "healthy"}}}"#
            )
    );
    
    endpoints.push(
        EndpointSpec::new("GET", "/capabilities", "health")
            .description("Hardware capabilities - CPU, memory, GPU, AI runtimes")
            .response_type("HardwareCapabilities")
            .example(
                "Get capabilities",
                "curl http://stone-01:7185/capabilities",
                r#"{"data": {"hardware": {"cpu": {"cores": 8}, "gpus": []}}}"#
            )
    );
    
    endpoints.push(
        EndpointSpec::new("GET", "/metrics", "health")
            .description("Prometheus metrics")
            .response_type("text/plain")
            .example(
                "Get metrics",
                "curl http://stone-01:7185/metrics",
                "# HELP moss_uptime_seconds Moss uptime\nmoss_uptime_seconds 3600"
            )
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
                r#"{"data": [{"name": "mongodb", "state": "installed"}]}"#
            )
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
                r#"{"data": {"results": [{"name": "mongodb", "score": 95}]}}"#
            )
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
                r#"{"data": {"name": "mongodb", "state": "removed"}}"#
            )
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
                "data: [2026-01-27T12:00:00Z] MongoDB starting...\n\n"
            )
    );
    
    // Stone Operations
    endpoints.push(
        EndpointSpec::new("GET", "/api/v1/stone/info", "stone")
            .description("Stone information (name, version, uptime, resources)")
            .response_type("StoneInfo")
            .example(
                "Get stone info",
                "curl http://stone-01:7185/api/v1/stone/info",
                r#"{"data": {"stone_name": "stone-01", "moss_version": "0.1.0.312"}}"#
            )
    );
    
    endpoints.push(
        EndpointSpec::new("GET", "/api/v1/stone/companions", "stone")
            .description("List registered Companions (Cricket, Firefly, OLED)")
            .response_type("Array<CompanionInfo>")
            .example(
                "List Companions",
                "curl http://stone-01:7185/api/v1/stone/companions",
                r#"{"data": {"companions": [{"id": "cricket", "port": 7187, "running": true}]}}"#
            )
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
                "event: stone_chirp\ndata: {\"stone_id\": \"...\", \"services\": []}\n\n"
            )
            .note("Long-running connection. Events broadcast every 30s + on service state change.")
    );
    
    // Garden Topology
    endpoints.push(
        EndpointSpec::new("GET", "/api/v1/garden/topology", "garden")
            .description("Garden topology - all stones and their services")
            .response_type("TopologyResponse")
            .example(
                "Get topology",
                "curl http://stone-01:7185/api/v1/garden/topology",
                r#"{"data": {"stones": [{"stone_id": "...", "services": []}]}}"#
            )
    );
    
    endpoints.push(
        EndpointSpec::new("GET", "/api/v1/garden/nourishment", "garden")
            .description("Check garden-wide updates (software + firmware)")
            .response_type("NourishmentCheckResponse")
            .example(
                "Check updates",
                "curl http://stone-01:7185/api/v1/garden/nourishment",
                r#"{"data": {"offerings": {"available": 3}, "firmware": {"available": 1}}}"#
            )
            .note("Orchestrated: tended Moss queries all stones and aggregates results")
    );
    
    endpoints.push(
        EndpointSpec::new("POST", "/api/v1/garden/nourishment/execute", "garden")
            .description("Execute garden-wide updates")
            .body_schema(r#"NourishmentExecuteRequest { scope: "all" | "offerings" | "firmware" }"#)
            .response_type("NourishmentExecuteResponse")
            .example(
                "Update all offerings",
                r#"curl -X POST http://stone-01:7185/api/v1/garden/nourishment/execute -H "Content-Type: application/json" -d '{"scope": "offerings"}'"#,
                r#"{"data": {"stones": [{"stone_name": "stone-01", "job_id": "job_..."}]}}"#
            )
            .note("Orchestrated: tended Moss dispatches to each affected stone")
    );
    
    // Events & Jobs
    endpoints.push(
        EndpointSpec::new("GET", "/api/v1/events", "events")
            .description("Stream Moss events (SSE - installs, upgrades, state changes)")
            .response_type("text/event-stream")
            .example(
                "Watch events",
                "curl -N http://stone-01:7185/api/v1/events",
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
                r#"{"data": {"job_id": "job_...", "status": "completed", "progress": 100}}"#
            )
    );
    
    // Admin Operations
    endpoints.push(
        EndpointSpec::new("POST", "/api/v1/admin/moss/shutdown", "admin")
            .description("Graceful Moss daemon shutdown")
            .response_type("ShutdownResponse")
            .example(
                "Shutdown Moss",
                "curl -X POST http://stone-01:7185/api/v1/admin/moss/shutdown",
                r#"{"data": {"status": "shutting_down"}}"#
            )
            .note("Stops Companions, flushes logs, closes connections. Stone remains on.")
    );
    
    endpoints.push(
        EndpointSpec::new("POST", "/api/v1/admin/stone/reboot", "admin")
            .description("Reboot the Stone (machine-level)")
            .response_type("RebootResponse")
            .example(
                "Reboot stone",
                "curl -X POST http://stone-01:7185/api/v1/admin/stone/reboot",
                r#"{"data": {"status": "rebooting"}}"#
            )
            .note("Requires sudo. Stone will be offline during reboot (~30-60s).")
    );
    
    ApiManifest {
        version: env!("CARGO_PKG_VERSION").into(),
        base_url: base_url.into(),
        categories: vec![
            garden_common::api_manifest::ApiCategory {
                name: "health".into(),
                description: "Health checks and monitoring".into(),
                endpoints: vec!["/health".into(), "/capabilities".into(), "/metrics".into()],
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
                ],
            },
            garden_common::api_manifest::ApiCategory {
                name: "garden".into(),
                description: "Cross-stone topology and orchestration".into(),
                endpoints: vec![
                    "/api/v1/garden/topology".into(),
                    "/api/v1/garden/nourishment".into(),
                ],
            },
            garden_common::api_manifest::ApiCategory {
                name: "events".into(),
                description: "Event streams and job tracking".into(),
                endpoints: vec![
                    "/api/v1/events".into(),
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
