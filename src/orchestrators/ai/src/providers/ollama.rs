//! Ollama provider — capability-event driven (ORCH-0030 R2 M3).
//!
//! Ollama maintains its own [`OllamaCapabilityMatrix`] built from
//! probing every stone's `/api/tags`, `/api/ps`, and `/api/show`,
//! and publishes a [`CapabilityAnnouncement`] event to the bus on
//! every change. The [`CapabilityDirectory`] (populated by the
//! `DirectorySubscriber` task) is the authoritative view of what
//! Ollama can serve.
//!
//! # Dispatch flow
//!
//! On every request:
//!
//! 1. The caller sends `POST /v1/text/chat` with `selectors.model`
//!    either absent, set to `"recommended:chat"`, or set to a concrete
//!    model name like `"llama3.1:8b"`.
//! 2. `Provider::onboard` reads the current matrix (via a read lock).
//! 3. Extracts the caller's intent:
//!    - Missing / `recommended:*` → resolve via
//!      [`OllamaSelector::pick_recommended`] using the primitive's
//!      default capability.
//!    - Concrete model name → resolve via
//!      [`OllamaSelector::pick_pinned`] to a specific instance.
//! 4. Dispatches to the chosen instance URL with the chosen model.
//!
//! The selector encodes the architectural invariant from R2 of
//! ORCH-0030: **never recommend a model that is not actually
//! installed on a healthy instance.** See [`ollama_matrix::tests::
//! scoring_anti_phantom_filter_rejects_uninstalled_models`].

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::domain::capability_announcement::{
    Capability as AnnCapability, CapabilityAnnouncement, CapabilityMediaInput,
};
use crate::domain::events::EventBus;
use crate::domain::ids::ProviderName;
use crate::domain::keys;
use crate::domain::output::Output;
use crate::domain::primitive::Primitive;
use crate::domain::provider::{Provider, ProviderError, ProviderOutcome};
use crate::domain::request::OrchestratorRequest;
use crate::services::directory_subscriber::publish_capability_announcement;
use crate::services::garden_discovery::GardenDiscovery;

use super::common::{build_http_client, check_status, map_reqwest_error, PerFqnInstances};
use super::ollama_matrix::{
    Capability, InstanceEntry, InstanceHealth, ModelInfo, OllamaCapabilityMatrix, OllamaSelector,
    SelectionError, SelectionResult,
};

