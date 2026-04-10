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
    /// Per-instance volume inventory (ORCH-0030 R2 M5).
    ///
    /// Key: instance URL (the ComfyUI service endpoint, NOT the
    /// derived Moss endpoint).
    /// Value: the set of volume-relative file paths present on that
    /// instance's `comfyui-models` volume, as returned by
    /// `moss_volume::list_volume_paths`.
    ///
    /// Populated by [`Self::refresh_inventory_and_readiness`] on every
    /// discovery event. The single source of truth for both
    /// capability publication and dispatch routing — mirrors
    /// Ollama's `OllamaCapabilityMatrix.instances[*].models_available`
    /// pattern. An instance whose probe fails (Moss unreachable, 5xx,
    /// malformed body) is absent from the map and contributes to
    /// zero skill readiness — conservative by design.
    instance_inventories:
        Arc<tokio::sync::RwLock<HashMap<String, std::collections::HashSet<String>>>>,
    /// Per-skill ready instance list (ORCH-0030 R2 M5).
    ///
    /// Key: skill moniker.
    /// Value: instance URLs that have **all** of the skill's
    /// `required_models` present in their inventory (alias-resolved
    /// through the dependency manifest).
    ///
    /// Populated by [`Self::refresh_inventory_and_readiness`] after
    /// every inventory pass. Read by [`publish_capabilities`] (to
    /// filter the announced skill list) and by `onboard` (to pick a
    /// dispatch target). Skills missing from the map have zero
    /// healthy instances and are NOT published — the dispatcher
    /// routes around them via `CapabilityDirectory`.
    ready_instances: Arc<tokio::sync::RwLock<HashMap<Moniker, Vec<String>>>>,
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
            instance_inventories: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            ready_instances: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        });

        // Publish the initial snapshot. No instances and no
        // inventory probe yet, so `enabled` will be false and the
        // skill list will be empty — but the Directory learns the
        // provider exists. The first real announcement happens
        // after `refresh_inventory_and_readiness` completes its
        // first pass on the next discovery event.
        provider.publish_capabilities().await;

        spawn_subscriber(provider.clone(), discovery, shutdown.clone());
        spawn_provisioning_worker(provider.clone(), shutdown);
        provider
    }

    // ORCH-0030 R2 M5: the unconstrained `pick()` was removed.
    // Dispatch routing now goes through `pick_ready_instance(&moniker)`
    // which only returns instances that have all the skill's
    // required models present in their inventory — never routes to
    // an instance missing dependencies, mirroring Ollama's
    // `pick_pinned` semantics.

    /// Publish the current loaded-skill set as a
    /// `CapabilityAnnouncement` event, filtered by per-skill
    /// instance readiness. Called at construction and on every
    /// instance-pool / inventory / skill-set change.
    ///
    /// **Inventory-first gating (ORCH-0030 R2 M5):** only skills
    /// listed in `self.ready_instances` are announced. A skill is
    /// "ready" iff at least one ComfyUI instance has every required
    /// model present on its volume — verified by walking the
    /// per-instance file inventory built from
    /// [`crate::services::skills::moss_volume::list_volume_paths`].
    ///
    /// The announcement carries:
    /// - `enabled`: `true` iff at least one ComfyUI instance is in
    ///   the pool AND at least one skill has a ready instance.
    ///   False when the pool is empty OR every skill is missing its
    ///   dependencies on every instance.
    /// - `capabilities`: one entry per unique primitive any **ready**
    ///   skill declares. Skills missing dependencies do not
    ///   contribute to the capability set, so a primitive that has
    ///   no usable skill is not advertised at all.
    /// - `skills`: the `SkillDeclaration` list filtered to ready
    ///   skills only.
    async fn publish_capabilities(&self) {
        // Snapshot the inputs into local owned values so we can
        // drop both locks before the (potentially heavy)
        // computation + bus publish.
        let filtered_map: HashMap<Moniker, LoadedSkill> = {
            let skills_map = self.skills.read().await;
            let ready = self.ready_instances.read().await;
            skills_map
                .iter()
                .filter(|(moniker, _)| ready.contains_key(*moniker))
                .map(|(moniker, loaded)| (moniker.clone(), loaded.clone()))
                .collect()
        };

        let skills = compute_skill_declarations(&filtered_map);
        let capabilities = compute_capabilities(&filtered_map);

        let announcement = build_capability_announcement(
            &self.name,
            !self.instances.is_empty(),
            capabilities,
            skills,
        );
        publish_capability_announcement(&self.events, &announcement).await;
    }

    /// Refresh the per-instance volume inventory and recompute
    /// per-skill readiness, then republish capabilities.
    ///
    /// This is the inventory-first pattern (ORCH-0030 R2 M5) that
    /// mirrors Ollama's matrix rebuild: probe every instance for
    /// its installed resources ONCE, store the result on the
    /// adapter, and use it for both publication AND routing.
    ///
    /// Probes are issued in parallel via
    /// [`crate::services::skills::moss_volume::list_volume_paths`].
    /// An instance whose probe fails (Moss unreachable, 5xx,
    /// malformed body) is absent from the inventory map and
    /// therefore contributes zero readiness — no skill becomes
    /// publishable on that instance until the next refresh
    /// succeeds.
    ///
    /// The provisioning queue is still notified about missing
    /// dependencies via [`Self::queue_missing_dependencies`] —
    /// inventory gating doesn't replace provisioning, it informs
    /// it.
    async fn refresh_inventory_and_readiness(&self, instance_urls: &[String]) {
        // Step 1: probe every instance in parallel.
        let probe_futs = instance_urls.iter().map(|url| {
            let http = self.http.clone();
            let provider_name = self.name.clone();
            let url_owned = url.clone();
            async move {
                let moss = moss_volume::derive_moss_endpoint(&url_owned);
                let paths = moss_volume::list_volume_paths(
                    &http,
                    &moss,
                    provider_name.as_str(),
                    COMFYUI_MODELS_VOLUME,
                )
                .await;
                (url_owned, paths)
            }
        });
        let results: Vec<(String, Option<std::collections::HashSet<String>>)> =
            futures_util::future::join_all(probe_futs).await;

        // Step 2: replace the inventory map. Probes that returned
        // None are dropped — the instance has no entry, so no skill
        // can be marked ready on it.
        let mut new_inventories: HashMap<String, std::collections::HashSet<String>> =
            HashMap::new();
        let mut probe_failures = 0usize;
        for (url, paths) in &results {
            match paths {
                Some(p) => {
                    tracing::debug!(
                        instance = %url,
                        files = p.len(),
                        "comfyui: inventory probe ok"
                    );
                    new_inventories.insert(url.clone(), p.clone());
                }
                None => {
                    probe_failures += 1;
                    tracing::warn!(
                        instance = %url,
                        "comfyui: inventory probe failed (instance contributes zero readiness)"
                    );
                }
            }
        }
        {
            let mut inv = self.instance_inventories.write().await;
            *inv = new_inventories;
        }

        // Step 3: recompute ready_instances from skills × inventories.
        // The dependency manifest is loaded once per pass — it's a
        // small JSON file and the read is cheap, but pulling it in
        // loop iterations would be wasteful.
        let manifest = DependencyManifest::load(&self.cache_paths.manifest_path).await;
        let new_ready: HashMap<Moniker, Vec<String>> = {
            let skills_map = self.skills.read().await;
            let inventories = self.instance_inventories.read().await;
            let mut ready: HashMap<Moniker, Vec<String>> = HashMap::new();
            for (moniker, loaded) in skills_map.iter() {
                let mut ready_for_skill: Vec<String> = Vec::new();
                for (instance_url, inventory) in inventories.iter() {
                    if skill_dependencies_present(loaded, inventory, &manifest) {
                        ready_for_skill.push(instance_url.clone());
                    }
                }
                if !ready_for_skill.is_empty() {
                    // Sort for deterministic dispatch ordering.
                    ready_for_skill.sort();
                    ready.insert(moniker.clone(), ready_for_skill);
                }
            }
            ready
        };
        let ready_skill_count = new_ready.len();
        {
            let mut ready = self.ready_instances.write().await;
            *ready = new_ready;
        }

        tracing::info!(
            instance_count = instance_urls.len(),
            probe_failures,
            ready_skills = ready_skill_count,
            "comfyui: inventory + readiness pass complete"
        );

        // Step 4: republish the (newly filtered) capability set.
        self.publish_capabilities().await;
    }

    /// Pick one ready instance for the given skill. Returns
    /// `Unreachable` when no instance has all required dependencies
    /// installed — never routes to an instance missing models, in
    /// the same way Ollama's selector never routes to an instance
    /// missing the requested model.
    async fn pick_ready_instance(
        &self,
        skill_moniker: &Moniker,
    ) -> Result<String, ProviderError> {
        let ready = self.ready_instances.read().await;
        let urls = ready.get(skill_moniker);
        match urls.and_then(|v| v.first()) {
            Some(url) => Ok(url.clone()),
            None => Err(ProviderError::Unreachable(format!(
                "no comfyui instance in the garden has all required models for skill `{}`",
                skill_moniker
            ))),
        }
    }

    /// Replace the instance pool with the given URL list. Returns
    /// `true` if the pool changed structurally (so the caller knows
    /// whether to issue a fresh inventory pass).
    ///
    /// Capability publication does NOT happen here — it is owned by
    /// [`Self::refresh_inventory_and_readiness`], which runs after
    /// each apply_merged in the discovery subscriber loop. Without
    /// the inventory probe, `ready_instances` would be stale and
    /// `publish_capabilities` would either lie or stay empty.
    async fn apply_merged(&self, urls: Vec<String>) -> bool {
        self.instances.set(urls)
    }

    /// Snapshot the adapter's currently-loaded skills as a list of
    /// `SkillDeclaration`s in the shape the `CapabilityDirectory`
    /// accepts. Used by tests and diagnostics.
    pub async fn skill_declarations(&self) -> Vec<SkillDeclaration> {
        let map = self.skills.read().await;
        compute_skill_declarations(&map)
    }

    /// For every loaded skill × every discovered instance, check the
    /// in-memory inventory and submit a provisioning job for any
    /// (skill, instance) pair where the dependencies are missing.
    ///
    /// **Inventory-first (ORCH-0030 R2 M5):** this no longer issues
    /// HEAD probes per file. The inventory was already populated by
    /// [`Self::refresh_inventory_and_readiness`] with one LIST call
    /// per instance. We just walk the cached file set in memory.
    /// Per-file network calls were O(skills × instances ×
    /// required_models); the inventory-first path is O(instances).
    ///
    /// The provisioning queue dedupes by (skill, endpoint), so
    /// re-submitting the same target on every discovery event is
    /// harmless — the queue ignores duplicates already in flight.
    async fn queue_missing_dependencies(&self, instances: &[DiscoveredInstance]) {
        if instances.is_empty() {
            return;
        }
        let manifest = DependencyManifest::load(&self.cache_paths.manifest_path).await;
        let loaded = self.skills.read().await.clone();
        let inventories = self.instance_inventories.read().await.clone();

        for (moniker, loaded_skill) in loaded.iter() {
            for instance in instances {
                // Look up this instance's inventory. Probe failures
                // mean the inventory map has no entry — treat that
                // as "we don't know what's there", which is more
                // conservative than "nothing is there". We do NOT
                // queue downloads against unknown inventories.
                let Some(inventory) = inventories.get(&instance.url) else {
                    continue;
                };
                if skill_dependencies_present(loaded_skill, inventory, &manifest) {
                    // Already provisioned. Nothing to do.
                    continue;
                }
                // Submit. Returns false if the queue already has
                // this target in flight (dedup), which is harmless.
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
                        "comfyui: queued provisioning job (missing dependencies)"
                    );
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
                    let instances = event.instances.clone();
                    let urls: Vec<String> =
                        instances.iter().map(|i| i.url.clone()).collect();
                    pool.set(&event.fqn, urls);
                    let merged = pool.flatten();
                    provider.apply_merged(merged.clone()).await;

                    // ORCH-0030 R2 M5 inventory-first pipeline:
                    //
                    // 1. Probe each instance's volume ONCE via
                    //    Moss's LIST endpoint and cache the file
                    //    set on the adapter.
                    // 2. Recompute per-skill readiness from the
                    //    in-memory inventory.
                    // 3. Republish the (filtered) capability set.
                    // 4. Submit provisioning jobs for any (skill,
                    //    instance) pair that's still missing
                    //    dependencies — the queue dedupes by
                    //    (skill, endpoint) so we never submit the
                    //    same job twice.
                    //
                    // Steps 1-3 happen synchronously inside
                    // `refresh_inventory_and_readiness`; the
                    // provisioning queue submission is fire-and-
                    // forget after that.
                    let provider_for_refresh = provider.clone();
                    tokio::spawn(async move {
                        provider_for_refresh
                            .refresh_inventory_and_readiness(&merged)
                            .await;
                        provider_for_refresh
                            .queue_missing_dependencies(&instances)
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

        // ── 2. Pin a READY instance ───────────────────────────
        //
        // A ComfyUI request must use the SAME instance for upload,
        // queue, history poll, and view — workflows reference
        // uploaded filenames that only exist on the instance they
        // were uploaded to. ORCH-0030 R2 M5: the picked instance
        // must also have every required model for this skill —
        // `pick_ready_instance` enforces that and returns
        // `Unreachable` otherwise.
        let instance = self.pick_ready_instance(skill_moniker).await?;
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
                parameters: comfyui_base_parameters_for(primitive),
            }
        })
        .collect();
    capabilities.sort_by(|a, b| a.primitive.dotted().cmp(b.primitive.dotted()));
    capabilities
}

