//! Scheduled task store - Persistence for offering maintenance tasks
//!
//! Manages the TaskRegistry which tracks scheduled tasks for each offering.
//! Tasks are defined in manifests and registered during installation.
//!
//! # Storage Layout
//! ```text
//! {config_dir}/tasks/
//!   registry.json    <- TaskRegistry (all scheduled tasks)
//!   history/
//!     {task_id}.json <- Task execution history
//! ```

use anyhow::{Context, Result};
use garden_common::{ScheduledTask, TaskDefinition, TaskResult};
use std::collections::HashMap;
use std::path::PathBuf;

// TaskRegistry (pure data type) lives in domain; re-export for backward compat.
pub use crate::domain::task_registry::TaskRegistry;

/// Store for scheduled task persistence
pub struct TaskStore {
    /// Path to task registry
    registry_path: PathBuf,
    /// Path to task history directory (reserved for future use)
    #[expect(dead_code)]
    history_dir: PathBuf,
}

impl TaskStore {
    /// Create a new task store
    pub fn new() -> Self {
        let config_dir = PathBuf::from(garden_common::constants::CONFIG_DIR);
        let tasks_dir = config_dir.join("tasks");

        Self {
            registry_path: tasks_dir.join("registry.json"),
            history_dir: tasks_dir.join("history"),
        }
    }

    /// Load the task registry from disk
    pub async fn load_registry(&self) -> Result<TaskRegistry> {
        if !self.registry_path.exists() {
            return Ok(TaskRegistry::new());
        }

        let content = tokio::fs::read_to_string(&self.registry_path)
            .await
            .context("Failed to read task registry")?;

        serde_json::from_str(&content).context("Failed to parse task registry")
    }

    /// Save the task registry to disk
    pub async fn save_registry(&self, registry: &TaskRegistry) -> Result<()> {
        if let Some(parent) = self.registry_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .context("Failed to create tasks directory")?;
        }

        let content =
            serde_json::to_string_pretty(registry).context("Failed to serialize task registry")?;

        // Atomic write
        let tmp_path = self.registry_path.with_extension("tmp");
        tokio::fs::write(&tmp_path, &content)
            .await
            .context("Failed to write task registry")?;

        // Windows doesn't allow rename over existing file
        #[cfg(windows)]
        if self.registry_path.exists() {
            let _ = tokio::fs::remove_file(&self.registry_path).await;
        }

        tokio::fs::rename(&tmp_path, &self.registry_path)
            .await
            .context("Failed to rename task registry")?;

        Ok(())
    }

    /// Register tasks for an offering
    ///
    /// Creates ScheduledTask entries for each task defined in the manifest.
    /// Returns the number of tasks registered.
    pub async fn register_tasks(
        &self,
        offering_id: &str,
        offering_name: &str,
        tasks: &HashMap<String, TaskDefinition>,
    ) -> Result<usize> {
        if tasks.is_empty() {
            return Ok(0);
        }

        let mut registry = self.load_registry().await?;
        let now = chrono::Utc::now().to_rfc3339();
        let mut count = 0;

        for (task_name, definition) in tasks {
            let task_id = format!("{}:{}", offering_id, task_name);

            // Only register if not already present (preserve history)
            if !registry.contains(&task_id) {
                let task = ScheduledTask {
                    task_id: task_id.clone(),
                    offering_id: offering_id.to_string(),
                    offering_name: offering_name.to_string(),
                    task_name: task_name.clone(),
                    definition: definition.clone(),
                    registered_at: now.clone(),
                    last_run: None,
                    last_result: None,
                    next_run: None, // Will be calculated by scheduler
                };

                registry.upsert(task);
                count += 1;

                tracing::info!(
                    task_id = %task_id,
                    offering_id = %offering_id,
                    schedule = %definition.schedule,
                    "Registered scheduled task"
                );
            }
        }

        if count > 0 {
            self.save_registry(&registry).await?;
        }

        Ok(count)
    }

    /// Unregister all tasks for an offering
    ///
    /// Removes all tasks associated with the offering_id.
    /// Returns the tasks that were removed.
    pub async fn unregister_tasks(&self, offering_id: &str) -> Result<Vec<ScheduledTask>> {
        let mut registry = self.load_registry().await?;
        let removed = registry.remove_for_offering(offering_id);

        if !removed.is_empty() {
            self.save_registry(&registry).await?;

            for task in &removed {
                tracing::info!(
                    task_id = %task.task_id,
                    offering_id = %offering_id,
                    "Unregistered scheduled task"
                );
            }
        }

        Ok(removed)
    }

    /// Update task execution result
    pub async fn update_task_result(
        &self,
        task_id: &str,
        result: TaskResult,
        next_run: Option<String>,
    ) -> Result<()> {
        let mut registry = self.load_registry().await?;

        if let Some(task) = registry.get_mut(task_id) {
            task.last_run = Some(chrono::Utc::now().to_rfc3339());
            task.last_result = Some(result);
            task.next_run = next_run;
            self.save_registry(&registry).await?;
        }

        Ok(())
    }

    /// Get all tasks that need to run
    ///
    /// Returns tasks where:
    /// 1. Task is enabled
    /// 2. next_run is in the past OR next_run is not set
    pub async fn due_tasks(&self) -> Result<Vec<ScheduledTask>> {
        let registry = self.load_registry().await?;
        let now = chrono::Utc::now();

        let due: Vec<ScheduledTask> = registry
            .enabled_tasks()
            .into_iter()
            .filter(|task| {
                match &task.next_run {
                    Some(next) => {
                        // Parse and check if due
                        chrono::DateTime::parse_from_rfc3339(next)
                            .map(|dt| dt < now)
                            .unwrap_or(true) // If parse fails, consider due
                    }
                    None => true, // No next_run means it needs scheduling
                }
            })
            .cloned()
            .collect();

        Ok(due)
    }

    /// List all tasks
    pub async fn list_tasks(&self) -> Result<Vec<ScheduledTask>> {
        let registry = self.load_registry().await?;
        Ok(registry.tasks.into_values().collect())
    }

    /// Get tasks for an offering
    pub async fn tasks_for_offering(&self, offering_id: &str) -> Result<Vec<ScheduledTask>> {
        let registry = self.load_registry().await?;
        Ok(registry
            .tasks_for_offering(offering_id)
            .into_iter()
            .cloned()
            .collect())
    }
}

