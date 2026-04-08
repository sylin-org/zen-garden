//! ComfyUI provider — skill-driven dispatch via the ORCH-0029 model,
//! rewritten for the ORCH-0030 R2 M3 lean `Provider` trait.
//!
//! ComfyUI workflows are node graphs serialized as JSON. Each
//! workflow is a "skill" — a declarative binding from canonical
//! vocabulary fields to placeholders / node addresses inside the
//! workflow template, plus a list of `required_models` for the
//! provisioning subsystem.
//!
//! # M3 shape
//!
//! After M3 the `Provider` trait is lean: `name`, `onboard`,
//! `flush_caches`. The adapter publishes its capability set directly
//! to the bus as a [`CapabilityAnnouncement`] and owns its skill
//! state internally — the central `Skills` aggregate is gone.
//!
//! The ComfyUI adapter holds a private `HashMap<Moniker, LoadedSkill>`
//! behind a `RwLock`, publishes a fresh snapshot whenever its
//! instance pool or loaded-skill set changes, and reads the map on
//! every dispatch.
//!
//! ## Lifecycle
//!
//! 1. **Construction**: scan `{data_dir}/skills/comfyui/` via the
//!    shared `services::skills::loader`. Every `SkillDefinition`
//!    is converted to a private [`LoadedSkill`] and stashed in the
//!    provider's skill map. The initial capability announcement is
//!    published immediately with `enabled: false` — no instances yet.
//!
//! 2. **Discovery**: when ComfyUI instances surface via
//!    `garden_discovery`, the adapter updates its instance pool,
//!    runs the readiness fast path (submitting provisioning jobs for
//!    any missing models), and republishes the announcement with
//!    `enabled: true`.
//!
//! 3. **Dispatch (`onboard`)**: lookup the loaded skill by moniker,
//!    pick the workflow variant via `selectors.variant`, walk the
//!    bindings to populate the workflow JSON, upload media for image
//!    bindings, queue + poll + fetch from the picked instance,
//!    return a `ProviderOutcome::Sync(Output)` with `image.media_id`
//!    populated. **Zero per-skill branches.**

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::domain::capability_announcement::{
    Capability as AnnCapability, CapabilityAnnouncement, CapabilityMediaInput, SkillDeclaration,
    SkillDisplay, SkillParameter,
};
use crate::domain::events::EventBus;
use crate::domain::ids::ProviderName;
use crate::domain::keys;
use crate::domain::media::{MediaDelivery, MediaSource};
use crate::domain::moniker::Moniker;
use crate::domain::output::Output;
use crate::domain::primitive::Primitive;
use crate::domain::provider::{Provider, ProviderError, ProviderOutcome};
use crate::domain::request::OrchestratorRequest;
use crate::services::directory_subscriber::publish_capability_announcement;
use crate::services::garden_discovery::{DiscoveredInstance, GardenDiscovery};
use crate::services::skills::cache::{CachePaths, DependencyManifest};
use crate::services::skills::loader as skills_loader;
use crate::services::skills::moss_volume::{self, COMFYUI_MODELS_VOLUME};
use crate::services::skills::provisioner;
use crate::services::skills::queue::{Priority, ProvisioningQueue, ProvisioningTarget};
use crate::services::skills::types::{
    Binding, BindingTarget, ModelSelector, SkillDefinition, Variant,
};

use super::common::{
    build_http_client, check_status, map_reqwest_error, InstancePool, PerFqnInstances,
};

