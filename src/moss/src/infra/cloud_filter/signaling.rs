//! Explorer integration signals — info bar and toast notifications (STORAGE-0016)
//!
//! ## Signal tiers
//!
//! **Set-level (WinRT toast)** — fires only when a replica set crosses the
//! available ↔ offline boundary.  A set is *available* when it has ≥1 ready
//! member; *offline* when it has none.  Adding a second replica to a set that
//! already has one member is silent.
//!
//! | Function        | Condition                          |
//! |-----------------|------------------------------------|
//! | `set_connected` | Set gained its first ready member  |
//! | `set_returned`  | Set came back after being offline  |
//! | `set_offline`   | Set lost its last ready member     |
//!
//! **Per-storage (console)** — fires on every individual managed storage
//! appearing or disappearing in the garden, regardless of replica-set state.
//!
//! | Function              | Condition                             |
//! |-----------------------|---------------------------------------|
//! | `storage_available`   | One managed storage became ready      |
//! | `storage_unavailable` | One managed storage departed          |
//!
//! ## Phase 3 — Explorer info bar
//!
//! `CfReportSyncStatus` sets a per-sync-root status message that Explorer
//! displays in a blue info bar above the file list when any set is offline.
//!
//! ## Phase 4 — Toast notifications
//!
//! WinRT `ToastNotification` fires on set-level boundary crossings.  The AUMID
//! `garden-moss` is registered in HKCU at cloud-filter startup — no installer
//! needed.

use std::path::Path;
use std::sync::Arc;

use garden_common::console::{ConsoleEvent, ConsolePrinter, EventCategory, EventStatus};
use tracing::warn;

// ============================================================================
// Bootstrap
// ============================================================================

const AUMID: &str = "garden-moss";

/// Register the AUMID in the current user's registry so WinRT toasts work.
///
/// Must be called once at cloud-filter startup.  Idempotent.
pub(crate) fn init() {
    #[cfg(target_os = "windows")]
    {
        use winreg::RegKey;
        use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};

        let path = format!(r"Software\Classes\AppUserModelId\{AUMID}");
        match RegKey::predef(HKEY_CURRENT_USER).create_subkey_with_flags(&path, KEY_SET_VALUE) {
            Ok((key, _)) => {
                let _ = key.set_value("DisplayName", &"Zen Garden");
            }
            Err(e) => {
                warn!(aumid = AUMID, error = %e, "failed to register AUMID (toasts may not appear)");
            }
        }
    }
}

// ============================================================================
// Set-level toast notifications
// ============================================================================

/// A set gained its first ready member (was previously absent or at zero).
pub(crate) fn set_connected(set_name: &str) {
    let body = format!("'{set_name}' is connected.");
    tracing::info!(set = %set_name, "storage set connected");

    #[cfg(target_os = "windows")]
    send_toast("Zen Garden", &body);

    #[cfg(not(target_os = "windows"))]
    let _ = body;
}

/// A set that was fully offline came back — at least one member is ready again.
pub(crate) fn set_returned(set_name: &str) {
    let body = format!("'{set_name}' is back online.");
    tracing::info!(set = %set_name, "storage set back online");

    #[cfg(target_os = "windows")]
    send_toast("Zen Garden", &body);

    #[cfg(not(target_os = "windows"))]
    let _ = body;
}

/// A set lost its last ready member — all replicas are now offline.
pub(crate) fn set_offline(set_name: &str) {
    let body = format!("'{set_name}' is offline.");
    tracing::info!(set = %set_name, "storage set offline");

    #[cfg(target_os = "windows")]
    send_toast("Zen Garden", &body);

    #[cfg(not(target_os = "windows"))]
    let _ = body;
}

// ============================================================================
// Per-storage console events
// ============================================================================

/// One individual managed storage became available to the garden.
pub(crate) fn storage_available(
    storage_name: &str,
    stone_name: &str,
    console: &Arc<ConsolePrinter>,
) {
    console.emit(ConsoleEvent::new(
        EventCategory::Storage,
        EventStatus::Connected,
        format!("'{storage_name}' available on {stone_name}"),
    ));
}

/// One individual managed storage departed the garden.
pub(crate) fn storage_unavailable(
    storage_name: &str,
    stone_name: &str,
    console: &Arc<ConsolePrinter>,
) {
    console.emit(ConsoleEvent::new(
        EventCategory::Storage,
        EventStatus::Disconnected,
        format!("'{storage_name}' unavailable (was on {stone_name})"),
    ));
}

