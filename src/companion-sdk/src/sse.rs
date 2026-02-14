//! SSE client for presence event subscription
//!
//! Connects to Moss SSE endpoint and dispatches events to handler.

use garden_common::presence::event_types::PRESENCE_STREAM_PATH;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;

/// Parsed SSE event
#[derive(Debug, Clone)]
pub struct SseEvent {
    /// Event type (e.g., "stone-online", "service-started")
    pub event_type: String,

    /// Event data (usually JSON)
    pub data: String,
}

/// Trait for handling SSE events
///
/// Implement this to react to presence events from Moss.
#[async_trait::async_trait]
pub trait EventHandler: Send + Sync + 'static {
    /// Handle an incoming SSE event
    async fn on_event(&self, event: SseEvent);
}

/// SSE client configuration
#[derive(Debug, Clone)]
pub struct SseClientConfig {
    /// Moss endpoint (e.g., "http://localhost:7185")
    pub stone_endpoint: String,

    /// Reconnect delay on disconnect
    pub reconnect_delay: Duration,

    /// SSE path (default: /api/v1/stone/presence/stream)
    pub path: String,
}

impl SseClientConfig {
    /// Create config with default settings
    pub fn new(stone_endpoint: impl Into<String>) -> Self {
        Self {
            stone_endpoint: stone_endpoint.into(),
            reconnect_delay: Duration::from_secs(5),
            path: PRESENCE_STREAM_PATH.into(),
        }
    }

    /// Set custom reconnect delay
    pub fn with_reconnect_delay(mut self, delay: Duration) -> Self {
        self.reconnect_delay = delay;
        self
    }

    /// Set custom SSE path
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    /// Get full SSE URL
    pub fn url(&self) -> String {
        format!("{}{}", self.stone_endpoint, self.path)
    }
}

/// SSE client for presence events
pub struct SseClient;

impl SseClient {
    /// Start SSE client in background
    ///
    /// Connects to Moss and dispatches events to handler.
    /// Automatically reconnects on disconnect with exponential backoff.
    pub fn start<H: EventHandler>(config: SseClientConfig, handler: Arc<H>) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut consecutive_failures = 0u32;
            // Backoff pattern: 1-2-4-8-16-32 seconds
            let backoff_secs = [1, 2, 4, 8, 16, 32];

            loop {
                let url = config.url();

                // Only log info on first attempt or after success
                if consecutive_failures == 0 {
                    tracing::info!(endpoint = %url, "Connecting to SSE stream");
                }

                match Self::connect_and_listen(&url, &handler).await {
                    Ok(()) => {
                        tracing::info!("SSE stream ended normally");
                        consecutive_failures = 0;
                    }
                    Err(e) => {
                        consecutive_failures += 1;

                        // Use debug for connection refused (expected during startup)
                        // Use warn for other errors or after many retries
                        let error_str = e.to_string();
                        if error_str.contains("refused") || error_str.contains("10061") {
                            if consecutive_failures <= 3 {
                                tracing::debug!(
                                    attempt = consecutive_failures,
                                    "SSE connection refused (service may be starting)"
                                );
                            } else {
                                tracing::warn!(
                                    attempt = consecutive_failures,
                                    "SSE connection still refused"
                                );
                            }
                        } else {
                            tracing::warn!(error = %e, attempt = consecutive_failures, "SSE connection error");
                        }
                    }
                }

                // Exponential backoff: 1-2-4-8-16-32 seconds
                let idx = (consecutive_failures as usize)
                    .saturating_sub(1)
                    .min(backoff_secs.len() - 1);
                let delay = Duration::from_secs(backoff_secs[idx]);
                tokio::time::sleep(delay).await;
            }
        })
    }

    /// Connect to SSE and process events
    async fn connect_and_listen<H: EventHandler>(
        endpoint: &str,
        handler: &Arc<H>,
    ) -> anyhow::Result<()> {
        let client = reqwest::Client::new();
        let response = client
            .get(endpoint)
            .header("Accept", "text/event-stream")
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("SSE endpoint returned {}", response.status());
        }

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        use futures_util::StreamExt;

        while let Some(result) = stream.next().await {
            let chunk = result?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // Process complete events (double newline delimited)
            while let Some(pos) = buffer.find("\n\n") {
                let event_text = buffer[..pos].to_string();
                buffer = buffer[pos + 2..].to_string();

                if let Some(event) = Self::parse_event(&event_text) {
                    handler.on_event(event).await;
                }
            }
        }

        Ok(())
    }

    /// Parse SSE event from text
    fn parse_event(text: &str) -> Option<SseEvent> {
        let mut event_type = String::new();
        let mut data = String::new();

        for line in text.lines() {
            if let Some(value) = line.strip_prefix("event:") {
                event_type = value.trim().to_string();
            } else if let Some(value) = line.strip_prefix("data:") {
                data = value.trim().to_string();
            }
        }

        if event_type.is_empty() && data.is_empty() {
            return None;
        }

        // Default event type
        if event_type.is_empty() {
            event_type = "message".to_string();
        }

        Some(SseEvent { event_type, data })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_event() {
        let text = "event: stone-online\ndata: {\"name\":\"test\"}";
        let event = SseClient::parse_event(text).unwrap();
        assert_eq!(event.event_type, "stone-online");
        assert_eq!(event.data, "{\"name\":\"test\"}");
    }

    #[test]
    fn test_parse_event_data_only() {
        let text = "data: hello";
        let event = SseClient::parse_event(text).unwrap();
        assert_eq!(event.event_type, "message");
        assert_eq!(event.data, "hello");
    }

    #[test]
    fn test_sse_config() {
        let config = SseClientConfig::new("http://localhost:7185")
            .with_reconnect_delay(Duration::from_secs(10));

        assert_eq!(
            config.url(),
            "http://localhost:7185/api/v1/stone/presence/stream"
        );
        assert_eq!(config.reconnect_delay, Duration::from_secs(10));
    }
}
