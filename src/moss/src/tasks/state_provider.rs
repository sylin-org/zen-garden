//! State provider for election criteria evaluation

use crate::app_state::AppState;
use crate::version_string;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

/// State provider that extracts election-relevant fields from AppState
pub struct MossStateProvider {
    state: Arc<AppState>,
}

impl MossStateProvider {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    /// Get current state snapshot asynchronously
    pub async fn get_state_async(&self) -> HashMap<String, Value> {
        let mut fields = HashMap::new();

        // Basic identity
        fields.insert("stone_id".to_string(), json!(self.state.stone_id));
        fields.insert("stone_name".to_string(), json!(self.state.stone_name));

        // Version
        fields.insert("moss_version".to_string(), json!(version_string()));

        // Uptime (seconds since start)
        let uptime_secs = self.state.start_time.elapsed().as_secs();
        fields.insert("uptime".to_string(), json!(uptime_secs));

        // Health (from self_entry)
        let health = self.state.self_entry.read().await.health.clone();
        fields.insert("health".to_string(), json!(health));

        // Offering count
        let offerings_count = self.state.offerings.read().await.len();
        fields.insert("offerings".to_string(), json!(offerings_count));

        fields
    }
}

impl super::election_service::StateProvider for MossStateProvider {
    fn get_state(&self) -> HashMap<String, Value> {
        // For sync access, return minimal identity
        // Async criteria evaluation should use get_state_async() instead
        let mut fields = HashMap::new();
        fields.insert("stone_id".to_string(), json!(self.state.stone_id));
        fields.insert("stone_name".to_string(), json!(self.state.stone_name));
        fields.insert("moss_version".to_string(), json!(version_string()));
        fields
    }
}

/// Placeholder state provider for bootstrapping
/// Used temporarily before AppState is fully constructed
pub struct PlaceholderStateProvider;

impl super::election_service::StateProvider for PlaceholderStateProvider {
    fn get_state(&self) -> HashMap<String, Value> {
        HashMap::new() // No criteria matching during bootstrap
    }
}
