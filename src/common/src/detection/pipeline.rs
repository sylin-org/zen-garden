//! Detection pipeline — orchestrates process matching + health verification.
//!
//! The pipeline captures a system snapshot once per scan cycle, then
//! runs service matching + health probes against it for each offering.

use super::inventory::SystemSnapshot;
use super::matcher::{ProcessSignature, match_processes};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Health verification configuration.
#[derive(Debug, Clone)]
pub struct HealthCheck {
    /// HTTP path to probe (e.g., "/health").
    pub path: String,
    /// Expected HTTP status code.
    pub expected_status: u16,
    /// Response body must contain this string (optional).
    pub response_contains: Option<String>,
}

/// Port discovery configuration.
#[derive(Debug, Clone)]
pub struct PortConfig {
    /// Default port to try if TCP table lookup yields nothing.
    pub default: u16,
    /// Port range to scan as last resort.
    pub range: Option<(u16, u16)>,
    /// Persist discovered port across restarts.
    pub remember: bool,
}

/// Result of a detection attempt.
#[derive(Debug, Clone)]
pub struct PipelineResult {
    /// Whether the service was detected and verified.
    pub detected: bool,
    /// Discovered port (from TCP table, health probe, or remembered).
    pub port: Option<u16>,
    /// PID of the matched process.
    pub pid: Option<u32>,
    /// Human-readable details for logging.
    pub details: String,
}

/// The detection pipeline.
///
/// Captures a system snapshot periodically and provides service
/// detection against the cached snapshot.
pub struct DetectionPipeline {
    snapshot: Arc<RwLock<Option<SystemSnapshot>>>,
    http_client: reqwest::Client,
}

impl DetectionPipeline {
    pub fn new() -> Self {
        let http_client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(5))
            .no_proxy()
            .build()
            .expect("HTTP client for detection");

        Self {
            snapshot: Arc::new(RwLock::new(None)),
            http_client,
        }
    }

    /// Refresh the system snapshot.
    ///
    /// Call once per scan cycle. All subsequent `detect()` calls
    /// use the cached snapshot until the next refresh.
    pub async fn refresh(&self) {
        let snapshot = tokio::task::spawn_blocking(SystemSnapshot::capture)
            .await
            .unwrap_or_else(|_| SystemSnapshot {
                processes: vec![],
                captured_at: Instant::now(),
            });

        tracing::debug!(processes = snapshot.len(), "process snapshot refreshed");

        *self.snapshot.write().await = Some(snapshot);
    }

    /// Detect a service using its process signature + health check.
    ///
    /// Steps:
    /// 1. Match processes against signature (from cached snapshot)
    /// 2. For each match with a port: verify via health probe
    /// 3. If no match has a port: try remembered port, then default
    /// 4. Return first successful detection
    pub async fn detect(
        &self,
        signature: &ProcessSignature,
        health: Option<&HealthCheck>,
        ports: &PortConfig,
        remembered_port: Option<u16>,
    ) -> PipelineResult {
        let snapshot = self.snapshot.read().await;
        let snapshot = match snapshot.as_ref() {
            Some(s) => s,
            None => {
                return PipelineResult {
                    detected: false,
                    port: None,
                    pid: None,
                    details: "no system snapshot available".to_string(),
                };
            }
        };

        // Step 1: Match processes
        let matches = match_processes(signature, snapshot);

        if matches.is_empty() {
            return PipelineResult {
                detected: false,
                port: None,
                pid: None,
                details: format!(
                    "no process matching executable={} cmdline={:?}",
                    signature.effective_executable(),
                    signature.cmdline_contains
                ),
            };
        }

        // Step 2: For matches with a discovered port, verify health
        for m in &matches {
            if let Some(port) = m.port {
                if let Some(hc) = health {
                    if self.verify_health(port, hc).await {
                        return PipelineResult {
                            detected: true,
                            port: Some(port),
                            pid: Some(m.pid),
                            details: format!(
                                "process {} (PID {}) on port {} — health verified",
                                m.name, m.pid, port
                            ),
                        };
                    }
                } else {
                    // No health check required — port match is sufficient
                    return PipelineResult {
                        detected: true,
                        port: Some(port),
                        pid: Some(m.pid),
                        details: format!(
                            "process {} (PID {}) on port {} — no health check configured",
                            m.name, m.pid, port
                        ),
                    };
                }
            }
        }

        // Step 3: Process found but no port from TCP table.
        // Try remembered port, then default port.
        let first_match = &matches[0];

        let ports_to_try: Vec<u16> = {
            let mut candidates = Vec::new();
            if let Some(rp) = remembered_port {
                candidates.push(rp);
            }
            candidates.push(ports.default);
            if let Some((start, end)) = ports.range {
                for p in start..=end {
                    if !candidates.contains(&p) {
                        candidates.push(p);
                    }
                }
            }
            candidates
        };

        if let Some(hc) = health {
            for port in &ports_to_try {
                if self.verify_health(*port, hc).await {
                    return PipelineResult {
                        detected: true,
                        port: Some(*port),
                        pid: Some(first_match.pid),
                        details: format!(
                            "process {} (PID {}) — health verified on port {} (fallback)",
                            first_match.name, first_match.pid, port
                        ),
                    };
                }
            }
        }

        // Process exists but not reachable on any known port
        PipelineResult {
            detected: true, // process IS running
            port: None,     // but we don't know its port
            pid: Some(first_match.pid),
            details: format!(
                "process {} (PID {}) found but no reachable port (tried {:?})",
                first_match.name, first_match.pid, ports_to_try
            ),
        }
    }

    /// HTTP health probe on localhost.
    async fn verify_health(&self, port: u16, check: &HealthCheck) -> bool {
        let url = format!("http://localhost:{}{}", port, check.path);

        let resp = match self.http_client.get(&url).send().await {
            Ok(r) => r,
            Err(_) => return false,
        };

        if resp.status().as_u16() != check.expected_status {
            return false;
        }

        if let Some(ref expected) = check.response_contains {
            match resp.text().await {
                Ok(body) => body.contains(expected),
                Err(_) => false,
            }
        } else {
            true
        }
    }
}

impl Default for DetectionPipeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn detect_nonexistent_returns_not_detected() {
        let pipeline = DetectionPipeline::new();
        pipeline.refresh().await;

        let sig = ProcessSignature {
            executable: "nonexistent_process_xyz".to_string(),
            ..Default::default()
        };
        let ports = PortConfig {
            default: 9999,
            range: None,
            remember: false,
        };

        let result = pipeline.detect(&sig, None, &ports, None).await;
        assert!(!result.detected);
    }
}
