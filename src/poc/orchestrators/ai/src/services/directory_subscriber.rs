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

    /// Run the subscriber loop. Two phases:
    ///
    /// 1. **Snapshot recovery.** Atomically grab every stateful
    ///    event currently in the bus's snapshot map (one entry per
    ///    topic — the latest capability announcement per provider)
    ///    plus a fresh broadcast receiver. Apply each snapshot
    ///    event in turn so the `CapabilityDirectory` reflects every
    ///    provider that has *ever* published a stateful event.
    /// 2. **Live tail.** Consume new events from the broadcast
    ///    receiver. The atomic capture in step 1 guarantees no
    ///    event is missed in the gap between snapshot and tail —
    ///    publishers using `publish_with_snapshot` hold the
    ///    snapshot write lock during their broadcast, and we hold
    ///    the snapshot read lock while subscribing to the
    ///    broadcast.
    ///
    /// This design lets the subscriber start arbitrarily late
    /// without losing any provider's announcement — even if the
    /// publisher fired its first announcement before the subscriber
    /// task was scheduled, the snapshot map preserves it for
    /// replay. The race condition the early-spawn workaround in
    /// `main.rs` mitigates is fully closed by this snapshot path.
    pub async fn run(self: Arc<Self>, shutdown: CancellationToken) {
        let (snapshot, mut rx) = self.events.raw_subscribe_with_snapshot().await;
        tracing::info!(
            snapshot_len = snapshot.len(),
            "directory_subscriber: started, replaying stateful snapshot"
        );

        // Phase 1: replay the snapshot. Apply each
        // `directory.provider.*.capabilities` event. Non-matching
        // events in the snapshot (other publishers' stateful
        // topics) are silently filtered by extract_capability_announcement.
        for event in snapshot {
            self.apply_event(&event).await;
        }

        // Phase 2: live tail.
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    tracing::info!("directory_subscriber: shutdown requested");
                    return;
                }
                result = rx.recv() => {
                    match result {
                        Ok(event) => {
                            self.apply_event(&event).await;
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

    /// Apply a single event from snapshot or live tail. Filters out
    /// non-capability-announcement topics, calls `apply` on a
    /// matching announcement, and logs the diff outcome. Pulled
    /// out of `run()` so both phases share the same processing.
    async fn apply_event(&self, event: &Event) {
        let Some(announcement) = extract_capability_announcement(event) else {
            return;
        };
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
///
/// Capability announcements are **stateful** — every late
/// subscriber needs to recover the latest one per provider — so we
/// route through [`EventBus::publish_with_snapshot`]. The bus
/// records the latest event per topic in its snapshot map and
/// late `DirectorySubscriber` consumers replay it via
/// [`EventBus::raw_subscribe_with_snapshot`] without ever needing
/// to trawl the full history ring.
pub async fn publish_capability_announcement(
    events: &EventBus,
    announcement: &CapabilityAnnouncement,
) {
    let topic = format!(
        "directory.provider.{}.capabilities",
        announcement.provider
    );
    events.publish_with_snapshot(topic, announcement).await;
}

