//! Stage 4 enforcement — verify clear-signed envelopes on the pond control plane.
//!
//! A request middleware that authorizes mutating control-plane requests by the
//! koi envelope they carry, generalizing the per-handler verify the `/pond/renew`
//! CA handler already does: parse `X-Koi-Envelope`, `core.verify`, then bind via
//! `Assurance::identity_for(env, canonical)` where the canonical bytes are rebuilt
//! from THIS request (verb + path?query + this stone's own name as audience +
//! blake3 of the received body). A verified signer that is bound to this exact
//! request, with a fresh (non-replayed) nonce, is authorized; anything else is
//! denied — or, in observe mode, logged and allowed.
//!
//! Scope (see [`requires_envelope`]): only mutating verbs on `/api/v1/stone/` and
//! `/api/v1/garden/`, minus the infra self-update endpoints (`deploy`/`upgrade`,
//! plain-HTTP by design) and the cross-stone storage data plane. Reads, the pond
//! bootstrap/recovery routes (`/api/v1/pond/*` — `/pond/renew` self-verifies), the
//! S3/WebDAV/browser data plane, and `/health` are never enforced.
//!
//! Rollout is staged via [`PondEnforceMode`] (`ZG_POND_ENFORCE`, default Observe):
//! Off → no verification; Observe → verify + warn on would-reject but allow;
//! Enforce → reject. Enforcement is additionally gated on this stone holding a pond
//! identity (an Open stone has no CA anchor to verify against, so it passes).

use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, Method, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::Moss;
use crate::domain::security::signing::core_for_signing;

/// Max control-plane body buffered for verification. Control-plane payloads are
/// small JSON; the data plane (large uploads) bypasses the enforce scope before
/// the body is ever touched, so it is never buffered here.
const MAX_VERIFY_BODY: usize = 16 * 1024 * 1024;

/// How strictly this stone enforces signed envelopes on the control plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PondEnforceMode {
    /// No verification at all.
    Off,
    /// Verify and emit a structured "would-reject" warning, but ALLOW the request.
    /// The safe default — surfaces unsigned/mismatched traffic before enforcing.
    Observe,
    /// Verify and REJECT unsigned / invalid / replayed control-plane mutations.
    Enforce,
}

impl PondEnforceMode {
    /// Read from `ZG_POND_ENFORCE` (`off` | `observe` | `enforce`); default Observe.
    pub fn from_env() -> Self {
        let raw = std::env::var("ZG_POND_ENFORCE").ok();
        match raw.as_deref().map(|s| s.trim().to_ascii_lowercase()).as_deref() {
            Some("off") => Self::Off,
            Some("enforce") => Self::Enforce,
            None | Some("") | Some("observe") => Self::Observe,
            Some(other) => {
                tracing::warn!(value = %other, "Unknown ZG_POND_ENFORCE value; defaulting to observe");
                Self::Observe
            }
        }
    }
}

/// Whether a request must carry a verified envelope.
fn requires_envelope(method: &Method, path: &str) -> bool {
    // Reads are never signed.
    if matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return false;
    }
    // The rake/inter-stone control plane: stone ops, garden ops, and the privileged
    // admin surface. rake signs all three via the typed StoneApi (admin
    // shutdown/reboot/wake go through `api.stone()`), so enforcing /api/v1/admin/ is
    // safe — its mutating routes are exactly those signed control operations.
    let control = path.starts_with("/api/v1/stone/")
        || path.starts_with("/api/v1/garden/")
        || path.starts_with("/api/v1/admin/");
    if !control {
        return false;
    }
    // Un-signable infra / data planes — clients here hold no koi identity (exactly
    // like the deployer), so enforcing them would brick the fleet:
    //  - infra self-update (deploy/upgrade),
    //  - cross-stone storage data plane (fs/objects),
    //  - orchestrator gateway registration (standalone crates with no koi identity,
    //    a ~60s-TTL heartbeat — the ollama/mongodb registry contract, STACK-0001),
    //  - offering volume writes (orchestrator large data-plane file I/O).
    if path.starts_with("/api/v1/stone/deploy") || path.starts_with("/api/v1/stone/upgrade") {
        return false;
    }
    if path.starts_with("/api/v1/garden/storage/") {
        return false;
    }
    if path.starts_with("/api/v1/garden/gateway/") {
        return false;
    }
    if path.starts_with("/api/v1/stone/offerings/") && path.contains("/volumes/") {
        return false;
    }
    true
}

