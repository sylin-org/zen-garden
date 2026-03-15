//! FQN Handler domain — registry of processes that handle FIND requests for a given FQN
//! (ARCH-0004, Phase 3).
//!
//! An FQN handler is an external process (e.g. `garden-ollama`, `garden-mongodb`) that
//! registers via `PUT /api/v1/garden/gateway/{offering}` to claim handler responsibility
//! for an [`OfferingFqn`]. When a FIND request arrives for that FQN, Moss forwards it to
//! the registered handler.
//!
//! Registrations are ephemeral — handlers refresh every 30 seconds; entries expire after
//! 60 seconds without a refresh.
//!
//! ## Domain paths
//! - `state.fqn_handler` — this stone's registered FQN handlers (local, authoritative).
//! - `state.current.fqn_handler` — same store; garden-wide aggregate introduced in Phase 9.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use garden_common::tools::{build_tool_key, GardenTool, ToolDelta, ToolDeltaKind};
use garden_common::utils::ids::generate_guidv7;
use tokio::sync::RwLock;

const DEFAULT_TTL_SECS: u64 = 60;

// ── Entry ────────────────────────────────────────────────────────────────────

/// A registered FQN handler entry.
#[derive(Debug, Clone)]
pub struct FqnHandlerEntry {
    /// Full tool data — wire format for propagation and API responses.
    pub tool: GardenTool,
    /// Offering names this handler claims responsibility for.
    pub handler_for: Vec<String>,
    /// When this entry expires (reset on each refresh).
    pub expires_at: Instant,
}

// ── Registry ─────────────────────────────────────────────────────────────────

/// Registry of FQN handlers registered on this stone.
///
/// Keyed by the offering name used in the gateway API path
/// (`PUT /api/v1/garden/gateway/{offering}`).
#[derive(Debug, Default)]
pub struct FqnHandlerRegistry {
    entries: HashMap<String, FqnHandlerEntry>,
}

impl FqnHandlerRegistry {
    // ── Mutations ────────────────────────────────────────────────────────

    /// Register or refresh an FQN handler.
    ///
    /// Returns a `ToolDelta` for SSE propagation if the entry changed or is new.
    /// Returns `None` if the entry is unchanged (content-identical refresh).
    pub fn upsert(
        &mut self,
        offering: &str,
        tool: GardenTool,
        handler_for: Vec<String>,
    ) -> Option<ToolDelta> {
        let expires_at = Instant::now() + Duration::from_secs(DEFAULT_TTL_SECS);

        if let Some(existing) = self.entries.get(offering) {
            if fqn_handler_equivalent(&existing.tool, &tool) {
                // Content unchanged — just refresh TTL silently.
                self.entries.get_mut(offering).unwrap().expires_at = expires_at;
                return None;
            }
        }

        let key = build_tool_key(&tool.stone.id, &tool.fqid, &tool.tool.category);
        self.entries.insert(
            offering.to_string(),
            FqnHandlerEntry {
                tool: tool.clone(),
                handler_for,
                expires_at,
            },
        );

        Some(ToolDelta {
            event_id: generate_guidv7(),
            cursor: 0,
            timestamp: Utc::now(),
            kind: ToolDeltaKind::Upsert,
            fqid: tool.fqid.clone(),
            tool_key: key,
            revision: 1,
            tool: Some(tool),
        })
    }

    /// Deregister an FQN handler by offering name.
    ///
    /// Returns a `ToolDelta` if the entry existed.
    pub fn remove(&mut self, offering: &str, stone_id: &str) -> Option<ToolDelta> {
        let entry = self.entries.remove(offering)?;
        let key = build_tool_key(stone_id, &entry.tool.fqid, &entry.tool.tool.category);
        Some(ToolDelta {
            event_id: generate_guidv7(),
            cursor: 0,
            timestamp: Utc::now(),
            kind: ToolDeltaKind::Remove,
            fqid: entry.tool.fqid,
            tool_key: key,
            revision: 0,
            tool: None,
        })
    }

    /// Remove all expired entries.
    ///
    /// Returns deltas for each removed entry (for SSE propagation).
    pub fn reap_expired(&mut self, stone_id: &str) -> Vec<ToolDelta> {
        let now = Instant::now();
        let expired: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, e)| e.expires_at <= now)
            .map(|(k, _)| k.clone())
            .collect();

        let mut deltas = Vec::new();
        for key in expired {
            tracing::debug!(offering = %key, "fqn_handler: reaping expired entry");
            if let Some(delta) = self.remove(&key, stone_id) {
                deltas.push(delta);
            }
        }
        deltas
    }

    // ── Queries ──────────────────────────────────────────────────────────

    /// Returns true if a handler is registered for the given offering type.
    pub fn handles_offering(&self, offering: &str) -> bool {
        self.entries.values().any(|e| {
            e.handler_for
                .iter()
                .any(|h| h.eq_ignore_ascii_case(offering))
        })
    }

    /// Returns the set of offering names that have registered handlers.
    pub fn handled_offerings(&self) -> HashSet<String> {
        self.entries
            .values()
            .flat_map(|e| e.handler_for.iter().cloned())
            .collect()
    }

    /// All registered entries.
    pub fn entries(&self) -> impl Iterator<Item = &FqnHandlerEntry> {
        self.entries.values()
    }
}

// ── Domain Context ────────────────────────────────────────────────────────────

/// FQN Handler domain context (`state.fqn_handler`).
///
/// Holds the registry of FQN handlers registered on this stone.
#[derive(Clone)]
pub struct FqnHandler {
    pub registry: Arc<RwLock<FqnHandlerRegistry>>,
}

impl Default for FqnHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl FqnHandler {
    pub fn new() -> Self {
        Self {
            registry: Arc::new(RwLock::new(FqnHandlerRegistry::default())),
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn fqn_handler_equivalent(lhs: &GardenTool, rhs: &GardenTool) -> bool {
    lhs.fqid == rhs.fqid
        && lhs.tool == rhs.tool
        && lhs.stone == rhs.stone
        && lhs.service == rhs.service
}
