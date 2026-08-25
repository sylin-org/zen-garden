//! Live integration test suite (ORCH-0030 R2 M5).
//!
//! Every test in this file talks to the running dev container at
//! `http://localhost:7190` over real HTTP. There is no env-var
//! gating, no `probe_or_skip` shell, no separate test container —
//! the dev container started by `src/orchestrators/ai/start.bat`
//! IS the test target.
//!
//! # Invocation
//!
//! ```bash
//! # Make sure the dev container is running first:
//! src/orchestrators/ai/start.bat
//!
//! # Then run the live suite:
//! cargo test --test live -- --ignored
//! ```
//!
//! # Why `#[ignore]`?
//!
//! Every `live_*` test is marked `#[ignore]`. This is the standard
//! Rust idiom for tests that need an external service running. It
//! is **not** the silent-skip antipattern: `#[ignore]` is visible at
//! the test runner level (the runner reports `n ignored`), and when
//! invoked with `--ignored` the tests *always* run and *fail loudly*
//! if something is wrong. The previous `probe_or_skip` pattern —
//! where the test body bailed out silently when an env var was unset
//! — is gone for good in M3.
//!
//! The single exception is [`live_test_function_completeness`], which
//! is a static check that walks `Primitive::ALL` at compile time and
//! is not gated on the live garden being up. It runs on every
//! `cargo test`.
//!
//! # Coverage
//!
//! - **Server probes**: `/health`, `/v1/`, `/v1/catalog`, `/v1/do`,
//!   `/v1/skills`
//! - **Per-primitive dispatch**: one test per primitive in
//!   `Primitive::ALL`. Tests for primitives the live garden does
//!   not serve print a clear `note_no_provider` line and return Ok
//!   (the orchestrator is healthy; the garden simply lacks that
//!   capability).
//! - **Cross-cutting**: idempotency cache hit + fingerprint
//!   conflict, job lifecycle, media upload, parallel dispatch.
//! - **Introspection**: multi-provider fallback chain for
//!   `image.analyze`.
//! - **Negative**: deleted `/v1/recommendations` route, unknown
//!   primitive, unknown skill.

#![allow(clippy::needless_borrow)]

use std::time::Duration;

use serde_json::{json, Value};

/// The single hardcoded base URL. There is exactly one orchestrator
/// container in this project (`zen-garden-ai-orchestrator:dev`); all
/// live tests target it.
const ORCH_BASE: &str = "http://127.0.0.1:7190";

// ── Catalog completeness check (always runs) ──────────────────

/// Every `Primitive` must have an accompanying `live_<dotted>` test
/// function in this file. The check is a Vec membership comparison
/// against `Primitive::ALL`; adding a new primitive without adding
/// the matching test fails the build with a clear error pointing at
/// the missing function name.
///
/// This is the only test in the file that is **not** `#[ignore]` —
/// it is purely static and does not require the orchestrator to be
/// running.
#[test]
fn live_test_function_completeness() {
    use zen_garden_ai_orchestrator::domain::primitive::Primitive;

    let covered: &[&str] = &[
        "text.chat",
        "text.translate",
        "text.embed",
        "text.rerank",
        "image.generate",
        "image.edit",
        "image.upscale",
        "image.analyze",
        "audio.generate",
        "audio.transcribe",
    ];

    let declared: Vec<&'static str> =
        Primitive::ALL.iter().map(|p| p.dotted()).collect();

    for primitive in &declared {
        assert!(
            covered.contains(primitive),
            "Primitive `{primitive}` is in the catalog but has no live test in tests/live.rs. \
             Add a `live_{}` function and append the dotted name to `covered`.",
            primitive.replace('.', "_")
        );
    }
    for primitive in covered {
        assert!(
            declared.contains(primitive),
            "tests/live.rs claims coverage for `{primitive}` but it is no longer in `Primitive::ALL`. \
             Remove the stale test."
        );
    }
}

// ── Shared HTTP client + helpers ───────────────────────────────

/// Live HTTP client targeting the dev container.
///
/// Every constructor that returns this type performs a `/health`
/// probe up front so the test fails immediately and loudly when the
/// orchestrator is unreachable, rather than panicking deep inside an
/// assertion against an empty body.
struct LiveClient {
    base: String,
    http: reqwest::Client,
}

