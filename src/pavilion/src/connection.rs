//! HTTP client construction for talking to the tended stone.
//!
//! Builds a [`garden_common::StoneApi`] pointing at the currently
//! tended endpoint. The client is constructed per-call rather than
//! cached because (a) endpoints change when tending changes, and (b)
//! the HTTP overhead of building a client is dominated by the actual
//! request anyway.
//!
//! Two flavours:
//!
//! - [`api_for`] — short-lived request/response calls (tile fetches,
//!   storage list, pond status). 8 s overall timeout — fails fast so
//!   a stale tending doesn't lock up the dashboard.
//! - [`streaming_api_for`] — long-lived SSE streams (storage tick
//!   observer). No overall timeout, but a per-read timeout that
//!   detects a dead connection after Moss's keep-alive interval has
//!   passed without traffic.
//!
//! Pre-Phase-4: `danger_accept_invalid_certs(true)`. Moss currently
//! serves a Koi-CA-signed cert that the system trust store doesn't
//! know; the LAN-only deployment makes this acceptable for now.
//! [`PAVILION-0001`](../../docs/decisions/PAVILION-0001-windows-client-separation.md)
//! §"Authentication boundary" tracks the mTLS upgrade.

use std::time::Duration;

use garden_common::StoneApi;

use crate::tending::TendedStone;

/// Build a fresh [`StoneApi`] pointing at `tended.endpoint` with the
/// short-lived-call timeout profile.
pub fn api_for(tended: &TendedStone) -> StoneApi {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .danger_accept_invalid_certs(true)
        .build()
        .expect("building reqwest client should not fail");
    StoneApi::new(client, tended.endpoint.clone())
}

/// Build a [`StoneApi`] tuned for SSE / long-lived streams — no
/// overall timeout, just a generous per-read timeout. Moss emits an
/// SSE keep-alive every 15 s by default, so 60 s of silence is a
/// strong signal the connection is dead and the observer should
/// reconnect.
pub fn streaming_api_for(tended: &TendedStone) -> StoneApi {
    let client = reqwest::Client::builder()
        .read_timeout(Duration::from_secs(60))
        .danger_accept_invalid_certs(true)
        .build()
        .expect("building reqwest client should not fail");
    StoneApi::new(client, tended.endpoint.clone())
}
