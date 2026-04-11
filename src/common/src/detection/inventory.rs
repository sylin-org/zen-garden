//! System process inventory with TCP port mapping.
//!
//! Captures a snapshot of all running processes and their listening TCP
//! ports using native APIs (sysinfo + netstat2). The snapshot is taken
//! once per scan cycle and shared across all service matchers.

use std::collections::HashMap;
use std::time::Instant;

/// Information about a running process.
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    /// OS process ID.
    pub pid: u32,
    /// Executable name (e.g., "python.exe", "ollama.exe").
    pub name: String,
    /// Full command line (executable + arguments).
    pub cmdline: String,
    /// Full path to the executable.
    pub exe_path: String,
    /// TCP ports this process is listening on.
    pub listening_ports: Vec<u16>,
    /// Parent process ID (for parent-child resolution).
    pub parent_pid: Option<u32>,
}

/// A cached snapshot of all running processes with port mappings.
#[derive(Debug)]
pub struct SystemSnapshot {
    /// All processes at capture time.
    pub processes: Vec<ProcessInfo>,
    /// When this snapshot was captured.
    pub captured_at: Instant,
}

impl SystemSnapshot {
    /// Capture a fresh system snapshot.
    ///
    /// Enumerates all processes via `sysinfo` (PIDs, names, exe paths,
    /// parent PIDs) enriched with command lines and TCP port mappings.
    ///
    /// On Windows: command lines come from WMI (`Win32_Process`) because
    /// `sysinfo` doesn't populate them. On Linux: `sysinfo` provides
    /// command lines via `/proc/{pid}/cmdline`.
    pub fn capture() -> Self {
        let mut system = sysinfo::System::new();
        system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

        // Build PID → listening ports map
        let port_map = build_port_map();

        // Build PID → command line map (platform-specific enrichment)
        let cmdline_map = build_cmdline_map();

        let processes = system
            .processes()
            .iter()
            .map(|(pid, proc)| {
                let pid_u32 = pid.as_u32();

                // Prefer platform-specific command line (WMI on Windows)
                // Fall back to sysinfo (works on Linux)
                let cmdline = cmdline_map
                    .get(&pid_u32)
                    .cloned()
                    .unwrap_or_else(|| {
                        proc.cmd()
                            .iter()
                            .map(|s| s.to_string_lossy())
                            .collect::<Vec<_>>()
                            .join(" ")
                    });

                ProcessInfo {
                    pid: pid_u32,
                    name: proc.name().to_string_lossy().into_owned(),
                    cmdline,
                    exe_path: proc
                        .exe()
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    listening_ports: port_map.get(&pid_u32).cloned().unwrap_or_default(),
                    parent_pid: proc.parent().map(|p| p.as_u32()),
                }
            })
            .collect();

        SystemSnapshot {
            processes,
            captured_at: Instant::now(),
        }
    }

    /// Find processes whose executable name contains the given substring
    /// (case-insensitive).
    pub fn find_by_executable(&self, name: &str) -> Vec<&ProcessInfo> {
        let lower = name.to_lowercase();
        self.processes
            .iter()
            .filter(|p| p.name.to_lowercase().contains(&lower))
            .collect()
    }

    /// Find processes whose command line contains the given substring
    /// (case-insensitive).
    pub fn find_by_cmdline(&self, pattern: &str) -> Vec<&ProcessInfo> {
        let lower = pattern.to_lowercase();
        self.processes
            .iter()
            .filter(|p| p.cmdline.to_lowercase().contains(&lower))
            .collect()
    }

    /// Get a process by PID.
    pub fn get(&self, pid: u32) -> Option<&ProcessInfo> {
        self.processes.iter().find(|p| p.pid == pid)
    }

    /// Find child processes of a given PID.
    pub fn children_of(&self, pid: u32) -> Vec<&ProcessInfo> {
        self.processes
            .iter()
            .filter(|p| p.parent_pid == Some(pid))
            .collect()
    }

    /// How old this snapshot is.
    pub fn age(&self) -> std::time::Duration {
        self.captured_at.elapsed()
    }

    /// Total number of processes captured.
    pub fn len(&self) -> usize {
        self.processes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.processes.is_empty()
    }
}

