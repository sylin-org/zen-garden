//! Ollama provider — `text.chat`, `text.embed`, `image.analyze`.
//!
//! Wire API:
//!
//! ```text
//! POST /api/chat      { "model": "...", "messages": [...], "options": {...} }
//! POST /api/embed     { "model": "...", "input": [...] }
//! GET  /api/tags      -> {"models": [...]} used for model discovery
//! ```
//!
//! Vision support: images are sent as `messages[].images: ["<base64>"]`.
//! The orchestrator's media resolver inlines base64 blobs before dispatch
//! (see [`crate::services::media_resolver`]). Provider narrowings declare
//! `image.source` as `Base64` delivery.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::watch;

use crate::domain::ids::{ModelFqn, ProviderName, RegistrationId};
use crate::domain::keys;
use crate::domain::media::MediaDelivery;
use crate::domain::output::Output;
use crate::domain::primitive::Primitive;
use crate::domain::provider::{
    HonoredField, MediaInputSpec, Model, Provider, ProviderError, ProviderHealth,
    ProviderOutcome, ProviderState, ProviderStatePublisher, Registration, RegistrationStrategy,
};
use crate::domain::request::OrchestratorRequest;

use crate::services::garden_discovery::GardenDiscovery;
use tokio_util::sync::CancellationToken;

use super::common::{
    build_http_client, check_status, map_reqwest_error, InstancePool, PerFqnInstances,
};

/// Ollama base name. Discovery's base-name match automatically
/// picks up `ollama` as well as any `ollama::adopted`,
/// `ollama::dev`, `ollama::cpu`, etc. variants the garden recognizes
/// — no code changes needed for new qualifiers.
const FQNS: &[&'static str] = &["ollama"];

#[derive(Debug, Clone, Default)]
pub struct OllamaConfig;

pub struct OllamaProvider {
    name: ProviderName,
    instances: Arc<InstancePool>,
    http: Client,
    publisher: ProviderStatePublisher,
}

fn build_registrations(name: &ProviderName) -> Vec<Registration> {
    vec![
        build_chat_registration(name),
        build_embed_registration(name),
        build_analyze_registration(name),
    ]
}

impl OllamaProvider {
    pub fn new(
        _config: OllamaConfig,
        discovery: Arc<GardenDiscovery>,
        shutdown: CancellationToken,
    ) -> Arc<Self> {
        let name = ProviderName::new(keys::providers::OLLAMA);
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
            instances: Arc::new(InstancePool::new()),
            http: build_http_client(),
            publisher: ProviderStatePublisher::new(initial),
        });
        spawn_subscriber(provider.clone(), discovery, shutdown);
        provider
    }

    fn pick(&self) -> Result<String, ProviderError> {
        self.instances.pick().ok_or_else(|| {
            ProviderError::Unreachable("no ollama instances in the garden".to_string())
        })
    }

    /// Refresh the model catalog by enumerating every reachable
    /// Ollama instance, then enriching each unique model with
    /// `/api/show` metadata so the recommendation engine can rank
    /// it under the right capability profile.
    ///
    /// Pipeline:
    /// 1. `/api/tags` per instance → union of `(name, size)` rows.
    /// 2. For each unique model, fan out a single `/api/show` call
    ///    against the first reachable instance that hosts it.
    ///    `/api/show` returns Ollama-native `capabilities`
    ///    (`completion`, `embedding`, `vision`, `tools`, `thinking`),
    ///    `parameter_count`, and the architecture-specific
    ///    `<arch>.context_length` field. These are the inputs the
    ///    capability-aware recommender needs.
    /// 3. Translate Ollama capability tags into the orchestrator's
    ///    primitive set: `completion → TextChat`,
    ///    `embedding → TextEmbed`, `vision → ImageAnalyze`. Models
    ///    without any recognised capability are skipped — they
    ///    aren't usable through this orchestrator.
    /// 4. Publish the enriched model list. Discovery's per-provider
    ///    forwarder picks it up; the directory rebuilds; the
    ///    recommendation engine reranks under every capability
    ///    profile.
    async fn refresh_models_from_pool(&self) {
        let urls = self.instances.snapshot();
        let mut tag_rows: Vec<(String, u64, String)> = Vec::new();
        let mut any_reachable = false;

        // Step 1: enumerate models from /api/tags on every instance.
        for base in &urls {
            let endpoint = format!("{}/api/tags", base.trim_end_matches('/'));
            let resp = self
                .http
                .get(&endpoint)
                .timeout(Duration::from_secs(5))
                .send()
                .await;
            let Ok(resp) = resp else { continue };
            let Ok(resp) = resp.error_for_status() else { continue };
            let Ok(tags) = resp.json::<TagsResponse>().await else { continue };
            any_reachable = true;
            for m in tags.models {
                if tag_rows.iter().any(|(n, _, _)| n == &m.name) {
                    continue;
                }
                tag_rows.push((m.name, m.size, base.trim_end_matches('/').to_string()));
            }
        }

        // Step 2 + 3: enrich every unique model in parallel via
        // /api/show, translate capabilities, build the canonical
        // Model records.
        let enrich_futs = tag_rows.into_iter().map(|(name, size, endpoint)| {
            let http = self.http.clone();
            let provider_name = self.name.clone();
            async move {
                match show_model(&http, &endpoint, &name).await {
                    Some(detail) => Some(model_from_show(&provider_name, name, size, detail)),
                    None => None,
                }
            }
        });
        let mut models: Vec<Model> = futures_util::future::join_all(enrich_futs)
            .await
            .into_iter()
            .flatten()
            .flatten()
            .collect();
        models.sort_by(|a, b| a.short_name.cmp(&b.short_name));

        let name = self.name.clone();
        let count = urls.len();
        self.publisher.modify(move |mut state| {
            state.models = models;
            state.registrations = build_registrations(&name);
            state.health = if count == 0 {
                ProviderHealth::Offline {
                    reason: "no garden instances discovered".to_string(),
                }
            } else if !any_reachable {
                ProviderHealth::Degraded {
                    reason: "instances discovered but none reachable".to_string(),
                }
            } else {
                ProviderHealth::Healthy
            };
            state
        });
    }
}

