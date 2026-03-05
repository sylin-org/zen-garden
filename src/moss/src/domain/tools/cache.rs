use chrono::Utc;
use garden_common::tools::{
    build_tool_key, GardenTool, ToolDelta, ToolDeltaKind,
    ToolsBeacon,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;

// ToolQuery is now defined in garden_registry.rs (TOOLS-0003).
pub use crate::domain::garden_registry::ToolQuery;

const DEFAULT_HISTORY_LIMIT: usize = 4096;

pub type ToolsCache = Arc<RwLock<ToolsCacheInner>>;

pub fn new_tools_cache() -> ToolsCache {
    Arc::new(RwLock::new(ToolsCacheInner::default()))
}

#[derive(Debug)]
pub struct ToolsCacheInner {
    /// Keyed by tool_key: `"{stone_id}:{fqid}:{category}"`.
    tools: BTreeMap<String, GardenTool>,
    cursor: u64,
    history_limit: usize,
    history: VecDeque<ToolDelta>,
}

impl Default for ToolsCacheInner {
    fn default() -> Self {
        Self {
            tools: BTreeMap::new(),
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

    pub fn snapshot(&self, query: &ToolQuery) -> (u64, Vec<GardenTool>) {
        let mut tools: Vec<GardenTool> = self
            .tools
            .values()
            .filter(|tool| query.matches_tool(tool))
            .cloned()
            .collect();

        // Sort: orchestrators first, then offerings, then storage.
        tools.sort_by_key(|t| t.category_priority());

        (self.cursor, tools)
    }

    pub fn deltas_since(&self, since_cursor: u64, query: &ToolQuery) -> Vec<ToolDelta> {
        self.history
            .iter()
            .filter(|delta| delta.cursor > since_cursor && query.matches_delta(delta))
            .cloned()
            .collect()
    }

    /// Reconcile a fresh batch of local tools against what's cached.
    /// Returns deltas for anything that changed (upserts + removals of stale entries).
    pub fn reconcile_local(
        &mut self,
        local_stone_id: &str,
        incoming: Vec<GardenTool>,
    ) -> Vec<ToolDelta> {
        let mut applied = Vec::new();
        let mut seen_keys = BTreeSet::new();

        for tool in incoming {
            let key = build_tool_key(&tool.stone.id, &tool.fqid, &tool.tool.category);
            seen_keys.insert(key.clone());
            if let Some(delta) = self.local_upsert(key, tool) {
                applied.push(delta);
            }
        }

        // Remove stale entries belonging to this stone that weren't in the incoming batch.
        let stale: Vec<String> = self
            .tools
            .iter()
            .filter(|(_, tool)| tool.stone.id == local_stone_id)
            .filter(|(key, _)| !seen_keys.contains(*key))
            .map(|(key, _)| key.clone())
            .collect();

        for key in stale {
            if let Some(delta) = self.local_remove(&key) {
                applied.push(delta);
            }
        }

        applied
    }

    pub fn remove_stone_tools(&mut self, stone_id: &str) -> Vec<ToolDelta> {
        let stale: Vec<String> = self
            .tools
            .iter()
            .filter(|(_, tool)| tool.stone.id == stone_id)
            .map(|(key, _)| key.clone())
            .collect();

        let mut removed = Vec::new();
        for key in stale {
            if let Some(delta) = self.local_remove(&key) {
                removed.push(delta);
            }
        }
        removed
    }

    pub fn local_snapshot_for_beacon(&self, stone_id: &str) -> Vec<ToolDelta> {
        self.tools
            .iter()
            .filter(|(_, tool)| tool.stone.id == stone_id)
            .map(|(key, tool)| ToolDelta {
                event_id: garden_common::utils::ids::generate_guidv7(),
                cursor: self.cursor,
                timestamp: Utc::now(),
                kind: ToolDeltaKind::Upsert,
                fqid: tool.fqid.clone(),
                tool_key: key.clone(),
                revision: 0,
                tool: Some(tool.clone()),
            })
            .collect()
    }

    pub fn apply_remote_beacon(&mut self, beacon: &ToolsBeacon) -> Vec<ToolDelta> {
        let mut applied = Vec::new();

        for incoming in &beacon.deltas {
            match incoming.kind {
                ToolDeltaKind::Upsert => {
                    let Some(tool) = incoming.tool.clone() else {
                        continue;
                    };

                    let key = build_tool_key(&tool.stone.id, &tool.fqid, &tool.tool.category);

                    let should_apply = self
                        .tools
                        .get(&key)
                        .map(|_existing| incoming.revision > 0)
                        .unwrap_or(true);

                    if !should_apply {
                        continue;
                    }

                    self.tools.insert(key.clone(), tool.clone());

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
                    if self.tools.remove(key).is_some() {
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

        applied
    }

    fn local_upsert(&mut self, key: String, tool: GardenTool) -> Option<ToolDelta> {
        let previous = self.tools.get(&key);

        if let Some(prev) = previous {
            if tool_equivalent(prev, &tool) {
                return None;
            }
        }

        let revision = previous.map(|_| 1u64).unwrap_or(1);

        self.tools.insert(key.clone(), tool.clone());

        Some(self.append_history(ToolDelta {
            event_id: garden_common::utils::ids::generate_guidv7(),
            cursor: 0,
            timestamp: Utc::now(),
            kind: ToolDeltaKind::Upsert,
            fqid: tool.fqid.clone(),
            tool_key: key,
            revision,
            tool: Some(tool),
        }))
    }

    fn local_remove(&mut self, key: &str) -> Option<ToolDelta> {
        let existing = self.tools.remove(key)?;
        let delta = ToolDelta {
            event_id: garden_common::utils::ids::generate_guidv7(),
            cursor: 0,
            timestamp: Utc::now(),
            kind: ToolDeltaKind::Remove,
            fqid: existing.fqid.clone(),
            tool_key: key.to_string(),
            revision: 1,
            tool: None,
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

fn tool_equivalent(lhs: &GardenTool, rhs: &GardenTool) -> bool {
    lhs.fqid == rhs.fqid
        && lhs.tool == rhs.tool
        && lhs.stone == rhs.stone
        && lhs.service == rhs.service
        && lhs.capabilities == rhs.capabilities
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use garden_common::tools::{Capability, ServiceInfo, Stone, ToolIdentity};

    fn sample_tool(fqid: &str, category: &str) -> GardenTool {
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
            },
            stone: Stone {
                id: "stone-a".to_string(),
                name: "stone-a".to_string(),
                endpoint: "http://192.168.1.100:7185".to_string(),
            },
            service: ServiceInfo {
                status: "running".to_string(),
                ready: true,
                protocol: tool_type.to_string(),
                uris: Vec::new(),
            },
            capabilities: vec![Capability {
                cap_type: "model".to_string(),
                items: vec!["llama3".to_string()],
            }],
            storage: None,
        }
    }

    #[test]
    fn local_reconcile_creates_upsert_then_remove() {
        let mut cache = ToolsCacheInner::default();
        let tool = sample_tool("ollama", "offering");
        let deltas = cache.reconcile_local("stone-a", vec![tool.clone()]);
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].kind, ToolDeltaKind::Upsert);

        let deltas = cache.reconcile_local("stone-a", vec![]);
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].kind, ToolDeltaKind::Remove);
    }

    #[test]
    fn remote_beacon_applies_upsert() {
        let mut cache = ToolsCacheInner::default();

        let mut tool = sample_tool("ollama", "offering");
        tool.stone.id = "stone-b".to_string();
        tool.stone.name = "stone-b".to_string();

        let beacon = ToolsBeacon {
            stone_id: "stone-b".to_string(),
            stone_name: "stone-b".to_string(),
            endpoint: "http://stone-b.local:7185".to_string(),
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
        };

        let applied = cache.apply_remote_beacon(&beacon);
        assert_eq!(applied.len(), 1);
        assert_eq!(applied[0].kind, ToolDeltaKind::Upsert);
    }

    #[test]
    fn fqid_filter_matches_tool() {
        let tool = sample_tool("ollama::adopted", "offering");

        let query = ToolQuery {
            fqid: Some("ollama".to_string()),
            ..Default::default()
        };
        // bare "ollama" matches tool_type "ollama"
        assert!(query.matches_tool(&tool));

        let query_exact = ToolQuery {
            fqid: Some("ollama::adopted".to_string()),
            ..Default::default()
        };
        assert!(query_exact.matches_tool(&tool));

        // V1 legacy query also normalizes and matches
        let query_v1 = ToolQuery {
            fqid: Some("ollama:adopted".to_string()),
            ..Default::default()
        };
        assert!(query_v1.matches_tool(&tool));

        let query_miss = ToolQuery {
            fqid: Some("redis".to_string()),
            ..Default::default()
        };
        assert!(!query_miss.matches_tool(&tool));
    }

    #[test]
    fn category_filter_matches() {
        let tool = sample_tool("mongodb", "orchestrator");

        let query = ToolQuery {
            category: Some("orchestrator".to_string()),
            ..Default::default()
        };
        assert!(query.matches_tool(&tool));

        let query_miss = ToolQuery {
            category: Some("offering".to_string()),
            ..Default::default()
        };
        assert!(!query_miss.matches_tool(&tool));
    }

    #[test]
    fn snapshot_sorts_by_category_priority() {
        let mut cache = ToolsCacheInner::default();

        let offering = sample_tool("mongodb", "offering");
        let mut orchestrator = sample_tool("mongodb", "orchestrator");
        orchestrator.stone.id = "stone-b".to_string();
        orchestrator.stone.name = "stone-b".to_string();

        cache.reconcile_local("stone-a", vec![offering]);
        // Manually insert orchestrator on different stone
        let key = build_tool_key("stone-b", "mongodb", "orchestrator");
        cache.local_upsert(key, orchestrator);

        let (_, tools) = cache.snapshot(&ToolQuery::default());
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].tool.category, "orchestrator");
        assert_eq!(tools[1].tool.category, "offering");
    }

    #[test]
    fn capability_filter_matches() {
        let tool = sample_tool("ollama", "offering");

        let query = ToolQuery {
            capabilities: vec![CapabilitySelector {
                cap_type: "model".to_string(),
                item: "llama3".to_string(),
            }],
            ..Default::default()
        };
        assert!(query.matches_tool(&tool));

        let query_miss = ToolQuery {
            capabilities: vec![CapabilitySelector {
                cap_type: "model".to_string(),
                item: "gpt-4".to_string(),
            }],
            ..Default::default()
        };
        assert!(!query_miss.matches_tool(&tool));
    }
}
