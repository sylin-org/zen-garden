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

/// Per-FQN URL bookkeeping used by adapters that subscribe to
/// multiple FQNs.
///
/// The garden discovery service emits `DiscoveryEvent { fqn, instances }`
/// values one FQN at a time. An adapter that claims several FQNs
/// (e.g. Ollama: `"ollama"`, `"ollama_cpu"`, `"ollama::adopted"`)
/// uses this map to remember the latest set per FQN, then calls
/// [`PerFqnInstances::flatten`] to produce the deduplicated merged
/// URL list it pushes into its [`InstancePool`].
#[derive(Debug, Default)]
pub struct PerFqnInstances {
    inner: std::sync::Mutex<std::collections::HashMap<String, Vec<String>>>,
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