const FQNS: &[&'static str] = &["comfyui"];

/// How long to poll the ComfyUI history endpoint waiting for the
/// workflow to complete.
const POLL_BUDGET: Duration = Duration::from_secs(600);

#[derive(Debug, Clone)]
pub struct ComfyUiConfig {
    /// Absolute path to `{data_dir}/skills/comfyui/`.
    pub skills_dir: PathBuf,
    /// Absolute path to `{data_dir}` — the provisioner uses this to
    /// derive `{data_dir}/cache/dependencies/comfyui/` for the
    /// content-addressed cache.
    pub data_dir: PathBuf,
}

/// A skill loaded into the adapter's private state. Mirrors the
/// public `SkillDefinition` from the loader but only carries the bits
/// the executor + provisioner need.
#[derive(Debug, Clone)]
struct LoadedSkill {
    moniker: Moniker,
    primitive: Primitive,
    display_name: String,
    description: String,
    vram_mb: u64,
    workflows: HashMap<String, Value>,
    default_workflow: String,
    bindings: Vec<Binding>,
    model_selector: Option<ModelSelector>,
    /// Output node IDs to scan for the result. We accept all `SaveImage`
    /// node IDs at dispatch time so the per-skill `output_node` field
    /// (which the legacy schema sometimes omits) is not strictly required.
    output_node: Option<String>,
    /// Variants the catalog exposes via `selectors.variant`.
    variants: Option<Vec<Variant>>,
    /// Required model files — read by the provisioner's
    /// `ensure_cached` and `check_instance_readiness` helpers.
    required_models: Vec<crate::services::skills::types::ModelRef>,
}

impl LoadedSkill {
    /// Synthesize a `SkillDefinition` the provisioner can consume.
    /// The provisioner only reads `moniker`, `required_models`, and
    /// the primitive — everything else is empty.
    fn as_skill_definition(&self, moniker: &Moniker) -> SkillDefinition {
        SkillDefinition {
            moniker: moniker.clone(),
            display_name: self.display_name.clone(),
            primitive: self.primitive,
            description: self.description.clone(),
            vram_mb: self.vram_mb,
            default_workflow: self.default_workflow.clone(),
            workflows: HashMap::new(),
            bindings: Vec::new(),
            model_selector: None,
            variants: None,
            required_models: self.required_models.clone(),
            source: None,
            preview_url: None,
            output_node: None,
        }
    }
}

pub struct ComfyUiProvider {
    name: ProviderName,
    instances: Arc<InstancePool>,
    http: Client,
    /// Adapter-private skill state. Loaded at construction and
    /// re-loaded by hot-reload (Phase 4). Read on every dispatch.
    skills: Arc<tokio::sync::RwLock<HashMap<Moniker, LoadedSkill>>>,
    /// Event bus — the adapter publishes `CapabilityAnnouncement`
    /// events to the `directory.provider.comfyui.capabilities` topic
    /// whenever its instance pool or skill set changes.
    events: Arc<EventBus>,
    /// Shared provisioning queue — adapter submits jobs to this
    /// when discovery surfaces instances that don't have every
    /// required model yet. The worker loop drains it.
    provisioning: Arc<ProvisioningQueue>,
    /// Paths for the content-addressed dependency cache shared
    /// across all ComfyUI skills.
    cache_paths: CachePaths,
}

impl ComfyUiProvider {
    /// Load skills from disk and construct the provider.
    ///
    /// The instance pool starts empty; the discovery subscriber
    /// (spawned at the end of construction) populates it as
    /// `garden_discovery` emits ComfyUI events. Per-skill failures in
    /// the loader are logged and skipped — one bad `skill.json` never
    /// stops the rest of the registry from coming up.
    pub async fn new(
        config: ComfyUiConfig,
        provisioning: Arc<ProvisioningQueue>,
        discovery: Arc<GardenDiscovery>,
        events: Arc<EventBus>,
        shutdown: CancellationToken,
    ) -> Arc<Self> {
        let name = ProviderName::new(keys::providers::COMFYUI);
        let cache_paths = CachePaths::new(&config.data_dir, name.as_str());

        // Scan disk via the shared loader. The ComfyUI adapter
        // already knows it owns the `comfyui` provider directory, so
        // we use the per-provider entry point that scans
        // `{skills_dir}/{moniker}/skill.json` directly.
        let definitions = skills_loader::load_provider_skills(&config.skills_dir).await;
        tracing::info!(
            count = definitions.len(),
            dir = %config.skills_dir.display(),
            "comfyui: loaded skill definitions from disk"
        );

        // Convert every `SkillDefinition` into a private `LoadedSkill`.
        // There is no public Registration anymore — the adapter
        // announces its capabilities (and the skills that cover them)
        // directly on the bus via `publish_capabilities`.
        let mut skills_map: HashMap<Moniker, LoadedSkill> = HashMap::new();
        for def in definitions {
            let loaded = definition_to_loaded(def);
            skills_map.insert(loaded.moniker.clone(), loaded);
        }
        tracing::info!(
            loaded = skills_map.len(),
            "comfyui: constructed loaded-skill map"
        );

        let provider = Arc::new(Self {
            name,
            instances: Arc::new(InstancePool::new()),
            http: build_http_client(),
            skills: Arc::new(tokio::sync::RwLock::new(skills_map)),
            events,
            provisioning,
            cache_paths,
        });

        // Publish the initial snapshot. No instances yet, so
        // `enabled` will be false — but the Directory learns the
        // provider exists and sees its full skill list.
        provider.publish_capabilities().await;

        spawn_subscriber(provider.clone(), discovery, shutdown.clone());
        spawn_provisioning_worker(provider.clone(), shutdown);
        provider
    }

    fn pick(&self) -> Result<String, ProviderError> {
        self.instances.pick().ok_or_else(|| {
            ProviderError::Unreachable("no comfyui instances in the garden".to_string())
        })
    }

    /// Publish the current loaded-skill set + instance pool state as
    /// a `CapabilityAnnouncement` event. Called at construction and
    /// on every instance-pool / skill-set change.
    ///
    /// The announcement carries:
    /// - `enabled`: `true` iff at least one ComfyUI instance is in
    ///   the pool. False when no instances are reachable, even if
    ///   the skill list is non-empty.
    /// - `capabilities`: one entry per unique primitive any loaded
    ///   skill declares. Each capability's `media_inputs` is the
    ///   union of media bindings across every skill for that
    ///   primitive.
    /// - `skills`: the full `SkillDeclaration` list, in the shape
    ///   `compute_skill_declarations` produces.
    async fn publish_capabilities(&self) {
        let map = self.skills.read().await;
        let skills = compute_skill_declarations(&map);
        let capabilities = compute_capabilities(&map);
        drop(map);

        let enabled = !self.instances.is_empty() && !capabilities.is_empty();

        let announcement = CapabilityAnnouncement {
            provider: self.name.clone(),
            enabled,
            capabilities,
            skills,
        };
        publish_capability_announcement(&self.events, &announcement).await;
    }

    /// Replace the instance pool with the given URL list and publish
    /// a fresh capability announcement if the pool actually changed.
    async fn apply_merged(&self, urls: Vec<String>) {
        if !self.instances.set(urls) {
            return;
        }
        self.publish_capabilities().await;
    }

    /// Snapshot the adapter's currently-loaded skills as a list of
    /// `SkillDeclaration`s in the shape the `CapabilityDirectory`
    /// accepts. Used by tests and diagnostics.
    pub async fn skill_declarations(&self) -> Vec<SkillDeclaration> {
        let map = self.skills.read().await;
        compute_skill_declarations(&map)
    }

    /// Readiness fast path: for every loaded skill × every freshly-
    /// discovered instance, check whether all required models are
    /// present on that instance. If no, submit a provisioning job.
    ///
    /// Called from the discovery subscriber every time a
    /// `DiscoveryEvent` arrives.
    async fn readiness_pass(&self, instances: &[DiscoveredInstance]) {
        if instances.is_empty() {
            return;
        }
        let manifest = DependencyManifest::load(&self.cache_paths.manifest_path).await;
        let loaded = self.skills.read().await.clone();
        for (moniker, loaded_skill) in loaded.iter() {
            let def = loaded_skill.as_skill_definition(moniker);
            for instance in instances {
                let moss_endpoint = moss_volume::derive_moss_endpoint(&instance.url);
                // Fast path — HEAD every required model.
                let readiness = provisioner::check_instance_readiness(
                    &self.http,
                    &def,
                    &manifest,
                    &moss_endpoint,
                    self.name.as_str(),
                    COMFYUI_MODELS_VOLUME,
                )
                .await;
                if !readiness.ready {
                    // Submit the provisioning job. Returns false if
                    // the queue already has this target in flight
                    // (dedup), which is harmless.
                    let target = ProvisioningTarget {
                        skill: moniker.clone(),
                        endpoint: instance.url.clone(),
                    };
                    let submitted = self
                        .provisioning
                        .submit(
                            target,
                            Priority::Discovery,
                            instance.stone_name.clone(),
                            self.name.as_str().to_string(),
                        )
                        .await;
                    if submitted {
                        tracing::info!(
                            skill = moniker.as_str(),
                            endpoint = %instance.url,
                            reason = %readiness.reason,
                            "comfyui: queued provisioning job"
                        );
                    }
                }
            }
        }
    }

    /// Execute a single provisioning job: download any missing
    /// models into the local cache, then push cached files to the
    /// target instance. Marks the queue entry complete/failed on exit.
    async fn run_one_job(self: Arc<Self>, job: crate::services::skills::queue::ProvisioningJob) {
        use std::time::Instant;
        let started = Instant::now();
        let target = job.target.clone();
        let moniker = target.skill.clone();
        let endpoint = target.endpoint.clone();

        // Look up the loaded skill.
        let loaded_skill = {
            let map = self.skills.read().await;
            map.get(&moniker).cloned()
        };
        let Some(loaded_skill) = loaded_skill else {
            self.provisioning
                .fail(&target, format!("skill `{}` not loaded", moniker.as_str()))
                .await;
            return;
        };
        let def = loaded_skill.as_skill_definition(&moniker);

        let moss_endpoint = moss_volume::derive_moss_endpoint(&endpoint);
        let result = async {
            let cached = provisioner::ensure_cached(&self.http, &def, &self.cache_paths).await?;
            provisioner::push_to_instance(
                &self.http,
                &cached,
                &moss_endpoint,
                self.name.as_str(),
                COMFYUI_MODELS_VOLUME,
            )
            .await?;
            anyhow::Ok(())
        }
        .await;

        match result {
            Ok(()) => {
                self.provisioning
                    .complete(&target, started.elapsed())
                    .await;
                tracing::info!(
                    skill = moniker.as_str(),
                    endpoint = %endpoint,
                    duration_secs = started.elapsed().as_secs(),
                    "comfyui: provisioning complete"
                );
            }
            Err(e) => {
                let reason = format!("{e:#}");
                self.provisioning.fail(&target, reason.clone()).await;
                tracing::warn!(
                    skill = moniker.as_str(),
                    endpoint = %endpoint,
                    error = %reason,
                    "comfyui: provisioning failed (will retry after backoff)"
                );
            }
        }
    }
}

fn spawn_subscriber(
    provider: Arc<ComfyUiProvider>,
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
                    // Update the instance pool + republish capabilities.
                    let instances = event.instances.clone();
                    let urls: Vec<String> =
                        instances.iter().map(|i| i.url.clone()).collect();
                    pool.set(&event.fqn, urls);
                    provider.apply_merged(pool.flatten()).await;

                    // For every instance in this event, run the
                    // readiness fast path per skill and, if any
                    // model is missing, submit a provisioning job.
                    //
                    // This is the main entry point into the
                    // download-and-push pipeline. Happy path for
                    // the workspace's pre-populated cache: every
                    // required model is already present, readiness
                    // passes, no provisioning job is ever submitted.
                    let provider_for_check = provider.clone();
                    tokio::spawn(async move {
                        provider_for_check
                            .readiness_pass(&instances)
                            .await;
                    });
                }
            }
        }
    });
}

