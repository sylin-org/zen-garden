//! Firefly adapters.
//!
//! Each device type (RP2040 matrix, OLED v1/v2, T-Display) gets its own
//! [`Adapter`] implementation. Factories scan USB on every discovery
//! tick; the supervisor dedupes by `(kind, id)` so a plugged device
//! spawns once and an unplugged one is reaped after the grace window.
//!
//! Ch1 ships only [`matrix::MatrixFactory`]; Ch2-4 add the other three.
//!
//! [`Adapter`]: garden_companion_sdk::adapters::Adapter

pub mod matrix;

pub use matrix::MatrixFactory;
