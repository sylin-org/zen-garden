//! Sync root registration lifecycle (STORAGE-0012)
//!
//! Manages the CfApi sync root registration via the `cloud-filter` crate's
//! WinRT-backed `SyncRootId::register()`.  State is tracked via the API
//! (`is_registered()`), not sentinel files.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cloud_filter::root::{
    HydrationType, PopulationType, SecurityId, SyncRootId, SyncRootIdBuilder, SyncRootInfo,
};
use tracing::{debug, info, warn};

// ============================================================================
// Constants
// ============================================================================

/// Provider name registered with Windows (part of the sync root ID).
const PROVIDER_NAME: &str = "ZenGarden";

/// Display name shown in Explorer's navigation pane.
const DISPLAY_NAME: &str = "Zen Garden";

/// Folder name under the user's home directory.
const SYNC_ROOT_FOLDER: &str = "Zen Garden";

/// Fallback icon resource if the exe path can't be resolved.
const ICON_RESOURCE_FALLBACK: &str = "%SystemRoot%\\system32\\imageres.dll,3";

/// Provider version string.  Bump to force re-registration on upgrade.
pub const PROVIDER_VERSION: &str = "2.2";

// ============================================================================
// Public API
// ============================================================================

/// Ensure the sync root is registered and the directory exists.
///
/// Idempotent: if the sync root is already registered and the directory
/// exists, this is a no-op.  Otherwise it creates the directory and
/// registers a fresh sync root.
pub async fn ensure_registered() -> Result<PathBuf> {
    let sync_root_path = default_sync_root_path()?;
    let sync_root_id = build_sync_root_id()?;
    let already_registered = sync_root_id.is_registered().unwrap_or(false);
    let dir_exists = sync_root_path.exists();

    // Check if existing registration matches our version.  If not, force
    // a clean re-registration to clear stale CfApi mini-filter state.
    let version_matches = if already_registered {
        match sync_root_id.info() {
            Ok(info) => {
                let registered_version = info.version();
                let matches = registered_version == PROVIDER_VERSION;
                if !matches {
                    info!(
                        registered = ?registered_version,
                        expected = PROVIDER_VERSION,
                        "version mismatch — forcing re-registration"
                    );
                }
                matches
            }
            Err(_) => false,
        }
    } else {
        false
    };

    if already_registered && dir_exists && version_matches {
        info!(
            path = %sync_root_path.display(),
            version = PROVIDER_VERSION,
            "sync root already registered — reusing"
        );
        return Ok(sync_root_path);
    }

    // Need fresh registration — nuke everything to clear stale state.
    if already_registered {
        info!("unregistering stale sync root");
        let _ = sync_root_id.unregister();
    }
    if dir_exists {
        debug!("removing sync root directory");
        let _ = tokio::fs::remove_dir_all(&sync_root_path).await;
    }

    tokio::fs::create_dir_all(&sync_root_path)
        .await
        .context("failed to create sync root directory")?;

    register(&sync_root_path, &sync_root_id)?;

    info!(path = %sync_root_path.display(), version = PROVIDER_VERSION, "sync root registered (fresh)");
    Ok(sync_root_path)
}

/// Unregister the sync root (for clean uninstall).
pub fn unregister() -> Result<()> {
    let sync_root_id = build_sync_root_id()?;
    if sync_root_id.is_registered().unwrap_or(false) {
        sync_root_id
            .unregister()
            .context("failed to unregister Cloud Filter sync root")?;
        info!("sync root unregistered");
    }
    Ok(())
}

// ============================================================================
// Internal helpers
// ============================================================================

/// Build the sync root ID for this provider + current user.
fn build_sync_root_id() -> Result<SyncRootId> {
    let sid = SecurityId::current_user().context("failed to get current user SID")?;
    debug!("building sync root ID for current user");
    Ok(SyncRootIdBuilder::new(PROVIDER_NAME)
        .user_security_id(sid)
        .build())
}

/// Register the sync root with Windows.
///
/// The directory must already exist.
fn register(path: &Path, sync_root_id: &SyncRootId) -> Result<()> {
    let icon = icon_resource();
    let info = SyncRootInfo::default()
        .with_display_name(DISPLAY_NAME)
        .with_icon(&icon)
        .with_version(PROVIDER_VERSION)
        .with_hydration_type(HydrationType::Full)
        .with_population_type(PopulationType::Full)
        .with_path(path)
        .context("failed to set sync root path")?;

    sync_root_id.register(info).map_err(|e| {
        warn!(
            error = %e,
            path = %path.display(),
            "sync root registration failed — is Windows Search (WSearch) running?"
        );
        anyhow::anyhow!("failed to register Cloud Filter sync root: {e}")
    })
}

/// Default sync root path: `%USERPROFILE%\Zen Garden\`.
fn default_sync_root_path() -> Result<PathBuf> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .context("could not determine user home directory")?;
    Ok(PathBuf::from(home).join(SYNC_ROOT_FOLDER))
}

/// Build the icon resource string: `{exe_path},1` (embedded resource ID 1).
fn icon_resource() -> String {
    match std::env::current_exe() {
        Ok(exe) => {
            let resource = format!("{},1", exe.display());
            debug!(icon = %resource, "using embedded icon");
            resource
        }
        Err(e) => {
            warn!(error = %e, "could not resolve exe path, using fallback icon");
            ICON_RESOURCE_FALLBACK.to_string()
        }
    }
}

// ============================================================================
// Process context diagnostics
// ============================================================================

/// Check if the current process is running elevated (admin).
pub fn is_elevated() -> bool {
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TokenElevation};

    // SAFETY: `-4isize as HANDLE` is the well-known pseudo-handle for the current process token
    // (GetCurrentProcessToken). `elevation` is a stack-allocated TOKEN_ELEVATION valid for the
    // duration of the call. `size` is initialized to the correct struct size as required by
    // GetTokenInformation.
    unsafe {
        let token: HANDLE = -4isize as HANDLE;
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut size = std::mem::size_of::<TOKEN_ELEVATION>() as u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut _,
            size,
            &mut size,
        );
        ok != 0 && elevation.TokenIsElevated != 0
    }
}

/// Heuristic: detect if running as a Windows service.
pub fn is_running_as_service() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    // SAFETY: GetForegroundWindow takes no arguments and is always safe to call.
    let has_console = unsafe { GetForegroundWindow() } != 0;
    let username = std::env::var("USERNAME").unwrap_or_default();
    let is_system = username.eq_ignore_ascii_case("SYSTEM")
        || username.eq_ignore_ascii_case("LOCAL SERVICE")
        || username.eq_ignore_ascii_case("NETWORK SERVICE");

    !has_console && is_system
}
