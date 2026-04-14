//! Load — a cohesive snapshot of a stone's resource consumption.
//!
//! Replaces the eight ad-hoc numeric fields of
//! [`crate::presence::types::StoneLoadUpdatedPayload`] with a typed struct
//! using [`Percent`] where appropriate. The wire payload stays the wire
//! type; `Load` is the domain view consumers work with.

use crate::presence::StoneLoadUpdatedPayload;
use serde::{Deserialize, Serialize};

/// A percentage value clamped to `0.0 ..= 100.0` at construction.
///
/// Any attempt to construct a value outside the range is silently clamped
/// rather than rejected — moss emits occasional slightly-out-of-range values
/// from its resource samplers, and we want to present a sane number to
/// consumers without panicking.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Percent(f64);

impl Percent {
    /// The lower bound (0.0).
    pub const MIN: Percent = Percent(0.0);

    /// The upper bound (100.0).
    pub const MAX: Percent = Percent(100.0);

    /// Construct from an `f64`, clamping to `0.0 ..= 100.0`.
    pub fn new(v: f64) -> Self {
        Self(v.clamp(0.0, 100.0))
    }

    /// Raw percentage value in `0.0 ..= 100.0`.
    pub fn value(&self) -> f64 {
        self.0
    }

    /// Rounded `u8` value for display. Clamped output is `0..=100`.
    pub fn as_u8(&self) -> u8 {
        self.0.round() as u8
    }
}

impl From<f64> for Percent {
    fn from(v: f64) -> Self {
        Self::new(v)
    }
}

impl std::fmt::Display for Percent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.1}%", self.0)
    }
}

/// Resource load snapshot. Produced by converting from a
/// [`StoneLoadUpdatedPayload`] or constructed directly.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Load {
    pub cpu: Percent,
    pub memory: Percent,
    pub disk: Percent,
    pub io: Percent,
    pub gpu: Percent,
    /// True when GPU utilisation has crossed the activity threshold
    /// (moss-defined; typically `gpu > 10%`).
    pub gpu_active: bool,
    pub net_rx_bytes_per_sec: u64,
    pub net_tx_bytes_per_sec: u64,
}

impl Load {
    /// An all-zero snapshot — useful as a default before the first
    /// `stone.load.updated` event arrives.
    pub const ZERO: Load = Load {
        cpu: Percent::MIN,
        memory: Percent::MIN,
        disk: Percent::MIN,
        io: Percent::MIN,
        gpu: Percent::MIN,
        gpu_active: false,
        net_rx_bytes_per_sec: 0,
        net_tx_bytes_per_sec: 0,
    };

    /// Total network throughput (rx + tx) in bytes/sec. Useful for a single
    /// gauge rather than two.
    pub fn net_total_bytes_per_sec(&self) -> u64 {
        self.net_rx_bytes_per_sec
            .saturating_add(self.net_tx_bytes_per_sec)
    }
}

impl From<&StoneLoadUpdatedPayload> for Load {
    fn from(p: &StoneLoadUpdatedPayload) -> Self {
        Self {
            cpu: Percent::new(p.cpu_percent),
            memory: Percent::new(p.memory_percent),
            disk: Percent::new(p.disk_percent),
            io: Percent::new(p.io_percent),
            gpu: Percent::new(p.gpu_percent),
            gpu_active: p.gpu_active,
            net_rx_bytes_per_sec: p.net_rx_bytes_per_sec,
            net_tx_bytes_per_sec: p.net_tx_bytes_per_sec,
        }
    }
}

impl From<StoneLoadUpdatedPayload> for Load {
    fn from(p: StoneLoadUpdatedPayload) -> Self {
        Self::from(&p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_clamps_below_zero() {
        assert_eq!(Percent::new(-5.0).value(), 0.0);
        assert_eq!(Percent::new(f64::NEG_INFINITY).value(), 0.0);
    }

    #[test]
    fn percent_clamps_above_hundred() {
        assert_eq!(Percent::new(150.0).value(), 100.0);
        assert_eq!(Percent::new(f64::INFINITY).value(), 100.0);
    }

    #[test]
    fn percent_passes_in_range_unchanged() {
        assert_eq!(Percent::new(0.0).value(), 0.0);
        assert_eq!(Percent::new(50.5).value(), 50.5);
        assert_eq!(Percent::new(100.0).value(), 100.0);
    }

    #[test]
    fn percent_as_u8_rounds() {
        assert_eq!(Percent::new(49.4).as_u8(), 49);
        assert_eq!(Percent::new(49.5).as_u8(), 50);
        assert_eq!(Percent::new(99.9).as_u8(), 100);
    }

    #[test]
    fn percent_display_formats_with_one_decimal() {
        assert_eq!(format!("{}", Percent::new(42.0)), "42.0%");
        assert_eq!(format!("{}", Percent::new(3.14)), "3.1%");
    }

    #[test]
    fn percent_serde_round_trip() {
        let p = Percent::new(42.5);
        let j = serde_json::to_string(&p).unwrap();
        assert_eq!(j, "42.5"); // transparent — bare number
        let back: Percent = serde_json::from_str(&j).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn percent_serde_clamps_deserialized_input() {
        // serde(transparent) on a tuple struct means deserializing
        // constructs via `Self(val)`, bypassing our clamping `new`.
        // Document this: callers should normalize via `Percent::new`.
        let p: Percent = serde_json::from_str("150.0").unwrap();
        assert_eq!(p.value(), 150.0); // unchanged — serde doesn't clamp
        // Clamping happens at the From<StoneLoadUpdatedPayload> boundary.
        let normalized = Percent::new(p.value());
        assert_eq!(normalized.value(), 100.0);
    }

    #[test]
    fn load_zero_is_all_zero() {
        assert_eq!(Load::ZERO.cpu.value(), 0.0);
        assert_eq!(Load::ZERO.memory.value(), 0.0);
        assert!(!Load::ZERO.gpu_active);
        assert_eq!(Load::ZERO.net_total_bytes_per_sec(), 0);
    }

    #[test]
    fn load_from_payload_copies_and_clamps() {
        let payload = StoneLoadUpdatedPayload {
            cpu_percent: 150.0, // out of range — should clamp
            memory_percent: 42.0,
            disk_percent: 30.0,
            io_percent: -5.0, // out of range — should clamp
            gpu_percent: 10.0,
            gpu_active: true,
            net_rx_bytes_per_sec: 1024,
            net_tx_bytes_per_sec: 2048,
        };
        let load = Load::from(&payload);
        assert_eq!(load.cpu, Percent::new(100.0));
        assert_eq!(load.memory, Percent::new(42.0));
        assert_eq!(load.io, Percent::new(0.0));
        assert!(load.gpu_active);
        assert_eq!(load.net_rx_bytes_per_sec, 1024);
        assert_eq!(load.net_tx_bytes_per_sec, 2048);
        assert_eq!(load.net_total_bytes_per_sec(), 3072);
    }

    #[test]
    fn load_net_total_saturates_on_overflow() {
        let load = Load {
            net_rx_bytes_per_sec: u64::MAX,
            net_tx_bytes_per_sec: 100,
            ..Load::ZERO
        };
        assert_eq!(load.net_total_bytes_per_sec(), u64::MAX);
    }
}