impl crate::domain::traits::TaskRegistryPersistence for TaskStore {
    async fn load_registry(&self) -> Result<TaskRegistry> {
        TaskStore::load_registry(self).await
    }

    async fn save_registry(&self, registry: &TaskRegistry) -> Result<()> {
        TaskStore::save_registry(self, registry).await
    }
}

impl Default for TaskStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use garden_common::TaskCategory;

    fn test_definition() -> TaskDefinition {
        TaskDefinition {
            description: "Test task".to_string(),
            schedule: "0 0 * * *".to_string(),
            command: vec!["echo".to_string(), "test".to_string()],
            category: TaskCategory::Maintenance,
            enabled: true,
            timeout_secs: 60,
        }
    }

    #[test]
    fn test_registry_operations() {
        let mut registry = TaskRegistry::new();
        assert!(registry.is_empty());

        let task = ScheduledTask {
            task_id: "offer-123:update".to_string(),
            offering_id: "offer-123".to_string(),
            offering_name: "pihole".to_string(),
            task_name: "update".to_string(),
            definition: test_definition(),
            registered_at: chrono::Utc::now().to_rfc3339(),
            last_run: None,
            last_result: None,
            next_run: None,
        };

        registry.upsert(task.clone());
        assert_eq!(registry.len(), 1);
        assert!(registry.contains("offer-123:update"));

        let found = registry.get("offer-123:update");
        assert!(found.is_some());
        assert_eq!(found.unwrap().offering_name, "pihole");

        let by_offering = registry.tasks_for_offering("offer-123");
        assert_eq!(by_offering.len(), 1);

        let removed = registry.remove("offer-123:update");
        assert!(removed.is_some());
        assert!(registry.is_empty());
    }

    #[test]
    fn test_remove_for_offering() {
        let mut registry = TaskRegistry::new();

        // Add multiple tasks for same offering
        for name in &["update", "cleanup", "health-check"] {
            let task = ScheduledTask {
                task_id: format!("offer-123:{}", name),
                offering_id: "offer-123".to_string(),
                offering_name: "pihole".to_string(),
                task_name: name.to_string(),
                definition: test_definition(),
                registered_at: chrono::Utc::now().to_rfc3339(),
                last_run: None,
                last_result: None,
                next_run: None,
            };
            registry.upsert(task);
        }

        // Add task for different offering
        let other_task = ScheduledTask {
            task_id: "offer-456:backup".to_string(),
            offering_id: "offer-456".to_string(),
            offering_name: "mongodb".to_string(),
            task_name: "backup".to_string(),
            definition: test_definition(),
            registered_at: chrono::Utc::now().to_rfc3339(),
            last_run: None,
            last_result: None,
            next_run: None,
        };
        registry.upsert(other_task);

        assert_eq!(registry.len(), 4);

        // Remove all for offer-123
        let removed = registry.remove_for_offering("offer-123");
        assert_eq!(removed.len(), 3);
        assert_eq!(registry.len(), 1);
        assert!(registry.contains("offer-456:backup"));
    }
}