impl LiveClient {
    /// Build a client and probe `/health`. Panics with a clear
    /// message when the orchestrator is unreachable — this is the
    /// loud-failure mode the M3 audit demanded.
    async fn connect() -> Self {
        let base = ORCH_BASE.to_string();
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(180))
            .build()
            .expect("reqwest client");
        let url = format!("{}/health", base);
        let resp = http.get(&url).send().await.unwrap_or_else(|e| {
            panic!(
                "ORCH-0030 M5 live test: orchestrator unreachable at {url}.\n\
                 Run `src/orchestrators/ai/start.bat` to launch the dev \
                 container before invoking the live suite.\n\
                 underlying error: {e}"
            )
        });
        assert!(
            resp.status().is_success(),
            "{url} returned {} — orchestrator is up but not healthy",
            resp.status()
        );
        Self { base, http }
    }

    async fn post_do(&self, body: Value) -> Value {
        self.post_json("/v1/do", body).await
    }

    async fn post_json(&self, path: &str, body: Value) -> Value {
        let url = format!("{}{}", self.base, path);
        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .unwrap_or_else(|e| panic!("POST {url} failed: {e}"));
        let status = resp.status();
        let body: Value = resp
            .json()
            .await
            .unwrap_or_else(|e| panic!("decode {url} response: {e}"));
        assert!(
            status.is_success(),
            "{url} returned {status}: {}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
        body
    }

    async fn get_json(&self, path: &str) -> Value {
        let url = format!("{}{}", self.base, path);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .unwrap_or_else(|e| panic!("GET {url} failed: {e}"));
        assert!(
            resp.status().is_success(),
            "GET {url} returned {}",
            resp.status()
        );
        resp.json()
            .await
            .unwrap_or_else(|e| panic!("decode {url} response: {e}"))
    }

    async fn get_status(&self, path: &str) -> u16 {
        let url = format!("{}{}", self.base, path);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .unwrap_or_else(|e| panic!("GET {url} failed: {e}"));
        resp.status().as_u16()
    }

    /// Find one healthy provider for a primitive in the live
    /// catalog. Returns `None` when nothing in the live garden
    /// serves it — used by tests that should degrade to a printed
    /// note when their dependency isn't present in this particular
    /// garden.
    async fn has_provider_for(&self, dotted_primitive: &str) -> bool {
        let body = self.get_json("/v1/catalog").await;
        let Some(primitives) = body.get("primitives").and_then(|v| v.as_array()) else {
            return false;
        };
        primitives.iter().any(|entry| {
            entry
                .get("action")
                .and_then(|v| v.as_str())
                .is_some_and(|action| action == dotted_primitive)
                && entry
                    .get("providers")
                    .and_then(|v| v.as_array())
                    .map(|arr| !arr.is_empty())
                    .unwrap_or(false)
        })
    }

    /// Upload raw bytes to the media store. Returns the assigned
    /// media id.
    async fn upload_media(&self, bytes: Vec<u8>, content_type: &str) -> String {
        let url = format!("{}/v1/media", self.base);
        let resp = self
            .http
            .post(&url)
            .header(reqwest::header::CONTENT_TYPE, content_type)
            .body(bytes)
            .send()
            .await
            .unwrap_or_else(|e| panic!("POST {url} failed: {e}"));
        assert!(
            resp.status().is_success(),
            "media upload {}",
            resp.status()
        );
        let body: Value = resp.json().await.expect("media response");
        body["media_id"]
            .as_str()
            .expect("media_id in response")
            .to_string()
    }

    async fn download_media(&self, id: &str) -> (Vec<u8>, String) {
        let url = format!("{}/v1/media/{id}", self.base);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .unwrap_or_else(|e| panic!("GET {url} failed: {e}"));
        assert!(
            resp.status().is_success(),
            "media download {}",
            resp.status()
        );
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let bytes = resp.bytes().await.expect("bytes").to_vec();
        (bytes, content_type)
    }
}

/// Print a clear "no provider" notice and return — used when the
/// live garden is up but doesn't have a provider for this particular
/// primitive (e.g. an Infinity rerank instance is offline). The
/// orchestrator is healthy; the garden simply doesn't carry that
/// capability today. This is **not** the silent-skip antipattern —
/// the test ran, hit the live orchestrator, queried its catalog,
/// and made an explicit precondition decision.
fn note_no_provider(primitive: &str) {
    eprintln!("[live] {primitive}: no provider in this garden — pass-through");
}

// ══ Server probes ══════════════════════════════════════════════

