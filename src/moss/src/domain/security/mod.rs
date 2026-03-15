//! Security domain — trust, pond, and inter-stone TLS (ARCH-0004).

pub mod ceremony;
pub mod pond;

pub use ceremony::Ceremony;
pub use pond::Pond;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use crate::infra::stone_client::StoneClient;

/// Security domain context (`state.security`).
#[derive(Clone)]
pub struct Security {
    /// Pond trust domain — enrollment, CA lifecycle, ceremonies.
    pub pond: Pond,

    /// Stone-to-stone HTTP client gateway.
    /// Automatically upgrades to HTTPS+mTLS when pond certs are available.
    /// Call `stone_client.reload_tls()` after enrollment changes.
    pub stone_client: Arc<StoneClient>,

    /// HTTPS listener started guard — prevents double-binding :7183.
    /// Set true after the first successful HTTPS bind (boot or dynamic).
    pub https: Arc<AtomicBool>,
}
