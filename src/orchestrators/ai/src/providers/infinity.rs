//! Infinity provider — `text.embed` and `text.rerank`.
//!
//! Wire API (OpenAI-compat for embeddings, simple /rerank for rerank):
//!
//! ```text
//! POST /embeddings
//! { "model": "BAAI/bge-small-en-v1.5", "input": ["..."] }
//! -> { "data": [{"embedding": [...]}, ...], "usage": {"prompt_tokens": N} }
//!
//! POST /rerank
//! { "model": "...", "query": "...", "documents": ["..."] }
//! -> { "results": [{"index": 0, "relevance_score": 0.9}, ...] }
//! ```

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::watch;

use crate::domain::ids::{ProviderName, RegistrationId};
use crate::domain::keys;
use crate::domain::output::Output;
use crate::domain::primitive::Primitive;
use crate::domain::provider::{
    HonoredField, Provider, ProviderError, ProviderHealth, ProviderOutcome, ProviderState,
    ProviderStatePublisher, Registration, RegistrationStrategy,
};
use crate::domain::request::OrchestratorRequest;

use crate::services::garden_discovery::GardenDiscovery;
use tokio_util::sync::CancellationToken;

use super::common::{
    build_http_client, check_status, map_reqwest_error, InstancePool, PerFqnInstances,
};

const FQNS: &[&'static str] = &["infinity"];

#[derive(Debug, Clone, Default)]
pub struct InfinityConfig {
    pub default_embed_model: String,
    pub default_rerank_model: String,
    pub api_key: Option<String>,
}

pub struct InfinityProvider {
    name: ProviderName,
    config: InfinityConfig,
    instances: Arc<InstancePool>,
    http: Client,
    publisher: ProviderStatePublisher,
}

fn build_registrations(name: &ProviderName) -> Vec<Registration> {
    let embed = Registration {
        id: RegistrationId::generate(),
        provider: name.clone(),
        primitive: Primitive::TextEmbed,
        strategy: RegistrationStrategy::Bare,
        honored_fields: vec![HonoredField::new(keys::text::INPUT).required()],
        media_inputs: Vec::new(),
        media_outputs: Vec::new(),
    };
    let rerank = Registration {
        id: RegistrationId::generate(),
        provider: name.clone(),
        primitive: Primitive::TextRerank,
        strategy: RegistrationStrategy::Bare,
        honored_fields: vec![
            HonoredField::new(keys::text::QUERY).required(),
            HonoredField::new(keys::text::DOCUMENTS).required(),
            HonoredField::new(keys::text::RESULTS_TOP_K),
        ],
        media_inputs: Vec::new(),
        media_outputs: Vec::new(),
    };
    vec![embed, rerank]
}

impl InfinityProvider {
    pub fn new(
        config: InfinityConfig,
        discovery: Arc<GardenDiscovery>,
        shutdown: CancellationToken,
    ) -> Arc<Self> {
        let name = ProviderName::new(keys::providers::INFINITY);
        let initial = ProviderState {
            health: ProviderHealth::Offline {
                reason: "no garden instances discovered yet".to_string(),
            },
            registrations: build_registrations(&name),
            models: Vec::new(),
            performance_hints: Vec::new(),
        };
        let provider = Arc::new(Self {
            name,
            config,
            instances: Arc::new(InstancePool::new()),
            http: build_http_client(),
            publisher: ProviderStatePublisher::new(initial),
        });
        spawn_subscriber(provider.clone(), discovery, shutdown);
        provider
    }

    fn auth(&self, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.config.api_key {
            Some(k) => rb.bearer_auth(k),
            None => rb,
        }
    }

    fn pick(&self) -> Result<String, ProviderError> {
        self.instances.pick().ok_or_else(|| {
            ProviderError::Unreachable("no infinity instances in the garden".to_string())
        })
    }

    async fn call_embed(&self, payload: &EmbedRequest<'_>) -> Result<EmbedResponse, ProviderError> {
        let base = self.pick()?;
        let endpoint = format!("{}/embeddings", base.trim_end_matches('/'));
        let resp = self
            .auth(self.http.post(&endpoint).json(payload))
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let resp = check_status(resp, "infinity embeddings").await?;
        resp.json::<EmbedResponse>()
            .await
            .map_err(|e| ProviderError::Upstream(e.to_string()))
    }

    async fn call_rerank(&self, payload: &RerankRequest<'_>) -> Result<RerankResponse, ProviderError> {
        let base = self.pick()?;
        let endpoint = format!("{}/rerank", base.trim_end_matches('/'));
        let resp = self
            .auth(self.http.post(&endpoint).json(payload))
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let resp = check_status(resp, "infinity rerank").await?;
        resp.json::<RerankResponse>()
            .await
            .map_err(|e| ProviderError::Upstream(e.to_string()))
    }
}

impl InfinityProvider {
    fn apply_merged(&self, urls: Vec<String>) {
        if !self.instances.set(urls) {
            return;
        }
        let count = self.instances.len();
        let name = self.name.clone();
        self.publisher.modify(move |mut state| {
            state.health = if count == 0 {
                ProviderHealth::Offline {
                    reason: "no garden instances discovered".to_string(),
                }
            } else {
                ProviderHealth::Healthy
            };
            state.registrations = build_registrations(&name);
            state
        });
    }
}

fn spawn_subscriber(
    provider: Arc<InfinityProvider>,
    discovery: Arc<GardenDiscovery>,
    shutdown: CancellationToken,
) {
    tokio::spawn(async move {
        let pool = PerFqnInstances::new();
        let mut rx = discovery.subscribe(FQNS).await;
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                event = rx.recv() => {
                    let Some(event) = event else { break };
                    let urls: Vec<String> = event.instances.into_iter().map(|i| i.url).collect();
                    pool.set(&event.fqn, urls);
                    provider.apply_merged(pool.flatten());
                }
            }
        }
    });
}

