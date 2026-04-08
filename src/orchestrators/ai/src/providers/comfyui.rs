//! ComfyUI provider — skill-driven dispatch via the ORCH-0029 model.
//!
//! ComfyUI workflows are node graphs serialized as JSON. Each
//! workflow is a "skill" — a declarative binding from canonical
//! vocabulary fields to placeholders / node addresses inside the
//! workflow template, plus a list of `required_models` for the
//! provisioning subsystem.
//!
//! ## Lifecycle (ORCH-0029 §The ComfyUI adapter — owner of the lifecycle)
//!
//! 1. **Construction**: scan `{data_dir}/skills/comfyui/` via the
//!    shared `services::skills::loader`. Each `SkillDefinition` is
//!    split into:
//!    - **Public Registration** — pushed to the Directory via
//!      `ProviderState`. Carries `HonoredField` entries derived from
//!      the skill's `Binding`s, with constraint/default/label
//!      overlays. The catalog renders forms by joining vocabulary +
//!      these overlays.
//!    - **Private `LoadedSkill`** — kept in `Arc<RwLock<HashMap>>`
//!      inside the adapter. Carries the workflow JSON files
//!      (potentially multiple variants), the model selector, the
//!      output node, and the required model list. Never leaves the
//!      provider.
//!    - **`SkillMeta`** — pushed to the `Skills` aggregate on
//!      `AppState` so the catalog can render the skill's variants,
//!      model selector, source/preview, and per-instance readiness
//!      alongside its schema.
//!
//! 2. **Discovery**: when a ComfyUI instance comes up via
//!    `garden_discovery`, the adapter updates its instance pool,
//!    re-publishes provider state with `Healthy` health, and (in
//!    Phase 2) submits provisioning jobs for any missing models.
//!
//! 3. **Dispatch (`onboard`)**: lookup the loaded skill by moniker,
//!    pick the workflow variant via `selectors.variant`, walk the
//!    bindings to populate the workflow JSON, upload media for image
//!    bindings, queue + poll + fetch from the picked instance,
//!    return a `ProviderOutcome::Sync(Output)` with `image.media_id`
//!    populated. **Zero per-skill branches.**

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::domain::capability_announcement::{
    SkillDeclaration, SkillDisplay, SkillParameter,
};
use crate::domain::ids::{ProviderName, RegistrationId};
use crate::domain::keys;
use crate::domain::media::MediaSource;
use crate::domain::moniker::Moniker;
use crate::domain::output::Output;
use crate::domain::primitive::Primitive;
use crate::domain::provider::{
    HonoredField, MediaInputSpec, MediaOutputSpec, Provider, ProviderError, ProviderHealth,
    ProviderOutcome, ProviderState, ProviderStatePublisher, Registration, RegistrationStrategy,
};
use crate::domain::request::OrchestratorRequest;
use crate::services::garden_discovery::{DiscoveredInstance, GardenDiscovery};
use crate::services::skills::cache::{CachePaths, DependencyManifest};
use crate::services::skills::moss_volume::{self, COMFYUI_MODELS_VOLUME};
use crate::services::skills::provisioner;
use crate::services::skills::queue::{Priority, ProvisioningQueue, ProvisioningTarget};
use crate::services::skills::registry::{
    InstanceReadiness as SkillReadiness, SkillKey, SkillMeta, Skills,
};
use crate::services::skills::types::{
    Binding, BindingTarget, ModelSelector, SkillDefinition, Variant,
};
use crate::services::skills::{loader as skills_loader};

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
    /// Used by the provisioning worker (Phase 2) to look up the
    /// matching `Skills` aggregate entry.
    #[allow(dead_code)]
    fn key(&self, provider: &ProviderName) -> SkillKey {
        SkillKey::new(provider.clone(), self.moniker.clone())
    }

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
    publisher: ProviderStatePublisher,
    /// Adapter-private skill state. Loaded at construction and
    /// re-loaded by hot-reload (Phase 4). Read on every dispatch.
    skills: Arc<tokio::sync::RwLock<HashMap<Moniker, LoadedSkill>>>,
    /// Cached list of registrations published to the Directory. We
    /// keep them so health changes can re-publish the same set
    /// without rebuilding the bindings.
    initial_registrations: Vec<Registration>,
    /// Shared `Skills` aggregate — adapter writes registration
    /// metadata at load time and per-instance readiness as
    /// provisioning progresses.
    skills_aggregate: Arc<Skills>,
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
        skills_aggregate: Arc<Skills>,
        provisioning: Arc<ProvisioningQueue>,
        discovery: Arc<GardenDiscovery>,
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

        // Split each definition into:
        //   - public Registration (Directory) + SkillMeta (Skills aggregate)
        //   - private LoadedSkill (workflow files + binding plan)
        let mut skills_map: HashMap<Moniker, LoadedSkill> = HashMap::new();
        let mut registrations: Vec<Registration> = Vec::new();
        for def in definitions {
            let (registration, loaded, meta) = match split_definition(&name, def) {
                Ok(triple) => triple,
                Err(e) => {
                    tracing::warn!(error = %e, "comfyui: skipping skill that failed to split");
                    continue;
                }
            };
            // Publish to Skills aggregate immediately — the adapter is
            // the writer, the catalog is the reader.
            skills_aggregate.register(meta).await;

            registrations.push(registration);
            skills_map.insert(loaded.moniker.clone(), loaded);
        }
        tracing::info!(
            registered = registrations.len(),
            "comfyui: published registrations to Directory"
        );

        let initial = ProviderState {
            health: ProviderHealth::Offline {
                reason: "no garden instances discovered yet".to_string(),
            },
            registrations: registrations.clone(),
            models: Vec::new(),
            performance_hints: Vec::new(),
        };

        let provider = Arc::new(Self {
            name,
            instances: Arc::new(InstancePool::new()),
            http: build_http_client(),
            publisher: ProviderStatePublisher::new(initial),
            skills: Arc::new(tokio::sync::RwLock::new(skills_map)),
            initial_registrations: registrations,
            skills_aggregate,
            provisioning,
            cache_paths,
        });
        spawn_subscriber(provider.clone(), discovery, shutdown.clone());
        spawn_provisioning_worker(provider.clone(), shutdown);
        provider
    }

    fn pick(&self) -> Result<String, ProviderError> {
        self.instances.pick().ok_or_else(|| {
            ProviderError::Unreachable("no comfyui instances in the garden".to_string())
        })
    }

    fn apply_merged(&self, urls: Vec<String>) {
        if !self.instances.set(urls) {
            return;
        }
        let count = self.instances.len();
        let registrations = self.initial_registrations.clone();
        self.publisher.modify(move |mut state| {
            state.health = if count == 0 {
                ProviderHealth::Offline {
                    reason: "no garden instances discovered".to_string(),
                }
            } else {
                ProviderHealth::Healthy
            };
            state.registrations = registrations;
            state
        });
    }

    /// Snapshot the adapter's currently-loaded skills as a list of
    /// `SkillDeclaration`s in the shape the post-M3 `CapabilityDirectory`
    /// will accept.
    ///
    /// **Status (ORCH-0030 R2 M2):** This method is additive and
    /// currently unused outside its own unit tests. It exists so the
    /// M3 trait switch can wire ComfyUI's capability publication
    /// path with no new conversion code or risk. Once M3 lands, the
    /// adapter will publish a `CapabilityAnnouncement` event whose
    /// `skills` field is exactly this vector, and delete the legacy
    /// `Skills` aggregate path.
    ///
    /// The conversion lives in the free function
    /// [`compute_skill_declarations`] so the unit tests can exercise
    /// it without constructing a full provider.
    pub async fn skill_declarations(&self) -> Vec<SkillDeclaration> {
        let map = self.skills.read().await;
        compute_skill_declarations(&map)
    }

    /// Readiness fast path: for every loaded skill × every freshly-
    /// discovered instance, check whether all required models are
    /// present on that instance. If yes, publish readiness to the
    /// Skills aggregate. If no, submit a provisioning job.
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
                let key = SkillKey::new(self.name.clone(), moniker.clone());
                self.skills_aggregate
                    .set_readiness(
                        &key,
                        SkillReadiness {
                            stone_name: instance.stone_name.clone(),
                            endpoint: instance.url.clone(),
                            ready: readiness.ready,
                            reason: readiness.reason.clone(),
                            vram_mb: 0,
                        },
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
    /// target instance. Updates `Skills.set_readiness` and marks
    /// the queue entry complete/failed on exit.
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

        let key = SkillKey::new(self.name.clone(), moniker.clone());
        match result {
            Ok(()) => {
                self.provisioning
                    .complete(&target, started.elapsed())
                    .await;
                self.skills_aggregate
                    .set_readiness(
                        &key,
                        SkillReadiness {
                            stone_name: job.stone_name.clone(),
                            endpoint: endpoint.clone(),
                            ready: true,
                            reason: "provisioned".into(),
                            vram_mb: 0,
                        },
                    )
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
                self.skills_aggregate
                    .set_readiness(
                        &key,
                        SkillReadiness {
                            stone_name: job.stone_name.clone(),
                            endpoint: endpoint.clone(),
                            ready: false,
                            reason: format!("provisioning failed: {reason}"),
                            vram_mb: 0,
                        },
                    )
                    .await;
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
                    // Update the instance pool + publisher health.
                    let instances = event.instances.clone();
                    let urls: Vec<String> =
                        instances.iter().map(|i| i.url.clone()).collect();
                    pool.set(&event.fqn, urls);
                    provider.apply_merged(pool.flatten());

                    // For every instance in this event, run the
                    // readiness fast path per skill and, if any
                    // model is missing, submit a provisioning job.
                    //
                    // This is the main entry point into Phase 2's
                    // download-and-push pipeline. Happy path for
                    // the workspace's 90 GB cache: every required
                    // model is already present, readiness passes,
                    // no provisioning job is ever submitted.
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
/// runs `ensure_cached` → `push_to_instance` for each job, updating
/// the `Skills` aggregate with per-instance readiness on success.
///
/// Single writer (`set_readiness`, `complete`, `fail`) per
/// ORCH-0028 §6. The concurrency cap is owned by the queue, not the
/// worker — we spawn up to `max_concurrency` in-flight tasks.
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

#[async_trait]
impl Provider for ComfyUiProvider {
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
        // are deferred to step 5 (they need an upload to the picked
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
            substitute_placeholder_in_workflow(&mut workflow, &selector.placeholder, &Value::String(chosen));
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
            let Some(outputs) = history_entry.pointer("/outputs").and_then(|v| v.as_object()) else {
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
                    output_filename = first.get("filename").and_then(|v| v.as_str()).map(String::from);
                    output_subfolder = first.get("subfolder").and_then(|v| v.as_str()).map(String::from);
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

// ── Definition splitting ──────────────────────────────────────

/// Split a loaded `SkillDefinition` into the three things the
/// orchestrator needs:
///
/// 1. **Public Registration** for the Directory.
/// 2. **Private LoadedSkill** for the adapter's `onboard`.
/// 3. **SkillMeta** for the Skills aggregate.
fn split_definition(
    provider: &ProviderName,
    def: SkillDefinition,
) -> Result<(Registration, LoadedSkill, SkillMeta), String> {
    // Build HonoredField + MediaInputSpec lists from the bindings.
    let mut honored_fields: Vec<HonoredField> = Vec::new();
    let mut media_inputs: Vec<MediaInputSpec> = Vec::new();
    for binding in &def.bindings {
        if is_media_binding(binding) {
            media_inputs.push(MediaInputSpec {
                field: binding.field.clone(),
                delivery: binding
                    .delivery
                    .unwrap_or(crate::domain::media::MediaDelivery::Transfer),
                accepted_types: binding.accepted_types.clone(),
                overlay: binding.overlay.clone(),
            });
            honored_fields.push(
                HonoredField::new(binding.field.clone())
                    .with_label(binding.label.clone().unwrap_or_default()),
            );
        } else {
            let mut hf = HonoredField::new(binding.field.clone());
            if binding.required {
                hf = hf.required();
            }
            if let Some(label) = &binding.label {
                hf = hf.with_label(label.clone());
            }
            if let Some(default) = &binding.default {
                hf = hf.with_default(default.clone());
            }
            if let Some(narrow) = &binding.narrow {
                hf = hf.with_constraint(narrow.clone());
            }
            honored_fields.push(hf);
        }
    }

    // The output spec depends on the primitive.
    let media_outputs = match def.primitive {
        Primitive::ImageGenerate | Primitive::ImageEdit | Primitive::ImageUpscale => {
            vec![MediaOutputSpec {
                field: keys::image::MEDIA_ID,
                content_type: "image/png".to_string(),
            }]
        }
        Primitive::AudioGenerate => vec![MediaOutputSpec {
            field: keys::audio::MEDIA_ID,
            content_type: "audio/mpeg".to_string(),
        }],
        _ => Vec::new(),
    };

    let registration = Registration {
        id: RegistrationId::generate(),
        provider: provider.clone(),
        primitive: def.primitive,
        strategy: RegistrationStrategy::Skill {
            moniker: def.moniker.clone(),
            display_name: def.display_name.clone(),
            description: if def.description.is_empty() {
                None
            } else {
                Some(def.description.clone())
            },
        },
        honored_fields,
        media_inputs,
        media_outputs,
    };

    let meta = SkillMeta {
        provider: provider.clone(),
        moniker: def.moniker.clone(),
        primitive: def.primitive,
        display_name: def.display_name.clone(),
        description: def.description.clone(),
        vram_mb: def.vram_mb,
        variants: def.variants.clone(),
        model_selector: def.model_selector.clone(),
        required_models: def.required_models.clone(),
        source: def.source.clone(),
        preview_url: def.preview_url.clone(),
    };

    let loaded = LoadedSkill {
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
    };

    Ok((registration, loaded, meta))
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

// ── SkillDeclaration conversion (ORCH-0030 R2 M2) ─────────────
//
// These free functions translate the adapter's private
// `LoadedSkill` map into the public `SkillDeclaration` shape that
// the M3 trait switch will publish over the bus. They live here
// (rather than as `impl LoadedSkill` methods) so the unit tests can
// build synthetic `LoadedSkill`s and exercise the conversion
// without constructing a full `ComfyUiProvider`.

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
}
