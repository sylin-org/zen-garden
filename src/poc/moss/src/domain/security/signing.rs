//! Inter-stone request signing (Moss→Moss control plane).
//!
//! rake signs its mutating requests via the loopback oracle; the renewal flow
//! signs `/pond/renew` with `core.sign`. The remaining privileged Moss→Moss
//! mutations (companion command broadcast, garden updates dispatch, cross-stone
//! capability mirror) used plain clients with no envelope. Under Stage 4
//! enforcement those would be rejected, so they sign here too — using the same
//! proven pattern as renewal: build the body once, sign the canonical bytes with
//! this stone's in-process identity key, and attach the envelope as the
//! `X-Koi-Envelope` header.
//!
//! Signing is **best-effort by design**: when this stone holds no usable
//! certmesh core (an Open, non-pond garden), the calls go out unsigned exactly as
//! before. Enforcement only verifies when the pond is active, so an unsigned call
//! in a pond-less garden is never rejected.

use std::sync::Arc;

use crate::Moss;

/// The certmesh core to sign with, or `None` when this stone has no usable koi
/// certmesh (no pond / koi not ready) — callers then send the request unsigned,
/// preserving non-pond behavior.
pub fn core_for_signing(state: &Moss) -> Option<Arc<koi_certmesh::CertmeshCore>> {
    state.discovery.koi().certmesh().and_then(|h| h.core()).ok()
}

/// Sign the canonical bytes of an inter-stone request with this stone's identity
/// and return the `X-Koi-Envelope` header value to attach.
///
/// `audience` MUST be the **target stone's name** (what the receiver rebuilds as
/// its own name when verifying); `path` is the request path verbatim (with query
/// if any); `body` is the exact serialized body the caller will send — sign and
/// send the *same* bytes so the receiver's body-hash bind-check matches.
///
/// Returns `None` only if the envelope fails to serialize (logged) — `core.sign`
/// itself is mode-transparent (an Open posture yields an unsigned passthrough
/// envelope; an Authenticated one an ES256 signature).
pub async fn inter_stone_envelope(
    core: &koi_certmesh::CertmeshCore,
    method: &str,
    audience: &str,
    path: &str,
    body: &[u8],
) -> Option<String> {
    let canonical =
        garden_common::pond_authz::canonical_request_bytes_for(method, path, audience, body);
    let envelope = core.sign(&canonical).await;
    match serde_json::to_string(&envelope) {
        Ok(header) => Some(header),
        Err(e) => {
            tracing::warn!(error = %e, "Failed to serialize inter-stone envelope; sending unsigned");
            None
        }
    }
}
