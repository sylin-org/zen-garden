//! Garden discovery — a pub/sub event source backed by Moss's
//! `/api/v1/garden/tools/stream` SSE endpoint.
//!
//! Architecture (DDD, eventual consistency):
//!
//! - The discovery service is a long-lived aggregate. Internally it
//!   maintains a registry of `(stone_id, fqid) → DiscoveredInstance`
//!   and a list of subscribers, each with their own set of FQNs of
//!   interest and an `mpsc::Sender` they receive events on.
//! - A single background task consumes the Moss tools SSE stream:
//!   - `tools.snapshot` events replace the entire registry.
//!   - `tool.upsert` events insert or update a single
//!     `(stone_id, fqid)` row.
//!   - `tool.remove` events delete a single row by `tool_key`.
//!   On every change the affected FQN(s) are recomputed and any
//!   subscribers interested in that FQN receive a fresh
//!   [`DiscoveryEvent`].
//! - Adapters subscribe synchronously by calling
//!   [`GardenDiscovery::subscribe`] with the list of FQNs they
//!   manage. The subscriber receives an immediate snapshot event
//!   per FQN (possibly empty if discovery hasn't seen anything yet)
//!   followed by deltas.
//!
//! There is no static offering map and no polling. Adapters declare
//! what they care about; discovery emits events when reality
//! matches.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::Value;
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

// ── Public types ──────────────────────────────────────────────

/// A single offering instance discovered in the garden.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredInstance {
    pub stone_id: String,
    pub stone_name: String,
    /// Fully constructed base URL (e.g. `http://10.0.0.5:11434`).
    /// Built by Moss from the offering manifest's port and the
    /// stone's address.
    pub url: String,
}

/// An event emitted to subscribers whenever the live instance set
/// for one of their declared FQNs changes. The event always carries
/// the full current set for that FQN — adapters do not need to
/// maintain delta state of their own.
#[derive(Debug, Clone)]
pub struct DiscoveryEvent {
    /// The FQN this event pertains to.
    pub fqn: String,
    /// Every currently-known instance for that FQN, across all
    /// stones in the garden.
    pub instances: Vec<DiscoveredInstance>,
}

// ── Internal registry shape ───────────────────────────────────

/// `(stone_id + tool_key) → instance`. The tool_key is Moss's
/// `"{stone_id}:{fqid}:{category}"` and is unique within the
/// garden, so it's our removal key.
type Registry = HashMap<String /* fqn */, HashMap<String /* tool_key */, DiscoveredInstance>>;

/// Subscriptions are stored by **base name**, not by exact FQN.
/// A base name like `"ollama"` matches both the bare `ollama`
/// instance and any `ollama::adopted`, `ollama::dev`, … variants.
/// New variants appear automatically without code changes.
struct Subscription {
    bases: HashSet<String>,
    tx: mpsc::Sender<DiscoveryEvent>,
}

/// Does the given FQN match any of the subscription's base names?
/// A match is either exact equality or `"<base>::<variant>"`.
fn fqn_matches_bases(fqn: &str, bases: &HashSet<String>) -> bool {
    if bases.contains(fqn) {
        return true;
    }
    bases.iter().any(|base| {
        fqn.len() > base.len() + 2
            && fqn.starts_with(base.as_str())
            && &fqn[base.len()..base.len() + 2] == "::"
    })
}

// ── Aggregate ─────────────────────────────────────────────────

pub struct GardenDiscovery {
    state: Mutex<DiscoveryState>,
    tended_stone: String,
    http: Client,
}

struct DiscoveryState {
    registry: Registry,
    subscribers: Vec<Subscription>,
}

impl GardenDiscovery {
    /// Construct the service and spawn its background SSE consumer.
    /// Adapters can call [`subscribe`](Self::subscribe) immediately;
    /// they receive empty snapshots until the SSE stream delivers
    /// its first `tools.snapshot` event.
    pub fn spawn(tended_stone: String, shutdown: CancellationToken) -> Arc<Self> {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            // SSE streams are long-lived — no overall timeout.
            .build()
            .expect("garden discovery http client");
        let this = Arc::new(Self {
            state: Mutex::new(DiscoveryState {
                registry: HashMap::new(),
                subscribers: Vec::new(),
            }),
            tended_stone,
            http,
        });
        let task = this.clone();
        tokio::spawn(async move { task.run(shutdown).await });
        this
    }