/// Base form-schema parameters for ComfyUI primitives.
///
/// These are the common fields the catalog renders when the user
/// selects a bare primitive (e.g. `GET /v1/catalog/image/generate`)
/// without choosing a specific skill. Derived from the universal
/// fields present across all skills for each primitive — see the
/// live API analysis confirming these fields appear on 100% of
/// image.generate skills.
///
/// Skills override these with skill-specific defaults and narrowing;
/// the base parameters serve as a sensible starting point for bare-
/// primitive dispatch and for the dashboard's skill picker view.
fn comfyui_base_parameters_for(p: Primitive) -> Vec<SkillParameter> {
    use crate::domain::capability_announcement::{
        AutoDescriptor, ParameterType, ParameterWidget, SkillParameter,
    };

    match p {
        Primitive::ImageGenerate => vec![
            SkillParameter {
                field: "image.prompt.positive".into(),
                required: true,
                label: Some("Prompt".into()),
                field_type: Some(ParameterType::String),
                widget: Some(ParameterWidget::Textarea),
                placeholder: Some("Describe the image you want to create...".into()),
                ..Default::default()
            },
            SkillParameter {
                field: "image.prompt.negative".into(),
                required: false,
                label: Some("Negative Prompt".into()),
                field_type: Some(ParameterType::String),
                widget: Some(ParameterWidget::Textarea),
                placeholder: Some("What to avoid...".into()),
                ..Default::default()
            },
            SkillParameter {
                field: "image.dimensions.width".into(),
                required: false,
                label: Some("Width".into()),
                field_type: Some(ParameterType::Integer),
                widget: Some(ParameterWidget::Select),
                default: Some(serde_json::json!(1024)),
                options: Some(vec![
                    serde_json::json!(512),
                    serde_json::json!(768),
                    serde_json::json!(1024),
                    serde_json::json!(1280),
                    serde_json::json!(1536),
                    serde_json::json!(2048),
                ]),
                ..Default::default()
            },
            SkillParameter {
                field: "image.dimensions.height".into(),
                required: false,
                label: Some("Height".into()),
                field_type: Some(ParameterType::Integer),
                widget: Some(ParameterWidget::Select),
                default: Some(serde_json::json!(1024)),
                options: Some(vec![
                    serde_json::json!(512),
                    serde_json::json!(768),
                    serde_json::json!(1024),
                    serde_json::json!(1280),
                    serde_json::json!(1536),
                    serde_json::json!(2048),
                ]),
                ..Default::default()
            },
            SkillParameter {
                field: "image.sampling.steps".into(),
                required: false,
                label: Some("Steps".into()),
                field_type: Some(ParameterType::Number),
                widget: Some(ParameterWidget::Slider),
                default: Some(serde_json::json!(20)),
                min: Some(1.0),
                max: Some(50.0),
                step: Some(1.0),
                ..Default::default()
            },
            SkillParameter {
                field: "image.sampling.guidance".into(),
                required: false,
                label: Some("CFG Scale".into()),
                field_type: Some(ParameterType::Number),
                widget: Some(ParameterWidget::Slider),
                default: Some(serde_json::json!(7.0)),
                min: Some(1.0),
                max: Some(30.0),
                step: Some(0.5),
                ..Default::default()
            },
            SkillParameter {
                field: "image.sampling.seed".into(),
                required: false,
                label: Some("Seed".into()),
                widget: Some(ParameterWidget::Hidden),
                ..Default::default()
            },
        ],

        Primitive::ImageEdit => vec![
            SkillParameter {
                field: "image.prompt.positive".into(),
                required: true,
                label: Some("Prompt".into()),
                field_type: Some(ParameterType::String),
                widget: Some(ParameterWidget::Textarea),
                placeholder: Some("Describe the edit...".into()),
                ..Default::default()
            },
            SkillParameter {
                field: "image.prompt.negative".into(),
                required: false,
                label: Some("Negative Prompt".into()),
                field_type: Some(ParameterType::String),
                widget: Some(ParameterWidget::Textarea),
                placeholder: Some("What to avoid...".into()),
                ..Default::default()
            },
            SkillParameter {
                field: "image.sampling.steps".into(),
                required: false,
                label: Some("Steps".into()),
                field_type: Some(ParameterType::Number),
                widget: Some(ParameterWidget::Slider),
                default: Some(serde_json::json!(20)),
                min: Some(1.0),
                max: Some(50.0),
                step: Some(1.0),
                ..Default::default()
            },
        ],

        Primitive::ImageUpscale => vec![
            // Upscale has minimal base parameters — source image
            // is a media_input (handled separately), and the model/
            // variant selectors come from the skill.
        ],

        // Other ComfyUI primitives (image.analyze via ComfyUI skills,
        // audio.generate via TTS skills) get their parameters from
        // skills only. The base capability is skill-driven.
        _ => vec![],
    }
}

