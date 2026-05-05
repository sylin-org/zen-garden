//! Ceremony driver for Pavilion modal flows (PAVILION-0002 §M2).
//!
//! Pond ceremonies (`init`, `join`, `invite`, `unlock`) and replant
//! all share Moss's `/api/v1/pond/ceremony` endpoint, which speaks
//! the protocol defined in [`koi_common::ceremony`]:
//!
//! 1. Client POSTs a [`CeremonyRequest`] (no session_id on the
//!    first call; ceremony name carries the kind).
//! 2. Server replies with a [`CeremonyResponse`] containing
//!    messages to display, prompts to collect, and a session_id
//!    to send back.
//! 3. Client repeats with the user's answers in the next request's
//!    `data` map until `complete: true`.
//!
//! Rake's `ceremony_render::run_ceremony_http` is a CLI loop that
//! blocks the terminal between turns. Pavilion can't block — the
//! frontend modal needs to render between turns, collect input
//! asynchronously, and submit at the user's pace. So this module
//! exposes a *stateless* `ceremony_step` command: each call POSTs
//! one round-trip and returns the response. The frontend holds
//! the session_id between calls.

use std::time::Duration;

use koi_common::ceremony::{CeremonyRequest, CeremonyResponse, QrFormat, RenderHints};
use tauri::State;

use crate::tending::Tending;

/// Per-call HTTP timeout for ceremony round-trips. Most steps are
/// instant (validation + next-prompt computation); the cornerstone-
/// to-applicant TOTP submission is the slowest case and well under
/// this bound.
const CEREMONY_TIMEOUT: Duration = Duration::from_secs(20);

/// Drive one round-trip of a ceremony.
///
/// First call: pass `ceremony` (e.g. "init") and any prefill data;
/// `session_id` is `None`. Subsequent calls: pass the
/// `session_id` from the prior response plus the user's answers
/// to the prior round's prompts in `data`.
///
/// The `qr_format` argument controls how the server renders QR
/// codes for the client (UTF-8 art for the modal's `<pre>` block,
/// PNG base64 for an `<img>`, or URI-only). Defaults to UTF-8.
#[tauri::command]
pub async fn ceremony_step(
    request: CeremonyRequest,
    tending: State<'_, std::sync::Arc<Tending>>,
) -> Result<CeremonyResponse, String> {
    let tended = tending
        .current()
        .await
        .ok_or_else(|| "no stone tended".to_string())?;
    let url = format!("{}/api/v1/pond/ceremony", tended.endpoint);

    let client = reqwest::Client::builder()
        .timeout(CEREMONY_TIMEOUT)
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| format!("build ceremony client: {e}"))?;

    post_ceremony_step(&client, &url, request)
        .await
        .map_err(|e| e.to_string())
}

/// Pure HTTP round-trip — the testable seam under the
/// `ceremony_step` Tauri command. Takes a built reqwest client
/// and the full ceremony URL so tests can point it at a fake
/// server without touching the Tauri state surface.
///
/// Default render hints (PngBase64 QR) are applied when the
/// caller leaves them unset; an explicit `request.render`
/// passes through unchanged.
pub(crate) async fn post_ceremony_step(
    client: &reqwest::Client,
    url: &str,
    mut request: CeremonyRequest,
) -> anyhow::Result<CeremonyResponse> {
    if request.render.is_none() {
        request.render = Some(RenderHints {
            qr: Some(QrFormat::PngBase64),
        });
    }

    let resp = client
        .post(url)
        .json(&request)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("ceremony POST {url}: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("ceremony {status}: {body}");
    }

    resp.json::<CeremonyResponse>()
        .await
        .map_err(|e| anyhow::anyhow!("ceremony parse: {e}"))
}

#[cfg(test)]
mod tests {
    //! Multi-step ceremony integration test.
    //!
    //! Spins up a tiny axum fixture that mimics Moss's
    //! `/api/v1/pond/ceremony` endpoint: it returns a different
    //! `CeremonyResponse` for the first request (no session_id)
    //! vs subsequent requests (session_id present). The test
    //! drives the ceremony to completion via the
    //! [`post_ceremony_step`] seam and asserts the wire types
    //! round-trip correctly through the typed API.
    //!
    //! No mocks — real reqwest, real axum, real serde — so a drift
    //! in `koi_common::ceremony` field naming or the
    //! request/response envelope shape would fail this test.
    use super::*;
    use axum::{
        Json,
        routing::post,
        Router,
    };
    use koi_common::ceremony::{
        CeremonyRequest, CeremonyResponse, InputType, Message, MessageKind, Prompt, QrFormat,
    };
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    /// Counter the fake server uses to differentiate the first
    /// request (start) from subsequent requests (steps).
    type StepCounter = Arc<Mutex<u32>>;

