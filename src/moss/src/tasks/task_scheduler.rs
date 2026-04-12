//! Task Scheduler - Background task for scheduled maintenance operations
//!
//! Handles execution of scheduled tasks defined in offering manifests:
//! 1. Parses cron expressions to determine next run times
//! 2. Executes commands inside containers via docker exec
//! 3. Records execution results and schedules next run
//!
//! # Task Lifecycle
//! 1. Tasks are registered during offering installation
//! 2. Scheduler periodically checks for due tasks
//! 3. Commands are executed inside the running container
//! 4. Results are recorded and next run is scheduled
//! 5. Tasks are unregistered when offering is removed

use crate::AppState;
use crate::infra::TaskStore;
use anyhow::Result;
use garden_common::{ScheduledTask, TaskResult};
use std::time::Duration;

/// Task scheduler configuration
#[derive(Debug, Clone)]
pub struct TaskSchedulerConfig {
    /// How often to check for due tasks (default: 60 seconds)
    pub check_interval: Duration,
    /// Whether to catch up on missed tasks at startup
    pub catchup_on_start: bool,
    /// Maximum concurrent task executions
    pub max_concurrent: usize,
}

impl Default for TaskSchedulerConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(60),
            catchup_on_start: false,
            max_concurrent: 4,
        }
    }
}

/// Calculate next run time from cron expression
///
/// Uses the cron crate for parsing. Returns None if expression is invalid
/// or no next occurrence exists.
fn next_run_time(cron_expr: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    use cron::Schedule;
    use std::str::FromStr;

    // Try to parse with optional seconds field
    let schedule = if cron_expr.split_whitespace().count() == 5 {
        // Standard 5-field cron, add seconds
        Schedule::from_str(&format!("0 {}", cron_expr)).ok()?
    } else {
        // Already has seconds or non-standard format
        Schedule::from_str(cron_expr).ok()?
    };

    schedule.upcoming(chrono::Utc).next()
}

/// Execute a task inside a container
async fn execute_task(state: &AppState, task: &ScheduledTask) -> TaskResult {
    let start = std::time::Instant::now();

    tracing::info!(
        task_id = %task.task_id,
        container = %task.offering_name,
        command = ?task.definition.command,
        "Executing scheduled task"
    );

    // Execute command inside container (timeout_secs as u32)
    let timeout_secs = task.definition.timeout_secs.min(u32::MAX as u64) as u32;
    let result = state
        .platform
        .docker
        .exec_in_container(&task.offering_name, &task.definition.command, timeout_secs)
        .await;

    let duration_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok((exit_code, output)) => {
            let success = exit_code == 0;
            let output_truncated = if output.len() > 10000 {
                format!("{}...(truncated)", &output[..10000])
            } else {
                output
            };

            if success {
                tracing::info!(
                    task_id = %task.task_id,
                    duration_ms,
                    "Task completed successfully"
                );
            } else {
                tracing::warn!(
                    task_id = %task.task_id,
                    exit_code,
                    duration_ms,
                    "Task completed with non-zero exit code"
                );
            }

            TaskResult {
                success,
                exit_code: Some(exit_code as i32),
                duration_ms,
                output: Some(output_truncated),
                error: None,
            }
        }
        Err(e) => {
            tracing::error!(
                task_id = %task.task_id,
                error = ?e,
                duration_ms,
                "Task execution failed"
            );

            TaskResult {
                success: false,
                exit_code: None,
                duration_ms,
                output: None,
                error: Some(e.to_string()),
            }
        }
    }
}

/// Run a single iteration of the scheduler
///
/// Checks for due tasks, executes them, and updates their status.
pub async fn run_scheduler_iteration(state: &AppState) -> Result<usize> {
    let task_store = TaskStore::new();
    let due_tasks = task_store.due_tasks().await?;

    if due_tasks.is_empty() {
        return Ok(0);
    }

    tracing::debug!(task_count = due_tasks.len(), "Found due scheduled tasks");

    let mut executed = 0;

    for task in due_tasks {
        // Verify container is running before executing
        let container_running = {
            let offerings = state.offerings.read().await;
            offerings
                .iter()
                .find(|o| o.offering_id == task.offering_id)
                .map(|o| o.status == garden_common::OfferingStatus::Running)
                .unwrap_or(false)
        };

        if !container_running {
            tracing::debug!(
                task_id = %task.task_id,
                "Skipping task: container not running"
            );
            continue;
        }

        // Execute the task
        let result = execute_task(state, &task).await;

        // Calculate next run time
        let next_run = next_run_time(&task.definition.schedule).map(|dt| dt.to_rfc3339());

        // Update task with result
        if let Err(e) = task_store
            .update_task_result(&task.task_id, result, next_run)
            .await
        {
            tracing::error!(
                task_id = %task.task_id,
                error = ?e,
                "Failed to update task result"
            );
        }

        executed += 1;
    }

    if executed > 0 {
        tracing::info!(
            executed_count = executed,
            "Completed scheduled task execution cycle"
        );
    }

    Ok(executed)
}

