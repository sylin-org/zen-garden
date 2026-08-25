//! Task registry — pure domain data type for scheduled task tracking.
//!
//! Moved from `infra/task_store.rs` because `TaskRegistry` is a pure
//! value type (HashMap + serde) with no I/O. The I/O-performing
//! `TaskStore` stays in infra and implements the `TaskRegistryPersistence` trait.

use garden_common::ScheduledTask;
use std::collections::HashMap;

/// Registry of all scheduled tasks
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct TaskRegistry {
    /// All scheduled tasks keyed by task_id
    pub tasks: HashMap<String, ScheduledTask>,
    /// Last update timestamp
    pub updated_at: String,
}

impl TaskRegistry {
    /// Create empty registry
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Get all tasks for an offering
    pub fn tasks_for_offering(&self, offering_id: &str) -> Vec<&ScheduledTask> {
        self.tasks
            .values()
            .filter(|t| t.offering_id == offering_id)
            .collect()
    }

    /// Get a specific task by ID
    pub fn get(&self, task_id: &str) -> Option<&ScheduledTask> {
        self.tasks.get(task_id)
    }

    /// Get a mutable reference to a task
    pub fn get_mut(&mut self, task_id: &str) -> Option<&mut ScheduledTask> {
        self.tasks.get_mut(task_id)
    }

    /// Check if a task exists
    pub fn contains(&self, task_id: &str) -> bool {
        self.tasks.contains_key(task_id)
    }

    /// Add or update a task
    pub fn upsert(&mut self, task: ScheduledTask) {
        self.tasks.insert(task.task_id.clone(), task);
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }

    /// Remove a task by ID
    pub fn remove(&mut self, task_id: &str) -> Option<ScheduledTask> {
        let result = self.tasks.remove(task_id);
        if result.is_some() {
            self.updated_at = chrono::Utc::now().to_rfc3339();
        }
        result
    }

    /// Remove all tasks for an offering
    pub fn remove_for_offering(&mut self, offering_id: &str) -> Vec<ScheduledTask> {
        let task_ids: Vec<String> = self
            .tasks
            .iter()
            .filter(|(_, t)| t.offering_id == offering_id)
            .map(|(id, _)| id.clone())
            .collect();

        let mut removed = Vec::new();
        for id in task_ids {
            if let Some(task) = self.tasks.remove(&id) {
                removed.push(task);
            }
        }

        if !removed.is_empty() {
            self.updated_at = chrono::Utc::now().to_rfc3339();
        }

        removed
    }

    /// Get count of all tasks
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    /// Get all enabled tasks
    pub fn enabled_tasks(&self) -> Vec<&ScheduledTask> {
        self.tasks
            .values()
            .filter(|t| t.definition.enabled)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use garden_common::{TaskCategory, TaskDefinition};

    fn test_definition() -> TaskDefinition {
        TaskDefinition {
            description: "Test task".to_string(),
            schedule: "0 0 * * *".to_string(),
            command: vec!["echo".to_string(), "test".to_string()],
            action: garden_common::TaskAction::Exec,
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

        let removed = registry.remove_for_offering("offer-123");
        assert_eq!(removed.len(), 3);
        assert_eq!(registry.len(), 1);
        assert!(registry.contains("offer-456:backup"));
    }
}