#[async_trait]
impl Provider for InfinityProvider {
    fn name(&self) -> ProviderName {
        self.name.clone()
    }

    fn state(&self) -> Arc<ProviderState> {
        self.publisher.snapshot()
    }

    fn subscribe(&self) -> watch::Receiver<Arc<ProviderState>> {
        self.publisher.subscribe()
    }

    async fn onboard(
        &self,
        request: OrchestratorRequest,
    ) -> Result<ProviderOutcome, ProviderError> {
        match request.action.primitive {
            Primitive::TextEmbed => self.onboard_embed(request).await,
            Primitive::TextRerank => self.onboard_rerank(request).await,
            p => Err(ProviderError::Unsupported(format!(
                "infinity does not serve {}",
                p.dotted()
            ))),
        }
    }
}

impl InfinityProvider {
    async fn onboard_embed(
        &self,
        request: OrchestratorRequest,
    ) -> Result<ProviderOutcome, ProviderError> {
        let input = request
            .payload
            .pointer("/text/input")
            .cloned()
            .ok_or_else(|| ProviderError::Unsupported("missing text.input".to_string()))?;
        let inputs: Vec<String> = match input {
            Value::String(s) => vec![s],
            Value::Array(a) => a
                .into_iter()
                .map(|v| match v {
                    Value::String(s) => Ok(s),
                    other => Err(ProviderError::Unsupported(format!(
                        "text.input array must contain strings (got {other:?})"
                    ))),
                })
                .collect::<Result<Vec<_>, _>>()?,
            other => {
                return Err(ProviderError::Unsupported(format!(
                    "text.input must be string or array (got {other:?})"
                )));
            }
        };
        let model = request
            .resolved_model
            .as_ref()
            .map(|m| m.short_name.clone())
            .unwrap_or_else(|| self.config.default_embed_model.clone());
        let req = EmbedRequest {
            model: &model,
            input: &inputs,
        };
        let response = self.call_embed(&req).await?;

        let embeddings_json: Value = serde_json::to_value(
            response.data.iter().map(|d| &d.embedding).collect::<Vec<_>>(),
        )
        .map_err(|e| ProviderError::Internal(e.to_string()))?;

        let mut out = Output::new();
        out.set(&keys::text::EMBEDDINGS, embeddings_json);
        if let Some(usage) = response.usage {
            out.set(&keys::usage::TOKENS_INPUT, usage.prompt_tokens);
            out.set(&keys::usage::TOKENS_TOTAL, usage.total_tokens);
        }
        Ok(ProviderOutcome::Sync(out))
    }

    async fn onboard_rerank(
        &self,
        request: OrchestratorRequest,
    ) -> Result<ProviderOutcome, ProviderError> {
        let query = request
            .payload
            .pointer("/text/query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ProviderError::Unsupported("missing text.query".to_string()))?;
        let documents: Vec<String> = request
            .payload
            .pointer("/text/documents")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ProviderError::Unsupported("missing text.documents".to_string()))?
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        let top_k = request
            .payload
            .pointer("/text/results/top_k")
            .and_then(|v| v.as_i64());
        let model = request
            .resolved_model
            .as_ref()
            .map(|m| m.short_name.clone())
            .unwrap_or_else(|| self.config.default_rerank_model.clone());
        let req = RerankRequest {
            model: &model,
            query,
            documents: &documents,
            top_n: top_k,
        };
        let response = self.call_rerank(&req).await?;

        let segments: Vec<Value> = response
            .results
            .into_iter()
            .map(|r| {
                json!({
                    "index": r.index,
                    "score": r.relevance_score,
                    "document": documents.get(r.index as usize).cloned().unwrap_or_default(),
                })
            })
            .collect();
        let mut out = Output::new();
        out.set(&keys::text::SEGMENTS, Value::Array(segments));
        Ok(ProviderOutcome::Sync(out))
    }
}

// ── Wire types ────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct EmbedRequest<'a> {
    model: &'a str,
    input: &'a [String],
}

#[derive(Debug, Deserialize)]
struct EmbedResponse {
    data: Vec<EmbeddingEntry>,
    #[serde(default)]
    usage: Option<EmbeddingUsage>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingEntry {
    embedding: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingUsage {
    prompt_tokens: u64,
    total_tokens: u64,
}

#[derive(Debug, Serialize)]
struct RerankRequest<'a> {
    model: &'a str,
    query: &'a str,
    documents: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    top_n: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct RerankResponse {
    results: Vec<RerankResult>,
}

#[derive(Debug, Deserialize)]
struct RerankResult {
    index: i64,
    relevance_score: f64,
}
