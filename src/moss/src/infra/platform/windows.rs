//! Windows platform runtime — stdout output, SCM lifecycle signals (ARCH-0002).

use garden_common::PlatformRuntime;

pub struct WindowsRuntime;

impl WindowsRuntime {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WindowsRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformRuntime for WindowsRuntime {
    fn write_line(&self, text: &str) {
        println!("{}", text);
    }

    fn notify_ready(&self) {
        // Future: SetServiceStatus(SERVICE_RUNNING) via Windows SCM
        tracing::debug!("notify_ready (Windows SCM integration pending)");
    }

    fn notify_stopping(&self) {
        // Future: SetServiceStatus(SERVICE_STOP_PENDING) via Windows SCM
        tracing::debug!("notify_stopping (Windows SCM integration pending)");
    }

    fn notify_watchdog(&self) {
        // No watchdog equivalent on Windows
    }

    fn notify_status(&self, status: &str, _extend_timeout_usec: Option<u64>) {
        // Future: SetServiceStatus with dwWaitHint via Windows SCM
        tracing::debug!(status = %status, "notify_status (Windows SCM integration pending)");
    }
}
