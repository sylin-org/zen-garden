//! Toast dispatcher — thin wrapper over `tauri-plugin-notification`.
//!
//! The Announcer decides *whether* to fire; this module decides *how*
//! the message reads. Title/body shape follows
//! [pavilion-interaction-design §6](../../../../docs/specs/pavilion-interaction-design.md):
//! - Title: 1 line, < 50 chars
//! - Body: ≤ 3 lines, explains *why* it matters
//!
//! Action buttons aren't wired yet — Tauri's notification plugin
//! supports them, but mapping them back to in-app navigation is a
//! follow-up. For v1 toasts are read-only.

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use super::event::GardenEvent;

/// Format an event into a (title, body) pair. Returned strings are
/// already short enough for Windows toast layout — callers don't need
/// to truncate.
pub fn format(event: &GardenEvent) -> (String, String) {
    match event {
        GardenEvent::StoneJoined { stone_name, .. } => (
            format!("{stone_name} joined"),
            format!("New stone visible in your garden. Open Pavilion to tend it."),
        ),
        GardenEvent::StoneLeft { stone_name, .. } => (
            format!("{stone_name} offline"),
            "Lost contact. Services on it are unavailable until it returns.".into(),
        ),
        GardenEvent::StorageActivity {
            stone_name,
            bank_name,
            creates,
            modifies,
            deletes,
        } => {
            let total = creates + modifies + deletes;
            let title = format!("{bank_name} synced {total} files");
            let body = format!(
                "{creates} new, {modifies} changed, {deletes} removed on {stone_name}."
            );
            (title, body)
        }
    }
}

/// Fire a Windows toast for the event. Errors are logged but never
/// propagated — a failed toast must not take the Announcer down.
pub fn fire(app: &AppHandle, event: &GardenEvent) {
    let (title, body) = format(event);
    if let Err(e) = app
        .notification()
        .builder()
        .title(&title)
        .body(&body)
        .show()
    {
        tracing::warn!(error = %e, title = %title, "toast: dispatch failed");
    } else {
        tracing::debug!(title = %title, "toast: fired");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stone_joined_title_under_50_chars() {
        let event = GardenEvent::StoneJoined {
            stone_id: "abc".into(),
            stone_name: "crystal-forest".into(),
            endpoint: "http://crystal-forest:7185".into(),
        };
        let (title, _) = format(&event);
        assert!(title.len() < 50, "title was {} chars: {title}", title.len());
    }

    #[test]
    fn storage_activity_includes_total() {
        let event = GardenEvent::StorageActivity {
            stone_name: "stone-01".into(),
            bank_name: "personal".into(),
            creates: 3,
            modifies: 1,
            deletes: 0,
        };
        let (title, body) = format(&event);
        assert!(title.contains("4 files"));
        assert!(body.contains("stone-01"));
    }
}
