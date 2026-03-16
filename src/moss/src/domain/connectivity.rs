//! Connectivity enforcement for adopted offerings
//!
//! Uses manifest-defined checks and ensure commands to make adopted services
//! reachable on the LAN (binding, firewall, etc.).

use anyhow::{Context, Result};
use dashmap::DashMap;
use garden_common::infra::network::get_local_ip_and_mac;
use garden_common::manifests::{
    CommandDetection, DetectionConfig, DetectionMethod, DetectionRule, HttpProbeDetection,
    Offering as OfferingManifest,
};
use garden_common::templates::{render_template, Template};
use garden_common::OfferingLocation;
use regex::Regex;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::domain::traits::ServiceDetector;
use garden_common::detection::{detect_by_http_probe, DetectionResult};

/// Connectivity orchestration with caching and enforcement cooldowns
pub struct ConnectivityOrchestrator {
    detector: Arc<dyn ServiceDetector>,
    cache: Arc<DashMap<String, CachedCheck>>,
    last_enforced: Arc<DashMap<String, Instant>>,
    attempts: Arc<DashMap<String, EnforceState>>,
}

#[derive(Debug, Clone)]
struct CachedCheck {
    ok: bool,
    cached_at: Instant,
    ttl: Duration,
    details: String,
}

#[derive(Debug, Clone)]
struct EnforceState {
    attempts: u32,
}

#[derive(Debug, Clone)]
pub struct ConnectivityOutcome {
    pub status: ConnectivityStatus,
    pub details: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectivityStatus {
    Skipped,
    Satisfied,
    Enforced,
    Failed,
}

impl ConnectivityOutcome {
    pub fn is_ok(&self) -> bool {
        matches!(
            self.status,
            ConnectivityStatus::Skipped
                | ConnectivityStatus::Satisfied
                | ConnectivityStatus::Enforced
        )
    }

    fn skipped(details: impl Into<String>) -> Self {
        Self {
            status: ConnectivityStatus::Skipped,
            details: details.into(),
        }
    }

    fn satisfied(details: impl Into<String>) -> Self {
        Self {
            status: ConnectivityStatus::Satisfied,
            details: details.into(),
        }
    }

    fn enforced(details: impl Into<String>) -> Self {
        Self {
            status: ConnectivityStatus::Enforced,
            details: details.into(),
        }
    }

    fn failed(details: impl Into<String>) -> Self {
        Self {
            status: ConnectivityStatus::Failed,
            details: details.into(),
        }
    }
}

impl ConnectivityOrchestrator {
    pub fn new(detector: Arc<dyn ServiceDetector>) -> Self {
        Self {
            detector,
            cache: Arc::new(DashMap::new()),
            last_enforced: Arc::new(DashMap::new()),
            attempts: Arc::new(DashMap::new()),
        }
    }

    /// Ensure connectivity for an adopted offering using manifest rules.
    pub async fn ensure_connectivity(
        &self,
        manifest: &OfferingManifest,
        location: Option<&OfferingLocation>,
        stone_name: &str,
    ) -> Result<ConnectivityOutcome> {
        let Some(config) = manifest.get_connectivity_config() else {
            return Ok(ConnectivityOutcome::skipped("No connectivity config"));
        };

        let Some(rules) = config.get_current_os_rules() else {
            return Ok(ConnectivityOutcome::skipped(
                "No connectivity rules for current OS",
            ));
        };

        if rules.checks.is_empty() {
            return Ok(ConnectivityOutcome::skipped(
                "No connectivity checks configured",
            ));
        }

        let context = ConnectivityContext::from_manifest(manifest, location, stone_name);
        let check_result = self
            .run_checks(manifest.name.as_str(), rules, &context)
            .await?;

        if check_result.ok {
            self.reset_attempts(manifest.name.as_str());
            return Ok(ConnectivityOutcome::satisfied(check_result.details));
        }

        if !config.enforce {
            return Ok(ConnectivityOutcome::failed(check_result.details));
        }

        let max_attempts = config.max_attempts.unwrap_or(5);
        let max_attempts = if max_attempts == 0 {
            u32::MAX
        } else {
            max_attempts
        };
        if self.max_attempts_reached(manifest.name.as_str(), max_attempts) {
            return Ok(ConnectivityOutcome::failed(format!(
                "{} (max enforcement attempts reached)",
                check_result.details
            )));
        }

        if !self.should_enforce(manifest.name.as_str(), config.enforce_cooldown_secs()) {
            return Ok(ConnectivityOutcome::failed(format!(
                "{} (enforcement cooldown active)",
                check_result.details
            )));
        }

        if rules.ensure.is_empty() {
            return Ok(ConnectivityOutcome::failed(format!(
                "{} (no ensure commands configured)",
                check_result.details
            )));
        }

        self.record_attempt(manifest.name.as_str());
        self.run_ensure_commands(manifest.name.as_str(), rules, &context)
            .await?;

        // Re-check after enforcement, bypassing cache
        self.invalidate_cache(manifest.name.as_str(), context.os.as_str());
        let recheck = self
            .run_checks(manifest.name.as_str(), rules, &context)
            .await?;

        if recheck.ok {
            self.reset_attempts(manifest.name.as_str());
            Ok(ConnectivityOutcome::enforced(recheck.details))
        } else {
            Ok(ConnectivityOutcome::failed(recheck.details))
        }
    }

