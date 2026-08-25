//! Companion Registry
//!
//! Discovers and manages external Companions (Cricket, Firefly, etc.)
//! that extend Moss with additional capabilities.
//!
//! Discovery Process:
//! 1. On boot (or refresh), scan `{data_dir}/companions/` directory
//! 2. Each subfolder is an Companion: `Companions/{Companion-name}/Companion[.exe]`
//! 3. Spawn each Companion with `--dump-commands` flag
//! 4. Parse JSON output into CommandManifest
//! 5. Cache manifests for API queries
//!
//! Companions communicate via their own protocols (SSE, HTTP, etc.)
//! Moss just stores their command manifests for help/discovery.

use anyhow::{Context, Result};
use garden_common::command_manifest::CommandManifest;
use garden_common::constants::paths::{companions_dir, data_dir};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use garden_common::constants::{COMPANION_PORT_BASE, COMPANION_PORT_MAX};

/// Timeout for Companion --dump-commands execution
const DUMP_COMMANDS_TIMEOUT: Duration = Duration::from_secs(5);

/// Ledger file name for persisting port assignments
const PORT_LEDGER_FILE: &str = "companion-ports.json";

/// State file name for persisting Companion enabled/disabled state
const STATE_FILE: &str = "Companion-state.json";

/// Loopback timeout for companion `/health` probes during reconciliation.
/// Probes are sent over loopback to a process expected to respond
/// instantly; 500ms is generous and bounds worst-case startup.
const HEALTH_PROBE_TIMEOUT: Duration = Duration::from_millis(500);

/// Companion enabled/disabled state ledger - persisted to disk
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CompanionStateLedger {
    /// Map of companion_id -> enabled (true = start on boot, false = disabled by user)
    /// Companions not in this map default to enabled
    enabled: HashMap<String, bool>,
}

impl CompanionStateLedger {
    /// Load from disk or create new (all enabled by default)
    async fn load(data_path: &Path) -> Self {
        let state_path = data_path.join(STATE_FILE);
        if state_path.exists() {
            match tokio::fs::read_to_string(&state_path).await {
                Ok(content) => match serde_json::from_str(&content) {
                    Ok(state) => return state,
                    Err(e) => warn!(error = %e, "Failed to parse Companion state, using defaults"),
                },
                Err(e) => warn!(error = %e, "Failed to read Companion state, using defaults"),
            }
        }
        Self {
            enabled: HashMap::new(),
        }
    }

    /// Save to disk
    async fn save(&self, data_path: &Path) -> Result<()> {
        let state_path = data_path.join(STATE_FILE);
        let content = serde_json::to_string_pretty(self)?;
        tokio::fs::write(&state_path, content).await?;
        Ok(())
    }

    /// Check if Companion is enabled (defaults to true if not in map)
    fn is_enabled(&self, companion_id: &str) -> bool {
        self.enabled.get(companion_id).copied().unwrap_or(true)
    }

    /// Set Companion enabled state
    fn set_enabled(&mut self, companion_id: &str, enabled: bool) {
        self.enabled.insert(companion_id.to_string(), enabled);
    }
}

/// Port assignment ledger - persisted to disk
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PortLedger {
    /// Map of companion_id -> assigned port
    assignments: HashMap<String, u16>,
    /// Next port to assign (starts at companion_port_BASE)
    next_port: u16,
}

impl PortLedger {
    /// Load from disk or create new
    async fn load(data_path: &Path) -> Self {
        let ledger_path = data_path.join(PORT_LEDGER_FILE);
        if ledger_path.exists() {
            match tokio::fs::read_to_string(&ledger_path).await {
                Ok(content) => match serde_json::from_str(&content) {
                    Ok(ledger) => return ledger,
                    Err(e) => warn!(error = %e, "Failed to parse port ledger, starting fresh"),
                },
                Err(e) => warn!(error = %e, "Failed to read port ledger, starting fresh"),
            }
        }
        Self {
            assignments: HashMap::new(),
            next_port: COMPANION_PORT_BASE,
        }
    }

    /// Save to disk
    async fn save(&self, data_path: &Path) -> Result<()> {
        let ledger_path = data_path.join(PORT_LEDGER_FILE);
        let content = serde_json::to_string_pretty(self)?;
        tokio::fs::write(&ledger_path, content).await?;
        Ok(())
    }

    /// Get or assign a port for an Companion
    fn get_or_assign(&mut self, companion_id: &str) -> Result<u16> {
        // Return existing assignment
        if let Some(&port) = self.assignments.get(companion_id) {
            return Ok(port);
        }

        // Assign new port
        if self.next_port > COMPANION_PORT_MAX {
            return Err(anyhow::anyhow!(
                "Port pool exhausted ({}-{}). Cannot register more companions.",
                COMPANION_PORT_BASE,
                COMPANION_PORT_MAX
            ));
        }

        let port = self.next_port;
        self.next_port += 1;
        self.assignments.insert(companion_id.to_string(), port);

        info!(companion = %companion_id, port = port, "Assigned port to Companion");
        Ok(port)
    }