/// Background worker that drains the shared `ProvisioningQueue` and
/// runs `ensure_cached` → `push_to_instance` for each job.
///
/// Single writer per queue entry — the worker cap is owned by the
/// queue, not the worker; we spawn up to `max_concurrency` in-flight
/// tasks here.
fn spawn_provisioning_worker(provider: Arc<ComfyUiProvider>, shutdown: CancellationToken) {
    tokio::spawn(async move {
        let concurrency = provider.provisioning.max_concurrency();
        let sem = Arc::new(tokio::sync::Semaphore::new(concurrency));
        let notifier = provider.provisioning.notifier();
        tracing::info!(concurrency, "comfyui: provisioning worker started");

        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    provider.provisioning.drain().await;
                    tracing::info!("comfyui: provisioning worker shutting down");
                    return;
                }
                _ = notifier.notified() => {}
            }

            // Drain all currently-ready jobs up to the concurrency
            // cap. Each job gets its own spawned task that holds an
            // owned semaphore permit for the duration.
            loop {
                let job = match provider.provisioning.take_next().await {
                    Some(j) => j,
                    None => break,
                };
                let permit = match sem.clone().acquire_owned().await {
                    Ok(p) => p,
                    Err(_) => break,
                };
                let provider = provider.clone();
                tokio::spawn(async move {
                    let _permit = permit;
                    provider.run_one_job(job).await;
                });
            }
        }
    });
}

// ── Provider trait impl ──────────────────────────────────────

#[async_trait]
impl Provider for ComfyUiProvider {
    fn name(&self) -> ProviderName {
        self.name.clone()
    }

