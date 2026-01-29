//! Adapter Registry
//!
//! Discovers and manages external adapters (Cricket, Firefly, etc.)
//! that extend Moss with additional capabilities.
//!
//! Discovery Process:
//! 1. On boot (or refresh), scan `{data_dir}/adapters/` directory
//! 2. Each subfolder is an adapter: `adapters/{adapter-name}/adapter[.exe]`
//! 3. Spawn each adapter with `--dump-commands` flag
//! 4. Parse JSON output into CommandManifest
//! 5. Cache manifests for API queries
//!
//! Adapters communicate via their own protocols (SSE, HTTP, etc.)
//! Moss just stores their command manifests for help/discovery.

use anyhow::{Context, Result};
use garden_common::command_manifest::CommandManifest;
use garden_common::constants::paths::{adapters_dir, data_dir};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Timeout for adapter --dump-commands execution
const DUMP_COMMANDS_TIMEOUT: Duration = Duration::from_secs(5);

/// Port range for adapter command servers (assigned by Moss)
/// Base port: ASCII sum of "moss adapter" (1187) + 6000 = 7187
const ADAPTER_PORT_BASE: u16 = 7187;
const ADAPTER_PORT_MAX: u16 = 7199;

/// Ledger file name for persisting port assignments
const PORT_LEDGER_FILE: &str = "adapter-ports.json";

/// State file name for persisting adapter enabled/disabled state
const STATE_FILE: &str = "adapter-state.json";

/// Adapter enabled/disabled state ledger - persisted to disk
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct AdapterStateLedger {
    /// Map of adapter_id -> enabled (true = start on boot, false = disabled by user)
    /// Adapters not in this map default to enabled
    enabled: HashMap<String, bool>,
}