// ============================================================================
// Phase 3 — CfReportSyncStatus info bar
// ============================================================================

/// Show the Explorer info bar listing offline sets.
///
/// Overwrites any previous message.  Silently no-ops on non-Windows.
pub(crate) fn report_sync_status(sync_root_path: &Path, offline_sets: &[&str]) {
    if offline_sets.is_empty() {
        return;
    }

    let names = offline_sets.join(", ");
    let message = if offline_sets.len() == 1 {
        format!(
            "'{names}' is not reachable. Check that the stone hosting it is powered on and connected to your network."
        )
    } else {
        format!(
            "Some storages are not reachable ({names}). Check that the stones hosting them are powered on and connected to your network."
        )
    };

    #[cfg(target_os = "windows")]
    {
        if let Err(e) = set_sync_status(sync_root_path, Some(&message)) {
            tracing::debug!(error = %e, "CfReportSyncStatus failed (non-fatal)");
        }
    }

    #[cfg(not(target_os = "windows"))]
    let _ = (sync_root_path, message);
}

/// Clear the Explorer info bar (all sets are back online).
pub(crate) fn clear_sync_status(sync_root_path: &Path) {
    #[cfg(target_os = "windows")]
    {
        if let Err(e) = set_sync_status(sync_root_path, None) {
            tracing::debug!(error = %e, "CfReportSyncStatus clear failed (non-fatal)");
        }
    }

    #[cfg(not(target_os = "windows"))]
    let _ = sync_root_path;
}

#[cfg(target_os = "windows")]
fn set_sync_status(sync_root_path: &Path, message: Option<&str>) -> windows::core::Result<()> {
    use std::mem;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::CloudFilters::{CF_SYNC_STATUS, CfReportSyncStatus};
    use windows::core::PCWSTR;

    let path_wide: Vec<u16> = sync_root_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    match message {
        // SAFETY: `path_wide` is a null-terminated wide string valid for the duration of this call.
        // `PCWSTR::from_raw` wraps the pointer without taking ownership; the Vec keeps it alive.
        None => unsafe { CfReportSyncStatus(PCWSTR::from_raw(path_wide.as_ptr()), None) },
        Some(msg) => {
            let desc_wide: Vec<u16> = msg.encode_utf16().collect();
            let desc_bytes = desc_wide.len() * mem::size_of::<u16>();

            let header_size = mem::size_of::<CF_SYNC_STATUS>();
            let total_size = header_size + desc_bytes;

            let mut buf: Vec<u8> = vec![0u8; total_size];

            // SAFETY: buf is sized to hold the header + description; all
            // pointer arithmetic is within that allocation.
            unsafe {
                let header = buf.as_mut_ptr() as *mut CF_SYNC_STATUS;
                (*header).StructSize = total_size as u32;
                (*header).Code = 1; // non-zero = informational status
                (*header).DescriptionOffset = header_size as u32;
                (*header).DescriptionLength = desc_bytes as u32;
                (*header).DeviceIdOffset = 0;
                (*header).DeviceIdLength = 0;

                let desc_dst = buf[header_size..].as_mut_ptr() as *mut u16;
                std::ptr::copy_nonoverlapping(desc_wide.as_ptr(), desc_dst, desc_wide.len());

                CfReportSyncStatus(
                    PCWSTR::from_raw(path_wide.as_ptr()),
                    Some(buf.as_ptr() as *const CF_SYNC_STATUS),
                )
            }
        }
    }
}

// ============================================================================
// WinRT toast helper
// ============================================================================

#[cfg(target_os = "windows")]
fn send_toast(title: &str, body: &str) {
    if let Err(e) = try_send_toast(title, body) {
        warn!(error = %e, "toast notification failed (non-fatal)");
    }
}

#[cfg(target_os = "windows")]
fn try_send_toast(title: &str, body: &str) -> windows::core::Result<()> {
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};

    let title_escaped = xml_escape(title);
    let body_escaped = xml_escape(body);

    let xml = format!(
        r#"<toast>
  <visual>
    <binding template="ToastGeneric">
      <text>{title_escaped}</text>
      <text>{body_escaped}</text>
    </binding>
  </visual>
</toast>"#
    );

    let doc = XmlDocument::new()?;
    doc.LoadXml(&windows::core::HSTRING::from(xml.as_str()))?;

    let notification = ToastNotification::CreateToastNotification(&doc)?;

    let notifier =
        ToastNotificationManager::CreateToastNotifierWithId(&windows::core::HSTRING::from(AUMID))?;
    notifier.Show(&notification)?;

    Ok(())
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