/// Call `/api/show` for one model on one instance and return the
/// raw JSON response. Returns `None` on any failure — the model
/// will simply not be enriched and the recommender will skip it.
async fn show_model(http: &Client, endpoint: &str, model: &str) -> Option<ShowResponse> {
    let url = format!("{}/api/show", endpoint);
    let resp = http
        .post(&url)
        .json(&serde_json::json!({ "model": model }))
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .ok()?;
    let resp = resp.error_for_status().ok()?;
    resp.json::<ShowResponse>().await.ok()
}

/// Translate the Ollama `/api/show` response into the orchestrator's
/// canonical [`Model`] shape. Returns `Option<Model>` so callers can
/// drop models that declare no recognised capability — they aren't
/// reachable through any orchestrator primitive.
fn model_from_show(
    provider: &ProviderName,
    name: String,
    size_disk: u64,
    detail: ShowResponse,
) -> Option<Model> {
    let capabilities = detail.capabilities.unwrap_or_default();

    // Map Ollama tags onto orchestrator primitives.
    let mut primitives = Vec::new();
    if capabilities.iter().any(|c| c == "completion") {
        primitives.push(Primitive::TextChat);
        // Image analyze is also "completion" plus "vision".
        if capabilities.iter().any(|c| c == "vision") {
            primitives.push(Primitive::ImageAnalyze);
        }
    }
    if capabilities.iter().any(|c| c == "embedding") {
        primitives.push(Primitive::TextEmbed);
    }
    if primitives.is_empty() {
        // Unknown capability set — skip rather than guess.
        tracing::debug!(
            model = %name,
            tags = ?capabilities,
            "ollama: model has no recognised capability, skipping"
        );
        return None;
    }

    // Pull parameter_count from model_info or fall back to parsing
    // the parameter_size string ("7B" → 7_000_000_000).
    let parameter_count = detail
        .model_info
        .as_ref()
        .and_then(|m| m.get("general.parameter_count"))
        .and_then(|v| v.as_u64())
        .or_else(|| {
            detail
                .details
                .as_ref()
                .and_then(|d| d.parameter_size.as_deref())
                .and_then(parse_parameter_size)
        });

    // Pull context_length from the architecture-specific
    // `<arch>.context_length` key in model_info. The architecture
    // name is in `details.family`.
    let context_length = detail
        .model_info
        .as_ref()
        .and_then(|m| {
            // Look for any *.context_length key.
            m.iter()
                .find(|(k, _)| k.ends_with(".context_length"))
                .and_then(|(_, v)| v.as_u64())
        });

    Some(Model {
        fqn: ModelFqn::new(provider, &name),
        short_name: name,
        primitives,
        capability_tags: capabilities,
        size_bytes: Some(size_disk),
        context_length,
        parameter_count,
    })
}