    async fn onboard(
        &self,
        request: OrchestratorRequest,
    ) -> Result<ProviderOutcome, ProviderError> {
        // ── 1. Resolve the skill ──────────────────────────────
        let skill_moniker = request.action.skill.as_ref().ok_or_else(|| {
            ProviderError::Unsupported(
                "comfyui requires a skill moniker; use `image.generate.<skill>`".to_string(),
            )
        })?;
        let skill = {
            let map = self.skills.read().await;
            map.get(skill_moniker).cloned()
        }
        .ok_or_else(|| {
            ProviderError::Unsupported(format!(
                "comfyui skill `{}.{}` not loaded",
                request.action.primitive.dotted(),
                skill_moniker
            ))
        })?;
        if skill.primitive != request.action.primitive {
            return Err(ProviderError::Unsupported(format!(
                "skill `{}` is registered for primitive `{}`, not `{}`",
                skill_moniker,
                skill.primitive.dotted(),
                request.action.primitive.dotted()
            )));
        }

        // ── 2. Pin an instance ────────────────────────────────
        //
        // A ComfyUI request must use the SAME instance for upload,
        // queue, history poll, and view — workflows reference uploaded
        // filenames that only exist on the instance they were uploaded
        // to.
        let instance = self.pick()?;
        let instance = instance.trim_end_matches('/').to_string();

        // ── 3. Pick the workflow variant ──────────────────────
        let variant_name = request
            .selectors
            .variant
            .clone()
            .unwrap_or_else(|| skill.default_workflow.clone());
        if let Some(variants) = &skill.variants {
            let known: Vec<&str> = variants.iter().map(|v| v.value.as_str()).collect();
            if !known.contains(&variant_name.as_str()) {
                return Err(ProviderError::Unsupported(format!(
                    "skill `{}` does not have variant `{}`; available: {:?}",
                    skill_moniker, variant_name, known
                )));
            }
        }
        let template = skill.workflows.get(&variant_name).ok_or_else(|| {
            ProviderError::Internal(format!(
                "workflow `{}` not loaded for skill `{}`",
                variant_name, skill_moniker
            ))
        })?;
        let mut workflow = template.clone();

        // ── 4. Walk bindings ──────────────────────────────────
        //
        // For each binding: pull the value from the request payload
        // (or fall back to the binding's skill-default), then apply
        // it to the workflow via the binding's target. Image bindings
        // are deferred to step 6 (they need an upload to the picked
        // instance first).
        let mut deferred_media_bindings: Vec<&Binding> = Vec::new();
        for binding in &skill.bindings {
            if is_media_binding(binding) {
                deferred_media_bindings.push(binding);
                continue;
            }
            let value = lookup_field(&request.payload, &binding.field)
                .or_else(|| binding.default.clone());
            if let Some(value) = value {
                apply_binding_target(&mut workflow, &binding.target, value);
            }
        }

        // ── 5. Resolve model_selector ─────────────────────────
        //
        // The caller picks a model via `selectors.model`; otherwise
        // the skill's default wins. Validate against the option set
        // when one is declared. Substitute the chosen filename into
        // the placeholder.
        if let Some(selector) = &skill.model_selector {
            let chosen = request
                .selectors
                .model
                .clone()
                .unwrap_or_else(|| selector.default.clone());
            if !selector.options.is_empty() {
                let allowed: Vec<&str> = selector
                    .options
                    .iter()
                    .filter_map(|o| o.value.as_str())
                    .collect();
                if !allowed.is_empty() && !allowed.contains(&chosen.as_str()) {
                    return Err(ProviderError::Unsupported(format!(
                        "skill `{}` model_selector does not include `{}`; available: {:?}",
                        skill_moniker, chosen, allowed
                    )));
                }
            }
            substitute_placeholder_in_workflow(
                &mut workflow,
                &selector.placeholder,
                &Value::String(chosen),
            );
        }

        // ── 6. Upload media for deferred image bindings ───────
        for binding in deferred_media_bindings {
            let media_ref = match request.media.find_at_field(&binding.field) {
                Some(r) => r,
                None => {
                    if binding.required {
                        return Err(ProviderError::Unsupported(format!(
                            "skill `{}` requires media at `{}`",
                            skill_moniker, binding.field
                        )));
                    }
                    continue;
                }
            };
            let bytes = request
                .context
                .media_store
                .get_bytes(&media_ref.id)
                .await
                .map_err(|e| ProviderError::Internal(format!("media fetch: {e}")))?;
            let meta = request
                .context
                .media_store
                .get_metadata(&media_ref.id)
                .await
                .map_err(|e| ProviderError::Internal(format!("media meta: {e}")))?;
            let filename = format!(
                "{}{}",
                media_ref.id,
                content_type_to_ext(&meta.content_type)
            );
            let part = reqwest::multipart::Part::bytes(bytes.to_vec())
                .file_name(filename.clone())
                .mime_str(&meta.content_type)
                .map_err(|e| ProviderError::Internal(format!("mime: {e}")))?;
            let form = reqwest::multipart::Form::new().part("image", part);
            let endpoint = format!("{}/upload/image", instance.as_str());
            let resp = self
                .http
                .post(&endpoint)
                .multipart(form)
                .send()
                .await
                .map_err(map_reqwest_error)?;
            let resp = check_status(resp, "comfyui upload").await?;
            let upload: UploadResponse = resp
                .json()
                .await
                .map_err(|e| ProviderError::Upstream(e.to_string()))?;
            // Substitute the uploaded filename into the binding's target.
            apply_binding_target(&mut workflow, &binding.target, Value::String(upload.name));
        }

        // ── 7. Queue the prompt ───────────────────────────────
        let queue_body = json!({"prompt": workflow});
        let endpoint = format!("{}/prompt", instance.as_str());
        let resp = self
            .http
            .post(&endpoint)
            .json(&queue_body)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let resp = check_status(resp, "comfyui prompt").await?;
        let queued: PromptResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::Upstream(e.to_string()))?;
        let prompt_id = queued.prompt_id;

