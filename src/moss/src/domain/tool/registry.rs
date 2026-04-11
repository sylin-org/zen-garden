//! Unified Garden Registry — TOOLS-0003.
//!
//! Single write-through cache per stone holding all [`GardenTool`] entries
//! (offerings, gateways, storage) from all known stones.
//!
//! **Write path**: every mutation (offering change, gateway PUT/DELETE, seed bank
//! mount, remote beacon) goes through [`GardenRegistryInner::upsert`] /
//! [`GardenRegistryInner::remove`].
//!
//! **Read path**: all query endpoints (`garden/tools`, `garden/services`,
//! `stone/tools`, `stone/services`) project from this registry using
//! [`ToolQuery`] filters.
//!
//! Replaces: `ToolsCache`, `StorageCache`, `state.gateways`.

use chrono::Utc;
use garden_common::tools::{
    CapabilitySelector, GardenTool, ToolDelta, ToolDeltaKind, ToolsBeacon, build_tool_key,
    fqid_matches,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

const DEFAULT_HISTORY_LIMIT: usize = 4096;

pub type GardenRegistry = Arc<RwLock<GardenRegistryInner>>;

pub fn new_registry() -> GardenRegistry {
    Arc::new(RwLock::new(GardenRegistryInner::default()))
}

// ── Tool Query ──────────────────────────────────────────────────────────────

/// Filter predicate for registry queries.
#[derive(Debug, Clone, Default)]
pub struct ToolQuery {
    /// Filter by fqid (bare name = type match, instance = exact match).
    pub fqid: Option<String>,
    /// Filter by category: `"orchestrator"`, `"offering"`, `"storage"`.
    pub category: Option<String>,
    /// Filter by status: `"running"`, `"degraded"`, `"stopped"`.
    pub status: Option<String>,
    /// Filter by stone ID.
    pub stone_id: Option<String>,
    /// Capability selectors (AND semantics).
    pub capabilities: Vec<CapabilitySelector>,
}

impl ToolQuery {
    pub fn matches_tool(&self, tool: &GardenTool) -> bool {
        if let Some(ref fqid) = self.fqid
            && !fqid_matches(fqid, tool)
        {
            return false;
        }

        if let Some(ref category) = self.category
            && !tool.tool.category.eq_ignore_ascii_case(category)
        {
            return false;
        }

        if let Some(ref status) = self.status
            && !tool.service.status.eq_ignore_ascii_case(status)
        {
            return false;
        }

        if let Some(ref stone_id) = self.stone_id
            && !tool.stone.id.eq_ignore_ascii_case(stone_id)
        {
            return false;
        }

        for selector in &self.capabilities {
            if !tool.has_capability(&selector.cap_type, &selector.item) {
                return false;
            }
        }

        true
    }

    pub fn matches_delta(&self, delta: &ToolDelta) -> bool {
        if let Some(ref fqid) = self.fqid
            && !delta.fqid.eq_ignore_ascii_case(fqid)
        {
            if let Some(ref tool) = delta.tool {
                if !fqid_matches(fqid, tool) {
                    return false;
                }
            } else {
                return false;
            }
        }

        match delta.kind {
            ToolDeltaKind::Upsert => delta
                .tool
                .as_ref()
                .map(|tool| self.matches_tool(tool))
                .unwrap_or(false),
            ToolDeltaKind::Remove => {
                (self.category.is_none() && self.status.is_none() && self.capabilities.is_empty())
                    || self.fqid.is_some()
            }
        }
    }
}

// ── Entry Origin ────────────────────────────────────────────────────────────

/// Who wrote this entry — each origin has exactly one lifecycle owner.
///
/// - `Local` — projected from offerings + storage. `reconcile_local` owns lifecycle.
/// - `Gateway` — written directly by orchestrator registration. TTL reaping owns lifecycle.
/// - `Announced` — received from remote stone. Beacon reconciliation owns lifecycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryOrigin {
    /// Projected from local offerings or storage volumes.
    Local,
    /// Registered directly by an orchestrator gateway (`PUT /api/v1/garden/gateway`).
    /// TTL-managed — expires if not refreshed within the lease period.
    Gateway,
    /// Received from a remote stone via beacon.
    Announced { stone_id: String },
}

// ── Registry Entry ──────────────────────────────────────────────────────────

