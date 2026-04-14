//! Device bus — plug-and-play hardware discovery for companions.
//!
//! See [COMPANION-0012] for the architecture. The bus owns physical-resource
//! enumeration; adapters declare interest as data; the bus probes each new
//! device exactly once and offers it to interested adapters by descriptor.
//!
//! # Layers
//!
//! 1. [`ResourceClass`] — what kind of physical thing the bus enumerates.
//! 2. [`IdentityProtocol`] — bridges an opened device to a structured
//!    [`Identification`]. Registered per ecosystem.
//! 3. [`AdapterRegistration`] — pure data + a builder fn; no probe code.
//!
//! The [`DeviceBus`] runtime ties these together.
//!
//! [COMPANION-0012]: https://github.com/zen-garden/zen-garden/blob/dev/docs/decisions/COMPANION-0012-device-bus.md

pub mod backoff;
pub mod cache;
pub mod claim;
pub mod descriptor;
pub mod device;
pub mod identity;
pub mod predicate;
pub mod registration;
pub mod resource;
pub mod runtime;
pub mod telemetry;
pub mod usb_serial;

pub use backoff::BackoffTracker;
pub use cache::DeviceCache;
pub use claim::{ClaimOutcome, pick_winner};
pub use descriptor::Identification;
pub use device::{Device, DeviceHandle, OpenedDevice};
pub use identity::{IdentifyError, IdentifyResult, IdentityProtocol};
pub use predicate::Predicate;
pub use registration::{AdapterBuilder, AdapterRegistration};
pub use resource::ResourceClass;
pub use runtime::{DeviceBus, DeviceBusBuilder, DEFAULT_OPEN_STABILIZATION, DEFAULT_SCAN_INTERVAL};
pub use telemetry::{DeviceForeign, DeviceUnclaimed, DeviceUnprovisioned};
pub use usb_serial::{ScanDelta, UsbSerialEnumerator, UsbSerialPort};
