//! Explorer integration signals — info bar and toast notifications (STORAGE-0016)
//!
//! ## Phase 3 — Explorer info bar
//!
//! `CfReportSyncStatus` sets a per-sync-root status message that Explorer
//! displays in a blue info bar above the file list when any storage is
//! offline.  Passing `None` clears the bar.
//!
//! ## Phase 4 — Toast notifications
//!
//! WinRT `ToastNotification` fires once per state *transition* (online →
//! offline, offline → online).  Notifications are suppressed for the first
//! 120 s after Moss starts to avoid alerting on cold-boot.
//!
//! ### Service context note
//!
//! WinRT toasts from a Windows service (Session 0) require the app to have
//! a registered AppUserModelID (AUMID).  Until the Moss installer registers
//! one, toasts are best-effort: they compile and function when Moss runs
//! interactively (dev / debugging), and are silently skipped if the WinRT
//! stack returns an error in service context.

use std::path::Path;
use std::time::Instant;

use tracing::{debug, warn};

// ============================================================================
// Phase 3 — CfReportSyncStatus info bar
// ============================================================================

/// Show the Explorer info bar for the sync root listing offline storages.
///
/// If the bar is already showing, this overwrites it with the current list.
/// Silently no-ops on non-Windows or if the Win32 call fails.
pub(crate) fn report_sync_status(sync_root_path: &Path, offline_storages: &[&str]) {
    if offline_storages.is_empty() {
        return;
    }

    let names = offline_storages.join(", ");
    let message = if offline_storages.len() == 1 {
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
            debug!(error = %e, "CfReportSyncStatus failed (non-fatal)");
        }
    }

    #[cfg(not(target_os = "windows"))]
    let _ = (sync_root_path, message);
}

/// Clear the Explorer info bar (all storages are back online).
pub(crate) fn clear_sync_status(sync_root_path: &Path) {
    #[cfg(target_os = "windows")]
    {
        if let Err(e) = set_sync_status(sync_root_path, None) {
            debug!(error = %e, "CfReportSyncStatus clear failed (non-fatal)");
        }
    }

    #[cfg(not(target_os = "windows"))]
    let _ = sync_root_path;
}

#[cfg(target_os = "windows")]
fn set_sync_status(sync_root_path: &Path, message: Option<&str>) -> windows::core::Result<()> {
    use std::mem;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::CloudFilters::{CfReportSyncStatus, CF_SYNC_STATUS};
    use windows::core::PCWSTR;

    let path_wide: Vec<u16> = sync_root_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    match message {
        None => {
            // Passing null clears the status bar
            unsafe { CfReportSyncStatus(PCWSTR::from_raw(path_wide.as_ptr()), None) }
        }
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
// Phase 4 — Toast notifications
// ============================================================================

const STARTUP_SUPPRESS_SECS: u64 = 120;

/// Notify the user that a storage went offline.
///
/// Suppressed during the startup window to avoid alerting on cold-boot.
pub(crate) fn notify_offline(storage_name: &str, startup_at: Instant) {
    if startup_at.elapsed().as_secs() < STARTUP_SUPPRESS_SECS {
        debug!(storage = %storage_name, "suppressing offline notification (startup window)");
        return;
    }

    let title = "Zen Garden";
    let body = format!("'{storage_name}' is offline — check that its stone is reachable.");

    tracing::info!(storage = %storage_name, "storage went offline");

    #[cfg(target_os = "windows")]
    send_toast(title, &body);

    #[cfg(not(target_os = "windows"))]
    let _ = (title, body);
}

/// Notify the user that a storage came back online.
///
/// Suppressed during the startup window.
pub(crate) fn notify_online(storage_name: &str, stone_name: &str, startup_at: Instant) {
    if startup_at.elapsed().as_secs() < STARTUP_SUPPRESS_SECS {
        debug!(storage = %storage_name, "suppressing online notification (startup window)");
        return;
    }

    let title = "Zen Garden";
    let body = format!("'{storage_name}' is back online on {stone_name}.");

    tracing::info!(storage = %storage_name, stone = %stone_name, "storage came back online");

    #[cfg(target_os = "windows")]
    send_toast(title, &body);

    #[cfg(not(target_os = "windows"))]
    let _ = (title, body);
}

// ============================================================================
// WinRT toast helper
// ============================================================================

/// Fire a WinRT toast notification.
///
/// Best-effort: logs a warning if the WinRT stack is unavailable (e.g.
/// running under Session 0 without a registered AUMID) and continues.
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

    // Escape XML special characters in user-visible strings
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

    // "garden-moss" is the AUMID.  Must be registered in the Start menu
    // for toasts to appear from a service context — see STORAGE-0016 §Phase 4.
    let notifier = ToastNotificationManager::CreateToastNotifierWithId(
        &windows::core::HSTRING::from("garden-moss"),
    )?;
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