/// A single entry in the registry.
#[derive(Debug, Clone)]
pub struct RegistryEntry {
    /// The tool data (TOOLS-0002 contract).
    pub tool: GardenTool,
    /// Per-entry monotonic version (incremented on each upsert).
    pub version: u64,
    /// Who wrote this entry.
    pub origin: EntryOrigin,
    /// When this entry expires (Gateway entries only). `None` = permanent.
    pub expires_at: Option<Instant>,
}

// ── Registry Core ───────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct GardenRegistryInner {
    /// Keyed by tool_key: `"{stone_id}:{fqid}:{category}"`.
    entries: BTreeMap<String, RegistryEntry>,
    cursor: u64,
    history_limit: usize,
    history: VecDeque<ToolDelta>,
}

impl Default for GardenRegistryInner {
    fn default() -> Self {
        Self {
            entries: BTreeMap::new(),
            cursor: 0,
            history_limit: DEFAULT_HISTORY_LIMIT,
            history: VecDeque::with_capacity(DEFAULT_HISTORY_LIMIT),
        }
    }
}

impl GardenRegistryInner {
    // ── Queries ─────────────────────────────────────────────────────────

    pub fn current_cursor(&self) -> u64 {
        self.cursor
    }

    pub fn cursor_for_event_id(&self, event_id: &str) -> Option<u64> {
        self.history
            .iter()
            .rev()
            .find(|d| d.event_id == event_id)
            .map(|d| d.cursor)
    }

    /// Filtered snapshot of all entries. Returns (cursor, sorted tools).
    pub fn snapshot(&self, query: &ToolQuery) -> (u64, Vec<GardenTool>) {
        let mut tools: Vec<GardenTool> = self
            .entries
            .values()
            .filter(|e| query.matches_tool(&e.tool))
            .map(|e| e.tool.clone())
            .collect();

        // 3-tier sort matching sort_found_services policy:
        let search_fqid = query.fqid.as_deref();
        tools.sort_by(|a, b| {
            // Primary: exact fqid match first (when fqid filter is present).
            if let Some(fqid) = search_fqid {
                let a_exact = a.fqid.eq_ignore_ascii_case(fqid);
                let b_exact = b.fqid.eq_ignore_ascii_case(fqid);
                match (a_exact, b_exact) {
                    (true, false) => return std::cmp::Ordering::Less,
                    (false, true) => return std::cmp::Ordering::Greater,
                    _ => {}
                }
            }
            // Secondary: category priority (orchestrators → offerings → storage).
            a.category_priority()
                .cmp(&b.category_priority())
                // Tertiary: alphabetical by fqid, then stone name.
                .then_with(|| a.fqid.cmp(&b.fqid))
                .then_with(|| a.stone.name.cmp(&b.stone.name))
        });

        (self.cursor, tools)
    }

    /// Delta replay since a given cursor, filtered by query.
    pub fn deltas_since(&self, since_cursor: u64, query: &ToolQuery) -> Vec<ToolDelta> {
        self.history
            .iter()
            .filter(|d| d.cursor > since_cursor && query.matches_delta(d))
            .cloned()
            .collect()
    }

    /// Get a specific entry by key.
    pub fn get(&self, key: &str) -> Option<&RegistryEntry> {
        self.entries.get(key)
    }

    /// Get a tool by key.
    pub fn get_tool(&self, key: &str) -> Option<&GardenTool> {
        self.entries.get(key).map(|e| &e.tool)
    }

    // ── Direct Mutations ────────────────────────────────────────────────

    /// Upsert a single entry. Returns a delta if the entry changed.
    ///
    /// This is the primary write method. All write adapters call this.
    pub fn upsert(&mut self, tool: GardenTool, origin: EntryOrigin) -> Option<ToolDelta> {
        self.upsert_with_expiry(tool, origin, None)
    }

    /// Upsert with an optional expiry. Gateway entries use this to set TTL.
    pub fn upsert_with_expiry(
        &mut self,
        tool: GardenTool,
        origin: EntryOrigin,
        expires_at: Option<Instant>,
    ) -> Option<ToolDelta> {
        let key = build_tool_key(&tool.stone.id, &tool.fqid, &tool.tool.category);

        if let Some(existing) = self.entries.get_mut(&key)
            && tool_equivalent(&existing.tool, &tool)
            && existing.origin == origin
        {
            // Content unchanged — just refresh TTL silently if applicable.
            if expires_at.is_some() {
                existing.expires_at = expires_at;
            }
            return None;
        }

        let version = self.entries.get(&key).map(|e| e.version + 1).unwrap_or(1);

        self.entries.insert(
            key.clone(),
            RegistryEntry {
                tool: tool.clone(),
                version,
                origin,
                expires_at,
            },
        );

        Some(self.append_history(ToolDelta {
            event_id: garden_common::utils::ids::generate_guidv7(),
            cursor: 0,
            timestamp: Utc::now(),
            kind: ToolDeltaKind::Upsert,
            fqid: tool.fqid.clone(),
            tool_key: key,
            revision: version,
            tool: Some(tool),
        }))
    }

