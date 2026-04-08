//! Directory subscriber — consumes capability announcements from the
//! event bus and maintains a read-only view of every provider's
//! declared capabilities and skills (ORCH-0030 §R2.2, §R2.8).
//!
//! The subscriber is the **one-way bridge** between adapter-published
//! capability events and the Directory's authoritative view. Adapters
//! publish atomic full-snapshot announcements under
//! `directory.provider.{name}.capabilities`; the subscriber validates
//! each announcement, replaces its view of the provider wholesale, and
//! emits fine-grained derived events so clients can react to specific
//! changes without re-fetching state.
//!
//! # Two pieces
//!
//! - [`CapabilityDirectory`] — the read-only view other components
//!   query. Holds a `HashMap<ProviderName, ProviderCapabilities>`
//!   behind a `RwLock` and exposes lock-free snapshot methods.
//! - [`DirectorySubscriber`] — the task that drives the directory.
//!   Consumes events from the bus (via a broadcast receiver),
//!   validates, mutates, and republishes derived events.
//!
//! # Derived events
//!
//! Every accepted announcement fires at minimum:
//!
//! ```text
//! directory.provider.{name}.updated         — coarse "something changed"
//! ```
//!
//! Plus, by diffing the previous and current snapshots:
//!
//! ```text
//! directory.provider.{name}.enabled         — { enabled: true/false } (only on transition)
//! directory.provider.{name}.capability.added    — { primitive }
//! directory.provider.{name}.capability.removed  — { primitive }
//! directory.provider.{name}.skill.added         — { skill_id, primitive, display }
//! directory.provider.{name}.skill.removed       — { skill_id, primitive }
//! ```
//!
//! A dashboard subscribed to `directory.provider.*.skill.*` sees
//! exactly the skill churn it cares about; a coarse subscriber to
//! `*.updated` gets one refresh trigger per announcement.
//!
//! # Invariants
//!
//! 1. The `DirectorySubscriber` rejects any announcement that fails
//!    [`CapabilityAnnouncement::validate`]. Rejections are logged and
//!    a `directory.provider.{name}.rejected` event is emitted so
//!    observers know the adapter is broken — it is **not** silently
//!    dropped.
//! 2. The subscriber never mutates the view on an invalid announcement.
//! 3. Events are emitted *after* the view is updated, so any
//!    subscriber reacting to an `updated` event sees the new state.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;
use tokio::sync::{broadcast, RwLock};
use tokio_util::sync::CancellationToken;

use crate::domain::capability_announcement::{
    AnnouncementError, Capability, CapabilityAnnouncement, SkillDeclaration,
};
use crate::domain::events::{Event, EventBus};
use crate::domain::ids::ProviderName;
use crate::domain::primitive::Primitive;

// ── Per-provider view ───────────────────────────────────────

/// One provider's currently-declared state, reconstructed from the
/// most recent accepted announcement. The `CapabilityDirectory`
/// holds one of these per active provider.
#[derive(Debug, Clone)]
pub struct ProviderCapabilities {
    pub provider: ProviderName,
    pub enabled: bool,
    pub announcement: CapabilityAnnouncement,
    /// Monotonic sequence number incremented on every successful
    /// update. Exists so clients can observe "did this provider's
    /// view change since I last looked?" without diffing content.
    pub version: u64,
}

impl ProviderCapabilities {
    fn initial(announcement: CapabilityAnnouncement) -> Self {
        Self {
            provider: announcement.provider.clone(),
            enabled: announcement.enabled,
            announcement,
            version: 1,
        }
    }

    fn with_update(&self, announcement: CapabilityAnnouncement) -> Self {
        Self {
            provider: announcement.provider.clone(),
            enabled: announcement.enabled,
            announcement,
            version: self.version.saturating_add(1),
        }
    }
}

// ── The directory view ──────────────────────────────────────

