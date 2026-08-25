//! BackgroundTask trait and supporting types for the task supervisor (ARCH-0015).
//!
//! Every background task implements `BackgroundTask`. The supervisor boxes each
//! task, validates the dependency DAG, wires ready-signals, spawns all tasks,
//! and monitors outcomes.

use std::future::Future;
use std::pin::Pin;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::Moss;

// ── Outcome ─────────────────────────────────────────────────────────────

/// What a task reports when it finishes.
#[derive(Debug, Clone)]
pub enum TaskOutcome {
    /// Normal completion (one-shot tasks, or long-running tasks that exit cleanly).
    Completed,
    /// Exited because the cancellation token fired.
    Cancelled,
    /// Task-level error (not a panic — panics are caught by the supervisor).
    Failed { error: String },
}

impl std::fmt::Display for TaskOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Completed => write!(f, "completed"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::Failed { error } => write!(f, "failed: {error}"),
        }
    }
}

// ── Context ─────────────────────────────────────────────────────────────

/// Provided to every task by the supervisor.
///
/// Carries the application state, a scoped cancellation token, a dependency
/// gate (blocks until upstream tasks signal ready), and a ready signal
/// (unblocks downstream tasks).
pub struct TaskContext {
    /// Full application state.
    pub state: Moss,
    /// Cancellation token scoped to this task (child of the global shutdown token).
    pub token: CancellationToken,
    /// Gate that blocks until all declared dependencies have signaled ready.
    pub deps: DependencyGate,
    /// Signal to announce this task is ready for dependents.
    pub ready: ReadySignal,
}

// ── DependencyGate ──────────────────────────────────────────────────────

/// Blocks until all upstream dependencies have signaled ready.
///
/// If the cancellation token fires before all deps are ready, `wait()`
/// returns `false` so the task can return `TaskOutcome::Cancelled`.
pub struct DependencyGate {
    receivers: Vec<(&'static str, watch::Receiver<bool>)>,
    token: CancellationToken,
}

impl DependencyGate {
    pub(crate) fn new(
        receivers: Vec<(&'static str, watch::Receiver<bool>)>,
        token: CancellationToken,
    ) -> Self {
        Self { receivers, token }
    }

    /// Block until all dependencies signal ready.
    ///
    /// Returns `true` when all are ready, `false` if cancelled or a
    /// dependency sender was dropped (will never become ready).
    pub async fn wait(&mut self) -> bool {
        for (dep_name, rx) in &mut self.receivers {
            loop {
                if *rx.borrow() {
                    tracing::debug!(dependency = *dep_name, "dependency satisfied");
                    break;
                }
                tokio::select! {
                    result = rx.changed() => {
                        if result.is_err() {
                            tracing::warn!(
                                dependency = *dep_name,
                                "dependency sender dropped — will never be ready"
                            );
                            return false;
                        }
                    }
                    _ = self.token.cancelled() => {
                        tracing::debug!("cancelled while waiting for dependencies");
                        return false;
                    }
                }
            }
        }
        true
    }
}

// ── ReadySignal ─────────────────────────────────────────────────────────

/// Signals downstream dependents that this task is ready.
///
/// Calling `signal()` is idempotent. One-shot tasks should signal before
/// returning. Long-running tasks should signal once their initialization
/// is complete (e.g., after loading cache, opening a connection).
pub struct ReadySignal {
    sender: watch::Sender<bool>,
}

impl ReadySignal {
    pub(crate) fn new(sender: watch::Sender<bool>) -> Self {
        Self { sender }
    }

    /// Mark this task as ready. Idempotent — safe to call multiple times.
    pub fn signal(&self) {
        let _ = self.sender.send(true);
    }

    /// Check if ready has been signaled.
    pub fn is_signaled(&self) -> bool {
        *self.sender.borrow()
    }
}

// ── Trait ────────────────────────────────────────────────────────────────

/// Every background task implements this trait.
///
/// The supervisor:
/// 1. Calls `dependencies()` to build the DAG and validate it (no cycles,
///    no missing deps).
/// 2. Creates a `watch::channel(false)` per task for ready signaling.
/// 3. Spawns `run()` with a `TaskContext` wired to the correct gates and
///    signals.
/// 4. Catches panics and records `TaskOutcome` for every task.
pub trait BackgroundTask: Send + 'static {
    /// Unique task name. Used in tracing spans, dependency references,
    /// supervisor status, and the `/tasks` API endpoint.
    fn name(&self) -> &'static str;

    /// Names of tasks that must call `ctx.signal_ready()` before this
    /// task's `run()` should start meaningful work.
    ///
    /// The task should call `ctx.deps.wait().await` at the top of `run()`
    /// to block until all dependencies are satisfied.
    ///
    /// Default: no dependencies (start immediately).
    fn dependencies(&self) -> &'static [&'static str] {
        &[]
    }

    /// Execute the task. Called exactly once by the supervisor.
    ///
    /// The boxed future allows each task struct to carry unique state
    /// (channels, config) that gets moved into the async block.
    fn run(self: Box<Self>, ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>>;
}
