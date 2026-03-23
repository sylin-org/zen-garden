//! Platform-specific timer management for nurturing schedules
//!
//! Provides cross-platform timer creation and management:
//! - **Linux**: systemd timer units (zen-nurturing-{name}.timer)
//! - **Windows**: Task Scheduler tasks (via schtasks.exe)
//!
//! # Usage
//! ```rust,ignore
//! use garden_common::infra::timer::{PlatformTimer, TimerConfig};
//!
//! let timer = PlatformTimer::new();
//! timer.create("mongodb", &TimerConfig::default()).await?;
//! timer.remove("mongodb").await?;
//! ```

use anyhow::{Context, Result};
use std::sync::LazyLock;
use std::time::Duration;

/// Shared HTTP client for timer trigger requests.
static TIMER_HTTP: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("timer HTTP client")
});

/// Timer configuration for nurturing schedules
#[derive(Debug, Clone)]
pub struct TimerConfig {
    /// Interval between nurturing runs (default: 24 hours)
    pub interval: Duration,
    /// Randomized delay to spread load (default: 30 minutes)
    pub randomized_delay: Duration,
    /// Whether the timer should persist across reboots
    pub persistent: bool,
    /// Description for the timer
    pub description: Option<String>,
}

impl Default for TimerConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(24 * 60 * 60), // 24 hours
            randomized_delay: Duration::from_secs(30 * 60), // 30 minutes
            persistent: true,
            description: None,
        }
    }
}

impl TimerConfig {
    /// Create a new timer config with the given interval
    pub fn with_interval(interval: Duration) -> Self {
        Self {
            interval,
            ..Default::default()
        }
    }

    /// Create a timer config for testing (short interval)
    pub fn for_testing() -> Self {
        Self {
            interval: Duration::from_secs(60), // 1 minute
            randomized_delay: Duration::from_secs(5),
            persistent: false,
            description: Some("Test timer".into()),
        }
    }

    /// Format interval for systemd (e.g., "24h", "1d")
    #[cfg(any(target_os = "linux", test))]
    fn systemd_interval(&self) -> String {
        let secs = self.interval.as_secs();
        if secs >= 86400 && secs.is_multiple_of(86400) {
            format!("{}d", secs / 86400)
        } else if secs >= 3600 && secs.is_multiple_of(3600) {
            format!("{}h", secs / 3600)
        } else if secs >= 60 && secs.is_multiple_of(60) {
            format!("{}m", secs / 60)
        } else {
            format!("{}s", secs)
        }
    }

    /// Format randomized delay for systemd
    #[cfg(target_os = "linux")]
    fn systemd_random_delay(&self) -> String {
        let secs = self.randomized_delay.as_secs();
        if secs >= 60 && secs.is_multiple_of(60) {
            format!("{}m", secs / 60)
        } else {
            format!("{}s", secs)
        }
    }

    /// Format interval for Task Scheduler (minutes)
    #[cfg(target_os = "windows")]
    fn task_scheduler_minutes(&self) -> u64 {
        self.interval.as_secs() / 60
    }
}

/// Result of a timer operation
#[derive(Debug, Clone)]
pub struct TimerResult {
    /// Name of the timer
    pub name: String,
    /// Whether the operation succeeded
    pub success: bool,
    /// Human-readable message
    pub message: String,
}

impl TimerResult {
    pub fn success(name: &str, message: &str) -> Self {
        Self {
            name: name.to_string(),
            success: true,
            message: message.to_string(),
        }
    }

    pub fn failure(name: &str, message: &str) -> Self {
        Self {
            name: name.to_string(),
            success: false,
            message: message.to_string(),
        }
    }
}

/// Platform-agnostic timer manager
///
/// Provides unified interface for creating and managing nurturing timers
/// across Linux (systemd) and Windows (Task Scheduler).
pub struct PlatformTimer {
    /// Command to execute when timer fires (curl to local API)
    api_base_url: String,
}

impl PlatformTimer {
    /// Create a new platform timer with default API URL
    pub fn new() -> Self {
        Self::with_api_url("http://127.0.0.1:7185")
    }

    /// Create with custom API base URL
    pub fn with_api_url(api_base_url: &str) -> Self {
        Self {
            api_base_url: api_base_url.to_string(),
        }
    }

    /// Create a nurturing timer for an offering
    pub async fn create(&self, offering_name: &str, config: &TimerConfig) -> Result<TimerResult> {
        #[cfg(target_os = "linux")]
        {
            self.create_systemd_timer(offering_name, config).await
        }

        #[cfg(target_os = "windows")]
        {
            self.create_windows_task(offering_name, config).await
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            Ok(TimerResult::failure(
                offering_name,
                "Timer creation not supported on this platform",
            ))
        }
    }