/// Build a map of PID → command line via platform-specific methods.
///
/// On Windows: uses PowerShell `Get-CimInstance Win32_Process` (WMI)
/// because `sysinfo` doesn't populate command lines on Windows.
/// On Linux: returns empty map (sysinfo handles cmdlines via /proc).
#[cfg(windows)]
fn build_cmdline_map() -> HashMap<u32, String> {
    use std::process::Command;

    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-CimInstance Win32_Process | Select-Object ProcessId, CommandLine | ForEach-Object { \"$($_.ProcessId)|$($_.CommandLine)\" }",
        ])
        .output();

    let mut map = HashMap::new();

    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            for line in stdout.lines() {
                let line = line.trim();
                if let Some((pid_str, cmdline)) = line.split_once('|')
                    && let Ok(pid) = pid_str.trim().parse::<u32>()
                {
                    let cmdline = cmdline.trim();
                    if !cmdline.is_empty() {
                        map.insert(pid, cmdline.to_string());
                    }
                }
            }
            tracing::debug!(entries = map.len(), "WMI command line map built");
        }
        Ok(out) => {
            tracing::warn!(
                status = ?out.status,
                "WMI command line query failed"
            );
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to run WMI command line query");
        }
    }

    map
}

#[cfg(not(windows))]
fn build_cmdline_map() -> HashMap<u32, String> {
    // On Linux, sysinfo reads /proc/{pid}/cmdline which works fine.
    HashMap::new()
}

/// Build a map of PID → listening TCP ports using netstat2.
fn build_port_map() -> HashMap<u32, Vec<u16>> {
    use netstat2::{get_sockets_info, AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo};

    let mut map: HashMap<u32, Vec<u16>> = HashMap::new();

    let af = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    let proto = ProtocolFlags::TCP;

    match get_sockets_info(af, proto) {
        Ok(sockets) => {
            for si in sockets {
                if let ProtocolSocketInfo::Tcp(tcp) = si.protocol_socket_info {
                    // Filter for LISTEN state
                    if tcp.state == netstat2::TcpState::Listen {
                        for pid in si.associated_pids {
                            map.entry(pid).or_default().push(tcp.local_port);
                        }
                    }
                }
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to enumerate TCP sockets for port mapping");
        }
    }

    // Deduplicate ports per PID (same port can appear for IPv4 and IPv6)
    for ports in map.values_mut() {
        ports.sort_unstable();
        ports.dedup();
    }

    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_returns_processes() {
        let snapshot = SystemSnapshot::capture();
        // At minimum, our own test process should be in the list
        assert!(!snapshot.is_empty());
    }

    #[test]
    fn find_by_executable_works() {
        let snapshot = SystemSnapshot::capture();
        // cargo test runs as a process — find it
        let results = snapshot.find_by_executable("cargo");
        // May or may not find cargo depending on how test runner works.
        // At least verify the function doesn't panic.
        let _ = results;
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;
    use crate::detection::*;

    #[tokio::test]
    async fn detect_live_services() {
        let pipeline = DetectionPipeline::new();
        pipeline.refresh().await;

        let services = vec![
            ("Ollama", ProcessSignature { executable: "ollama".into(), cmdline_contains: Some("serve".into()), ..Default::default() },
             Some(HealthCheck { path: "/".into(), expected_status: 200, response_contains: Some("Ollama".into()) }),
             PortConfig { default: 11434, range: None, remember: false }),
            ("Speech", ProcessSignature { executable: "python".into(), cmdline_contains: Some("speech.py".into()), ..Default::default() },
             Some(HealthCheck { path: "/health".into(), expected_status: 200, response_contains: Some("status".into()) }),
             PortConfig { default: 8000, range: Some((8000, 8010)), remember: true }),
            ("Infinity", ProcessSignature { executable: "python".into(), cmdline_contains: Some("start.py".into()), ..Default::default() },
             Some(HealthCheck { path: "/health".into(), expected_status: 200, response_contains: Some("unix".into()) }),
             PortConfig { default: 7997, range: Some((7990, 8000)), remember: true }),
            ("Whisper", ProcessSignature { executable: "whisper-server".into(), cmdline_contains: None, ..Default::default() },
             Some(HealthCheck { path: "/health".into(), expected_status: 200, response_contains: Some("status".into()) }),
             PortConfig { default: 8000, range: Some((8000, 8010)), remember: true }),
            ("ComfyUI", ProcessSignature { executable: "python".into(), cmdline_contains: Some("main.py".into()), ..Default::default() },
             Some(HealthCheck { path: "/system_stats".into(), expected_status: 200, response_contains: Some("system".into()) }),
             PortConfig { default: 8188, range: None, remember: false }),
        ];

        for (name, sig, health, ports) in &services {
            let result = pipeline.detect(sig, health.as_ref(), ports, None).await;
            println!("{name:10}: detected={:<5} port={:<10} pid={:<10} — {}",
                result.detected, format!("{:?}", result.port), format!("{:?}", result.pid), result.details);
        }
    }
}
