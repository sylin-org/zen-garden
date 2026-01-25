//! Console output modes

/// Console output mode - determines what events are displayed
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConsoleMode {
    /// No console output (Windows service, systemd with no TTY)
    Silent,
    /// Startup + critical events only (daemon default)
    Minimal,
    /// Major lifecycle events (interactive default)
    Informative,
    /// Full debug output (opt-in)
    Verbose,
}

impl Default for ConsoleMode {
    fn default() -> Self {
        Self::Minimal
    }
}

impl std::fmt::Display for ConsoleMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Silent => write!(f, "silent"),
            Self::Minimal => write!(f, "minimal"),
            Self::Informative => write!(f, "informative"),
            Self::Verbose => write!(f, "verbose"),
        }
    }
}

impl std::str::FromStr for ConsoleMode {
    type Err = anyhow::Error;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "silent" => Ok(Self::Silent),
            "minimal" => Ok(Self::Minimal),
            "informative" => Ok(Self::Informative),
            "verbose" => Ok(Self::Verbose),
            _ => Err(anyhow::anyhow!("Invalid console mode: {}", s)),
        }
    }
}
