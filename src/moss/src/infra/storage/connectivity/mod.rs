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
//! Linux is the only target where these modules do real work — the
//! `/sys/block`, `/sys/bus/usb` paths they read and write are Linux-
//! specific. On other platforms the production functions still
//! compile (they use plain `std::fs`), but they read paths that don't
//! exist and produce verdicts based on the absence of signals
//! (typically `EmptyEnclosure`). Pipeline orchestration on non-Linux
//! should skip recovery entirely; the modules are exposed
//! cross-platform mainly so the type system catches drift across
//! shared types.
//!
//! Tests that require synthetic sysfs trees with symlinks are
//! Linux-gated since `std::os::unix::fs::symlink` is the cleanest
//! cross-FS way to fake the topology.

pub mod probe;
pub mod recovery;