        // ── 8. Poll history until the result lands ───────────
        let history_endpoint = format!("{}/history/{prompt_id}", instance.as_str());
        let mut output_filename: Option<String> = None;
        let mut output_subfolder: Option<String> = None;
        let mut output_type: Option<String> = None;
        let started = std::time::Instant::now();
        while started.elapsed() < POLL_BUDGET {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let resp = self
                .http
                .get(&history_endpoint)
                .send()
                .await
                .map_err(map_reqwest_error)?;
            if !resp.status().is_success() {
                continue;
            }
            let body: Value = resp
                .json()
                .await
                .map_err(|e| ProviderError::Upstream(e.to_string()))?;

            // Response shape: {"<prompt_id>": {"outputs": {"<node_id>": {"images": [...]}}}}
            let Some(history_entry) = body.get(&prompt_id) else {
                continue;
            };
            let Some(outputs) = history_entry.pointer("/outputs").and_then(|v| v.as_object())
            else {
                continue;
            };

            // Pick the output node:
            //   - If the skill declared one, prefer it.
            //   - Otherwise, take the first node whose output has an
            //     `images: [...]` array (typical SaveImage node).
            let chosen = if let Some(declared) = &skill.output_node {
                outputs.get(declared)
            } else {
                outputs.values().find(|v| {
                    v.get("images")
                        .and_then(|i| i.as_array())
                        .map(|a| !a.is_empty())
                        .unwrap_or(false)
                })
            };

            if let Some(node_output) = chosen {
                if let Some(images) = node_output
                    .get("images")
                    .and_then(|v| v.as_array())
                    .filter(|a| !a.is_empty())
                {
                    let first = &images[0];
                    output_filename =
                        first.get("filename").and_then(|v| v.as_str()).map(String::from);
                    output_subfolder =
                        first.get("subfolder").and_then(|v| v.as_str()).map(String::from);
                    output_type = first.get("type").and_then(|v| v.as_str()).map(String::from);
                    break;
                }
            }
        }

        let filename = output_filename.ok_or_else(|| {
            ProviderError::Timeout(format!(
                "comfyui workflow {} did not produce output in time",
                prompt_id
            ))
        })?;

        // ── 9. Fetch the output image ─────────────────────────
        let view_url = format!(
            "{}/view?filename={}&subfolder={}&type={}",
            instance.as_str(),
            urlencode(&filename),
            urlencode(output_subfolder.as_deref().unwrap_or("")),
            urlencode(output_type.as_deref().unwrap_or("output")),
        );
        let resp = self
            .http
            .get(&view_url)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let resp = check_status(resp, "comfyui view").await?;
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| ProviderError::Upstream(e.to_string()))?;

        let entry = request
            .context
            .media_store
            .put(
                bytes,
                content_type_for_filename(&filename),
                MediaSource::generated(
                    self.name.clone(),
                    request.action.dotted(),
                    request.id.clone(),
                ),
            )
            .await
            .map_err(|e| ProviderError::Internal(format!("media store: {e}")))?;

        // ── 10. Build the canonical Output ────────────────────
        //
        // The output key depends on the primitive: image.* uses
        // image.media_id, audio.* uses audio.media_id. image.analyze
        // (vision.tag legacy skill) returns text via text.response.
        let mut out = Output::new();
        match skill.primitive {
            Primitive::ImageGenerate | Primitive::ImageEdit | Primitive::ImageUpscale => {
                out.set(&keys::image::MEDIA_ID, entry.id.as_str());
            }
            Primitive::ImageAnalyze => {
                // Currently the legacy `tag` skill emits a SaveImage
                // node carrying tag text in its filename or in a
                // companion text node. For Phase 1 we surface the
                // media_id under text.response — Phase 2 will add
                // proper output extraction once Docling-style text
                // outputs ship.
                out.set(&keys::text::RESPONSE, entry.id.as_str());
            }
            Primitive::AudioGenerate => {
                out.set(&keys::audio::MEDIA_ID, entry.id.as_str());
            }
            _ => {
                out.set(&keys::image::MEDIA_ID, entry.id.as_str());
            }
        }
        Ok(ProviderOutcome::Sync(out))
    }
}

// ── Definition conversion ─────────────────────────────────────

/// Strip a loaded `SkillDefinition` down to the private `LoadedSkill`
/// the adapter keeps in its skill map. Every field the `onboard`
/// dispatch logic needs is copied; metadata-only fields (`source`,
/// `preview_url`) are dropped because the adapter no longer feeds a
/// public catalog structure directly — the catalog builder renders
/// from the `CapabilityAnnouncement` the adapter publishes.
fn definition_to_loaded(def: SkillDefinition) -> LoadedSkill {
    LoadedSkill {
        moniker: def.moniker,
        primitive: def.primitive,
        display_name: def.display_name,
        description: def.description,
        vram_mb: def.vram_mb,
        workflows: def.workflows,
        default_workflow: def.default_workflow,
        bindings: def.bindings,
        model_selector: def.model_selector,
        output_node: def.output_node,
        variants: def.variants,
        required_models: def.required_models,
    }
}

/// A binding is a media binding when it has a `delivery` mode set
/// (which the loader only sets for content slots whose role is
/// image/audio source/mask).
fn is_media_binding(binding: &Binding) -> bool {
    binding.delivery.is_some() || !binding.accepted_types.is_empty()
}