/// Parse Ollama's human-readable parameter-size string into a raw
/// count (e.g. `"7B"` → `7_000_000_000`, `"1.5B"` → `1_500_000_000`,
/// `"137M"` → `137_000_000`).
fn parse_parameter_size(s: &str) -> Option<u64> {
    let s = s.trim();
    let (num_str, mul): (&str, u64) = if let Some(rest) = s.strip_suffix('B') {
        (rest, 1_000_000_000)
    } else if let Some(rest) = s.strip_suffix('M') {
        (rest, 1_000_000)
    } else if let Some(rest) = s.strip_suffix('K') {
        (rest, 1_000)
    } else {
        return s.parse::<u64>().ok();
    };
    let n: f64 = num_str.trim().parse().ok()?;
    Some((n * mul as f64) as u64)
}

impl OllamaProvider {
    /// Apply a fresh merged URL list from the garden discovery
    /// subscriber and refresh the model catalog from `/api/tags`.
    async fn apply_merged(&self, urls: Vec<String>) {
        if !self.instances.set(urls) {
            return;
        }
        self.refresh_models_from_pool().await;
    }
}

fn spawn_subscriber(
    provider: Arc<OllamaProvider>,
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
                    provider.apply_merged(pool.flatten()).await;
                }
            }
        }
    });
}

#[async_trait]
impl Provider for OllamaProvider {
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
            Primitive::TextChat => self.do_chat(request).await,
            Primitive::TextEmbed => self.do_embed(request).await,
            Primitive::ImageAnalyze => self.do_analyze(request).await,
            other => Err(ProviderError::Unsupported(format!(
                "ollama does not serve {}",
                other.dotted()
            ))),
        }
    }
}

impl OllamaProvider {
    async fn do_chat(
        &self,
        request: OrchestratorRequest,
    ) -> Result<ProviderOutcome, ProviderError> {
        let model = request
            .resolved_model
            .as_ref()
            .ok_or_else(|| ProviderError::Unsupported("model selector required".to_string()))?
            .short_name
            .clone();

        let stream_requested = request
            .payload
            .pointer("/text/stream")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let messages = build_chat_messages(&request.payload, false)?;
        let options = build_options(&request.payload);
        let body = json!({
            "model": model,
            "messages": messages,
            "options": options,
            "stream": stream_requested,
        });
        let base = self.pick()?;
        let endpoint = format!("{}/api/chat", base.trim_end_matches('/'));

        if stream_requested {
            return self.do_chat_streaming(&endpoint, body).await;
        }

        let resp = self
            .http
            .post(&endpoint)
            .json(&body)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let resp = check_status(resp, "ollama chat").await?;
        let wire: ChatResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::Upstream(e.to_string()))?;