    /// Remove an entry by key. Returns a delta if the entry existed.
    pub fn remove(&mut self, key: &str) -> Option<ToolDelta> {
        let entry = self.entries.remove(key)?;
        Some(self.append_history(ToolDelta {
            event_id: garden_common::utils::ids::generate_guidv7(),
            cursor: 0,
            timestamp: Utc::now(),
            kind: ToolDeltaKind::Remove,
            fqid: entry.tool.fqid,
            tool_key: key.to_string(),
            revision: entry.version,
            tool: None,
        }))
    }

    // ── Batch Operations ────────────────────────────────────────────────

    /// Reconcile a full batch of local entries against the registry.
    ///
    /// Used at startup when loading persisted offerings and scanning local
    /// seed banks. Entries not in the incoming batch (with the same origin)
    /// are removed.
    pub fn reconcile_local(
        &mut self,
        local_stone_id: &str,
        incoming: Vec<GardenTool>,
        origin: EntryOrigin,
    ) -> Vec<ToolDelta> {
        let mut applied = Vec::new();
        let mut seen_keys = BTreeSet::new();

        for tool in incoming {
            let key = build_tool_key(&tool.stone.id, &tool.fqid, &tool.tool.category);
            seen_keys.insert(key);
            if let Some(delta) = self.upsert(tool, origin.clone()) {
                applied.push(delta);
            }
        }

        // Remove stale entries with the same origin that weren't in the batch.
        let stale: Vec<String> = self
            .entries
            .iter()
            .filter(|(key, e)| {
                e.tool.stone.id == local_stone_id && e.origin == origin && !seen_keys.contains(*key)
            })
            .map(|(key, _)| key.clone())
            .collect();

        for key in stale {
            if let Some(delta) = self.remove(&key) {
                applied.push(delta);
            }
        }

        applied
    }

