//! Lazy-fetching bag for stone metadata.
//!
//! Caches capabilities so they are fetched **at most once** per command
//! invocation.  When constructed via [`StoneBag::from_tending`], the
//! capabilities stored in the tending file are used directly — zero
//! network calls in the hot path.
//!
//! # Response header extraction
//!
//! Moss injects `X-Stone-Name` / `X-Stone-Id` on every response.
//! Use [`extract_stone_identity`] to read them from any
//! `reqwest::Response` — no dedicated call needed.
//!
//! # Usage
//!
//! ```ignore
//! // Hot path — capabilities already in tending cache → zero HTTP
//! let bag = StoneBag::from_tending(&tending_state, client);
//! let name = bag.stone_name().await;   // instant
//!
//! // Cold path — no tending cache → single GET /capabilities
//! let bag = StoneBag::new(client, endpoint);
//! let name = bag.stone_name().await;   // HTTP on first call
//! let caps = bag.capabilities().await; // instant (cached)
//!
//! // From any response's headers (zero extra calls)
//! let identity = extract_stone_identity(response.headers());
//! ```

use garden_common::client::StoneApi;
use garden_common::constants::headers::{HEADER_STONE_ID, HEADER_STONE_NAME};
use garden_common::HardwareCapabilities;
use tokio::sync::OnceCell;

/// Lazily-fetched stone metadata, scoped to a single command invocation.
///
/// Created once the endpoint is resolved.  The capabilities getter fetches
/// on the first call and returns the cached value on all subsequent calls.
/// When seeded from tending state, no HTTP call is needed at all.
pub struct StoneBag {
    client: reqwest::Client,
    endpoint: String,
    capabilities: OnceCell<Option<HardwareCapabilities>>,
}

impl StoneBag {
    /// Create a bag with no cached data.  The first call to
    /// [`capabilities()`](Self::capabilities) will trigger an HTTP fetch.
    pub fn new(client: reqwest::Client, endpoint: String) -> Self {
        Self {
            client,
            endpoint,
            capabilities: OnceCell::new(),
        }
    }

    /// Create a bag pre-seeded from a [`TendingState`](crate::tending::TendingState).
    ///
    /// If the tending file contains cached capabilities they are used
    /// directly, making `stone_name()` and `is_reachable()` free.
    pub fn from_tending(tending: &crate::tending::TendingState, client: reqwest::Client) -> Self {
        let bag = Self {
            client,
            endpoint: tending.endpoint.clone(),
            capabilities: OnceCell::new(),
        };
        if let Some(ref caps) = tending.capabilities {
            // Pre-seed — no HTTP needed
            let _ = bag.capabilities.set(Some(caps.clone()));
        }
        bag
    }

    /// The resolved stone endpoint this bag targets.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Lazily fetch stone capabilities.
    ///
    /// If the bag was seeded (via [`from_tending`](Self::from_tending)),
    /// returns the cached value immediately.  Otherwise triggers a single
    /// `GET /api/v1/stone/capabilities` call and caches the result.
    ///
    /// Returns `None` if the stone is unreachable or the response is
    /// malformed.
    pub async fn capabilities(&self) -> Option<&HardwareCapabilities> {
        self.capabilities
            .get_or_init(|| async {
                let api = StoneApi::new(self.client.clone(), self.endpoint.clone());
                tracing::debug!(endpoint = %self.endpoint, "StoneBag: fetching capabilities");
                api.stone().capabilities_core().await.ok()
            })
            .await
            .as_ref()
    }

    /// Get the stone name (extracted from capabilities).
    pub async fn stone_name(&self) -> Option<&str> {
        self.capabilities().await.map(|c| c.stone_name.as_str())
    }

    /// Get the stone ID (extracted from capabilities).
    pub async fn stone_id(&self) -> Option<&str> {
        self.capabilities()
            .await
            .and_then(|c| c.stone_id.as_deref())
    }

    /// Whether the stone is reachable.
    ///
    /// If seeded from tending, this returns `true` without network I/O
    /// (optimistic — actual reachability is proven when the command's own
    /// HTTP call succeeds or fails).  If not seeded, triggers the
    /// capabilities fetch as a reachability probe.
    pub async fn is_reachable(&self) -> bool {
        self.capabilities().await.is_some()
    }

    /// Return a clone of the cached capabilities, if available.
    ///
    /// Used to persist capabilities into the tending file after a
    /// successful connection.
    pub async fn capabilities_owned(&self) -> Option<HardwareCapabilities> {
        self.capabilities().await.cloned()
    }
}

// ============================================================================
// Response header extraction
// ============================================================================

/// Stone identity extracted from HTTP response headers.
///
/// Moss ≥ 0.48 injects `X-Stone-Id` / `X-Stone-Name` on every response.
/// This struct captures both, making it easy to seed tending or display
/// the banner without a dedicated `/capabilities` call.
#[derive(Debug, Clone, Default)]
pub struct StoneIdentity {
    pub stone_id: Option<String>,
    pub stone_name: Option<String>,
}

impl StoneIdentity {
    /// Whether at least the stone name was present.
    pub fn has_name(&self) -> bool {
        self.stone_name.is_some()
    }
}

/// Extract stone identity from HTTP response headers.
///
/// Call this on **any** `reqwest::Response` to a Moss endpoint.  If the
/// stone is running a version that injects identity headers, you get the
/// name and ID for free — no separate round-trip required.
///
/// ```ignore
/// let resp = client.get(url).send().await?;
/// let id = extract_stone_identity(resp.headers());
/// if let Some(name) = &id.stone_name {
///     println!("Talking to {name}");
/// }
/// ```
pub fn extract_stone_identity(headers: &reqwest::header::HeaderMap) -> StoneIdentity {
    StoneIdentity {
        stone_id: headers
            .get(HEADER_STONE_ID)
            .and_then(|v| v.to_str().ok())
            .map(String::from),
        stone_name: headers
            .get(HEADER_STONE_NAME)
            .and_then(|v| v.to_str().ok())
            .map(String::from),
    }
}
