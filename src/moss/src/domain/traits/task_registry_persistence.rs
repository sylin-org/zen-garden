//! Task registry persistence trait.

use anyhow::Result;
use std::future::Future;

use crate::domain::task_registry::TaskRegistry;

/// Persistence operations for the task registry.
///
/// Domain code uses this trait to load/save task registries
/// without depending on the concrete `TaskStore` in infra.
pub trait TaskRegistryPersistence: Send + Sync {
    /// Load the task registry from persistent storage.
    fn load_registry(&self) -> impl Future<Output = Result<TaskRegistry>> + Send;

    /// Save the task registry to persistent storage.
    fn save_registry(&self, registry: &TaskRegistry) -> impl Future<Output = Result<()>> + Send;
}
