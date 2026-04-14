//! [`AdapterRegistration`] — pure data + a builder fn.
//!
//! An adapter registration tells the bus:
//!
//! 1. **What resource class** the adapter wants to consume (USB
//!    serial; later Bluetooth / mDNS / GPIO).
//! 2. **What descriptor shape** it claims — a [`Predicate`] that the
//!    bus evaluates against the parsed [`Identification`].
//! 3. **How to build** an instance once a device claims. The build
//!    fn receives the open port and the descriptor and returns a
//!    `Box<dyn Adapter>` that the supervisor spawns.
//!
//! There is no probe code in a registration. The bus does all the I/O;
//! adapters stay pure.

use super::descriptor::Identification;
use super::device::OpenedDevice;
use super::predicate::Predicate;
use super::resource::ResourceClass;
use crate::adapters::Adapter;
use std::sync::Arc;

/// Builder function: given an opened device and its descriptor,
/// construct the adapter instance. Returns `Box<dyn Adapter>` so the
/// supervisor can spawn it through the existing spawn path.
pub type AdapterBuilder =
    Arc<dyn Fn(OpenedDevice, &Identification) -> Box<dyn Adapter> + Send + Sync>;

/// A registration binds a resource class + a descriptor predicate +
/// a builder function into a single value the bus matches against
/// each discovered device.
#[derive(Clone)]
pub struct AdapterRegistration {
    /// Human-readable registration name (used in telemetry and logs,
    /// typically matches the adapter's `info.kind`).
    pub name: &'static str,

    /// Which enumerator's devices this registration is interested in.
    pub resource: ResourceClass,

    /// Descriptor predicate. Evaluated against the parsed
    /// [`Identification`]; a `Some(score)` return makes this
    /// registration a candidate for the device.
    pub interest: Predicate,

    /// Builder function invoked when this registration wins the claim.
    pub build: AdapterBuilder,
}

impl AdapterRegistration {
    /// Helper to construct a registration from parts. The builder fn is
    /// wrapped in an `Arc` so the same registration value can be cloned
    /// cheaply (e.g. passed to multiple bus mount points in tests).
    pub fn new<F>(
        name: &'static str,
        resource: ResourceClass,
        interest: Predicate,
        build: F,
    ) -> Self
    where
        F: Fn(OpenedDevice, &Identification) -> Box<dyn Adapter> + Send + Sync + 'static,
    {
        Self {
            name,
            resource,
            interest,
            build: Arc::new(build),
        }
    }

    /// Evaluate the registration's predicate against an identification.
    /// Returns `Some(score)` on match — score is the specificity used
    /// by the claim engine.
    pub fn score(&self, id: &Identification) -> Option<u32> {
        self.interest.eval(id)
    }
}

impl std::fmt::Debug for AdapterRegistration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdapterRegistration")
            .field("name", &self.name)
            .field("resource", &self.resource)
            .field("interest", &self.interest)
            .field("build", &"<fn>")
            .finish()
    }
}