    /// Get port for an Companion (if assigned)
    #[expect(dead_code)]
    fn get(&self, companion_id: &str) -> Option<u16> {
        self.assignments.get(companion_id).copied()
    }
}

/// Registered Companion with its manifest and metadata
#[derive(Debug)]
pub struct RegisteredCompanion {
    /// Companion identifier (folder name)
    pub id: String,

    /// Path to the Companion executable
    pub executable: PathBuf,

    /// Command manifest (parsed from --dump-commands output)
    pub manifest: CommandManifest,

    /// Running process handle (if started by us)
    process: Option<Child>,

    /// Process ID. Present when we spawned the process; may be `None`
    /// for adopted companions whose `Child` handle we lost across a moss
    /// restart. Adoption-time PID lookup is best-effort and not required
    /// for liveness — see [COMPANION-0016].
    pid: Option<u32>,

    /// Assigned command server port (always set after registration).
    assigned_port: Option<u16>,

    /// Liveness flag — see [COMPANION-0016]. Set by `start` (we just
    /// spawned it) or `mark_adopted` (a `/health` probe succeeded).
    /// Cleared by `stop`. Authoritative for `is_running`; PID is
    /// bookkeeping only.
    alive: bool,
}

impl Clone for RegisteredCompanion {
    fn clone(&self) -> Self {
        // Process handle is not cloned - only metadata
        Self {
            id: self.id.clone(),
            executable: self.executable.clone(),
            manifest: self.manifest.clone(),
            process: None,
            pid: self.pid,
            assigned_port: self.assigned_port,
            alive: self.alive,
        }
    }
}

impl RegisteredCompanion {
    /// Whether the Companion is considered alive. Source of truth is the
    /// `alive` flag, which is set by spawn or by a successful `/health`
    /// probe at adoption time. PID is no longer consulted.
    pub fn is_running(&self) -> bool {
        self.alive
    }

    /// Get the process ID if running and known. May return `None` for
    /// adopted companions whose PID was never resolved.
    pub fn pid(&self) -> Option<u32> {
        if self.alive { self.pid } else { None }
    }

    /// Get the assigned command port (always available once registered)
    pub fn port(&self) -> Option<u16> {
        self.assigned_port
    }
}

/// Companion registry - discovers and caches Companion manifests
#[derive(Debug)]
pub struct CompanionRegistry {
    /// Registered Companions by ID
    companions: Arc<RwLock<HashMap<String, RegisteredCompanion>>>,

    /// Path to Companions directory
    companions_path: PathBuf,

    /// Path to data directory (for ledger persistence)
    data_path: PathBuf,

    /// Port assignment ledger
    port_ledger: Arc<RwLock<PortLedger>>,

    /// Companion enabled/disabled state ledger
    state_ledger: Arc<RwLock<CompanionStateLedger>>,
}

impl CompanionRegistry {
    /// Create a new Companion registry. Loads port and state ledgers
    /// from disk; liveness is determined at reconcile time via
    /// `/health` probes (COMPANION-0016) — no PID ledger is persisted.
    pub async fn new() -> Self {
        let data_path = PathBuf::from(data_dir());
        let port_ledger = PortLedger::load(&data_path).await;
        let state_ledger = CompanionStateLedger::load(&data_path).await;

        Self {
            companions: Arc::new(RwLock::new(HashMap::new())),
            companions_path: PathBuf::from(companions_dir()),
            data_path,
            port_ledger: Arc::new(RwLock::new(port_ledger)),
            state_ledger: Arc::new(RwLock::new(state_ledger)),
        }
    }

    /// Create with custom Companions directory (for testing)
    pub async fn with_path(companions_path: PathBuf, data_path: PathBuf) -> Self {
        let port_ledger = PortLedger::load(&data_path).await;
        let state_ledger = CompanionStateLedger::load(&data_path).await;

        Self {
            companions: Arc::new(RwLock::new(HashMap::new())),
            companions_path,
            data_path,
            port_ledger: Arc::new(RwLock::new(port_ledger)),
            state_ledger: Arc::new(RwLock::new(state_ledger)),
        }
    }

    /// Scan Companions directory and register all found Companions
    pub async fn scan(&self) -> Result<usize> {
        // Self-heal: a legacy installer (the old `moss-update-helper.sh`, still the ExecStartPre on
        // stones whose unit predates the `garden-moss pre-start` migration) hardcodes companions to
        // `bin_install/companions`, but we scan `profile().paths.companions`. Move any found there
        // into the scan dir so the scan registers them regardless of which installer ran.
        consolidate_legacy_companions(&self.companions_path);

        let companions_path = &self.companions_path;

        // Ensure directory exists
        if !companions_path.exists() {
            tokio::fs::create_dir_all(companions_path)
                .await
                .context("Failed to create Companions directory")?;
            return Ok(0);
        }

        let mut found = Vec::new();
        let mut entries = tokio::fs::read_dir(companions_path).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let companion_id = match path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };

            // Look for executable
            if let Some(executable) = find_companion_executable(&path).await {
                found.push((companion_id, executable));
            }
        }

        // Register each Companion
        let count = found.len();
        for (companion_id, executable) in found {
            match self.register_companion(&companion_id, &executable).await {
                Ok(()) => info!(companion = %companion_id, "Registered Companion"),
                Err(e) => {
                    warn!(companion = %companion_id, error = %e, "Failed to register Companion")
                }
            }
        }

        info!(count = count, "Companion scan complete");
        Ok(count)
    }

    /// Scan Companions directory, register, and auto-start enabled Companions
    ///
    /// This is the main entry point for Companion initialization at boot.
    /// Companions are started unless explicitly disabled by the user.
    ///
    /// On restart, this method first reconciles with any Companions that survived
    /// the previous Moss session (thanks to kill_on_drop(false)), then only
    /// starts Companions that aren't already running.
    pub async fn scan_and_autostart(&self, moss_endpoint: &str) -> Result<(usize, usize)> {
        // First scan and register all Companions
        let registered = self.scan().await?;

        if registered == 0 {
            return Ok((0, 0));
        }

        // Reconcile with any Companions still running from previous session
        let (adopted, _dead) = self.reconcile_running_companions().await;

        // Get list of companion IDs to potentially start
        let companion_ids: Vec<String> = {
            let companions = self.companions.read().await;
            companions.keys().cloned().collect()
        };

        // Check state ledger and start enabled Companions (if not already running)
        let state_ledger = self.state_ledger.read().await;
        let mut started = 0;

        for companion_id in companion_ids {
            // Skip if already running (adopted from previous session)
            if self.is_running(&companion_id).await {
                debug!(companion = %companion_id, "Companion already running, skipping start");
                continue;
            }

            if state_ledger.is_enabled(&companion_id) {
                match self.start(&companion_id, moss_endpoint).await {
                    Ok(pid) => {
                        info!(companion = %companion_id, pid = pid, "Auto-started Companion");
                        started += 1;
                    }
                    Err(e) => {
                        warn!(companion = %companion_id, error = %e, "Failed to auto-start Companion");
                    }
                }
            } else {
                info!(companion = %companion_id, "Companion disabled, skipping auto-start");
            }
        }

        info!(
            registered = registered,
            adopted = adopted,
            started = started,
            "Companion scan and auto-start complete"
        );
        Ok((registered, started + adopted))
    }

    /// Register a single Companion by running --dump-commands
    /// Gets or assigns a port from the ledger and passes it to the Companion
    async fn register_companion(&self, companion_id: &str, executable: &Path) -> Result<()> {
        // Get or assign port from ledger
        let port = {
            let mut ledger = self.port_ledger.write().await;
            let port = ledger.get_or_assign(companion_id)?;
            // Persist ledger after assignment
            if let Err(e) = ledger.save(&self.data_path).await {
                warn!(error = %e, "Failed to persist port ledger");
            }
            port
        };

        // Call --dump-commands with --port argument
        let manifest = invoke_dump_commands(executable, port)
            .await
            .with_context(|| format!("Failed to get manifest from Companion {}", companion_id))?;

        let companion = RegisteredCompanion {
            id: companion_id.to_string(),
            executable: executable.to_path_buf(),
            manifest,
            process: None,
            pid: None,
            assigned_port: Some(port),
            alive: false,
        };

        let mut companions = self.companions.write().await;
        companions.insert(companion_id.to_string(), companion);

        info!(companion = %companion_id, port = port, "Registered Companion");
        Ok(())
    }

    /// Get all registered Companions
    pub(crate) async fn list_registered(&self) -> Vec<RegisteredCompanion> {
        let companions = self.companions.read().await;
        companions.values().cloned().collect()
    }

    /// Get a specific Companion by ID (returns infra type)
    pub(crate) async fn get_registered(&self, id: &str) -> Option<RegisteredCompanion> {
        let companions = self.companions.read().await;
        companions.get(id).cloned()
    }

    /// Get Companion manifest by ID
    pub async fn get_manifest(&self, id: &str) -> Option<CommandManifest> {
        let companions = self.companions.read().await;
        companions.get(id).map(|a| a.manifest.clone())
    }

    /// Refresh a specific Companion's manifest
    pub async fn refresh(&self, id: &str) -> Result<()> {
        let executable = {
            let companions = self.companions.read().await;
            match companions.get(id) {
                Some(a) => a.executable.clone(),
                None => return Err(anyhow::anyhow!("Companion not found: {}", id)),
            }
        };

        self.register_companion(id, &executable).await
    }

    /// Refresh all Companions (rescan directory)
    pub async fn refresh_all(&self) -> Result<usize> {
        // Clear existing
        {
            let mut companions_guard = self.companions.write().await;
            companions_guard.clear();
        }

        // Rescan
        self.scan().await
    }

    /// Get Companion count
    pub async fn count(&self) -> usize {
        let companions = self.companions.read().await;
        companions.len()
    }

    /// Check if an Companion is running
    pub async fn is_running(&self, id: &str) -> bool {
        let companions = self.companions.read().await;
        companions.get(id).map(|a| a.is_running()).unwrap_or(false)
    }

    /// Get assigned port for a running Companion
    pub async fn get_port(&self, id: &str) -> Option<u16> {
        let companions = self.companions.read().await;
        companions.get(id).and_then(|a| a.port())
    }

    /// Start an Companion process
    ///
    /// Spawns the Companion executable as a background process.
    /// Uses the pre-assigned port from the ledger.
    pub async fn start(&self, id: &str, moss_endpoint: &str) -> Result<u32> {
        let mut companions = self.companions.write().await;

        // Get companion and check state
        let c = companions
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("Companion not found: {}", id))?;

        if c.is_running()
            && let Some(pid) = c.pid
        {
            info!(companion = %id, pid = pid, "Companion already running");
            return Ok(pid);
        }

        // Port was assigned during registration
        let port = c
            .assigned_port
            .ok_or_else(|| anyhow::anyhow!("Companion '{}' has no assigned port", id))?;

        let executable = c.executable.clone();

        // Now get mutable reference
        let companion = companions.get_mut(id).unwrap(); // Safe: we checked above

        // Spawn the Companion process with --stone and --port arguments
        info!(
            companion = %id,
            executable = %executable.display(),
            endpoint = %moss_endpoint,
            port = port,
            "Starting Companion"
        );

        // Create Companion-specific state directory
        let companion_state_dir = self.data_path.join("companions").join(id);

        let child = Command::new(&executable)
            .arg("--stone")
            .arg(moss_endpoint)
            .arg("--port")
            .arg(port.to_string())
            .arg("--state-dir")
            .arg(&companion_state_dir)
            .kill_on_drop(false) // Keep running if Moss restarts
            .spawn()
            .with_context(|| format!("Failed to start Companion {}", id))?;

        let pid = child.id().unwrap_or(0);
        companion.process = Some(child);
        companion.pid = Some(pid);
        companion.assigned_port = Some(port);
        companion.alive = true;

        info!(companion = %id, pid = pid, port = port, "Companion started");
        Ok(pid)
    }

    /// Stop an Companion process (does NOT disable it - will restart on next boot)
    pub async fn stop(&self, id: &str) -> Result<()> {
        self.stop_internal(id).await
    }

    /// Stop an Companion and disable it (will NOT restart on next boot)
    ///
    /// Use this when user explicitly wants to turn off an Companion.
    pub async fn stop_and_disable(&self, id: &str) -> Result<()> {
        // First stop the process
        self.stop_internal(id).await?;

        // Then persist disabled state
        {
            let mut state_ledger = self.state_ledger.write().await;
            state_ledger.set_enabled(id, false);
            if let Err(e) = state_ledger.save(&self.data_path).await {
                warn!(companion = %id, error = %e, "Failed to persist Companion disabled state");
            }
        }

        info!(companion = %id, "Companion stopped and disabled (will not auto-start)");
        Ok(())
    }

    /// Enable an Companion (will auto-start on next boot or can be started manually)
    pub async fn enable(&self, id: &str) -> Result<()> {
        // Verify Companion exists
        {
            let companions = self.companions.read().await;
            if !companions.contains_key(id) {
                return Err(anyhow::anyhow!("Companion not found: {}", id));
            }
        }

        // Persist enabled state
        {
            let mut state_ledger = self.state_ledger.write().await;
            state_ledger.set_enabled(id, true);
            if let Err(e) = state_ledger.save(&self.data_path).await {
                warn!(companion = %id, error = %e, "Failed to persist Companion enabled state");
            }
        }

        info!(companion = %id, "Companion enabled (will auto-start on next boot)");
        Ok(())
    }

    /// Check if an Companion is enabled (will auto-start on boot)
    pub async fn is_enabled(&self, id: &str) -> bool {
        let state_ledger = self.state_ledger.read().await;
        state_ledger.is_enabled(id)
    }

    /// Reap terminated Companion processes
    ///
    /// Checks all Companions with process handles and calls try_wait() to collect
    /// exit status from terminated processes. This prevents zombie processes.
    ///
    /// Returns the number of processes reaped.
    pub async fn reap_terminated(&self) -> usize {
        let mut companions = self.companions.write().await;
        let mut reaped = 0;

        for (id, c) in companions.iter_mut() {
            // Skip companions without a process handle
            let child = match c.process.as_mut() {
                Some(ch) => ch,
                None => continue,
            };

            // Try to collect exit status without blocking
            match child.try_wait() {
                Ok(Some(status)) => {
                    let pid = c.pid.unwrap_or(0);
                    if status.success() {
                        info!(companion = %id, pid = pid, "Companion exited normally");
                    } else {
                        warn!(companion = %id, pid = pid, status = ?status, "Companion exited with error");
                    }

                    c.process = None;
                    c.pid = None;
                    c.alive = false;
                    reaped += 1;
                }
                Ok(None) => {
                    // Process still running - nothing to do
                }
                Err(e) => {
                    warn!(companion = %id, error = %e, "Failed to check companion process status");
                }
            }
        }

        if reaped > 0 {
            debug!(reaped = reaped, "Reaped terminated Companion processes");
        }

        reaped
    }

    /// Reconcile running Companions after Moss restart.
    ///
    /// On restart, Moss has no `Child` handle for Companions that
    /// survived the previous session (they were spawned with
    /// `kill_on_drop(false)`). Liveness is determined by probing
    /// `GET http://127.0.0.1:{port}/health` on each registered
    /// Companion's assigned port — a companion's HTTP service is the
    /// authoritative liveness signal (COMPANION-0016).
    ///
    /// Returns `(adopted_count, _dead_unused)`. The second value is
    /// retained for callsite signature stability and is always 0; the
    /// PID-ledger "dead" notion is gone with the ledger itself.
    pub async fn reconcile_running_companions(&self) -> (usize, usize) {
        // Snapshot (id, port) under read lock; probe outside the lock
        // so concurrent calls aren't serialized on each other.
        let to_probe: Vec<(String, u16)> = {
            let companions = self.companions.read().await;
            companions
                .iter()
                .filter_map(|(id, c)| c.assigned_port.map(|p| (id.clone(), p)))
                .collect()
        };

        let mut adopted = 0;
        let mut companions_guard = self.companions.write().await;
        for (id, port) in to_probe {
            if companion_health_probe(port).await {
                if let Some(c) = companions_guard.get_mut(&id) {
                    c.alive = true;
                    // PID intentionally not resolved — adoption only
                    // needs liveness. Shutdown targeting handles a
                    // missing PID via HTTP /shutdown + best-effort
                    // port→PID lookup.
                    c.pid = None;
                    info!(
                        companion = %id,
                        port = port,
                        "Adopted running Companion via /health probe"
                    );
                    adopted += 1;
                }
            } else {
                debug!(
                    companion = %id,
                    port = port,
                    "Companion did not respond to /health — will spawn fresh"
                );
            }
        }

        if adopted > 0 {
            info!(adopted = adopted, "Reconciled Companion processes via health probe");
        }

        (adopted, 0)
    }

    /// Internal stop implementation. Prefers the `Child` handle when we
    /// own it (freshly-spawned companions) and falls back to PID-based
    /// kill — resolving the PID via `find_pid_on_port` for adopted
    /// companions whose handle we lost across a restart.
    async fn stop_internal(&self, id: &str) -> Result<()> {
        let mut companions_guard = self.companions.write().await;
        let c = companions_guard
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("Companion not found: {}", id))?;

        if c.alive {
            info!(companion = %id, pid = ?c.pid, "Stopping Companion");

            if let Some(ref mut child) = c.process {
                if let Err(e) = child.kill().await {
                    warn!(companion = %id, error = %e, "Failed to kill Companion via handle, trying by PID");
                    if let Some(pid) = c.pid {
                        kill_process_by_pid(pid);
                    }
                }
                let _ = child.wait().await;
            } else if let Some(pid) = c.pid.or_else(|| c.assigned_port.and_then(find_pid_on_port)) {
                kill_process_by_pid(pid);
            }

            info!(companion = %id, "Companion stopped");
        }

        c.process = None;
        c.pid = None;
        c.alive = false;

        Ok(())
    }

    /// SIGTERM all running Companions immediately (non-blocking, no waiting)
    ///
    /// Sends SIGTERM to every companion process so they begin graceful shutdown.
    /// Does NOT wait for them to exit — call `kill_all_survivors()` later for cleanup.
    /// Used at the start of Moss shutdown to give companions a head start on cleanup.
    pub async fn sigterm_all(&self) {
        let companions = self.companions.read().await;
        for (id, c) in companions.iter() {
            if !c.alive {
                continue;
            }
            let pid = c.pid.or_else(|| c.assigned_port.and_then(find_pid_on_port));
            if let Some(pid) = pid {
                info!(companion = %id, pid = pid, "Sending SIGTERM to Companion");
                sigterm_process_by_pid(pid);
            } else {
                debug!(companion = %id, "SIGTERM skipped — no PID resolvable");
            }
        }
    }

    /// SIGKILL all companion processes that are still alive
    ///
    /// Called just before Moss exits to ensure no orphaned companions keep
    /// the systemd CGroup alive and delay the unit transition to `inactive`.
    pub async fn kill_all_survivors(&self) {
        let companions = self.companions.read().await;
        for (id, c) in companions.iter() {
            if !c.alive {
                continue;
            }
            let pid = c.pid.or_else(|| c.assigned_port.and_then(find_pid_on_port));
            if let Some(pid) = pid
                && is_process_alive(pid)
            {
                warn!(companion = %id, pid = pid, "Companion still alive after drain, sending SIGKILL");
                kill_process_by_pid(pid);
            }
        }
    }

    /// Stop all running Companions
    ///
    /// Used during package deployment to ensure clean upgrade.
    /// Attempts graceful HTTP shutdown first, then force kills.
    pub async fn stop_all(&self) -> Vec<(String, Result<()>)> {
        let companion_ids: Vec<String> = {
            let companions = self.companions.read().await;
            companions.keys().cloned().collect()
        };

        let mut results = Vec::new();

        for id in companion_ids {
            if self.is_running(&id).await {
                info!(companion = %id, "Stopping Companion for upgrade");

                // Try graceful HTTP shutdown first
                if let Some(port) = self.get_port(&id).await {
                    let shutdown_url = format!("http://127.0.0.1:{}/shutdown", port);
                    match crate::http::COMPANION
                        .post(&shutdown_url)
                        .timeout(std::time::Duration::from_secs(2))
                        .send()
                        .await
                    {
                        Ok(response) if response.status().is_success() => {
                            info!(companion = %id, "Graceful shutdown via HTTP");
                            // Give it a moment to clean up
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        }
                        Ok(_) | Err(_) => {
                            debug!(companion = %id, "HTTP shutdown failed, will force stop");
                        }
                    }
                }

                // Force stop if still running
                let result = self.stop(&id).await;
                results.push((id, result));
            }
        }

        results
    }
}

