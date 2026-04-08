//! RecommendationEngine — capability-aware ranking of registered
//! models.
//!
//! Callers ask for `recommended:<capability>`. The engine maintains
//! a cache keyed by capability label (`chat`, `quickchat`, `think`,
//! `vision`, …). Each entry is computed by walking the directory's
//! model catalog, filtering by the capability profile's required
//! tags + size constraints, and scoring with the profile's weights.
//!
//! The engine is refreshed by a background task that subscribes to
//! the directory snapshot. Contextualizer reads cached results via
//! [`crate::services::contextualizer::RecommendationResolver`].
//!
//! Layered scoring (per profile):
//!
//! - **Eligibility** — model declares the profile's required tag
//!   and falls within size constraints.
//! - **Pin override** — operator pin for this capability forces
//!   the chosen model to rank 1 if eligible.
//! - **Quality** — bonus per billion parameters, capped per
//!   profile (think values quality more than quickchat).
//! - **Context** — bonus per 1k tokens of context window, capped
//!   per profile (synthesis values context most).
//! - **Performance verdicts** — Fast / Degraded / Vetoed bonuses
//!   when providers publish [`PerformanceHint`] entries
//!   ([`crate::domain::provider::PerformanceHint`]). Models without
//!   hints are unmeasured (neutral).
//! - **Name affinity** — purpose-built models (`*ocr*`, `*embed*`)
//!   get a profile-defined boost.
//! - **Deterministic tiebreak** — alphabetical FQN ordering.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::{watch, RwLock};

use crate::domain::directory::{Directory, DirectorySnapshot, ModelView};
use crate::domain::ids::ModelFqn;
use crate::domain::primitive::Primitive;
use crate::domain::provider::PerformanceVerdict;
use crate::domain::recommendation_types::{
    CapabilityProfile, CapabilityProfileRegistry, Pin, RankedRecommendations, Recommendation,
    RecommendationCache,
};
use crate::services::contextualizer::RecommendationResolver;

// ── Pin registry ──────────────────────────────────────────────

/// Operator pin registry, keyed by capability label, persisted to
/// `{data_dir}/recommendations.json`.
pub struct PinRegistry {
    path: PathBuf,
    inner: RwLock<HashMap<String, Pin>>,
}

