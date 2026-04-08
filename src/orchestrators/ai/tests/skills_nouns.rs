//! Integration tests for the skill noun surface (ORCH-0030 §3,
//! commit 2): GET /v1/skills, GET /v1/skills/{moniker},
//! DELETE /v1/skills/{moniker}.
//!
//! Skips gracefully when the orchestrator is unreachable.

mod common;

use serde_json::Value;

use common::garden_probe::GardenHandle;

#[tokio::test]
async fn list_skills_returns_envelope() {
    let Some(garden) = GardenHandle::probe_or_skip().await else {
        return;
    };
    let resp = garden.http().get(garden.endpoint("/v1/skills")).send().await;
    let Ok(resp) = resp else {
        eprintln!("⊘ skills endpoint not reachable");
        return;
    };
    assert!(
        resp.status().is_success(),
        "GET /v1/skills returned {}",
        resp.status()
    );
    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("⊘ skills body parse: {e}");
            return;
        }
    };
    assert!(body.get("version").is_some(), "missing `version`");
    assert!(body.get("count").is_some(), "missing `count`");
    assert!(
        body.get("skills").and_then(|v| v.as_array()).is_some(),
        "missing or non-array `skills`"
    );
    eprintln!(
        "✓ list returned {} skills (version={})",
        body["count"], body["version"]
    );
}

#[tokio::test]
async fn list_skills_filters_by_provider() {
    let Some(garden) = GardenHandle::probe_or_skip().await else {
        return;
    };
    let resp = garden
        .http()
        .get(garden.endpoint("/v1/skills?provider=comfyui"))
        .send()
        .await;
    let Ok(resp) = resp else {
        return;
    };
    assert!(resp.status().is_success());
    let body: Value = resp.json().await.unwrap();
    let skills = body["skills"].as_array().unwrap();
    for s in skills {
        assert_eq!(
            s["meta"]["provider"].as_str(),
            Some("comfyui"),
            "non-comfyui skill in filtered response: {s}"
        );
    }
    eprintln!("✓ provider filter returned {} comfyui skills", skills.len());
}

#[tokio::test]
async fn list_skills_rejects_unknown_primitive() {
    let Some(garden) = GardenHandle::probe_or_skip().await else {
        return;
    };
    let resp = garden
        .http()
        .get(garden.endpoint("/v1/skills?primitive=video.generate"))
        .send()
        .await;
    let Ok(resp) = resp else {
        return;
    };
    assert_eq!(resp.status().as_u16(), 400);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["error"]["code"].as_str(),
        Some("validation_failed"),
        "expected validation_failed error: {body}"
    );
    eprintln!("✓ unknown primitive rejected with 400");
}

#[tokio::test]
async fn get_skill_unknown_returns_404() {
    let Some(garden) = GardenHandle::probe_or_skip().await else {
        return;
    };
    let resp = garden
        .http()
        .get(garden.endpoint("/v1/skills/this-does-not-exist"))
        .send()
        .await;
    let Ok(resp) = resp else {
        return;
    };
    assert_eq!(resp.status().as_u16(), 404);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"].as_str(), Some("not_found"));
    eprintln!("✓ unknown skill returns 404");
}

#[tokio::test]
async fn get_skill_invalid_moniker_returns_400() {
    let Some(garden) = GardenHandle::probe_or_skip().await else {
        return;
    };
    let resp = garden
        .http()
        .get(garden.endpoint("/v1/skills/INVALID--MONIKER--CAPS"))
        .send()
        .await;
    let Ok(resp) = resp else {
        return;
    };
    // Either rejected as invalid moniker or returned as not_found.
    let status = resp.status().as_u16();
    assert!(
        status == 400 || status == 404,
        "expected 400 or 404 for invalid moniker, got {status}"
    );
    eprintln!("✓ invalid moniker rejected ({status})");
}

#[tokio::test]
async fn delete_unknown_skill_returns_404() {
    let Some(garden) = GardenHandle::probe_or_skip().await else {
        return;
    };
    let resp = garden
        .http()
        .delete(garden.endpoint("/v1/skills/nope-not-here"))
        .send()
        .await;
    let Ok(resp) = resp else {
        return;
    };
    assert_eq!(resp.status().as_u16(), 404);
    eprintln!("✓ deleting unknown skill returns 404");
}

#[tokio::test]
async fn sitemap_advertises_skills() {
    let Some(garden) = GardenHandle::probe_or_skip().await else {
        return;
    };
    let resp = garden.http().get(garden.endpoint("/v1/")).send().await;
    let Ok(resp) = resp else {
        return;
    };
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body.get("skills").and_then(|v| v.as_str()),
        Some("/v1/skills"),
        "sitemap should advertise skills endpoint"
    );
    eprintln!("✓ sitemap advertises /v1/skills");
}
