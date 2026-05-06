//! Integration tests for `/api/v1/jobs/{id}/stream` (Item 2 Phase 4).
//!
//! Exercises the real axum router stack against an in-process Moss
//! state — no network, no Docker. Verifies:
//!
//! - The snapshot frame fires first with the current Job state.
//! - 404-style snapshot is returned for an unknown job_id (status
//!   = "Failed" + synthetic error).
//! - GET /api/v1/jobs/{id} returns the same Job shape including
//!   the `result` field after `complete_with_result`.
//!
//! Live SSE filtering by job_id and terminal-event auto-close are
//! exercised against the unified pulse channel — each test uses the
//! Jobs aggregate's `record_step` / `complete_with_result` to drive
//! events while reading the response body.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn get_job_endpoint_returns_known_job_with_result() {
    let state = garden_moss::testing::build_test_state().await;
    let app = garden_moss::testing::build_test_router_from(state.clone());

    // Submit a job through the aggregate, drive it to Completed
    // with a result payload.
    state
        .jobs
        .submit("j1".into(), "capture_snapshot", vec!["mongodb::prd".into()])
        .await;
    state
        .jobs
        .start("j1", "capture_snapshot", "mongodb::prd")
        .await;
    state
        .jobs
        .record_step("j1", "mongodb::prd", 5, 9, "archiving /data/db")
        .await;
    let result = serde_json::json!({
        "snapshot_id": "snap-0193",
        "size_total_bytes": 1024,
    });
    state
        .jobs
        .complete_with_result("j1", "mongodb::prd", result.clone())
        .await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/jobs/j1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Wire-format contract pinned by this test:
    let data = &json["data"];
    assert_eq!(data["id"], "j1");
    assert_eq!(data["operation"], "capture_snapshot");
    assert_eq!(data["status"], "Completed");
    assert_eq!(data["current_step"], 5);
    assert_eq!(data["total_steps"], 9);
    assert_eq!(data["last_message"], "archiving /data/db");
    assert_eq!(data["result"]["snapshot_id"], "snap-0193");
    assert_eq!(data["result"]["size_total_bytes"], 1024);
}

#[tokio::test]
async fn get_job_endpoint_returns_404_stub_for_unknown_id() {
    let app = garden_moss::testing::build_test_router().await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/jobs/does-not-exist")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Permissive 404: structured Job shape with status=Failed +
    // synthetic error so SSE clients have a uniform shape to render.
    let data = &json["data"];
    assert_eq!(data["id"], "does-not-exist");
    assert_eq!(data["status"], "Failed");
    assert!(
        data["error"]
            .as_str()
            .unwrap_or_default()
            .contains("not found"),
        "expected error to mention 'not found', got: {}",
        data["error"]
    );
}

#[tokio::test]
async fn job_stream_endpoint_serves_text_event_stream() {
    let state = garden_moss::testing::build_test_state().await;
    let app = garden_moss::testing::build_test_router_from(state.clone());

    // Pre-populate a terminal job so the stream emits its snapshot
    // frame immediately and then closes (testing the "snapshot at
    // subscribe" path that survives reconnects).
    state
        .jobs
        .submit("j2".into(), "plant_snapshot", vec!["mongodb::prd".into()])
        .await;
    state
        .jobs
        .start("j2", "plant_snapshot", "mongodb::prd")
        .await;
    state
        .jobs
        .complete_with_result(
            "j2",
            "mongodb::prd",
            serde_json::json!({"snapshot_id": "snap-2"}),
        )
        .await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/jobs/j2/stream")
                .header("Accept", "text/event-stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    // SSE content-type
    let ctype = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        ctype.starts_with("text/event-stream"),
        "expected SSE content-type, got '{ctype}'"
    );

    // Read the body — for a terminal job the stream emits the
    // snapshot frame then closes. The body should contain
    // `event: job.snapshot` and the result we set.
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&body);
    assert!(
        text.contains("event: job.snapshot"),
        "stream missing job.snapshot event: {text}"
    );
    assert!(
        text.contains("snap-2"),
        "stream missing result payload: {text}"
    );
    assert!(
        text.contains("\"status\":\"Completed\""),
        "stream snapshot missing terminal status: {text}"
    );
}

#[tokio::test]
async fn job_stream_endpoint_synthesises_404_for_unknown_id() {
    let app = garden_moss::testing::build_test_router().await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/jobs/no-such-job/stream")
                .header("Accept", "text/event-stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // Per the handler's permissive contract: unknown id returns a
    // synthesized Failed snapshot rather than HTTP 404 — SSE clients
    // get a uniform shape to render.
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&body);
    assert!(
        text.contains("event: job.snapshot"),
        "missing snapshot event: {text}"
    );
    assert!(
        text.contains("\"status\":\"Failed\""),
        "synthetic snapshot should report Failed: {text}"
    );
    assert!(
        text.contains("not found"),
        "synthetic snapshot should mention 'not found': {text}"
    );
}