/// Check whether every required model for a skill is present in
/// the given instance inventory, alias-resolved through the
/// dependency manifest. Pure function — no IO, no `&self` — so the
/// readiness pass and any unit tests can exercise it directly.
///
/// A skill with empty `required_models` is always present (no
/// dependencies to satisfy). Otherwise, every model must resolve
/// to a path `{model_type}/{canonical_filename}` that exists in
/// the inventory set.
///
/// The canonical filename comes from the dependency manifest's
/// alias chain: when ComfyUI's filesystem stores `model-v2.bin`
/// but the skill references `model.bin`, the manifest holds the
/// alias and `manifest.resolve()` returns `model-v2.bin` so the
/// inventory lookup hits the actual file on disk.
fn skill_dependencies_present(
    skill: &LoadedSkill,
    inventory: &std::collections::HashSet<String>,
    manifest: &DependencyManifest,
) -> bool {
    if skill.required_models.is_empty() {
        return true;
    }
    for model in &skill.required_models {
        let canonical = manifest.resolve(&model.filename);
        let path = format!("{}/{}", model.model_type, canonical);
        if !inventory.contains(&path) {
            return false;
        }
    }
    true
}

/// Bundle the provider name, instance pool state, computed
/// capabilities, and skill declarations into the wire-shaped
/// [`CapabilityAnnouncement`] published to the bus.
///
/// `enabled` is true only when both conditions hold:
///
/// - the instance pool has at least one URL (something to dispatch to)
/// - the loaded skill set produces at least one capability (otherwise
///   ComfyUI is technically up but cannot serve anything until skills
///   land on disk)
///
/// Either condition alone leaves the adapter `enabled: false`. The
/// dispatcher then routes around it via `CapabilityDirectory`.
fn build_capability_announcement(
    name: &ProviderName,
    has_instances: bool,
    capabilities: Vec<AnnCapability>,
    skills: Vec<SkillDeclaration>,
) -> CapabilityAnnouncement {
    let enabled = has_instances && !capabilities.is_empty();
    CapabilityAnnouncement {
        provider: name.clone(),
        enabled,
        capabilities,
        skills,
    }
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
    use crate::domain::capability_announcement::{
        AutoDescriptor, ParameterType, ParameterWidget,
    };
    use crate::services::skills::types::FieldConstraint;

    let mut parameters: Vec<SkillParameter> = Vec::with_capacity(loaded.bindings.len() + 2);

    for binding in &loaded.bindings {
        let is_media = is_media_binding(binding);

        // Derive widget and constraints from the binding's narrow field.
        let (widget, min, max, step, options) = match &binding.narrow {
            Some(FieldConstraint::Range { min, max, step }) => (
                Some(ParameterWidget::Slider),
                Some(*min),
                Some(*max),
                *step,
                None,
            ),
            Some(FieldConstraint::Options { options: opts }) => (
                Some(ParameterWidget::Select),
                None,
                None,
                None,
                Some(opts.iter().map(|o| o.value.clone()).collect()),
            ),
            Some(FieldConstraint::Auto { .. }) => (
                Some(ParameterWidget::Hidden),
                None,
                None,
                None,
                None,
            ),
            None if is_media => (Some(ParameterWidget::File), None, None, None, None),
            None => (None, None, None, None, None),
        };

        // Derive field_type from self_described_type or fall back to
        // string for text fields, number for numeric constraints.
        let field_type = if binding.self_described_type.is_some() {
            // x_* fields with a self-described type — use that.
            Some(ParameterType::String)
        } else if min.is_some() || max.is_some() {
            // Has numeric constraints → Number
            Some(ParameterType::Number)
        } else {
            // Default to String for text fields
            None
        };

        parameters.push(SkillParameter {
            field: binding.field.as_str().to_string(),
            required: binding.required || is_media,
            description: binding.label.clone(),
            default: binding.default.clone(),
            auto: None,
            pinnable: !is_media,
            label: binding.label.clone(),
            field_type,
            widget,
            min,
            max,
            step,
            options,
            placeholder: None,
        });
    }

    if let Some(selector) = &loaded.model_selector {
        // Model selector is hidden for skills — the model is baked
        // into the workflow. The adapter decides, not the caller.
        parameters.push(SkillParameter {
            field: "selectors.model".to_string(),
            required: false,
            description: Some("Model used by this skill.".to_string()),
            default: Some(serde_json::Value::String(selector.default.clone())),
            auto: Some(AutoDescriptor {
                default: "recommended:generate".to_string(),
                description: Some("Model is fixed by the skill's workflow".to_string()),
            }),
            pinnable: false,
            label: Some("Model".to_string()),
            field_type: Some(ParameterType::String),
            widget: Some(ParameterWidget::Hidden),
            min: None,
            max: None,
            step: None,
            options: if selector.options.is_empty() {
                None
            } else {
                Some(
                    selector
                        .options
                        .iter()
                        .map(|o| o.value.clone())
                        .collect(),
                )
            },
            placeholder: None,
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
                label: Some("Variant".to_string()),
                field_type: Some(ParameterType::String),
                widget: Some(ParameterWidget::Select),
                min: None,
                max: None,
                step: None,
                options: Some(
                    variants
                        .iter()
                        .map(|v| serde_json::Value::String(v.value.clone()))
                        .collect(),
                ),
                placeholder: None,
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
        // Model is baked into the workflow → hidden, not pinnable.
        assert!(!model_param.pinnable);
        assert_eq!(model_param.widget, Some(crate::domain::capability_announcement::ParameterWidget::Hidden));
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

    // ── M4: bundled CapabilityAnnouncement ────────────────────

    fn provider_name() -> ProviderName {
        ProviderName::new(keys::providers::COMFYUI)
    }

    fn loaded_pair_for_announcement_test(
    ) -> (HashMap<Moniker, LoadedSkill>, Vec<AnnCapability>, Vec<SkillDeclaration>) {
        let (m_a, loaded_a) = synthetic_skill(
            "skill-a",
            Primitive::ImageGenerate,
            vec![
                param_binding("image.prompt.positive", "Positive", true),
                media_binding("image.source", &["image/png"]),
            ],
            None,
            None,
        );
        let (m_b, loaded_b) = synthetic_skill(
            "skill-b",
            Primitive::ImageUpscale,
            vec![param_binding("image.upscale.factor", "Factor", false)],
            None,
            None,
        );
        let mut map: HashMap<Moniker, LoadedSkill> = HashMap::new();
        map.insert(m_a, loaded_a);
        map.insert(m_b, loaded_b);
        let caps = compute_capabilities(&map);
        let skills = compute_skill_declarations(&map);
        (map, caps, skills)
    }

    #[test]
    fn announcement_disabled_when_no_instances() {
        let (_map, caps, skills) = loaded_pair_for_announcement_test();
        let ann = build_capability_announcement(&provider_name(), false, caps, skills);
        assert!(!ann.enabled);
        // Even disabled, the contents are still attached so observers
        // can see what ComfyUI WOULD serve once an instance comes up.
        assert!(!ann.capabilities.is_empty());
        assert!(!ann.skills.is_empty());
    }

    #[test]
    fn announcement_disabled_when_no_loaded_skills() {
        // Empty map → empty capabilities → enabled=false even with
        // a non-empty instance pool. ComfyUI without any loaded
        // skills cannot serve anything.
        let map: HashMap<Moniker, LoadedSkill> = HashMap::new();
        let caps = compute_capabilities(&map);
        let skills = compute_skill_declarations(&map);
        let ann = build_capability_announcement(&provider_name(), true, caps, skills);
        assert!(!ann.enabled);
        assert!(ann.capabilities.is_empty());
        assert!(ann.skills.is_empty());
    }

    #[test]
    fn announcement_enabled_when_instances_and_skills_present() {
        let (_map, caps, skills) = loaded_pair_for_announcement_test();
        let ann = build_capability_announcement(&provider_name(), true, caps, skills);
        assert!(ann.enabled);
    }

    #[test]
    fn announcement_bundles_capabilities_and_skills_together() {
        let (_map, caps, skills) = loaded_pair_for_announcement_test();
        let cap_count = caps.len();
        let skill_count = skills.len();
        let ann = build_capability_announcement(&provider_name(), true, caps, skills);
        assert_eq!(ann.capabilities.len(), cap_count);
        assert_eq!(ann.skills.len(), skill_count);
        assert_eq!(ann.provider.as_str(), "comfyui");
        // The two test skills declare two distinct primitives, so
        // capabilities should also be 2.
        assert_eq!(ann.capabilities.len(), 2);
        assert_eq!(ann.skills.len(), 2);
    }

    #[test]
    fn announcement_validates_against_directory_subscriber() {
        // The DirectorySubscriber rejects an announcement whose
        // skills reference a primitive that isn't declared in the
        // capabilities list. Our compute_capabilities +
        // compute_skill_declarations pair should always produce a
        // pair that round-trips through `validate()`.
        let (_map, caps, skills) = loaded_pair_for_announcement_test();
        let ann = build_capability_announcement(&provider_name(), true, caps, skills);
        ann.validate().expect("self-built announcement must validate");
    }
}