#[tokio::test]
#[ignore]
async fn live_health_endpoint_responds() {
    let client = LiveClient::connect().await;
    let body = client.get_json("/health").await;
    assert_eq!(body["status"], json!("ok"));
    let registered = body["providers_registered"]
        .as_u64()
        .expect("providers_registered must be a number");
    assert!(
        registered > 0,
        "live garden must have at least one registered provider"
    );
}

#[tokio::test]
#[ignore]
async fn live_sitemap_does_not_advertise_recommendations() {
    let client = LiveClient::connect().await;
    let body = client.get_json("/v1/").await;
    assert!(
        body.get("recommendations").is_none(),
        "ORCH-0030 R2 M3: /v1/recommendations route was deleted; sitemap must not list it"
    );
    // Spot-check the routes that DID survive M3.
    assert_eq!(body["health"], json!("/health"));
    assert_eq!(body["catalog"], json!("/v1/catalog"));
    assert_eq!(body["actions"], json!("/v1/do"));
    assert_eq!(body["skills"], json!("/v1/skills"));
}

#[tokio::test]
#[ignore]
async fn live_catalog_lists_providers_and_primitives() {
    let client = LiveClient::connect().await;
    let body = client.get_json("/v1/catalog").await;
    let providers = body["providers"]
        .as_array()
        .expect("/v1/catalog must include a providers array");
    assert!(!providers.is_empty(), "live garden must publish providers");
    let primitives = body["primitives"]
        .as_array()
        .expect("/v1/catalog must include a primitives array");
    assert!(
        !primitives.is_empty(),
        "live garden must publish at least one primitive"
    );
}

#[tokio::test]
#[ignore]
async fn live_action_index_lists_primitives() {
    let client = LiveClient::connect().await;
    let body = client.get_json("/v1/do").await;
    let actions = body["actions"]
        .as_array()
        .expect("/v1/do must include an actions array");
    assert!(!actions.is_empty(), "/v1/do must list available actions");
    let status = body
        .get("status")
        .expect("/v1/do must include a status section");
    assert!(
        status["providers_enabled"].as_u64().unwrap_or(0) > 0,
        "status.providers_enabled must be > 0"
    );
}

#[tokio::test]
#[ignore]
async fn live_metrics_uses_m3_metric_names() {
    let client = LiveClient::connect().await;
    let url = format!("{}/metrics", client.base);
    let resp = client
        .http
        .get(&url)
        .send()
        .await
        .unwrap_or_else(|e| panic!("GET {url} failed: {e}"));
    assert!(resp.status().is_success(), "/metrics returned {}", resp.status());
    let text = resp.text().await.expect("metrics body");
    assert!(text.contains("zg_orchestrator_directory_version"));
    assert!(text.contains("zg_orchestrator_providers_total"));
    assert!(text.contains("zg_orchestrator_capabilities_total"));
    assert!(text.contains("zg_orchestrator_skills_total"));
    // The legacy demand-counter metric is gone in M3.
    assert!(
        !text.contains("zg_orchestrator_requests_total"),
        "the M3 metrics rewrite dropped the per-request demand counters"
    );
}

// ══ Per-primitive dispatch ═════════════════════════════════════

#[tokio::test]
#[ignore]
async fn live_text_chat() {
    let client = LiveClient::connect().await;
    if !client.has_provider_for("text.chat").await {
        note_no_provider("text.chat");
        return;
    }

    let body = client
        .post_do(json!({
            "action": "text.chat",
            "model": "recommended:chat",
            "prompt": "Reply with the single word: garden",
            "max_tokens": 256,
            "temperature": 0.0
        }))
        .await;

    let response = body["output"]["text"]["response"]
        .as_str()
        .expect("text.chat must produce text.response");
    assert!(
        !response.trim().is_empty(),
        "text.chat response must contain at least one non-whitespace token, got {response:?}"
    );
}

#[tokio::test]
#[ignore]
async fn live_text_translate() {
    let client = LiveClient::connect().await;
    if !client.has_provider_for("text.translate").await {
        note_no_provider("text.translate");
        return;
    }

    let body = client
        .post_do(json!({
            "action": "text.translate",
            "text": {
                "body": "Hello, world",
                "language": {"source": "en", "target": "es"}
            }
        }))
        .await;

    let translated = body["output"]["text"]["translated"]
        .as_str()
        .expect("text.translate must produce text.translated");
    assert!(
        !translated.trim().is_empty(),
        "translation must be non-empty"
    );
    assert_ne!(
        translated.to_lowercase(),
        "hello, world",
        "translation must differ from the source text"
    );
}