/// The read-only view maintained by the subscriber. Other components
/// (the HTTP introspection handler, the dispatcher, the catalog
/// builder) query this view to answer "who serves what?"
///
/// All mutations go through [`DirectorySubscriber::apply`]. Readers
/// take a read lock; writers take a write lock. The lock is held
/// briefly — mutations are O(providers) in the common case.
pub struct CapabilityDirectory {
    providers: RwLock<HashMap<ProviderName, ProviderCapabilities>>,
    /// Sequence counter bumped on every mutation. Used by observers
    /// to detect "directory has changed since I last looked" without
    /// diffing content.
    version: std::sync::atomic::AtomicU64,
}

impl CapabilityDirectory {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            providers: RwLock::new(HashMap::new()),
            version: std::sync::atomic::AtomicU64::new(0),
        })
    }

    pub fn version(&self) -> u64 {
        self.version.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Snapshot of every currently-known provider. The returned map
    /// is a clone; callers can hold it arbitrarily long without
    /// blocking writers.
    pub async fn providers(&self) -> HashMap<ProviderName, ProviderCapabilities> {
        self.providers.read().await.clone()
    }

    /// Look up one provider by name.
    pub async fn provider(&self, name: &ProviderName) -> Option<ProviderCapabilities> {
        self.providers.read().await.get(name).cloned()
    }

    /// All providers currently declaring the given primitive as a
    /// base capability and `enabled: true`. Returns an empty `Vec`
    /// if no provider has declared it.
    ///
    /// This is the dispatcher's primary query: "which airports serve
    /// this route?"
    pub async fn providers_for_primitive(&self, primitive: Primitive) -> Vec<ProviderName> {
        let state = self.providers.read().await;
        state
            .values()
            .filter(|p| p.enabled && p.announcement.has_capability(primitive))
            .map(|p| p.provider.clone())
            .collect()
    }

    /// All providers currently declaring a skill with the given
    /// `(primitive, skill_id)` pair and `enabled: true`.
    ///
    /// Multiple providers can declare the same skill id for the same
    /// primitive; the dispatcher ranks the returned list by locality
    /// / preferences (commit 12).
    pub async fn providers_for_skill(
        &self,
        primitive: Primitive,
        skill_id: &str,
    ) -> Vec<ProviderName> {
        let state = self.providers.read().await;
        state
            .values()
            .filter(|p| {
                p.enabled
                    && p.announcement.has_capability(primitive)
                    && p.announcement
                        .find_skill(skill_id)
                        .map(|s| s.primitive == primitive)
                        .unwrap_or(false)
            })
            .map(|p| p.provider.clone())
            .collect()
    }

    /// Look up a skill declaration by (provider, skill_id). Returns
    /// the cloned declaration so the caller can introspect its
    /// parameters without holding any lock.
    pub async fn skill(
        &self,
        provider: &ProviderName,
        skill_id: &str,
    ) -> Option<SkillDeclaration> {
        let state = self.providers.read().await;
        state
            .get(provider)
            .and_then(|p| p.announcement.find_skill(skill_id).cloned())
    }

    /// Look up the capability declaration (with `media_inputs`) that
    /// `provider` exposes for `primitive`. Returns `None` if the
    /// provider is not registered, is disabled, or does not declare
    /// the primitive.
    ///
    /// This is the primary lookup for the dispatcher and the
    /// media_resolver after ORCH-0030 R2 M3 — the dispatcher uses
    /// it to confirm the chosen provider serves the requested
    /// primitive, and the media_resolver reads the returned
    /// `media_inputs` list to resolve every media reference in the
    /// request.
    pub async fn capability(
        &self,
        provider: &ProviderName,
        primitive: Primitive,
    ) -> Option<Capability> {
        let state = self.providers.read().await;
        state.get(provider).filter(|p| p.enabled).and_then(|p| {
            p.announcement
                .capabilities
                .iter()
                .find(|c| c.primitive == primitive)
                .cloned()
        })
    }

    /// All skills across all enabled providers, grouped by provider.
    /// Used by the catalog builder and `GET /v1/skills`.
    pub async fn all_skills(&self) -> Vec<(ProviderName, SkillDeclaration)> {
        let state = self.providers.read().await;
        let mut out = Vec::new();
        for provider in state.values() {
            if !provider.enabled {
                continue;
            }
            for skill in &provider.announcement.skills {
                out.push((provider.provider.clone(), skill.clone()));
            }
        }
        out
    }

    /// How many providers are currently registered (regardless of
    /// enabled state).
    pub async fn provider_count(&self) -> usize {
        self.providers.read().await.len()
    }

    /// How many providers are enabled and serving at least one
    /// capability.
    pub async fn enabled_provider_count(&self) -> usize {
        self.providers
            .read()
            .await
            .values()
            .filter(|p| p.enabled && !p.announcement.capabilities.is_empty())
            .count()
    }
}