impl AdapterStateLedger {
    /// Load from disk or create new (all enabled by default)
    async fn load(data_path: &Path) -> Self {
        let state_path = data_path.join(STATE_FILE);
        if state_path.exists() {
            match tokio::fs::read_to_string(&state_path).await {
                Ok(content) => {
                    match serde_json::from_str(&content) {
                        Ok(state) => return state,
                        Err(e) => warn!(error = %e, "Failed to parse adapter state, using defaults"),
                    }
                }
                Err(e) => warn!(error = %e, "Failed to read adapter state, using defaults"),
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
    
    /// Check if adapter is enabled (defaults to true if not in map)
    fn is_enabled(&self, adapter_id: &str) -> bool {
        self.enabled.get(adapter_id).copied().unwrap_or(true)
    }
    
    /// Set adapter enabled state
    fn set_enabled(&mut self, adapter_id: &str, enabled: bool) {
        self.enabled.insert(adapter_id.to_string(), enabled);
    }
}

/// Port assignment ledger - persisted to disk
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PortLedger {
    /// Map of adapter_id -> assigned port
    assignments: HashMap<String, u16>,
    /// Next port to assign (starts at ADAPTER_PORT_BASE)
    next_port: u16,
}

impl PortLedger {
    /// Load from disk or create new
    async fn load(data_path: &Path) -> Self {
        let ledger_path = data_path.join(PORT_LEDGER_FILE);
        if ledger_path.exists() {
            match tokio::fs::read_to_string(&ledger_path).await {
                Ok(content) => {
                    match serde_json::from_str(&content) {
                        Ok(ledger) => return ledger,
                        Err(e) => warn!(error = %e, "Failed to parse port ledger, starting fresh"),
                    }
                }
                Err(e) => warn!(error = %e, "Failed to read port ledger, starting fresh"),
            }
        }
        Self {
            assignments: HashMap::new(),
            next_port: ADAPTER_PORT_BASE,
        }
    }
    
    /// Save to disk
    async fn save(&self, data_path: &Path) -> Result<()> {
        let ledger_path = data_path.join(PORT_LEDGER_FILE);
        let content = serde_json::to_string_pretty(self)?;
        tokio::fs::write(&ledger_path, content).await?;
        Ok(())
    }
    
    /// Get or assign a port for an adapter
    fn get_or_assign(&mut self, adapter_id: &str) -> Result<u16> {
        // Return existing assignment
        if let Some(&port) = self.assignments.get(adapter_id) {
            return Ok(port);
        }
        
        // Assign new port
        if self.next_port > ADAPTER_PORT_MAX {
            return Err(anyhow::anyhow!(
                "Port pool exhausted ({}-{}). Cannot register more adapters.",
                ADAPTER_PORT_BASE, ADAPTER_PORT_MAX
            ));
        }
        
        let port = self.next_port;
        self.next_port += 1;
        self.assignments.insert(adapter_id.to_string(), port);
        
        info!(adapter = %adapter_id, port = port, "Assigned port to adapter");
        Ok(port)
    }
    
    /// Get port for an adapter (if assigned)
    #[allow(dead_code)]
    fn get(&self, adapter_id: &str) -> Option<u16> {
        self.assignments.get(adapter_id).copied()
    }
}

/// Registered adapter with its manifest and metadata
#[derive(Debug)]
pub struct RegisteredAdapter {
    /// Adapter identifier (folder name)
    pub id: String,
    
    /// Path to the adapter executable
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

impl Clone for RegisteredAdapter {
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

impl RegisteredAdapter {
    /// Check if the adapter process is running
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

/// Adapter registry - discovers and caches adapter manifests
#[derive(Debug)]
pub struct AdapterRegistry {
    /// Registered adapters by ID
    adapters: Arc<RwLock<HashMap<String, RegisteredAdapter>>>,
    
    /// Path to adapters directory
    adapters_path: PathBuf,
    
    /// Path to data directory (for ledger persistence)
    data_path: PathBuf,
    
    /// Port assignment ledger
    port_ledger: Arc<RwLock<PortLedger>>,
    
    /// Adapter enabled/disabled state ledger
    state_ledger: Arc<RwLock<AdapterStateLedger>>,
}

impl AdapterRegistry {
    /// Create a new adapter registry
    /// Loads port ledger and state ledger from disk
    pub async fn new() -> Self {
        let data_path = PathBuf::from(data_dir());
        let port_ledger = PortLedger::load(&data_path).await;
        let state_ledger = AdapterStateLedger::load(&data_path).await;
        
        Self {
            adapters: Arc::new(RwLock::new(HashMap::new())),
            adapters_path: PathBuf::from(adapters_dir()),
            data_path,
            port_ledger: Arc::new(RwLock::new(port_ledger)),
            state_ledger: Arc::new(RwLock::new(state_ledger)),
        }
    }
    
    /// Create with custom adapters directory (for testing)
    pub async fn with_path(adapters_path: PathBuf, data_path: PathBuf) -> Self {
        let port_ledger = PortLedger::load(&data_path).await;
        let state_ledger = AdapterStateLedger::load(&data_path).await;
        
        Self {
            adapters: Arc::new(RwLock::new(HashMap::new())),
            adapters_path,
            data_path,
            port_ledger: Arc::new(RwLock::new(port_ledger)),
            state_ledger: Arc::new(RwLock::new(state_ledger)),
        }
    }
    
    /// Scan adapters directory and register all found adapters
    pub async fn scan(&self) -> Result<usize> {
        let adapters_path = &self.adapters_path;
        
        // Ensure directory exists
        if !adapters_path.exists() {
            tokio::fs::create_dir_all(adapters_path)
                .await
                .context("Failed to create adapters directory")?;
            return Ok(0);
        }
        
        let mut found = Vec::new();
        let mut entries = tokio::fs::read_dir(adapters_path).await?;
        
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            
            let adapter_id = match path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };
            
            // Look for executable
            if let Some(executable) = find_adapter_executable(&path).await {
                found.push((adapter_id, executable));
            }
        }
        
        // Register each adapter
        let count = found.len();
        for (adapter_id, executable) in found {
            match self.register_adapter(&adapter_id, &executable).await {
                Ok(()) => info!(adapter = %adapter_id, "Registered adapter"),
                Err(e) => warn!(adapter = %adapter_id, error = %e, "Failed to register adapter"),
            }
        }
        
        info!(count = count, "Adapter scan complete");
        Ok(count)
    }
    
    /// Scan adapters directory, register, and auto-start enabled adapters
    /// 
    /// This is the main entry point for adapter initialization at boot.
    /// Adapters are started unless explicitly disabled by the user.
    pub async fn scan_and_autostart(&self, moss_endpoint: &str) -> Result<(usize, usize)> {
        // First scan and register all adapters
        let registered = self.scan().await?;
        
        if registered == 0 {
            return Ok((0, 0));
        }
        
        // Get list of adapter IDs to start
        let adapter_ids: Vec<String> = {
            let adapters = self.adapters.read().await;
            adapters.keys().cloned().collect()
        };
        
        // Check state ledger and start enabled adapters
        let state_ledger = self.state_ledger.read().await;
        let mut started = 0;
        
        for adapter_id in adapter_ids {
            if state_ledger.is_enabled(&adapter_id) {
                match self.start(&adapter_id, moss_endpoint).await {
                    Ok(pid) => {
                        info!(adapter = %adapter_id, pid = pid, "Auto-started adapter");
                        started += 1;
                    }
                    Err(e) => {
                        warn!(adapter = %adapter_id, error = %e, "Failed to auto-start adapter");
                    }
                }
            } else {
                info!(adapter = %adapter_id, "Adapter disabled, skipping auto-start");
            }
        }
        
        info!(registered = registered, started = started, "Adapter scan and auto-start complete");
        Ok((registered, started))
    }
    
    /// Register a single adapter by running --dump-commands
    /// Gets or assigns a port from the ledger and passes it to the adapter
    async fn register_adapter(&self, adapter_id: &str, executable: &Path) -> Result<()> {
        // Get or assign port from ledger
        let port = {
            let mut ledger = self.port_ledger.write().await;
            let port = ledger.get_or_assign(adapter_id)?;
            // Persist ledger after assignment
            if let Err(e) = ledger.save(&self.data_path).await {
                warn!(error = %e, "Failed to persist port ledger");
            }
            port
        };
        
        // Call --dump-commands with --port argument
        let manifest = invoke_dump_commands(executable, port).await
            .with_context(|| format!("Failed to get manifest from adapter {}", adapter_id))?;
        
        let adapter = RegisteredAdapter {
            id: adapter_id.to_string(),
            executable: executable.to_path_buf(),
            manifest,
            process: None,
            pid: None,
            assigned_port: Some(port),
        };
        
        let mut adapters = self.adapters.write().await;
        adapters.insert(adapter_id.to_string(), adapter);
        
        info!(adapter = %adapter_id, port = port, "Registered adapter");
        Ok(())
    }
    
    /// Get all registered adapters
    pub async fn list(&self) -> Vec<RegisteredAdapter> {
        let adapters = self.adapters.read().await;
        adapters.values().cloned().collect()
    }
    
    /// Get a specific adapter by ID
    pub async fn get(&self, id: &str) -> Option<RegisteredAdapter> {
        let adapters = self.adapters.read().await;
        adapters.get(id).cloned()
    }
    
    /// Get adapter manifest by ID
    pub async fn get_manifest(&self, id: &str) -> Option<CommandManifest> {
        let adapters = self.adapters.read().await;
        adapters.get(id).map(|a| a.manifest.clone())
    }
    
    /// Refresh a specific adapter's manifest
    pub async fn refresh(&self, id: &str) -> Result<()> {
        let executable = {
            let adapters = self.adapters.read().await;
            match adapters.get(id) {
                Some(a) => a.executable.clone(),
                None => return Err(anyhow::anyhow!("Adapter not found: {}", id)),
            }
        };
        
        self.register_adapter(id, &executable).await
    }
    
    /// Refresh all adapters (rescan directory)
    pub async fn refresh_all(&self) -> Result<usize> {
        // Clear existing
        {
            let mut adapters = self.adapters.write().await;
            adapters.clear();
        }
        
        // Rescan
        self.scan().await
    }
    
    /// Get adapter count
    pub async fn count(&self) -> usize {
        let adapters = self.adapters.read().await;
        adapters.len()
    }
    
    /// Check if an adapter is running
    pub async fn is_running(&self, id: &str) -> bool {
        let adapters = self.adapters.read().await;
        adapters.get(id).map(|a| a.is_running()).unwrap_or(false)
    }
    
    /// Get assigned port for a running adapter
    pub async fn get_port(&self, id: &str) -> Option<u16> {
        let adapters = self.adapters.read().await;
        adapters.get(id).and_then(|a| a.port())
    }
    
    /// Start an adapter process
    /// 
    /// Spawns the adapter executable as a background process.
    /// Uses the pre-assigned port from the ledger.
    pub async fn start(&self, id: &str, moss_endpoint: &str) -> Result<u32> {
        let mut adapters = self.adapters.write().await;
        
        // Get adapter and check state
        let adapter = adapters.get(id)
            .ok_or_else(|| anyhow::anyhow!("Adapter not found: {}", id))?;
        
        if adapter.is_running() {
            if let Some(pid) = adapter.pid {
                info!(adapter = %id, pid = pid, "Adapter already running");
                return Ok(pid);
            }
        }
        
        // Port was assigned during registration
        let port = adapter.assigned_port
            .ok_or_else(|| anyhow::anyhow!("Adapter '{}' has no assigned port", id))?;
        
        let executable = adapter.executable.clone();
        
        // Now get mutable reference
        let adapter = adapters.get_mut(id).unwrap(); // Safe: we checked above
        
        // Spawn the adapter process with --stone and --port arguments
        info!(
            adapter = %id, 
            executable = %executable.display(), 
            endpoint = %moss_endpoint,
            port = port,
            "Starting adapter"
        );
        
        let child = Command::new(&executable)
            .arg("--stone")
            .arg(moss_endpoint)
            .arg("--port")
            .arg(port.to_string())
            .kill_on_drop(false) // Keep running if Moss restarts
            .spawn()
            .with_context(|| format!("Failed to start adapter {}", id))?;
        
        let pid = child.id().unwrap_or(0);
        adapter.process = Some(child);
        adapter.pid = Some(pid);
        adapter.assigned_port = Some(port);
        
        info!(adapter = %id, pid = pid, port = port, "Adapter started");
        Ok(pid)
    }
    
    /// Stop an adapter process (does NOT disable it - will restart on next boot)
    pub async fn stop(&self, id: &str) -> Result<()> {
        self.stop_internal(id).await
    }
    
    /// Stop an adapter and disable it (will NOT restart on next boot)
    /// 
    /// Use this when user explicitly wants to turn off an adapter.
    pub async fn stop_and_disable(&self, id: &str) -> Result<()> {
        // First stop the process
        self.stop_internal(id).await?;
        
        // Then persist disabled state
        {
            let mut state_ledger = self.state_ledger.write().await;
            state_ledger.set_enabled(id, false);
            if let Err(e) = state_ledger.save(&self.data_path).await {
                warn!(adapter = %id, error = %e, "Failed to persist adapter disabled state");
            }
        }
        
        info!(adapter = %id, "Adapter stopped and disabled (will not auto-start)");
        Ok(())
    }
    
    /// Enable an adapter (will auto-start on next boot or can be started manually)
    pub async fn enable(&self, id: &str) -> Result<()> {
        // Verify adapter exists
        {
            let adapters = self.adapters.read().await;
            if !adapters.contains_key(id) {
                return Err(anyhow::anyhow!("Adapter not found: {}", id));
            }
        }
        
        // Persist enabled state
        {
            let mut state_ledger = self.state_ledger.write().await;
            state_ledger.set_enabled(id, true);
            if let Err(e) = state_ledger.save(&self.data_path).await {
                warn!(adapter = %id, error = %e, "Failed to persist adapter enabled state");
            }
        }
        
        info!(adapter = %id, "Adapter enabled (will auto-start on next boot)");
        Ok(())
    }
    
    /// Check if an adapter is enabled (will auto-start on boot)
    pub async fn is_enabled(&self, id: &str) -> bool {
        let state_ledger = self.state_ledger.read().await;
        state_ledger.is_enabled(id)
    }
    
    /// Internal stop implementation
    async fn stop_internal(&self, id: &str) -> Result<()> {
        let mut adapters = self.adapters.write().await;
        let adapter = adapters.get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("Adapter not found: {}", id))?;
        
        if let Some(pid) = adapter.pid {
            if is_process_alive(pid) {
                info!(adapter = %id, pid = pid, "Stopping adapter");
                
                // Try graceful shutdown first via process handle
                if let Some(ref mut child) = adapter.process {
                    if let Err(e) = child.kill().await {
                        warn!(adapter = %id, error = %e, "Failed to kill adapter via handle, trying by PID");
                        kill_process_by_pid(pid);
                    }
                } else {
                    // No handle, kill by PID
                    kill_process_by_pid(pid);
                }
                
                info!(adapter = %id, "Adapter stopped");
            }
        }
        
        adapter.process = None;
        adapter.pid = None;
        adapter.assigned_port = None;
        
        Ok(())
    }
    
    /// Stop all running adapters
    /// 
    /// Used during package deployment to ensure clean upgrade.
    /// Attempts graceful HTTP shutdown first, then force kills.
    pub async fn stop_all(&self) -> Vec<(String, Result<()>)> {
        let adapter_ids: Vec<String> = {
            let adapters = self.adapters.read().await;
            adapters.keys().cloned().collect()
        };
        
        let mut results = Vec::new();
        
        for id in adapter_ids {
            if self.is_running(&id).await {
                info!(adapter = %id, "Stopping adapter for upgrade");
                
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
                            info!(adapter = %id, "Graceful shutdown via HTTP");
                            // Give it a moment to clean up
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        }
                        Ok(_) | Err(_) => {
                            debug!(adapter = %id, "HTTP shutdown failed, will force stop");
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

/// Find the adapter executable in a directory
/// 
/// Scans for any executable file in the adapter folder.
/// On Windows: looks for .exe files
/// On Linux: looks for files with execute permission
async fn find_adapter_executable(adapter_dir: &Path) -> Option<PathBuf> {
    // Scan for any executable in the folder
    if let Ok(mut entries) = tokio::fs::read_dir(adapter_dir).await {
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

/// Invoke an adapter with --dump-commands and --port, parse the output
async fn invoke_dump_commands(executable: &Path, port: u16) -> Result<CommandManifest> {
    debug!(executable = %executable.display(), port = port, "Invoking --dump-commands");
    
    let output = tokio::time::timeout(
        DUMP_COMMANDS_TIMEOUT,
        Command::new(executable)
            .arg("--dump-commands")
            .arg("--port")
            .arg(port.to_string())
            .output()
    )
    .await
    .context("Adapter timed out")?
    .context("Failed to execute adapter")?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!(
            "Adapter exited with {}: {}",
            output.status,
            stderr.trim()
        ));
    }
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let manifest: CommandManifest = serde_json::from_str(&stdout)
        .context("Failed to parse adapter manifest JSON")?;
    
    debug!(
        adapter_id = %manifest.id,
        commands = manifest.commands.len(),
        port = port,
        "Parsed adapter manifest"
    );
    
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_registry_new() {
        let registry = AdapterRegistry::new().await;
        assert_eq!(registry.count().await, 0);
    }
    
    #[tokio::test]
    async fn test_registry_scan_empty_dir() {
        let temp_dir = std::env::temp_dir().join("zen-garden-test-adapters");
        let data_dir = std::env::temp_dir().join("zen-garden-test-data");
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
        let _ = tokio::fs::remove_dir_all(&data_dir).await;
        
        let registry = AdapterRegistry::with_path(temp_dir.clone(), data_dir.clone()).await;
        let count = registry.scan().await.unwrap();
        
        assert_eq!(count, 0);
        
        // Cleanup
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
        let _ = tokio::fs::remove_dir_all(&data_dir).await;
    }
}