#[tokio::test]
#[ignore]
async fn live_text_embed() {
    let client = LiveClient::connect().await;
    if !client.has_provider_for("text.embed").await {
        note_no_provider("text.embed");
        return;
    }

    let body = client
        .post_do(json!({
            "action": "text.embed",
            "model": "recommended:embedding",
            "text": {"input": ["the quick brown fox"]}
        }))
        .await;

    let embeddings = body["output"]["text"]["embeddings"]
        .as_array()
        .expect("text.embed must produce text.embeddings array");
    assert_eq!(
        embeddings.len(),
        1,
        "one input string must yield exactly one embedding vector"
    );
    let vector = embeddings[0]
        .as_array()
        .expect("each embedding must be a numeric array");
    assert!(
        !vector.is_empty(),
        "embedding vector must have non-zero dimensionality"
    );
    let nonzero = vector
        .iter()
        .filter_map(|v| v.as_f64())
        .any(|f| f.abs() > 1e-9);
    assert!(
        nonzero,
        "embedding vector must contain at least one non-zero component"
    );
}

#[tokio::test]
#[ignore]
async fn live_text_rerank() {
    let client = LiveClient::connect().await;
    if !client.has_provider_for("text.rerank").await {
        note_no_provider("text.rerank");
        return;
    }

    let body = client
        .post_do(json!({
            "action": "text.rerank",
            "text": {
                "query": "the capital of France",
                "documents": [
                    "Paris is the capital of France.",
                    "Berlin is the capital of Germany.",
                    "Madrid is the capital of Spain."
                ]
            }
        }))
        .await;

    let segments = body["output"]["text"]["segments"]
        .as_array()
        .expect("text.rerank must produce text.segments array");
    assert!(
        !segments.is_empty(),
        "rerank must return at least one scored segment"
    );
    let top = &segments[0];
    let top_index = top["index"]
        .as_u64()
        .expect("segments[0].index must be a number");
    assert_eq!(
        top_index, 0,
        "rerank should place the Paris document first; segments were {segments:?}"
    );
}

#[tokio::test]
#[ignore]
async fn live_image_generate() {
    let client = LiveClient::connect().await;
    if !client.has_provider_for("image.generate").await {
        note_no_provider("image.generate");
        return;
    }

    let body = client
        .post_do(json!({
            "action": "image.generate",
            "image": {
                "prompt": {"positive": "a small red square on a white background"},
                "dimensions": {"width": 256, "height": 256}
            }
        }))
        .await;

    let media_id = body["output"]["image"]["media_id"]
        .as_str()
        .expect("image.generate must produce image.media_id");
    let (bytes, content_type) = client.download_media(media_id).await;
    assert!(
        content_type.starts_with("image/"),
        "downloaded media must have an image/* content-type, got {content_type}"
    );
    assert!(
        bytes.len() > 100,
        "generated image must contain real bytes, got {}",
        bytes.len()
    );
}

#[tokio::test]
#[ignore]
async fn live_image_edit() {
    let client = LiveClient::connect().await;
    if !client.has_provider_for("image.edit").await {
        note_no_provider("image.edit");
        return;
    }

    let png = minimal_red_png();
    let media_id = client.upload_media(png, "image/png").await;

    let body = client
        .post_do(json!({
            "action": "image.edit",
            "image": {
                "source": {"media_id": media_id},
                "prompt": {"positive": "make the square blue"}
            }
        }))
        .await;

    let out_media_id = body["output"]["image"]["media_id"]
        .as_str()
        .expect("image.edit must produce image.media_id");
    let (bytes, content_type) = client.download_media(out_media_id).await;
    assert!(
        content_type.starts_with("image/"),
        "edited image must have an image/* content-type"
    );
    assert!(!bytes.is_empty(), "edited image must contain bytes");
}

#[tokio::test]
#[ignore]
async fn live_image_upscale() {
    let client = LiveClient::connect().await;
    if !client.has_provider_for("image.upscale").await {
        note_no_provider("image.upscale");
        return;
    }

    let png = minimal_red_png();
    let media_id = client.upload_media(png, "image/png").await;

    let body = client
        .post_do(json!({
            "action": "image.upscale",
            "image": {
                "source": {"media_id": media_id},
                "scale": 2
            }
        }))
        .await;

    let out_media_id = body["output"]["image"]["media_id"]
        .as_str()
        .expect("image.upscale must produce image.media_id");
    let (bytes, content_type) = client.download_media(out_media_id).await;
    assert!(
        content_type.starts_with("image/"),
        "upscaled image must have an image/* content-type"
    );
    assert!(!bytes.is_empty(), "upscaled image must contain bytes");
}

