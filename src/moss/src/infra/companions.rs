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

/// Runtime file name for persisting running Companion PIDs (for restart recovery)
const RUNTIME_FILE: &str = "Companion-runtime.json";

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

/// Runtime ledger - tracks currently running Companion PIDs
/// Persisted to disk for restart recovery
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RuntimeLedger {
    /// Map of companion_id -> (pid, port)
    running: HashMap<String, (u32, u16)>,
}

impl RuntimeLedger {
    /// Load from disk or create new
    async fn load(data_path: &Path) -> Self {
        let runtime_path = data_path.join(RUNTIME_FILE);
        if runtime_path.exists() {
            match tokio::fs::read_to_string(&runtime_path).await {
                Ok(content) => match serde_json::from_str(&content) {
                    Ok(ledger) => return ledger,
                    Err(e) => debug!(error = %e, "Failed to parse runtime ledger, starting fresh"),
                },
                Err(e) => debug!(error = %e, "Failed to read runtime ledger, starting fresh"),
            }
        }
        Self::default()
    }

    /// Save to disk
    async fn save(&self, data_path: &Path) -> Result<()> {
        let runtime_path = data_path.join(RUNTIME_FILE);
        let content = serde_json::to_string_pretty(self)?;
        tokio::fs::write(&runtime_path, content).await?;
        Ok(())
    }

    /// Record an Companion as running
    fn set_running(&mut self, companion_id: &str, pid: u32, port: u16) {
        self.running.insert(companion_id.to_string(), (pid, port));
    }

    /// Record an Companion as stopped
    fn set_stopped(&mut self, companion_id: &str) {
        self.running.remove(companion_id);
    }

    /// Get running Companion info
    #[allow(dead_code)]
    fn get(&self, companion_id: &str) -> Option<(u32, u16)> {
        self.running.get(companion_id).copied()
    }

    /// Get all running Companions
    fn all_running(&self) -> impl Iterator<Item = (&String, &(u32, u16))> {
        self.running.iter()
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
    #[allow(dead_code)]
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

    /// Running process handle (if started)
    process: Option<Child>,

    /// Process ID (cached for quick checks)
    pid: Option<u32>,

    /// Assigned command server port (when running)
    assigned_port: Option<u16>,
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
        }
    }
}

impl RegisteredCompanion {
    /// Check if the Companion process is running
    pub fn is_running(&self) -> bool {
        if let Some(pid) = self.pid {
            is_process_alive(pid)
        } else {
            false
        }
    }

    /// Get the process ID if running
    pub fn pid(&self) -> Option<u32> {
        if self.is_running() {
            self.pid
        } else {
            None
        }
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

    /// Runtime ledger - tracks running PIDs for restart recovery
    runtime_ledger: Arc<RwLock<RuntimeLedger>>,
}

impl CompanionRegistry {
    /// Create a new Companion registry
    /// Loads port ledger, state ledger, and runtime ledger from disk
    pub async fn new() -> Self {
        let data_path = PathBuf::from(data_dir());
        let port_ledger = PortLedger::load(&data_path).await;
        let state_ledger = CompanionStateLedger::load(&data_path).await;
        let runtime_ledger = RuntimeLedger::load(&data_path).await;

        Self {
            companions: Arc::new(RwLock::new(HashMap::new())),
            companions_path: PathBuf::from(companions_dir()),
            data_path,
            port_ledger: Arc::new(RwLock::new(port_ledger)),
            state_ledger: Arc::new(RwLock::new(state_ledger)),
            runtime_ledger: Arc::new(RwLock::new(runtime_ledger)),
        }
    }

    /// Create with custom Companions directory (for testing)
    pub async fn with_path(companions_path: PathBuf, data_path: PathBuf) -> Self {
        let port_ledger = PortLedger::load(&data_path).await;
        let state_ledger = CompanionStateLedger::load(&data_path).await;
        let runtime_ledger = RuntimeLedger::load(&data_path).await;

        Self {
            companions: Arc::new(RwLock::new(HashMap::new())),
            companions_path,
            data_path,
            port_ledger: Arc::new(RwLock::new(port_ledger)),
            state_ledger: Arc::new(RwLock::new(state_ledger)),
            runtime_ledger: Arc::new(RwLock::new(runtime_ledger)),
        }
    }

    /// Scan Companions directory and register all found Companions
    pub async fn scan(&self) -> Result<usize> {
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
        };

        let mut companions = self.companions.write().await;
        companions.insert(companion_id.to_string(), companion);

        info!(companion = %companion_id, port = port, "Registered Companion");
        Ok(())
    }

    /// Get all registered Companions
    pub async fn list(&self) -> Vec<RegisteredCompanion> {
        let companions = self.companions.read().await;
        companions.values().cloned().collect()
    }

