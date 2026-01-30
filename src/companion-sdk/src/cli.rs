//! Standard CLI arguments for Companions
//!
//! All Companions share these common arguments:
//! - `--stone <endpoint>` - Moss HTTP endpoint
//! - `--port <port>` - Assigned port for command server
//! - `--dump-commands` - Output manifest and exit (handled by garden_common)

use clap::Parser;

/// Standard Companion CLI arguments
///
/// Use `CompanionConfig::from_cli()` for simple Companions, or
/// `CompanionConfig::parse_with::<T>()` to combine with Companion-specific args.
#[derive(Parser, Debug, Clone)]
#[command(about = "Zen Garden Companion")]
pub struct CompanionConfig {
    /// Stone endpoint (e.g., http://10.0.0.5:7185)
    #[arg(short, long, env = "GARDEN_STONE")]
    pub stone: Option<String>,

    /// Command server port (assigned by Moss)
    #[arg(long, env = "companion_port")]
    pub port: Option<u16>,

    /// Output command manifest and exit
    #[arg(long)]
    pub dump_commands: bool,
}

impl CompanionConfig {
    /// Parse CLI arguments
    pub fn from_cli() -> Self {
        Self::parse()
    }

    /// Parse with additional Companion-specific arguments
    ///
    /// # Example
    ///
    /// ```ignore
    /// #[derive(Parser)]
    /// struct MyCli {
    ///     #[command(flatten)]
    ///     Companion: CompanionConfig,
    ///     
    ///     #[arg(long)]
    ///     my_option: String,
    /// }
    ///
    /// let cli = MyCli::parse();
    /// let config = cli.Companion;
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
        let config = CompanionConfig {
            stone: None,
            port: Some(7187),
            dump_commands: false,
        };
        assert!(config.validate_daemon().is_err());
    }

    #[test]
    fn test_validate_daemon_missing_port() {
        let config = CompanionConfig {
            stone: Some("http://localhost:7185".into()),
            port: None,
            dump_commands: false,
        };
        assert!(config.validate_daemon().is_err());
    }

    #[test]
    fn test_validate_daemon_success() {
        let config = CompanionConfig {
            stone: Some("http://localhost:7185".into()),
            port: Some(7187),
            dump_commands: false,
        };
        let (stone, port) = config.validate_daemon().unwrap();
        assert_eq!(stone, "http://localhost:7185");
        assert_eq!(port, 7187);
    }
}
