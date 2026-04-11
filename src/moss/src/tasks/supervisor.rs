//! Dependency-aware task supervisor (ARCH-0015).
//!
//! Replaces the thin JoinSet wrapper with a structured supervisor that:
//! - Validates the dependency DAG at startup (cycle + missing dep detection)
//! - Wires ready-signals between tasks via `watch` channels
//! - Spawns all tasks with tracing spans and panic protection
//! - Tracks outcomes for every task
//! - Provides status for the `/tasks` API endpoint

use futures_util::FutureExt;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc, watch};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::AppState;

use super::task_trait::{BackgroundTask, DependencyGate, ReadySignal, TaskContext, TaskOutcome};

// ── Public types ────────────────────────────────────────────────────────

/// Per-task state visible via the status API.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskState {
    /// Blocked on dependencies.
    Waiting,
    /// Running (dependencies satisfied, not yet finished).
    Running,
    /// Finished normally.
    Completed,
    /// Finished due to cancellation.
    Cancelled,
    /// Finished with an error or panic.
    Failed { error: String },
}

/// Status snapshot for one task.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TaskStatus {
    pub name: &'static str,
    pub state: TaskState,
    pub ready: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub waiting_on: Vec<&'static str>,
}

/// Aggregate supervisor status.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SupervisorStatus {
    pub total: usize,
    pub running: usize,
    pub completed: usize,
    pub failed: usize,
    pub tasks: Vec<TaskStatus>,
}

// ── Internal bookkeeping ────────────────────────────────────────────────

// ── Supervisor ──────────────────────────────────────────────────────────

/// Cloneable handle for querying supervisor status from API handlers.
///
/// Extracted from the supervisor before `run()` consumes it. Holds shared
/// references to the same state that the supervisor monitors.
#[derive(Clone)]
pub struct SupervisorHandle {
    entries: Arc<Vec<TaskEntrySnapshot>>,
    states: Arc<RwLock<HashMap<&'static str, TaskState>>>,
    ready_map: Arc<HashMap<&'static str, watch::Receiver<bool>>>,
}

#[derive(Clone)]
struct TaskEntrySnapshot {
    name: &'static str,
    dependencies: &'static [&'static str],
    ready_rx: watch::Receiver<bool>,
}

impl SupervisorHandle {
    /// Current status of all tasks.
    pub async fn status(&self) -> SupervisorStatus {
        let states = self.states.read().await;
        let mut tasks = Vec::with_capacity(self.entries.len());

        for entry in self.entries.iter() {
            let state = states
                .get(entry.name)
                .cloned()
                .unwrap_or(TaskState::Waiting);
            let ready = *entry.ready_rx.borrow();

            let waiting_on: Vec<&'static str> = entry
                .dependencies
                .iter()
                .filter(|dep| {
                    self.ready_map
                        .get(**dep)
                        .map(|rx| !*rx.borrow())
                        .unwrap_or(false)
                })
                .copied()
                .collect();

            tasks.push(TaskStatus {
                name: entry.name,
                state,
                ready,
                waiting_on,
            });
        }

        let running = tasks
            .iter()
            .filter(|t| matches!(t.state, TaskState::Running | TaskState::Waiting))
            .count();
        let completed = tasks
            .iter()
            .filter(|t| matches!(t.state, TaskState::Completed))
            .count();
        let failed = tasks
            .iter()
            .filter(|t| matches!(t.state, TaskState::Failed { .. }))
            .count();

        SupervisorStatus {
            total: tasks.len(),
            running,
            completed,
            failed,
            tasks,
        }
    }
}