impl crate::domain::traits::CompanionOps for CompanionRegistry {
    async fn scan_and_autostart(&self, moss_endpoint: &str) -> Result<(usize, usize)> {
        CompanionRegistry::scan_and_autostart(self, moss_endpoint).await
    }

    async fn refresh_all(&self) -> Result<usize> {
        CompanionRegistry::refresh_all(self).await
    }

    async fn list(&self) -> Vec<crate::domain::traits::companion_ops::CompanionInfo> {
        let companions = CompanionRegistry::list_registered(self).await;
        companions
            .into_iter()
            .map(|c| {
                let running = c.is_running();
                let pid = c.pid();
                let port = c.port();
                crate::domain::traits::companion_ops::CompanionInfo {
                    id: c.id,
                    pid,
                    port,
                    manifest: c.manifest,
                    running,
                }
            })
            .collect()
    }

    async fn get(&self, id: &str) -> Option<crate::domain::traits::companion_ops::CompanionInfo> {
        CompanionRegistry::get_registered(self, id).await.map(|c| {
            let running = c.is_running();
            let pid = c.pid();
            let port = c.port();
            crate::domain::traits::companion_ops::CompanionInfo {
                id: c.id,
                pid,
                port,
                manifest: c.manifest,
                running,
            }
        })
    }