    fn should_enforce(&self, offering: &str, cooldown_secs: u64) -> bool {
        let now = Instant::now();
        if let Some(mut entry) = self.last_enforced.get_mut(offering) {
            if now.duration_since(*entry) >= Duration::from_secs(cooldown_secs) {
                *entry = now;
                true
            } else {
                false
            }
        } else {
            self.last_enforced.insert(offering.to_string(), now);
            true
        }
    }

    fn invalidate_cache(&self, offering: &str, os: &str) {
        let prefix = format!("{}:{}:", offering, os);
        self.cache.retain(|k, _| !k.starts_with(&prefix));
    }

    fn max_attempts_reached(&self, offering: &str, max_attempts: u32) -> bool {
        if max_attempts == u32::MAX {
            return false;
        }
        self.attempts
            .get(offering)
            .map(|state| state.attempts >= max_attempts)
            .unwrap_or(false)
    }

    fn record_attempt(&self, offering: &str) {
        self.attempts
            .entry(offering.to_string())
            .and_modify(|state| state.attempts = state.attempts.saturating_add(1))
            .or_insert(EnforceState { attempts: 1 });
    }

    fn reset_attempts(&self, offering: &str) {
        self.attempts.remove(offering);
        self.last_enforced.remove(offering);
    }

    async fn run_checks(
        &self,
        offering: &str,
        rules: &garden_common::manifests::ConnectivityRules,
        context: &ConnectivityContext,
    ) -> Result<CheckResult> {
        for (idx, rule) in rules.checks.iter().enumerate() {
            let cache_key = format!("{}:{}:{}", offering, context.os, idx);
            if let Some(cached) = self.cache.get(&cache_key) {
                if cached.cached_at.elapsed() < cached.ttl {
                    if !cached.ok {
                        return Ok(CheckResult::failed(cached.details.clone()));
                    }
                    continue;
                }
            }

            let result = self.execute_check(rule, context).await?;

            let ttl = Duration::from_secs(rule.cache_ttl_secs.unwrap_or(300));
            if ttl.as_secs() > 0 {
                self.cache.insert(
                    cache_key,
                    CachedCheck {
                        ok: result.detected,
                        cached_at: Instant::now(),
                        ttl,
                        details: result.details.clone(),
                    },
                );
            }

            if !result.detected {
                return Ok(CheckResult::failed(result.details));
            }
        }

        Ok(CheckResult::ok("All connectivity checks passed"))
    }

    async fn execute_check(
        &self,
        rule: &DetectionRule,
        context: &ConnectivityContext,
    ) -> Result<DetectionResult> {
        match rule.method {
            DetectionMethod::Command => {
                let DetectionConfig::Command(ref config) = rule.config else {
                    return Ok(DetectionResult {
                        detected: false,
                        version: None,
                        details: "Invalid command detection config".into(),
                    });
                };
                let templated = context.template_command(&config.command);
                let command_config = CommandDetection {
                    command: templated,
                    expected_pattern: config.expected_pattern.clone(),
                    expected_exit_code: config.expected_exit_code,
                };
                let timeout = Duration::from_secs(5);
                run_shell_command_check(&command_config, timeout).await
            }
            DetectionMethod::ContainerInspect => {
                let DetectionConfig::ContainerInspect(ref config) = rule.config else {
                    return Ok(DetectionResult {
                        detected: false,
                        version: None,
                        details: "Invalid container_inspect config".into(),
                    });
                };
                match self.detector.detect_by_container_inspect(config).await {
                    Ok(result) => Ok(result),
                    Err(e) => Ok(DetectionResult {
                        detected: false,
                        version: None,
                        details: format!("Container inspect failed: {}", e),
                    }),
                }
            }
            DetectionMethod::HttpProbe => {
                let DetectionConfig::HttpProbe(ref config) = rule.config else {
                    return Ok(DetectionResult {
                        detected: false,
                        version: None,
                        details: "Invalid http_probe config".into(),
                    });
                };
                let templated = context.template_command(&config.url);
                let probe = HttpProbeDetection {
                    url: templated,
                    expected_status: config.expected_status,
                    timeout_ms: config.timeout_ms,
                };
                match detect_by_http_probe(&probe).await {
                    Ok(result) => Ok(result),
                    Err(e) => Ok(DetectionResult {
                        detected: false,
                        version: None,
                        details: format!("HTTP probe failed: {}", e),
                    }),
                }
            }
        }
    }