/// Manages all background tasks with dependency ordering and structured shutdown.
pub(crate) struct TaskSupervisor {
    join_set: JoinSet<()>,
    outcome_rx: mpsc::Receiver<(&'static str, TaskOutcome)>,
    states: Arc<RwLock<HashMap<&'static str, TaskState>>>,
    handle_entries: Arc<Vec<TaskEntrySnapshot>>,
    handle_ready_map: Arc<HashMap<&'static str, watch::Receiver<bool>>>,
}

impl TaskSupervisor {
    /// Validate the dependency graph, wire channels, and spawn all tasks.
    ///
    /// Returns `Err` if the dependency graph contains cycles or references
    /// missing tasks. This is a hard startup failure — better to crash loud
    /// than deadlock silently.
    pub fn build(
        tasks: Vec<Box<dyn BackgroundTask>>,
        state: AppState,
        token: CancellationToken,
    ) -> anyhow::Result<Self> {
        let task_count = tasks.len();

        // Collect names and deps for validation
        let mut names: HashSet<&'static str> = HashSet::with_capacity(task_count);
        let mut dep_map: HashMap<&'static str, &'static [&'static str]> =
            HashMap::with_capacity(task_count);

        for task in &tasks {
            let name = task.name();
            if !names.insert(name) {
                anyhow::bail!("Duplicate task name: {name}");
            }
            dep_map.insert(name, task.dependencies());
        }

        // Validate: all referenced dependencies exist
        for (task_name, deps) in &dep_map {
            for dep in *deps {
                if !names.contains(dep) {
                    anyhow::bail!(
                        "Task '{task_name}' depends on '{dep}' which does not exist in the registry"
                    );
                }
            }
        }

        // Validate: no cycles (Kahn's algorithm)
        validate_no_cycles(&dep_map)?;

        // Create ready channels: one (Sender, Receiver) pair per task
        let mut ready_senders: HashMap<&'static str, watch::Sender<bool>> =
            HashMap::with_capacity(task_count);
        let mut ready_receivers: HashMap<&'static str, watch::Receiver<bool>> =
            HashMap::with_capacity(task_count);

        for name in &names {
            let (tx, rx) = watch::channel(false);
            ready_senders.insert(name, tx);
            ready_receivers.insert(name, rx);
        }

        // Outcome channel
        let (outcome_tx, outcome_rx) = mpsc::channel::<(&'static str, TaskOutcome)>(task_count);

        // Shared state map
        let states: Arc<RwLock<HashMap<&'static str, TaskState>>> = Arc::new(RwLock::new(
            names.iter().map(|n| (*n, TaskState::Waiting)).collect(),
        ));

        // Build entry snapshots for status queries (shared via SupervisorHandle)
        let mut entry_snapshots = Vec::with_capacity(task_count);
        for name in &names {
            let deps = dep_map.get(name).copied().unwrap_or(&[]);
            entry_snapshots.push(TaskEntrySnapshot {
                name,
                dependencies: deps,
                ready_rx: ready_receivers.get(name).expect("just created").clone(),
            });
        }
        let handle_entries = Arc::new(entry_snapshots);

        // Spawn all tasks
        let mut join_set = JoinSet::new();

        for task in tasks {
            let name = task.name();
            let task_token = token.child_token();
            let state = state.clone();

            // Build DependencyGate from this task's declared dependencies
            let dep_receivers: Vec<(&'static str, watch::Receiver<bool>)> = task
                .dependencies()
                .iter()
                .map(|dep_name| {
                    let rx = ready_receivers
                        .get(dep_name)
                        .expect("validated above")
                        .clone();
                    (*dep_name, rx)
                })
                .collect();

            let gate = DependencyGate::new(dep_receivers, task_token.clone());

            // Build ReadySignal
            let ready_tx = ready_senders
                .remove(name)
                .expect("each task has exactly one sender");
            let signal = ReadySignal::new(ready_tx);

            let ctx = TaskContext {
                state,
                token: task_token,
                deps: gate,
                ready: signal,
            };

            let outcome_tx = outcome_tx.clone();
            let states = states.clone();
            let span = tracing::info_span!("task", name);

            join_set.spawn(tracing::Instrument::instrument(
                async move {
                    // Mark running
                    {
                        let mut s = states.write().await;
                        s.insert(name, TaskState::Running);
                    }

                    tracing::info!("task starting");

                    // Run with panic protection
                    let outcome = match std::panic::AssertUnwindSafe(task.run(ctx))
                        .catch_unwind()
                        .await
                    {
                        Ok(outcome) => outcome,
                        Err(panic_payload) => {
                            let msg = panic_message(&panic_payload);
                            tracing::error!(error = %msg, "task PANICKED");
                            TaskOutcome::Failed {
                                error: format!("panic: {msg}"),
                            }
                        }
                    };

                    // Log outcome
                    match &outcome {
                        TaskOutcome::Completed => tracing::info!("task completed"),
                        TaskOutcome::Cancelled => tracing::info!("task cancelled"),
                        TaskOutcome::Failed { error } => {
                            tracing::error!(error, "task failed")
                        }
                    }

                    // Record state
                    {
                        let mut s = states.write().await;
                        s.insert(
                            name,
                            match &outcome {
                                TaskOutcome::Completed => TaskState::Completed,
                                TaskOutcome::Cancelled => TaskState::Cancelled,
                                TaskOutcome::Failed { error } => TaskState::Failed {
                                    error: error.clone(),
                                },
                            },
                        );
                    }

                    let _ = outcome_tx.send((name, outcome)).await;
                },
                span,
            ));
        }

        tracing::info!(
            tasks = task_count,
            "Task supervisor built — all tasks spawned"
        );

        let handle_ready_map = Arc::new(ready_receivers);

        Ok(Self {
            join_set,
            outcome_rx,
            states,
            handle_entries,
            handle_ready_map,
        })
    }

    /// Get a cloneable handle for querying task status from API handlers.
    ///
    /// Call this before `run()` which consumes the supervisor.
    pub fn handle(&self) -> SupervisorHandle {
        SupervisorHandle {
            entries: self.handle_entries.clone(),
            states: self.states.clone(),
            ready_map: self.handle_ready_map.clone(),
        }
    }

    /// Run the supervisor — monitors tasks, logs outcomes, handles shutdown.
    ///
    /// Returns when all tasks complete or the cancellation token fires.
    /// Signature preserved from the old supervisor for `bootstrap/run.rs`.
    pub async fn run(mut self, token: CancellationToken) {
        loop {
            tokio::select! {
                // Drain outcome messages (for logging; state already recorded)
                msg = self.outcome_rx.recv() => {
                    match msg {
                        Some((name, outcome)) => {
                            tracing::debug!(task = name, outcome = %outcome, "outcome recorded");
                        }
                        None => {
                            // All senders dropped — all tasks finished
                            break;
                        }
                    }
                }
                // JoinSet completion (handles task exit)
                result = self.join_set.join_next() => {
                    match result {
                        Some(Ok(())) => {}
                        Some(Err(e)) if e.is_panic() => {
                            // Panic already caught inside the spawn wrapper,
                            // but JoinSet may still report it
                            tracing::error!(error = %e, "Task join error (panic)");
                        }
                        Some(Err(e)) => {
                            tracing::warn!(error = %e, "Task join error");
                        }
                        None => break, // All tasks complete
                    }
                }
                // Shutdown signal
                _ = token.cancelled() => {
                    tracing::info!("Shutting down all supervised background tasks");
                    self.join_set.shutdown().await;
                    break;
                }
            }
        }
        tracing::info!("Task supervisor stopped");
    }
}

// ── DAG validation ──────────────────────────────────────────────────────

/// Kahn's algorithm — detect cycles in the dependency graph.
fn validate_no_cycles(
    dep_map: &HashMap<&'static str, &'static [&'static str]>,
) -> anyhow::Result<()> {
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();

    // Initialize
    for &name in dep_map.keys() {
        in_degree.entry(name).or_insert(0);
    }

    // Build edges
    for (&name, deps) in dep_map {
        in_degree.insert(name, deps.len());
        for dep in *deps {
            dependents.entry(dep).or_default().push(name);
        }
    }

    // Start with zero in-degree nodes
    let mut queue: VecDeque<&str> = in_degree
        .iter()
        .filter(|(_, deg)| **deg == 0)
        .map(|(&name, _)| name)
        .collect();

    let mut visited = 0usize;

    while let Some(node) = queue.pop_front() {
        visited += 1;
        if let Some(children) = dependents.get(node) {
            for &child in children {
                let deg = in_degree.get_mut(child).expect("node exists");
                *deg -= 1;
                if *deg == 0 {
                    queue.push_back(child);
                }
            }
        }
    }

    if visited != dep_map.len() {
        let stuck: Vec<&str> = in_degree
            .iter()
            .filter(|(_, deg)| **deg > 0)
            .map(|(&name, _)| name)
            .collect();
        anyhow::bail!(
            "Dependency cycle detected involving tasks: {}",
            stuck.join(", ")
        );
    }

    Ok(())
}

/// Extract a human-readable message from a panic payload.
fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_cycles_empty() {
        let dep_map = HashMap::new();
        assert!(validate_no_cycles(&dep_map).is_ok());
    }

    #[test]
    fn test_no_cycles_linear() {
        let mut dep_map = HashMap::new();
        dep_map.insert("a", &["b"] as &[&str]);
        dep_map.insert("b", &["c"] as &[&str]);
        dep_map.insert("c", &[] as &[&str]);
        assert!(validate_no_cycles(&dep_map).is_ok());
    }

    #[test]
    fn test_cycle_detected() {
        let mut dep_map = HashMap::new();
        dep_map.insert("a", &["b"] as &[&str]);
        dep_map.insert("b", &["a"] as &[&str]);
        let err = validate_no_cycles(&dep_map).unwrap_err();
        assert!(err.to_string().contains("cycle"));
    }

    #[test]
    fn test_diamond_no_cycle() {
        let mut dep_map = HashMap::new();
        dep_map.insert("a", &[] as &[&str]);
        dep_map.insert("b", &["a"] as &[&str]);
        dep_map.insert("c", &["a"] as &[&str]);
        dep_map.insert("d", &["b", "c"] as &[&str]);
        assert!(validate_no_cycles(&dep_map).is_ok());
    }
}
