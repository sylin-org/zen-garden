//! Cloud Filter integration — Pavilion's Windows Explorer sync provider.
//!
//! Backed by the tended stone's REST API (PAVILION-0001 §"Cloud Filter
//! migration"). The sync root lives at `%USERPROFILE%\Zen Garden\` and
//! every directory or file access translates into a request against
//! `/api/v1/garden/storage` on the tended stone.
//!
//! ## Lifecycle
//!
//! - [`start`] waits for the initial tending, builds a `StoneApi`, then
//!   registers the sync root and connects [`PavilionProvider`]. The
//!   connection is held in a static so it lives for the process
//!   lifetime; dropping it disconnects the provider.
//! - [`stop`] disconnects the provider but **leaves the sync root
//!   registered** so the next launch reuses it.
//! - [`uninstall`] disconnects AND unregisters — call from the NSIS
//!   uninstall hook only.
//!
//! ## Module layout
//!
//! - `registration.rs` — sync root registration (lifted from Moss)
//! - `placeholders.rs` — Cloud Filter placeholder builders
//! - `provider.rs`     — `Filter` impl that delegates to `StoneApi`

mod placeholders;
mod provider;
pub(crate) mod registration;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use cloud_filter::root::{Connection, Session};

use crate::tending::Tending;

use self::provider::PavilionProvider;

/// Holds the Cloud Filter `Connection` alive for the process lifetime.
/// Dropping the connection disconnects the provider.
static CONNECTION: std::sync::Mutex<Option<Connection<PavilionProvider>>> =
    std::sync::Mutex::new(None);

/// Wait-for-initial-tend poll cadence. Cheap — just reads the in-memory
/// `RwLock<Option<TendingState>>` Tending already maintains.
const TEND_POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Register the sync root and connect the `PavilionProvider`.
///
/// Waits indefinitely for [`Tending`] to settle on a stone before
/// continuing — Cloud Filter has nothing to serve without a tended
/// endpoint. Idempotent: safe to call on every Pavilion startup, no-op
/// when [`cloud_filter::root::is_supported`] returns false.
pub async fn start(tending: Arc<Tending>) -> Result<()> {
    match cloud_filter::root::is_supported() {
        Ok(true) => {}
        Ok(false) => {
            tracing::info!("Cloud Filter API not supported on this Windows build");
            return Ok(());
        }
        Err(e) => {
            tracing::warn!(error = %e, "Cloud Filter support check failed");
            return Ok(());
        }
    }

    tracing::info!(
        elevated = registration::is_elevated(),
        service = registration::is_running_as_service(),
        username = %std::env::var("USERNAME").unwrap_or_default(),
        "Cloud Filter: process context"
    );

    // Block until Tending has resolved an anchor stone. Auto-tend in
    // tending.rs sits indefinitely waiting for a chirp; we mirror that
    // patience here so `start` doesn't fail on a cold garden.
    let tended = loop {
        if let Some(t) = tending.current().await {
            break t;
        }
        tokio::time::sleep(TEND_POLL_INTERVAL).await;
    };
    tracing::info!(
        stone = %tended.stone_name,
        endpoint = %tended.endpoint,
        "Cloud Filter: tended stone confirmed, building StoneApi"
    );

    let api = Arc::new(crate::connection::api_for(&tended));

    let sync_root_path = registration::ensure_registered().await?;

    // `PavilionProvider::new` captures `Handle::current()` — the
    // tokio runtime that drives Tauri — so each sync CfApi callback
    // can `block_on` to drive StoneApi futures. `connect()` (sync)
    // keeps the static type concrete; the alternative `connect_async`
    // wraps the provider in an `AsyncBridge<P, F>` whose closure type
    // can't be named in a `static`.
    let provider = PavilionProvider::new(api, sync_root_path.clone());
    let connection = Session::new()
        .connect(&sync_root_path, provider)
        .context("failed to connect Cloud Filter provider")?;

    {
        let mut slot = CONNECTION.lock().expect("CONNECTION mutex poisoned");
        *slot = Some(connection);
    }

    tracing::info!(
        path = %sync_root_path.display(),
        "Cloud Filter provider connected — sync root visible in Explorer"
    );
    Ok(())
}

/// Disconnect the provider on graceful shutdown. The sync root *stays
/// registered* so the next Pavilion launch can reuse it — full
/// teardown is the uninstaller's job (call [`uninstall`]).
pub fn stop() {
    let mut slot = CONNECTION.lock().expect("CONNECTION mutex poisoned");
    if slot.take().is_some() {
        tracing::info!("Cloud Filter provider disconnected");
    }
}

/// Full teardown — disconnect AND unregister the sync root. Intended
/// for the NSIS uninstall hook.
#[allow(dead_code)]
pub fn uninstall() {
    stop();
    if let Err(e) = registration::unregister() {
        tracing::warn!(error = %e, "Cloud Filter unregister failed");
    }
}
