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
//!
//! No business logic here - pure I/O adapters.

pub mod api_helpers;
pub mod archive;
pub mod auth;
pub mod ceremony_journal;
pub mod config;
pub mod container;
pub mod detection;
pub mod filesystem;
pub mod firmware;
pub mod hardware;
pub mod harvest;
pub mod harvest_store;
pub mod manifests;
pub mod network;
pub mod persistence;
pub mod platform;
pub mod process;
pub mod registry;
pub mod secrets;
pub mod service;

pub use api_helpers::{error_response, error_codes};
pub use auth::NoAuth;
pub use config::MossConfig;
pub use container::ContainerRuntime;
pub use network::get_local_ip;
pub use process::{kill_existing_moss_processes_graceful, check_moss_processes_exist, kill_existing_moss_processes};
#[cfg(target_os = "windows")]
pub use service::{install_windows_service, finalize_service_update, cleanup_after_service_update};
pub use filesystem::FileSystem;
pub use hardware::{detect_hardware, load_cached_capabilities, save_capabilities_cache, create_skeleton};
pub use manifests::{ManifestRegistry, SwManifests, SwEntry, HwManifests, HwEntry};
pub use manifests::{RUNTIME_TEMPLATES_DIR, RUNTIME_HW_MANIFESTS_DIR, RUNTIME_MANIFESTS_DIR};
pub use persistence::{load_registry, save_registry, save_registry_vec, load_offerings_cache, save_offerings_cache, load_or_generate_stone_id};
pub use platform::{is_running_from_removable_media, shutdown_signal};
pub use secrets::SecretsManager;
pub use archive::{Archiver, ArchiveInfo, create_archive, extract_archive, calculate_checksum, verify_checksum};
pub use ceremony_journal::CeremonyJournal;
pub use harvest_store::HarvestStore;
pub use harvest::{create_harvest, restore_harvest, verify_harvest};
