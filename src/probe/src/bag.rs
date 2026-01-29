//! The Bag - accumulates all state and history across test steps
//!
//! This is the valuable part: a holistic view of what happened during test execution.

use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BagError {
    #[error("Key not found: {0}")]
    NotFound(String),

    #[error("Failed to deserialize '{0}': {1}")]
    DeserializeError(String, String),
}

/// Result of a single step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StepResult {
    /// Step succeeded with optional output
    Ok(Option<serde_json::Value>),

    /// Step failed with error message
    Failed(String),

    /// Step was skipped (dependency failed, etc.)
    Skipped(String),

    /// Step timed out
    TimedOut { timeout_secs: u64 },
}

impl StepResult {
    pub fn ok() -> Self {
        Self::Ok(None)
    }

    pub fn ok_with<T: Serialize>(value: T) -> Self {
        Self::Ok(Some(serde_json::to_value(value).unwrap()))
    }

    pub fn failed(msg: impl Into<String>) -> Self {
        Self::Failed(msg.into())
    }

    pub fn is_success(&self) -> bool {
        matches!(self, Self::Ok(_))
    }
}

/// Record of a single step execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepRecord {
    pub step_id: String,
    pub description: String,
    pub started_at: DateTime<Utc>,
    pub duration_ms: u64,
    pub result: StepResult,
}

/// The Bag - accumulates everything across all steps
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Bag {
    /// Key-value store for passing data between steps
    entries: HashMap<String, serde_json::Value>,

    /// Step execution history (for holistic view)
    history: Vec<StepRecord>,

    /// When the bag was created
    pub created_at: Option<DateTime<Utc>>,
}

impl Bag {
    /// Create a new empty bag
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            history: Vec::new(),
            created_at: Some(Utc::now()),
        }
    }

    /// Create bag with initial values
    pub fn with_initial(initial: HashMap<String, serde_json::Value>) -> Self {
        Self {
            entries: initial,
            history: Vec::new(),
            created_at: Some(Utc::now()),
        }
    }

    /// Put a value in the bag
    pub fn put<T: Serialize>(&mut self, key: impl Into<String>, value: T) {
        self.entries
            .insert(key.into(), serde_json::to_value(value).unwrap());
    }

    /// Get a value from the bag
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.entries
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Get a required value - returns error if not found
    pub fn require<T: DeserializeOwned>(&self, key: &str) -> Result<T, BagError> {
        let value = self
            .entries
            .get(key)
            .ok_or_else(|| BagError::NotFound(key.into()))?;

        serde_json::from_value(value.clone())
            .map_err(|e| BagError::DeserializeError(key.into(), e.to_string()))
    }

    /// Check if a key exists
    pub fn has(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    /// Get all keys
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(|s| s.as_str())
    }

    /// Get raw JSON value
    pub fn get_raw(&self, key: &str) -> Option<&serde_json::Value> {
        self.entries.get(key)
    }

    /// Record a step execution
    pub fn record_step(
        &mut self,
        step_id: impl Into<String>,
        description: impl Into<String>,
        duration_ms: u64,
        result: StepResult,
    ) {
        self.history.push(StepRecord {
            step_id: step_id.into(),
            description: description.into(),
            started_at: Utc::now(),
            duration_ms,
            result,
        });
    }

    /// Get step history
    pub fn history(&self) -> &[StepRecord] {
        &self.history
    }

    /// Get the last step
    pub fn last_step(&self) -> Option<&StepRecord> {
        self.history.last()
    }

    /// Count successful steps
    pub fn successful_steps(&self) -> usize {
        self.history.iter().filter(|s| s.result.is_success()).count()
    }

    /// Count failed steps
    pub fn failed_steps(&self) -> usize {
        self.history
            .iter()
            .filter(|s| matches!(s.result, StepResult::Failed(_)))
            .count()
    }

    /// Check if all steps passed
    pub fn all_passed(&self) -> bool {
        self.history.iter().all(|s| s.result.is_success())
    }

    /// Get total duration of all steps
    pub fn total_duration_ms(&self) -> u64 {
        self.history.iter().map(|s| s.duration_ms).sum()
    }

    /// Pretty print the bag contents (for debugging)
    pub fn dump(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| format!("{:?}", self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bag_put_get() {
        let mut bag = Bag::new();
        bag.put("name", "redis");
        bag.put("port", 6379);

        assert_eq!(bag.get::<String>("name"), Some("redis".to_string()));
        assert_eq!(bag.get::<i32>("port"), Some(6379));
        assert_eq!(bag.get::<String>("missing"), None);
    }

    #[test]
    fn test_bag_require() {
        let mut bag = Bag::new();
        bag.put("name", "redis");

        assert!(bag.require::<String>("name").is_ok());
        assert!(bag.require::<String>("missing").is_err());
    }

    #[test]
    fn test_step_recording() {
        let mut bag = Bag::new();
        bag.record_step("deploy", "Deploy offering", 1234, StepResult::ok());
        bag.record_step(
            "wait",
            "Wait for healthy",
            5678,
            StepResult::ok_with(serde_json::json!({"attempts": 3})),
        );

        assert_eq!(bag.history().len(), 2);
        assert_eq!(bag.successful_steps(), 2);
        assert!(bag.all_passed());
        assert_eq!(bag.total_duration_ms(), 1234 + 5678);
    }
}