    /// Remove a nurturing timer
    pub async fn remove(&self, offering_name: &str) -> Result<TimerResult> {
        #[cfg(target_os = "linux")]
        {
            self.remove_systemd_timer(offering_name).await
        }

        #[cfg(target_os = "windows")]
        {
            self.remove_windows_task(offering_name).await
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            Ok(TimerResult::failure(
                offering_name,
                "Timer removal not supported on this platform",
            ))
        }
    }

    /// Rename a nurturing timer
    pub async fn rename(
        &self,
        old_name: &str,
        new_name: &str,
        config: &TimerConfig,
    ) -> Result<TimerResult> {
        // Remove old timer
        let remove_result = self.remove(old_name).await?;
        if !remove_result.success {
            tracing::warn!(
                old_name,
                message = %remove_result.message,
                "Failed to remove old timer during rename (continuing)"
            );
        }

        // Create new timer
        self.create(new_name, config).await
    }

    /// Check if a timer exists
    pub async fn exists(&self, offering_name: &str) -> Result<bool> {
        #[cfg(target_os = "linux")]
        {
            self.systemd_timer_exists(offering_name).await
        }

        #[cfg(target_os = "windows")]
        {
            self.windows_task_exists(offering_name).await
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            Ok(false)
        }
    }

    /// List all nurturing timers
    pub async fn list(&self) -> Result<Vec<String>> {
        #[cfg(target_os = "linux")]
        {
            self.list_systemd_timers().await
        }

        #[cfg(target_os = "windows")]
        {
            self.list_windows_tasks().await
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            Ok(Vec::new())
        }
    }

    /// Trigger a timer immediately (for testing)
    pub async fn trigger(&self, offering_name: &str) -> Result<TimerResult> {
        // Call the nurture API endpoint directly
        let url = format!(
            "{}/api/v1/nurturing/{}/trigger",
            self.api_base_url, offering_name
        );

        let response = TIMER_HTTP.post(&url).send().await;

        match response {
            Ok(resp) if resp.status().is_success() => Ok(TimerResult::success(
                offering_name,
                "Nurturing triggered successfully",
            )),
            Ok(resp) => Ok(TimerResult::failure(
                offering_name,
                &format!("Trigger failed: HTTP {}", resp.status()),
            )),
            Err(e) => Ok(TimerResult::failure(
                offering_name,
                &format!("Trigger failed: {}", e),
            )),
        }
    }

    // ========================================================================
    // Linux systemd implementation
    // ========================================================================

    #[cfg(target_os = "linux")]
    async fn create_systemd_timer(
        &self,
        offering_name: &str,
        config: &TimerConfig,
    ) -> Result<TimerResult> {
        use tokio::process::Command;

        let timer_name = format!("zen-nurturing-{}", offering_name);
        let description = config
            .description
            .clone()
            .unwrap_or_else(|| format!("Garden nurturing timer for {}", offering_name));

        // Timer unit content
        let timer_content = format!(
            r#"[Unit]
Description={}

[Timer]
OnBootSec=5m
OnUnitActiveSec={}
RandomizedDelaySec={}
Persistent={}

[Install]
WantedBy=timers.target
"#,
            description,
            config.systemd_interval(),
            config.systemd_random_delay(),
            if config.persistent { "true" } else { "false" }
        );

        // Service unit content (what the timer runs)
        let service_content = format!(
            r#"[Unit]
Description=Garden nurturing service for {}

[Service]
Type=oneshot
ExecStart=/usr/bin/curl -s -X POST {}/api/v1/nurturing/{}/trigger
"#,
            offering_name, self.api_base_url, offering_name
        );

        // Write timer unit
        let timer_path = format!("/etc/systemd/system/{}.timer", timer_name);
        tokio::fs::write(&timer_path, &timer_content)
            .await
            .context(format!("Failed to write timer unit: {}", timer_path))?;

        // Write service unit
        let service_path = format!("/etc/systemd/system/{}.service", timer_name);
        tokio::fs::write(&service_path, &service_content)
            .await
            .context(format!("Failed to write service unit: {}", service_path))?;

        // Reload systemd
        let reload = Command::new("systemctl")
            .args(["daemon-reload"])
            .output()
            .await
            .context("Failed to reload systemd")?;

        if !reload.status.success() {
            return Ok(TimerResult::failure(
                offering_name,
                &format!(
                    "systemctl daemon-reload failed: {}",
                    String::from_utf8_lossy(&reload.stderr)
                ),
            ));
        }

        // Enable and start timer
        let enable = Command::new("systemctl")
            .args(["enable", "--now", &format!("{}.timer", timer_name)])
            .output()
            .await
            .context("Failed to enable timer")?;

        if !enable.status.success() {
            return Ok(TimerResult::failure(
                offering_name,
                &format!(
                    "Failed to enable timer: {}",
                    String::from_utf8_lossy(&enable.stderr)
                ),
            ));
        }

        tracing::info!(
            offering = offering_name,
            timer = timer_name,
            interval = %config.systemd_interval(),
            "Created systemd nurturing timer"
        );

        Ok(TimerResult::success(
            offering_name,
            &format!(
                "Timer {} created with {} interval",
                timer_name,
                config.systemd_interval()
            ),
        ))
    }

