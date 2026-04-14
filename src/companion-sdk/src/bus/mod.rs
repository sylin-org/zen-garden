//! Device bus — plug-and-play hardware discovery for companions.
//!
//! See [COMPANION-0012] for the architecture. The bus owns physical-resource
//! enumeration; adapters declare interest as data; the bus probes each new
//! device exactly once and offers it to interested adapters by descriptor.
//!
//! # Layers
//!
//! 1. [`ResourceClass`] — what kind of physical thing (USB serial today;
//!    Bluetooth / network / GPIO when the first consumer needs them).
//! 2. [`IdentityProtocol`] — bridges an opened device to a structured
//!    [`Identification`]. Registered per ecosystem (e.g. firefly).
//! 3. `AdapterRegistration` — pure data + a builder fn. No probe code.
//!    Lands in Ch2 alongside the predicate engine and claim mechanics.
//!
//! # Phase 1 scope
//!
//! Ch1 ships the bus *core types* and the `UsbSerial` enumerator. No
//! adapter integration yet — that lands in Ch2 with [`AdapterRegistration`]
//! and [`MockBus`].
//!
//! [COMPANION-0012]: https://github.com/zen-garden/zen-garden/blob/dev/docs/decisions/COMPANION-0012-device-bus.md

pub mod descriptor;
pub mod device;
pub mod identity;
pub mod resource;
pub mod usb_serial;

pub use descriptor::Identification;
pub use device::{Device, DeviceHandle, OpenedDevice};
pub use identity::IdentityProtocol;
pub use resource::ResourceClass;
pub use usb_serial::{UsbSerialEnumerator, UsbSerialPort};