/// Ollama base name. Discovery's base-name match automatically picks up
/// `ollama` and any `ollama::adopted`, `ollama::dev`, etc. variants.
const FQNS: &[&'static str] = &["ollama"];

#[derive(Debug, Clone, Default)]
pub struct OllamaConfig;

pub struct OllamaProvider {
    name: ProviderName,
    matrix: Arc<RwLock<OllamaCapabilityMatrix>>,
    http: Client,
    events: Arc<EventBus>,
}

impl OllamaProvider {
    pub fn new(
        _config: OllamaConfig,
        discovery: Arc<GardenDiscovery>,
        events: Arc<EventBus>,
        shutdown: CancellationToken,
    ) -> Arc<Self> {
        let name = ProviderName::new(keys::providers::OLLAMA);
        let provider = Arc::new(Self {
            name: name.clone(),
            matrix: Arc::new(RwLock::new(OllamaCapabilityMatrix::new())),
            http: build_http_client(),
            events,
        });
        spawn_subscriber(provider.clone(), discovery, shutdown);
        provider
    }

    /// Snapshot of the current matrix — used by tests and the
    /// adapter's own scoring path.
    pub async fn matrix_snapshot(&self) -> OllamaCapabilityMatrix {
        self.matrix.read().await.clone()
    }

    /// Publish the current matrix as a capability announcement.
    /// Includes the model selector field with live `options` from
    /// installed models so the catalog can render a dropdown.
    async fn publish_capabilities(&self) {
        let (primitives, has_healthy, model_names) = {
            let matrix = self.matrix.read().await;
            let mut names: Vec<String> = matrix
                .loadable_models()
                .into_iter()
                .map(|s| s.to_string())
                .collect();
            names.sort();
            (
                matrix.supported_primitives(),
                !matrix.healthy_instances().is_empty(),
                names,
            )
        };
        let announcement =
            build_capability_announcement(&self.name, primitives, has_healthy, &model_names);
        publish_capability_announcement(&self.events, &announcement).await;
    }

    /// Rebuild the matrix from the given URL list. Probes every URL
    /// for `/api/tags`, `/api/ps`, and per-model `/api/show`
    /// metadata, marks each instance Healthy / Unhealthy, and
    /// publishes the resulting capability announcement.
    async fn rebuild_matrix(&self, urls: Vec<String>) {
        let mut new_matrix = OllamaCapabilityMatrix::new();

        // Probe every instance in parallel for its model list and
        // loaded models.
        let probe_futs = urls.into_iter().map(|url| {
            let http = self.http.clone();
            async move { probe_instance(&http, &url).await }
        });
        let instance_probes: Vec<Option<InstanceEntry>> =
            futures_util::future::join_all(probe_futs).await;

        // Build the instance map. Unhealthy probes (None) become
        // `InstanceHealth::Unhealthy` entries so the adapter still
        // knows about them but the selector will filter them out.
        for (idx, maybe) in instance_probes.into_iter().enumerate() {
            match maybe {
                Some(entry) => {
                    new_matrix.instances.insert(entry.endpoint.clone(), entry);
                }
                None => {
                    let _ = idx; // placeholder — we do not have the
                                 // URL at this point for unhealthy
                                 // entries; they simply don't appear.
                }
            }
        }

        // Enrich every unique model across all healthy instances via
        // `/api/show`. One call per unique model name using the first
        // reachable instance that hosts it.
        let mut seen_models: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut enrich_tasks: Vec<(String, String)> = Vec::new();
        for inst in new_matrix.instances.values() {
            if !inst.is_routable() {
                continue;
            }
            for m in &inst.models_available {
                if seen_models.insert(m.clone()) {
                    enrich_tasks.push((m.clone(), inst.endpoint.clone()));
                }
            }
        }

        let enrich_futs = enrich_tasks.into_iter().map(|(model, endpoint)| {
            let http = self.http.clone();
            async move {
                let info = enrich_model(&http, &endpoint, &model).await;
                (model, info)
            }
        });
        let enriched: Vec<(String, Option<ModelInfo>)> =
            futures_util::future::join_all(enrich_futs).await;
        for (model, info) in enriched {
            if let Some(info) = info {
                new_matrix.models.insert(model, info);
            }
        }

        // Swap the new matrix in and publish the announcement.
        {
            let mut m = self.matrix.write().await;
            *m = new_matrix;
        }
        self.publish_capabilities().await;
    }

    /// Resolve the caller's model selector against the current
    /// matrix. Returns the concrete (model, endpoint) pair plus the
    /// full selection result for observability.
    async fn resolve_selection(
        &self,
        request: &OrchestratorRequest,
    ) -> Result<SelectionResult, ProviderError> {
        let matrix = self.matrix.read().await;
        let selector_input = request.selectors.model.as_deref();

        // 1. No selector → default capability for this primitive
        // 2. recommended:* moniker → explicit capability
        // 3. Concrete model name → pin
        let result = if let Some(s) = selector_input {
            if let Some(cap) = Capability::parse_recommended(s) {
                OllamaSelector::pick_recommended(&matrix, cap)
            } else {
                // Concrete model — treat as pin.
                OllamaSelector::pick_pinned(&matrix, s)
            }
        } else {
            // Bare primitive — pick the default capability for it.
            let cap = Capability::default_for(request.action.primitive).ok_or_else(|| {
                ProviderError::Unsupported(format!(
                    "ollama does not serve {}",
                    request.action.primitive.dotted()
                ))
            })?;
            OllamaSelector::pick_recommended(&matrix, cap)
        };

        result.map_err(selection_error_to_provider_error)
    }

    fn name_ref(&self) -> &ProviderName {
        &self.name
    }
}

/// Translate a [`SelectionError`] into the canonical [`ProviderError`]
/// taxonomy. The selector's errors are adapter-local; the dispatcher
/// understands the provider-level taxonomy.
fn selection_error_to_provider_error(err: SelectionError) -> ProviderError {
    match err {
        SelectionError::NoHealthyInstances => {
            ProviderError::Unreachable("no healthy ollama instances".to_string())
        }
        SelectionError::NoEligibleModels { capability } => {
            ProviderError::Unsupported(format!(
                "no model on any healthy ollama instance declares capability `{capability}`"
            ))
        }
        SelectionError::PinNotServable { model, reason } => {
            ProviderError::PinNotServable {
                model,
                reason: reason.to_string(),
            }
        }
        SelectionError::UnsupportedPrimitive(p) => {
            ProviderError::Unsupported(format!("ollama does not serve {}", p.dotted()))
        }
    }
}

/// Probe a single Ollama instance for its tags and currently-loaded
/// models. Returns `None` if any call fails — the adapter treats that
/// as "unreachable right now" and the instance simply doesn't appear
/// in the matrix until the next probe.
async fn probe_instance(http: &Client, base: &str) -> Option<InstanceEntry> {
    let base_trim = base.trim_end_matches('/').to_string();

    // `/api/tags` — installed models
    let tags_url = format!("{}/api/tags", base_trim);
    let tags_resp = http
        .get(&tags_url)
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    let tags: TagsResponse = tags_resp.json().await.ok()?;
    let models_available: Vec<String> =
        tags.models.iter().map(|m| m.name.clone()).collect();

    // `/api/ps` — currently-loaded models. Optional — a brand-new
    // instance may not have anything loaded and returns an empty list.
    let ps_url = format!("{}/api/ps", base_trim);
    let models_loaded: Vec<String> = match http
        .get(&ps_url)
        .timeout(Duration::from_secs(5))
        .send()
        .await
    {
        Ok(resp) => match resp.json::<PsResponse>().await {
            Ok(ps) => ps.models.into_iter().map(|m| m.name).collect(),
            Err(_) => Vec::new(),
        },
        Err(_) => Vec::new(),
    };

    // Stone name: extract from the URL. Everything between `//` and
    // the first `:` (port) — good enough for display purposes.
    let stone_name = base_trim
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .split(':')
        .next()
        .unwrap_or("unknown")
        .to_string();

    Some(InstanceEntry {
        endpoint: base_trim,
        stone_name,
        health: InstanceHealth::Healthy,
        models_available,
        models_loaded,
        queue_depth: 0,
    })
}

/// Enrich one model with `/api/show` metadata. Called once per unique
/// model name against the first reachable instance that hosts it.
async fn enrich_model(http: &Client, endpoint: &str, model: &str) -> Option<ModelInfo> {
    let show_url = format!("{}/api/show", endpoint.trim_end_matches('/'));
    let show_resp = http
        .post(&show_url)
        .json(&json!({ "model": model }))
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    let detail: ShowResponse = show_resp.json().await.ok()?;

    let capabilities = detail.capabilities.unwrap_or_default();
    if capabilities.is_empty() {
        return None;
    }

    // Parameter count from model_info first, then parameter_size.
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

    // Context length from any `*.context_length` key in model_info.
    let context_length = detail.model_info.as_ref().and_then(|m| {
        m.iter()
            .find(|(k, _)| k.ends_with(".context_length"))
            .and_then(|(_, v)| v.as_u64())
    });

    Some(ModelInfo {
        name: model.to_string(),
        capabilities,
        parameter_count,
        context_length,
        size_bytes: 0, // /api/show doesn't report it; /api/tags does
                       // but we don't carry the number through here —
                       // scoring doesn't use it.
    })
}

/// Parse Ollama's human-readable parameter-size string into a raw
/// count (`"7B"` → `7_000_000_000`, `"1.5B"` → `1_500_000_000`,
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

// ── Discovery subscriber ─────────────────────────────────────

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
                    let urls: Vec<String> =
                        event.instances.into_iter().map(|i| i.url).collect();
                    pool.set(&event.fqn, urls);
                    provider.rebuild_matrix(pool.flatten()).await;
                }
            }
        }
    });
}

