//! Member-side pond renewal — rotate this stone's leaf over the clear plane.
//!
//! zen drives renewal itself (it arms neither koi's `ensure_identity` nor
//! `.certmesh_background`, so koi never auto-renews). When a member's leaf passes
//! its renewal threshold, this flow:
//!
//! 1. generates a fresh keypair + CSR locally ([`prepare_member_csr`] — the
//!    private key never leaves this daemon),
//! 2. signs the canonical request bytes (audience = the cornerstone's name) with
//!    the in-process identity key — `core.sign`, not the loopback oracle, because
//!    Moss holds its own key and the oracle is only for rake's keyless requests,
//! 3. POSTs `{hostname, csr}` to the cornerstone's [`POND_RENEW_PATH`] carrying
//!    the signed envelope, and
//! 4. installs the returned leaf next to the rotated key ([`install_member_cert`]).
//!
//! The bytes signed and the bytes sent are the *same* serialized body, so the
//! cornerstone's `identity_for` bind-check (which hashes the body it received)
//! matches what was signed.
//!
//! The cornerstone's *own* self leaf is renewed by the sibling
//! [`renew_cornerstone_self_leaf_if_due`] (koi's local re-issue from its own CA) —
//! the member→CA plane cannot renew the CA's identity, so [`renew_member_identity`]
//! skips the cornerstone and that function handles it. zen drives both from its own
//! renewal timer because it does not run koi's background renewal loop.
//!
//! ## Renewal threshold (koi ADR-022 N4)
//!
//! The due-check uses [`RenewalHealth::renew_overdue`], which koi derives from
//! this node's *local* policy. Because zen members do not arm `member.json`, that
//! policy is koi's conservative default rather than the CA's configured
//! threshold. The expiry facts (`expires_at`, `expired`) are always exact; only
//! the threshold-derived trigger may fire slightly early — which is safe. The CA
//! does return its real policy on `RenewResponse.policy` (N4), but zen has no
//! `member.json` to persist it into, so it is deliberately not consumed: wiring
//! it through with nowhere to apply it would be a stub, not a feature.

use anyhow::{Context, Result, anyhow};
use garden_common::api_utils::ApiErrorResponse;
use serde::{Deserialize, Serialize};

use super::cornerstone;
use crate::Moss;

/// The clear-plane renewal endpoint path. Both the member (when signing the
/// renewal envelope) and the CA (when rebuilding the canonical bytes to
/// bind-check) must use this exact string, so it lives in one place.
pub const POND_RENEW_PATH: &str = "/api/v1/pond/renew";

/// Error code the cornerstone returns when a renewal is refused because the
/// signer's identity is past its grace window — the one non-transient outcome.
const REJOIN_REQUIRED_CODE: &str = "POND_REJOIN_REQUIRED";

#[derive(Serialize, Deserialize)]
pub struct PondRenewRequest {
    /// The renewing member's hostname (CN). Informational only — the
    /// authoritative identity is the envelope signer, never this field.
    pub hostname: String,
    /// PKCS#10 CSR (PEM) for the member's freshly rotated keypair.
    pub csr: String,
}

#[derive(Serialize, Deserialize)]
pub struct PondRenewResponse {
    /// The renewed CA-signed leaf certificate (PEM).
    pub service_cert: String,
    /// The CA root certificate (PEM).
    pub ca_cert: String,
    /// CA fingerprint, for the member to cross-check against its pin.
    pub ca_fingerprint: String,
    /// RFC 3339 expiry of the renewed leaf.
    pub expires: String,
}