    #[cfg(target_os = "linux")]
    async fn remove_systemd_timer(&self, offering_name: &str) -> Result<TimerResult> {
        use tokio::process::Command;

        let timer_name = format!("zen-nurturing-{}", offering_name);

        // Stop and disable timer
        let _ = Command::new("systemctl")
            .args(["stop", &format!("{}.timer", timer_name)])
            .output()
            .await;

        let _ = Command::new("systemctl")
            .args(["disable", &format!("{}.timer", timer_name)])
            .output()
            .await;

        // Remove unit files
        let timer_path = format!("/etc/systemd/system/{}.timer", timer_name);
        let service_path = format!("/etc/systemd/system/{}.service", timer_name);

        let timer_removed = tokio::fs::remove_file(&timer_path).await.is_ok();
        let service_removed = tokio::fs::remove_file(&service_path).await.is_ok();

        // Reload systemd
        let _ = Command::new("systemctl")
            .args(["daemon-reload"])
            .output()
            .await;

        if timer_removed || service_removed {
            tracing::info!(
                offering = offering_name,
                timer = timer_name,
                "Removed systemd nurturing timer"
            );
            Ok(TimerResult::success(
                offering_name,
                &format!("Timer {} removed", timer_name),
            ))
        } else {
            Ok(TimerResult::success(
                offering_name,
                "Timer not found (already removed)",
            ))
        }
    }

    #[cfg(target_os = "linux")]
    async fn systemd_timer_exists(&self, offering_name: &str) -> Result<bool> {
        let timer_path = format!("/etc/systemd/system/zen-nurturing-{}.timer", offering_name);
        Ok(tokio::fs::metadata(&timer_path).await.is_ok())
    }

    #[cfg(target_os = "linux")]
    async fn list_systemd_timers(&self) -> Result<Vec<String>> {
        use tokio::process::Command;

        let output = Command::new("systemctl")
            .args(["list-timers", "--all", "--no-legend"])
            .output()
            .await
            .context("Failed to list timers")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut timers = Vec::new();

        for line in stdout.lines() {
            if line.contains("zen-nurturing-") {
                // Extract timer name from output
                if let Some(name) = line.split_whitespace().last() {
                    if let Some(offering) = name.strip_prefix("zen-nurturing-") {
                        if let Some(offering) = offering.strip_suffix(".timer") {
                            timers.push(offering.to_string());
                        }
                    }
                }
            }
        }

        Ok(timers)
    }

    // ========================================================================
    // Windows Task Scheduler implementation
    // ========================================================================

    #[cfg(target_os = "windows")]
    async fn create_windows_task(
        &self,
        offering_name: &str,
        config: &TimerConfig,
    ) -> Result<TimerResult> {
        use tokio::process::Command;

        let task_name = format!("ZenGarden-Nurturing-{}", offering_name);
        let interval_minutes = config.task_scheduler_minutes().max(1); // Minimum 1 minute

        // PowerShell command to create the task
        // Using curl.exe (built into Windows 10+) or Invoke-WebRequest
        let trigger_command = format!(
            "curl.exe -s -X POST {}/api/v1/nurturing/{}/trigger",
            self.api_base_url, offering_name
        );

        // Create scheduled task using schtasks
        // /SC MINUTE for recurring, /MO for interval
        let output = Command::new("schtasks")
            .args([
                "/Create",
                "/F", // Force overwrite if exists
                "/SC",
                "MINUTE",
                "/MO",
                &interval_minutes.to_string(),
                "/TN",
                &task_name,
                "/TR",
                &format!("cmd /c {}", trigger_command),
                "/RL",
                "LIMITED", // Run with limited privileges
            ])
            .output()
            .await
            .context("Failed to create scheduled task")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Ok(TimerResult::failure(
                offering_name,
                &format!("Failed to create task: {}", stderr),
            ));
        }

