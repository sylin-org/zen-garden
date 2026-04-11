//! Tracing layer that broadcasts formatted log events to a channel
//!
//! Used by the log streaming API endpoint (`GET /api/v1/stone/logs/stream`).
//! Each tracing event is formatted as a single line and sent to a broadcast
//! channel. SSE subscribers receive live log events.

use std::fmt;
use tokio::sync::broadcast;
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;

/// Tracing layer that sends formatted log lines to a broadcast channel
pub struct LogBroadcastLayer {
    tx: broadcast::Sender<String>,
}

impl LogBroadcastLayer {
    pub fn new(tx: broadcast::Sender<String>) -> Self {
        Self { tx }
    }
}

impl<S> Layer<S> for LogBroadcastLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();
        let level = metadata.level();
        let target = metadata.target();

        // Extract the message field from the event
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);

        let line = format!(
            "{} {} {} {}",
            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ"),
            level,
            target,
            visitor.message,
        );

        // Best-effort send; if no subscribers or channel full, drop silently
        let _ = self.tx.send(line);
    }
}

/// Visitor that extracts the message field from a tracing event
#[derive(Default)]
struct MessageVisitor {
    message: String,
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        } else if self.message.is_empty() {
            // Capture first field as fallback
            self.message = format!("{}={:?}", field.name(), value);
        } else {
            // Append additional fields
            self.message
                .push_str(&format!(" {}={:?}", field.name(), value));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else if self.message.is_empty() {
            self.message = format!("{}={}", field.name(), value);
        } else {
            self.message
                .push_str(&format!(" {}={}", field.name(), value));
        }
    }
}
