//! Pond authorization plane — shared signing primitives (clear-signed requests).
//!
//! The pond authenticates each request with a koi clear-signed
//! [`Envelope`](koi_common::envelope::Envelope) carried in the
//! [`HEADER_KOI_ENVELOPE`](crate::constants::headers::HEADER_KOI_ENVELOPE) header.
//! The envelope's signature covers the **canonical request bytes** produced here.
//! Binding those bytes to the verb, the exact path+query, the destination stone
//! (audience), and a hash of the body means a captured signature cannot be:
//!
//! - lifted to a different operation (the method/path are bound),
//! - applied to a different body (the body hash is bound),
//! - replayed to a different stone within the ±300s freshness window (the
//!   audience — the target stone's name — is bound; all stones share one CA, so
//!   without this an envelope would verify at any of them).
//!
//! Freshness/replay-window enforcement (±300s) is koi's; koi keeps **no** seen-
//! nonce cache, so single-use replay defence for destructive operations is the
//! consumer's responsibility (the verifier side).
//!
//! Both ends MUST build the canonical bytes through these functions so the
//! signer (the origin stone's Moss) and the verifier (the target stone's Moss)
//! agree byte-for-byte. The signer is given the body hash (so the loopback sign
//! oracle never carries the body); the verifier hashes the body it actually
//! received.

/// Domain-separation tag for v1 canonical pond-request bytes. Distinct from any
/// other zen signing context so a signature can never be replayed across zen's
/// own protocols. Sits *inside* koi's envelope, which adds its own
/// `koi-envelope-v1` domain prefix over these bytes.
const POND_REQUEST_DOMAIN_V1: &str = "zen-pond-request-v1";

/// Lowercase hex of the blake3 hash of `body`. The body's contribution to the
/// canonical bytes — passed to the signer so the body itself need not travel to
/// the loopback sign oracle, and recomputed by the verifier over the bytes it
/// actually received.
pub fn body_hash_hex(body: &[u8]) -> String {
    blake3::hash(body).to_hex().to_string()
}

/// The exact bytes a pond request's envelope signature covers (v1).
///
/// `method` is uppercased so `get`/`GET` canonicalize identically; `path_and_query`
/// is the request target verbatim (e.g. axum's `uri.path_and_query()`); `audience`
/// is the destination stone's name; `body_hash_hex` is [`body_hash_hex`] of the
/// request body. Deterministic and trivially reproducible.
pub fn canonical_request_bytes(
    method: &str,
    path_and_query: &str,
    audience: &str,
    body_hash_hex: &str,
) -> Vec<u8> {
    let method = method.to_ascii_uppercase();
    format!("{POND_REQUEST_DOMAIN_V1}\n{method}\n{path_and_query}\n{audience}\n{body_hash_hex}")
        .into_bytes()
}

/// Convenience: hash `body` and build the canonical bytes in one call — the form
/// the verifier uses (it has the real body in hand).
pub fn canonical_request_bytes_for(
    method: &str,
    path_and_query: &str,
    audience: &str,
    body: &[u8],
) -> Vec<u8> {
    canonical_request_bytes(method, path_and_query, audience, &body_hash_hex(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_is_deterministic() {
        let a = canonical_request_bytes_for("POST", "/api/v1/x?y=1", "stone-a", b"body");
        let b = canonical_request_bytes_for("POST", "/api/v1/x?y=1", "stone-a", b"body");
        assert_eq!(a, b);
    }

    #[test]
    fn method_is_case_insensitive() {
        let upper = canonical_request_bytes("POST", "/p", "s", "h");
        let lower = canonical_request_bytes("post", "/p", "s", "h");
        assert_eq!(upper, lower);
    }

    #[test]
    fn each_field_changes_the_bytes() {
        let base = canonical_request_bytes_for("POST", "/p", "stone-a", b"body");
        assert_ne!(base, canonical_request_bytes_for("PUT", "/p", "stone-a", b"body"));
        assert_ne!(base, canonical_request_bytes_for("POST", "/q", "stone-a", b"body"));
        assert_ne!(base, canonical_request_bytes_for("POST", "/p", "stone-b", b"body"));
        assert_ne!(base, canonical_request_bytes_for("POST", "/p", "stone-a", b"other"));
    }

    #[test]
    fn signer_hash_path_equals_verifier_body_path() {
        // The signer is handed the hash; the verifier hashes the real body.
        // Both must yield identical canonical bytes.
        let body = b"the actual request body";
        let from_hash =
            canonical_request_bytes("DELETE", "/api/v1/stone/x", "stone-z", &body_hash_hex(body));
        let from_body = canonical_request_bytes_for("DELETE", "/api/v1/stone/x", "stone-z", body);
        assert_eq!(from_hash, from_body);
    }

    #[test]
    fn body_hash_is_stable_and_distinct() {
        assert_eq!(body_hash_hex(b""), body_hash_hex(b""));
        assert_ne!(body_hash_hex(b"a"), body_hash_hex(b"b"));
        // blake3 hex is 64 chars (32 bytes).
        assert_eq!(body_hash_hex(b"x").len(), 64);
    }
}
