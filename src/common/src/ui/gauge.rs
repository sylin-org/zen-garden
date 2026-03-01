//! Terminal gauge rendering
//!
//! Horizontal bar gauges using ASCII characters that work on every terminal.
//! Adapts to available width with color thresholds matching Firefly LED companion.

use colored::{ColoredString, Colorize};

/// Color thresholds for gauges (matches Firefly LED companion)
const THRESHOLD_WARN: f64 = 60.0;
const THRESHOLD_CRIT: f64 = 85.0;

/// Format a gauge bar for terminal display.
///
/// Adapts to available width:
/// - `>= 16 chars`: `CPU [=====---] 42%`
/// - `<  16 chars`: `CPU 42%`
///
/// The `width` parameter is the total width available for the entire gauge
/// including the label.
pub fn format_gauge(label: &str, value: f64, width: usize, color: bool) -> String {
    let value = value.clamp(0.0, 100.0);
    let pct_str = format!("{:>3.0}%", value);

    // Minimum for bar rendering: "LBL [==--] 42%" needs label + 1 space + 2 brackets + 4 bar + 1 space + 4 pct = label+12
    let bar_overhead = label.len() + 12; // "LBL " + "[" + "]" + " " + "NNN%"
    if width < bar_overhead || width < 16 {
        // Narrow: just "LBL 42%"
        let val_str = format!("{:>2.0}", value);
        return format!("{} {}", label, val_str);
    }

    let bar_width = width - bar_overhead + 4; // inner bar chars (the +4 accounts for min bar)
    let bar_width = bar_width.min(40); // cap at 40 chars wide
    let filled = ((value / 100.0) * bar_width as f64).round() as usize;
    let empty = bar_width.saturating_sub(filled);

    let fill_str = "=".repeat(filled);
    let empty_str = "-".repeat(empty);

    if color {
        let colored_fill = colorize_value(&fill_str, value);
        let colored_pct = colorize_value(&pct_str, value);
        format!("{} [{}{}] {}", label, colored_fill, empty_str.dimmed(), colored_pct)
    } else {
        format!("{} [{}{}] {}", label, fill_str, empty_str, pct_str)
    }
}

/// Format a network rate for display.
pub fn format_net_rate(bytes_per_sec: u64) -> String {
    if bytes_per_sec >= 1_073_741_824 {
        format!("{:.1} GB/s", bytes_per_sec as f64 / 1_073_741_824.0)
    } else if bytes_per_sec >= 1_048_576 {
        format!("{:.1} MB/s", bytes_per_sec as f64 / 1_048_576.0)
    } else if bytes_per_sec >= 1024 {
        format!("{:.0} KB/s", bytes_per_sec as f64 / 1024.0)
    } else {
        format!("{} B/s", bytes_per_sec)
    }
}

/// Colorize a string based on gauge value thresholds.
fn colorize_value(s: &str, value: f64) -> ColoredString {
    if value >= THRESHOLD_CRIT {
        s.red()
    } else if value >= THRESHOLD_WARN {
        s.yellow()
    } else {
        s.green()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_narrow_gauge() {
        let result = format_gauge("CPU", 42.0, 12, false);
        assert_eq!(result, "CPU 42");
    }

    #[test]
    fn test_wide_gauge_format() {
        let result = format_gauge("CPU", 50.0, 30, false);
        assert!(result.starts_with("CPU ["));
        assert!(result.contains("="));
        assert!(result.contains("-"));
        assert!(result.ends_with("50%"));
    }

    #[test]
    fn test_gauge_clamp() {
        let result = format_gauge("X", 150.0, 30, false);
        assert!(result.contains("100%"));
        let result = format_gauge("X", -10.0, 30, false);
        assert!(result.contains("  0%"));
    }

    #[test]
    fn test_net_rate_formatting() {
        assert_eq!(format_net_rate(500), "500 B/s");
        assert_eq!(format_net_rate(2048), "2 KB/s");
        assert_eq!(format_net_rate(1_500_000), "1.4 MB/s");
        assert_eq!(format_net_rate(2_000_000_000), "1.9 GB/s");
    }
}
