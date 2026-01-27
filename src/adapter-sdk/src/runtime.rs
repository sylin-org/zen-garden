//! Adapter runtime - main loop and shutdown coordination
//!
//! The runtime manages:
//! - HTTP command server lifecycle
//! - SSE client lifecycle (optional)
//! - Graceful shutdown on Ctrl+C or /shutdown

use std::sync::Arc;
use tokio::task::JoinHandle;

use crate::cli::AdapterConfig;
use crate::handler::CommandHandler;
use crate::server;
use crate::sse::{EventHandler, SseClient, SseClientConfig};

/// Adapter runtime builder and executor
///
/// # Example
///
/// ```ignore
/// AdapterRuntime::new(config, "my-adapter")
///     .command_handler(my_handler)
///     .event_handler(my_event_handler)  // optional
///     .run()
///     .await
/// ```
pub struct AdapterRuntime<H: CommandHandler> {
    config: AdapterConfig,
    adapter_name: String,
    handler: Option<Arc<H>>,
    event_handler: Option<Box<dyn FnOnce(SseClientConfig) -> JoinHandle<()> + Send>>,
}

impl<H: CommandHandler> AdapterRuntime<H> {
    /// Create a new runtime
    ///
    /// # Arguments
    ///
    /// * `config` - Parsed CLI config
    /// * `adapter_name` - Adapter identifier (e.g., "cricket")
    pub fn new(config: AdapterConfig, adapter_name: impl Into<String>) -> Self {
        Self {
            config,
            adapter_name: adapter_name.into(),
            handler: None,
            event_handler: None,
        }
    }

    /// Set the command handler
    pub fn command_handler(mut self, handler: H) -> Self {
        self.handler = Some(Arc::new(handler));
        self
    }

    /// Set the command handler (Arc version)
    pub fn command_handler_arc(mut self, handler: Arc<H>) -> Self {
        self.handler = Some(handler);
        self
    }

    /// Set an SSE event handler
    ///
    /// The handler will be connected to the Moss presence stream.
    pub fn event_handler<E: EventHandler>(mut self, handler: E) -> Self {
        let handler = Arc::new(handler);
        self.event_handler = Some(Box::new(move |config: SseClientConfig| {
            SseClient::start(config, handler)
        }));
        self
    }

    /// Run the adapter
    ///
    /// Starts the HTTP server, optionally the SSE client, and waits
    /// for shutdown (Ctrl+C or /shutdown endpoint).
    pub async fn run(self) -> anyhow::Result<()> {
        let (stone, port) = self.config.validate_daemon()?;

        let handler = self
            .handler
            .ok_or_else(|| anyhow::anyhow!("Command handler not set"))?;

        tracing::info!(
            adapter = %self.adapter_name,
            stone = %stone,
            port = port,
            "Starting adapter"
        );

        // Start command server
        let (server_handle, mut shutdown_rx) =
            server::start_server(port, Arc::clone(&handler), &self.adapter_name).await?;

        // Start SSE client if handler provided
        let sse_handle = if let Some(start_sse) = self.event_handler {
            let sse_config = SseClientConfig::new(stone);
            Some(start_sse(sse_config))
        } else {
            None
        };

        tracing::info!(
            adapter = %self.adapter_name,
            "Adapter running. Press Ctrl+C to stop."
        );

        // Wait for shutdown signal
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!(adapter = %self.adapter_name, "Ctrl+C received, shutting down");
            }
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    tracing::info!(adapter = %self.adapter_name, "Shutdown signal received");
                }
            }
        }

        // Cleanup
        tracing::info!(adapter = %self.adapter_name, "Shutting down adapter");

        // Notify handler
        handler.on_shutdown().await;

        // Stop tasks
        server_handle.abort();
        if let Some(handle) = sse_handle {
            handle.abort();
        }

        tracing::info!(adapter = %self.adapter_name, "Adapter stopped");
        Ok(())
    }
}

/// Initialize tracing with standard adapter configuration
///
/// Call this early in main() before other logging.
pub fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handler::CommandResult;
    use async_trait::async_trait;

    struct TestHandler;

    #[async_trait]
    impl CommandHandler for TestHandler {
        async fn handle(&self, _args: &[String]) -> CommandResult {
            CommandResult::success("OK")
        }
    }

    #[test]
    fn test_runtime_builder() {
        let config = AdapterConfig {
            stone: Some("http://localhost:7185".into()),
            port: Some(7187),
            dump_commands: false,
        };

        let _runtime = AdapterRuntime::new(config, "test").command_handler(TestHandler);
        // Just verify it builds without running
    }
}
