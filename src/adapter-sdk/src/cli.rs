//! Standard CLI arguments for adapters
//!
//! All adapters share these common arguments:
//! - `--stone <endpoint>` - Moss HTTP endpoint
//! - `--port <port>` - Assigned port for command server
//! - `--dump-commands` - Output manifest and exit (handled by garden_common)

use clap::Parser;

/// Standard adapter CLI arguments
///
/// Use `AdapterConfig::from_cli()` for simple adapters, or
/// `AdapterConfig::parse_with::<T>()` to combine with adapter-specific args.
#[derive(Parser, Debug, Clone)]
#[command(about = "Zen Garden Adapter")]
pub struct AdapterConfig {
    /// Stone endpoint (e.g., http://10.0.0.5:7185)
    #[arg(short, long, env = "GARDEN_STONE")]
    pub stone: Option<String>,

    /// Command server port (assigned by Moss)
    #[arg(long, env = "ADAPTER_PORT")]
    pub port: Option<u16>,

    /// Output command manifest and exit
    #[arg(long)]
    pub dump_commands: bool,
}

impl AdapterConfig {
    /// Parse CLI arguments
    pub fn from_cli() -> Self {
        Self::parse()
    }

    /// Parse with additional adapter-specific arguments
    ///
    /// # Example
    ///
    /// ```ignore
    /// #[derive(Parser)]
    /// struct MyCli {
    ///     #[command(flatten)]
    ///     adapter: AdapterConfig,
    ///     
    ///     #[arg(long)]
    ///     my_option: String,
    /// }
    ///
    /// let cli = MyCli::parse();
    /// let config = cli.adapter;
    /// ```
    pub fn parse_with<T: Parser>() -> T {
        T::parse()
    }

    /// Validate that required fields are present for daemon mode
    ///
    /// Returns error if stone or port is missing.
    pub fn validate_daemon(&self) -> anyhow::Result<(&str, u16)> {
        let stone = self
            .stone
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("--stone endpoint required"))?;

        let port = self
            .port
            .ok_or_else(|| anyhow::anyhow!("--port required (assigned by Moss)"))?;

        Ok((stone, port))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_daemon_missing_stone() {
        let config = AdapterConfig {
            stone: None,
            port: Some(7187),
            dump_commands: false,
        };
        assert!(config.validate_daemon().is_err());
    }

    #[test]
    fn test_validate_daemon_missing_port() {
        let config = AdapterConfig {
            stone: Some("http://localhost:7185".into()),
            port: None,
            dump_commands: false,
        };
        assert!(config.validate_daemon().is_err());
    }

    #[test]
    fn test_validate_daemon_success() {
        let config = AdapterConfig {
            stone: Some("http://localhost:7185".into()),
            port: Some(7187),
            dump_commands: false,
        };
        let (stone, port) = config.validate_daemon().unwrap();
        assert_eq!(stone, "http://localhost:7185");
        assert_eq!(port, 7187);
    }
}