// ── Diff for derived events ─────────────────────────────────

/// The set of changes between a previous and current announcement for
/// one provider. Emitted as derived events by the subscriber.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnnouncementDiff {
    pub enabled_changed: Option<bool>,
    pub capabilities_added: Vec<Primitive>,
    pub capabilities_removed: Vec<Primitive>,
    pub skills_added: Vec<SkillDeclaration>,
    pub skills_removed: Vec<(String, Primitive)>,
}

impl AnnouncementDiff {
    pub fn is_empty(&self) -> bool {
        self.enabled_changed.is_none()
            && self.capabilities_added.is_empty()
            && self.capabilities_removed.is_empty()
            && self.skills_added.is_empty()
            && self.skills_removed.is_empty()
    }

    /// Compute the diff from `prev` (may be `None` for the very first
    /// announcement from a provider) to `next`.
    pub fn compute(
        prev: Option<&CapabilityAnnouncement>,
        next: &CapabilityAnnouncement,
    ) -> Self {
        let mut diff = Self::default();

        let prev_enabled = prev.map(|p| p.enabled).unwrap_or(false);
        if prev_enabled != next.enabled {
            diff.enabled_changed = Some(next.enabled);
        }

        let prev_caps: std::collections::HashSet<Primitive> = prev
            .map(|p| p.capabilities.iter().map(|c| c.primitive).collect())
            .unwrap_or_default();
        let next_caps: std::collections::HashSet<Primitive> =
            next.capabilities.iter().map(|c| c.primitive).collect();

        for added in next_caps.difference(&prev_caps) {
            diff.capabilities_added.push(*added);
        }
        for removed in prev_caps.difference(&next_caps) {
            diff.capabilities_removed.push(*removed);
        }

        // Skill diff is keyed by id within this provider (ids are
        // unique per R2.8 invariant 2).
        let prev_skills: std::collections::HashMap<&str, &SkillDeclaration> = prev
            .map(|p| p.skills.iter().map(|s| (s.id.as_str(), s)).collect())
            .unwrap_or_default();
        let next_skills: std::collections::HashMap<&str, &SkillDeclaration> =
            next.skills.iter().map(|s| (s.id.as_str(), s)).collect();

        for (id, skill) in &next_skills {
            if !prev_skills.contains_key(id) {
                diff.skills_added.push((*skill).clone());
            }
        }
        for (id, skill) in &prev_skills {
            if !next_skills.contains_key(id) {
                diff.skills_removed.push(((*id).to_string(), skill.primitive));
            }
        }

        diff
    }
}

// ── Derived event payloads ──────────────────────────────────

#[derive(Debug, Clone, Serialize)]
struct UpdatedPayload<'a> {
    provider: &'a ProviderName,
    enabled: bool,
    capabilities_count: usize,
    skills_count: usize,
    version: u64,
}

#[derive(Debug, Clone, Serialize)]
struct EnabledPayload<'a> {
    provider: &'a ProviderName,
    enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
struct CapabilityChangePayload<'a> {
    provider: &'a ProviderName,
    primitive: Primitive,
}

#[derive(Debug, Clone, Serialize)]
struct SkillAddedPayload<'a> {
    provider: &'a ProviderName,
    skill: &'a SkillDeclaration,
}

