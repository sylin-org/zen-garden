//! Action queue — persisted pending mutations for a clustered service.
//!
//! When a membership change is requested but the target is unreachable
//! or the operation fails, the action is queued for retry. The queue
//! persists to disk so it survives orchestrator restarts.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Pending Action ────────────────────────────────────────────────────

/// A pending action queued for eventual execution.
///
/// Generic over the action type — each adapter defines its own action
/// variants (e.g. `RemoveMember`, `AddMember`, `Reconfig`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingAction<A: Serialize> {
    /// The adapter-specific action payload.
    pub action: A,
    /// When the action was first requested.
    pub requested_at: DateTime<Utc>,
    /// How many times execution has been attempted.
    pub attempts: u32,
    /// Last attempt timestamp (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_attempt: Option<DateTime<Utc>>,
}

impl<A: Serialize> PendingAction<A> {
    pub fn new(action: A) -> Self {
        Self {
            action,
            requested_at: Utc::now(),
            attempts: 0,
            last_attempt: None,
        }
    }

    /// Mark an attempt (increment counter, update timestamp).
    pub fn mark_attempt(&mut self) {
        self.attempts += 1;
        self.last_attempt = Some(Utc::now());
    }
}

// ── Action Queue ──────────────────────────────────────────────────────

/// Persisted queue of pending actions.
pub struct ActionQueue<A: Serialize + for<'de> Deserialize<'de>> {
    actions: Vec<PendingAction<A>>,
    data_dir: String,
    filename: String,
}

impl<A: Serialize + for<'de> Deserialize<'de> + Clone + std::fmt::Debug> ActionQueue<A> {
    /// Create an action queue backed by a JSON file in `data_dir`.
    pub fn new(data_dir: impl Into<String>, filename: impl Into<String>) -> Self {
        Self {
            actions: Vec::new(),
            data_dir: data_dir.into(),
            filename: filename.into(),
        }
    }

    /// Load pending actions from disk.
    pub async fn load(&mut self) {
        let path = std::path::Path::new(&self.data_dir).join(&self.filename);
        if let Ok(contents) = tokio::fs::read_to_string(&path).await {
            self.actions = serde_json::from_str(&contents).unwrap_or_default();
        }
    }

    /// Persist current actions to disk.
    pub async fn save(&self) {
        let path = std::path::Path::new(&self.data_dir).join(&self.filename);
        if let Ok(json) = serde_json::to_string_pretty(&self.actions) {
            if let Err(e) = tokio::fs::write(&path, json).await {
                tracing::warn!(error = %e, file = %self.filename, "failed to persist action queue");
            }
        }
    }

    /// Enqueue a new action.
    pub fn enqueue(&mut self, action: A) {
        self.actions.push(PendingAction::new(action));
    }

    /// All pending actions (immutable).
    pub fn pending(&self) -> &[PendingAction<A>] {
        &self.actions
    }

    /// Drain all pending actions (take ownership).
    pub fn drain(&mut self) -> Vec<PendingAction<A>> {
        std::mem::take(&mut self.actions)
    }

    /// Remove actions matching a predicate.
    pub fn remove_where<F: Fn(&A) -> bool>(&mut self, predicate: F) {
        self.actions.retain(|pa| !predicate(&pa.action));
    }

    /// Number of pending actions.
    pub fn len(&self) -> usize {
        self.actions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    enum TestAction {
        Remove { endpoint: String },
        Add { endpoint: String },
    }

    #[test]
    fn enqueue_and_drain() {
        let mut queue = ActionQueue::<TestAction>::new("/tmp", "test.json");
        queue.enqueue(TestAction::Remove {
            endpoint: "10.0.0.1:5432".into(),
        });
        queue.enqueue(TestAction::Add {
            endpoint: "10.0.0.2:5432".into(),
        });
        assert_eq!(queue.len(), 2);

        let drained = queue.drain();
        assert_eq!(drained.len(), 2);
        assert!(queue.is_empty());
    }

    #[test]
    fn remove_where() {
        let mut queue = ActionQueue::<TestAction>::new("/tmp", "test.json");
        queue.enqueue(TestAction::Remove {
            endpoint: "10.0.0.1:5432".into(),
        });
        queue.enqueue(TestAction::Add {
            endpoint: "10.0.0.2:5432".into(),
        });

        queue.remove_where(|a| matches!(a, TestAction::Remove { .. }));
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn mark_attempt() {
        let mut pa = PendingAction::new(TestAction::Remove {
            endpoint: "x".into(),
        });
        assert_eq!(pa.attempts, 0);
        pa.mark_attempt();
        assert_eq!(pa.attempts, 1);
        assert!(pa.last_attempt.is_some());
    }
}
