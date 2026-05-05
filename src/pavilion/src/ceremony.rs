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

    // Default render hints: the modal renders QR codes as image
    // tags, so request the base64 PNG form. The frontend can
    // request a different format by passing it explicitly in
    // request.render — that takes precedence.
    let mut request = request;
    if request.render.is_none() {
        request.render = Some(RenderHints {
            qr: Some(QrFormat::PngBase64),
        });
    }

    // Build a per-call client — ceremonies are infrequent enough
    // that a fresh client per round-trip beats threading a shared
    // pool through the Tauri state surface.
    let client = reqwest::Client::builder()
        .timeout(CEREMONY_TIMEOUT)
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| format!("build ceremony client: {e}"))?;

    let resp = client
        .post(&url)
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("ceremony POST {url}: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("ceremony {status}: {body}"));
    }

    let parsed: CeremonyResponse = resp
        .json()
        .await
        .map_err(|e| format!("ceremony parse: {e}"))?;
    Ok(parsed)
}
