//! Pavilion's OS-integration layer.
//!
//! Modules in this layer talk to platform APIs (Win32 / Cloud Filter /
//! WinRT) rather than to the Zen Garden domain. They're Windows-only by
//! definition (Pavilion as a whole is Windows-only per PAVILION-0001).

pub mod cloud_filter;
