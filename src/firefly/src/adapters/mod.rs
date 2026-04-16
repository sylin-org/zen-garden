//! Firefly adapters — one [`Adapter`](garden_companion_sdk::adapters::Adapter)
//! per physical firefly variant. Each takes an `Arc<Firefly>` from
//! [`crate::orchestrator::FireflyOrchestrator`] and drives the device
//! using the firefly-protocol vocabulary exposed by
//! [`crate::firefly::Firefly`].

pub mod matrix;
pub mod oled_v1;
pub mod oled_v2;
pub mod tdisplay;

pub use matrix::MatrixAdapter;
pub use oled_v1::OledV1Adapter;
pub use oled_v2::OledV2Adapter;
pub use tdisplay::TDisplayAdapter;