// ── Workflow manipulation ─────────────────────────────────────

/// Pull a value out of the request payload by canonical field path.
///
/// `image.prompt.positive` becomes the JSON pointer
/// `/image/prompt/positive`.
fn lookup_field(payload: &Value, field: &crate::domain::field_path::FieldPath) -> Option<Value> {
    let pointer = format!("/{}", field.as_str().replace('.', "/"));
    payload.pointer(&pointer).cloned()
}

/// Apply a binding's target to the workflow.
fn apply_binding_target(workflow: &mut Value, target: &BindingTarget, value: Value) {
    match target {
        BindingTarget::Placeholder(placeholder) => {
            substitute_placeholder_in_workflow(workflow, placeholder, &value);
        }
        BindingTarget::NodeInput { node, input } => {
            if let Some(slot) = workflow
                .get_mut(node)
                .and_then(|n| n.get_mut("inputs"))
                .and_then(|i| i.get_mut(input))
            {
                *slot = value;
            }
        }
    }
}

/// Recursively replace every occurrence of `"placeholder"` (as a
/// JSON string value) anywhere in the workflow tree with `value`.
fn substitute_placeholder_in_workflow(workflow: &mut Value, placeholder: &str, value: &Value) {
    match workflow {
        Value::Object(map) => {
            for v in map.values_mut() {
                substitute_placeholder_in_workflow(v, placeholder, value);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                substitute_placeholder_in_workflow(v, placeholder, value);
            }
        }
        Value::String(s) if s == placeholder => {
            *workflow = value.clone();
        }
        _ => {}
    }
}

// ── Utilities ─────────────────────────────────────────────────

fn content_type_to_ext(ct: &str) -> &'static str {
    match ct {
        "image/png" => ".png",
        "image/jpeg" => ".jpg",
        "image/webp" => ".webp",
        "audio/mpeg" => ".mp3",
        "audio/wav" => ".wav",
        _ => ".bin",
    }
}