    /// Remove all entries for a given stone. Used on stone goodbye/offline.
    pub fn remove_stone(&mut self, stone_id: &str) -> Vec<ToolDelta> {
        let keys: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, e)| e.tool.stone.id == stone_id)
            .map(|(key, _)| key.clone())
            .collect();

        let mut removed = Vec::new();
        for key in keys {
            if let Some(delta) = self.remove(&key) {
                removed.push(delta);
            }
        }
        removed
    }

    // Reap expired gateway entries. Returns deltas for each removal.

    // ── Beacon Support ──────────────────────────────────────────────────

    /// Build a snapshot of local entries as deltas for beacon broadcast.
    pub fn local_snapshot_for_beacon(&self, stone_id: &str) -> Vec<ToolDelta> {
        self.entries
            .iter()
            .filter(|(_, e)| e.tool.stone.id == stone_id)
            .map(|(key, e)| ToolDelta {
                event_id: garden_common::utils::ids::generate_guidv7(),
                cursor: self.cursor,
                timestamp: Utc::now(),
                kind: ToolDeltaKind::Upsert,
                fqid: e.tool.fqid.clone(),
                tool_key: key.clone(),
                revision: e.version,
                tool: Some(e.tool.clone()),
            })
            .collect()
    }

    /// Merge a remote beacon into the registry.
    ///
    /// Applies upserts and removals from the beacon. Entries are tagged
    /// with [`EntryOrigin::Announced`] and have no TTL.
    ///
    /// When `beacon.snapshot` is true, the beacon represents the complete
    /// set of tools for that stone. Any previously-announced entries from
    /// that stone that are absent from the snapshot are removed
    /// (reconciliation). This prevents stale entries from persisting when
    /// a Remove beacon is lost over UDP.
    pub fn apply_remote_beacon(&mut self, beacon: &ToolsBeacon) -> Vec<ToolDelta> {
        let origin = EntryOrigin::Announced {
            stone_id: beacon.stone_id.clone(),
        };
        let mut applied = Vec::new();
        let mut seen_keys = BTreeSet::new();

        for incoming in &beacon.deltas {
            match incoming.kind {
                ToolDeltaKind::Upsert => {
                    let Some(tool) = incoming.tool.clone() else {
                        continue;
                    };

                    let key = build_tool_key(&tool.stone.id, &tool.fqid, &tool.tool.category);

                    if beacon.snapshot {
                        seen_keys.insert(key.clone());
                    }

                    // Skip if existing entry has same content and revision is 0
                    // (initial snapshot that hasn't changed).
                    let should_apply = self
                        .entries
                        .get(&key)
                        .map(|_existing| incoming.revision > 0)
                        .unwrap_or(true);

                    if !should_apply {
                        continue;
                    }

                    let version = self.entries.get(&key).map(|e| e.version + 1).unwrap_or(1);

                    self.entries.insert(
                        key.clone(),
                        RegistryEntry {
                            tool: tool.clone(),
                            version,
                            origin: origin.clone(),
                            expires_at: None,
                        },
                    );

                    let delta = self.append_history(ToolDelta {
                        event_id: incoming.event_id.clone(),
                        cursor: 0,
                        timestamp: incoming.timestamp,
                        kind: ToolDeltaKind::Upsert,
                        fqid: tool.fqid.clone(),
                        tool_key: key,
                        revision: incoming.revision,
                        tool: Some(tool),
                    });
                    applied.push(delta);
                }
                ToolDeltaKind::Remove => {
                    let key = &incoming.tool_key;
                    if self.entries.remove(key).is_some() {
                        let delta = self.append_history(ToolDelta {
                            event_id: incoming.event_id.clone(),
                            cursor: 0,
                            timestamp: incoming.timestamp,
                            kind: ToolDeltaKind::Remove,
                            fqid: incoming.fqid.clone(),
                            tool_key: key.clone(),
                            revision: incoming.revision,
                            tool: None,
                        });
                        applied.push(delta);
                    }
                }
            }
        }

        // Snapshot reconciliation: remove announced entries from this stone
        // that were not present in the snapshot.
        //
        // Guard: skip reconciliation when the snapshot is empty. An empty
        // snapshot almost always means the sender is still starting up
        // (storage not yet projected into the registry). Purging valid
        // entries cached from a previous beacon would create an unnecessary
        // gap in garden visibility until the next non-empty snapshot arrives.
        // Genuine removal is handled by explicit Remove deltas, not by the
        // absence of entries in an empty snapshot.
        if beacon.snapshot && !seen_keys.is_empty() {
            let stale: Vec<String> = self
                .entries
                .iter()
                .filter(|(key, e)| {
                    matches!(&e.origin, EntryOrigin::Announced { stone_id } if stone_id == &beacon.stone_id)
                        && !seen_keys.contains(*key)
                })
                .map(|(key, _)| key.clone())
                .collect();

            for key in stale {
                tracing::debug!(
                    key = %key,
                    stone = %beacon.stone_name,
                    "registry: removing stale announced entry (snapshot reconciliation)"
                );
                if let Some(entry) = self.entries.remove(&key) {
                    let delta = self.append_history(ToolDelta {
                        event_id: garden_common::utils::ids::generate_guidv7(),
                        cursor: 0,
                        timestamp: chrono::Utc::now(),
                        kind: ToolDeltaKind::Remove,
                        fqid: entry.tool.fqid,
                        tool_key: key,
                        revision: entry.version,
                        tool: None,
                    });
                    applied.push(delta);
                }
            }
        }

        applied
    }

    // ── Storage-Specific Queries ────────────────────────────────────────

    /// Find all storage entries (seed banks) across all stones.
    pub fn storage_entries(&self) -> Vec<&RegistryEntry> {
        self.entries
            .values()
            .filter(|e| e.tool.tool.category == garden_common::constants::CATEGORY_STORAGE)
            .collect()
    }

    /// Find storage entries for a specific seed bank name.
    pub fn storage_by_name(&self, name: &str) -> Vec<&RegistryEntry> {
        self.entries
            .values()
            .filter(|e| {
                e.tool.tool.category == garden_common::constants::CATEGORY_STORAGE
                    && (e.tool.fqid.eq_ignore_ascii_case(name)
                        || e.tool.tool.name.eq_ignore_ascii_case(name))
            })
            .collect()
    }

    /// Find the Primary replica for a named seed bank.
    ///
    /// Prefers Primary role. Falls back to any replica if no Primary found.
    pub fn storage_primary(&self, name: &str) -> Option<&RegistryEntry> {
        let replicas = self.storage_by_name(name);
        let primary = replicas.iter().find(|e| {
            e.tool
                .storage
                .as_ref()
                .and_then(|s| s.role.as_deref())
                .is_some_and(|r| r.eq_ignore_ascii_case(garden_common::constants::ROLE_PRIMARY))
        });
        primary.copied().or_else(|| replicas.into_iter().next())
    }

    /// Route to the Primary replica for a named seed bank.
    ///
    /// Returns `(stone_id, endpoint, seed_bank_id)` for the Primary stone,
    /// excluding `exclude_stone` (typically this stone, for remote routing).
    pub fn route_to_primary(
        &self,
        name: &str,
        exclude_stone: &str,
    ) -> Option<(String, String, String)> {
        let replicas = self.storage_by_name(name);

        // First try: Primary role on a different stone
        let primary = replicas.iter().find(|e| {
            e.tool.stone.id != exclude_stone
                && e.tool
                    .storage
                    .as_ref()
                    .and_then(|s| s.role.as_deref())
                    .is_some_and(|r| r.eq_ignore_ascii_case(garden_common::constants::ROLE_PRIMARY))
        });

        // Fallback: any replica on a different stone
        let target = primary.or_else(|| replicas.iter().find(|e| e.tool.stone.id != exclude_stone));

        target.map(|e| {
            (
                e.tool.stone.id.clone(),
                e.tool.stone.endpoint.clone(),
                e.tool.tool.id.clone(),
            )
        })
    }

    /// Find all storage entries with S3 protocol support.
    pub fn find_s3_gateways(&self) -> Vec<&RegistryEntry> {
        self.entries
            .values()
            .filter(|e| {
                e.tool.tool.category == garden_common::constants::CATEGORY_STORAGE
                    && e.tool.storage.as_ref().is_some_and(|s| {
                        s.protocols
                            .iter()
                            .any(|p| p == garden_common::constants::PROTOCOL_S3)
                    })
            })
            .collect()
    }

    /// Find a storage entry by seed bank ID across all stones.
    pub fn storage_by_id(&self, id: &str) -> Option<&RegistryEntry> {
        self.entries.values().find(|e| {
            e.tool.tool.category == garden_common::constants::CATEGORY_STORAGE
                && e.tool.tool.id == id
        })
    }

    /// Group storage entries by stone_id.
    ///
    /// Returns a map of `stone_id → Vec<&RegistryEntry>` for all storage entries.
    /// Used by portrait/overview endpoints.
    pub fn storage_grouped_by_stone(&self) -> BTreeMap<String, Vec<&RegistryEntry>> {
        let mut grouped: BTreeMap<String, Vec<&RegistryEntry>> = BTreeMap::new();
        for entry in self.entries.values() {
            if entry.tool.tool.category == garden_common::constants::CATEGORY_STORAGE {
                grouped
                    .entry(entry.tool.stone.id.clone())
                    .or_default()
                    .push(entry);
            }
        }
        grouped
    }

    /// Count stones that have storage entries.
    pub fn storage_stone_count(&self) -> usize {
        self.storage_grouped_by_stone().len()
    }

    /// Count total storage entries (seed banks) across all stones.
    pub fn storage_count(&self) -> usize {
        self.entries
            .values()
            .filter(|e| e.tool.tool.category == garden_common::constants::CATEGORY_STORAGE)
            .count()
    }

    /// Get the Moss endpoint for a given stone_id.
    pub fn stone_endpoint(&self, stone_id: &str) -> Option<&str> {
        self.entries
            .values()
            .find(|e| e.tool.stone.id == stone_id)
            .map(|e| e.tool.stone.endpoint.as_str())
    }

    // ── Gateway Lifecycle ────────────────────────────────────────────────

    /// Remove all expired gateway entries. Returns deltas for each removal.
    pub fn reap_expired_gateways(&mut self) -> Vec<ToolDelta> {
        let now = Instant::now();
        let expired: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, e)| {
                e.origin == EntryOrigin::Gateway && e.expires_at.is_some_and(|t| t <= now)
            })
            .map(|(key, _)| key.clone())
            .collect();

        let mut removed = Vec::new();
        for key in expired {
            if let Some(delta) = self.remove(&key) {
                removed.push(delta);
            }
        }
        removed
    }

    /// Remove the gateway entry for a given offering on a given stone.
    ///
    /// Finds the entry by matching `EntryOrigin::Gateway` + `tool_type` + `stone_id`.
    pub fn remove_gateway(&mut self, offering: &str, stone_id: &str) -> Option<ToolDelta> {
        let key = self
            .entries
            .iter()
            .find(|(_, e)| {
                e.origin == EntryOrigin::Gateway
                    && e.tool.stone.id == stone_id
                    && e.tool.tool.tool_type.eq_ignore_ascii_case(offering)
            })
            .map(|(k, _)| k.clone());
        key.and_then(|k| self.remove(&k))
    }

    /// Returns true if any gateway entry handles the given offering type.
    pub fn handles_offering(&self, offering: &str) -> bool {
        self.entries.values().any(|e| {
            e.origin == EntryOrigin::Gateway && e.tool.tool.tool_type.eq_ignore_ascii_case(offering)
        })
    }

    /// Returns the set of offering types that have registered gateway handlers.
    pub fn handled_offerings(&self) -> std::collections::HashSet<String> {
        self.entries
            .values()
            .filter(|e| e.origin == EntryOrigin::Gateway)
            .map(|e| e.tool.tool.tool_type.clone())
            .collect()
    }

    // ── Internals ───────────────────────────────────────────────────────

    fn append_history(&mut self, mut delta: ToolDelta) -> ToolDelta {
        self.cursor = self.cursor.saturating_add(1);
        delta.cursor = self.cursor;

        self.history.push_back(delta.clone());
        while self.history.len() > self.history_limit {
            self.history.pop_front();
        }

        delta
    }
}