        tracing::info!(
            offering = offering_name,
            task = task_name,
            interval_minutes,
            "Created Windows scheduled task"
        );

        Ok(TimerResult::success(
            offering_name,
            &format!(
                "Task {} created with {} minute interval",
                task_name, interval_minutes
            ),
        ))
    }

    #[cfg(target_os = "windows")]
    async fn remove_windows_task(&self, offering_name: &str) -> Result<TimerResult> {
        use tokio::process::Command;

        let task_name = format!("ZenGarden-Nurturing-{}", offering_name);

        let output = Command::new("schtasks")
            .args(["/Delete", "/F", "/TN", &task_name])
            .output()
            .await
            .context("Failed to delete scheduled task")?;

        if output.status.success() {
            tracing::info!(
                offering = offering_name,
                task = task_name,
                "Removed Windows scheduled task"
            );
            Ok(TimerResult::success(
                offering_name,
                &format!("Task {} removed", task_name),
            ))
        } else {
            // Check if task simply didn't exist
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("does not exist") {
                Ok(TimerResult::success(
                    offering_name,
                    "Task not found (already removed)",
                ))
            } else {
                Ok(TimerResult::failure(
                    offering_name,
                    &format!("Failed to remove task: {}", stderr),
                ))
            }
        }
    }

    #[cfg(target_os = "windows")]
    async fn windows_task_exists(&self, offering_name: &str) -> Result<bool> {
        use tokio::process::Command;

        let task_name = format!("ZenGarden-Nurturing-{}", offering_name);

        let output = Command::new("schtasks")
            .args(["/Query", "/TN", &task_name])
            .output()
            .await
            .context("Failed to query task")?;

        Ok(output.status.success())
    }

    #[cfg(target_os = "windows")]
    async fn list_windows_tasks(&self) -> Result<Vec<String>> {
        use tokio::process::Command;

        let output = Command::new("schtasks")
            .args(["/Query", "/FO", "LIST"])
            .output()
            .await
            .context("Failed to list tasks")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut tasks = Vec::new();

        for line in stdout.lines() {
            if line.contains("ZenGarden-Nurturing-")
                && let Some(name) = line.split(':').next_back()
            {
                let name = name.trim();
                if let Some(offering) = name.strip_prefix("ZenGarden-Nurturing-") {
                    tasks.push(offering.to_string());
                }
            }
        }

        Ok(tasks)
    }
}

impl Default for PlatformTimer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timer_config_default() {
        let config = TimerConfig::default();
        assert_eq!(config.interval.as_secs(), 24 * 60 * 60);
        assert_eq!(config.randomized_delay.as_secs(), 30 * 60);
        assert!(config.persistent);
    }

    #[test]
    fn test_timer_config_systemd_interval() {
        let config = TimerConfig::with_interval(Duration::from_secs(86400));
        assert_eq!(config.systemd_interval(), "1d");

        let config = TimerConfig::with_interval(Duration::from_secs(3600));
        assert_eq!(config.systemd_interval(), "1h");

        let config = TimerConfig::with_interval(Duration::from_secs(60));
        assert_eq!(config.systemd_interval(), "1m");

        let config = TimerConfig::with_interval(Duration::from_secs(45));
        assert_eq!(config.systemd_interval(), "45s");
    }

    #[test]
    fn test_timer_config_task_scheduler_minutes() {
        let config = TimerConfig::with_interval(Duration::from_secs(86400));
        assert_eq!(config.task_scheduler_minutes(), 1440);

        let config = TimerConfig::with_interval(Duration::from_secs(3600));
        assert_eq!(config.task_scheduler_minutes(), 60);
    }

    #[test]
    fn test_timer_result() {
        let success = TimerResult::success("mongodb", "Created timer");
        assert!(success.success);
        assert_eq!(success.name, "mongodb");

        let failure = TimerResult::failure("redis", "Failed");
        assert!(!failure.success);
    }

    #[test]
    fn test_platform_timer_creation() {
        let timer = PlatformTimer::new();
        assert_eq!(timer.api_base_url, "http://127.0.0.1:7185");

        let custom = PlatformTimer::with_api_url("http://localhost:8080");
        assert_eq!(custom.api_base_url, "http://localhost:8080");
    }
}
