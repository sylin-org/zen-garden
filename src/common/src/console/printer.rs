//! Console printer with deduplication

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Instant;

#[cfg(target_os = "linux")]
use std::fs::OpenOptions;
#[cfg(target_os = "linux")]
use std::io::Write;

use super::events::{ConsoleEvent, EventCategory, EventStatus};
use super::formatters::OutputFormatter;
use super::modes::ConsoleMode;

/// Event deduplicator to prevent high-frequency event spam
pub struct EventDeduplicator {
    seen: HashMap<String, Instant>,
    ttl_seconds: u64,
}

impl EventDeduplicator {
    pub fn new(ttl_seconds: u64) -> Self {
        Self {
            seen: HashMap::new(),
            ttl_seconds,
        }
    }

    /// Check if event should be emitted (returns true) or suppressed (returns false)
    pub fn should_emit(&mut self, event: &ConsoleEvent) -> bool {
        let key = event.dedupe_key();
        let now = Instant::now();

        // Clean up expired entries (simple approach: check on each call)
        self.seen
            .retain(|_, last_seen| now.duration_since(*last_seen).as_secs() < self.ttl_seconds);

        // Check if we've seen this event recently
        if let Some(last_seen) = self.seen.get(&key) {
            if now.duration_since(*last_seen).as_secs() < self.ttl_seconds {
                return false; // Suppress duplicate
            }
        }

        // Record this event
        self.seen.insert(key, now);
        true
    }
}

/// Console printer with pluggable formatters
pub struct ConsolePrinter {
    mode: Arc<RwLock<ConsoleMode>>,
    deduplicator: Arc<RwLock<EventDeduplicator>>,
    formatter: Box<dyn OutputFormatter + Send + Sync>,
}

impl ConsolePrinter {
    #[allow(dead_code)]
    pub fn new(mode: ConsoleMode) -> Self {
        Self::with_dedup_ttl(mode, 10)
    }

    /// Create console printer with custom deduplication TTL
    pub fn with_dedup_ttl(mode: ConsoleMode, dedup_ttl_secs: u64) -> Self {
        use super::formatters::TtyFormatter;
        Self {
            mode: Arc::new(RwLock::new(mode)),
            deduplicator: Arc::new(RwLock::new(EventDeduplicator::new(dedup_ttl_secs))),
            formatter: Box::new(TtyFormatter::new()),
        }
    }

    /// Create console printer with custom formatter (for SSE, API, etc.)
    #[allow(dead_code)]
    pub fn with_formatter(
        mode: ConsoleMode,
        formatter: Box<dyn OutputFormatter + Send + Sync>,
    ) -> Self {
        Self {
            mode: Arc::new(RwLock::new(mode)),
            deduplicator: Arc::new(RwLock::new(EventDeduplicator::new(10))),
            formatter,
        }
    }

    /// Update console mode (for remote control)
    pub fn set_mode(&self, mode: ConsoleMode) {
        if let Ok(mut m) = self.mode.write() {
            *m = mode;
        }
    }

    /// Get current console mode
    pub fn get_mode(&self) -> ConsoleMode {
        self.mode.read().map(|m| *m).unwrap_or_default()
    }

    /// Emit a console event (respects mode filtering and deduplication)
    pub fn emit(&self, event: ConsoleEvent) {
        let mode = self.get_mode();

        // Filter by mode
        if !self.should_display(&event, mode) {
            return;
        }

        // Check deduplication for high-frequency events
        let should_emit = {
            let mut dedup = self.deduplicator.write().unwrap();
            dedup.should_emit(&event)
        };

        if !should_emit {
            return;
        }

        // Format using pluggable formatter and print
        let formatted = self.formatter.format(&event);

        // On Linux, write critical events to /dev/tty1 regardless of mode
        // In verbose mode, write ALL events to tty1
        #[cfg(target_os = "linux")]
        {
            let should_write_tty =
                mode == ConsoleMode::Verbose || Self::is_critical_tty_event(&event);

            if should_write_tty {
                if let Ok(mut tty) = OpenOptions::new().write(true).open("/dev/tty1") {
                    let _ = writeln!(tty, "{}", formatted);
                }
            }
        }

        // Always write to stdout for journal
        println!("{}", formatted);
    }

    /// Determine if event should be displayed in given mode
    fn should_display(&self, event: &ConsoleEvent, mode: ConsoleMode) -> bool {
        match mode {
            ConsoleMode::Silent => false,
            ConsoleMode::Minimal => {
                // Only critical system events
                matches!(event.category, EventCategory::System)
                    && matches!(
                        event.status,
                        EventStatus::Starting
                            | EventStatus::Ready
                            | EventStatus::Stopped
                            | EventStatus::FirstBoot
                            | EventStatus::FirstBootDone
                            | EventStatus::FsError
                    )
            }
            ConsoleMode::Informative => {
                // All high-level lifecycle events (exclude verbose-only)
                // Special case: Docker | CONNECTED is visible, Services | CONNECTED is not
                if event.status == EventStatus::Connected {
                    matches!(event.category, EventCategory::Docker)
                } else {
                    !matches!(
                        event.status,
                        EventStatus::Reading
                            | EventStatus::TryingCompose
                            | EventStatus::NoCompat
                            | EventStatus::LanternUnreachable
                            | EventStatus::SseLag
                    )
                }
            }
            ConsoleMode::Verbose => true, // Show everything
        }
    }

    /// Check if this event should always be visible on physical console (tty1)
    /// regardless of console mode. Used for critical startup/restart visibility.
    #[cfg(target_os = "linux")]
    fn is_critical_tty_event(event: &ConsoleEvent) -> bool {
        // System lifecycle events (startup, restart, ready)
        if matches!(event.category, EventCategory::System) {
            return matches!(
                event.status,
                EventStatus::Starting
                    | EventStatus::Ready
                    | EventStatus::Stopped
                    | EventStatus::FirstBoot
                    | EventStatus::FirstBootDone
            );
        }

        // Jobs starting/completing (auto-install visibility)
        if matches!(event.category, EventCategory::Jobs) {
            return matches!(
                event.status,
                EventStatus::Started | EventStatus::Completed | EventStatus::Failed
            );
        }

        // Ops update events (deploy/update visibility on TTY1)
        if matches!(event.category, EventCategory::Ops) {
            return matches!(
                event.status,
                EventStatus::Active
                    | EventStatus::Staged
                    | EventStatus::RestartTriggered
                    | EventStatus::RestartError
                    | EventStatus::ShutdownDone
            );
        }

        // Docker connection (critical for understanding service readiness)
        if matches!(event.category, EventCategory::Docker)
            && matches!(event.status, EventStatus::Connected)
        {
            return true;
        }

        false
    }
}