    /// Subscribe to events for a set of base offering names.
    ///
    /// Base names match both exact FQNs and any `base::variant`
    /// FQNs. For example, subscribing to `&["ollama"]` receives
    /// events for `ollama`, `ollama::adopted`, `ollama::dev`, and
    /// any other future variant — no code changes when the garden
    /// adds a new instance qualifier.
    ///
    /// The returned `mpsc` receiver immediately yields one snapshot
    /// event per matched FQN currently in the registry, then every
    /// subsequent change for the lifetime of the receiver.
    pub async fn subscribe(&self, bases: &[&'static str]) -> mpsc::Receiver<DiscoveryEvent> {
        let (tx, rx) = mpsc::channel::<DiscoveryEvent>(64);
        let base_set: HashSet<String> = bases.iter().map(|s| s.to_string()).collect();

        let mut state = self.state.lock().await;

        // Walk every known FQN; for each one that matches the new
        // subscriber's bases, send a snapshot event. If discovery
        // hasn't seen anything yet, the loop sends nothing — the
        // adapter starts with an empty pool, which is correct.
        for (fqn, instances_map) in state.registry.iter() {
            if !fqn_matches_bases(fqn, &base_set) {
                continue;
            }
            let instances: Vec<DiscoveredInstance> = instances_map.values().cloned().collect();
            let _ = tx.try_send(DiscoveryEvent {
                fqn: fqn.clone(),
                instances,
            });
        }

        state.subscribers.push(Subscription {
            bases: base_set,
            tx,
        });
        rx
    }

    /// Background task: consume the Moss garden tools SSE stream
    /// forever, reconnecting on failure. Loops until `shutdown`.
    async fn run(self: Arc<Self>, shutdown: CancellationToken) {
        let url = format!(
            "{}/api/v1/garden/tools/stream",
            self.tended_stone.trim_end_matches('/')
        );
        tracing::info!(url = %url, "garden discovery starting SSE consumer");

        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    tracing::info!("garden discovery shutdown");
                    return;
                }
                result = self.consume_stream(&url) => {
                    if let Err(e) = result {
                        tracing::warn!(error = %e, "garden discovery stream ended; reconnecting in 5s");
                    }
                    // Reconnect cooldown.
                    tokio::select! {
                        _ = shutdown.cancelled() => return,
                        _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                    }
                }
            }
        }
    }

    async fn consume_stream(&self, url: &str) -> Result<()> {
        let response = self
            .http
            .get(url)
            .header("Accept", "text/event-stream")
            .send()
            .await
            .with_context(|| format!("connect to garden tools stream at {url}"))?;
        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "garden tools stream returned status {}",
                response.status()
            ));
        }

        let mut bytes = response.bytes_stream();
        let mut buffer = String::new();
        let mut event_type = String::new();
        let mut data_lines: Vec<String> = Vec::new();

        while let Some(chunk) = bytes.next().await {
            let chunk = chunk.context("read SSE chunk")?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(nl) = buffer.find('\n') {
                let line = buffer[..nl].trim_end_matches('\r').to_string();
                buffer.drain(..=nl);

                if line.is_empty() {
                    if !data_lines.is_empty() {
                        let data = data_lines.join("\n");
                        self.handle_event(&event_type, &data).await;
                        data_lines.clear();
                        event_type.clear();
                    }
                } else if let Some(rest) = line.strip_prefix("event:") {
                    event_type = rest.trim().to_string();
                } else if let Some(rest) = line.strip_prefix("data:") {
                    data_lines.push(rest.trim().to_string());
                }
            }
        }
        Ok(())
    }

    async fn handle_event(&self, event_type: &str, data: &str) {
        match event_type {
            "tools.snapshot" => self.handle_snapshot(data).await,
            "tool.upsert" => self.handle_upsert(data).await,
            "tool.remove" => self.handle_remove(data).await,
            "tools.heartbeat" => {}
            other => tracing::trace!(event = other, "ignoring tools event"),
        }
    }

    async fn handle_snapshot(&self, data: &str) {
        let Ok(json) = serde_json::from_str::<Value>(data) else {
            tracing::warn!("malformed tools.snapshot payload");
            return;
        };
        let Some(tools) = json.get("tools").and_then(|v| v.as_array()) else {
            return;
        };

        // Build a fresh registry from the snapshot.
        let mut new_registry: Registry = HashMap::new();
        for tool in tools {
            let Some((tool_key, fqn, instance)) = parse_tool(tool) else {
                continue;
            };
            new_registry
                .entry(fqn)
                .or_default()
                .insert(tool_key, instance);
        }

        // Determine which FQNs changed (any FQN whose set differs
        // between old and new).
        let mut state = self.state.lock().await;
        let mut affected: HashSet<String> = HashSet::new();
        for fqn in new_registry.keys() {
            affected.insert(fqn.clone());
        }
        for fqn in state.registry.keys() {
            affected.insert(fqn.clone());
        }
        state.registry = new_registry;
        let registry_snapshot = state.registry.clone();
        let subscribers = state.subscribers.clone_subscribers();
        drop(state);

        for fqn in affected {
            self.fanout(&fqn, &registry_snapshot, &subscribers).await;
        }
    }

    async fn handle_upsert(&self, data: &str) {
        let Ok(json) = serde_json::from_str::<Value>(data) else {
            return;
        };
        // The upsert event embeds the projection at `.projection`
        // (per parse_upsert in tools_stream.rs). Use it.
        let projection = match json.get("projection") {
            Some(p) => p,
            None => &json,
        };
        let Some((tool_key, fqn, instance)) = parse_tool(projection) else {
            return;
        };
        let mut state = self.state.lock().await;
        state
            .registry
            .entry(fqn.clone())
            .or_default()
            .insert(tool_key, instance);
        let registry_snapshot = state.registry.clone();
        let subscribers = state.subscribers.clone_subscribers();
        drop(state);
        self.fanout(&fqn, &registry_snapshot, &subscribers).await;
    }

    async fn handle_remove(&self, data: &str) {
        let Ok(json) = serde_json::from_str::<Value>(data) else {
            return;
        };
        let tool_key = match json.get("tool_key").and_then(|v| v.as_str()) {
            Some(k) => k.to_string(),
            None => return,
        };
        let fqn = match json.get("fqid").and_then(|v| v.as_str()) {
            Some(f) => f.to_string(),
            None => {
                // Fall back to scanning the registry — slower but
                // never incorrect.
                let mut state = self.state.lock().await;
                let mut found_fqn: Option<String> = None;
                for (fqn, set) in state.registry.iter_mut() {
                    if set.remove(&tool_key).is_some() {
                        found_fqn = Some(fqn.clone());
                        break;
                    }
                }
                if let Some(fqn) = found_fqn {
                    let registry_snapshot = state.registry.clone();
                    let subscribers = state.subscribers.clone_subscribers();
                    drop(state);
                    self.fanout(&fqn, &registry_snapshot, &subscribers).await;
                }
                return;
            }
        };
        let mut state = self.state.lock().await;
        if let Some(set) = state.registry.get_mut(&fqn) {
            set.remove(&tool_key);
            if set.is_empty() {
                state.registry.remove(&fqn);
            }
        }
        let registry_snapshot = state.registry.clone();
        let subscribers = state.subscribers.clone_subscribers();
        drop(state);
        self.fanout(&fqn, &registry_snapshot, &subscribers).await;
    }

    /// Push a `DiscoveryEvent` for `fqn` to every subscriber whose
    /// base names match. Dead subscribers (closed channels) are
    /// ignored — send errors are not fatal here.
    async fn fanout(
        &self,
        fqn: &str,
        registry: &Registry,
        subscribers: &[Subscription],
    ) {
        let instances = registry
            .get(fqn)
            .map(|m| m.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        for sub in subscribers {
            if !fqn_matches_bases(fqn, &sub.bases) {
                continue;
            }
            let _ = sub
                .tx
                .send(DiscoveryEvent {
                    fqn: fqn.to_string(),
                    instances: instances.clone(),
                })
                .await;
        }
    }
}

