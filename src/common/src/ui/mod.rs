pub mod gauge;
pub mod layout;
pub mod rendering;

// Re-export commonly used types
pub use layout::{IndentLevel, Layout};
pub use rendering::{constants, OutputWriter, TerminalInfo};

// Re-export shared TUI primitives
pub use rendering::{
    extract_sse_time, format_separator, format_wall_clock, pad_visible, terminal_dimensions,
    truncate_visible, visible_length,
};
