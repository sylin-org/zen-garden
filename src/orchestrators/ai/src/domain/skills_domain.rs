//! Skills domain — skill registry + workflow jobs (ORCH-0020).
//!
//! Owns skill definitions and workflow job tracking.
//! Publishes snapshots via watch.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{watch, Mutex};

use super::skill::*;

// ── Snapshot ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SkillsSnapshot {
    pub skills: Arc<Vec<SkillDefinition>>,
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
    workflow_jobs: HashMap<String, WorkflowJob>,
}

impl SkillsDomain {
    pub fn new(tx: watch::Sender<Arc<SkillsSnapshot>>) -> Self {
        Self {
            state: Mutex::new(SkillsState {
                registry: SkillRegistry::new(),
                workflow_jobs: HashMap::new(),
            }),
            tx,
        }
    }

    pub fn snapshot(&self) -> watch::Ref<'_, Arc<SkillsSnapshot>> {
        self.tx.borrow()
    }

    // ── Skill Registry ─────────────────────────────────────────

    pub async fn register(&self, skill: SkillDefinition) {
        let mut state = self.state.lock().await;
        state.registry.register(skill);
        self.publish(&state);
    }

    pub async fn update_status(&self, name: &str, status: SkillStatus) {
        let mut state = self.state.lock().await;
        if let Some(s) = state.registry.get_mut(name) {
            s.status = status;
            self.publish(&state);
        }
    }

    /// Get a skill definition (brief lock, no publish).
    pub async fn get_skill(&self, name: &str) -> Option<SkillDefinition> {
        let state = self.state.lock().await;
        state.registry.get(name).cloned()
    }

    /// Check if a skill is in a given status.
    pub async fn has_status(&self, name: &str, status: SkillStatus) -> bool {
        let state = self.state.lock().await;
        state
            .registry
            .get(name)
            .map(|s| s.status == status)
            .unwrap_or(false)
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

    fn publish(&self, state: &SkillsState) {
        let skills: Vec<_> = state.registry.list().into_iter().cloned().collect();
        let snapshot = Arc::new(SkillsSnapshot {
            skills: Arc::new(skills),
            workflow_jobs: Arc::new(state.workflow_jobs.clone()),
        });
        let _ = self.tx.send(snapshot);
    }
}
