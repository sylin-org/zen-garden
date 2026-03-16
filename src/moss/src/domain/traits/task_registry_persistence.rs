//! Task registry persistence trait.

use anyhow::Result;
use async_trait::async_trait;

use crate::domain::task_registry::TaskRegistry;

/// Persistence operations for the task registry.
///
/// Domain code uses this trait to load/save task registries
/// without depending on the concrete `TaskStore` in infra.
#[async_trait]
pub trait TaskRegistryPersistence: Send + Sync {
    /// Load the task registry from persistent storage.
    async fn load_registry(&self) -> Result<TaskRegistry>;

    /// Save the task registry to persistent storage.
    async fn save_registry(&self, registry: &TaskRegistry) -> Result<()>;
}
