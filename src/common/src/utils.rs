//! Zen Common Utilities
//! Helper functions for formatting and common operations

// Enhanced utility modules
pub mod env;
pub mod formatting;
pub mod fs;
pub mod ids;
pub mod json;
pub mod platform;
pub mod strings;
pub mod validation;

// Re-export commonly used functions for backwards compatibility
pub use formatting::{
    format_bytes,
    format_bytes_precision,
    format_bytes_short,
    format_bytes_whole,
    format_memory_mb,
    format_uptime,
};