fn tool_equivalent(lhs: &GardenTool, rhs: &GardenTool) -> bool {
    lhs.fqid == rhs.fqid
        && lhs.tool == rhs.tool
        && lhs.stone == rhs.stone
        && lhs.service == rhs.service
        && lhs.capabilities == rhs.capabilities
        && lhs.storage == rhs.storage
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use garden_common::tools::{Capability, ServiceInfo, Stone, ToolIdentity};

    fn sample_tool(fqid: &str, category: &str, stone_id: &str) -> GardenTool {
        let tool_type = if fqid.contains(':') {
            fqid.split_once(':').unwrap().0
        } else {
            fqid
        };
        let name = if fqid.contains(':') {
            fqid.split_once(':').unwrap().1
        } else {
            ""
        };
        GardenTool {
            fqid: fqid.to_string(),
            tool: ToolIdentity {
                name: name.to_string(),
                tool_type: tool_type.to_string(),
                category: category.to_string(),
                id: format!("uid-{}", fqid),
                tags: Vec::new(),
                source: String::new(),
            },
            stone: Stone {
                id: stone_id.to_string(),
                name: stone_id.to_string(),
                endpoint: format!("http://{}:7185", stone_id),
            },
            service: ServiceInfo {
                status: "running".to_string(),
                ready: true,
                protocol: tool_type.to_string(),
                uris: Vec::new(),
                hostname: None,
                ip: None,
                port: None,
                uri_template: None,
            },
            capabilities: vec![Capability {
                cap_type: "model".to_string(),
                items: vec!["llama3".to_string()],
            }],
            storage: None,
        }
    }

    #[test]
    fn upsert_creates_delta() {
        let mut reg = GardenRegistryInner::default();
        let tool = sample_tool("ollama", "offering", "stone-a");

        let delta = reg.upsert(tool, EntryOrigin::Local);
        assert!(delta.is_some());
        let d = delta.unwrap();
        assert_eq!(d.kind, ToolDeltaKind::Upsert);
        assert_eq!(d.cursor, 1);
        assert_eq!(reg.entries.len(), 1);
    }

    #[test]
    fn upsert_same_content_no_delta() {
        let mut reg = GardenRegistryInner::default();
        let tool = sample_tool("ollama", "offering", "stone-a");

        reg.upsert(tool.clone(), EntryOrigin::Local);
        let delta = reg.upsert(tool, EntryOrigin::Local);
        assert!(delta.is_none());
        assert_eq!(reg.current_cursor(), 1); // no increment
    }

    #[test]
    fn upsert_changed_content_emits_delta() {
        let mut reg = GardenRegistryInner::default();
        let tool = sample_tool("ollama", "offering", "stone-a");
        reg.upsert(tool, EntryOrigin::Local);

        let mut updated = sample_tool("ollama", "offering", "stone-a");
        updated.service.status = "degraded".to_string();
        let delta = reg.upsert(updated, EntryOrigin::Local);
        assert!(delta.is_some());
        assert_eq!(reg.current_cursor(), 2);
    }

    #[test]
    fn remove_returns_delta() {
        let mut reg = GardenRegistryInner::default();
        let tool = sample_tool("ollama", "offering", "stone-a");
        reg.upsert(tool, EntryOrigin::Local);

        let key = build_tool_key("stone-a", "ollama", "offering");
        let delta = reg.remove(&key);
        assert!(delta.is_some());
        assert_eq!(delta.unwrap().kind, ToolDeltaKind::Remove);
        assert!(reg.entries.is_empty());
    }

    #[test]
    fn remove_nonexistent_returns_none() {
        let mut reg = GardenRegistryInner::default();
        assert!(reg.remove("nonexistent").is_none());
    }

    #[test]
    fn reconcile_local_adds_and_removes() {
        let mut reg = GardenRegistryInner::default();

        let tool = sample_tool("ollama", "offering", "stone-a");
        let deltas = reg.reconcile_local("stone-a", vec![tool], EntryOrigin::Local);
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].kind, ToolDeltaKind::Upsert);

        let deltas = reg.reconcile_local("stone-a", vec![], EntryOrigin::Local);
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].kind, ToolDeltaKind::Remove);
    }

    #[test]
    fn reconcile_does_not_remove_other_origins() {
        let mut reg = GardenRegistryInner::default();

        let local = sample_tool("ollama", "offering", "stone-a");
        reg.upsert(local, EntryOrigin::Local);

        let announced = sample_tool("mongodb", "orchestrator", "stone-b");
        reg.upsert(
            announced,
            EntryOrigin::Announced {
                stone_id: "stone-b".to_string(),
            },
        );

        // Reconcile local with empty → removes offering, keeps announced
        let deltas = reg.reconcile_local("stone-a", vec![], EntryOrigin::Local);
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].fqid, "ollama");
        assert_eq!(reg.entries.len(), 1); // announced entry remains
    }

    #[test]
    fn remove_stone_removes_all_origins() {
        let mut reg = GardenRegistryInner::default();

        let tool1 = sample_tool("ollama", "offering", "stone-a");
        let tool2 = sample_tool("mongodb", "orchestrator", "stone-a");
        reg.upsert(tool1, EntryOrigin::Local);
        reg.upsert(
            tool2,
            EntryOrigin::Announced {
                stone_id: "stone-a".to_string(),
            },
        );

        let deltas = reg.remove_stone("stone-a");
        assert_eq!(deltas.len(), 2);
        assert!(reg.entries.is_empty());
    }

    #[test]
    fn snapshot_sorts_by_category_priority() {
        let mut reg = GardenRegistryInner::default();

        let offering = sample_tool("mongodb", "offering", "stone-a");
        let orchestrator = sample_tool("mongodb", "orchestrator", "stone-b");

        reg.upsert(offering, EntryOrigin::Local);
        reg.upsert(
            orchestrator,
            EntryOrigin::Announced {
                stone_id: "stone-b".to_string(),
            },
        );

        let (_, tools) = reg.snapshot(&ToolQuery::default());
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].tool.category, "orchestrator");
        assert_eq!(tools[1].tool.category, "offering");
    }

    #[test]
    fn snapshot_exact_fqid_first_then_alphabetical() {
        let mut reg = GardenRegistryInner::default();

        // Insert in deliberately wrong order.
        reg.upsert(
            sample_tool("mongodb:staging", "offering", "stone-c"),
            EntryOrigin::Local,
        );
        reg.upsert(
            sample_tool("mongodb", "offering", "stone-b"),
            EntryOrigin::Local,
        );
        reg.upsert(
            sample_tool("mongodb:prod", "offering", "stone-a"),
            EntryOrigin::Local,
        );

        let query = ToolQuery {
            fqid: Some("mongodb".to_string()),
            ..Default::default()
        };
        let (_, tools) = reg.snapshot(&query);
        assert_eq!(tools.len(), 3);
        // Exact fqid match pinned first.
        assert_eq!(tools[0].fqid, "mongodb");
        // Remaining sorted alphabetically by fqid.
        assert_eq!(tools[1].fqid, "mongodb:prod");
        assert_eq!(tools[2].fqid, "mongodb:staging");
    }

    #[test]
    fn snapshot_alphabetical_tiebreak_by_stone() {
        let mut reg = GardenRegistryInner::default();

        reg.upsert(
            sample_tool("redis", "offering", "stone-z"),
            EntryOrigin::Local,
        );
        reg.upsert(
            sample_tool("redis", "offering", "stone-a"),
            EntryOrigin::Local,
        );

        let (_, tools) = reg.snapshot(&ToolQuery::default());
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].stone.name, "stone-a");
        assert_eq!(tools[1].stone.name, "stone-z");
    }

    #[test]
    fn remote_beacon_merge() {
        let mut reg = GardenRegistryInner::default();

        let mut tool = sample_tool("ollama", "offering", "stone-b");
        tool.stone.id = "stone-b".to_string();

        let beacon = ToolsBeacon {
            stone_id: "stone-b".to_string(),
            stone_name: "stone-b".to_string(),
            endpoint: "http://stone-b:7185".to_string(),
            deltas: vec![ToolDelta {
                event_id: "evt-1".to_string(),
                cursor: 1,
                timestamp: Utc::now(),
                kind: ToolDeltaKind::Upsert,
                fqid: "ollama".to_string(),
                tool_key: build_tool_key("stone-b", "ollama", "offering"),
                revision: 1,
                tool: Some(tool),
            }],
            timestamp: Utc::now(),
            snapshot: false,
        };

        let applied = reg.apply_remote_beacon(&beacon);
        assert_eq!(applied.len(), 1);
        assert_eq!(applied[0].kind, ToolDeltaKind::Upsert);

        let entry = reg.entries.values().next().unwrap();
        assert_eq!(
            entry.origin,
            EntryOrigin::Announced {
                stone_id: "stone-b".to_string(),
            }
        );
    }

    #[test]
    fn storage_queries() {
        use garden_common::tools::StorageMetadata;

        let mut reg = GardenRegistryInner::default();

        let mut bank = sample_tool("seed-clear-valley", "storage", "stone-a");
        bank.tool.tool_type = "seed-bank".to_string();
        bank.storage = Some(StorageMetadata {
            replica_set_id: String::new(),
            replica_set_name: String::new(),
            role: Some("primary".to_string()),
            capacity_bytes: 1_000_000_000,
            used_bytes: 500_000_000,
            visibility: "open".to_string(),
            encrypted: false,
            pin_id: None,
            protocols: vec!["s3".to_string(), "storage".to_string()],
            roles: vec!["seed-bank".to_string()],
        });
        reg.upsert(bank, EntryOrigin::Local);

        let mut bank2 = sample_tool("seed-clear-valley", "storage", "stone-b");
        bank2.tool.tool_type = "seed-bank".to_string();
        bank2.storage = Some(StorageMetadata {
            replica_set_id: String::new(),
            replica_set_name: String::new(),
            role: Some("dormant".to_string()),
            capacity_bytes: 1_000_000_000,
            used_bytes: 500_000_000,
            visibility: "open".to_string(),
            encrypted: false,
            pin_id: None,
            protocols: vec!["s3".to_string()],
            roles: vec!["seed-bank".to_string()],
        });
        reg.upsert(
            bank2,
            EntryOrigin::Announced {
                stone_id: "stone-b".to_string(),
            },
        );

        assert_eq!(reg.storage_entries().len(), 2);
        assert_eq!(reg.storage_by_name("seed-clear-valley").len(), 2);

        let primary = reg.storage_primary("seed-clear-valley");
        assert!(primary.is_some());
        assert_eq!(primary.unwrap().tool.stone.id, "stone-a");

        // S3 gateways
        assert_eq!(reg.find_s3_gateways().len(), 2);

        // Route to primary (excluding stone-a → should get stone-b)
        let route = reg.route_to_primary("seed-clear-valley", "stone-a");
        assert!(route.is_some());
        let (sid, _, _) = route.unwrap();
        assert_eq!(sid, "stone-b");

        // Route to primary (excluding stone-b → should get stone-a as Primary)
        let route = reg.route_to_primary("seed-clear-valley", "stone-b");
        assert!(route.is_some());
        let (sid, _, _) = route.unwrap();
        assert_eq!(sid, "stone-a");

        // Count
        assert_eq!(reg.storage_count(), 2);
        assert_eq!(reg.storage_stone_count(), 2);
    }

    #[test]
    fn delta_history_respects_limit() {
        let mut reg = GardenRegistryInner {
            history_limit: 3,
            ..Default::default()
        };

        for i in 0..5 {
            let mut tool = sample_tool("ollama", "offering", "stone-a");
            tool.service.status = format!("status-{}", i);
            reg.upsert(tool, EntryOrigin::Local);
        }

        assert_eq!(reg.history.len(), 3);
        assert_eq!(reg.history.front().unwrap().cursor, 3);
        assert_eq!(reg.history.back().unwrap().cursor, 5);
    }
}
