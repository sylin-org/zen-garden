//! Formatting utilities
//!
//! Consistent formatting for bytes, uptime, and display values.

/// Format bytes with customizable precision
pub fn format_bytes_precision(bytes: u64, precision: usize) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.prec$} TB", bytes as f64 / TB as f64, prec = precision)
    } else if bytes >= GB {
        format!("{:.prec$} GB", bytes as f64 / GB as f64, prec = precision)
    } else if bytes >= MB {
        format!("{:.prec$} MB", bytes as f64 / MB as f64, prec = precision)
    } else if bytes >= KB {
        format!("{:.prec$} KB", bytes as f64 / KB as f64, prec = precision)
    } else {
        format!("{} B", bytes)
    }
}

/// Format bytes with 2 decimal places (default)
pub fn format_bytes(bytes: u64) -> String {
    format_bytes_precision(bytes, 2)
}

/// Format bytes with 1 decimal place (for UI)
pub fn format_bytes_short(bytes: u64) -> String {
    format_bytes_precision(bytes, 1)
}

/// Format bytes as whole numbers (no decimals)
pub fn format_bytes_whole(bytes: u64) -> String {
    format_bytes_precision(bytes, 0)
}

/// Format memory in MB to GB display
pub fn format_memory_mb(mb: u64) -> String {
    format_bytes_short(mb * 1024 * 1024)
}

/// Format seconds into human-readable uptime (e.g., "2d 5h 30m", "3h 45m", "25m")
pub fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    if days > 0 {
        format!("{}d {}h {}m", days, hours, minutes)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, secs)
    } else {
        format!("{}s", secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes_precision() {
        assert_eq!(format_bytes_short(1_073_741_824), "1.0 GB");
        assert_eq!(format_bytes(1_073_741_824), "1.00 GB");
        assert_eq!(format_bytes_whole(1_073_741_824), "1 GB");
        
        assert_eq!(format_bytes_short(2_147_483_648), "2.0 GB");
        assert_eq!(format_bytes_short(5_242_880), "5.0 MB");
        assert_eq!(format_bytes_short(2048), "2.0 KB");
        assert_eq!(format_bytes_short(500), "500 B");
    }
    
    #[test]
    fn test_format_memory_mb() {
        assert_eq!(format_memory_mb(8192), "8.0 GB");
        assert_eq!(format_memory_mb(512), "512.0 MB");
    }

    #[test]
    fn test_format_bytes_edge_cases() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1), "1 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1_099_511_627_776), "1.00 TB");
    }

    #[test]
    fn test_format_uptime() {
        assert_eq!(format_uptime(0), "0s");
        assert_eq!(format_uptime(30), "30s");
        assert_eq!(format_uptime(90), "1m 30s");
        assert_eq!(format_uptime(3661), "1h 1m");
        assert_eq!(format_uptime(86400), "1d 0h 0m");
        assert_eq!(format_uptime(90061), "1d 1h 1m");
    }
}