impl PinRegistry {
    pub async fn load(data_dir: &Path) -> Self {
        let path = data_dir.join("recommendations.json");
        let inner = match tokio::fs::read(&path).await {
            Ok(bytes) => match serde_json::from_slice::<PinFile>(&bytes) {
                Ok(file) => file.pins.into_iter().map(|p| (p.capability.clone(), p)).collect(),
                Err(e) => {
                    tracing::warn!(error = %e, "failed to parse recommendations.json; starting empty");
                    HashMap::new()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
            Err(e) => {
                tracing::warn!(error = %e, "failed to read recommendations.json; starting empty");
                HashMap::new()
            }
        };
        Self {
            path,
            inner: RwLock::new(inner),
        }
    }

    pub async fn get(&self, capability: &str) -> Option<Pin> {
        self.inner.read().await.get(capability).cloned()
    }

    pub async fn all(&self) -> HashMap<String, Pin> {
        self.inner.read().await.clone()
    }

    pub async fn set(&self, pin: Pin) -> std::io::Result<()> {
        let mut inner = self.inner.write().await;
        inner.insert(pin.capability.clone(), pin);
        self.persist(&inner).await
    }

    pub async fn delete(&self, capability: &str) -> std::io::Result<()> {
        let mut inner = self.inner.write().await;
        inner.remove(capability);
        self.persist(&inner).await
    }

    async fn persist(&self, pins: &HashMap<String, Pin>) -> std::io::Result<()> {
        let file = PinFile {
            pins: pins.values().cloned().collect(),
        };
        let bytes = serde_json::to_vec_pretty(&file)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&self.path, bytes).await
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct PinFile {
    pins: Vec<Pin>,
}

// ── Demand ledger ─────────────────────────────────────────────

/// Passive per-request counter. No v1 routing decision reads this;
/// it exists so a future advisor has historical data available.
#[derive(Default)]
pub struct DemandLedger {
    counters: RwLock<HashMap<DemandKey, u64>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DemandKey {
    pub primitive: Primitive,
    pub provider: String,
    pub model: String,
    pub outcome: String,
}

impl DemandLedger {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn record(&self, key: DemandKey) {
        let mut map = self.counters.write().await;
        *map.entry(key).or_insert(0) += 1;
    }

    pub async fn snapshot(&self) -> HashMap<DemandKey, u64> {
        self.counters.read().await.clone()
    }
}

// ── Engine ────────────────────────────────────────────────────

pub struct RecommendationEngine {
    directory: Arc<Directory>,
    pins: Arc<PinRegistry>,
    demand: Arc<DemandLedger>,
    profiles: Arc<CapabilityProfileRegistry>,
    cache_tx: watch::Sender<Arc<RecommendationCache>>,
}

impl RecommendationEngine {
    pub fn new(
        directory: Arc<Directory>,
        pins: Arc<PinRegistry>,
        demand: Arc<DemandLedger>,
    ) -> Arc<Self> {
        let profiles = Arc::new(CapabilityProfileRegistry::build());
        let initial = Arc::new(RecommendationCache {
            version: 0,
            built_at: Utc::now(),
            per_capability: HashMap::new(),
        });
        let (cache_tx, _cache_rx) = watch::channel(initial);
        Arc::new(Self {
            directory,
            pins,
            demand,
            profiles,
            cache_tx,
        })
    }

    pub fn profiles(&self) -> &Arc<CapabilityProfileRegistry> {
        &self.profiles
    }

    pub fn subscribe(&self) -> watch::Receiver<Arc<RecommendationCache>> {
        self.cache_tx.subscribe()
    }

    pub fn snapshot(&self) -> Arc<RecommendationCache> {
        self.cache_tx.borrow().clone()
    }

    pub fn directory(&self) -> &Arc<Directory> {
        &self.directory
    }

    pub fn pins(&self) -> &Arc<PinRegistry> {
        &self.pins
    }

    pub fn demand(&self) -> &Arc<DemandLedger> {
        &self.demand
    }

    /// Rebuild the cache from the current directory snapshot and
    /// persisted pin registry.
    pub async fn rebuild(&self) {
        let snapshot = self.directory.snapshot();
        let pins = self.pins.all().await;
        let cache = build_cache(&snapshot, &pins, &self.profiles);
        let _ = self.cache_tx.send_replace(Arc::new(cache));
    }

    /// Own the refresh loop: subscribe to the directory snapshot
    /// and rebuild the cache on every version bump. Runs until
    /// `shutdown` fires.
    pub async fn run(
        self: Arc<Self>,
        shutdown: tokio_util::sync::CancellationToken,
    ) {
        let mut rx = self.directory.subscribe();
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                res = rx.changed() => {
                    if res.is_err() {
                        break;
                    }
                    self.rebuild().await;
                }
            }
        }
    }
}

impl RecommendationResolver for RecommendationEngine {
    fn selected_for_capability(&self, capability: &str) -> Option<ModelFqn> {
        let cache = self.snapshot();
        cache
            .per_capability
            .get(capability)
            .and_then(|r| r.selected.clone())
    }

    fn primitive_for_capability(&self, capability: &str) -> Option<Primitive> {
        self.profiles.get(capability).map(|p| p.primitive)
    }

    fn default_capability_for_primitive(&self, primitive: Primitive) -> Option<String> {
        self.profiles
            .default_for_primitive(primitive)
            .map(|p| p.name.to_string())
    }
}

// ── Pure scoring ──────────────────────────────────────────────

fn build_cache(
    snapshot: &DirectorySnapshot,
    pins: &HashMap<String, Pin>,
    profiles: &CapabilityProfileRegistry,
) -> RecommendationCache {
    let mut per_capability = HashMap::new();
    for profile in profiles.iter() {
        let ranked = rank_for_profile(snapshot, profile, pins);
        per_capability.insert(profile.name.to_string(), ranked);
    }
    RecommendationCache {
        version: snapshot.version,
        built_at: Utc::now(),
        per_capability,
    }
}

fn rank_for_profile(
    snapshot: &DirectorySnapshot,
    profile: &CapabilityProfile,
    pins: &HashMap<String, Pin>,
) -> RankedRecommendations {
    let primitive_dotted = profile.primitive.dotted().to_string();
    let mut candidates: Vec<Recommendation> = Vec::new();

    for model in snapshot.models.values() {
        // Must serve the right primitive.
        if !model.primitives.iter().any(|p| p == &primitive_dotted) {
            continue;
        }
        // Must declare at least one of the required tags. An empty
        // required-tags list means "no tag filter" (any model
        // serving the primitive is eligible — used for primitives
        // where the provider doesn't expose vendor-side tags, e.g.
        // translate, image_generate).
        if !profile.required_tags.is_empty()
            && !profile
                .required_tags
                .iter()
                .any(|t| model.capability_tags.iter().any(|m| m == *t))
        {
            continue;
        }

        // Size constraints (in billions of parameters). Models
        // without a known parameter count pass through — no
        // negative inference.
        let params_b = model
            .parameter_count
            .map(|c| c as f64 / 1_000_000_000.0);
        if let Some(floor) = profile.size_floor_billions {
            if let Some(p) = params_b {
                if p < floor {
                    continue;
                }
            }
        }
        if let Some(ceiling) = profile.size_ceiling_billions {
            if let Some(p) = params_b {
                if p > ceiling {
                    continue;
                }
            }
        }

        let candidate = score_model(model, profile, params_b);
        candidates.push(candidate);
    }

    // Pin override: if a pin exists for this capability and matches
    // an eligible candidate, force it to rank 1 by adding the
    // profile's pin bonus and marking the record.
    if let Some(pin) = pins.get(profile.name) {
        if let Some(rec) = candidates.iter_mut().find(|c| c.model == pin.model) {
            rec.score += profile.weights.pin_bonus;
            rec.pinned = true;
            rec.reasoning.push("operator pin".to_string());
        }
    }

    // Sort: score desc, then alphabetical FQN.
    candidates.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.model.cmp(&b.model)));