#[derive(Debug, Clone, Serialize)]
struct SkillRemovedPayload<'a> {
    provider: &'a ProviderName,
    skill_id: &'a str,
    primitive: Primitive,
}

#[derive(Debug, Clone, Serialize)]
struct RejectedPayload<'a> {
    provider: &'a ProviderName,
    reason: String,
}

// ── Subscriber ──────────────────────────────────────────────

/// The task that drives the `CapabilityDirectory` from bus events.
///
/// Construct one via [`DirectorySubscriber::new`], then call
/// [`DirectorySubscriber::run`] to spawn the consuming loop. Tests
/// can bypass the loop and apply announcements directly via
/// [`DirectorySubscriber::apply`].
pub struct DirectorySubscriber {
    pub directory: Arc<CapabilityDirectory>,
    pub events: Arc<EventBus>,
}

impl DirectorySubscriber {
    pub fn new(directory: Arc<CapabilityDirectory>, events: Arc<EventBus>) -> Arc<Self> {
        Arc::new(Self { directory, events })
    }

    /// Apply a received announcement to the directory. On success,
    /// the directory's view of the provider is replaced wholesale
    /// and derived events are emitted. On validation failure, a
    /// `directory.provider.{name}.rejected` event is emitted and the
    /// directory is not mutated.
    ///
    /// Returns `Ok(diff)` on success, `Err(error)` on validation
    /// failure. Tests call this directly; production loops invoke it
    /// from [`DirectorySubscriber::run`].
    pub async fn apply(
        &self,
        announcement: CapabilityAnnouncement,
    ) -> Result<AnnouncementDiff, AnnouncementError> {
        // Validate before touching state.
        if let Err(err) = announcement.validate() {
            self.events
                .publish(
                    format!("directory.provider.{}.rejected", announcement.provider),
                    &RejectedPayload {
                        provider: &announcement.provider,
                        reason: err.to_string(),
                    },
                )
                .await;
            return Err(err);
        }

        // Compute the diff from the previous view before we replace
        // it. A `None` prev means this is the first announcement
        // from this provider.
        let (diff, version) = {
            let mut state = self.directory.providers.write().await;
            let prev = state
                .get(&announcement.provider)
                .map(|p| p.announcement.clone());
            let diff = AnnouncementDiff::compute(prev.as_ref(), &announcement);

            let next_entry = match state.get(&announcement.provider) {
                Some(existing) => existing.with_update(announcement.clone()),
                None => ProviderCapabilities::initial(announcement.clone()),
            };
            let version = next_entry.version;
            state.insert(announcement.provider.clone(), next_entry);
            self.directory
                .version
                .fetch_add(1, std::sync::atomic::Ordering::Release);
            (diff, version)
        };

        // Emit the coarse updated event — always fires, even if the
        // diff is empty (e.g., an adapter republishing identical
        // state for idempotency). Subscribers that want fine-grained
        // events look at the specific topics below.
        self.events
            .publish(
                format!("directory.provider.{}.updated", announcement.provider),
                &UpdatedPayload {
                    provider: &announcement.provider,
                    enabled: announcement.enabled,
                    capabilities_count: announcement.capabilities.len(),
                    skills_count: announcement.skills.len(),
                    version,
                },
            )
            .await;

        // Emit fine-grained derived events for observers that only
        // care about specific changes.
        if let Some(enabled) = diff.enabled_changed {
            self.events
                .publish(
                    format!("directory.provider.{}.enabled", announcement.provider),
                    &EnabledPayload {
                        provider: &announcement.provider,
                        enabled,
                    },
                )
                .await;
        }

        for primitive in &diff.capabilities_added {
            self.events
                .publish(
                    format!(
                        "directory.provider.{}.capability.added",
                        announcement.provider
                    ),
                    &CapabilityChangePayload {
                        provider: &announcement.provider,
                        primitive: *primitive,
                    },
                )
                .await;
        }

        for primitive in &diff.capabilities_removed {
            self.events
                .publish(
                    format!(
                        "directory.provider.{}.capability.removed",
                        announcement.provider
                    ),
                    &CapabilityChangePayload {
                        provider: &announcement.provider,
                        primitive: *primitive,
                    },
                )
                .await;
        }

        for skill in &diff.skills_added {
            self.events
                .publish(
                    format!("directory.provider.{}.skill.added", announcement.provider),
                    &SkillAddedPayload {
                        provider: &announcement.provider,
                        skill,
                    },
                )
                .await;
        }

        for (skill_id, primitive) in &diff.skills_removed {
            self.events
                .publish(
                    format!(
                        "directory.provider.{}.skill.removed",
                        announcement.provider
                    ),
                    &SkillRemovedPayload {
                        provider: &announcement.provider,
                        skill_id,
                        primitive: *primitive,
                    },
                )
                .await;
        }

        Ok(diff)
    }

