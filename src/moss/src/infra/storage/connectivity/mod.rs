//! Storage connectivity health (STORAGE-0019).
//!
//! The pipeline stage that sits between the storage listener and the
//! candidate classifier. Reads sysfs to decide whether a freshly-detected
//! block device is in a recoverable degraded state, attempts recovery
//! (SCSI rescan / USB re-authorization), and forwards the device with a
//! [`ConnectivityStatus`](garden_common::storage::ConnectivityStatus)
//! companion describing what happened.
//!
//! This module is **pre-adoption only**. STORAGE-0018 covers the
//! post-adoption observe loop on `Volume`s already in the managed map;
//! this module covers the moment between hotplug detection and the
//! user seeing a candidate in `garden-rake storage add`.
//!
//! See [STORAGE-0019](../../../../../../docs/decisions/STORAGE-0019-candidate-lifecycle-and-foreign-filesystem-adoption.md)
//! §"Pipeline architecture" for the full design.
//!
//! ## Platform support
//!
//! Linux only. Windows manages USB device lifecycle through different
//! mechanisms (Plug and Play subsystem, no /sys equivalent), so the
//! `probe` and `recovery` submodules don't compile on non-Linux. The
//! pipeline orchestration in `mod.rs` dispatches to a no-op shim on
//! non-Linux so the rest of the storage stack stays cross-platform.

#[cfg(target_os = "linux")]
pub mod probe;