fn content_type_for_filename(filename: &str) -> String {
    let lower = filename.to_lowercase();
    if lower.ends_with(".png") {
        "image/png".to_string()
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg".to_string()
    } else if lower.ends_with(".webp") {
        "image/webp".to_string()
    } else if lower.ends_with(".mp3") {
        "audio/mpeg".to_string()
    } else if lower.ends_with(".wav") {
        "audio/wav".to_string()
    } else {
        "application/octet-stream".to_string()
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
            out.push(c);
        } else {
            for byte in c.to_string().bytes() {
                out.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    out
}

// ── Capability + SkillDeclaration conversion (ORCH-0030 R2 M3) ─

/// Walk a `LoadedSkill` map and produce the list of `Capability`
/// entries for the provider's announcement.
///
/// One capability entry per unique primitive any loaded skill
/// declares. Each entry's `media_inputs` list is the union of every
/// media binding whose owning skill targets that primitive —
/// deduplicated by `field` so two skills asking for `image.source`
/// don't produce duplicate entries. Accepted types are unioned.
///
/// Output is sorted by primitive dotted name for stable
/// announcements (the `DirectorySubscriber` diffs by primitive, so
/// a stable ordering keeps derived events clean).
fn compute_capabilities(skills: &HashMap<Moniker, LoadedSkill>) -> Vec<AnnCapability> {
    // Group media bindings by (primitive, field) so two skills that
    // both accept `image.source` for `image.generate` collapse to a
    // single entry with the union of accepted types.
    let mut by_primitive: HashMap<Primitive, HashMap<String, CapabilityMediaInput>> =
        HashMap::new();

    for loaded in skills.values() {
        let entry = by_primitive.entry(loaded.primitive).or_default();
        for binding in &loaded.bindings {
            if !is_media_binding(binding) {
                continue;
            }
            let field_key = binding.field.as_str().to_string();
            let delivery = binding.delivery.unwrap_or(MediaDelivery::Transfer);
            match entry.get_mut(&field_key) {
                Some(existing) => {
                    // Union accepted types while preserving insertion
                    // order of the first occurrence.
                    let mut seen: HashSet<String> =
                        existing.accepted_types.iter().cloned().collect();
                    for ty in &binding.accepted_types {
                        if seen.insert(ty.clone()) {
                            existing.accepted_types.push(ty.clone());
                        }
                    }
                    if existing.overlay.is_none() {
                        existing.overlay = binding.overlay.clone();
                    }
                }
                None => {
                    entry.insert(
                        field_key.clone(),
                        CapabilityMediaInput {
                            field: field_key,
                            delivery,
                            accepted_types: binding.accepted_types.clone(),
                            overlay: binding.overlay.clone(),
                        },
                    );
                }
            }
        }
    }

    let mut capabilities: Vec<AnnCapability> = by_primitive
        .into_iter()
        .map(|(primitive, media_map)| {
            let mut media_inputs: Vec<CapabilityMediaInput> = media_map.into_values().collect();
            // Stable order: sort by field name.
            media_inputs.sort_by(|a, b| a.field.cmp(&b.field));
            AnnCapability {
                primitive,
                media_inputs,
            }
        })
        .collect();
    capabilities.sort_by(|a, b| a.primitive.dotted().cmp(b.primitive.dotted()));
    capabilities
}

/// Walk a `LoadedSkill` map and produce one `SkillDeclaration` per
/// entry. The output ordering follows the underlying `HashMap`
/// iteration order — callers that care must sort by `id`.
fn compute_skill_declarations(
    skills: &HashMap<Moniker, LoadedSkill>,
) -> Vec<SkillDeclaration> {
    skills.values().map(loaded_to_skill_declaration).collect()
}

/// Convert a single `LoadedSkill` to a `SkillDeclaration`.
///
/// Field mapping:
/// - `id` ← `loaded.moniker.as_str()`
/// - `primitive` ← `loaded.primitive`
/// - `display.name` ← `loaded.display_name`
/// - `display.description` ← `loaded.description` (omitted when empty)
/// - `parameters` ← one entry per `Binding` (canonical or media);
///   plus a `selectors.model` entry when the skill has a model
///   selector; plus a `selectors.variant` entry when the skill
///   declares variants.
///
/// Media bindings become non-pinnable parameters (the caller must
/// supply the media reference; they cannot pin a literal value).
/// Non-media bindings are pinnable. The `auto` field is always
/// `None` for skills — auto-resolution is a primitive-level
/// concern, not a skill-level one.
fn loaded_to_skill_declaration(loaded: &LoadedSkill) -> SkillDeclaration {
    let mut parameters: Vec<SkillParameter> = Vec::with_capacity(loaded.bindings.len() + 2);

    for binding in &loaded.bindings {
        let is_media = is_media_binding(binding);
        parameters.push(SkillParameter {
            field: binding.field.as_str().to_string(),
            required: binding.required || is_media,
            description: binding.label.clone(),
            default: binding.default.clone(),
            auto: None,
            pinnable: !is_media,
        });
    }

    if let Some(selector) = &loaded.model_selector {
        parameters.push(SkillParameter {
            field: "selectors.model".to_string(),
            required: false,
            description: Some("Model used by this skill.".to_string()),
            default: Some(serde_json::Value::String(selector.default.clone())),
            auto: None,
            pinnable: true,
        });
    }

    if let Some(variants) = &loaded.variants {
        if !variants.is_empty() {
            parameters.push(SkillParameter {
                field: "selectors.variant".to_string(),
                required: false,
                description: Some("Workflow variant.".to_string()),
                default: variants
                    .first()
                    .map(|v| serde_json::Value::String(v.value.clone())),
                auto: None,
                pinnable: true,
            });
        }
    }

    let mut display = SkillDisplay::new(loaded.display_name.clone());
    if !loaded.description.is_empty() {
        display = display.with_description(loaded.description.clone());
    }

    SkillDeclaration {
        id: loaded.moniker.as_str().to_string(),
        primitive: loaded.primitive,
        display,
        parameters,
    }
}

// ── Wire types ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct UploadResponse {
    name: String,
}

#[derive(Debug, Deserialize)]
struct PromptResponse {
    prompt_id: String,
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::field_path::FieldPath;
    use crate::domain::media::MediaDelivery;
    use crate::services::skills::types::{ModelSelector, ParamOption, Variant};

    fn moniker(s: &str) -> Moniker {
        Moniker::new(s).expect("valid moniker")
    }

    fn field(s: &str) -> FieldPath {
        FieldPath::parse(s).expect("valid field path")
    }

    fn param_binding(field_str: &str, label: &str, required: bool) -> Binding {
        Binding {
            field: field(field_str),
            target: BindingTarget::Placeholder("PH".to_string()),
            default: None,
            narrow: None,
            label: Some(label.to_string()),
            required,
            delivery: None,
            accepted_types: Vec::new(),
            overlay: None,
            self_described_type: None,
        }
    }

    fn media_binding(field_str: &str, accepted: &[&str]) -> Binding {
        Binding {
            field: field(field_str),
            target: BindingTarget::Placeholder("PH".to_string()),
            default: None,
            narrow: None,
            label: None,
            required: false,
            delivery: Some(MediaDelivery::Transfer),
            accepted_types: accepted.iter().map(|s| s.to_string()).collect(),
            overlay: None,
            self_described_type: None,
        }
    }

    fn synthetic_skill(
        id: &str,
        primitive: Primitive,
        bindings: Vec<Binding>,
        model_selector: Option<ModelSelector>,
        variants: Option<Vec<Variant>>,
    ) -> (Moniker, LoadedSkill) {
        let m = moniker(id);
        let loaded = LoadedSkill {
            moniker: m.clone(),
            primitive,
            display_name: format!("Synthetic {id}"),
            description: format!("A test skill named {id}."),
            vram_mb: 0,
            workflows: HashMap::new(),
            default_workflow: "default".to_string(),
            bindings,
            model_selector,
            output_node: None,
            variants,
            required_models: Vec::new(),
        };
        (m, loaded)
    }

    #[test]
    fn empty_skill_map_yields_empty_declarations() {
        let map: HashMap<Moniker, LoadedSkill> = HashMap::new();
        let result = compute_skill_declarations(&map);
        assert!(result.is_empty(), "expected zero declarations, got {result:?}");
    }

    #[test]
    fn two_synthetic_skills_become_two_declarations_with_correct_fields() {
        let (m_a, loaded_a) = synthetic_skill(
            "skill-a",
            Primitive::ImageGenerate,
            vec![
                param_binding("image.prompt.positive", "Positive prompt", true),
                media_binding("image.source", &["image/png"]),
            ],
            Some(ModelSelector {
                placeholder: "MODEL".to_string(),
                default: "sdxl_base.safetensors".to_string(),
                options: vec![ParamOption {
                    value: serde_json::Value::String("sdxl_base.safetensors".to_string()),
                    label: Some("SDXL Base".to_string()),
                }],
            }),
            None,
        );
        let (m_b, loaded_b) = synthetic_skill(
            "skill-b",
            Primitive::ImageUpscale,
            vec![param_binding("image.upscale.factor", "Upscale factor", false)],
            None,
            Some(vec![
                Variant {
                    value: "fast".to_string(),
                    label: Some("Fast".to_string()),
                },
                Variant {
                    value: "high".to_string(),
                    label: Some("High Quality".to_string()),
                },
            ]),
        );

        let mut map: HashMap<Moniker, LoadedSkill> = HashMap::new();
        map.insert(m_a, loaded_a);
        map.insert(m_b, loaded_b);

        let mut result = compute_skill_declarations(&map);
        assert_eq!(result.len(), 2);
        result.sort_by(|x, y| x.id.cmp(&y.id));

        // ── skill-a (image.generate, model selector, two bindings)
        let a = &result[0];
        assert_eq!(a.id, "skill-a");
        assert_eq!(a.primitive, Primitive::ImageGenerate);
        assert_eq!(a.display.name, "Synthetic skill-a");
        assert_eq!(
            a.display.description.as_deref(),
            Some("A test skill named skill-a.")
        );
        // Two bindings + one selectors.model parameter.
        assert_eq!(a.parameters.len(), 3);

        // The canonical text param is required + pinnable.
        let positive = a
            .parameters
            .iter()
            .find(|p| p.field == "image.prompt.positive")
            .expect("positive prompt parameter missing");
        assert!(positive.required);
        assert!(positive.pinnable);
        assert_eq!(positive.description.as_deref(), Some("Positive prompt"));

        // The media binding is required (callers must always supply
        // media inputs) but NOT pinnable (no literal value pinning).
        let source = a
            .parameters
            .iter()
            .find(|p| p.field == "image.source")
            .expect("image.source parameter missing");
        assert!(source.required);
        assert!(!source.pinnable);

        // The synthesized selectors.model parameter carries the
        // selector's default value.
        let model_param = a
            .parameters
            .iter()
            .find(|p| p.field == "selectors.model")
            .expect("selectors.model parameter missing");
        assert!(!model_param.required);
        assert!(model_param.pinnable);
        assert_eq!(
            model_param.default.as_ref().and_then(|v| v.as_str()),
            Some("sdxl_base.safetensors")
        );

        // ── skill-b (image.upscale, variants, one binding)
        let b = &result[1];
        assert_eq!(b.id, "skill-b");
        assert_eq!(b.primitive, Primitive::ImageUpscale);
        // One binding + one selectors.variant parameter (no model selector).
        assert_eq!(b.parameters.len(), 2);

        let factor = b
            .parameters
            .iter()
            .find(|p| p.field == "image.upscale.factor")
            .expect("upscale.factor parameter missing");
        assert!(!factor.required);
        assert!(factor.pinnable);

        // The synthesized selectors.variant parameter defaults to the
        // first variant's value.
        let variant_param = b
            .parameters
            .iter()
            .find(|p| p.field == "selectors.variant")
            .expect("selectors.variant parameter missing");
        assert_eq!(
            variant_param.default.as_ref().and_then(|v| v.as_str()),
            Some("fast")
        );
    }

    #[test]
    fn empty_skill_map_yields_empty_capabilities() {
        let map: HashMap<Moniker, LoadedSkill> = HashMap::new();
        let caps = compute_capabilities(&map);
        assert!(caps.is_empty());
    }

    #[test]
    fn capabilities_group_by_primitive_and_union_media_inputs() {
        // Two skills on image.generate, both with an image.source
        // media binding (different accepted types) → one capability
        // with unioned accepted_types.
        let (m_a, loaded_a) = synthetic_skill(
            "skill-a",
            Primitive::ImageGenerate,
            vec![media_binding("image.source", &["image/png"])],
            None,
            None,
        );
        let (m_b, loaded_b) = synthetic_skill(
            "skill-b",
            Primitive::ImageGenerate,
            vec![media_binding("image.source", &["image/jpeg", "image/png"])],
            None,
            None,
        );
        // A third skill on image.upscale with a different media
        // field → separate capability entry.
        let (m_c, loaded_c) = synthetic_skill(
            "skill-c",
            Primitive::ImageUpscale,
            vec![media_binding("image.source", &["image/webp"])],
            None,
            None,
        );

        let mut map: HashMap<Moniker, LoadedSkill> = HashMap::new();
        map.insert(m_a, loaded_a);
        map.insert(m_b, loaded_b);
        map.insert(m_c, loaded_c);

        let caps = compute_capabilities(&map);
        assert_eq!(caps.len(), 2);

        // image.generate carries the unioned types.
        let generate_cap = caps
            .iter()
            .find(|c| c.primitive == Primitive::ImageGenerate)
            .expect("image.generate capability missing");
        assert_eq!(generate_cap.media_inputs.len(), 1);
        let media = &generate_cap.media_inputs[0];
        assert_eq!(media.field, "image.source");
        assert!(media.accepted_types.contains(&"image/png".to_string()));
        assert!(media.accepted_types.contains(&"image/jpeg".to_string()));

        // image.upscale carries just its one entry.
        let up = caps
            .iter()
            .find(|c| c.primitive == Primitive::ImageUpscale)
            .expect("image.upscale capability missing");
        assert_eq!(up.media_inputs.len(), 1);
        assert_eq!(up.media_inputs[0].accepted_types, vec!["image/webp"]);
    }

    #[test]
    fn capabilities_include_skills_with_no_media_bindings() {
        // A skill with only canonical (non-media) bindings still
        // contributes a capability entry — just with an empty
        // `media_inputs` list. Needed so the dispatcher can route
        // primitives whose skills don't take media inputs.
        let (m, loaded) = synthetic_skill(
            "skill-only-text",
            Primitive::ImageGenerate,
            vec![param_binding("image.prompt.positive", "Prompt", true)],
            None,
            None,
        );

        let mut map: HashMap<Moniker, LoadedSkill> = HashMap::new();
        map.insert(m, loaded);

        let caps = compute_capabilities(&map);
        assert_eq!(caps.len(), 1);
        assert_eq!(caps[0].primitive, Primitive::ImageGenerate);
        assert!(caps[0].media_inputs.is_empty());
    }
}
