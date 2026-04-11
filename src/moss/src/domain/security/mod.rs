//! Security domain — trust, pond, and inter-stone TLS (ARCH-0004).

pub mod ceremony;
pub mod pond;
pub mod pond_lifecycle;

pub use ceremony::Ceremony;
pub use pond::Pond;

use crate::domain::traits::PondClient;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// Security domain context (`state.security`).
///
/// Generic over the inter-stone HTTP client. Defaults to `StoneClient`
/// (the sole production implementation). Tests can substitute a mock
/// via the type parameter. See ARCH-0007 §D for the migration pattern.
#[derive(Clone)]
pub struct Security<P: PondClient = crate::infra::stone_client::StoneClient> {
    /// Pond trust domain — enrollment, CA lifecycle, ceremonies.
    pub pond: Pond,

    /// Stone-to-stone HTTP client gateway.
    /// Automatically upgrades to HTTPS+mTLS when pond certs are available.
    /// Call `stone_client.reload_tls()` after enrollment changes.
    pub stone_client: Arc<P>,

    /// HTTPS listener started guard — prevents double-binding :7183.
    /// Set true after the first successful HTTPS bind (boot or dynamic).
    pub https: Arc<AtomicBool>,
}
