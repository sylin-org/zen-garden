//! Companion registry operations trait.

use anyhow::Result;
use async_trait::async_trait;
use garden_common::command_manifest::CommandManifest;

/// Companion registry operations.
///
/// The domain layer holds `Arc<dyn CompanionOps>` so it never depends
/// on the concrete `CompanionRegistry` in infra. API and task code
/// accesses companions through this trait.
#[async_trait]
pub trait CompanionOps: Send + Sync {
    // ── Discovery & Registration ─────────────────────────────────────

    /// Scan companion directory and auto-start enabled companions.
    async fn scan_and_autostart(&self, moss_endpoint: &str) -> Result<(usize, usize)>;

    /// Rescan companion directory (clear and rebuild).
    async fn refresh_all(&self) -> Result<usize>;

    // ── Query ────────────────────────────────────────────────────────

    /// List all registered companions (id, manifest, running state).
    async fn list(&self) -> Vec<CompanionInfo>;

    /// Get a companion by ID.
    async fn get(&self, id: &str) -> Option<CompanionInfo>;

    /// Get a companion's command manifest.
    async fn get_manifest(&self, id: &str) -> Option<CommandManifest>;

    /// Check if a companion is running.
    async fn is_running(&self, id: &str) -> bool;

    // ── Lifecycle ────────────────────────────────────────────────────

    /// Start a companion process.
    async fn start(&self, id: &str, moss_endpoint: &str) -> Result<u32>;

    /// Stop a companion process.
    async fn stop(&self, id: &str) -> Result<()>;

    /// Stop and disable a companion (no auto-start on boot).
    async fn stop_and_disable(&self, id: &str) -> Result<()>;

    /// Enable a companion for auto-start.
    async fn enable(&self, id: &str) -> Result<()>;

    /// Stop all companion processes.
    async fn stop_all(&self) -> Vec<(String, Result<()>)>;

    // ── Process Management ───────────────────────────────────────────

    /// Reap terminated companion processes (prevent zombies).
    async fn reap_terminated(&self) -> usize;

    // ── Shutdown ─────────────────────────────────────────────────────

    /// Send SIGTERM to all companion processes.
    async fn sigterm_all(&self);

    /// Force-kill any still-running companion processes.
    async fn kill_all_survivors(&self);
}

/// Domain-visible companion information.
///
/// Pure data extracted from `RegisteredCompanion` — no OS handles.
#[derive(Debug, Clone)]
pub struct CompanionInfo {
    /// Companion identifier (folder name).
    pub id: String,
    /// Command manifest (capabilities, commands, metadata).
    pub manifest: CommandManifest,
    /// Whether the companion process is currently running.
    pub running: bool,
    /// Process ID (if running).
    pub pid: Option<u32>,
    /// Assigned command server port.
    pub port: Option<u16>,
}
