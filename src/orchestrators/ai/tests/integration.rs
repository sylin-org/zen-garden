//! End-to-end HTTP tests with a mock provider. Complements
//! `acceptance.rs` which focuses on the ADR acceptance criteria.

mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;

use zen_garden_ai_orchestrator::http::router;

use common::{body_json, fixture_with_mock_chat, get, post_json};

#[tokio::test]
async fn do_text_chat_returns_success_envelope() {
    let (fx, _mock) = fixture_with_mock_chat().await;
    let app = router::build(fx.state);

    let req = Request::post("/v1/do")
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-correlation-id", "test-corr-1")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "action": "text.chat",
                "prompt": "Hi!"
            }))
            .unwrap(),
        ))
        .unwrap();

    let response = app.oneshot(req).await.expect("oneshot");
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_json(response.into_body()).await;
    assert_eq!(
        body["output"]["text"]["response"],
        serde_json::json!("mock-reply: Hi!")
    );
    assert_eq!(
        body["output"]["text"]["finish_reason"],
        serde_json::json!("stop")
    );
    assert_eq!(body["_meta"]["action"], serde_json::json!("text.chat"));
    assert_eq!(body["_meta"]["provider"], serde_json::json!("mockchat"));
    assert_eq!(body["_meta"]["mode"], serde_json::json!("sync"));
    assert_eq!(
        body["_meta"]["correlation_id"],
        serde_json::json!("test-corr-1")
    );
}

#[tokio::test]
async fn hierarchical_sugar_matches_do_surface() {
    let (fx, _mock) = fixture_with_mock_chat().await;
    let app = router::build(fx.state);

    let req = post_json(
        "/v1/text/chat",
        serde_json::json!({"prompt": "Hello world"}),
    );

    let response = app.oneshot(req).await.expect("oneshot");
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response.into_body()).await;
    assert_eq!(
        body["output"]["text"]["response"],
        serde_json::json!("mock-reply: Hello world")
    );
}

#[tokio::test]
async fn unknown_action_returns_not_found() {
    let (fx, _mock) = fixture_with_mock_chat().await;
    let app = router::build(fx.state);

    let req = post_json("/v1/do", serde_json::json!({"action": "text.unknown"}));
    let response = app.oneshot(req).await.expect("oneshot");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = body_json(response.into_body()).await;
    assert_eq!(
        body["error"]["code"],
        serde_json::json!("validation_failed")
    );
}

#[tokio::test]
async fn missing_required_field_returns_validation_error() {
    let (fx, _mock) = fixture_with_mock_chat().await;
    let app = router::build(fx.state);

    let req = post_json(
        "/v1/do",
        serde_json::json!({
            "action": "text.chat",
            "text": {"sampling": {"temperature": 0.5}}
        }),
    );

    let response = app.oneshot(req).await.expect("oneshot");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = body_json(response.into_body()).await;
    assert_eq!(
        body["error"]["code"],
        serde_json::json!("validation_failed")
    );
}

#[tokio::test]
async fn media_upload_download_roundtrip() {
    let (fx, _mock) = fixture_with_mock_chat().await;
    let app = router::build(fx.state);

    let upload_req = Request::post("/v1/media")
        .header(header::CONTENT_TYPE, "image/png")
        .body(Body::from(b"fake-png-bytes".to_vec()))
        .unwrap();
    let upload_resp = app.clone().oneshot(upload_req).await.expect("upload");
    assert_eq!(upload_resp.status(), StatusCode::CREATED);
    let upload_body = body_json(upload_resp.into_body()).await;
    let media_id = upload_body["media_id"].as_str().unwrap().to_string();

    let download_req = get(&format!("/v1/media/{media_id}"));
    let download_resp = app.clone().oneshot(download_req).await.expect("download");
    assert_eq!(download_resp.status(), StatusCode::OK);
    let content_type = download_resp
        .headers()
        .get(header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(content_type, "image/png");
    let bytes = axum::body::to_bytes(download_resp.into_body(), 1024)
        .await
        .unwrap();
    assert_eq!(bytes.as_ref(), b"fake-png-bytes");
}

#[tokio::test]
async fn idempotency_key_returns_cached_marker_on_repeat() {
    let (fx, _mock) = fixture_with_mock_chat().await;
    let app = router::build(fx.state);

    let body = serde_json::to_vec(&serde_json::json!({
        "action": "text.chat",
        "prompt": "Hi idempotent"
    }))
    .unwrap();

    let first = Request::post("/v1/do")
        .header(header::CONTENT_TYPE, "application/json")
        .header("idempotency-key", "key-1")
        .body(Body::from(body.clone()))
        .unwrap();
    let r1 = app.clone().oneshot(first).await.expect("first");
    assert_eq!(r1.status(), StatusCode::OK);
    let _ = body_json(r1.into_body()).await;

    let second = Request::post("/v1/do")
        .header(header::CONTENT_TYPE, "application/json")
        .header("idempotency-key", "key-1")
        .body(Body::from(body))
        .unwrap();
    let r2 = app.oneshot(second).await.expect("second");
    assert_eq!(r2.status(), StatusCode::OK);
    let b2 = body_json(r2.into_body()).await;
    assert_eq!(b2["_meta"]["idempotent"], serde_json::json!(true));
}

#[tokio::test]
async fn messages_alias_decomposed_into_prompt_fields() {
    let (fx, _mock) = fixture_with_mock_chat().await;
    let app = router::build(fx.state);

    let req = post_json(
        "/v1/text/chat",
        serde_json::json!({
            "messages": [
                {"role": "system", "content": "You are helpful."},
                {"role": "user", "content": "decompose me"}
            ]
        }),
    );

    let response = app.oneshot(req).await.expect("oneshot");
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response.into_body()).await;
    assert_eq!(
        body["output"]["text"]["response"],
        serde_json::json!("mock-reply: decompose me")
    );
}

#[tokio::test]
async fn sitemap_advertises_endpoints() {
    let (fx, _mock) = fixture_with_mock_chat().await;
    let app = router::build(fx.state);

    let req = get("/v1/");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["actions"], serde_json::json!("/v1/do"));
    assert_eq!(body["catalog"], serde_json::json!("/v1/catalog"));
    assert_eq!(body["media"], serde_json::json!("/v1/media"));
    assert_eq!(body["jobs"], serde_json::json!("/v1/jobs"));
    assert_eq!(body["health"], serde_json::json!("/health"));
}

#[tokio::test]
async fn health_reports_ok_and_counts_providers() {
    let (fx, _mock) = fixture_with_mock_chat().await;
    let app = router::build(fx.state);

    let req = get("/health");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["status"], serde_json::json!("ok"));
    assert_eq!(body["providers_registered"], serde_json::json!(1));
    assert_eq!(body["providers_healthy"], serde_json::json!(1));
}