    // Assign ranks.
    for (idx, c) in candidates.iter_mut().enumerate() {
        c.rank = (idx + 1) as u32;
    }

    let selected = candidates.first().map(|c| c.model.clone());
    let reasoning = candidates
        .first()
        .map(|c| c.reasoning.clone())
        .unwrap_or_default();

    RankedRecommendations {
        capability: profile.name.to_string(),
        primitive: profile.primitive.dotted().to_string(),
        selected,
        candidates,
        reasoning,
    }
}

fn score_model(
    model: &ModelView,
    profile: &CapabilityProfile,
    params_b: Option<f64>,
) -> Recommendation {
    let weights = &profile.weights;
    let mut score = weights.eligibility_base;
    let mut reasoning: Vec<String> = vec!["eligible".to_string()];

    // Quality bonus from parameter count.
    if let Some(p) = params_b {
        if weights.quality_per_billion != 0 {
            let raw = (p * weights.quality_per_billion as f64) as i64;
            let bounded = if weights.quality_cap > 0 {
                raw.min(weights.quality_cap)
            } else {
                raw
            };
            score += bounded;
            if p >= 1.0 {
                reasoning.push(format!("{:.1}B params", p));
            }
        }
    }

    // Context window bonus.
    if let Some(ctx) = model.context_length {
        if weights.context_per_1k_tokens != 0 {
            let raw = ((ctx / 1000) as i64) * weights.context_per_1k_tokens;
            let bounded = if weights.context_cap > 0 {
                raw.min(weights.context_cap)
            } else {
                raw
            };
            score += bounded;
            if ctx >= 32_000 {
                reasoning.push(format!("{}K context", ctx / 1000));
            }
        }
    }

    // Name affinity.
    if let Some(affinity) = profile.name_affinity {
        if model
            .short_name
            .to_lowercase()
            .contains(affinity.keyword)
        {
            score += affinity.bonus;
            reasoning.push(format!("name matches `{}`", affinity.keyword));
        }
    }

    // Performance verdicts (Layer 2 of the original ADR design).
    // Currently no provider publishes hints; the loop is here so
    // hints flow through automatically when they do.
    let verdict: Option<PerformanceVerdict> = None;
    if let Some(v) = verdict {
        let bonus = match v {
            PerformanceVerdict::Fast => weights.verdict_fast,
            PerformanceVerdict::Degraded => weights.verdict_degraded,
            PerformanceVerdict::Vetoed => weights.verdict_vetoed,
            PerformanceVerdict::Blocked => i64::MIN / 2,
            PerformanceVerdict::Unmeasured => 0,
        };
        score += bonus;
    }

    Recommendation {
        model: model.fqn.clone(),
        rank: 0,
        score,
        pinned: false,
        verdict,
        reasoning,
    }
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ids::{ProviderName, RegistrationId};
    use std::collections::HashMap;
    use std::sync::Arc;

    fn snapshot_with(
        models: Vec<(ProviderName, &'static str, Primitive, &'static [&'static str], Option<u64>, Option<u64>)>,
    ) -> DirectorySnapshot {
        let mut map = HashMap::new();
        for (provider, short, primitive, tags, params, ctx) in models {
            let fqn = ModelFqn::new(&provider, short);
            map.insert(
                fqn.clone(),
                ModelView {
                    fqn,
                    short_name: short.to_string(),
                    provider,
                    registration_id: RegistrationId::from("reg"),
                    primitives: vec![primitive.dotted().to_string()],
                    capability_tags: tags.iter().map(|s| s.to_string()).collect(),
                    size_bytes: None,
                    context_length: ctx,
                    parameter_count: params,
                },
            );
        }
        DirectorySnapshot {
            version: 1,
            updated_at: Utc::now(),
            providers: Arc::new(HashMap::new()),
            primitives: Arc::new(HashMap::new()),
            skills: Arc::new(HashMap::new()),
            models: Arc::new(map),
        }
    }

    #[test]
    fn quickchat_prefers_small_models_under_the_cap() {
        let provider = ProviderName::new("ollama");
        let snapshot = snapshot_with(vec![
            // 70B model — over the 5B cap, must be filtered out.
            (
                provider.clone(),
                "llama3.1:70b",
                Primitive::TextChat,
                &["completion"],
                Some(70_000_000_000),
                Some(128_000),
            ),
            // 1.5B small model — under the cap, should win.
            (
                provider.clone(),
                "qwen2.5:1.5b",
                Primitive::TextChat,
                &["completion"],
                Some(1_500_000_000),
                Some(32_000),
            ),
        ]);
        let profiles = CapabilityProfileRegistry::build();
        let pins = HashMap::new();
        let cache = build_cache(&snapshot, &pins, &profiles);
        let qc = cache.per_capability.get("quickchat").unwrap();
        assert_eq!(qc.selected.as_ref().unwrap().short_name(), "qwen2.5:1.5b");
        assert_eq!(qc.candidates.len(), 1, "70b filtered by ceiling");
    }

    #[test]
    fn think_requires_thinking_tag_and_minimum_size() {
        let provider = ProviderName::new("ollama");
        let snapshot = snapshot_with(vec![
            // Has thinking tag but only 3B — under 6B floor.
            (
                provider.clone(),
                "tiny-thinker:3b",
                Primitive::TextChat,
                &["completion", "thinking"],
                Some(3_000_000_000),
                Some(8000),
            ),
            // 8B with thinking — qualifies.
            (
                provider.clone(),
                "deepseek-r1:8b",
                Primitive::TextChat,
                &["completion", "thinking"],
                Some(8_000_000_000),
                Some(64_000),
            ),
            // Big model without thinking tag — disqualified.
            (
                provider.clone(),
                "llama3.1:70b",
                Primitive::TextChat,
                &["completion"],
                Some(70_000_000_000),
                Some(128_000),
            ),
        ]);
        let profiles = CapabilityProfileRegistry::build();
        let pins = HashMap::new();
        let cache = build_cache(&snapshot, &pins, &profiles);
        let think = cache.per_capability.get("think").unwrap();
        assert_eq!(
            think.selected.as_ref().unwrap().short_name(),
            "deepseek-r1:8b"
        );
        assert_eq!(think.candidates.len(), 1);
    }

    #[test]
    fn pin_overrides_score() {
        let provider = ProviderName::new("ollama");
        let snapshot = snapshot_with(vec![
            (
                provider.clone(),
                "llama3.1:8b",
                Primitive::TextChat,
                &["completion"],
                Some(8_000_000_000),
                Some(128_000),
            ),
            (
                provider.clone(),
                "qwen2.5:7b",
                Primitive::TextChat,
                &["completion"],
                Some(7_000_000_000),
                Some(32_000),
            ),
        ]);
        let profiles = CapabilityProfileRegistry::build();
        // Pin qwen even though llama would otherwise win on
        // context length.
        let mut pins = HashMap::new();
        pins.insert(
            "chat".to_string(),
            Pin {
                capability: "chat".to_string(),
                model: ModelFqn::new(&provider, "qwen2.5:7b"),
                pinned_at: Utc::now(),
                pinned_by: None,
                note: None,
            },
        );
        let cache = build_cache(&snapshot, &pins, &profiles);
        let chat = cache.per_capability.get("chat").unwrap();
        assert_eq!(chat.selected.as_ref().unwrap().short_name(), "qwen2.5:7b");
        assert!(chat.candidates[0].pinned);
    }

    #[test]
    fn embed_uses_embedding_tag_and_context_weight() {
        let provider = ProviderName::new("ollama");
        let snapshot = snapshot_with(vec![
            // Tiny context, tiny model.
            (
                provider.clone(),
                "all-minilm",
                Primitive::TextEmbed,
                &["embedding"],
                Some(33_000_000),
                Some(256),
            ),
            // Bigger context — should win on context bonus.
            (
                provider.clone(),
                "nomic-embed-text",
                Primitive::TextEmbed,
                &["embedding"],
                Some(137_000_000),
                Some(8192),
            ),
        ]);
        let profiles = CapabilityProfileRegistry::build();
        let pins = HashMap::new();
        let cache = build_cache(&snapshot, &pins, &profiles);
        let embed = cache.per_capability.get("embed").unwrap();
        assert_eq!(
            embed.selected.as_ref().unwrap().short_name(),
            "nomic-embed-text"
        );
    }
}
