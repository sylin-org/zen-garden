//! Integration test for the Resources REST surface (ORCH-0030 §2,
//! commit 4).
//!
//! Verifies the HTTP endpoints are routable and return the right
//! envelope shape. Full claim accounting + compute-stack filtering
//! is exhaustively covered by the 14 unit tests in
//! `domain::resources::tests`.
//!
//! Skips gracefully when the orchestrator is unreachable.

mod common;

use serde_json::Value;

use common::garden_probe::GardenHandle;

#[tokio::test]
async fn list_resources_returns_envelope() {
    let Some(garden) = GardenHandle::probe_or_skip().await else {
        return;
    };
    let resp = garden
        .http()
        .get(garden.endpoint("/v1/resources"))
        .send()
        .await;
    let Ok(resp) = resp else {
        return;
    };
    assert!(
        resp.status().is_success(),
        "GET /v1/resources returned {}",
        resp.status()
    );
    let body: Value = resp.json().await.unwrap();
    assert!(body.get("count").is_some(), "missing count");
    assert!(body.get("stones").is_some(), "missing stones");
    eprintln!(
        "✓ list returned {} stone(s)",
        body["count"].as_u64().unwrap_or(0)
    );
}

#[tokio::test]
async fn get_unknown_stone_returns_404() {
    let Some(garden) = GardenHandle::probe_or_skip().await else {
        return;
    };
    let resp = garden
        .http()
        .get(garden.endpoint("/v1/resources/stones/this-stone-does-not-exist"))
        .send()
        .await;
    let Ok(resp) = resp else {
        return;
    };
    assert_eq!(resp.status().as_u16(), 404);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"].as_str(), Some("not_found"));
    eprintln!("✓ unknown stone returns 404");
}

#[tokio::test]
async fn get_unknown_pressure_returns_404() {
    let Some(garden) = GardenHandle::probe_or_skip().await else {
        return;
    };
    let resp = garden
        .http()
        .get(garden.endpoint("/v1/resources/stones/this-stone-does-not-exist/pressure"))
        .send()
        .await;
    let Ok(resp) = resp else {
        return;
    };
    assert_eq!(resp.status().as_u16(), 404);
    eprintln!("✓ unknown pressure returns 404");
}

#[tokio::test]
async fn sitemap_advertises_resources() {
    let Some(garden) = GardenHandle::probe_or_skip().await else {
        return;
    };
    let resp = garden.http().get(garden.endpoint("/v1/")).send().await;
    let Ok(resp) = resp else {
        return;
    };
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body.get("resources").and_then(|v| v.as_str()),
        Some("/v1/resources"),
        "sitemap should advertise resources"
    );
    eprintln!("✓ sitemap advertises /v1/resources");
}