        let mut out = Output::new();
        out.set(&keys::text::RESPONSE, wire.message.content);
        out.set(
            &keys::text::FINISH_REASON,
            wire.done_reason
                .unwrap_or_else(|| keys::text::values::FINISH_REASON_STOP.to_string()),
        );
        if let Some(eval) = wire.prompt_eval_count {
            out.set(&keys::usage::TOKENS_INPUT, eval);
        }
        if let Some(eval) = wire.eval_count {
            out.set(&keys::usage::TOKENS_OUTPUT, eval);
        }
        if let Some(total) = wire.total_duration {
            out.set(&keys::timing::TOTAL_MS, total / 1_000_000);
        }
        Ok(ProviderOutcome::Sync(out))
    }

    /// Ollama's streaming chat emits newline-delimited JSON: each
    /// line is a ChatResponse with `done: false` until the terminal
    /// line with `done: true` and aggregate counters. This method
    /// converts that byte stream into a
    /// `BoxStream<Result<Output, ProviderError>>` of canonical
    /// deltas.
    async fn do_chat_streaming(
        &self,
        endpoint: &str,
        body: serde_json::Value,
    ) -> Result<ProviderOutcome, ProviderError> {
        use futures_util::StreamExt;

        let resp = self
            .http
            .post(endpoint)
            .json(&body)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let resp = check_status(resp, "ollama chat stream").await?;

        let byte_stream = resp.bytes_stream();

        // Parse NDJSON into canonical Output deltas on the fly.
        let deltas = async_stream::stream! {
            let mut buffer: Vec<u8> = Vec::new();
            let mut byte_stream = byte_stream;
            while let Some(chunk) = byte_stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        buffer.extend_from_slice(&bytes);
                        loop {
                            let Some(nl) = buffer.iter().position(|b| *b == b'\n') else {
                                break;
                            };
                            let line: Vec<u8> = buffer.drain(..=nl).collect();
                            let trimmed = &line[..line.len() - 1];
                            if trimmed.is_empty() {
                                continue;
                            }
                            match serde_json::from_slice::<ChatStreamChunk>(trimmed) {
                                Ok(chunk) => {
                                    let mut out = Output::new();
                                    if let Some(msg) = chunk.message {
                                        if !msg.content.is_empty() {
                                            out.set(&keys::text::RESPONSE, msg.content);
                                        }
                                    }
                                    if chunk.done {
                                        if let Some(reason) = chunk.done_reason {
                                            out.set(&keys::text::FINISH_REASON, reason);
                                        } else {
                                            out.set(
                                                &keys::text::FINISH_REASON,
                                                keys::text::values::FINISH_REASON_STOP,
                                            );
                                        }
                                        if let Some(c) = chunk.prompt_eval_count {
                                            out.set(&keys::usage::TOKENS_INPUT, c);
                                        }
                                        if let Some(c) = chunk.eval_count {
                                            out.set(&keys::usage::TOKENS_OUTPUT, c);
                                        }
                                        if let Some(t) = chunk.total_duration {
                                            out.set(&keys::timing::TOTAL_MS, t / 1_000_000);
                                        }
                                    }
                                    if !out.is_empty() {
                                        yield Ok(out);
                                    }
                                }
                                Err(e) => {
                                    yield Err(ProviderError::Upstream(format!(
                                        "ollama stream parse: {e}"
                                    )));
                                    return;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        yield Err(ProviderError::Upstream(format!(
                            "ollama stream read: {e}"
                        )));
                        return;
                    }
                }
            }
        };

        let initial = Output::new();
        Ok(ProviderOutcome::Streaming {
            initial,
            stream: Box::pin(deltas),
        })
    }

    async fn do_embed(
        &self,
        request: OrchestratorRequest,
    ) -> Result<ProviderOutcome, ProviderError> {
        let model = request
            .resolved_model
            .as_ref()
            .ok_or_else(|| ProviderError::Unsupported("model selector required".to_string()))?
            .short_name
            .clone();
        let input = request
            .payload
            .pointer("/text/input")
            .cloned()
            .ok_or_else(|| ProviderError::Unsupported("missing text.input".to_string()))?;
        let inputs: Vec<String> = match input {
            Value::String(s) => vec![s],
            Value::Array(a) => a
                .into_iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect(),
            _ => {
                return Err(ProviderError::Unsupported(
                    "text.input must be a string or array".to_string(),
                ));
            }
        };

        let body = json!({
            "model": model,
            "input": inputs,
        });
        let base = self.pick()?;
        let endpoint = format!("{}/api/embed", base.trim_end_matches('/'));
        let resp = self
            .http
            .post(&endpoint)
            .json(&body)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let resp = check_status(resp, "ollama embed").await?;
        let wire: EmbedResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::Upstream(e.to_string()))?;

        let mut out = Output::new();
        out.set(
            &keys::text::EMBEDDINGS,
            serde_json::to_value(&wire.embeddings).unwrap_or(Value::Null),
        );
        if let Some(count) = wire.prompt_eval_count {
            out.set(&keys::usage::TOKENS_INPUT, count);
        }
        Ok(ProviderOutcome::Sync(out))
    }

    async fn do_analyze(
        &self,
        request: OrchestratorRequest,
    ) -> Result<ProviderOutcome, ProviderError> {
        // image.analyze uses Ollama's chat API with a vision model.
        // The media resolver has already inlined base64 for image.source.
        let model = request
            .resolved_model
            .as_ref()
            .ok_or_else(|| ProviderError::Unsupported("model selector required".to_string()))?
            .short_name
            .clone();

        let image_base64 = request
            .payload
            .pointer("/image/source/base64")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ProviderError::Unsupported(
                    "image.source.base64 missing — media resolver should have inlined it"
                        .to_string(),
                )
            })?
            .to_string();

        let prompt = request
            .payload
            .pointer("/text/prompt/user")
            .and_then(|v| v.as_str())
            .unwrap_or("Describe this image.")
            .to_string();

        let body = json!({
            "model": model,
            "messages": [
                {
                    "role": "user",
                    "content": prompt,
                    "images": [image_base64],
                }
            ],
            "stream": false,
        });
        let base = self.pick()?;
        let endpoint = format!("{}/api/chat", base.trim_end_matches('/'));
        let resp = self
            .http
            .post(&endpoint)
            .json(&body)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let resp = check_status(resp, "ollama analyze").await?;
        let wire: ChatResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::Upstream(e.to_string()))?;

        let mut out = Output::new();
        out.set(&keys::text::RESPONSE, wire.message.content);
        if let Some(count) = wire.prompt_eval_count {
            out.set(&keys::usage::TOKENS_INPUT, count);
        }
        if let Some(count) = wire.eval_count {
            out.set(&keys::usage::TOKENS_OUTPUT, count);
        }
        Ok(ProviderOutcome::Sync(out))
    }
}

