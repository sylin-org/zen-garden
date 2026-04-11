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
    Capability as AnnCapability, CapabilityAnnouncement, CapabilityMediaInput, Example,
};
use crate::domain::events::EventBus;
use crate::domain::ids::ProviderName;
use crate::domain::keys;
use crate::domain::output::Output;
use crate::domain::primitive::Primitive;
use crate::domain::provider::{
    Provider, ProviderError, ProviderMeta, ProviderOutcome, ProviderResult, WorkspaceDescription,
};
use crate::domain::request::OrchestratorRequest;
use crate::services::directory_subscriber::publish_capability_announcement;
use crate::services::garden_discovery::GardenDiscovery;

use super::common::{build_http_client, check_status, map_reqwest_error, truncate_str, PerFqnInstances};
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
    /// Cross-adapter resource domain — consulted at matrix-build
    /// time to filter models that no stone can host.
    resources: Arc<crate::domain::resources::Resources>,
    /// Preferences store. Read at matrix-build time for the
    /// `orchestrator.strict_fit` setting (ORCH-0038). Default
    /// `true` when the setting is unset.
    preferences: Arc<crate::domain::preferences::Preferences>,
}

impl OllamaProvider {
    pub fn new(
        _config: OllamaConfig,
        discovery: Arc<GardenDiscovery>,
        events: Arc<EventBus>,
        resources: Arc<crate::domain::resources::Resources>,
        preferences: Arc<crate::domain::preferences::Preferences>,
        shutdown: CancellationToken,
    ) -> Arc<Self> {
        let name = ProviderName::new(keys::providers::OLLAMA);
        let provider = Arc::new(Self {
            name: name.clone(),
            matrix: Arc::new(RwLock::new(OllamaCapabilityMatrix::new())),
            http: build_http_client(),
            events,
            resources,
            preferences,
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

    /// Rebuild the matrix from the given binding list. Probes
    /// every URL for `/api/tags`, `/api/ps`, and per-model
    /// `/api/show` metadata, marks each instance Healthy /
    /// Unhealthy, and publishes the resulting capability
    /// announcement.
    ///
    /// Each binding carries the stone_name from garden discovery
    /// so the probed `InstanceEntry` is stamped with the real
    /// identity Moss knows — matching the `StoneName` keys the
    /// Resources domain uses for the ORCH-0038 fit filter.
    async fn rebuild_matrix(&self, bindings: Vec<crate::providers::common::InstanceBinding>) {
        let mut new_matrix = OllamaCapabilityMatrix::new();

        // Probe every instance in parallel for its model list and
        // loaded models. Each probe is stamped with the
        // discovery-provided stone_name, not one derived from the
        // URL.
        let probe_futs = bindings.into_iter().map(|binding| {
            let http = self.http.clone();
            async move { probe_instance(&http, &binding.url, &binding.stone_name).await }
        });
        let instance_probes: Vec<Option<InstanceProbe>> =
            futures_util::future::join_all(probe_futs).await;

        // Union of model disk sizes across all probed instances.
        // Different instances should report the same size for the
        // same model name (it's the GGUF file's on-disk size); we
        // take the first observation.
        let mut sizes_by_model: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();

        // Build the instance map. Unhealthy probes (None) become
        // `InstanceHealth::Unhealthy` entries so the adapter still
        // knows about them but the selector will filter them out.
        for maybe in instance_probes.into_iter() {
            if let Some(probe) = maybe {
                for (name, size) in probe.sizes_by_model {
                    sizes_by_model.entry(name).or_insert(size);
                }
                new_matrix
                    .instances
                    .insert(probe.entry.endpoint.clone(), probe.entry);
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
            if let Some(mut info) = info {
                // Backfill disk size from /api/tags union — the
                // fit filter uses this as a lower bound on
                // required VRAM for cold models.
                if let Some(&size) = sizes_by_model.get(&model) {
                    info.size_bytes = size;
                }
                new_matrix.models.insert(model, info);
            }
        }

        // ORCH-0038 fit filter (M3): drop any model whose required
        // VRAM exceeds the largest GPU in the garden. Consults the
        // Resources domain for per-stone GPU topology (populated by
        // the garden_hardware puller in M1) and removes models that
        // have no possible host.
        //
        // The filter internally checks the `orchestrator.strict_fit`
        // preference (default true). Operators who want to see
        // every installed model — regardless of whether it can
        // run — can flip the setting via the preferences API.
        self.apply_fit_filter(&mut new_matrix).await;

        // Swap the new matrix in and publish the announcement.
        {
            let mut m = self.matrix.write().await;
            *m = new_matrix;
        }
        self.publish_capabilities().await;
    }

    /// Apply the ORCH-0038 fit filter to a freshly-built matrix.
    /// Removes any model whose required VRAM exceeds the largest
    /// GPU in the garden on any stone that hosts it.
    ///
    /// Walked per model rather than per stone because the question
    /// is "does any stone I could route this to have the capacity"
    /// — not "does this specific stone have capacity". A model
    /// survives the filter if at least one healthy instance that
    /// hosts it lives on a stone that could host its workload
    /// according to `Resources::stones_capable_of`.
    ///
    /// Unknown required VRAM (`required_vram_bytes() == 0`) is
    /// treated permissively — the workload passes unless the stone
    /// explicitly fails. Matches the old-orchestrator intent of
    /// not blocking on absence of evidence; the learning loop (M7)
    /// tightens this over time.
    async fn apply_fit_filter(
        &self,
        matrix: &mut OllamaCapabilityMatrix,
    ) {
        use crate::domain::resources::{StoneName, Workload};

        // Preferences gate: when `orchestrator.strict_fit` is
        // explicitly false, the filter is a no-op. Default is
        // true — surfacing unhostable models is a lie by omission.
        let strict = self
            .preferences
            .get_setting_bool("orchestrator.strict_fit", true)
            .await;
        if !strict {
            return;
        }

        // Build a map from model name → set of stone names that
        // host it, walking the healthy instances.
        let mut hosts: std::collections::HashMap<String, std::collections::HashSet<StoneName>> =
            std::collections::HashMap::new();
        for inst in matrix.instances.values() {
            if !inst.is_routable() {
                continue;
            }
            let stone = StoneName::new(&inst.stone_name);
            for model in &inst.models_available {
                hosts
                    .entry(model.clone())
                    .or_default()
                    .insert(stone.clone());
            }
        }

        // Walk every model in the matrix. For each model, ask the
        // Resources domain which stones could host its workload.
        // Intersect with the instances that actually have the
        // model on disk; if the intersection is empty, drop it.
        let all_model_names: Vec<String> = matrix.models.keys().cloned().collect();
        let mut dropped: Vec<(String, u64)> = Vec::new();
        for name in all_model_names {
            let required = matrix
                .models
                .get(&name)
                .map(|m| m.required_vram_bytes())
                .unwrap_or(0);

            // Unknown required VRAM → permissive. Matches old
            // orchestrator intent. The learning loop (M7) tightens
            // this by recording actual load outcomes.
            if required == 0 {
                continue;
            }

            // Required VRAM in MB for the Resources domain query.
            // Round up: a 7_000_000_001-byte model needs at least
            // 6676 MB, not 6675.
            let required_mb = required.div_ceil(1024 * 1024);

            // Ollama is stack-agnostic: llama.cpp picks the best
            // backend for the GPU at runtime (CUDA, ROCm, Vulkan,
            // Metal, CPU). Any device with a declared stack is a
            // valid target — we only care about the VRAM budget.
            let workload = Workload::any_gpu(Some(required_mb));
            let capable = self.resources.stones_capable_of(&workload).await;

            let available_hosts = hosts.get(&name).cloned().unwrap_or_default();
            let intersection_empty = available_hosts
                .intersection(&capable)
                .next()
                .is_none();

            if intersection_empty {
                dropped.push((name.clone(), required));
                matrix.models.remove(&name);
            }
        }

        if !dropped.is_empty() {
            tracing::info!(
                count = dropped.len(),
                "ollama fit filter dropped models (no stone in garden has enough VRAM)"
            );
            for (name, required) in &dropped {
                tracing::debug!(
                    model = %name,
                    required_mb = required / 1_048_576,
                    "ollama fit filter: dropped"
                );
            }
        }
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
/// Probe result for one instance: the `InstanceEntry` itself plus a
/// side-channel map of `model_name → disk_size_bytes` extracted
/// from the same `/api/tags` response. The caller unions these
/// maps across instances to populate `ModelInfo.size_bytes` during
/// matrix enrichment — needed by the M3 fit filter as a lower
/// bound on required VRAM.
struct InstanceProbe {
    entry: InstanceEntry,
    sizes_by_model: std::collections::HashMap<String, u64>,
}

async fn probe_instance(
    http: &Client,
    base: &str,
    stone_name: &str,
) -> Option<InstanceProbe> {
    let base_trim = base.trim_end_matches('/').to_string();

    // `/api/tags` — installed models (with on-disk sizes)
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
    let sizes_by_model: std::collections::HashMap<String, u64> = tags
        .models
        .iter()
        .map(|m| (m.name.clone(), m.size))
        .collect();

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

    Some(InstanceProbe {
        entry: InstanceEntry {
            endpoint: base_trim,
            stone_name: stone_name.to_string(),
            health: InstanceHealth::Healthy,
            models_available,
            models_loaded,
            queue_depth: 0,
        },
        sizes_by_model,
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
        // `size_bytes` populated by the caller after this returns
        // — /api/show doesn't report it, /api/tags does. The fit
        // filter (M3) needs it as a conservative lower bound on
        // required VRAM until M4's /api/ps probe captures the
        // measured value.
        size_bytes: 0,
        observed_vram_bytes: None,
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
    use crate::providers::common::InstanceBinding;
    tokio::spawn(async move {
        let pool = PerFqnInstances::new();
        let mut rx = discovery.subscribe(FQNS).await;
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                event = rx.recv() => {
                    let Some(event) = event else { break };
                    // Keep the stone_name from discovery alongside
                    // each URL. Deriving stone identity from the
                    // URL (IP/hostname) doesn't match the Resources
                    // domain's `StoneName` keys — the ORCH-0038 fit
                    // filter would always miss.
                    let bindings: Vec<InstanceBinding> = event
                        .instances
                        .into_iter()
                        .map(|i| InstanceBinding {
                            url: i.url,
                            stone_name: i.stone_name,
                        })
                        .collect();
                    pool.set_bindings(&event.fqn, bindings);
                    provider.rebuild_matrix(pool.flatten_bindings()).await;
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
    ) -> Result<ProviderResult, ProviderError> {
        let selection = self.resolve_selection(&request).await?;
        let model = selection.winner.model.clone();
        let endpoint = selection.winner.instance.clone();
        let stone = selection.winner.stone_name.clone();

        tracing::debug!(
            provider = %self.name_ref(),
            request_id = %request.id,
            model = %model,
            instance = %endpoint,
            stone = %stone,
            score = selection.winner.score,
            alternates = selection.alternates.len(),
            "ollama resolved selection",
        );

        // Extract input preview for summary before move.
        let input_preview = request
            .payload
            .pointer("/text/prompt/user")
            .or_else(|| request.payload.pointer("/text/input"))
            .and_then(|v| v.as_str())
            .map(|s| truncate_str(s, 25))
            .unwrap_or_default();

        let primitive = request.action.primitive;
        let outcome = match primitive {
            Primitive::TextChat => self.do_chat(request, &model, &endpoint).await?,
            Primitive::TextEmbed => self.do_embed(request, &model, &endpoint).await?,
            Primitive::ImageAnalyze => self.do_analyze(request, &model, &endpoint).await?,
            other => {
                return Err(ProviderError::Unsupported(format!(
                    "ollama does not serve {}",
                    other.dotted()
                )));
            }
        };

        // Extract output preview and tokens from the outcome.
        let (output_preview, tokens_in, tokens_out) = match &outcome {
            ProviderOutcome::Sync(out) => {
                let text = out
                    .get(&keys::text::RESPONSE)
                    .or_else(|| out.get(&keys::text::DETECTED_LANGUAGE))
                    .and_then(|v| v.as_str())
                    .map(|s| truncate_str(s, 25));
                let ti = out.get(&keys::usage::TOKENS_INPUT).and_then(|v| v.as_u64());
                let to = out.get(&keys::usage::TOKENS_OUTPUT).and_then(|v| v.as_u64());
                (text, ti, to)
            }
            _ => (None, None, None),
        };

        let summary = match primitive {
            Primitive::TextChat => {
                let out_part = output_preview
                    .map(|o| format!(" → '{o}'"))
                    .unwrap_or_default();
                Some(format!("'{input_preview}'{out_part}"))
            }
            Primitive::TextEmbed => {
                let dims = match &outcome {
                    ProviderOutcome::Sync(out) => out
                        .get(&keys::text::DIMENSIONS)
                        .and_then(|v| v.as_u64()),
                    _ => None,
                };
                let dims_str = dims.map(|d| format!(", {d} dims")).unwrap_or_default();
                Some(format!("'{input_preview}'{dims_str}"))
            }
            Primitive::ImageAnalyze => {
                let out_part = output_preview
                    .map(|o| format!(" → '{o}'"))
                    .unwrap_or_default();
                Some(format!("'{input_preview}'{out_part}"))
            }
            _ => None,
        };

        Ok(ProviderResult {
            outcome,
            meta: ProviderMeta {
                model: Some(model),
                instance: Some(endpoint),
                stone: Some(stone),
                tokens_in,
                tokens_out,
                summary,
            },
        })
    }

    async fn describe_workspace(
        &self,
        primitive: Primitive,
        model_hint: Option<&str>,
    ) -> Option<WorkspaceDescription> {
        // Snapshot the matrix: supported primitives, the list of
        // loadable models that declare the tag for this primitive,
        // and the resolved model's capability tags. The capabilities
        // feed the per-model overlay hook in base_parameters_for
        // (e.g. the `thinking` toggle for reasoning models).
        let (supported, mut model_names, resolved_caps) = {
            let matrix = self.matrix.read().await;
            let supported = matrix.supported_primitives();
            let loadable = matrix.loadable_models();
            let tag = match primitive {
                Primitive::TextChat => Some("completion"),
                Primitive::TextEmbed => Some("embedding"),
                Primitive::ImageAnalyze => Some("vision"),
                _ => None,
            };
            let names: Vec<String> = match tag {
                Some(tag) => {
                    let mut v: Vec<String> = matrix
                        .models
                        .values()
                        .filter(|m| loadable.contains(m.name.as_str()))
                        .filter(|m| m.has_capability(tag))
                        .map(|m| m.name.clone())
                        .collect();
                    v.sort();
                    v.dedup();
                    v
                }
                None => Vec::new(),
            };

            // Resolve the model name: hint if available, else the
            // first candidate. Then snapshot its capability tags so
            // we can drop the lock before touching the builder.
            let resolved_name: Option<String> = match model_hint {
                Some(m) if names.iter().any(|n| n == m) => Some(m.to_string()),
                Some(_) => None,
                None => names.first().cloned(),
            };
            let caps: Vec<String> = resolved_name
                .as_deref()
                .and_then(|n| matrix.models.get(n))
                .map(|m| m.capabilities.clone())
                .unwrap_or_default();

            // The hint was provided but no matching model exists.
            if model_hint.is_some() && resolved_name.is_none() {
                return None;
            }
            (supported, names, (resolved_name, caps))
        };

        if !supported.contains(&primitive) {
            return None;
        }

        let (resolved, capabilities) = resolved_caps;

        // Ensure deterministic order for options rendering.
        model_names.sort();

        // Build fields directly — base params tailored to the
        // resolved model, plus the live-options model selector.
        // This is the ORCH-0038 hook point: base_parameters_for
        // receives the resolved model context and may append
        // per-model overlay fields (e.g. reasoning-mode controls).
        let ctx = resolved.as_deref().map(|name| ResolvedModelContext {
            name,
            capabilities: &capabilities,
        });
        let mut fields = base_parameters_for(primitive, ctx.as_ref());
        fields.push(build_model_selector(primitive, &model_names));
        let media_inputs = ollama_workspace_media_inputs_for(primitive, ctx.as_ref());
        // `ctx` borrows `resolved` and `capabilities`; drop it before
        // moving `resolved` into the returned struct.
        drop(ctx);

        Some(WorkspaceDescription {
            resolved_model: resolved,
            fields,
            media_inputs,
            examples: ollama_examples_for(primitive),
        })
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

        // ORCH-0038 per-model overlay: reasoning models honor a
        // `think: true` flag that asks them to emit chain-of-thought
        // in a separate `message.thinking` field. Non-reasoning
        // models ignore it. We only forward the flag when the form
        // set it — omitting it preserves the model's default
        // behavior.
        let think = request
            .payload
            .pointer("/text/reasoning/think")
            .and_then(|v| v.as_bool());

        let messages = build_chat_messages(&request.payload, false)?;
        let options = build_options(&request.payload);
        let mut body = json!({
            "model": model,
            "messages": messages,
            "options": options,
            "stream": stream_requested,
        });
        if let Some(think) = think {
            if let Some(obj) = body.as_object_mut() {
                obj.insert("think".into(), json!(think));
            }
        }
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
        if let Some(thinking) = wire.message.thinking {
            if !thinking.is_empty() {
                out.set(&keys::text::REASONING, thinking);
            }
        }
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
                                        if let Some(t) = msg.thinking {
                                            if !t.is_empty() {
                                                out.set(&keys::text::REASONING, t);
                                            }
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

/// Accepted image types for Ollama vision paths. Shared between
/// `image.analyze` and the `text.chat` vision overlay so both surface
/// the same media contract.
const OLLAMA_IMAGE_TYPES: &[&str] =
    &["image/png", "image/jpeg", "image/webp", "image/gif"];

fn ollama_image_source_input() -> CapabilityMediaInput {
    CapabilityMediaInput::base64(
        keys::image::SOURCE.as_str().to_string(),
        OLLAMA_IMAGE_TYPES.iter().map(|s| s.to_string()).collect(),
    )
}

/// The **superset** of media inputs Ollama might accept for a
/// primitive, published in the startup capability announcement.
/// The [`crate::services::media_resolver::MediaResolver`] reads
/// this off the `CapabilityDirectory` and uses it to validate
/// incoming media references — it must be permissive enough to
/// accept any media that ANY Ollama model could need.
///
/// This is deliberately **wider** than what the UI form shows.
/// The form surface is per-model and comes from
/// `describe_workspace` via [`ollama_workspace_media_inputs_for`];
/// the announcement covers the union so dispatches with a media
/// reference pointed at a text.chat primitive aren't rejected for
/// unknown fields when the caller knows the resolved model supports
/// it.
fn ollama_announcement_media_inputs_for(primitive: Primitive) -> Vec<CapabilityMediaInput> {
    match primitive {
        Primitive::ImageAnalyze | Primitive::TextChat => {
            vec![ollama_image_source_input()]
        }
        _ => Vec::new(),
    }
}

/// Per-model media inputs surfaced on the live workspace form via
/// `describe_workspace`. This is the **subset** of the superset
/// above — only the slots that apply to the currently-resolved
/// model. Non-vision chat models see no image slot; vision models
/// do; `image.analyze` always does.
///
/// This is the ORCH-0038 hook on the media-inputs axis — analogous
/// to `base_parameters_for` for form fields.
fn ollama_workspace_media_inputs_for(
    primitive: Primitive,
    resolved: Option<&ResolvedModelContext<'_>>,
) -> Vec<CapabilityMediaInput> {
    match primitive {
        Primitive::ImageAnalyze => vec![ollama_image_source_input()],
        Primitive::TextChat
            if resolved.is_some_and(|ctx| ctx.has_capability("vision")) =>
        {
            vec![ollama_image_source_input()]
        }
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
    let enabled = !primitives.is_empty() && has_healthy_instances;

    let capabilities: Vec<AnnCapability> = primitives
        .into_iter()
        .map(|p| {
            // Startup announcement: no resolved model yet. The
            // per-model overlays (e.g. reasoning-mode toggle) only
            // appear on the live describe_workspace path, where the
            // adapter looks up the resolved model's capabilities
            // from the matrix.
            let mut params = base_parameters_for(p, None);
            params.push(build_model_selector(p, model_names));

            AnnCapability {
                primitive: p,
                priority: 0,
                // Startup announcement publishes the SUPERSET of
                // accepted media so the resolver validates any
                // reference a caller might include. The UI form's
                // per-model subset comes from describe_workspace.
                media_inputs: ollama_announcement_media_inputs_for(p),
                parameters: params,
                examples: ollama_examples_for(p),
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

/// Build Ollama's `selectors.model` field with live options and a
/// `recommended:{cap}` auto descriptor. Extracted so both the
/// startup announcement and the live `describe_workspace` path build
/// it the same way.
fn build_model_selector(
    p: Primitive,
    model_names: &[String],
) -> crate::domain::capability_announcement::SkillParameter {
    use crate::domain::capability_announcement::{
        AutoDescriptor, ParameterType, ParameterWidget, SkillParameter,
    };

    let cap_name = capability_name_for(p);
    let options: Vec<Value> = model_names.iter().map(|n| json!(n)).collect();

    SkillParameter {
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
        options: if options.is_empty() { None } else { Some(options) },
        placeholder: None,
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

/// Per-primitive examples for Ollama's capability announcements.
fn ollama_examples_for(p: Primitive) -> Vec<Example> {
    match p {
        Primitive::TextChat => vec![Example {
            label: "Ask about geography".into(),
            description: Some("A factual question to test knowledge".into()),
            payload: json!({"text": {"prompt": {"user": "What are the three largest countries by area, and what makes each one geographically unique?"}}}),
        }],
        Primitive::TextEmbed => vec![Example {
            label: "Embed a sentence".into(),
            description: Some("A sample sentence for semantic embedding".into()),
            payload: json!({"text": {"input": "The quick brown fox jumps over the lazy dog"}}),
        }],
        Primitive::ImageAnalyze => vec![Example {
            label: "Describe what you see".into(),
            description: Some("Vision analysis prompt".into()),
            payload: json!({"text": {"prompt": {"user": "Describe everything you see in this image in detail."}}}),
        }],
        _ => vec![],
    }
}

/// Per-model context passed to `base_parameters_for`. Carries the
/// resolved model name and the capability tags that model declares
/// (harvested from Ollama's `/api/show` at matrix-build time). The
/// helper inspects these to decide whether to append model-specific
/// fields like the `thinking` toggle for reasoning models.
pub(super) struct ResolvedModelContext<'a> {
    pub name: &'a str,
    pub capabilities: &'a [String],
}

impl<'a> ResolvedModelContext<'a> {
    fn has_capability(&self, tag: &str) -> bool {
        self.capabilities.iter().any(|c| c == tag)
    }
}

/// Base form-schema parameters for Ollama's primitives.
/// These are the fields the Try It form renders (excluding
/// the model selector, which is added separately with live options).
///
/// `resolved` is the concrete model the provider would use for this
/// call, along with its capability tags. When `Some`, the function
/// may append model-specific fields (e.g. a `thinking` toggle for
/// reasoning models). When `None` (used at startup, before any model
/// is resolved), the function returns the base field surface only.
fn base_parameters_for(
    p: Primitive,
    resolved: Option<&ResolvedModelContext<'_>>,
) -> Vec<crate::domain::capability_announcement::SkillParameter> {
    use crate::domain::capability_announcement::{ParameterType, ParameterWidget, SkillParameter};

    let mut params = match p {
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
            // Conversation history: the dashboard renders this as a
            // dialogue thread widget. The type and widget carry the
            // rendering intent — no special-casing on field names.
            SkillParameter {
                field: "text.prompt.history".into(),
                required: false,
                label: Some("Conversation".into()),
                field_type: Some(ParameterType::Dialogue),
                widget: Some(ParameterWidget::Dialogue),
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
    };

    // ── Per-model field overlays (ORCH-0038) ─────────────────────
    //
    // Ollama's reasoning models (deepseek-r1, qwq, magistral, ...)
    // advertise a `thinking` capability tag in `/api/show`. When
    // enabled via the request body's `think: true`, the model emits
    // its chain-of-thought in a separate `message.thinking` field
    // alongside the final answer. Surface this as a boolean toggle
    // in the form so the user can opt in per-request.
    if let Some(ctx) = resolved {
        if p == Primitive::TextChat && ctx.has_capability("thinking") {
            params.push(SkillParameter {
                field: "text.reasoning.think".into(),
                required: false,
                label: Some("Show reasoning".into()),
                description: Some(
                    "Ask the model to emit its chain-of-thought before the final answer."
                        .into(),
                ),
                default: Some(json!(false)),
                field_type: Some(ParameterType::Boolean),
                widget: Some(ParameterWidget::Toggle),
                ..Default::default()
            });
        }
    }

    params
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
        .pointer("/text/prompt/history")
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

    // Attach an image to the current user turn when the resolver
    // has inlined `image.source` as base64 on the payload. This is
    // the ORCH-0038 vision-model overlay path — the form surfaces
    // `image.source` only when the resolved model has the `vision`
    // capability, so reaching this branch means the model can
    // accept images at `messages[].images[]` per Ollama's wire API.
    let mut user_msg = json!({"role": "user", "content": user});
    if let Some(image_b64) = payload
        .pointer("/image/source/base64")
        .and_then(|v| v.as_str())
    {
        if let Some(obj) = user_msg.as_object_mut() {
            obj.insert("images".into(), json!([image_b64]));
        }
    }
    messages.push(user_msg);

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
    /// Reasoning-model chain-of-thought. Populated when the request
    /// asked `think: true` AND the model supports it. Absent
    /// otherwise — we surface it under `text.reasoning` in the
    /// output when present.
    #[serde(default)]
    thinking: Option<String>,
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
    fn announcement_text_chat_superset_includes_image_source() {
        // Post-ORCH-0038 vision overlay: text.chat advertises
        // image.source in its announcement as part of the
        // SUPERSET (so the media resolver accepts the field).
        // The per-model subset shown in the UI form comes from
        // describe_workspace only when the resolved model has
        // the `vision` capability tag. text.embed still has no
        // media inputs.
        let ann = build_capability_announcement(
            &provider_name(),
            vec![Primitive::TextChat, Primitive::TextEmbed],
            true,
            &[],
        );
        assert_eq!(ann.capabilities.len(), 2);
        let chat = ann
            .capabilities
            .iter()
            .find(|c| c.primitive == Primitive::TextChat)
            .expect("text.chat capability");
        assert_eq!(chat.media_inputs.len(), 1);
        assert_eq!(chat.media_inputs[0].field, "image.source");

        let embed = ann
            .capabilities
            .iter()
            .find(|c| c.primitive == Primitive::TextEmbed)
            .expect("text.embed capability");
        assert!(embed.media_inputs.is_empty());
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
