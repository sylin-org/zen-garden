use chrono::Utc;
use garden_common::tools::{
    CapabilityDelta, CapabilitySelector, ToolDelta, ToolDeltaKind, ToolProjection, ToolState,
    ToolType, ToolsBeacon,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;

const DEFAULT_HISTORY_LIMIT: usize = 4096;

pub type ToolsCache = Arc<RwLock<ToolsCacheInner>>;

pub fn new_tools_cache() -> ToolsCache {
    Arc::new(RwLock::new(ToolsCacheInner::default()))
}

#[derive(Debug, Clone, Default)]
pub struct ToolQuery {
    pub tool_type: Option<ToolType>,
    pub tool_fqid: Option<String>,
    pub state: Option<ToolState>,
    pub capabilities: Vec<CapabilitySelector>,
}

impl ToolQuery {
    pub fn matches_projection(&self, projection: &ToolProjection) -> bool {
        if let Some(tool_type) = self.tool_type {
            if projection.tool_type != tool_type {
                return false;
            }
        }

        if let Some(tool_fqid) = &self.tool_fqid {
            let fqid_matches = projection.tool_fqid.eq_ignore_ascii_case(tool_fqid)
                || projection
                    .aliases
                    .iter()
                    .any(|a| a.eq_ignore_ascii_case(tool_fqid));
            if !fqid_matches {
                return false;
            }
        }

        if let Some(state) = self.state {
            if projection.state != state {
                return false;
            }
        }

        for selector in &self.capabilities {
            let cap_type = selector.cap_type.to_ascii_lowercase();
            let item = selector.item.to_ascii_lowercase();
            let has_item = projection
                .capabilities
                .get(&cap_type)
                .map(|items| {
                    items
                        .iter()
                        .any(|existing| existing.eq_ignore_ascii_case(&item))
                })
                .unwrap_or(false);
            if !has_item {
                return false;
            }
        }

        true
    }

    pub fn matches_delta(&self, delta: &ToolDelta) -> bool {
        if let Some(tool_fqid) = &self.tool_fqid {
            let fqid_matches = delta.tool_fqid.eq_ignore_ascii_case(tool_fqid)
                || delta
                    .projection
                    .as_ref()
                    .map(|p| {
                        p.aliases
                            .iter()
                            .any(|a| a.eq_ignore_ascii_case(tool_fqid))
                    })
                    .unwrap_or(false);
            if !fqid_matches {
                return false;
            }
        }

        match delta.kind {
            ToolDeltaKind::Upsert => delta
                .projection
                .as_ref()
                .map(|projection| self.matches_projection(projection))
                .unwrap_or(false),
            ToolDeltaKind::Remove => {
                // Remove events only carry tool_fqid + tool_uid. Only tool_fqid filtering
                // is meaningful without the removed projection payload.
                (self.tool_type.is_none() && self.state.is_none() && self.capabilities.is_empty())
                    || self.tool_fqid.is_some()
            }
        }
    }
}

#[derive(Debug)]
pub struct ToolsCacheInner {
    projections: BTreeMap<String, ToolProjection>,
    cursor: u64,
    history_limit: usize,
    history: VecDeque<ToolDelta>,
}

impl Default for ToolsCacheInner {
    fn default() -> Self {
        Self {
            projections: BTreeMap::new(),
            cursor: 0,
            history_limit: DEFAULT_HISTORY_LIMIT,
            history: VecDeque::with_capacity(DEFAULT_HISTORY_LIMIT),
        }
    }
}

impl ToolsCacheInner {
    pub fn current_cursor(&self) -> u64 {
        self.cursor
    }

    pub fn cursor_for_event_id(&self, event_id: &str) -> Option<u64> {
        self.history
            .iter()
            .rev()
            .find(|delta| delta.event_id == event_id)
            .map(|delta| delta.cursor)
    }

    pub fn snapshot(&self, query: &ToolQuery) -> (u64, Vec<ToolProjection>) {
        let tools = self
            .projections
            .values()
            .filter(|projection| query.matches_projection(projection))
            .cloned()
            .collect();

        (self.cursor, tools)
    }

    pub fn deltas_since(&self, since_cursor: u64, query: &ToolQuery) -> Vec<ToolDelta> {
        self.history
            .iter()
            .filter(|delta| delta.cursor > since_cursor && query.matches_delta(delta))
            .cloned()
            .collect()
    }

    pub fn reconcile_local(
        &mut self,
        local_stone_id: &str,
        mut local_projections: Vec<ToolProjection>,
    ) -> Vec<ToolDelta> {
        for projection in &mut local_projections {
            normalize_projection(projection);
        }
        local_projections.sort_by(|a, b| a.tool_fqid.cmp(&b.tool_fqid));

        let mut applied = Vec::new();
        let mut seen = BTreeSet::new();
        for projection in local_projections {
            seen.insert(projection.tool_fqid.clone());
            if let Some(delta) = self.local_upsert(projection) {
                applied.push(delta);
            }
        }

        let stale: Vec<String> = self
            .projections
            .iter()
            .filter(|(_, projection)| projection.stone_id == local_stone_id)
            .filter(|(tool_fqid, _)| !seen.contains(*tool_fqid))
            .map(|(tool_fqid, _)| tool_fqid.clone())
            .collect();

        for tool_fqid in stale {
            if let Some(delta) = self.local_remove(&tool_fqid) {
                applied.push(delta);
            }
        }

        applied
    }

    pub fn remove_stone_tools(&mut self, stone_id: &str) -> Vec<ToolDelta> {
        let stale: Vec<String> = self
            .projections
            .iter()
            .filter(|(_, projection)| projection.stone_id == stone_id)
            .map(|(tool_fqid, _)| tool_fqid.clone())
            .collect();

        let mut removed = Vec::new();
        for tool_fqid in stale {
            if let Some(delta) = self.local_remove(&tool_fqid) {
                removed.push(delta);
            }
        }
        removed
    }

    pub fn local_snapshot_for_beacon(&self, stone_id: &str) -> Vec<ToolDelta> {
        self.projections
            .values()
            .filter(|projection| projection.stone_id == stone_id)
            .cloned()
            .map(|projection| ToolDelta {
                event_id: garden_common::utils::ids::generate_guidv7(),
                cursor: self.cursor,
                timestamp: Utc::now(),
                kind: ToolDeltaKind::Upsert,
                tool_fqid: projection.tool_fqid.clone(),
                tool_uid: projection.tool_uid.clone(),
                revision: projection.revision,
                projection: Some(projection),
            })
            .collect()
    }

    pub fn apply_remote_beacon(&mut self, beacon: &ToolsBeacon) -> Vec<ToolDelta> {
        let mut applied = Vec::new();

        for incoming in &beacon.deltas {
            match incoming.kind {
                ToolDeltaKind::Upsert => {
                    let Some(mut projection) = incoming.projection.clone() else {
                        continue;
                    };
                    normalize_projection(&mut projection);

                    let should_apply = self
                        .projections
                        .get(&projection.tool_fqid)
                        .map(|existing| projection.revision > existing.revision)
                        .unwrap_or(true);

                    if !should_apply {
                        continue;
                    }

                    self.projections
                        .insert(projection.tool_fqid.clone(), projection.clone());

                    let delta = self.append_history(ToolDelta {
                        event_id: incoming.event_id.clone(),
                        cursor: 0,
                        timestamp: incoming.timestamp,
                        kind: ToolDeltaKind::Upsert,
                        tool_fqid: projection.tool_fqid.clone(),
                        tool_uid: projection.tool_uid.clone(),
                        revision: projection.revision,
                        projection: Some(projection),
                    });
                    applied.push(delta);
                }
                ToolDeltaKind::Remove => {
                    let should_apply = self
                        .projections
                        .get(&incoming.tool_fqid)
                        .map(|existing| incoming.revision > existing.revision)
                        .unwrap_or(true);
                    if !should_apply {
                        continue;
                    }

                    let removed_uid = self
                        .projections
                        .remove(&incoming.tool_fqid)
                        .map(|p| p.tool_uid)
                        .unwrap_or_else(|| incoming.tool_uid.clone());

                    let delta = self.append_history(ToolDelta {
                        event_id: incoming.event_id.clone(),
                        cursor: 0,
                        timestamp: incoming.timestamp,
                        kind: ToolDeltaKind::Remove,
                        tool_fqid: incoming.tool_fqid.clone(),
                        tool_uid: removed_uid,
                        revision: incoming.revision,
                        projection: None,
                    });
                    applied.push(delta);
                }
            }
        }

        applied
    }

    fn local_upsert(&mut self, mut projection: ToolProjection) -> Option<ToolDelta> {
        normalize_projection(&mut projection);

        let previous = self.projections.get(&projection.tool_fqid).cloned();
        let (revision, capability_revision, capability_delta) = match &previous {
            Some(previous) => {
                let caps_changed = previous.capabilities != projection.capabilities;
                let capability_delta = caps_changed.then(|| {
                    build_capability_delta(&previous.capabilities, &projection.capabilities)
                });
                let capability_revision = if caps_changed {
                    previous.capability_revision.saturating_add(1)
                } else {
                    previous.capability_revision
                };

                (
                    previous.revision.saturating_add(1),
                    capability_revision,
                    capability_delta,
                )
            }
            None => {
                let capability_delta = (!projection.capabilities.is_empty())
                    .then(|| build_capability_delta(&BTreeMap::new(), &projection.capabilities));
                let capability_revision = if projection.capabilities.is_empty() {
                    0
                } else {
                    1
                };
                (1, capability_revision, capability_delta)
            }
        };

        projection.revision = revision;
        projection.capability_revision = capability_revision;
        projection.capability_delta = capability_delta;
        projection.updated_at = Utc::now();

        if let Some(previous) = previous {
            if projection_equivalent(&previous, &projection) {
                return None;
            }
        }

        self.projections
            .insert(projection.tool_fqid.clone(), projection.clone());

        Some(self.append_history(ToolDelta {
            event_id: garden_common::utils::ids::generate_guidv7(),
            cursor: 0,
            timestamp: Utc::now(),
            kind: ToolDeltaKind::Upsert,
            tool_fqid: projection.tool_fqid.clone(),
            tool_uid: projection.tool_uid.clone(),
            revision: projection.revision,
            projection: Some(projection),
        }))
    }

    fn local_remove(&mut self, tool_fqid: &str) -> Option<ToolDelta> {
        let existing = self.projections.remove(tool_fqid)?;
        let delta = ToolDelta {
            event_id: garden_common::utils::ids::generate_guidv7(),
            cursor: 0,
            timestamp: Utc::now(),
            kind: ToolDeltaKind::Remove,
            tool_fqid: existing.tool_fqid.clone(),
            tool_uid: existing.tool_uid.clone(),
            revision: existing.revision.saturating_add(1),
            projection: None,
        };
        Some(self.append_history(delta))
    }

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

fn normalize_projection(projection: &mut ToolProjection) {
    projection.aliases = projection
        .aliases
        .iter()
        .map(|alias| alias.trim().to_ascii_lowercase())
        .filter(|alias| !alias.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    let mut normalized_caps = BTreeMap::new();
    for (cap_type, items) in &projection.capabilities {
        let key = cap_type.trim().to_ascii_lowercase();
        if key.is_empty() {
            continue;
        }
        let values: Vec<String> = items
            .iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if !values.is_empty() {
            normalized_caps.insert(key, values);
        }
    }
    projection.capabilities = normalized_caps;
}

fn projection_equivalent(lhs: &ToolProjection, rhs: &ToolProjection) -> bool {
    lhs.tool_fqid == rhs.tool_fqid
        && lhs.tool_uid == rhs.tool_uid
        && lhs.tool_type == rhs.tool_type
        && lhs.state == rhs.state
        && lhs.ready == rhs.ready
        && lhs.stone_id == rhs.stone_id
        && lhs.stone_name == rhs.stone_name
        && lhs.aliases == rhs.aliases
        && lhs.connection == rhs.connection
        && lhs.capabilities == rhs.capabilities
        && lhs.job_id == rhs.job_id
        && lhs.request_id == rhs.request_id
}

fn build_capability_delta(
    previous: &BTreeMap<String, Vec<String>>,
    next: &BTreeMap<String, Vec<String>>,
) -> CapabilityDelta {
    let mut added = BTreeMap::new();
    let mut removed = BTreeMap::new();

    for (cap_type, items) in next {
        let previous_items: BTreeSet<String> = previous
            .get(cap_type)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect();
        let next_items: BTreeSet<String> = items.iter().cloned().collect();
        let added_items: Vec<String> = next_items.difference(&previous_items).cloned().collect();
        if !added_items.is_empty() {
            added.insert(cap_type.clone(), added_items);
        }
    }

    for (cap_type, items) in previous {
        let next_items: BTreeSet<String> = next
            .get(cap_type)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect();
        let previous_items: BTreeSet<String> = items.iter().cloned().collect();
        let removed_items: Vec<String> = previous_items.difference(&next_items).cloned().collect();
        if !removed_items.is_empty() {
            removed.insert(cap_type.clone(), removed_items);
        }
    }

    CapabilityDelta { added, removed }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn sample_projection(tool_fqid: &str, revision: u64) -> ToolProjection {
        ToolProjection {
            tool_fqid: tool_fqid.to_string(),
            tool_uid: format!("uid-{}", tool_fqid),
            tool_type: ToolType::Offering,
            state: ToolState::Ready,
            ready: true,
            revision,
            stone_id: "stone-a".to_string(),
            stone_name: "stone-a".to_string(),
            aliases: vec![],
            connection: None,
            capabilities: BTreeMap::from([("model".to_string(), vec!["llama3".to_string()])]),
            capability_revision: 0,
            capability_delta: None,
            job_id: None,
            request_id: None,
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn local_reconcile_creates_upsert_then_remove() {
        let mut cache = ToolsCacheInner::default();
        let projection = sample_projection("offering:ollama", 0);
        let deltas = cache.reconcile_local("stone-a", vec![projection.clone()]);
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].kind, ToolDeltaKind::Upsert);

        let deltas = cache.reconcile_local("stone-a", vec![]);
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].kind, ToolDeltaKind::Remove);
    }

    #[test]
    fn remote_beacon_ignores_older_revisions() {
        let mut cache = ToolsCacheInner::default();
        let projection = sample_projection("offering:ollama", 0);
        cache.reconcile_local("stone-a", vec![projection]);

        let mut remote_projection = sample_projection("offering:ollama", 1);
        remote_projection.revision = 1;
        remote_projection.stone_id = "stone-b".to_string();
        remote_projection.stone_name = "stone-b".to_string();

        let beacon = ToolsBeacon {
            stone_id: "stone-b".to_string(),
            stone_name: "stone-b".to_string(),
            endpoint: "http://stone-b.local:7185".to_string(),
            deltas: vec![ToolDelta {
                event_id: "evt-1".to_string(),
                cursor: 1,
                timestamp: Utc::now(),
                kind: ToolDeltaKind::Upsert,
                tool_fqid: remote_projection.tool_fqid.clone(),
                tool_uid: remote_projection.tool_uid.clone(),
                revision: 1,
                projection: Some(remote_projection.clone()),
            }],
            timestamp: Utc::now(),
        };

        let applied = cache.apply_remote_beacon(&beacon);
        assert!(applied.is_empty());

        remote_projection.revision = 3;
        let beacon = ToolsBeacon {
            stone_id: "stone-b".to_string(),
            stone_name: "stone-b".to_string(),
            endpoint: "http://stone-b.local:7185".to_string(),
            deltas: vec![ToolDelta {
                event_id: "evt-2".to_string(),
                cursor: 2,
                timestamp: Utc::now(),
                kind: ToolDeltaKind::Upsert,
                tool_fqid: remote_projection.tool_fqid.clone(),
                tool_uid: remote_projection.tool_uid.clone(),
                revision: 3,
                projection: Some(remote_projection),
            }],
            timestamp: Utc::now(),
        };

        let applied = cache.apply_remote_beacon(&beacon);
        assert_eq!(applied.len(), 1);
    }

    #[test]
    fn tool_fqid_filter_matches_aliases() {
        let mut projection = sample_projection("offering:ollama:adopted", 1);
        projection.aliases = vec![
            "offering:ollama".to_string(),
            "offering:ollama:adopted".to_string(),
        ];

        let query = ToolQuery {
            tool_fqid: Some("offering:ollama".to_string()),
            ..Default::default()
        };

        // Should match via alias even though FQID is "offering:ollama:adopted"
        assert!(query.matches_projection(&projection));

        // Should also match the exact FQID
        let query_exact = ToolQuery {
            tool_fqid: Some("offering:ollama:adopted".to_string()),
            ..Default::default()
        };
        assert!(query_exact.matches_projection(&projection));

        // Should NOT match an unrelated FQID
        let query_miss = ToolQuery {
            tool_fqid: Some("offering:redis".to_string()),
            ..Default::default()
        };
        assert!(!query_miss.matches_projection(&projection));
    }

    #[test]
    fn tool_fqid_delta_filter_matches_aliases() {
        let mut projection = sample_projection("offering:ollama:adopted", 1);
        projection.aliases = vec![
            "offering:ollama".to_string(),
            "offering:ollama:adopted".to_string(),
        ];

        let delta = ToolDelta {
            event_id: "evt-alias".to_string(),
            cursor: 1,
            timestamp: Utc::now(),
            kind: ToolDeltaKind::Upsert,
            tool_fqid: "offering:ollama:adopted".to_string(),
            tool_uid: "uid-test".to_string(),
            revision: 1,
            projection: Some(projection),
        };

        let query = ToolQuery {
            tool_fqid: Some("offering:ollama".to_string()),
            ..Default::default()
        };

        assert!(query.matches_delta(&delta));
    }
}