/// Parse the `X-Koi-Envelope` header, or `None` if absent/malformed.
fn envelope_from_headers(headers: &HeaderMap) -> Option<koi_common::envelope::Envelope> {
    let raw = headers
        .get(garden_common::constants::headers::HEADER_KOI_ENVELOPE)?
        .to_str()
        .ok()?;
    serde_json::from_str(raw).ok()
}

/// Why a control-plane request was denied (or would be, in observe mode).
enum Denial {
    /// No / malformed `X-Koi-Envelope`.
    Unsigned,
    /// A previously-seen `(signer, nonce)` — replay within the freshness window.
    Replay,
    /// Verified-but-unauthorized: bad signature, expired/unknown signer, wrong
    /// audience, or payload not bound to this request. Carries the rich response
    /// (the warm "rejoin" prompt for an expired identity).
    Unverified(Response),
}

impl Denial {
    fn reason(&self) -> &'static str {
        match self {
            Denial::Unsigned => "missing or malformed signature",
            Denial::Replay => "replayed nonce",
            Denial::Unverified(_) => "signature did not verify / not bound to this request",
        }
    }

    fn into_response(self) -> Response {
        match self {
            Denial::Unsigned => crate::error_response(
                StatusCode::UNAUTHORIZED,
                "ENVELOPE_REQUIRED",
                "This operation requires a signed request (no valid X-Koi-Envelope).",
                None,
            )
            .into_response(),
            Denial::Replay => crate::error_response(
                StatusCode::CONFLICT,
                "REPLAY_DETECTED",
                "This signed request was already used (replay rejected).",
                None,
            )
            .into_response(),
            Denial::Unverified(response) => response,
        }
    }
}