    /// Run the subscriber loop. Consumes events from the bus's
    /// broadcast receiver and applies any matching
    /// `directory.provider.*.capabilities` payloads. Returns when
    /// the cancellation token is cancelled or the bus is closed.
    pub async fn run(self: Arc<Self>, shutdown: CancellationToken) {
        let mut rx = self.events.raw_subscribe();
        tracing::info!("directory_subscriber: started");

        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    tracing::info!("directory_subscriber: shutdown requested");
                    return;
                }
                result = rx.recv() => {
                    match result {
                        Ok(event) => {
                            if let Some(announcement) = extract_capability_announcement(&event) {
                                match self.apply(announcement).await {
                                    Ok(diff) => {
                                        if !diff.is_empty() {
                                            tracing::debug!(
                                                topic = %event.topic,
                                                capabilities_added = diff.capabilities_added.len(),
                                                capabilities_removed = diff.capabilities_removed.len(),
                                                skills_added = diff.skills_added.len(),
                                                skills_removed = diff.skills_removed.len(),
                                                "capability announcement applied",
                                            );
                                        }
                                    }
                                    Err(err) => {
                                        tracing::warn!(
                                            topic = %event.topic,
                                            error = %err,
                                            "capability announcement rejected",
                                        );
                                    }
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(
                                skipped = n,
                                "directory_subscriber: lagged, some announcements may have been missed",
                            );
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            tracing::info!("directory_subscriber: bus closed, exiting");
                            return;
                        }
                    }
                }
            }
        }
    }
}

/// Extract a `CapabilityAnnouncement` from a bus event if the topic
/// matches `directory.provider.{name}.capabilities`. Returns `None`
/// for any other topic, including derived events the subscriber
/// itself publishes (which would otherwise cause a re-entry loop).
fn extract_capability_announcement(event: &Event) -> Option<CapabilityAnnouncement> {
    let topic = event.topic.as_str();
    // Match exactly `directory.provider.{name}.capabilities` — no
    // suffix, no wildcard. Derived events have different suffixes
    // (.updated, .enabled, .capability.added, .skill.added, .rejected).
    let rest = topic.strip_prefix("directory.provider.")?;
    let rest = rest.strip_suffix(".capabilities")?;
    // `rest` is now the provider name, which must not contain
    // another `.` (that would be a derived event like
    // `.capability.added`).
    if rest.contains('.') {
        return None;
    }
    serde_json::from_value::<CapabilityAnnouncement>(Value::from(event.payload.clone())).ok()
}

// ── Publish helper ──────────────────────────────────────────

/// Helper for adapters: publish a capability announcement under the
/// correct topic. Use this from adapter code instead of calling
/// `EventBus::publish` directly to ensure topic grammar is consistent.
pub async fn publish_capability_announcement(
    events: &EventBus,
    announcement: &CapabilityAnnouncement,
) {
    let topic = format!(
        "directory.provider.{}.capabilities",
        announcement.provider
    );
    events.publish(topic, announcement).await;
}

// ── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::capability_announcement::{
        Capability, SkillDeclaration, SkillDisplay, SkillParameter,
    };

    fn provider() -> ProviderName {
        ProviderName::new("ollama")
    }

    fn other_provider() -> ProviderName {
        ProviderName::new("comfyui")
    }

    fn base_announcement() -> CapabilityAnnouncement {
        CapabilityAnnouncement {
            provider: provider(),
            enabled: true,
            capabilities: vec![
                Capability::new(Primitive::TextChat),
                Capability::new(Primitive::ImageAnalyze),
            ],
            skills: vec![],
        }
    }

    fn skill(id: &str, primitive: Primitive) -> SkillDeclaration {
        SkillDeclaration {
            id: id.into(),
            primitive,
            display: SkillDisplay::new(format!("Skill {id}")),
            parameters: vec![SkillParameter {
                field: "selectors.model".into(),
                required: false,
                description: None,
                default: Some(serde_json::json!("recommended:vision")),
                auto: None,
                pinnable: true,
            }],
        }
    }

    fn make_subscriber() -> Arc<DirectorySubscriber> {
        let bus = EventBus::new();
        let directory = CapabilityDirectory::new();
        DirectorySubscriber::new(directory, bus)
    }

    #[tokio::test]
    async fn empty_directory_returns_empty_queries() {
        let directory = CapabilityDirectory::new();
        assert_eq!(directory.provider_count().await, 0);
        assert_eq!(directory.enabled_provider_count().await, 0);
        assert!(directory
            .providers_for_primitive(Primitive::TextChat)
            .await
            .is_empty());
        assert!(directory.provider(&provider()).await.is_none());
        assert!(directory.all_skills().await.is_empty());
    }

    #[tokio::test]
    async fn apply_first_announcement_populates_directory() {
        let sub = make_subscriber();
        sub.apply(base_announcement()).await.unwrap();
        assert_eq!(sub.directory.provider_count().await, 1);
        assert_eq!(sub.directory.enabled_provider_count().await, 1);
        let providers = sub
            .directory
            .providers_for_primitive(Primitive::TextChat)
            .await;
        assert_eq!(providers, vec![provider()]);
    }

    #[tokio::test]
    async fn apply_rejects_invalid_announcement() {
        let sub = make_subscriber();
        let bad = CapabilityAnnouncement {
            provider: provider(),
            enabled: true,
            capabilities: vec![Capability::new(Primitive::TextChat)],
            skills: vec![skill("image-understanding", Primitive::ImageAnalyze)],
        };
        let err = sub.apply(bad).await.unwrap_err();
        assert!(matches!(
            err,
            AnnouncementError::SkillWithoutCapability { .. }
        ));
        // Directory state was not mutated.
        assert_eq!(sub.directory.provider_count().await, 0);
    }

    #[tokio::test]
    async fn rejected_announcement_emits_rejected_event() {
        let sub = make_subscriber();
        let mut rx = sub.events.raw_subscribe();
        let bad = CapabilityAnnouncement {
            provider: provider(),
            enabled: true,
            capabilities: vec![Capability::new(Primitive::TextChat)],
            skills: vec![skill("bad-skill", Primitive::ImageAnalyze)],
        };
        let _ = sub.apply(bad).await;
        // Drain events, look for rejected
        let mut found = false;
        while let Ok(ev) = rx.try_recv() {
            if ev.topic == "directory.provider.ollama.rejected" {
                found = true;
                break;
            }
        }
        assert!(found, "expected directory.provider.ollama.rejected event");
    }

    #[tokio::test]
    async fn second_announcement_replaces_wholesale() {
        let sub = make_subscriber();
        sub.apply(base_announcement()).await.unwrap();
        let replaced = CapabilityAnnouncement {
            provider: provider(),
            enabled: true,
            capabilities: vec![Capability::new(Primitive::TextEmbed)], // chat and analyze gone
            skills: vec![],
        };
        sub.apply(replaced).await.unwrap();

        let view = sub.directory.provider(&provider()).await.unwrap();
        assert_eq!(view.version, 2);
        assert!(view.announcement.has_capability(Primitive::TextEmbed));
        assert!(!view.announcement.has_capability(Primitive::TextChat));
        assert!(!view.announcement.has_capability(Primitive::ImageAnalyze));
    }

    #[tokio::test]
    async fn disabled_provider_drops_out_of_candidate_lists() {
        let sub = make_subscriber();
        sub.apply(base_announcement()).await.unwrap();
        assert_eq!(
            sub.directory
                .providers_for_primitive(Primitive::TextChat)
                .await,
            vec![provider()]
        );

        let disabled = CapabilityAnnouncement {
            enabled: false,
            ..base_announcement()
        };
        sub.apply(disabled).await.unwrap();
        assert!(sub
            .directory
            .providers_for_primitive(Primitive::TextChat)
            .await
            .is_empty());
        // Still counted in provider_count (not removed, just disabled).
        assert_eq!(sub.directory.provider_count().await, 1);
        assert_eq!(sub.directory.enabled_provider_count().await, 0);
    }

    #[tokio::test]
    async fn multiple_providers_for_same_primitive() {
        let sub = make_subscriber();
        sub.apply(base_announcement()).await.unwrap();
        let other = CapabilityAnnouncement {
            provider: other_provider(),
            enabled: true,
            capabilities: vec![Capability::new(Primitive::TextChat)],
            skills: vec![],
        };
        sub.apply(other).await.unwrap();

        let mut names = sub
            .directory
            .providers_for_primitive(Primitive::TextChat)
            .await;
        names.sort_by_key(|n| n.as_ref().to_string());
        assert_eq!(names, vec![other_provider(), provider()]);
    }

    #[tokio::test]
    async fn skill_added_event_fires_on_first_announcement_with_skills() {
        let sub = make_subscriber();
        let mut rx = sub.events.raw_subscribe();
        let ann = CapabilityAnnouncement {
            provider: provider(),
            enabled: true,
            capabilities: vec![Capability::new(Primitive::ImageAnalyze)],
            skills: vec![skill("image-understanding", Primitive::ImageAnalyze)],
        };
        sub.apply(ann).await.unwrap();

        let mut saw_updated = false;
        let mut saw_skill_added = false;
        let mut saw_enabled = false;
        let mut saw_capability_added = false;
        while let Ok(ev) = rx.try_recv() {
            match ev.topic.as_str() {
                "directory.provider.ollama.updated" => saw_updated = true,
                "directory.provider.ollama.skill.added" => saw_skill_added = true,
                "directory.provider.ollama.enabled" => saw_enabled = true,
                "directory.provider.ollama.capability.added" => saw_capability_added = true,
                _ => {}
            }
        }
        assert!(saw_updated, "coarse updated event missing");
        assert!(saw_skill_added, "fine-grained skill.added missing");
        assert!(saw_enabled, "enabled transition event missing");
        assert!(saw_capability_added, "capability.added event missing");
    }

    #[tokio::test]
    async fn skill_removed_event_fires_when_skill_disappears() {
        let sub = make_subscriber();
        let ann = CapabilityAnnouncement {
            provider: provider(),
            enabled: true,
            capabilities: vec![Capability::new(Primitive::ImageAnalyze)],
            skills: vec![skill("image-understanding", Primitive::ImageAnalyze)],
        };
        sub.apply(ann).await.unwrap();

        // Republish without the skill.
        let without_skill = CapabilityAnnouncement {
            provider: provider(),
            enabled: true,
            capabilities: vec![Capability::new(Primitive::ImageAnalyze)],
            skills: vec![],
        };
        let mut rx = sub.events.raw_subscribe();
        sub.apply(without_skill).await.unwrap();

        let mut saw_removed = false;
        while let Ok(ev) = rx.try_recv() {
            if ev.topic == "directory.provider.ollama.skill.removed" {
                saw_removed = true;
                break;
            }
        }
        assert!(saw_removed);
    }

    #[tokio::test]
    async fn diff_computes_additions_and_removals() {
        let prev = base_announcement();
        let next = CapabilityAnnouncement {
            provider: provider(),
            enabled: true,
            capabilities: vec![
                Capability::new(Primitive::TextChat), // kept
                Capability::new(Primitive::TextEmbed), // added
                                                       // ImageAnalyze removed
            ],
            skills: vec![],
        };
        let diff = AnnouncementDiff::compute(Some(&prev), &next);
        assert!(diff.enabled_changed.is_none());
        assert_eq!(diff.capabilities_added, vec![Primitive::TextEmbed]);
        assert_eq!(diff.capabilities_removed, vec![Primitive::ImageAnalyze]);
    }

    #[tokio::test]
    async fn diff_initial_announcement_marks_everything_added() {
        let next = base_announcement();
        let diff = AnnouncementDiff::compute(None, &next);
        assert_eq!(diff.enabled_changed, Some(true));
        assert_eq!(diff.capabilities_added.len(), 2);
        assert!(diff.capabilities_removed.is_empty());
    }

    #[tokio::test]
    async fn providers_for_skill_resolves_correctly() {
        let sub = make_subscriber();
        let ann = CapabilityAnnouncement {
            provider: provider(),
            enabled: true,
            capabilities: vec![Capability::new(Primitive::ImageAnalyze)],
            skills: vec![skill("image-understanding", Primitive::ImageAnalyze)],
        };
        sub.apply(ann).await.unwrap();

        let owners = sub
            .directory
            .providers_for_skill(Primitive::ImageAnalyze, "image-understanding")
            .await;
        assert_eq!(owners, vec![provider()]);

        let missing = sub
            .directory
            .providers_for_skill(Primitive::ImageAnalyze, "nonexistent")
            .await;
        assert!(missing.is_empty());

        let wrong_primitive = sub
            .directory
            .providers_for_skill(Primitive::TextChat, "image-understanding")
            .await;
        assert!(wrong_primitive.is_empty());
    }

    #[tokio::test]
    async fn all_skills_excludes_disabled_providers() {
        let sub = make_subscriber();
        let ann = CapabilityAnnouncement {
            provider: provider(),
            enabled: true,
            capabilities: vec![Capability::new(Primitive::ImageAnalyze)],
            skills: vec![skill("image-understanding", Primitive::ImageAnalyze)],
        };
        sub.apply(ann.clone()).await.unwrap();
        assert_eq!(sub.directory.all_skills().await.len(), 1);

        let disabled = CapabilityAnnouncement {
            enabled: false,
            ..ann
        };
        sub.apply(disabled).await.unwrap();
        assert!(sub.directory.all_skills().await.is_empty());
    }

    #[tokio::test]
    async fn extract_ignores_non_capability_topics() {
        let bus = EventBus::new();
        bus.publish(
            "directory.provider.ollama.updated".to_string(),
            &serde_json::json!({"provider": "ollama"}),
        )
        .await;
        bus.publish(
            "directory.provider.ollama.skill.added".to_string(),
            &serde_json::json!({"provider": "ollama"}),
        )
        .await;
        bus.publish(
            "random.topic".to_string(),
            &serde_json::json!({}),
        )
        .await;

        let mut rx = bus.raw_subscribe();
        // Publish one capabilities event
        bus.publish(
            "directory.provider.ollama.capabilities".to_string(),
            &base_announcement(),
        )
        .await;
        let mut matched = 0;
        let mut tried = 0;
        while let Ok(ev) = rx.try_recv() {
            tried += 1;
            if extract_capability_announcement(&ev).is_some() {
                matched += 1;
            }
            if tried > 10 {
                break;
            }
        }
        assert_eq!(matched, 1, "exactly one announcement should match");
    }

    #[tokio::test]
    async fn directory_version_increments_on_each_apply() {
        let sub = make_subscriber();
        assert_eq!(sub.directory.version(), 0);
        sub.apply(base_announcement()).await.unwrap();
        assert_eq!(sub.directory.version(), 1);
        sub.apply(base_announcement()).await.unwrap();
        assert_eq!(sub.directory.version(), 2);
    }
}
