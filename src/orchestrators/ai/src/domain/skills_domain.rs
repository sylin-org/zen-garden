//! Skills domain — skill registry + workflow jobs + readiness (ORCH-0021).
//!
//! Skills are static singletons. Availability is computed from instance
//! readiness, not stored on the definition.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{watch, Mutex};

use super::skill::*;

// ── Snapshot ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SkillsSnapshot {
    pub skills: Arc<Vec<SkillView>>,
    pub workflow_jobs: Arc<HashMap<String, WorkflowJob>>,
}

impl SkillsSnapshot {
    pub fn empty() -> Self {
        Self {
            skills: Arc::new(Vec::new()),
            workflow_jobs: Arc::new(HashMap::new()),
        }
    }
}

// ── Domain ─────────────────────────────────────────────────────

pub struct SkillsDomain {
    state: Mutex<SkillsState>,
    tx: watch::Sender<Arc<SkillsSnapshot>>,
}

struct SkillsState {
    registry: SkillRegistry,
    /// Per (skill_name, endpoint) readiness.
    readiness: HashMap<String, HashMap<String, SkillInstanceView>>,
    /// Skills currently being provisioned — (skill_name, endpoint) dedup set.
    provisioning: HashSet<(String, String)>,
    workflow_jobs: HashMap<String, WorkflowJob>,
}

impl SkillsDomain {
    pub fn new(tx: watch::Sender<Arc<SkillsSnapshot>>) -> Self {
        Self {
            state: Mutex::new(SkillsState {
                registry: SkillRegistry::new(),
                readiness: HashMap::new(),
                provisioning: HashSet::new(),
                workflow_jobs: HashMap::new(),
            }),
            tx,
        }
    }

    pub fn snapshot(&self) -> watch::Ref<'_, Arc<SkillsSnapshot>> {
        self.tx.borrow()
    }

    // ── Skill Registry (singletons) ────────────────────────────

    /// Register or update a skill definition.
    pub async fn register(&self, skill: SkillDefinition) {
        let mut state = self.state.lock().await;
        state.registry.register(skill);
        self.publish(&state);
    }

    /// Unregister a skill (removed from disk).
    pub async fn unregister(&self, name: &str) {
        let mut state = self.state.lock().await;
        if state.registry.remove(name).is_some() {
            state.readiness.remove(name);
            self.publish(&state);
        }
    }

    /// Get a skill definition (brief lock).
    pub async fn get_skill(&self, name: &str) -> Option<SkillDefinition> {
        let state = self.state.lock().await;
        state.registry.get(name).cloned()
    }

    // ── Instance Readiness ─────────────────────────────────────

    /// Record readiness for a skill on a specific instance.
    pub async fn set_readiness(
        &self,
        skill_name: &str,
        endpoint: &str,
        view: SkillInstanceView,
    ) {
        let mut state = self.state.lock().await;
        state
            .readiness
            .entry(skill_name.to_string())
            .or_default()
            .insert(endpoint.to_string(), view);
        self.publish(&state);
    }

    /// Check if a skill+endpoint is currently being provisioned.
    pub async fn is_provisioning(&self, skill_name: &str, endpoint: &str) -> bool {
        let state = self.state.lock().await;
        state
            .provisioning
            .contains(&(skill_name.to_string(), endpoint.to_string()))
    }

    /// Mark a skill+endpoint as provisioning (prevents duplicate spawns).
    pub async fn mark_provisioning(&self, skill_name: &str, endpoint: &str) {
        let mut state = self.state.lock().await;
        state
            .provisioning
            .insert((skill_name.to_string(), endpoint.to_string()));
    }

    /// Clear provisioning mark (on completion or failure).
    pub async fn clear_provisioning(&self, skill_name: &str, endpoint: &str) {
        let mut state = self.state.lock().await;
        state
            .provisioning
            .remove(&(skill_name.to_string(), endpoint.to_string()));
    }

    // ── Workflow Jobs ──────────────────────────────────────────

    pub async fn submit_job(&self, job: WorkflowJob) {
        let mut state = self.state.lock().await;
        state.workflow_jobs.insert(job.id.clone(), job);
        self.publish(&state);
    }

    pub async fn get_job(&self, id: &str) -> Option<WorkflowJob> {
        let state = self.state.lock().await;
        state.workflow_jobs.get(id).cloned()
    }

    // ── Publish ────────────────────────────────────────────────

    fn publish(&self, state: &SkillsState) {
        let skills: Vec<SkillView> = state
            .registry
            .list()
            .into_iter()
            .map(|def| {
                let instances: Vec<SkillInstanceView> = state
                    .readiness
                    .get(&def.name)
                    .map(|map| map.values().cloned().collect())
                    .unwrap_or_default();

                let available = instances.iter().any(|i| i.ready);

                SkillView {
                    definition: def.clone(),
                    available,
                    instances,
                }
            })
            .collect();

        let snapshot = Arc::new(SkillsSnapshot {
            skills: Arc::new(skills),
            workflow_jobs: Arc::new(state.workflow_jobs.clone()),
        });

        // Use send_modify instead of send — send() silently drops the value
        // when there are no receivers, but send_modify always updates.
        self.tx.send_modify(|current| {
            *current = snapshot;
        });
    }
}