/// Verify-and-authorize middleware for the pond control plane.
pub async fn enforce_envelope(
    State(state): State<Moss>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let mode = state.security.enforce_mode();
    if mode == PondEnforceMode::Off {
        return next.run(request).await;
    }

    let method = request.method().clone();
    let path = request.uri().path().to_string();
    if !requires_envelope(&method, &path) {
        return next.run(request).await;
    }

    // Only a stone that holds a pond identity can verify peers — an Open stone has
    // no CA anchor, so every envelope would read as Anonymous. Pass through (no
    // enforcement) until this stone is enrolled.
    let core = match core_for_signing(&state) {
        Some(core) => core,
        None => return next.run(request).await,
    };
    if core.local_identity().await.is_none() {
        return next.run(request).await;
    }

    // Path+query verbatim — must match what the signer bound, or the body-hash
    // bind-check fails. The body is buffered (bounded) to hash it.
    let path_and_query = request
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str().to_owned())
        .unwrap_or_else(|| path.clone());

    let (parts, body) = request.into_parts();
    let bytes = match axum::body::to_bytes(body, MAX_VERIFY_BODY).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return crate::error_response(
                StatusCode::PAYLOAD_TOO_LARGE,
                "BODY_TOO_LARGE",
                "Control-plane request body is too large to verify.",
                None,
            )
            .into_response();
        }
    };

    let decision: Result<(), Denial> = match envelope_from_headers(&parts.headers) {
        None => Err(Denial::Unsigned),
        Some(envelope) => {
            let assurance = core.verify(&envelope).await;
            let canonical = garden_common::pond_authz::canonical_request_bytes_for(
                method.as_str(),
                &path_and_query,
                &state.current.stone.name,
                &bytes,
            );
            match assurance.identity_for(&envelope, &canonical) {
                // Authenticated + fresh + bound to this request. Record the nonce
                // (only now that it is trusted) for single-use replay defence.
                Some(cn) => {
                    if state
                        .security
                        .nonce_cache()
                        .check_and_record(cn, &envelope.nonce, envelope.ts)
                    {
                        Ok(())
                    } else {
                        Err(Denial::Replay)
                    }
                }
                None => Err(Denial::Unverified(
                    crate::api::v1::pond::reject_to_response(&assurance).into_response(),
                )),
            }
        }
    };

    match decision {
        Ok(()) => {
            let request = Request::from_parts(parts, Body::from(bytes));
            next.run(request).await
        }
        Err(denial) => match mode {
            PondEnforceMode::Enforce => denial.into_response(),
            // Observe (and Off is handled above): log loudly, allow through. This
            // is the rollout's safe phase — it shows exactly which peers do not
            // sign yet, fleet-wide, without blocking anything.
            _ => {
                tracing::warn!(
                    method = %method,
                    path = %path,
                    reason = denial.reason(),
                    "pond enforce(observe): would reject this control-plane request"
                );
                let request = Request::from_parts(parts, Body::from(bytes));
                next.run(request).await
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_never_require_envelope() {
        assert!(!requires_envelope(&Method::GET, "/api/v1/stone/offerings"));
        assert!(!requires_envelope(&Method::HEAD, "/api/v1/garden/banks"));
    }

    #[test]
    fn control_mutations_require_envelope() {
        assert!(requires_envelope(&Method::POST, "/api/v1/stone/offerings"));
        assert!(requires_envelope(&Method::DELETE, "/api/v1/stone/offerings/x"));
        assert!(requires_envelope(&Method::POST, "/api/v1/stone/companions/abc/command"));
        assert!(requires_envelope(&Method::POST, "/api/v1/stone/updates/execute"));
        assert!(requires_envelope(&Method::POST, "/api/v1/garden/updates/execute"));
        // Privileged admin surface is enforced (rake signs these via api.stone()).
        assert!(requires_envelope(&Method::POST, "/api/v1/admin/stone/reboot"));
        assert!(requires_envelope(&Method::POST, "/api/v1/admin/stone/shutdown"));
    }

    #[test]
    fn bootstrap_data_and_infra_bypass() {
        // Pond bootstrap/recovery (not under /stone or /garden).
        assert!(!requires_envelope(&Method::POST, "/api/v1/pond/join"));
        assert!(!requires_envelope(&Method::POST, "/api/v1/pond/renew"));
        assert!(!requires_envelope(&Method::DELETE, "/api/v1/pond"));
        // Infra self-update.
        assert!(!requires_envelope(&Method::POST, "/api/v1/stone/deploy"));
        assert!(!requires_envelope(&Method::POST, "/api/v1/stone/upgrade"));
        // Data plane.
        assert!(!requires_envelope(&Method::PUT, "/api/v1/storage/s3/bucket/key"));
        assert!(!requires_envelope(&Method::PUT, "/dav/personal/file.txt"));
        assert!(!requires_envelope(&Method::PUT, "/api/v1/garden/storage/personal/objects/x"));
        // Un-signable orchestrator planes (standalone crates, no koi identity).
        assert!(!requires_envelope(&Method::PUT, "/api/v1/garden/gateway/ollama"));
        assert!(!requires_envelope(&Method::DELETE, "/api/v1/garden/gateway/ollama"));
        assert!(!requires_envelope(
            &Method::PUT,
            "/api/v1/stone/offerings/ollama/volumes/data/models/x.bin"
        ));
    }

    #[test]
    fn default_mode_is_observe() {
        // With ZG_POND_ENFORCE unset (the test environment), the rollout starts in
        // the safe observe posture, never silently enforcing.
        assert_eq!(PondEnforceMode::from_env(), PondEnforceMode::Observe);
    }
}