    async fn get_manifest(&self, id: &str) -> Option<CommandManifest> {
        CompanionRegistry::get_manifest(self, id).await
    }

    async fn is_running(&self, id: &str) -> bool {
        CompanionRegistry::is_running(self, id).await
    }

    async fn start(&self, id: &str, moss_endpoint: &str) -> Result<u32> {
        CompanionRegistry::start(self, id, moss_endpoint).await
    }

    async fn stop(&self, id: &str) -> Result<()> {
        CompanionRegistry::stop(self, id).await
    }

    async fn stop_and_disable(&self, id: &str) -> Result<()> {
        CompanionRegistry::stop_and_disable(self, id).await
    }

    async fn enable(&self, id: &str) -> Result<()> {
        CompanionRegistry::enable(self, id).await
    }

    async fn stop_all(&self) -> Vec<(String, Result<()>)> {
        CompanionRegistry::stop_all(self).await
    }

    async fn reap_terminated(&self) -> usize {
        CompanionRegistry::reap_terminated(self).await
    }

    async fn sigterm_all(&self) {
        CompanionRegistry::sigterm_all(self).await
    }

    async fn kill_all_survivors(&self) {
        CompanionRegistry::kill_all_survivors(self).await
    }
}

/// Probe a Companion's `/health` endpoint. Returns true on a 2xx
/// response within `HEALTH_PROBE_TIMEOUT`. Source of truth for
/// adoption — see COMPANION-0016.
async fn companion_health_probe(port: u16) -> bool {
    let url = format!("http://127.0.0.1:{}/health", port);
    match crate::http::COMPANION
        .get(&url)
        .timeout(HEALTH_PROBE_TIMEOUT)
        .send()
        .await
    {
        Ok(r) => r.status().is_success(),
        Err(_) => false,
    }
}