    /// Get a specific Companion by ID
    pub async fn get(&self, id: &str) -> Option<RegisteredCompanion> {
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

        if c.is_running() {
            if let Some(pid) = c.pid {
                info!(companion = %id, pid = pid, "Companion already running");
                return Ok(pid);
            }
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

        // Persist to runtime ledger for restart recovery
        {
            let mut runtime_ledger = self.runtime_ledger.write().await;
            runtime_ledger.set_running(id, pid, port);
            if let Err(e) = runtime_ledger.save(&self.data_path).await {
                warn!(companion = %id, error = %e, "Failed to persist runtime ledger");
            }
        }

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
        let mut runtime_ledger = self.runtime_ledger.write().await;
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
                    // Process has exited - reap it
                    let pid = c.pid.unwrap_or(0);
                    if status.success() {
                        info!(companion = %id, pid = pid, "Companion exited normally");
                    } else {
                        warn!(companion = %id, pid = pid, status = ?status, "Companion exited with error");
                    }

                    // Clear process state
                    c.process = None;
                    c.pid = None;
                    runtime_ledger.set_stopped(id);
                    reaped += 1;
                }
                Ok(None) => {
                    // Process still running - nothing to do
                }
                Err(e) => {
                    // Error checking status - log but don't clear
                    warn!(companion = %id, error = %e, "Failed to check companion process status");
                }
            }
        }

        // Persist runtime ledger if anything changed
        if reaped > 0 {
            if let Err(e) = runtime_ledger.save(&self.data_path).await {
                warn!(error = %e, "Failed to persist runtime ledger after reaping");
            }
            debug!(reaped = reaped, "Reaped terminated Companion processes");
        }

        reaped
    }

    /// Reconcile running Companions after Moss restart
    ///
    /// On restart, Moss loses process handles but Companions may still be running
    /// (due to kill_on_drop(false)). This method:
    /// 1. Loads the runtime ledger (PIDs from before restart)
    /// 2. Checks which processes are still alive
    /// 3. Adopts still-running Companions (updates internal state, no process handle)
    /// 4. Cleans up entries for dead processes
    ///
    /// Returns (adopted_count, dead_count)
    pub async fn reconcile_running_companions(&self) -> (usize, usize) {
        let runtime_ledger = self.runtime_ledger.read().await;
        let mut to_adopt = Vec::new();
        let mut to_remove = Vec::new();

        // Check each entry in runtime ledger
        for (companion_id, (pid, port)) in runtime_ledger.all_running() {
            if is_process_alive(*pid) {
                to_adopt.push((companion_id.clone(), *pid, *port));
            } else {
                to_remove.push(companion_id.clone());
            }
        }
        drop(runtime_ledger);

        let adopted = to_adopt.len();
        let dead = to_remove.len();

        // Adopt still-running Companions
        if !to_adopt.is_empty() {
            let mut companions_guard = self.companions.write().await;
            for (companion_id, pid, port) in to_adopt {
                if let Some(c) = companions_guard.get_mut(&companion_id) {
                    // Companion is registered - update its state
                    // Note: we don't have a process handle (can't reattach to running process)
                    // but we can track the PID for is_running() checks
                    c.pid = Some(pid);
                    c.assigned_port = Some(port);
                    info!(
                        companion = %companion_id,
                        pid = pid,
                        port = port,
                        "Adopted running Companion from previous session"
                    );
                } else {
                    // Companion not registered (binary removed?) - kill orphan
                    warn!(
                        companion = %companion_id,
                        pid = pid,
                        "Found orphaned Companion process, killing"
                    );
                    kill_process_by_pid(pid);
                    to_remove.push(companion_id);
                }
            }
        }

        // Clean up dead entries from runtime ledger
        if !to_remove.is_empty() {
            let mut runtime_ledger = self.runtime_ledger.write().await;
            for companion_id in &to_remove {
                runtime_ledger.set_stopped(companion_id);
            }
            if let Err(e) = runtime_ledger.save(&self.data_path).await {
                warn!(error = %e, "Failed to persist runtime ledger after reconciliation");
            }
        }

        if adopted > 0 || dead > 0 {
            info!(
                adopted = adopted,
                dead = dead,
                "Reconciled Companion processes from previous session"
            );
        }

        (adopted, dead)
    }

    /// Internal stop implementation
    async fn stop_internal(&self, id: &str) -> Result<()> {
        let mut companions_guard = self.companions.write().await;
        let c = companions_guard
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("Companion not found: {}", id))?;

        if let Some(pid) = c.pid {
            if is_process_alive(pid) {
                info!(companion = %id, pid = pid, "Stopping Companion");

                // Try graceful shutdown first via process handle
                if let Some(ref mut child) = c.process {
                    if let Err(e) = child.kill().await {
                        warn!(companion = %id, error = %e, "Failed to kill Companion via handle, trying by PID");
                        kill_process_by_pid(pid);
                    }
                    // Reap the process to prevent zombie
                    let _ = child.wait().await;
                } else {
                    // No handle, kill by PID
                    kill_process_by_pid(pid);
                }

                info!(companion = %id, "Companion stopped");
            }
        }

        c.process = None;
        c.pid = None;

        // Remove from runtime ledger
        {
            let mut runtime_ledger = self.runtime_ledger.write().await;
            runtime_ledger.set_stopped(id);
            if let Err(e) = runtime_ledger.save(&self.data_path).await {
                warn!(companion = %id, error = %e, "Failed to persist runtime ledger");
            }
        }

        Ok(())
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
                    match reqwest::Client::new()
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

/// Kill a process by PID
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

/// Find the Companion executable in a directory
///
/// Scans for any executable file in the Companion folder.
/// On Windows: looks for .exe files
/// On Linux: looks for files with execute permission
async fn find_companion_executable(companion_dir: &Path) -> Option<PathBuf> {
    // Scan for any executable in the folder
    if let Ok(mut entries) = tokio::fs::read_dir(companion_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.is_file() {
                let is_executable = if cfg!(windows) {
                    path.extension().map(|e| e == "exe").unwrap_or(false)
                } else {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        if let Ok(meta) = path.metadata() {
                            meta.permissions().mode() & 0o111 != 0
                        } else {
                            false
                        }
                    }
                    #[cfg(not(unix))]
                    false
                };

                if is_executable {
                    debug!(path = %path.display(), "Found executable file");
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