// ── Registration builders ─────────────────────────────────────

fn honored_text_chat_fields() -> Vec<HonoredField> {
    vec![
        HonoredField::new(keys::text::PROMPT_USER).required(),
        HonoredField::new(keys::text::PROMPT_SYSTEM),
        HonoredField::new(keys::text::PROMPT_PREVIOUS),
        HonoredField::new(keys::text::TOKENS_MAX),
        HonoredField::new(keys::text::SAMPLING_TEMPERATURE),
        HonoredField::new(keys::text::SAMPLING_TOP_P),
        HonoredField::new(keys::text::SAMPLING_TOP_K),
        HonoredField::new(keys::text::SAMPLING_SEED),
        HonoredField::new(keys::text::STOP_SEQUENCES),
        HonoredField::new(keys::text::STREAM),
    ]
}

fn build_chat_registration(provider: &ProviderName) -> Registration {
    Registration {
        id: RegistrationId::generate(),
        provider: provider.clone(),
        primitive: Primitive::TextChat,
        strategy: RegistrationStrategy::Bare,
        honored_fields: honored_text_chat_fields(),
        media_inputs: Vec::new(),
        media_outputs: Vec::new(),
    }
}

fn build_embed_registration(provider: &ProviderName) -> Registration {
    Registration {
        id: RegistrationId::generate(),
        provider: provider.clone(),
        primitive: Primitive::TextEmbed,
        strategy: RegistrationStrategy::Bare,
        honored_fields: vec![HonoredField::new(keys::text::INPUT).required()],
        media_inputs: Vec::new(),
        media_outputs: Vec::new(),
    }
}

fn build_analyze_registration(provider: &ProviderName) -> Registration {
    Registration {
        id: RegistrationId::generate(),
        provider: provider.clone(),
        primitive: Primitive::ImageAnalyze,
        strategy: RegistrationStrategy::Bare,
        honored_fields: vec![
            HonoredField::new(keys::image::SOURCE).required(),
            HonoredField::new(keys::text::PROMPT_USER),
        ],
        media_inputs: vec![MediaInputSpec {
            field: keys::image::SOURCE,
            delivery: MediaDelivery::Base64,
            accepted_types: vec![
                "image/png".to_string(),
                "image/jpeg".to_string(),
                "image/webp".to_string(),
                "image/gif".to_string(),
            ],
            overlay: None,
        }],
        media_outputs: Vec::new(),
    }
}

// ── Payload helpers ───────────────────────────────────────────

