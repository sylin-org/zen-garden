//! Firefly adapters.
//!
//! Each device type (RP2040 matrix, OLED v1/v2, T-Display) gets its
//! own [`Adapter`] implementation under this module. Two integration
//! paths are supported:
//!
//! - **Legacy factory** — [`MatrixFactory`] / [`OledV1Factory`] /
//!   [`OledV2Factory`] / [`TDisplayFactory`] scan USB, probe, and
//!   produce adapters via `AdapterFactory::discover`. Kept only so
//!   existing tests still compile; no longer used by `main.rs`.
//!
//! - **Device bus** — [`bus_registrations`] returns the four
//!   [`AdapterRegistration`] values `main.rs` hands to the SDK's
//!   [`DeviceBus`]. The bus opens the port, runs the
//!   [`crate::identity::FireflyIdentityProtocol`], picks the
//!   specificity-winning registration, and invokes its builder.
//!
//! [`Adapter`]: garden_companion_sdk::adapters::Adapter
//! [`AdapterRegistration`]: garden_companion_sdk::bus::AdapterRegistration
//! [`DeviceBus`]: garden_companion_sdk::bus::DeviceBus

pub mod matrix;
pub mod oled_v1;
pub mod oled_v2;
pub mod tdisplay;

pub use matrix::MatrixAdapter;
pub use oled_v1::OledV1Adapter;
pub use oled_v2::OledV2Adapter;
pub use tdisplay::TDisplayAdapter;
// Legacy factories still exported for test harnesses; the live entry
// for production runs is [`bus_registrations`] below.
#[allow(unused_imports)]
pub use matrix::MatrixFactory;
#[allow(unused_imports)]
pub use oled_v1::OledV1Factory;
#[allow(unused_imports)]
pub use oled_v2::OledV2Factory;
#[allow(unused_imports)]
pub use tdisplay::TDisplayFactory;

use crate::serial::{FireflyConnection, FireflyDeviceType, FireflySerial};
use garden_companion_sdk::adapters::Adapter;
use garden_companion_sdk::bus::{
    AdapterRegistration, Identification, OpenedDevice, Predicate, ResourceClass,
};
use std::sync::Arc;

/// Extract the opened serial port from the bus-provided `OpenedDevice`
/// and wrap it into an `Arc<FireflyConnection>` ready for the adapter
/// to drive. Returns `None` on non-USB-serial devices (would be a bus
/// configuration error in Phase 1).
fn adopt_connection(
    opened: OpenedDevice,
    device_type: FireflyDeviceType,
) -> Option<(Arc<FireflyConnection>, String)> {
    let port_name = opened.device.handle.to_string();
    let port = opened.as_usb_serial()?;
    let serial = FireflySerial::adopt(port, device_type, port_name.clone());
    Some((Arc::new(FireflyConnection::from_serial(serial)), port_name))
}

fn matrix_registration(state_dir: Option<std::path::PathBuf>) -> AdapterRegistration {
    AdapterRegistration::new(
        "firefly.matrix",
        ResourceClass::UsbSerial {
            vid: None,
            pid: None,
        },
        Predicate::AllOf(vec![
            Predicate::eq("family", "firefly"),
            Predicate::eq("variant", "matrix"),
        ]),
        move |opened: OpenedDevice, _id: &Identification| -> Box<dyn Adapter> {
            let state_dir = state_dir.clone();
            match adopt_connection(opened, FireflyDeviceType::Rp2040Matrix) {
                Some((conn, port)) => {
                    Box::new(MatrixAdapter::from_connection(conn, port, state_dir))
                }
                None => Box::new(MatrixAdapter::new(String::from("<unknown>"), state_dir)),
            }
        },
    )
}

fn oled_v1_registration() -> AdapterRegistration {
    AdapterRegistration::new(
        "firefly.oled-v1",
        ResourceClass::UsbSerial {
            vid: None,
            pid: None,
        },
        // Specificity: 2 (family + variant). OLED v2 also matches
        // variant=oled but adds a capability predicate and wins on
        // score.
        Predicate::AllOf(vec![
            Predicate::eq("family", "firefly"),
            Predicate::eq("variant", "oled"),
        ]),
        |opened: OpenedDevice, _id: &Identification| -> Box<dyn Adapter> {
            match adopt_connection(opened, FireflyDeviceType::Esp8266Oled) {
                Some((conn, port)) => Box::new(OledV1Adapter::from_connection(conn, port)),
                None => Box::new(OledV1Adapter::new(String::from("<unknown>"))),
            }
        },
    )
}

fn oled_v2_registration() -> AdapterRegistration {
    AdapterRegistration::new(
        "firefly.oled-v2",
        ResourceClass::UsbSerial {
            vid: None,
            pid: None,
        },
        // Specificity: 3 (family + variant + dashboard capability).
        // Outranks OLED v1 when both are registered.
        Predicate::AllOf(vec![
            Predicate::eq("family", "firefly"),
            Predicate::eq("variant", "oled"),
            Predicate::has_capability("dashboard"),
        ]),
        |opened: OpenedDevice, _id: &Identification| -> Box<dyn Adapter> {
            match adopt_connection(opened, FireflyDeviceType::Esp8266OledV2) {
                Some((conn, port)) => Box::new(OledV2Adapter::from_connection(conn, port)),
                None => Box::new(OledV2Adapter::new(String::from("<unknown>"))),
            }
        },
    )
}

fn tdisplay_registration() -> AdapterRegistration {
    AdapterRegistration::new(
        "firefly.tdisplay",
        ResourceClass::UsbSerial {
            vid: None,
            pid: None,
        },
        Predicate::AllOf(vec![
            Predicate::eq("family", "firefly"),
            Predicate::eq("variant", "tdisplay"),
        ]),
        |opened: OpenedDevice, _id: &Identification| -> Box<dyn Adapter> {
            match adopt_connection(opened, FireflyDeviceType::Esp32TDisplay) {
                Some((conn, port)) => Box::new(TDisplayAdapter::from_connection(conn, port)),
                None => Box::new(TDisplayAdapter::new(String::from("<unknown>"))),
            }
        },
    )
}

/// All four firefly adapter registrations, in the order the bus
/// evaluates them. OLED v2 is registered before OLED v1 so the
/// specificity-tie-break (earlier registration wins) favours v2 if a
/// future firmware forgot to advertise the dashboard capability.
pub fn bus_registrations(state_dir: Option<std::path::PathBuf>) -> Vec<AdapterRegistration> {
    vec![
        matrix_registration(state_dir),
        oled_v2_registration(),
        oled_v1_registration(),
        tdisplay_registration(),
    ]
}