/// Best-effort lookup of the PID listening on a TCP port (loopback).
/// Used only as a fallback when shutting down an adopted Companion
/// whose `Child` handle was lost across a moss restart. Returns `None`
/// if the lookup is unsupported on this platform or finds nothing.
fn find_pid_on_port(port: u16) -> Option<u32> {
    #[cfg(unix)]
    {
        use std::process::Command as StdCommand;
        // ss -H -t -l -n -p sport = :PORT  →  one line per match
        let out = StdCommand::new("ss")
            .args([
                "-Htlnp",
                "sport",
                &format!("= :{}", port),
            ])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        // ss -p emits e.g.  users:(("garden-firefly",pid=1373,fd=12))
        for chunk in text.split("pid=").skip(1) {
            let digits: String = chunk.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(pid) = digits.parse::<u32>() {
                return Some(pid);
            }
        }
        None
    }
    #[cfg(windows)]
    {
        use std::process::Command as StdCommand;
        let out = StdCommand::new("netstat").args(["-ano", "-p", "TCP"]).output().ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let needle = format!(":{} ", port);
        for line in text.lines() {
            if line.contains(&needle)
                && line.contains("LISTENING")
                && let Some(pid_str) = line.split_whitespace().last()
                && let Ok(pid) = pid_str.parse::<u32>()
            {
                return Some(pid);
            }
        }
        None
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = port;
        None
    }
}

