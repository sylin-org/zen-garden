//! Shared helpers for Firefly ESP32 T-Display (FIREFLY-0003)
//!
//! Builds compact JSON payloads from presence data and sends them
//! via the `J,<json>` serial command.

use anyhow::Result;
use serde::Serialize;

use crate::events::PresenceSnapshot;
use crate::serial::FireflyConnection;

/// Compact JSON payload for T-Display full state push.
/// Field names are single-char to minimize serial transfer time.
#[derive(Serialize)]
struct TDisplayState {
    /// Stone name
    n: String,
    /// Health label (thriving / withering / wilting)
    h: String,
    /// CPU percent (0-100)
    c: u8,
    /// Memory percent (0-100)
    m: u8,
    /// Disk percent (0-100)
    d: u8,
    /// I/O percent (0-100)
    i: u8,
    /// GPU percent (0-100)
    g: u8,
    /// GPU active (1/0)
    ga: u8,
    /// Uptime in seconds
    up: u64,
    /// Number of active offerings (services)
    sv: usize,
    /// Has GPU capability
    hg: u8,
    /// Is Lantern present
    il: u8,
    /// Has Cricket companion
    hc: u8,
    /// Pond active
    pa: u8,
    /// Current hour (0-23)
    hr: u8,
    /// Network RX bytes/sec
    #[serde(skip_serializing_if = "is_zero_u64")]
    rx: u64,
    /// Network TX bytes/sec
    #[serde(skip_serializing_if = "is_zero_u64")]
    tx: u64,
    /// Seed bank info (name, used_gb, total_gb) - None if no seed bank
    #[serde(skip_serializing_if = "Option::is_none")]
    sb: Option<SeedBankCompact>,
    /// Offerings list (up to 4 entries with name + health)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    of: Vec<OfferingCompact>,
}

#[derive(Serialize)]
struct SeedBankCompact {
    n: String,
    u: u64,
    t: u64,
}

/// Compact offering entry: name + health initial
#[derive(Serialize)]
struct OfferingCompact {
    /// Short service name
    n: String,
    /// Health: "h"=healthy, "w"=warning, "d"=degraded
    h: String,
}

fn is_zero_u64(v: &u64) -> bool {
    *v == 0
}

/// Send a full presence snapshot to the T-Display as compact JSON.
///
/// This is called on initial connection and whenever a full PRESENCE_SNAPSHOT
/// event arrives. The T-Display firmware parses this to rebuild its entire
/// diorama state.
pub fn send_snapshot(connection: &FireflyConnection, snapshot: &PresenceSnapshot) -> Result<()> {
    let seed_bank = snapshot.stone.seed_bank.as_ref().map(|sb| SeedBankCompact {
        n: sb.name.clone(),
        u: sb.used_gb,
        t: sb.total_gb,
    });

    let offerings: Vec<OfferingCompact> = snapshot
        .offerings
        .iter()
        .take(4)
        .map(|o| OfferingCompact {
            n: o.name.clone(),
            h: match o.health.as_str() {
                "warning" => "w".into(),
                "degraded" | "unhealthy" => "d".into(),
                _ => "h".into(),
            },
        })
        .collect();

    let state = TDisplayState {
        n: snapshot.stone.name.clone(),
        h: snapshot.stone.health.clone(),
        c: snapshot.stone.cpu_percent as u8,
        m: snapshot.stone.memory_percent as u8,
        d: snapshot.stone.disk_percent as u8,
        i: snapshot.stone.io_percent as u8,
        g: snapshot.stone.gpu_percent as u8,
        ga: if snapshot.stone.gpu_active { 1 } else { 0 },
        up: snapshot.stone.uptime_seconds,
        sv: snapshot.offerings.len(),
        hg: if snapshot.stone.has_gpu { 1 } else { 0 },
        il: if snapshot.stone.is_lantern { 1 } else { 0 },
        hc: if snapshot.stone.has_cricket { 1 } else { 0 },
        pa: if snapshot.stone.pond_active { 1 } else { 0 },
        hr: snapshot.stone.hour as u8,
        rx: snapshot.stone.net_rx_bytes_per_sec,
        tx: snapshot.stone.net_tx_bytes_per_sec,
        sb: seed_bank,
        of: offerings,
    };

    let json = serde_json::to_string(&state)?;
    tracing::debug!(len = json.len(), "Sending T-Display JSON snapshot");

    connection.with_device(|serial| {
        serial.tdisplay_json_push(&json)?;
        Ok(())
    })
}
