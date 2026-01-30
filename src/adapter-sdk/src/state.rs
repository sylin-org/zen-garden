//! Adapter state management
//!
//! Provides shared state for adapters including:
//! - Enabled/disabled state for SSE event handling
//! - Persistent state across restarts
//!
//! # Example
//!
//! ```ignore
//! use garden_adapter_sdk::AdapterState;
//!
//! // Create state with persistence
//! let state = AdapterState::new(Some("/var/lib/garden/my-adapter"));
//!
//! // Check if enabled
//! if state.is_enabled() {
//!     // Process SSE events
//! }
//!
//! // Toggle state (persisted automatically)
//! state.disable();  // Stops SSE event processing
//! state.enable();   // Resumes SSE event processing
//! ```

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

/// Shared adapter state
///
/// Thread-safe state container for adapter-wide settings.
/// Automatically persists enabled state to disk when changed.
pub struct AdapterState {
    /// Whether SSE event handling is enabled
    enabled: AtomicBool,

    /// Path to state directory for persistence
    state_dir: Option<PathBuf>,
}

impl AdapterState {
    /// Create new adapter state
    ///
    /// If `state_dir` is provided, the enabled state will be loaded from
    /// `{state_dir}/sse_enabled` and persisted on changes.
    ///
    /// # Arguments
    ///
    /// * `state_dir` - Optional directory for persistent state
    pub fn new(state_dir: Option<PathBuf>) -> Self {
        let enabled = Self::load_enabled_state(state_dir.as_ref());

        if let Some(ref dir) = state_dir {
            tracing::debug!(
                path = %dir.display(),
                enabled = enabled,
                "Loaded adapter state"
            );
        }

        Self {
            enabled: AtomicBool::new(enabled),
            state_dir,
        }
    }

    /// Check if SSE event handling is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Enable SSE event handling
    ///
    /// Persists the state to disk if a state directory was configured.
    pub fn enable(&self) {
        self.enabled.store(true, Ordering::Relaxed);
        self.persist_state();
        tracing::info!("SSE event handling enabled");
    }

    /// Disable SSE event handling
    ///
    /// Persists the state to disk if a state directory was configured.
    pub fn disable(&self) {
        self.enabled.store(false, Ordering::Relaxed);
        self.persist_state();
        tracing::info!("SSE event handling disabled");
    }

    /// Get the state directory path
    pub fn state_dir(&self) -> Option<&PathBuf> {
        self.state_dir.as_ref()
    }

    /// Load enabled state from file
    fn load_enabled_state(state_dir: Option<&PathBuf>) -> bool {
        state_dir
            .map(|dir| dir.join("sse_enabled"))
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|s| s.trim() == "on")
            .unwrap_or(true) // Default to enabled
    }

    /// Persist enabled state to file
    fn persist_state(&self) {
        if let Some(ref dir) = self.state_dir {
            let path = dir.join("sse_enabled");
            let state = if self.enabled.load(Ordering::Relaxed) {
                "on"
            } else {
                "off"
            };

            // Ensure directory exists
            if let Err(e) = std::fs::create_dir_all(dir) {
                tracing::warn!(error = %e, path = %dir.display(), "Failed to create state directory");
                return;
            }

            if let Err(e) = std::fs::write(&path, state) {
                tracing::warn!(error = %e, path = %path.display(), "Failed to persist SSE state");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_default_enabled() {
        let state = AdapterState::new(None);
        assert!(state.is_enabled());
    }

    #[test]
    fn test_enable_disable() {
        let state = AdapterState::new(None);

        state.disable();
        assert!(!state.is_enabled());

        state.enable();
        assert!(state.is_enabled());
    }

    #[test]
    fn test_persistence() {
        let dir = tempdir().unwrap();
        let state_dir = dir.path().to_path_buf();

        // Create state and disable
        {
            let state = AdapterState::new(Some(state_dir.clone()));
            state.disable();
        }

        // Verify file was written
        let content = fs::read_to_string(state_dir.join("sse_enabled")).unwrap();
        assert_eq!(content, "off");

        // Create new state - should load disabled
        let state = AdapterState::new(Some(state_dir));
        assert!(!state.is_enabled());
    }

    #[test]
    fn test_persistence_enable() {
        let dir = tempdir().unwrap();
        let state_dir = dir.path().to_path_buf();

        // Pre-create disabled state
        fs::create_dir_all(&state_dir).unwrap();
        fs::write(state_dir.join("sse_enabled"), "off").unwrap();

        // Load and enable
        let state = AdapterState::new(Some(state_dir.clone()));
        assert!(!state.is_enabled());

        state.enable();
        assert!(state.is_enabled());

        // Verify file was updated
        let content = fs::read_to_string(state_dir.join("sse_enabled")).unwrap();
        assert_eq!(content, "on");
    }
}
