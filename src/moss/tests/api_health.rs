//! Integration tests: health and info endpoints.
//!
//! These tests exercise the real axum router stack (middleware, routing,
//! handler extraction) without network sockets or Docker activity.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

/// Helper: send a GET request and return (status, body_string).
async fn get(app: axum::Router, uri: &str) -> (StatusCode, String) {
    let response = app
        .oneshot(
            Request::builder()
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&body).to_string();
    (status, text)
}

// ── Health ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn health_endpoint_returns_json() {
    let app = garden_moss::testing::build_test_router().await;
    let (status, body) = get(app, "/health").await;

    // Health may return 200 OK or 503 (Docker unavailable) — both are valid.
    assert!(
        status == StatusCode::OK || status == StatusCode::SERVICE_UNAVAILABLE,
        "unexpected status: {status}"
    );

    // Must be valid JSON with at least a `status` field.
    let json: serde_json::Value = serde_json::from_str(&body)
        .unwrap_or_else(|e| panic!("health response is not valid JSON: {e}\nbody: {body}"));
    assert!(
        json.get("status").is_some(),
        "health response missing `status` field: {json}"
    );
}

#[tokio::test]
async fn health_response_contains_expected_fields() {
    let app = garden_moss::testing::build_test_router().await;
    let (_, body) = get(app, "/health").await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();

    // Structural fields that must always be present in the JSON wire format.
    // Note: uptime_seconds, docker_available, disk_space_ok, memory_ok are
    // `#[serde(skip_serializing)]` (legacy) and do NOT appear in the JSON.
    for field in ["status", "version", "timestamp", "components", "os", "architecture"] {
        assert!(
            json.get(field).is_some(),
            "missing field `{field}` in health response"
        );
    }

    // Components must include docker, disk, memory, initialization.
    let components = json["components"].as_object().expect("components is object");
    for comp in ["docker", "disk", "memory", "initialization"] {
        assert!(
            components.contains_key(comp),
            "missing component `{comp}` in health response"
        );
    }
}

// ── Stone Identity Headers ──────────────────────────────────────────────────

#[tokio::test]
async fn responses_include_stone_identity_headers() {
    let app = garden_moss::testing::build_test_router().await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // The inject_stone_identity middleware adds X-Stone-Id and X-Stone-Name.
    let stone_id = response.headers().get("x-stone-id");
    let stone_name = response.headers().get("x-stone-name");

    assert!(stone_id.is_some(), "missing X-Stone-Id header");
    assert!(stone_name.is_some(), "missing X-Stone-Name header");
    assert_eq!(stone_id.unwrap().to_str().unwrap(), "test-stone-id");
    assert_eq!(stone_name.unwrap().to_str().unwrap(), "stone-test");
}

// ── Offerings ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_offerings_empty() {
    let app = garden_moss::testing::build_test_router().await;
    let (status, body) = get(app, "/api/v1/stone/offerings").await;

    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    // With empty offerings, expect an array (possibly empty) or an object with an array.
    // The exact shape depends on the handler — verify it is valid JSON.
    assert!(json.is_array() || json.is_object(), "unexpected offerings shape: {json}");
}

// ── 404 ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn unknown_route_returns_404() {
    let app = garden_moss::testing::build_test_router().await;
    let (status, _) = get(app, "/api/v1/does-not-exist").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