    async fn run_ensure_commands(
        &self,
        offering: &str,
        rules: &garden_common::manifests::ConnectivityRules,
        context: &ConnectivityContext,
    ) -> Result<()> {
        for action in &rules.ensure {
            let templated = context.template_command(&action.command);
            let timeout = Duration::from_secs(action.timeout_secs.unwrap_or(30));
            let continue_on_error = action.continue_on_error.unwrap_or(false);

            tracing::info!(
                offering = %offering,
                command = %templated,
                "Executing connectivity ensure command"
            );

            let result = execute_shell_command(&templated, timeout).await;
            if let Err(e) = result {
                if continue_on_error {
                    tracing::warn!(
                        offering = %offering,
                        error = %e,
                        "Connectivity ensure command failed (continuing)"
                    );
                } else {
                    return Err(e);
                }
            }
        }

        Ok(())
    }
}

struct CheckResult {
    ok: bool,
    details: String,
}

impl CheckResult {
    fn ok(details: impl Into<String>) -> Self {
        Self {
            ok: true,
            details: details.into(),
        }
    }

    fn failed(details: impl Into<String>) -> Self {
        Self {
            ok: false,
            details: details.into(),
        }
    }
}

#[derive(Debug, Clone)]
struct ConnectivityContext {
    offering: String,
    host: String,
    port: u16,
    protocol: String,
    stone: String,
    lan_ip: String,
    os: String,
}

impl ConnectivityContext {
    fn from_manifest(
        manifest: &OfferingManifest,
        location: Option<&OfferingLocation>,
        stone_name: &str,
    ) -> Self {
        let host = location
            .map(|loc| loc.host.clone())
            .unwrap_or_else(|| "localhost".to_string());
        let port = location
            .map(|loc| loc.port)
            .unwrap_or_else(|| manifest.default_host_port());
        let protocol = location.map(|loc| loc.protocol.clone()).unwrap_or_else(|| {
            crate::domain::connection::infer_protocol_from_manifest_metadata(
                &manifest.name,
                &manifest.category,
                manifest.connection.as_ref(),
            )
        });

        let (lan_ip, _) = get_local_ip_and_mac();

        Self {
            offering: manifest.name.clone(),
            host,
            port,
            protocol,
            stone: stone_name.to_string(),
            lan_ip,
            os: std::env::consts::OS.to_string(),
        }
    }

    fn template_command(&self, template: &str) -> String {
        let mut ctx = Template::new();
        ctx.set("offering", &self.offering);
        ctx.set("host", &self.host);
        ctx.set("port", self.port.to_string());
        ctx.set("protocol", &self.protocol);
        ctx.set("stone", &self.stone);
        ctx.set("lan_ip", &self.lan_ip);
        ctx.set("os", &self.os);
        render_template(template, &ctx)
    }
}

async fn run_shell_command_check(
    config: &CommandDetection,
    timeout: Duration,
) -> Result<DetectionResult> {
    let output = execute_shell_command(&config.command, timeout).await;
    let output = match output {
        Ok(out) => out,
        Err(e) => {
            return Ok(DetectionResult {
                detected: false,
                version: None,
                details: format!("Command failed: {}", e),
            });
        }
    };

    let expected_code = config.expected_exit_code.unwrap_or(0);
    let actual_code = output.status.code().unwrap_or(-1);
    if actual_code != expected_code {
        return Ok(DetectionResult {
            detected: false,
            version: None,
            details: format!(
                "Exit code mismatch: expected {}, got {}",
                expected_code, actual_code
            ),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    if let Some(pattern_str) = &config.expected_pattern {
        let pattern = Regex::new(pattern_str).context("Invalid regex pattern")?;
        if !pattern.is_match(&combined) {
            return Ok(DetectionResult {
                detected: false,
                version: None,
                details: format!("Output pattern not found: {}", pattern_str),
            });
        }
    }

    Ok(DetectionResult {
        detected: true,
        version: None,
        details: format!("Command check passed: {}", config.command),
    })
}

async fn execute_shell_command(command: &str, timeout: Duration) -> Result<std::process::Output> {
    #[cfg(target_os = "windows")]
    let (shell, flag) = ("cmd", "/C");

    #[cfg(target_os = "linux")]
    let (shell, flag) = ("sh", "-c");

    let output = tokio::time::timeout(
        timeout,
        tokio::process::Command::new(shell)
            .arg(flag)
            .arg(command)
            .output(),
    )
    .await
    .context("Command timed out")?
    .context("Failed to execute command")?;

    Ok(output)
}