/// Check if a process is alive by PID
fn is_process_alive(pid: u32) -> bool {
    #[cfg(windows)]
    {
        use std::process::Command as StdCommand;
        // Use tasklist to check if process exists
        StdCommand::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid), "/NH"])
            .output()
            .map(|o| {
                let output = String::from_utf8_lossy(&o.stdout);
                output.contains(&pid.to_string())
            })
            .unwrap_or(false)
    }

    #[cfg(unix)]
    {
        // On Unix, check /proc/{pid} or use kill -0
        std::path::Path::new(&format!("/proc/{}", pid)).exists()
    }
}

/// Kill a process by PID (SIGKILL)
fn kill_process_by_pid(pid: u32) {
    #[cfg(windows)]
    {
        use std::process::Command as StdCommand;
        let _ = StdCommand::new("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .output();
    }

    #[cfg(unix)]
    {
        use std::process::Command as StdCommand;
        let _ = StdCommand::new("kill")
            .args(["-9", &pid.to_string()])
            .output();
    }
}

/// Send SIGTERM to a process by PID (graceful shutdown request)
fn sigterm_process_by_pid(pid: u32) {
    #[cfg(windows)]
    {
        // Windows has no SIGTERM — use taskkill without /F for graceful
        use std::process::Command as StdCommand;
        let _ = StdCommand::new("taskkill")
            .args(["/PID", &pid.to_string()])
            .output();
    }

    #[cfg(unix)]
    {
        use std::process::Command as StdCommand;
        let _ = StdCommand::new("kill")
            .args(["-15", &pid.to_string()])
            .output();
    }
}

/// Find the Companion executable in a directory
///
/// Scans for any executable file in the Companion folder.
/// On Windows: looks for .exe files
/// On Linux: looks for files with execute permission
/// Move companions a legacy installer left in `bin_install/companions` into the canonical scan dir.
///
/// The legacy `moss-update-helper.sh` updater hardcodes `/usr/local/bin/companions`
/// (= `bin_install/companions` on a standard Linux stone), while the running moss scans
/// `profile().paths.companions` (`{data}/companions`). On a stone still running that legacy
/// ExecStartPre, companions land outside the scan dir and never register. Copy any the scan dir is
/// MISSING into it (never overwriting one already present, so a healthy stone with a stale legacy
/// dir beside the current one is untouched). Idempotent — once the scan dir holds the binary, the
/// `has_companion_binary` guard short-circuits.
fn consolidate_legacy_companions(scan_dir: &Path) {
    let legacy = garden_common::host::profile().paths.bin_install.join("companions");
    if legacy == *scan_dir || !legacy.exists() {
        return;
    }
    let Ok(companions) = std::fs::read_dir(&legacy) else {
        return;
    };
    for entry in companions.flatten() {
        let src = entry.path();
        if !src.is_dir() {
            continue;
        }
        let dest = scan_dir.join(entry.file_name());
        if has_companion_binary(&dest) {
            continue; // scan dir already has this companion — never overwrite
        }
        if std::fs::create_dir_all(&dest).is_err() {
            continue;
        }
        if let Ok(files) = std::fs::read_dir(&src) {
            for file in files.flatten() {
                let fp = file.path();
                if fp.is_file() {
                    let _ = std::fs::copy(&fp, dest.join(file.file_name()));
                }
            }
        }
        tracing::info!(
            companion = %entry.file_name().to_string_lossy(),
            from = %legacy.display(),
            to = %dest.display(),
            "consolidated legacy companion into the scan dir"
        );
    }
}

/// True if `dir` holds a runnable companion binary — same name shape as `find_companion_executable`
/// (no extension on unix, `.exe` on Windows), so a `.old` backup or data file does not count.
fn has_companion_binary(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        #[cfg(windows)]
        if path.extension().map(|e| e == "exe").unwrap_or(false) {
            return true;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if path.extension().is_none()
                && path
                    .metadata()
                    .map(|m| m.permissions().mode() & 0o111 != 0)
                    .unwrap_or(false)
            {
                return true;
            }
        }
    }
    false
}

