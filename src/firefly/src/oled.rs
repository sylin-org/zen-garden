//! Shared helpers for Firefly OLED display

use anyhow::Result;

use crate::serial::FireflyConnection;

/// Default health label when none is available
pub const DEFAULT_HEALTH_LABEL: &str = "thriving";

/// Format uptime seconds into human-readable string (e.g., "1h", "3d", "2m")
pub fn format_uptime(seconds: u64) -> String {
    if seconds < 60 {
        format!("{}s", seconds)
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else if seconds < 86400 {
        format!("{}h", seconds / 3600)
    } else {
        format!("{}d", seconds / 86400)
    }
}

/// Send a full OLED snapshot (name, health, metrics)
pub fn send_oled_snapshot(
    connection: &FireflyConnection,
    stone_name: &str,
    health: &str,
    cpu: u8,
    memory: u8,
    uptime_seconds: u64,
) -> Result<()> {
    let uptime = format_uptime(uptime_seconds);
    connection.with_device(|serial| {
        serial.oled_stone_name(stone_name)?;
        serial.oled_health(health)?;
        serial.oled_metrics(cpu, memory, &uptime)?;
        Ok(())
    })
}