/// Background task scheduler loop
///
/// Runs indefinitely, checking for due tasks at the configured interval.
/// Should be spawned with tokio::spawn().
/// Exits cooperatively when the shutdown token is cancelled (MOSS-0004).
pub async fn task_scheduler_loop(
    state: AppState,
    config: TaskSchedulerConfig,
    token: tokio_util::sync::CancellationToken,
) {
    tracing::info!(
        check_interval_secs = config.check_interval.as_secs(),
        "Starting task scheduler"
    );

    // Run initial check if catchup is enabled
    if config.catchup_on_start
        && let Err(e) = run_scheduler_iteration(&state).await
    {
        tracing::error!(error = ?e, "Failed to run initial task check");
    }

    let mut interval = tokio::time::interval(config.check_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = interval.tick() => {}
            _ = token.cancelled() => {
                tracing::debug!("Task scheduler shutting down (MOSS-0004)");
                break;
            }
        }

        if let Err(e) = run_scheduler_iteration(&state).await {
            tracing::error!(error = ?e, "Task scheduler iteration failed");
        }
    }
}

/// Start the task scheduler in the background
pub fn start_task_scheduler(
    state: AppState,
    token: tokio_util::sync::CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(task_scheduler_loop(
        state,
        TaskSchedulerConfig::default(),
        token,
    ))
}

/// Backfill missing scheduled tasks for existing offerings
///
/// Called at boot time to ensure any offering that:
/// 1. Has tasks defined in its manifest
/// 2. Doesn't have those tasks registered
///
/// Gets the tasks registered.
///
/// Returns the number of tasks that were registered.
pub async fn backfill_missing_tasks(state: &AppState) -> usize {
    tracing::info!("Starting scheduled task backfill");

    let task_store = TaskStore::new();
    let mut registered = 0;

    // Get all managed offerings from registry
    let managed_offerings: Vec<(String, String, String)> = {
        let offerings = state.offerings.read().await;
        offerings
            .iter()
            .filter(|o| o.is_managed())
            .map(|o| {
                (
                    o.offering_id.clone(),
                    o.name.to_string(),
                    o.offering.clone(),
                )
            })
            .collect()
    };

    for (offering_id, offering_name, offering_type) in managed_offerings {
        // Get manifest for this offering type
        let Some(manifest) = state.catalog.get_manifest(&offering_type) else {
            continue;
        };

        // Parse template to get tasks
        let Ok(template) = manifest.parse_template() else {
            continue;
        };

        if template.tasks.is_empty() {
            continue;
        }

        // Check which tasks are missing
        let existing_tasks = match task_store.tasks_for_offering(&offering_id).await {
            Ok(tasks) => tasks,
            Err(e) => {
                tracing::warn!(
                    offering_id = %offering_id,
                    error = ?e,
                    "Failed to get existing tasks"
                );
                continue;
            }
        };

        let existing_names: std::collections::HashSet<_> = existing_tasks
            .iter()
            .map(|t| t.task_name.as_str())
            .collect();

        let missing_tasks: std::collections::HashMap<String, garden_common::TaskDefinition> =
            template
                .tasks
                .iter()
                .filter(|(name, _)| !existing_names.contains(name.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

        if missing_tasks.is_empty() {
            continue;
        }

        // Register missing tasks
        match task_store
            .register_tasks(&offering_id, &offering_name, &missing_tasks)
            .await
        {
            Ok(count) => {
                registered += count;
                tracing::info!(
                    offering = %offering_name,
                    count,
                    "Backfilled missing scheduled tasks"
                );
            }
            Err(e) => {
                tracing::error!(
                    offering = %offering_name,
                    error = ?e,
                    "Failed to register backfill tasks"
                );
            }
        }
    }

    if registered > 0 {
        tracing::info!(total = registered, "Completed scheduled task backfill");
    }

    registered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_next_run_time_parsing() {
        // Daily at midnight
        let next = next_run_time("0 0 * * *");
        assert!(next.is_some());

        // Every hour
        let next = next_run_time("0 * * * *");
        assert!(next.is_some());

        // Every minute (less common but valid)
        let next = next_run_time("* * * * *");
        assert!(next.is_some());

        // Invalid expression
        let next = next_run_time("invalid cron");
        assert!(next.is_none());
    }

    #[test]
    fn test_scheduler_config_default() {
        let config = TaskSchedulerConfig::default();
        assert_eq!(config.check_interval.as_secs(), 60);
        assert!(!config.catchup_on_start);
        assert_eq!(config.max_concurrent, 4);
    }
}