async fn find_companion_executable(companion_dir: &Path) -> Option<PathBuf> {
    // Find the companion binary. It is named `garden-<id>` with no extension on unix (`.exe` on
    // Windows). Other files in the folder may carry the executable bit and MUST NOT be launched —
    // most importantly the DEPLOY-0001 `.old` rollback backup that sits beside the new binary during
    // the apply/mark-good window (launching it would run the *previous* build: the firefly/cricket
    // "stuck idle" regression), plus data files like `device-bus-cache.json`. Requiring the binary's
    // name shape (no extension / `.exe`) excludes all of them regardless of readdir order.
    if let Ok(mut entries) = tokio::fs::read_dir(companion_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.is_file() {
                let is_executable = if cfg!(windows) {
                    // `garden-<id>.exe`; a `.exe.old` backup has extension `old`, so it is skipped.
                    path.extension().map(|e| e == "exe").unwrap_or(false)
                } else {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        path.extension().is_none()
                            && path
                                .metadata()
                                .map(|m| m.permissions().mode() & 0o111 != 0)
                                .unwrap_or(false)
                    }
                    #[cfg(not(unix))]
                    false
                };

                if is_executable {
                    debug!(path = %path.display(), "Found companion executable");
                    return Some(path);
                }
            }
        }
    }

    None
}

/// Invoke an Companion with --dump-commands and --port, parse the output
async fn invoke_dump_commands(executable: &Path, port: u16) -> Result<CommandManifest> {
    debug!(executable = %executable.display(), port = port, "Invoking --dump-commands");

    let output = tokio::time::timeout(
        DUMP_COMMANDS_TIMEOUT,
        Command::new(executable)
            .arg("--dump-commands")
            .arg("--port")
            .arg(port.to_string())
            .output(),
    )
    .await
    .context("Companion timed out")?
    .context("Failed to execute Companion")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!(
            "Companion exited with {}: {}",
            output.status,
            stderr.trim()
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let manifest: CommandManifest =
        serde_json::from_str(&stdout).context("Failed to parse Companion manifest JSON")?;

    debug!(
        companion_id = %manifest.id,
        commands = manifest.commands.len(),
        port = port,
        "Parsed Companion manifest"
    );

    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_registry_new() {
        let registry = CompanionRegistry::new().await;
        assert_eq!(registry.count().await, 0);
    }

    #[tokio::test]
    async fn test_registry_scan_empty_dir() {
        let temp_dir = std::env::temp_dir().join("zen-garden-test-Companions");
        let data_dir = std::env::temp_dir().join("zen-garden-test-data");
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
        let _ = tokio::fs::remove_dir_all(&data_dir).await;

        let registry = CompanionRegistry::with_path(temp_dir.clone(), data_dir.clone()).await;
        let count = registry.scan().await.unwrap();

        assert_eq!(count, 0);

        // Cleanup
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
        let _ = tokio::fs::remove_dir_all(&data_dir).await;
    }

    #[tokio::test]
    async fn test_reap_terminated_no_processes() {
        let temp_dir = std::env::temp_dir().join("zen-garden-test-Companions-reap");
        let data_dir = std::env::temp_dir().join("zen-garden-test-data-reap");
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
        let _ = tokio::fs::remove_dir_all(&data_dir).await;
        let _ = tokio::fs::create_dir_all(&data_dir).await;

        let registry = CompanionRegistry::with_path(temp_dir.clone(), data_dir.clone()).await;

        // Should return 0 when no Companions registered
        let reaped = registry.reap_terminated().await;
        assert_eq!(reaped, 0);

        // Cleanup
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
        let _ = tokio::fs::remove_dir_all(&data_dir).await;
    }

    #[tokio::test]
    async fn test_reconcile_empty() {
        let temp_dir = std::env::temp_dir().join("zen-garden-test-Companions-reconcile");
        let data_dir = std::env::temp_dir().join("zen-garden-test-data-reconcile");
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
        let _ = tokio::fs::remove_dir_all(&data_dir).await;
        let _ = tokio::fs::create_dir_all(&data_dir).await;

        let registry = CompanionRegistry::with_path(temp_dir.clone(), data_dir.clone()).await;

        // Should return (0, 0) when no runtime state
        let (adopted, dead) = registry.reconcile_running_companions().await;
        assert_eq!(adopted, 0);
        assert_eq!(dead, 0);

        // Cleanup
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
        let _ = tokio::fs::remove_dir_all(&data_dir).await;
    }
}
