//! Discovery domain — mDNS and Koi embedded service handles.

use std::sync::Arc;
use crate::mdns::MdnsHandle;

/// Discovery domain context (`state.discovery`).
///
/// Owns the mDNS re-registration handle and the Koi embedded handle
/// (mDNS, DNS, certmesh, proxy, health sub-handles).
#[derive(Clone)]
pub struct Discovery {
    /// mDNS handle for re-registration on IP/MAC resolution changes.
    pub mdns: Option<Arc<MdnsHandle>>,

    /// Koi embedded handle — mDNS, DNS, certmesh, proxy, health capabilities.
    /// Sub-handles accessed via `koi.mdns()`, `.dns()`, `.certmesh()`, etc.
    pub koi: Arc<koi_embedded::KoiHandle>,
}
