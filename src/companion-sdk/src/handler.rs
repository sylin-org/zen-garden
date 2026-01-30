//! Command handler trait for Companions
//!
//! Implement [`CommandHandler`] to process commands from Moss.

// Re-export CommandResponse for convenience
pub use garden_common::command_manifest::CommandResponse;

/// Trait for handling Companion commands
///
/// Implement this trait to process commands dispatched from Moss.
///
/// # Example
///
/// ```ignore
/// use garden_companion_sdk::prelude::*;
/// use garden_common::command_manifest::CommandResponse;
///
/// struct MyHandler;
///
/// #[async_trait]
/// impl CommandHandler for MyHandler {
///     async fn handle(&self, args: &[String]) -> CommandResponse {
///         let cmd = args.first().map(|s| s.as_str()).unwrap_or("");
///         
///         match cmd {
///             "hello" => CommandResponse::success("Hello, World!"),
///             "echo" => CommandResponse::success(args[1..].join(" ")),
///             "" => CommandResponse::error("No command provided"),
///             _ => CommandResponse::error(format!("Unknown command: {}", cmd)),
///         }
///     }
/// }
/// ```
#[async_trait::async_trait]
pub trait CommandHandler: Send + Sync + 'static {
    /// Handle a command
    ///
    /// # Arguments
    ///
    /// * `args` - Command arguments (first element is command name, rest are parameters)
    ///
    /// # Returns
    ///
    /// A [`CommandResponse`] indicating success/failure and output
    async fn handle(&self, args: &[String]) -> CommandResponse;

    /// Optional: Called before shutdown
    ///
    /// Override this to perform cleanup when the Companion is shutting down.
    async fn on_shutdown(&self) {
        // Default: no-op
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use garden_common::command_manifest::ResponseStatus;

    #[test]
    fn test_command_response_success() {
        let result = CommandResponse::success("OK");
        assert_eq!(result.status, ResponseStatus::Success);
        assert_eq!(result.message, "OK");
    }

    #[test]
    fn test_command_response_error_with_suggestion() {
        let result = CommandResponse::error("Bad command")
            .with_suggestion("try this");
        assert_eq!(result.status, ResponseStatus::Error);
        assert_eq!(result.suggestions.len(), 1);
    }

    #[test]
    fn test_command_response_serialization() {
        let result = CommandResponse::success("Hello");
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"status\":\"success\""));
        assert!(json.contains("\"message\":\"Hello\""));
    }
}