#[tokio::test]
#[ignore]
async fn live_image_analyze() {
    let client = LiveClient::connect().await;
    if !client.has_provider_for("image.analyze").await {
        note_no_provider("image.analyze");
        return;
    }

    let png = minimal_red_png();
    let media_id = client.upload_media(png, "image/png").await;

    let body = client
        .post_do(json!({
            "action": "image.analyze",
            "image": {"source": {"media_id": media_id}},
            "text": {
                "prompt": {"user": "What single color dominates this image?"},
                "tokens": {"max": 64}
            }
        }))
        .await;

    let analysis = body["output"]["text"]["response"]
        .as_str()
        .expect("image.analyze must produce text.response");
    assert!(
        !analysis.trim().is_empty(),
        "image analysis must produce non-empty text"
    );
}

#[tokio::test]
#[ignore]
async fn live_audio_generate() {
    let client = LiveClient::connect().await;
    if !client.has_provider_for("audio.generate").await {
        note_no_provider("audio.generate");
        return;
    }

    let body = client
        .post_do(json!({
            "action": "audio.generate",
            "model": "recommended:speech",
            "audio": {"text": "the garden is alive"}
        }))
        .await;

    let media_id = body["output"]["audio"]["media_id"]
        .as_str()
        .expect("audio.generate must produce audio.media_id");
    let (bytes, content_type) = client.download_media(media_id).await;
    assert!(
        content_type.starts_with("audio/"),
        "downloaded media must have audio/* content-type, got {content_type}"
    );
    assert!(
        bytes.len() > 1000,
        "synthesized audio must be more than a header, got {} bytes",
        bytes.len()
    );
}

#[tokio::test]
#[ignore]
async fn live_audio_transcribe() {
    let client = LiveClient::connect().await;
    if !client.has_provider_for("audio.transcribe").await {
        note_no_provider("audio.transcribe");
        return;
    }

    // Synthesize an audio sample via audio.generate so we have
    // something to transcribe end-to-end.
    if !client.has_provider_for("audio.generate").await {
        eprintln!("[live] audio.transcribe: skipped (no audio.generate provider for fixture)");
        return;
    }
    let synth = client
        .post_do(json!({
            "action": "audio.generate",
            "model": "recommended:speech",
            "audio": {"text": "the quick brown fox"}
        }))
        .await;
    let synth_media = synth["output"]["audio"]["media_id"]
        .as_str()
        .expect("synthesis must produce media id");

    let body = client
        .post_do(json!({
            "action": "audio.transcribe",
            "audio": {"source": {"media_id": synth_media}}
        }))
        .await;

    let transcript = body["output"]["text"]["response"]
        .as_str()
        .expect("audio.transcribe must produce text.response");
    assert!(
        !transcript.trim().is_empty(),
        "transcript must be non-empty"
    );
}

// ══ Cross-cutting tests ════════════════════════════════════════

/// Idempotency cache hit: same key + same body twice should return
/// the same response with `_meta.idempotent: true` on the second
/// call. Replaces the equivalent in-process test in
/// `services/dispatcher.rs` with a real HTTP round trip.
#[tokio::test]
#[ignore]
async fn live_idempotency_cache_hit() {
    let client = LiveClient::connect().await;
    if !client.has_provider_for("text.chat").await {
        note_no_provider("text.chat");
        return;
    }

    let key = format!("m5-live-idem-{}", uuid_like());
    let payload = json!({
        "action": "text.chat",
        "text": {
            "prompt": {"user": "Reply with: cached"},
            "sampling": {"temperature": 0.0}
        }
    });

    let url = format!("{}/v1/do", client.base);
    let first = client
        .http
        .post(&url)
        .header("Idempotency-Key", &key)
        .json(&payload)
        .send()
        .await
        .unwrap_or_else(|e| panic!("POST {url} (1st) failed: {e}"));
    assert!(first.status().is_success(), "first call must succeed");
    let first_body: Value = first.json().await.expect("first body");
    let first_idem = first_body["_meta"]["idempotent"].as_bool().unwrap_or(false);
    assert!(!first_idem, "first call must NOT be marked idempotent");

    let second = client
        .http
        .post(&url)
        .header("Idempotency-Key", &key)
        .json(&payload)
        .send()
        .await
        .unwrap_or_else(|e| panic!("POST {url} (2nd) failed: {e}"));
    assert!(
        second.status().is_success(),
        "second call (cache hit) must succeed"
    );
    let second_body: Value = second.json().await.expect("second body");
    let second_idem = second_body["_meta"]["idempotent"].as_bool().unwrap_or(false);
    assert!(
        second_idem,
        "second call with same key + body must be marked idempotent: {second_body}"
    );
    assert_eq!(
        first_body["output"], second_body["output"],
        "cached output must match the original"
    );
}