// ── Provider trait impl ──────────────────────────────────────

#[async_trait]
impl Provider for OllamaProvider {
    fn name(&self) -> ProviderName {
        self.name.clone()
    }

    async fn onboard(
        &self,
        request: OrchestratorRequest,
    ) -> Result<ProviderOutcome, ProviderError> {
        let selection = self.resolve_selection(&request).await?;
        let model = selection.winner.model.clone();
        let endpoint = selection.winner.instance.clone();

        // Log the routing decision for observability. Commit 7c adds
        // `dispatch.{id}.routed` to the bus; for now the tracing span
        // carries the information.
        tracing::debug!(
            provider = %self.name_ref(),
            request_id = %request.id,
            model = %model,
            instance = %endpoint,
            stone = %selection.winner.stone_name,
            score = selection.winner.score,
            alternates = selection.alternates.len(),
            "ollama resolved selection",
        );

        match request.action.primitive {
            Primitive::TextChat => self.do_chat(request, &model, &endpoint).await,
            Primitive::TextEmbed => self.do_embed(request, &model, &endpoint).await,
            Primitive::ImageAnalyze => self.do_analyze(request, &model, &endpoint).await,
            other => Err(ProviderError::Unsupported(format!(
                "ollama does not serve {}",
                other.dotted()
            ))),
        }
    }
}

