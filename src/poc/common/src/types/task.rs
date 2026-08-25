//! Scheduled task types — maintenance, backup, health operations.

use serde::{Deserialize, Serialize};

/// Category of scheduled task
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum TaskCategory {
    /// Maintenance tasks (updates, cleanup, optimization)
    #[default]
    Maintenance,
    /// Backup operations
    Backup,
    /// Health/monitoring tasks
    Health,
    /// Custom/other tasks
    Custom,
}

impl std::fmt::Display for TaskCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Maintenance => write!(f, "maintenance"),
            Self::Backup => write!(f, "backup"),
            Self::Health => write!(f, "health"),
            Self::Custom => write!(f, "custom"),
        }
    }
}

/// What a scheduled task does when it fires.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum TaskAction {
    /// Run `command` inside the container (default).
    #[default]
    Exec,
    /// Restart the container at the Moss level. `command` is ignored.
    Recycle,
}

/// Task definition in a manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDefinition {
    /// Human-readable description
    pub description: String,

    /// Cron schedule expression (e.g., "0 3 * * *" for daily at 3 AM)
    pub schedule: String,

    /// Command to execute inside the container (ignored for `recycle` actions)
    #[serde(default)]
    pub command: Vec<String>,

    /// What the task does when it fires (default: exec)
    #[serde(default)]
    pub action: TaskAction,

    /// Task category (default: maintenance)
    #[serde(default)]
    pub category: TaskCategory,

    /// Whether task is enabled (default: true)
    #[serde(default = "default_task_enabled")]
    pub enabled: bool,

    /// Timeout in seconds (default: 300 = 5 minutes)
    #[serde(default = "default_task_timeout")]
    pub timeout_secs: u64,
}

fn default_task_enabled() -> bool {
    true
}

fn default_task_timeout() -> u64 {
    300
}

/// Scheduled task instance for a specific offering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    /// Unique task ID (offering_id + task_name)
    pub task_id: String,

    /// Offering ID this task belongs to
    pub offering_id: String,

    /// Offering name (for display)
    pub offering_name: String,

    /// Task name (key from manifest)
    pub task_name: String,

    /// Task definition
    pub definition: TaskDefinition,

    /// When this task was registered
    pub registered_at: String,

    /// Last execution time (if any)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub last_run: Option<String>,

    /// Last execution result
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub last_result: Option<TaskResult>,

    /// Next scheduled run time
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub next_run: Option<String>,
}

/// Result of a task execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    /// Whether the task succeeded
    pub success: bool,

    /// Exit code (if available)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub exit_code: Option<i32>,

    /// Duration in milliseconds
    pub duration_ms: u64,

    /// Output (truncated if too long)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub output: Option<String>,

    /// Error message (if failed)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_action_defaults_to_exec() {
        assert_eq!(TaskAction::default(), TaskAction::Exec);
    }

    #[test]
    fn exec_task_deserializes_with_command() {
        let yaml = "description: update gravity\nschedule: \"0 3 * * 0\"\ncommand: [pihole, -g]\n";
        let def: TaskDefinition = serde_yml::from_str(yaml).unwrap();
        assert_eq!(def.action, TaskAction::Exec);
        assert_eq!(def.command, vec!["pihole".to_string(), "-g".to_string()]);
    }

    #[test]
    fn recycle_task_deserializes_without_command() {
        let yaml = "description: nightly recycle\nschedule: \"0 4 * * *\"\naction: recycle\n";
        let def: TaskDefinition = serde_yml::from_str(yaml).unwrap();
        assert_eq!(def.action, TaskAction::Recycle);
        assert!(def.command.is_empty());
        assert!(def.enabled);
        assert_eq!(def.timeout_secs, 300);
    }

    #[test]
    fn recycle_action_serializes_lowercase() {
        let def = TaskDefinition {
            description: "recycle".to_string(),
            schedule: "0 4 * * *".to_string(),
            command: vec![],
            action: TaskAction::Recycle,
            category: TaskCategory::Maintenance,
            enabled: true,
            timeout_secs: 300,
        };
        let json = serde_json::to_string(&def).unwrap();
        assert!(json.contains("\"action\":\"recycle\""), "got: {json}");
    }
}