fn build_chat_messages(payload: &Value, _vision: bool) -> Result<Vec<Value>, ProviderError> {
    let mut messages: Vec<Value> = Vec::new();

    if let Some(system) = payload.pointer("/text/prompt/system").and_then(|v| v.as_str()) {
        messages.push(json!({"role": "system", "content": system}));
    }

    if let Some(previous) = payload
        .pointer("/text/prompt/previous")
        .and_then(|v| v.as_array())
    {
        for turn in previous {
            if let (Some(user), Some(assistant)) = (
                turn.get("user").and_then(|v| v.as_str()),
                turn.get("assistant").and_then(|v| v.as_str()),
            ) {
                messages.push(json!({"role": "user", "content": user}));
                messages.push(json!({"role": "assistant", "content": assistant}));
            }
        }
    }

    let user = payload
        .pointer("/text/prompt/user")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ProviderError::Unsupported("text.prompt.user missing".to_string()))?;
    messages.push(json!({"role": "user", "content": user}));

    Ok(messages)
}

fn build_options(payload: &Value) -> Value {
    let mut options = serde_json::Map::new();
    if let Some(t) = payload
        .pointer("/text/sampling/temperature")
        .and_then(|v| v.as_f64())
    {
        options.insert("temperature".to_string(), json!(t));
    }
    if let Some(p) = payload.pointer("/text/sampling/top_p").and_then(|v| v.as_f64()) {
        options.insert("top_p".to_string(), json!(p));
    }
    if let Some(k) = payload.pointer("/text/sampling/top_k").and_then(|v| v.as_i64()) {
        options.insert("top_k".to_string(), json!(k));
    }
    if let Some(seed) = payload.pointer("/text/sampling/seed").and_then(|v| v.as_i64()) {
        options.insert("seed".to_string(), json!(seed));
    }
    if let Some(max) = payload.pointer("/text/tokens/max").and_then(|v| v.as_i64()) {
        options.insert("num_predict".to_string(), json!(max));
    }
    if let Some(stops) = payload
        .pointer("/text/stop/sequences")
        .and_then(|v| v.as_array())
    {
        options.insert("stop".to_string(), Value::Array(stops.clone()));
    }
    Value::Object(options)
}

// ── Wire types ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct TagsResponse {
    models: Vec<TagsModel>,
}

#[derive(Debug, Deserialize)]
struct TagsModel {
    name: String,
    #[serde(default)]
    size: u64,
}

/// Subset of the `/api/show` response that the orchestrator
/// consumes. Ollama's full response is much larger; we only deser
/// the fields the recommendation engine cares about.
#[derive(Debug, Deserialize)]
struct ShowResponse {
    /// Vendor-native capability tags: `"completion"`, `"embedding"`,
    /// `"vision"`, `"tools"`, `"thinking"`. Maps to the
    /// orchestrator's primitives and capability profile filters.
    #[serde(default)]
    capabilities: Option<Vec<String>>,
    /// Model metadata key-value bag. Contains
    /// `"general.parameter_count"` and architecture-specific keys
    /// like `"qwen2.context_length"`.
    #[serde(default)]
    model_info: Option<std::collections::HashMap<String, serde_json::Value>>,
    /// Display details. Includes `parameter_size` ("7B"), `family`,
    /// `quantization_level`.
    #[serde(default)]
    details: Option<ShowDetails>,
}

#[derive(Debug, Deserialize)]
struct ShowDetails {
    #[serde(default)]
    parameter_size: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    message: ChatMessage,
    #[serde(default)]
    done_reason: Option<String>,
    #[serde(default)]
    prompt_eval_count: Option<u64>,
    #[serde(default)]
    eval_count: Option<u64>,
    #[serde(default)]
    total_duration: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    #[serde(default)]
    content: String,
}

/// One NDJSON line from Ollama's streaming chat endpoint. Non-final
/// chunks carry a partial `message.content`; the final chunk has
/// `done: true` plus aggregate counters.
#[derive(Debug, Deserialize)]
struct ChatStreamChunk {
    #[serde(default)]
    message: Option<ChatMessage>,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    done_reason: Option<String>,
    #[serde(default)]
    prompt_eval_count: Option<u64>,
    #[serde(default)]
    eval_count: Option<u64>,
    #[serde(default)]
    total_duration: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct EmbedResponse {
    embeddings: Vec<Vec<f32>>,
    #[serde(default)]
    prompt_eval_count: Option<u64>,
}

#[derive(Debug, Serialize)]
struct _PhantomSerialize;
