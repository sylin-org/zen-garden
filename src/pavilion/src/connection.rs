//! HTTP client construction for talking to the tended stone.
//!
//! Builds a [`garden_common::StoneApi`] pointing at the currently
//! tended endpoint. The client is constructed per-call rather than
//! cached because (a) endpoints change when tending changes, and (b)
//! the HTTP overhead of building a client is dominated by the actual
//! request anyway.
//!
//! Pre-Phase-4: `danger_accept_invalid_certs(true)`. Moss currently
//! serves a Koi-CA-signed cert that the system trust store doesn't
//! know; the LAN-only deployment makes this acceptable for now.
//! [`PAVILION-0001`](../../docs/decisions/PAVILION-0001-windows-client-separation.md)
//! §"Authentication boundary" tracks the mTLS upgrade.

use std::time::Duration;

use garden_common::StoneApi;

use crate::tending::TendedStone;

/// Build a fresh [`StoneApi`] pointing at `tended.endpoint`.
pub fn api_for(tended: &TendedStone) -> StoneApi {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .danger_accept_invalid_certs(true)
        .build()
        .expect("building reqwest client should not fail");
    StoneApi::new(client, tended.endpoint.clone())
}
