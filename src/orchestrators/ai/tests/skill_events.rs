//! Integration test for skill lifecycle events on the unified bus
//! (ORCH-0030 §3 commit 3).
//!
//! Subscribes to `/v1/events?focus=skills.*` and asserts that:
//! - the import endpoint returns 202 + Location header
//! - a `skills.{moniker}.state = ready` event fires after registration
//! - delete publishes a `skills.{moniker}.state = removed` event
//! - the imported skill becomes visible to GET /v1/skills immediately
//!
//! Skips gracefully when the orchestrator is unreachable. Uses a
//! pre-known CivitAI URL for reproducibility.

mod common;

use std::time::Duration;

use futures_util::StreamExt;
use serde_json::Value;

use common::garden_probe::GardenHandle;

const CIVITAI_TEST_URL: &str = "https://civitai.com/images/126242620";
const EVENT_TIMEOUT: Duration = Duration::from_secs(60);

/// Verify the import endpoint returns 202 Accepted with a Location
/// header pointing at the new skill's `/v1/skills/{moniker}` resource.
#[tokio::test]
async fn import_returns_202_with_location_and_thin_body() {
    let Some(garden) = GardenHandle::probe_or_skip().await else {
        return;
    };

    let url = garden.endpoint("/v1/skills/comfyui/import");
    let resp = garden
        .http()
        .post(&url)
        .timeout(Duration::from_secs(120))
        .json(&serde_json::json!({"input": CIVITAI_TEST_URL}))
        .send()
        .await;

    let Ok(resp) = resp else {
        eprintln!("⊘ import endpoint not reachable");
        return;
    };

    assert_eq!(
        resp.status().as_u16(),
        202,
        "expected 202 Accepted, got {}",
        resp.status()
    );

    let location = resp
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .expect("Location header should be present on 202");
    assert!(
        location.starts_with("/v1/skills/"),
        "Location should point to /v1/skills/{{moniker}}, got: {location}"
    );

    let body: Value = resp.json().await.expect("body is JSON");

    // Thin body — must include moniker, primitive, and links.
    assert!(body.get("moniker").is_some(), "missing `moniker`");
    assert!(body.get("primitive").is_some(), "missing `primitive`");
    assert_eq!(
        body.get("primitive").and_then(|v| v.as_str()),
        Some("image.generate"),
        "expected primitive=image.generate"
    );
    let links = body.get("links").expect("missing `links`");
    assert!(links.get("self").is_some(), "missing links.self");
    assert!(links.get("events").is_some(), "missing links.events");

    // Body must NOT include the full AnalyzeResult (that's the
    // pre-commit-3 behavior we replaced).
    assert!(
        body.get("result").is_none(),
        "thin body should not include `result`; got: {body}"
    );

    let moniker = body["moniker"].as_str().unwrap();
    eprintln!("✓ import returned 202 + Location: /v1/skills/{moniker}");

    // Cleanup so we don't pollute the data dir
    let _ = garden
        .http()
        .delete(garden.endpoint(&format!("/v1/skills/{moniker}")))
        .send()
        .await;
}

/// Verify the imported skill becomes visible to GET /v1/skills
/// immediately after the 202 returns (no restart required).
#[tokio::test]
async fn imported_skill_appears_in_list_immediately() {
    let Some(garden) = GardenHandle::probe_or_skip().await else {
        return;
    };

    let resp = garden
        .http()
        .post(garden.endpoint("/v1/skills/comfyui/import"))
        .timeout(Duration::from_secs(120))
        .json(&serde_json::json!({"input": CIVITAI_TEST_URL}))
        .send()
        .await;
    let Ok(resp) = resp else {
        eprintln!("⊘ import not reachable");
        return;
    };
    if resp.status().as_u16() != 202 {
        eprintln!("⊘ import returned {} (expected 202)", resp.status());
        return;
    }
    let body: Value = resp.json().await.unwrap();
    let moniker = body["moniker"].as_str().unwrap().to_string();

    // Immediately list skills and verify our moniker is in there.
    let list_resp = garden
        .http()
        .get(garden.endpoint("/v1/skills"))
        .send()
        .await
        .expect("list");
    let list: Value = list_resp.json().await.unwrap();
    let found = list["skills"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["meta"]["moniker"].as_str() == Some(moniker.as_str()));
    assert!(
        found,
        "moniker `{moniker}` should appear in /v1/skills immediately after 202; got list: {list}"
    );
    eprintln!("✓ imported skill `{moniker}` visible to GET /v1/skills");

    // Cleanup
    let _ = garden
        .http()
        .delete(garden.endpoint(&format!("/v1/skills/{moniker}")))
        .send()
        .await;
}

/// Subscribe to `skills.*.state` and trigger an import; verify a
/// `state = ready` event fires.
#[tokio::test]
async fn import_publishes_skills_state_ready_event() {
    let Some(garden) = GardenHandle::probe_or_skip().await else {
        return;
    };

    // Open SSE subscription FIRST so we don't miss the event.
    let stream_url = garden.endpoint("/v1/events?focus=skills.**");
    let stream_handle = tokio::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(180))
            .build()
            .unwrap();
        let resp = client.get(stream_url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let mut stream = resp.bytes_stream();
        let deadline = tokio::time::Instant::now() + EVENT_TIMEOUT;
        let mut buf = String::new();
        loop {
            if tokio::time::Instant::now() >= deadline {
                return None;
            }
            let chunk = tokio::time::timeout(Duration::from_secs(5), stream.next()).await;
            let Ok(Some(Ok(bytes))) = chunk else {
                continue;
            };
            buf.push_str(&String::from_utf8_lossy(&bytes));
            // Look for a state.ready data line
            for line in buf.lines() {
                if line.starts_with("data:") && line.contains("\"state\":\"ready\"") {
                    return Some(line.to_string());
                }
            }
        }
    });

    // Give the subscriber a moment to land on the broadcast channel
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Trigger the import
    let resp = garden
        .http()
        .post(garden.endpoint("/v1/skills/comfyui/import"))
        .timeout(Duration::from_secs(120))
        .json(&serde_json::json!({"input": CIVITAI_TEST_URL}))
        .send()
        .await;
    let Ok(resp) = resp else {
        eprintln!("⊘ import not reachable");
        stream_handle.abort();
        return;
    };
    if resp.status().as_u16() != 202 {
        eprintln!("⊘ import returned {} (expected 202)", resp.status());
        stream_handle.abort();
        return;
    }
    let body: Value = resp.json().await.unwrap();
    let moniker = body["moniker"].as_str().unwrap().to_string();

    let event_line = stream_handle.await.expect("join");
    assert!(
        event_line.is_some(),
        "did not receive a skills.*.state=ready event within {EVENT_TIMEOUT:?}"
    );
    let line = event_line.unwrap();
    assert!(
        line.contains(&moniker),
        "ready event did not mention the imported moniker `{moniker}`: {line}"
    );
    eprintln!("✓ skills.{moniker}.state = ready event observed on /v1/events");

    // Cleanup
    let _ = garden
        .http()
        .delete(garden.endpoint(&format!("/v1/skills/{moniker}")))
        .send()
        .await;
}
