//! Infrastructure layer - I/O operations
//!
//! This layer contains all external I/O:
//! - Communications (UDP P2P, mDNS)
//! - Container runtime (Podman/Docker)
//! - File system operations
//! - Authentication implementation (NoAuth for v0.1.0)
//! - Platform-specific utilities
//! - API response helpers
//! - Archive operations (centralized compression/checksum)
//! - Harvest storage
//! - Ceremony journal (crash recovery)
//! - Adapter registry (external adapters like Cricket)
//!
//! No business logic here - pure I/O adapters.

pub mod adapters;
pub mod api_helpers;
pub mod auth;
pub mod ceremony_journal;
pub mod config;
pub mod container;
pub mod detection;
pub mod embedded;
pub mod filesystem;
pub mod firmware;
pub mod hardware;
pub mod hardware_id;
pub mod harvest;
pub mod harvest_store;
pub mod manifests;
pub mod persistence;
pub mod process;
pub mod secrets;
pub mod service;
pub mod storage;
#[cfg(target_os = "windows")]
pub mod update_transaction;

pub use adapters::AdapterRegistry;
pub use api_helpers::{error_response, error_codes};
pub use auth::NoAuth;
pub use config::MossConfig;
pub use container::ContainerRuntime;
pub use garden_common::infra::network::get_local_ip;
pub use process::{kill_existing_moss_processes_graceful, check_moss_processes_exist, kill_existing_moss_processes};
#[cfg(target_os = "windows")]
pub use service::{install_windows_service, finalize_service_update, cleanup_after_service_update, spawn_windows_updater, cleanup_updater_process};
pub use filesystem::FileSystem;
pub use hardware::{detect_hardware, load_cached_capabilities, save_capabilities_cache, create_skeleton};
pub use hardware_id::{generate_hardware_id, load_cached_hardware_id, save_hardware_id_cache};
pub use manifests::{ManifestRegistry, SwManifests, SwEntry, HwManifests, HwEntry};
pub use manifests::{runtime_manifests_dir, RUNTIME_HW_MANIFESTS_DIR, RUNTIME_MANIFESTS_DIR};
pub use persistence::{load_registry, save_registry, save_registry_vec, load_offerings_cache, save_offerings_cache, load_or_generate_stone_id};
pub use garden_common::infra::platform::{is_running_from_removable_media, shutdown_signal};
pub use secrets::SecretsManager;
pub use garden_common::infra::archive::{Archiver, ArchiveInfo, create_archive, extract_archive, calculate_checksum, verify_checksum};
pub use ceremony_journal::CeremonyJournal;
pub use harvest_store::HarvestStore;
pub use harvest::{create_harvest, restore_harvest, verify_harvest};
pub use embedded::{
    EmbeddedManifests, EmbeddedAdapters, 
    read_manifest_overlay, manifest_exists, list_all_manifests,
    load_sw_manifests_with_overlay,
    AssetSource, ManifestSource,
};