// ── Subscription cloning helper ───────────────────────────────

trait CloneSubscribers {
    fn clone_subscribers(&self) -> Vec<Subscription>;
}

impl CloneSubscribers for Vec<Subscription> {
    fn clone_subscribers(&self) -> Vec<Subscription> {
        self.iter()
            .map(|s| Subscription {
                bases: s.bases.clone(),
                tx: s.tx.clone(),
            })
            .collect()
    }
}

// ── Tool projection parsing ───────────────────────────────────

/// Extract `(tool_key, fqn, DiscoveredInstance)` from a `GardenTool`
/// JSON projection. Returns `None` for control-plane tools (the
/// `orchestrator` category, which represents proxy containers, not
/// the backing services), or for tools missing identity / URI
/// fields. All other categories are accepted — adapter FQN claims
/// are the only filter that matters.
fn parse_tool(tool: &Value) -> Option<(String, String, DiscoveredInstance)> {
    let category = tool
        .get("tool")
        .and_then(|t| t.get("category"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    // Skip orchestrator-category tools — those are the AI orchestrator
    // proxy containers themselves (e.g. `ollama::orchestrator` at the
    // Moss proxy port), not the backing service instances.
    if category == "orchestrator" {
        return None;
    }

    let fqid = tool.get("fqid").and_then(|v| v.as_str())?.to_string();

    let stone_obj = tool.get("stone")?;
    let stone_id = stone_obj.get("id").and_then(|v| v.as_str())?.to_string();
    let stone_name = stone_obj
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Prefer the first URI from `service.uris`, which Moss orders
    // by preference. Falls back to nothing — instances without a
    // reachable URI are skipped.
    let url = tool
        .get("service")
        .and_then(|s| s.get("uris"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.iter().find_map(|v| v.as_str()))
        .map(|s| s.to_string())?;

    let tool_key_category = tool
        .get("tool")
        .and_then(|t| t.get("category"))
        .and_then(|v| v.as_str())
        .unwrap_or("offering");
    let tool_key = format!("{stone_id}:{fqid}:{tool_key_category}");

    Some((
        tool_key,
        fqid,
        DiscoveredInstance {
            stone_id,
            stone_name,
            url,
        },
    ))
}
