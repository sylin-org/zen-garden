//! Infrastructure layer - I/O operations
//!
//! This layer contains all external I/O:
//! - Communications (UDP P2P, mDNS)
//! - Container runtime (Podman/Docker)
//! - File system operations
//! - Platform-specific utilities
//! - API response helpers
//! - Archive operations (centralized compression/checksum)
//! - Harvest storage
//! - Ceremony journal (crash recovery)
//! - Companion registry (external Companions like Cricket)
//!
//! No business logic here - pure I/O Companions.

pub mod api_helpers;
pub mod ceremony_journal;
pub mod cross_stone;
// Cloud Filter integration moved to garden-pavilion per PAVILION-0001.
pub mod companions;
pub mod config;
pub mod detection;
pub mod docker_config;
pub mod docker_config_adapter;
pub mod embedded;
pub mod event_bus;
pub mod filesystem;
pub mod firmware;
pub mod hardware;
pub mod hardware_id;
pub mod harvest;
pub mod harvest_store;
pub mod image_inspect;
pub mod installer;
pub mod listeners;
pub mod log_broadcast;
pub mod maintenance_store;
pub mod manifests;
pub mod network;
pub mod nurturing_store;
pub mod persistence;
pub mod platform;
pub mod privilege;
pub mod process;
pub mod registry_client;
pub mod secrets;
pub mod service;
#[cfg(target_os = "windows")]
pub mod shell_integration;
pub mod plant;
pub mod snapshot;
pub mod snapshot_scheduler;
pub mod stone_client;
pub mod storage;
pub mod task_store;
pub mod tools;
pub mod topology;
#[cfg(target_os = "windows")]
pub mod update_transaction;

pub use api_helpers::{error_response, require_docker};
pub use ceremony_journal::CeremonyJournal;
pub use companions::CompanionRegistry;
pub use config::{AdoptionConfig, MossConfig, NetworkConfig, StaticIpPoolConfig};
pub use docker_config_adapter::OsDockerConfig;
pub use embedded::{
    AssetSource, EmbeddedCompanions, EmbeddedManifests, EmbeddedSeeds, ManifestSource,
    extract_seeds, list_all_manifests, load_embedded_adopted_offerings,
    load_sw_manifests_with_overlay, manifest_exists, read_manifest_overlay,
};
pub use event_bus::{EventBus, EventListener, spawn_listener};
pub use filesystem::FileSystem;
pub use garden_common::infra::archive::{
    ArchiveInfo, Archiver, calculate_checksum, create_archive, extract_archive, verify_checksum,
};
pub use garden_common::infra::network::get_local_ip;
pub use garden_common::infra::platform::{is_running_from_removable_media, shutdown_signal};
pub use hardware::{create_skeleton, load_cached_capabilities, save_capabilities_cache};
#[cfg(target_os = "windows")]
pub use hardware_id::is_first_run_windows;
pub use hardware_id::{generate_hardware_id, load_cached_hardware_id, save_hardware_id_cache};
pub use hardware_id::{load_cached_stone_name, save_stone_name_cache};
pub use harvest::{create_harvest, restore_harvest, verify_harvest};
pub use harvest_store::HarvestStore;
pub use listeners::{
    ChirpListener, DomainPulse, PulseDomainBridge, PulseEvent, TransportPulse, spawn_transport_tap,
};
pub use manifests::{
    AdoptedConfig, BorrowedConfig, HwEntry, HwManifests, ManagedConfig, ManifestRegistry, Offering,
    OfferingMetadata, OfferingRegistry,
};
pub use manifests::{RUNTIME_HW_MANIFESTS_DIR, RUNTIME_MANIFESTS_DIR, runtime_manifests_dir};
pub use network::{
    NetworkPlatform, ProbeConfig, StaticIpApply, apply_static_from_pool, detect_platform,
    load_network_state, probe_ip_conflict, revert_to_dhcp, save_network_state, select_ip_from_pool,
};
pub use nurturing_store::NurturingStore;
pub use persistence::{
    load_offerings, load_offerings_cache, load_or_generate_stone_id, save_offerings,
    save_offerings_cache,
};
pub use process::{
    check_moss_processes_exist, kill_existing_moss_processes, kill_existing_moss_processes_graceful,
};
pub use secrets::{Vault, VaultError};
#[cfg(target_os = "windows")]
pub use service::{
    cleanup_after_service_update, cleanup_updater_process, finalize_service_update,
    install_windows_service, spawn_windows_updater,
};
pub use task_store::{TaskRegistry, TaskStore};
pub use tools::broadcast_tools_beacon;
pub use tools::broadcast_tools_snapshot_beacon;