// ── Wire-format dispatch ─────────────────────────────────────
//
// The helper methods below translate canonical orchestrator requests
// into Ollama's wire format. They accept the resolved model and
// endpoint as explicit parameters — unlike the ORCH-0028 version,
// which read them from `request.resolved_model` and round-robined
// via an InstancePool.

impl OllamaProvider {
    async fn do_chat(
        &self,
        request: OrchestratorRequest,
        model: &str,
        base: &str,
    ) -> Result<ProviderOutcome, ProviderError> {
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

    async fn do_chat_streaming(
        &self,
        endpoint: &str,
        body: Value,
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

        let deltas = async_stream::stream! {
            let mut buffer: Vec<u8> = Vec::new();
            let mut byte_stream = byte_stream;
            while let Some(chunk) = byte_stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        buffer.extend_from_slice(&bytes);
                        loop {
                            let Some(nl) = buffer.iter().position(|b| *b == b'\n') else { break };
                            let line: Vec<u8> = buffer.drain(..=nl).collect();
                            let trimmed = &line[..line.len() - 1];
                            if trimmed.is_empty() { continue; }
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
                                    if !out.is_empty() { yield Ok(out); }
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
        model: &str,
        base: &str,
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
        model: &str,
        base: &str,
    ) -> Result<ProviderOutcome, ProviderError> {
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

// ── Media input declarations per primitive ───────────────────

/// Per-primitive media input declarations Ollama publishes as part
/// of its capability announcement. The MediaResolver reads these
/// off the CapabilityDirectory and applies the declared delivery
/// mode (Base64 inline, by-id pass-through, or staged transfer).
///
/// `text.chat` and `text.embed` never carry media → empty list.
/// `image.analyze` accepts one image at `image.source`, delivered
/// as base64 because Ollama's `/api/chat` wire format embeds
/// images inline in the messages array.
fn ollama_media_inputs_for(primitive: Primitive) -> Vec<CapabilityMediaInput> {
    match primitive {
        Primitive::ImageAnalyze => vec![CapabilityMediaInput::base64(
            keys::image::SOURCE.as_str().to_string(),
            vec![
                "image/png".to_string(),
                "image/jpeg".to_string(),
                "image/webp".to_string(),
                "image/gif".to_string(),
            ],
        )],
        _ => Vec::new(),
    }
}

/// Build the capability announcement Ollama publishes for the given
/// matrix snapshot. Pure function — no IO, no `&self` — so unit
/// tests can exercise the wire shape directly.
///
/// `enabled` is true only when the matrix supports at least one
/// primitive AND has at least one healthy instance. Either condition
/// alone is insufficient: a matrix with primitives but no healthy
/// instances means the adapter knows what models are installed but
/// nobody is reachable; a matrix with healthy instances but no
/// supported primitives means the instances exist but lack any
/// capability tag the orchestrator routes on.
fn build_capability_announcement(
    name: &ProviderName,
    primitives: Vec<Primitive>,
    has_healthy_instances: bool,
    model_names: &[String],
) -> CapabilityAnnouncement {
    use crate::domain::capability_announcement::{
        AutoDescriptor, ParameterType, ParameterWidget, SkillParameter,
    };

    let enabled = !primitives.is_empty() && has_healthy_instances;

    // Build the model selector parameter with live options.
    let model_options: Vec<serde_json::Value> =
        model_names.iter().map(|n| json!(n)).collect();

    let capabilities: Vec<AnnCapability> = primitives
        .into_iter()
        .map(|p| {
            let cap_name = capability_name_for(p);
            let mut params = base_parameters_for(p);

            // Add model selector with live options and auto descriptor.
            params.push(SkillParameter {
                field: "selectors.model".into(),
                required: false,
                description: Some("Model to use for this request".into()),
                default: None,
                auto: Some(AutoDescriptor {
                    default: format!("recommended:{cap_name}"),
                    description: Some(format!(
                        "The garden picks the best available {cap_name} model"
                    )),
                }),
                pinnable: true,
                label: Some("Model".into()),
                field_type: Some(ParameterType::String),
                widget: Some(ParameterWidget::Select),
                min: None,
                max: None,
                step: None,
                options: if model_options.is_empty() {
                    None
                } else {
                    Some(model_options.clone())
                },
                placeholder: None,
            });

            AnnCapability {
                primitive: p,
                media_inputs: ollama_media_inputs_for(p),
                parameters: params,
            }
        })
        .collect();

    CapabilityAnnouncement {
        provider: name.clone(),
        enabled,
        capabilities,
        skills: Vec::new(),
    }
}

/// Human-readable capability name for the recommended:* moniker.
fn capability_name_for(p: Primitive) -> &'static str {
    match p {
        Primitive::TextChat => "chat",
        Primitive::TextTranslate => "translate",
        Primitive::TextEmbed => "embed",
        Primitive::TextRerank => "rerank",
        Primitive::ImageGenerate => "generate",
        Primitive::ImageEdit => "edit",
        Primitive::ImageUpscale => "upscale",
        Primitive::ImageAnalyze => "vision",
        Primitive::AudioGenerate => "speech",
        Primitive::AudioTranscribe => "transcribe",
    }
}

/// Base form-schema parameters for Ollama's primitives.
/// These are the fields the Try It form renders (excluding
/// the model selector, which is added separately with live options).
fn base_parameters_for(p: Primitive) -> Vec<crate::domain::capability_announcement::SkillParameter> {
    use crate::domain::capability_announcement::{ParameterType, ParameterWidget, SkillParameter};

    match p {
        Primitive::TextChat => vec![
            SkillParameter {
                field: "text.prompt.user".into(),
                required: true,
                label: Some("Message".into()),
                field_type: Some(ParameterType::String),
                widget: Some(ParameterWidget::Textarea),
                placeholder: Some("Ask anything...".into()),
                ..Default::default()
            },
            SkillParameter {
                field: "text.prompt.system".into(),
                required: false,
                label: Some("System Prompt".into()),
                field_type: Some(ParameterType::String),
                widget: Some(ParameterWidget::Textarea),
                placeholder: Some("You are a helpful assistant...".into()),
                ..Default::default()
            },
            SkillParameter {
                field: "text.sampling.temperature".into(),
                required: false,
                label: Some("Temperature".into()),
                field_type: Some(ParameterType::Number),
                widget: Some(ParameterWidget::Slider),
                default: Some(json!(0.7)),
                min: Some(0.0),
                max: Some(2.0),
                step: Some(0.1),
                ..Default::default()
            },
            SkillParameter {
                field: "text.tokens.max".into(),
                required: false,
                label: Some("Max Tokens".into()),
                field_type: Some(ParameterType::Integer),
                widget: Some(ParameterWidget::Number),
                default: Some(json!(2048)),
                min: Some(1.0),
                max: Some(131072.0),
                ..Default::default()
            },
        ],
        Primitive::TextEmbed => vec![
            SkillParameter {
                field: "text.input".into(),
                required: true,
                label: Some("Text".into()),
                field_type: Some(ParameterType::String),
                widget: Some(ParameterWidget::Textarea),
                placeholder: Some("Text to embed...".into()),
                ..Default::default()
            },
        ],
        Primitive::ImageAnalyze => vec![
            SkillParameter {
                field: "text.prompt.user".into(),
                required: true,
                label: Some("Question".into()),
                field_type: Some(ParameterType::String),
                widget: Some(ParameterWidget::Textarea),
                placeholder: Some("Describe this image...".into()),
                ..Default::default()
            },
        ],
        // Other primitives get an empty field list — they'll gain
        // parameters as Ollama's support for them matures.
        _ => vec![],
    }
}

// ── Payload helpers ──────────────────────────────────────────

fn build_chat_messages(payload: &Value, _vision: bool) -> Result<Vec<Value>, ProviderError> {
    let mut messages: Vec<Value> = Vec::new();

    if let Some(system) = payload
        .pointer("/text/prompt/system")
        .and_then(|v| v.as_str())
    {
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
    if let Some(p) = payload
        .pointer("/text/sampling/top_p")
        .and_then(|v| v.as_f64())
    {
        options.insert("top_p".to_string(), json!(p));
    }
    if let Some(k) = payload
        .pointer("/text/sampling/top_k")
        .and_then(|v| v.as_i64())
    {
        options.insert("top_k".to_string(), json!(k));
    }
    if let Some(seed) = payload
        .pointer("/text/sampling/seed")
        .and_then(|v| v.as_i64())
    {
        options.insert("seed".to_string(), json!(seed));
    }
    if let Some(max) = payload
        .pointer("/text/tokens/max")
        .and_then(|v| v.as_i64())
    {
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

// ── Wire types ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct TagsResponse {
    models: Vec<TagsModel>,
}

#[derive(Debug, Deserialize)]
struct TagsModel {
    name: String,
    #[serde(default)]
    #[allow(dead_code)]
    size: u64,
}

#[derive(Debug, Deserialize, Default)]
struct PsResponse {
    #[serde(default)]
    models: Vec<PsModel>,
}

#[derive(Debug, Deserialize)]
struct PsModel {
    name: String,
}

#[derive(Debug, Deserialize)]
struct ShowResponse {
    #[serde(default)]
    capabilities: Option<Vec<String>>,
    #[serde(default)]
    model_info: Option<std::collections::HashMap<String, Value>>,
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

// ── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_parameter_size_common_forms() {
        assert_eq!(parse_parameter_size("7B"), Some(7_000_000_000));
        assert_eq!(parse_parameter_size("1.5B"), Some(1_500_000_000));
        assert_eq!(parse_parameter_size("137M"), Some(137_000_000));
        assert_eq!(parse_parameter_size("512K"), Some(512_000));
        assert_eq!(parse_parameter_size("1000"), Some(1000));
    }

    #[test]
    fn selection_error_mapping_preserves_intent() {
        let err = selection_error_to_provider_error(SelectionError::NoHealthyInstances);
        assert!(matches!(err, ProviderError::Unreachable(_)));

        let err = selection_error_to_provider_error(SelectionError::NoEligibleModels {
            capability: "completion",
        });
        assert!(matches!(err, ProviderError::Unsupported(_)));

        let err = selection_error_to_provider_error(SelectionError::PinNotServable {
            model: "llama3.1:8b".into(),
            reason: "no healthy instance",
        });
        assert!(matches!(err, ProviderError::PinNotServable { .. }));
    }

    // ── M4: capability publication ──

    fn provider_name() -> ProviderName {
        ProviderName::new(keys::providers::OLLAMA)
    }

    #[test]
    fn announcement_disabled_when_no_primitives() {
        let ann = build_capability_announcement(&provider_name(), Vec::new(), true, &[]);
        assert!(!ann.enabled);
        assert!(ann.capabilities.is_empty());
    }

    #[test]
    fn announcement_disabled_when_no_healthy_instances() {
        // Even if the matrix knows about supported primitives, an
        // adapter with zero healthy instances cannot serve traffic.
        let ann = build_capability_announcement(
            &provider_name(),
            vec![Primitive::TextChat],
            false,
            &[],
        );
        assert!(!ann.enabled);
    }

    #[test]
    fn announcement_enabled_when_primitives_and_healthy_instances() {
        let ann = build_capability_announcement(
            &provider_name(),
            vec![Primitive::TextChat],
            true,
            &[],
        );
        assert!(ann.enabled);
        assert_eq!(ann.capabilities.len(), 1);
        assert_eq!(ann.capabilities[0].primitive, Primitive::TextChat);
    }

    #[test]
    fn announcement_text_primitives_carry_no_media_inputs() {
        let ann = build_capability_announcement(
            &provider_name(),
            vec![Primitive::TextChat, Primitive::TextEmbed],
            true,
            &[],
        );
        assert_eq!(ann.capabilities.len(), 2);
        for cap in &ann.capabilities {
            assert!(cap.media_inputs.is_empty(), "{:?} should have no media", cap.primitive);
        }
    }

    #[test]
    fn announcement_image_analyze_carries_base64_media_input() {
        let ann = build_capability_announcement(
            &provider_name(),
            vec![Primitive::ImageAnalyze],
            true,
            &[],
        );
        let cap = &ann.capabilities[0];
        assert_eq!(cap.primitive, Primitive::ImageAnalyze);
        assert_eq!(cap.media_inputs.len(), 1);
        let media = &cap.media_inputs[0];
        assert_eq!(media.field, "image.source");
        assert!(matches!(
            media.delivery,
            crate::domain::media::MediaDelivery::Base64
        ));
        assert!(media.accepted_types.contains(&"image/png".to_string()));
        assert!(media.accepted_types.contains(&"image/jpeg".to_string()));
        assert!(media.accepted_types.contains(&"image/webp".to_string()));
    }

    #[test]
    fn announcement_publishes_no_skills_in_m1() {
        // Ollama's first skill (image-understanding) lands in a
        // post-M1 commit; in M1 the skills list is empty even when
        // capabilities are populated.
        let ann = build_capability_announcement(
            &provider_name(),
            vec![Primitive::TextChat, Primitive::ImageAnalyze],
            true,
            &[],
        );
        assert!(ann.skills.is_empty());
    }

    #[test]
    fn announcement_provider_name_matches_keys_constant() {
        let ann =
            build_capability_announcement(&provider_name(), vec![Primitive::TextChat], true, &[]);
        assert_eq!(ann.provider.as_str(), keys::providers::OLLAMA);
        assert_eq!(ann.provider.as_str(), "ollama");
    }

    #[test]
    fn announcement_chat_carries_form_schema_parameters() {
        let models = vec!["llama3.1:8b".to_string(), "qwen2.5:7b".to_string()];
        let ann = build_capability_announcement(
            &provider_name(),
            vec![Primitive::TextChat],
            true,
            &models,
        );
        let cap = &ann.capabilities[0];
        assert!(!cap.parameters.is_empty(), "chat should have form-schema parameters");

        // Find the model selector parameter
        let model_param = cap.parameters.iter().find(|p| p.field == "selectors.model");
        assert!(model_param.is_some(), "should have a selectors.model parameter");
        let mp = model_param.unwrap();

        // It should be a Select widget with auto descriptor
        assert_eq!(
            mp.widget,
            Some(crate::domain::capability_announcement::ParameterWidget::Select)
        );
        assert!(mp.auto.is_some());
        assert_eq!(mp.auto.as_ref().unwrap().default, "recommended:chat");

        // Options should contain the live model names
        let opts = mp.options.as_ref().expect("should have options");
        assert_eq!(opts.len(), 2);
        assert!(opts.contains(&serde_json::json!("llama3.1:8b")));
        assert!(opts.contains(&serde_json::json!("qwen2.5:7b")));

        // Should also have prompt and temperature fields
        let prompt = cap.parameters.iter().find(|p| p.field == "text.prompt.user");
        assert!(prompt.is_some(), "should have text.prompt.user");
        assert!(prompt.unwrap().required);

        let temp = cap.parameters.iter().find(|p| p.field == "text.sampling.temperature");
        assert!(temp.is_some(), "should have temperature");
        let t = temp.unwrap();
        assert_eq!(t.min, Some(0.0));
        assert_eq!(t.max, Some(2.0));
        assert_eq!(t.step, Some(0.1));
        assert_eq!(t.default, Some(serde_json::json!(0.7)));
    }

    #[test]
    fn announcement_empty_models_omits_options() {
        let ann = build_capability_announcement(
            &provider_name(),
            vec![Primitive::TextChat],
            true,
            &[],
        );
        let cap = &ann.capabilities[0];
        let model_param = cap.parameters.iter().find(|p| p.field == "selectors.model").unwrap();
        // No models → no options (but auto descriptor still present)
        assert!(model_param.options.is_none());
        assert!(model_param.auto.is_some());
    }
}