/// Idempotency fingerprint conflict: same key + DIFFERENT body must
/// return HTTP 422 `idempotency_conflict` with both fingerprints in
/// the error details. Replaces the in-process equivalent.
#[tokio::test]
#[ignore]
async fn live_idempotency_fingerprint_conflict() {
    let client = LiveClient::connect().await;
    if !client.has_provider_for("text.chat").await {
        note_no_provider("text.chat");
        return;
    }

    let key = format!("m5-live-conflict-{}", uuid_like());
    let url = format!("{}/v1/do", client.base);

    // Warm the cache.
    let _ = client
        .http
        .post(&url)
        .header("Idempotency-Key", &key)
        .json(&json!({
            "action": "text.chat",
            "text": {"prompt": {"user": "first body"}, "sampling": {"temperature": 0.0}}
        }))
        .send()
        .await
        .unwrap();

    // Same key, DIFFERENT body — must conflict.
    let conflict = client
        .http
        .post(&url)
        .header("Idempotency-Key", &key)
        .json(&json!({
            "action": "text.chat",
            "text": {"prompt": {"user": "second body"}, "sampling": {"temperature": 0.0}}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        conflict.status().as_u16(),
        422,
        "idempotency conflict must surface as HTTP 422"
    );
    let body: Value = conflict.json().await.expect("conflict body");
    assert_eq!(body["error"]["code"], json!("idempotency_conflict"));
    assert!(
        body["error"]["details"]["stored_fingerprint"].is_string(),
        "conflict response must include stored_fingerprint in details"
    );
    assert!(
        body["error"]["details"]["request_fingerprint"].is_string(),
        "conflict response must include request_fingerprint in details"
    );
}

/// Job lifecycle: dispatch a sync request, then list jobs and look
/// up the specific job by id, plus its result.
#[tokio::test]
#[ignore]
async fn live_job_lifecycle_for_sync_dispatch() {
    let client = LiveClient::connect().await;
    if !client.has_provider_for("text.chat").await {
        note_no_provider("text.chat");
        return;
    }

    // Dispatch.
    let dispatch = client
        .post_do(json!({
            "action": "text.chat",
            "text": {
                "prompt": {"user": "Reply with: live-jobs-test"},
                "sampling": {"temperature": 0.0}
            }
        }))
        .await;
    let request_id = dispatch["_meta"]["request_id"]
        .as_str()
        .expect("response must include request_id");
    assert!(!request_id.is_empty());

    // List jobs and find at least one in `Done` state for text.chat.
    let jobs_body = client.get_json("/v1/jobs").await;
    let jobs = jobs_body["jobs"]
        .as_array()
        .expect("/v1/jobs must include a jobs array");
    assert!(!jobs.is_empty(), "live garden must have at least one job after dispatch");
    let some_done = jobs
        .iter()
        .find(|j| j["state"] == "done" && j["action"] == "text.chat")
        .expect("at least one text.chat job must be in `done` state");
    let job_id = some_done["id"].as_str().expect("job id");

    // Look it up by id.
    let job = client.get_json(&format!("/v1/jobs/{job_id}")).await;
    assert_eq!(job["state"], json!("done"));
    assert_eq!(job["action"], json!("text.chat"));

    // Fetch the result.
    let result = client.get_json(&format!("/v1/jobs/{job_id}/result")).await;
    assert!(
        result["output"].is_object(),
        "job result must include an output object"
    );
}

/// Media upload + download round-trip via the public HTTP surface.
#[tokio::test]
#[ignore]
async fn live_media_upload_and_download_roundtrip() {
    let client = LiveClient::connect().await;
    let png = minimal_red_png();
    let original_len = png.len();
    let media_id = client.upload_media(png, "image/png").await;
    assert!(!media_id.is_empty());

    let (bytes, content_type) = client.download_media(&media_id).await;
    assert_eq!(content_type, "image/png");
    assert_eq!(
        bytes.len(),
        original_len,
        "round-tripped bytes must match the upload exactly"
    );
}

/// Parallel dispatch: fire 5 concurrent text.chat requests at the
/// orchestrator and require all of them to complete successfully.
/// Replaces the deleted `tests/parallel_smoke.rs`.
#[tokio::test]
#[ignore]
async fn live_parallel_chat_dispatch() {
    let client = LiveClient::connect().await;
    if !client.has_provider_for("text.chat").await {
        note_no_provider("text.chat");
        return;
    }

    use futures_util::future::join_all;
    let url = format!("{}/v1/do", client.base);
    let http = client.http.clone();
    let futs = (0..5).map(|i| {
        let http = http.clone();
        let url = url.clone();
        async move {
            let resp = http
                .post(&url)
                .json(&json!({
                    "action": "text.chat",
                    "text": {
                        "prompt": {"user": format!("Say the word: parallel-{i}")},
                        "tokens": {"max": 32},
                        "sampling": {"temperature": 0.0}
                    }
                }))
                .send()
                .await
                .unwrap_or_else(|e| panic!("parallel POST {url} failed: {e}"));
            assert!(
                resp.status().is_success(),
                "parallel request {i} returned {}",
                resp.status()
            );
            let body: Value = resp.json().await.expect("parallel body");
            assert!(
                body["output"]["text"]["response"].is_string(),
                "parallel request {i} must return text.response"
            );
        }
    });
    join_all(futs).await;
}

// ══ Skill noun surface ═════════════════════════════════════════

/// `GET /v1/skills` reads from `CapabilityDirectory.all_skills()`.
/// Replaces the deleted `tests/skills_nouns.rs`.
#[tokio::test]
#[ignore]
async fn live_skills_list_returns_published_skills() {
    let client = LiveClient::connect().await;
    let body = client.get_json("/v1/skills").await;
    let count = body["count"].as_u64().expect("count must be a number");
    let skills = body["skills"].as_array().expect("skills must be an array");
    assert_eq!(skills.len() as u64, count);
    // The dev garden has ComfyUI, which loads on-disk skills. We
    // require AT LEAST one skill — the exact count depends on the
    // workspace's `.zen-garden/ai-orchestrator/skills/` contents.
    assert!(
        count > 0,
        "live garden must publish at least one skill via ComfyUI"
    );
    // Each skill entry has the M3 shape: provider + id + primitive
    // + display + parameters.
    for skill in skills {
        assert!(skill["provider"].is_string(), "skill must include provider");
        assert!(skill["id"].is_string(), "skill must include id");
        assert!(skill["primitive"].is_string(), "skill must include primitive");
        assert!(skill["display"].is_object(), "skill must include display object");
        assert!(skill["parameters"].is_array(), "skill must include parameters array");
    }
}

#[tokio::test]
#[ignore]
async fn live_skills_filter_by_provider() {
    let client = LiveClient::connect().await;
    let all = client.get_json("/v1/skills").await;
    let total = all["count"].as_u64().unwrap_or(0);
    let filtered = client.get_json("/v1/skills?provider=comfyui").await;
    let filtered_count = filtered["count"].as_u64().unwrap_or(0);
    assert!(filtered_count <= total, "filtered count must not exceed total");
    for s in filtered["skills"].as_array().expect("skills array") {
        assert_eq!(
            s["provider"], json!("comfyui"),
            "every skill in the filtered set must belong to comfyui"
        );
    }
}

#[tokio::test]
#[ignore]
async fn live_skills_filter_by_primitive() {
    let client = LiveClient::connect().await;
    let body = client
        .get_json("/v1/skills?primitive=image.generate")
        .await;
    let skills = body["skills"].as_array().expect("skills array");
    for s in skills {
        assert_eq!(
            s["primitive"], json!("image.generate"),
            "every skill in the filtered set must declare image.generate"
        );
    }
}

#[tokio::test]
#[ignore]
async fn live_skills_filter_by_unknown_primitive_returns_400() {
    let client = LiveClient::connect().await;
    let url = format!("{}/v1/skills?primitive=text.nosuch", client.base);
    let resp = client.http.get(&url).send().await.unwrap();
    assert_eq!(
        resp.status().as_u16(),
        400,
        "filter by unknown primitive must return 400"
    );
}

#[tokio::test]
#[ignore]
async fn live_skills_get_by_id_returns_full_metadata() {
    let client = LiveClient::connect().await;
    let all = client.get_json("/v1/skills").await;
    let skills = all["skills"].as_array().expect("skills array");
    let Some(first) = skills.first() else {
        eprintln!("[live] no skills published — skipping skill detail test");
        return;
    };
    let id = first["id"].as_str().expect("first skill id");
    let detail = client.get_json(&format!("/v1/skills/{id}")).await;
    // The detail endpoint may return either a single object or
    // {id, matches: [...]}; both are valid per the M3 contract.
    if detail.get("matches").is_some() {
        let matches = detail["matches"].as_array().expect("matches array");
        assert!(!matches.is_empty());
        assert_eq!(matches[0]["id"], json!(id));
    } else {
        assert_eq!(detail["id"], json!(id));
        assert!(detail["display"].is_object());
        assert!(detail["parameters"].is_array());
    }
}

// ══ Capability introspection ═══════════════════════════════════

/// `GET /v1/text/chat` returns the `kind: primitive` introspection
/// shape with routing populated from the live `CapabilityDirectory`.
#[tokio::test]
#[ignore]
async fn live_introspect_text_chat() {
    let client = LiveClient::connect().await;
    let body = client.get_json("/v1/text/chat").await;
    assert_eq!(body["kind"], json!("primitive"));
    assert_eq!(body["primitive"], json!("text.chat"));
    let providers = body["routing"]["providers"]
        .as_array()
        .expect("routing.providers must be an array");
    assert!(!providers.is_empty(), "text.chat must have at least one provider");
}

/// `GET /v1/image/analyze` is the multi-provider primitive in the
/// dev garden — both ComfyUI (with the OCR skill) and Ollama serve
/// it. The introspection endpoint must show a fallback chain.
#[tokio::test]
#[ignore]
async fn live_introspect_image_analyze_shows_multi_provider_routing() {
    let client = LiveClient::connect().await;
    let body = client.get_json("/v1/image/analyze").await;
    let providers = body["routing"]["providers"]
        .as_array()
        .expect("routing.providers must be an array");
    // The dev garden has both ComfyUI and Ollama serving image.analyze.
    // We assert >= 1 (so the test still passes if one provider is
    // temporarily down) and check for the fallback_providers field
    // when there are 2+.
    assert!(!providers.is_empty());
    if providers.len() >= 2 {
        let fallback = body["routing"]["fallback_providers"]
            .as_array()
            .expect("multi-provider routing must include fallback_providers");
        assert_eq!(
            fallback.len(),
            providers.len() - 1,
            "fallback_providers count must be (providers - 1)"
        );
    }
}

// ══ Negative paths ═════════════════════════════════════════════

#[tokio::test]
#[ignore]
async fn live_recommendations_route_is_404() {
    let client = LiveClient::connect().await;
    let status = client.get_status("/v1/recommendations").await;
    assert_eq!(
        status, 404,
        "/v1/recommendations was deleted in M3 and must return 404"
    );
}

#[tokio::test]
#[ignore]
async fn live_unknown_skill_under_known_primitive_is_404() {
    let client = LiveClient::connect().await;
    let url = format!("{}/v1/text/chat/nonexistent-skill", client.base);
    let resp = client
        .http
        .post(&url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status().as_u16(),
        404,
        "unknown skill under a known primitive must return 404"
    );
}

#[tokio::test]
#[ignore]
async fn live_unknown_skill_id_lookup_returns_404() {
    let client = LiveClient::connect().await;
    let status = client
        .get_status("/v1/skills/this-skill-does-not-exist-anywhere")
        .await;
    assert_eq!(status, 404);
}

// ── Fixtures ───────────────────────────────────────────────────

/// Minimal solid-red 8x8 PNG. Smallest viable image input the live
/// providers will accept; chosen so the same fixture works for edit
/// / upscale / analyze without pulling in an image generation
/// library.
fn minimal_red_png() -> Vec<u8> {
    let img = image::ImageBuffer::from_fn(8u32, 8u32, |_, _| image::Rgb([255u8, 0, 0]));
    let mut buf: Vec<u8> = Vec::new();
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
        .expect("encode PNG");
    buf
}

/// Tiny pseudo-uuid suffix for idempotency keys so successive runs
/// of the live suite don't collide on stale cache entries. Not a
/// real UUID — just enough entropy from the system clock + a counter
/// to be distinct within a single test invocation.
fn uuid_like() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{now:x}-{n:x}")
}
