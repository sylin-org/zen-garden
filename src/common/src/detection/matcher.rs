//! Service matcher — matches process signatures against a system snapshot.
//!
//! The matcher is pure data matching. No I/O, no shell commands.
//! It reads from a `SystemSnapshot` and produces match candidates.

use super::inventory::{ProcessInfo, SystemSnapshot};

/// Declarative process signature from a manifest.
#[derive(Debug, Clone, Default)]
pub struct ProcessSignature {
    /// Executable name to match (case-insensitive substring).
    /// e.g., "python", "ollama", "whisper-server"
    pub executable: String,

    /// Platform-specific executable override.
    pub windows_executable: Option<String>,
    pub linux_executable: Option<String>,

    /// Command line must contain this substring (case-insensitive).
    /// e.g., "speech.py", "serve", "main.py"
    pub cmdline_contains: Option<String>,
}

impl ProcessSignature {
    /// Get the executable name for the current platform.
    pub fn effective_executable(&self) -> &str {
        #[cfg(windows)]
        if let Some(ref win) = self.windows_executable {
            return win;
        }
        #[cfg(target_os = "linux")]
        if let Some(ref lin) = self.linux_executable {
            return lin;
        }
        &self.executable
    }
}

/// A matched process with its discovered port.
#[derive(Debug, Clone)]
pub struct ProcessMatch {
    /// The matched process.
    pub pid: u32,
    pub name: String,
    pub cmdline: String,
    pub exe_path: String,
    /// Listening port discovered from TCP table (or from child process).
    pub port: Option<u16>,
    /// Whether the port came from a child process (parent-child resolution).
    pub port_from_child: bool,
    /// The child PID that holds the port (if parent-child resolution was used).
    pub port_holder_pid: Option<u32>,
}

/// Match a process signature against a system snapshot.
///
/// Returns all matching processes with their discovered ports.
/// Handles parent-child resolution: if a matched process has no
/// listening port, checks its children for a port.
pub fn match_processes(
    signature: &ProcessSignature,
    snapshot: &SystemSnapshot,
) -> Vec<ProcessMatch> {
    let exe_name = signature.effective_executable().to_lowercase();

    // Find all processes matching executable name
    let exe_matches: Vec<&ProcessInfo> = snapshot
        .processes
        .iter()
        .filter(|p| p.name.to_lowercase().contains(&exe_name))
        .collect();

    // Filter by cmdline if specified
    let cmdline_filter = signature
        .cmdline_contains
        .as_ref()
        .map(|s| s.to_lowercase());

    let candidates: Vec<&ProcessInfo> = if let Some(ref pattern) = cmdline_filter {
        exe_matches
            .into_iter()
            .filter(|p| p.cmdline.to_lowercase().contains(pattern.as_str()))
            .collect()
    } else {
        exe_matches
    };

    // Build matches with port resolution
    let mut matches = Vec::new();
    let mut seen_ports: std::collections::HashSet<u16> = std::collections::HashSet::new();

    for proc in &candidates {
        let port = if !proc.listening_ports.is_empty() {
            // This process itself is listening
            Some((proc.listening_ports[0], false, None))
        } else {
            // Check children for a listening port (parent-child resolution)
            resolve_child_port(proc.pid, snapshot, &cmdline_filter)
        };

        let (discovered_port, from_child, holder_pid) = match port {
            Some((p, fc, hp)) => (Some(p), fc, hp),
            None => (None, false, None),
        };

        // Avoid duplicate matches for the same port (parent + child both match)
        if let Some(p) = discovered_port {
            if seen_ports.contains(&p) {
                continue;
            }
            seen_ports.insert(p);
        }

        matches.push(ProcessMatch {
            pid: proc.pid,
            name: proc.name.clone(),
            cmdline: proc.cmdline.clone(),
            exe_path: proc.exe_path.clone(),
            port: discovered_port,
            port_from_child: from_child,
            port_holder_pid: holder_pid,
        });
    }

    matches
}

/// Check child processes for a listening port.
///
/// Python venv services spawn a child process that holds the port:
///   venv/python.exe (parent, no port) → system/python.exe (child, has port)
///
/// If the child's cmdline matches the pattern (or no pattern specified),
/// its port is used.
fn resolve_child_port(
    parent_pid: u32,
    snapshot: &SystemSnapshot,
    cmdline_filter: &Option<String>,
) -> Option<(u16, bool, Option<u32>)> {
    let children = snapshot.children_of(parent_pid);

    for child in children {
        // If cmdline filter is set, child must also match
        if let Some(pattern) = cmdline_filter
            && !child.cmdline.to_lowercase().contains(pattern.as_str())
        {
            continue;
        }

        if !child.listening_ports.is_empty() {
            return Some((child.listening_ports[0], true, Some(child.pid)));
        }

        // Recurse one level deeper (grandchild)
        if let Some(result) = resolve_child_port(child.pid, snapshot, cmdline_filter) {
            return Some(result);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_nonexistent_returns_empty() {
        let snapshot = SystemSnapshot::capture();
        let sig = ProcessSignature {
            executable: "nonexistent_process_xyz_12345".to_string(),
            ..Default::default()
        };
        let matches = match_processes(&sig, &snapshot);
        assert!(matches.is_empty());
    }
}