/// What a renewal attempt resolved to. The background loop reacts to each:
/// `Renewed` emits a felt-safety event, `RejoinRequired` warns warmly (retrying
/// never helps), `NotDue`/`Skipped` are quiet no-ops. A transient failure is an
/// `Err` (the loop logs and retries next tick), never a variant here.
pub enum RenewOutcome {
    /// Leaf rotated and installed; carries the new RFC 3339 expiry.
    Renewed { expires: String },
    /// Not yet past the renewal threshold — nothing to do.
    NotDue { expires_in_days: i64 },
    /// Nothing to renew on this stone right now.
    Skipped { reason: &'static str },
    /// The cornerstone refused: this stone's identity is past its grace window.
    /// Automatic renewal cannot recover it — the operator must rejoin.
    RejoinRequired { reason: String },
}

/// Renew this stone's pond leaf if it is due. Idempotent and safe to call on any
/// stone in any posture — it returns [`RenewOutcome::Skipped`]/`NotDue` when there
/// is nothing to do.
pub async fn renew_member_identity(state: &Moss) -> Result<RenewOutcome> {
    let core = state
        .discovery
        .koi()
        .certmesh()
        .and_then(|h| h.core())
        .map_err(|e| anyhow!("certmesh core unavailable: {e}"))?;

    // The cornerstone holds the CA; its self leaf is not renewed over the
    // member→CA plane (it would have to ask itself). The sibling
    // `renew_cornerstone_self_leaf_if_due` re-issues it locally instead.
    if core.certmesh_status().await.ca_initialized {
        return Ok(RenewOutcome::Skipped {
            reason: "cornerstone — its CA self leaf is renewed locally by renew_cornerstone_self_leaf_if_due, not the member plane",
        });
    }

    // No usable identity → Open posture, nothing to renew.
    let identity = match core.local_identity().await {
        Some(id) => id,
        None => {
            return Ok(RenewOutcome::Skipped {
                reason: "no pond identity to renew (open posture)",
            });
        }
    };

    if !identity.renewal.renew_overdue {
        return Ok(RenewOutcome::NotDue {
            expires_in_days: identity.renewal.expires_in_days,
        });
    }

    let hostname = identity.hostname.clone();

    // Find the cornerstone (name = the audience the envelope binds to; address =
    // where to send). A discovery miss is transient — retry on the next tick.
    let cornerstone = cornerstone::discover(state)
        .await
        .map_err(|e| anyhow!("cannot reach cornerstone to renew: {e}"))?;

    // Fresh keypair + CSR. We request only the hostname as a SAN; koi pins
    // renewal SANs to the enrollment record (which always includes the hostname),
    // and substitutes the authorized SAN set into the issued leaf regardless.
    let csr = core
        .prepare_member_csr(&hostname, std::slice::from_ref(&hostname))
        .await
        .map_err(|e| anyhow!("prepare member CSR failed: {e}"))?;

    // Serialize the body ONCE: the bytes we hash into the signature must be the
    // exact bytes we send, so the CA's body-hash bind-check matches.
    let req = PondRenewRequest {
        hostname: hostname.clone(),
        csr,
    };
    let body = serde_json::to_vec(&req).context("serialize renew request")?;

    // Sign the canonical bytes (audience = cornerstone name) with the in-process
    // identity key. `core.sign` carries the leaf so the CA can verify the chain.
    let canonical = garden_common::pond_authz::canonical_request_bytes_for(
        "POST",
        POND_RENEW_PATH,
        &cornerstone.name,
        &body,
    );
    let envelope = core.sign(&canonical).await;
    let envelope_header = serde_json::to_string(&envelope).context("serialize signed envelope")?;

    // POST the exact signed bytes to the cornerstone.
    let resp = state
        .security
        .stone_client()
        .post(&cornerstone.address, POND_RENEW_PATH)
        .timeout(garden_common::constants::timeouts::pond_operation_timeout())
        .header(
            garden_common::constants::headers::HEADER_KOI_ENVELOPE,
            envelope_header,
        )
        .header("content-type", "application/json")
        .body(body)
        .send()
        .await
        .with_context(|| {
            format!(
                "POST renew to cornerstone '{}' at {}",
                cornerstone.name, cornerstone.address
            )
        })?;

    let status = resp.status();
    if !status.is_success() {
        let err_body = resp.text().await.unwrap_or_default();
        // A past-grace identity is a warm "rejoin", not a transient failure —
        // surface it distinctly so the loop stops retrying and prompts the user.
        if let Ok(parsed) = serde_json::from_str::<ApiErrorResponse>(&err_body) {
            if parsed.error.code == REJOIN_REQUIRED_CODE {
                return Ok(RenewOutcome::RejoinRequired {
                    reason: parsed.error.message,
                });
            }
            anyhow::bail!(
                "cornerstone refused renewal ({status}): {}",
                parsed.error.message
            );
        }
        anyhow::bail!("cornerstone refused renewal ({status}): {err_body}");
    }

    // Unwrap the ApiResponse envelope → the renew payload.
    let value: serde_json::Value = resp.json().await.context("parse renew response")?;
    let data = value
        .get("data")
        .ok_or_else(|| anyhow!("renew response missing 'data' field"))?;
    let renew: PondRenewResponse =
        serde_json::from_value(data.clone()).context("decode renew response")?;

    // Install the rotated leaf next to the new key. No CA endpoint/fingerprint:
    // this arms no koi mTLS pull-renewal (zen drives renewal over this plane), so
    // the unused SAN list is empty.
    core.install_member_cert(
        &hostname,
        &renew.service_cert,
        &renew.ca_cert,
        None,
        None,
        None,
        &[],
        None,
    )
    .await
    .map_err(|e| anyhow!("install renewed cert failed: {e}"))?;

    tracing::info!(
        stone = %hostname,
        cornerstone = %cornerstone.name,
        expires = %renew.expires,
        "Pond identity renewed (envelope-signed over the clear plane)"
    );

    Ok(RenewOutcome::Renewed {
        expires: renew.expires,
    })
}

/// Keep this stone's CA **self leaf** fresh when it is the cornerstone.
///
/// A cheap no-op on a member (koi returns `NotApplicable`). On the cornerstone it
/// drives koi's local re-issue ([`CertmeshCore::renew_ca_self_leaf_if_due`]): koi
/// runs the due-check and, when the self leaf is within the renewal threshold,
/// re-issues it from the local CA (no network). Historically that re-issue ran
/// only at daemon start, so a continuously-up cornerstone could cross its
/// threshold and expire without a restart — this closes that gap on the timer.
///
/// koi emits the lifecycle events itself — `CertRenewed` on success;
/// `CertRenewalFailed` + `CertExpiringSoon` when a locked CA cannot re-issue while
/// overdue — on its own stream, which the `koi_events` bridge forwards to the
/// event bus. So the caller only logs a failure and never re-emits (that would
/// double the PondEvents). zen drives this from its own renewal timer rather than
/// koi's background loop (which zen does not run); it is the cornerstone
/// counterpart of the member clear-plane flow in [`renew_member_identity`].
///
/// [`CertmeshCore::renew_ca_self_leaf_if_due`]: koi_certmesh::CertmeshCore::renew_ca_self_leaf_if_due
pub async fn renew_cornerstone_self_leaf_if_due(state: &Moss) -> Result<()> {
    let core = state
        .discovery
        .koi()
        .certmesh()
        .and_then(|h| h.core())
        .map_err(|e| anyhow!("certmesh core unavailable: {e}"))?;

    match core.renew_ca_self_leaf_if_due().await {
        // NotApplicable (member) / NotDue / Renewed — koi already emitted any
        // lifecycle event; we only trace the check.
        Ok(outcome) => {
            tracing::debug!(?outcome, "CA self-leaf renewal check");
            Ok(())
        }
        // koi has already emitted CertRenewalFailed + CertExpiringSoon on its
        // stream (forwarded by the bridge); surface the error so the caller logs.
        Err(e) => Err(anyhow!("CA self-leaf renewal failed (CA locked?): {e}")),
    }
}
