//! Console event formatters

use super::events::{ConsoleEvent, Severity};
use chrono::{Local, Timelike};
use std::io::IsTerminal;

/// Output formatter trait for different rendering contexts (DRY principle)
pub trait OutputFormatter {
    /// Format an event for the specific output context
    fn format(&self, event: &ConsoleEvent) -> String;
}

/// TTY console formatter (supports colors)
pub struct TtyFormatter {
    color_enabled: bool,
}

impl TtyFormatter {
    pub fn new() -> Self {
        Self {
            color_enabled: Self::detect_color_support(),
        }
    }

    /// Detect if terminal supports colors
    fn detect_color_support() -> bool {
        std::env::var("NO_COLOR").is_err() && std::io::stdin().is_terminal()
    }
}

impl Default for TtyFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputFormatter for TtyFormatter {
    fn format(&self, event: &ConsoleEvent) -> String {
        let now = Local::now();
        let time_str = format!("{:02}:{:02}:{:02}", now.hour(), now.minute(), now.second());
        let category_str = event.category.display_name();
        let status_str = event.status.display_name();

        let base = format!(
            "{} {} │ {} │ {}",
            time_str, category_str, status_str, event.message
        );

        // Apply colors based on event
        if self.color_enabled {
            let severity = event
                .hint
                .as_ref()
                .and_then(|h| h.severity)
                .unwrap_or_else(|| event.status.severity_hint());

            let color = event
                .hint
                .as_ref()
                .and_then(|h| h.color)
                .unwrap_or_else(|| severity.color());

            format!("{}{}\x1b[0m", color.code(), base)
        } else {
            base
        }
    }
}

/// SSE stream formatter (no colors, structured for event streaming)
pub struct SseFormatter;

impl OutputFormatter for SseFormatter {
    fn format(&self, event: &ConsoleEvent) -> String {
        let now = Local::now();
        let time_str = format!("{:02}:{:02}:{:02}", now.hour(), now.minute(), now.second());

        let severity = event
            .hint
            .as_ref()
            .and_then(|h| h.severity)
            .unwrap_or_else(|| event.status.severity_hint());

        // Structured format with severity prefix for SSE consumers
        let severity_prefix = match severity {
            Severity::Error => "[ERROR]",
            Severity::Warning => "[WARN]",
            Severity::Info => "[INFO]",
            Severity::Debug => "[DEBUG]",
        };

        format!(
            "{} {} {} │ {} │ {}",
            time_str,
            severity_prefix,
            event.category.display_name().trim(),
            event.status.display_name().trim(),
            event.message
        )
    }
}