    async fn ceremony_handler(
        axum::extract::State(counter): axum::extract::State<StepCounter>,
        Json(req): Json<CeremonyRequest>,
    ) -> Json<CeremonyResponse> {
        let mut count = counter.lock().unwrap();
        *count += 1;
        let step = *count;
        drop(count);

        let session_id = req.session_id.unwrap_or_else(Uuid::now_v7);

        if step == 1 {
            // First call — start the ceremony, return one prompt.
            assert_eq!(req.ceremony.as_deref(), Some("init"));
            return Json(CeremonyResponse {
                session_id,
                prompts: vec![Prompt {
                    key: "passphrase".to_string(),
                    prompt: "Pond passphrase (8+ characters)".to_string(),
                    input_type: InputType::SecretConfirm,
                    options: Vec::new(),
                    required: true,
                }],
                messages: vec![Message {
                    kind: MessageKind::Info,
                    title: "Place keystone".to_string(),
                    content: "Pick a passphrase for the cornerstone CA.".to_string(),
                }],
                complete: false,
                error: None,
                result_data: None,
            });
        }

        if step == 2 {
            // Second call — user submitted the passphrase. Verify
            // it came through the wire and complete the ceremony.
            let pass = req
                .data
                .get("passphrase")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            assert_eq!(pass, "test-passphrase");
            let mut result = serde_json::Map::new();
            result.insert(
                "pond_name".into(),
                serde_json::Value::String("test-pond".into()),
            );
            result.insert(
                "cornerstone".into(),
                serde_json::Value::String("test-stone".into()),
            );
            return Json(CeremonyResponse {
                session_id,
                prompts: Vec::new(),
                messages: vec![Message {
                    kind: MessageKind::Summary,
                    title: "Pond placed".to_string(),
                    content: "test-pond / test-stone".to_string(),
                }],
                complete: true,
                error: None,
                result_data: Some(result),
            });
        }

        unreachable!("test only drives 2 steps");
    }

    async fn spawn_fixture() -> (String, StepCounter) {
        let counter: StepCounter = Arc::new(Mutex::new(0));
        let app = Router::new()
            .route("/api/v1/pond/ceremony", post(ceremony_handler))
            .with_state(counter.clone());

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        (
            format!("http://{addr}/api/v1/pond/ceremony"),
            counter,
        )
    }

    #[tokio::test]
    async fn full_init_ceremony_round_trip() {
        let (url, counter) = spawn_fixture().await;
        let client = reqwest::Client::new();

        // Step 1 — start.
        let initial = CeremonyRequest {
            session_id: None,
            ceremony: Some("init".to_string()),
            data: serde_json::Map::new(),
            render: None,
        };
        let resp1 = post_ceremony_step(&client, &url, initial).await.unwrap();
        assert!(!resp1.complete, "step 1 must not be complete");
        assert_eq!(resp1.prompts.len(), 1);
        assert_eq!(resp1.prompts[0].key, "passphrase");
        assert_eq!(resp1.messages.len(), 1);

        // Step 2 — submit passphrase.
        let mut data = serde_json::Map::new();
        data.insert(
            "passphrase".into(),
            serde_json::Value::String("test-passphrase".into()),
        );
        let req2 = CeremonyRequest {
            session_id: Some(resp1.session_id),
            ceremony: None,
            data,
            render: None,
        };
        let resp2 = post_ceremony_step(&client, &url, req2).await.unwrap();
        assert!(resp2.complete, "step 2 must be complete");
        assert_eq!(resp2.session_id, resp1.session_id);
        assert!(resp2.error.is_none());
        let result = resp2.result_data.expect("result_data on completion");
        assert_eq!(
            result.get("pond_name").and_then(|v| v.as_str()),
            Some("test-pond")
        );

        // Counter should have ticked exactly twice — no spurious
        // re-fires from the test driver.
        assert_eq!(*counter.lock().unwrap(), 2);
    }

    #[tokio::test]
    async fn server_error_propagates_with_status_and_body() {
        // Tiny fixture that always 500s.
        let app = Router::new().route(
            "/api/v1/pond/ceremony",
            post(|| async {
                (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "ceremony exploded",
                )
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let url = format!("http://{addr}/api/v1/pond/ceremony");
        let client = reqwest::Client::new();
        let req = CeremonyRequest {
            session_id: None,
            ceremony: Some("init".to_string()),
            data: serde_json::Map::new(),
            render: None,
        };
        let err = post_ceremony_step(&client, &url, req).await.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("500"),
            "error must surface the status: {msg}"
        );
        assert!(
            msg.contains("ceremony exploded"),
            "error must surface the body: {msg}"
        );
    }

    #[tokio::test]
    async fn default_render_hints_are_set_when_caller_leaves_them_unset() {
        // Server captures the render hint payload to verify that the
        // default landed even though the caller passed None.
        // We capture only the render field (Clone) rather than the
        // full request, since CeremonyRequest is not Clone.
        let captured_qr: Arc<Mutex<Option<QrFormat>>> = Arc::new(Mutex::new(None));
        let captured_inner = captured_qr.clone();
        let app = Router::new().route(
            "/api/v1/pond/ceremony",
            post(move |Json(req): Json<CeremonyRequest>| {
                let captured = captured_inner.clone();
                async move {
                    let qr = req.render.and_then(|r| r.qr);
                    *captured.lock().unwrap() = qr;
                    Json(CeremonyResponse {
                        session_id: Uuid::now_v7(),
                        prompts: Vec::new(),
                        messages: Vec::new(),
                        complete: true,
                        error: None,
                        result_data: Some(serde_json::Map::new()),
                    })
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let url = format!("http://{addr}/api/v1/pond/ceremony");
        let client = reqwest::Client::new();
        let req = CeremonyRequest {
            session_id: None,
            ceremony: Some("init".to_string()),
            data: serde_json::Map::new(),
            render: None, // explicitly omitted
        };
        post_ceremony_step(&client, &url, req).await.unwrap();

        let received_qr = captured_qr
            .lock()
            .unwrap()
            .expect("server should have observed a render hint");
        assert_eq!(
            received_qr,
            QrFormat::PngBase64,
            "default QR format should be PngBase64 for the modal"
        );
    }
}
