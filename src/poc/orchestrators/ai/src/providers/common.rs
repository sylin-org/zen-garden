//! Shared provider helpers: HTTP client, timeouts, error mapping,
//! and the [`InstancePool`] / [`GardenInstanceProvider`] machinery
//! used by the garden discovery task to push live instance lists
//! into each provider.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::RwLock;
use std::time::Duration;

use reqwest::Client;

use crate::domain::provider::ProviderError;

// ── Instance pool ─────────────────────────────────────────────

/// A thread-safe round-robin pool of base URLs. Providers hold one
/// of these and select an instance per request via [`InstancePool::pick`].
/// The garden discovery task updates the pool via [`InstancePool::set`]
/// whenever the topology changes; the provider observes the new list
/// at its next call.
#[derive(Debug, Default)]
pub struct InstancePool {
    urls: RwLock<Vec<String>>,
    cursor: AtomicUsize,
}

impl InstancePool {
    pub fn new() -> Self {
        Self {
            urls: RwLock::new(Vec::new()),
            cursor: AtomicUsize::new(0),
        }
    }

    /// Replace the entire instance list. Returns `true` if the new
    /// list differs structurally from the previous one (so callers
    /// know whether to republish provider state).
    pub fn set(&self, urls: Vec<String>) -> bool {
        let mut current = self.urls.write().expect("instance pool write lock");
        if *current == urls {
            return false;
        }
        *current = urls;
        true
    }

    /// Pick an instance via round-robin. Returns `None` when the pool
    /// is empty.
    pub fn pick(&self) -> Option<String> {
        let urls = self.urls.read().expect("instance pool read lock");
        if urls.is_empty() {
            return None;
        }
        let idx = self.cursor.fetch_add(1, Ordering::Relaxed) % urls.len();
        Some(urls[idx].clone())
    }

    pub fn snapshot(&self) -> Vec<String> {
        self.urls.read().expect("instance pool read lock").clone()
    }

    pub fn len(&self) -> usize {
        self.urls.read().expect("instance pool read lock").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// An instance known to an adapter through garden discovery — the
/// URL the adapter will probe plus the stone identity it belongs
/// to. Adapters that care about stone-level bookkeeping (e.g. the
/// Ollama fit filter in ORCH-0038, which needs to correlate each
/// instance with a `StoneName` in the Resources domain) should
/// store this instead of bare URLs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceBinding {
    pub url: String,
    pub stone_name: String,
}

/// Per-FQN URL bookkeeping used by adapters that subscribe to
/// multiple FQNs.
///
/// The garden discovery service emits `DiscoveryEvent { fqn, instances }`
/// values one FQN at a time. An adapter that claims several FQNs
/// (e.g. Ollama: `"ollama"`, `"ollama_cpu"`, `"ollama::adopted"`)
/// uses this map to remember the latest set per FQN, then calls
/// [`PerFqnInstances::flatten`] to produce the deduplicated merged
/// URL list it pushes into its [`InstancePool`].
///
/// Adapters that need the stone identity for each URL — not just
/// the URL — should use [`PerFqnInstances::set_bindings`] and
/// [`PerFqnInstances::flatten_bindings`] instead. The two shapes
/// coexist so adapters can opt into name-aware bookkeeping without
/// the shared type forcing the rewrite on everyone.
#[derive(Debug, Default)]
pub struct PerFqnInstances {
    /// URL-only per-FQN state. Populated by `set`, read by
    /// `flatten`. Adapters using the name-aware `set_bindings`
    /// store to `bindings` instead, and this map stays empty.
    inner: std::sync::Mutex<std::collections::HashMap<String, Vec<String>>>,
    /// Name-aware per-FQN state. Populated by `set_bindings`,
    /// read by `flatten_bindings`. Kept separate from `inner` so
    /// the two APIs never cross-talk.
    bindings: std::sync::Mutex<std::collections::HashMap<String, Vec<InstanceBinding>>>,
}

impl PerFqnInstances {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the entry for `fqn` with the new URL list. An empty
    /// list removes the entry entirely.
    pub fn set(&self, fqn: &str, urls: Vec<String>) {
        let mut inner = self.inner.lock().expect("PerFqnInstances lock");
        if urls.is_empty() {
            inner.remove(fqn);
        } else {
            inner.insert(fqn.to_string(), urls);
        }
    }

    /// Flatten the per-FQN map into a deduplicated merged URL list.
    pub fn flatten(&self) -> Vec<String> {
        let inner = self.inner.lock().expect("PerFqnInstances lock");
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for urls in inner.values() {
            for url in urls {
                if seen.insert(url.clone()) {
                    out.push(url.clone());
                }
            }
        }
        out
    }

    /// Replace the entry for `fqn` with a name-aware binding list.
    /// Empty list removes the entry entirely. Adapters that need
    /// stone identity per URL call this instead of [`Self::set`].
    pub fn set_bindings(&self, fqn: &str, bindings: Vec<InstanceBinding>) {
        let mut b = self.bindings.lock().expect("PerFqnInstances bindings lock");
        if bindings.is_empty() {
            b.remove(fqn);
        } else {
            b.insert(fqn.to_string(), bindings);
        }
    }

    /// Flatten the per-FQN bindings into a deduplicated merged list.
    /// Dedup is by URL: if two FQNs report the same URL, only the
    /// first binding's stone_name survives.
    pub fn flatten_bindings(&self) -> Vec<InstanceBinding> {
        let b = self.bindings.lock().expect("PerFqnInstances bindings lock");
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for list in b.values() {
            for binding in list {
                if seen.insert(binding.url.clone()) {
                    out.push(binding.clone());
                }
            }
        }
        out
    }
}

pub fn build_http_client() -> Client {
    Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(300))
        .pool_max_idle_per_host(8)
        .build()
        .expect("http client")
}

/// Map a reqwest error to a [`ProviderError`] with a stable code.
pub fn map_reqwest_error(err: reqwest::Error) -> ProviderError {
    if err.is_timeout() {
        ProviderError::Timeout(err.to_string())
    } else if err.is_connect() {
        ProviderError::Unreachable(err.to_string())
    } else if let Some(status) = err.status() {
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            ProviderError::AuthFailed(err.to_string())
        } else if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            ProviderError::RateLimited(err.to_string())
        } else if status.is_server_error() {
            ProviderError::Upstream(err.to_string())
        } else {
            ProviderError::Upstream(err.to_string())
        }
    } else {
        ProviderError::Upstream(err.to_string())
    }
}

/// Check response status and read the body as text on failure.
pub async fn check_status(
    resp: reqwest::Response,
    label: &str,
) -> Result<reqwest::Response, ProviderError> {
    if resp.status().is_success() {
        return Ok(resp);
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        Err(ProviderError::AuthFailed(format!("{label}: {body}")))
    } else if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        Err(ProviderError::RateLimited(format!("{label}: {body}")))
    } else if status.is_server_error() {
        Err(ProviderError::Upstream(format!(
            "{label} upstream {status}: {body}"
        )))
    } else {
        Err(ProviderError::Upstream(format!(
            "{label} {status}: {body}"
        )))
    }
}

/// Truncate a string to `max_chars` with ellipsis. Shared across
/// adapters for summary generation (ORCH-0034).
pub fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{truncated}...")
    }
}
