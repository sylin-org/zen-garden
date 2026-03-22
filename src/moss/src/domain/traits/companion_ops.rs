//! Companion registry operations trait.

use anyhow::Result;
use garden_common::command_manifest::CommandManifest;
use std::future::Future;

/// Companion registry operations.
///
/// The domain layer holds `Arc<dyn CompanionOps>` so it never depends
/// on the concrete `CompanionRegistry` in infra. API and task code
/// accesses companions through this trait.
pub trait CompanionOps: Send + Sync {
    // ── Discovery & Registration ─────────────────────────────────────

    /// Scan companion directory and auto-start enabled companions.
    fn scan_and_autostart(
        &self,
        moss_endpoint: &str,
    ) -> impl Future<Output = Result<(usize, usize)>> + Send;

    /// Rescan companion directory (clear and rebuild).
    fn refresh_all(&self) -> impl Future<Output = Result<usize>> + Send;

    // ── Query ────────────────────────────────────────────────────────

    /// List all registered companions (id, manifest, running state).
    fn list(&self) -> impl Future<Output = Vec<CompanionInfo>> + Send;

    /// Get a companion by ID.
    fn get(&self, id: &str) -> impl Future<Output = Option<CompanionInfo>> + Send;

    /// Get a companion's command manifest.
    fn get_manifest(&self, id: &str) -> impl Future<Output = Option<CommandManifest>> + Send;

    /// Check if a companion is running.
    fn is_running(&self, id: &str) -> impl Future<Output = bool> + Send;

    // ── Lifecycle ────────────────────────────────────────────────────

    /// Start a companion process.
    fn start(&self, id: &str, moss_endpoint: &str) -> impl Future<Output = Result<u32>> + Send;

    /// Stop a companion process.
    fn stop(&self, id: &str) -> impl Future<Output = Result<()>> + Send;

    /// Stop and disable a companion (no auto-start on boot).
    fn stop_and_disable(&self, id: &str) -> impl Future<Output = Result<()>> + Send;

    /// Enable a companion for auto-start.
    fn enable(&self, id: &str) -> impl Future<Output = Result<()>> + Send;

    /// Stop all companion processes.
    fn stop_all(&self) -> impl Future<Output = Vec<(String, Result<()>)>> + Send;

    // ── Process Management ───────────────────────────────────────────

    /// Reap terminated companion processes (prevent zombies).
    fn reap_terminated(&self) -> impl Future<Output = usize> + Send;

    // ── Shutdown ─────────────────────────────────────────────────────

    /// Send SIGTERM to all companion processes.
    fn sigterm_all(&self) -> impl Future<Output = ()> + Send;

    /// Force-kill any still-running companion processes.
    fn kill_all_survivors(&self) -> impl Future<Output = ()> + Send;
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
