//! Tauri app shell. Windows-only.
//!
//! M0 + DISC-0001 wiring scope: tray + window + chirp-driven topology
//! awareness + Rake-compatible tending. mDNS discovery (Linux),
//! Cloud Filter, and the full Lantern UI hosting come in later
//! milestones.

use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, PhysicalPosition, Rect, WindowEvent,
};
use tauri_plugin_autostart::ManagerExt;

use crate::announce::{policy, ActivityStore, Announcer};
use crate::awareness::Awareness;
use crate::commands;
use crate::facilitators::{self, FacilitatorEngine};
use crate::integration::cloud_filter;
use crate::settings::SettingsStore;
use crate::tending::Tending;

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::get_topology,
            commands::get_tended,
            commands::set_tended,
            commands::get_services,
            commands::get_pond_status,
            commands::get_storage,
            commands::get_activity,
            commands::get_settings,
            commands::set_settings,
            commands::restart_service,
            commands::rest_service,
            commands::wake_service,
            commands::get_suggestion,
            commands::dismiss_suggestion,
            commands::hide_suggestion_kind,
            commands::show_main_window,
            commands::capture_snapshot,
            commands::plant_snapshot,
            commands::list_seeds_in_bank,
            commands::get_offering_sets,
            crate::ceremony::ceremony_step,
        ])
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // Another invocation tried to start; focus the existing window.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            tracing::info!("Pavilion starting");

            // Tray icon + menu.
            let open_item =
                MenuItem::with_id(app, "open", "Open Pavilion", true, None::<&str>)?;
            let quit_item =
                MenuItem::with_id(app, "quit", "Quit Pavilion", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open_item, &quit_item])?;

            let _tray = TrayIconBuilder::with_id("pavilion-tray")
                .icon(
                    app.default_window_icon()
                        .ok_or("default icon missing")?
                        .clone(),
                )
                .tooltip("Pavilion")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        tracing::info!("Pavilion exiting via tray menu");
                        // Disconnect Cloud Filter cleanly. Sync root stays
                        // registered so next launch reuses it; only an
                        // explicit uninstall flow should call unregister().
                        cloud_filter::stop();
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    // Left-click — toggle the popover anchored to the tray
                    // icon. Right-click is handled by the menu (set above
                    // via `show_menu_on_left_click(false)`).
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        rect,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        toggle_popover(app, rect);
                    }
                })
                .build(app)?;

            // Popover — Win11 acrylic flyout that surfaces tended-stone
            // status, recent activity, and the live facilitator
            // suggestion. Acrylic is applied once at startup; the
            // window is hidden by default and shown on tray click.
            // Apply blur to dismiss on focus loss — the standard
            // Windows tray-flyout behaviour.
            if let Some(popover) = app.get_webview_window("popover") {
                if let Err(e) = window_vibrancy::apply_acrylic(&popover, Some((20, 20, 25, 180))) {
                    tracing::warn!(
                        error = %e,
                        "popover: acrylic backdrop unavailable (non-fatal)"
                    );
                }
            }

            // Settings — keystone for everything Announcer-shaped.
            // Loaded synchronously at startup so the Announcer can
            // hold an Arc<SettingsStore> from first construction.
            let settings = Arc::new(SettingsStore::new(app.handle().clone()));
            app.manage(settings.clone());

            // Autostart — reconcile the OS-level autostart state
            // with the persisted setting, both at startup and on
            // every subsequent settings change.
            spawn_autostart_supervisor(settings.clone(), app.handle().clone());

            // Announcer — coalesces and dedupes events fed from
            // Awareness and the SSE storage observer; activity rows
            // for the Activity view; toasts when the policy promotes
            // (warmup + suppressions + quiet hours via SettingsStore).
            let activity_store = ActivityStore::default();
            let announcer = Announcer::new(
                app.handle().clone(),
                activity_store.clone(),
                settings.clone(),
            );
            policy::spawn_flush_loop(announcer.clone());
            app.manage(activity_store.clone());
            app.manage(announcer.clone());

            // Awareness — subscribes to STONE_CHIRP, evicts stale entries,
            // emits `topology-changed` events to the frontend, and feeds
            // join/leave events to the Announcer.
            let awareness = Arc::new(Awareness::new(app.handle().clone(), announcer.clone()));
            awareness.spawn_listeners();
            app.manage(awareness.clone());

            // Tending — Rake-compatible `~/.zen-garden/.tending` file.
            // Cloud Filter chains off the same task because it needs a
            // tended stone before it can connect a provider; doing
            // both in one task avoids racing the tending state.
            // Spawned async because `Tending::new` reads the file.
            let app_handle = app.handle().clone();
            let supervisor_announcer = announcer.clone();
            let supervisor_settings = settings.clone();
            tauri::async_runtime::spawn(async move {
                let tending = Arc::new(Tending::new(app_handle.clone()).await);
                app_handle.manage(tending.clone());
                tending
                    .clone()
                    .spawn_auto_tend(awareness.clone(), supervisor_settings.clone());

                // Multi-stone storage observer reconciler —
                // PAVILION-0002 §M2 garden-wide aggregation. One
                // SSE observer per stone in awareness; reconciles
                // on every awareness topology change.
                spawn_multi_stone_observer_supervisor(
                    supervisor_announcer,
                    awareness.clone(),
                );

                // FacilitatorEngine — sibling pipeline of the
                // Announcer that watches awareness + tending +
                // settings + pond status and surfaces a single
                // active suggestion banner at a time.
                let engine = FacilitatorEngine::new(
                    app_handle.clone(),
                    awareness,
                    tending.clone(),
                    supervisor_settings,
                );
                app_handle.manage(engine.clone());
                facilitators::engine::spawn_supervisor(engine);

                // Cloud Filter — register sync root + connect provider.
                // Non-fatal on failure (no admin, Win32 API unavailable);
                // Pavilion still runs as a tray app without Explorer
                // integration.
                if let Err(e) = cloud_filter::start(tending).await {
                    tracing::warn!(error = %e, "Cloud Filter startup failed (non-fatal)");
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            match event {
                // Close-to-tray. Pavilion only exits via the tray menu's Quit.
                // The main window hides on close; the popover hides too (it's
                // never user-closeable, but defensive).
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = window.hide();
                }
                // Popover dismisses on focus loss — the standard
                // Windows tray-flyout pattern. Click-outside ⇒ blur ⇒
                // hide. The main window keeps its blur unhandled so
                // it stays visible while the user works in another
                // app.
                WindowEvent::Focused(false) if window.label() == "popover" => {
                    let _ = window.hide();
                    record_popover_hide();
                }
                _ => {}
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Pavilion");
}

/// Time of the most recent popover hide. Used as a bounce guard
/// against the tray-click-to-dismiss double fire: clicking the tray
/// while the popover is focused triggers blur (which hides the
/// popover), and then the tray click handler runs and would
/// otherwise re-show what the user just dismissed. If the click
/// arrives within `POPOVER_BOUNCE` of the last hide, treat it as
/// the second half of that gesture and stay hidden.
fn popover_last_hide() -> &'static Mutex<Option<Instant>> {
    static SLOT: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

const POPOVER_BOUNCE: Duration = Duration::from_millis(180);

fn record_popover_hide() {
    if let Ok(mut slot) = popover_last_hide().lock() {
        *slot = Some(Instant::now());
    }
}

/// True when a recent hide should swallow the next show request —
/// the user just clicked the tray to dismiss, not to re-open.
fn popover_was_just_hidden() -> bool {
    let Ok(slot) = popover_last_hide().lock() else {
        return false;
    };
    slot.is_some_and(|t| t.elapsed() < POPOVER_BOUNCE)
}

/// Toggle the popover window in response to a tray-icon left click.
///
/// If the popover is currently visible, hide it. Otherwise position
/// it above the tray icon (Win11 system-tray flyout convention) and
/// show + focus it. The blur handler in `on_window_event` will hide
/// it again when the user clicks anywhere else.
///
/// Acrylic is applied once at startup; we don't re-apply on every
/// show — that triggers a brief reflow.
fn toggle_popover(app: &AppHandle, tray_rect: Rect) {
    let Some(window) = app.get_webview_window("popover") else {
        tracing::warn!("popover: window not found in config");
        return;
    };

    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        record_popover_hide();
        return;
    }

    // Click-to-dismiss bounce guard. See `popover_was_just_hidden`.
    if popover_was_just_hidden() {
        return;
    }

    // Position the popover so its bottom edge sits a few pixels above
    // the tray icon's top edge, horizontally centered on the icon.
    // tray_rect's position/size are platform-dependent unit kinds
    // (Position / Size enums); we normalise to physical pixels so
    // `set_position(PhysicalPosition)` is DPI-correct.
    let scale = window.scale_factor().unwrap_or(1.0);
    let icon_pos = tray_rect.position.to_physical::<f64>(scale);
    let icon_size = tray_rect.size.to_physical::<f64>(scale);
    let popover_size = window.outer_size().unwrap_or_else(|_| tauri::PhysicalSize {
        width: (360.0 * scale) as u32,
        height: (480.0 * scale) as u32,
    });

    let target_x = icon_pos.x + icon_size.width / 2.0 - (popover_size.width as f64) / 2.0;
    // 8 px logical gap between popover and tray icon.
    let gap = 8.0 * scale;
    let target_y = icon_pos.y - popover_size.height as f64 - gap;

    // Clamp to the popover's current monitor so it never spawns
    // off-screen on multi-display rigs where the tray icon sits at
    // the screen edge.
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());

    let (clamped_x, clamped_y) = if let Some(m) = monitor {
        let m_pos = m.position();
        let m_size = m.size();
        let max_x = (m_pos.x as f64 + m_size.width as f64) - popover_size.width as f64;
        let max_y = (m_pos.y as f64 + m_size.height as f64) - popover_size.height as f64;
        (
            target_x.clamp(m_pos.x as f64, max_x),
            target_y.clamp(m_pos.y as f64, max_y),
        )
    } else {
        (target_x.max(0.0), target_y.max(0.0))
    };

    let _ = window.set_position(PhysicalPosition::new(clamped_x, clamped_y));
    let _ = window.show();
    let _ = window.set_focus();
}

/// Reconcile OS-level autostart with the persisted `autostart_enabled`
/// setting. Runs once at startup (so an externally-edited settings
/// file lines up with the OS), then watches the settings channel and
/// flips OS state whenever the setting changes.
///
/// Idempotent — calling `enable()` on an already-enabled launcher is
/// a no-op, and same for `disable()`. We still gate on a "changed"
/// check just to keep log noise minimal.
fn spawn_autostart_supervisor(settings: Arc<SettingsStore>, app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let initial = settings.snapshot().await.autostart_enabled;
        apply_autostart(&app, initial);

        let mut rx = settings.subscribe();
        let mut last = initial;
        loop {
            if rx.changed().await.is_err() {
                tracing::warn!("autostart supervisor: settings channel closed");
                break;
            }
            let next = rx.borrow_and_update().autostart_enabled;
            if next == last {
                continue;
            }
            apply_autostart(&app, next);
            last = next;
        }
    });
}

/// Push the desired state to the OS-level autostart launcher.
/// Reads `is_enabled()` first and only flips state when it
/// differs — `auto-launch`'s `disable()` returns
/// `ERROR_FILE_NOT_FOUND` when the registry entry is already
/// missing, which is "already disabled" rather than a real
/// failure but the crate surfaces it as `Err` regardless.
///
/// Failures on the actual flip are logged but never propagated —
/// autostart is a convenience feature, not a correctness
/// invariant.
fn apply_autostart(app: &AppHandle, desired: bool) {
    let manager = app.autolaunch();
    let current = match manager.is_enabled() {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "autostart: is_enabled probe failed");
            return;
        }
    };
    if current == desired {
        tracing::debug!(
            enabled = desired,
            "autostart: OS state already matches settings"
        );
        return;
    }
    let result = if desired {
        manager.enable()
    } else {
        manager.disable()
    };
    match result {
        Ok(()) => {
            tracing::info!(
                enabled = desired,
                "autostart: OS state synced to settings"
            );
        }
        Err(e) => {
            tracing::warn!(error = %e, enabled = desired, "autostart: OS state flip failed");
        }
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the pure pieces extracted from the popover
    //! lifecycle. The bounce guard is the testable seam; the rest
    //! of `toggle_popover` is window plumbing better verified
    //! manually against a live Win11 desktop.
    //!
    //! The bounce guard is process-global state, so all tests in
    //! this module share a `TEST_LOCK` that serialises mutations
    //! to it — otherwise `cargo test`'s parallel runner can race
    //! one test's setup against another's read.
    use super::*;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Force-set the bounce guard to a specific instant — only used
    /// by tests to simulate "the popover was hidden N ms ago"
    /// without needing real elapsed time.
    fn set_last_hide_for_test(at: Option<Instant>) {
        let mut slot = popover_last_hide().lock().unwrap();
        *slot = at;
    }

    #[test]
    fn bounce_guard_rejects_show_within_window() {
        let _serialized = TEST_LOCK.lock().unwrap();
        // A hide that happened "now" — well inside the bounce
        // window. Toggle's first-click-after-hide path must treat
        // the click as the second half of a dismiss gesture.
        set_last_hide_for_test(Some(Instant::now()));
        assert!(popover_was_just_hidden(), "fresh hide must trigger guard");
    }

    #[test]
    fn bounce_guard_allows_show_after_window_elapses() {
        let _serialized = TEST_LOCK.lock().unwrap();
        // Simulate a hide far enough in the past to fall outside
        // the bounce window. Toggle should let the user re-open.
        let stale = Instant::now()
            .checked_sub(POPOVER_BOUNCE * 4)
            .expect("test pre-condition: clock supports POPOVER_BOUNCE-sized subtraction");
        set_last_hide_for_test(Some(stale));
        assert!(
            !popover_was_just_hidden(),
            "old hide must not block subsequent show"
        );
    }

    #[test]
    fn bounce_guard_is_open_on_cold_start() {
        let _serialized = TEST_LOCK.lock().unwrap();
        // No hide ever recorded — first tray click must be allowed
        // to open the popover.
        set_last_hide_for_test(None);
        assert!(
            !popover_was_just_hidden(),
            "guard must be open when no hide has happened"
        );
    }
}

/// Multi-stone storage-observer reconciler (PAVILION-0002 §M2).
///
/// Watches the awareness channel and keeps an active SSE observer
/// per known stone. On every topology snapshot:
///
/// - For each stone newly present: spawn an observer.
/// - For each stone newly absent (TTL evicted): cancel its
///   observer.
/// - For stones that stayed: leave their observer alone.
///
/// The announcer's existing per-key cooldown / coalesce policy
/// already handles the merged event firehose, so events from
/// every stone land in the same Activity feed without further
/// fan-in plumbing.
fn spawn_multi_stone_observer_supervisor(
    announcer: Announcer,
    awareness: Arc<crate::awareness::Awareness>,
) {
    use std::collections::HashMap;
    use tokio_util::sync::CancellationToken;

    tauri::async_runtime::spawn(async move {
        let mut active: HashMap<String, CancellationToken> = HashMap::new();
        let mut rx = awareness.subscribe();

        // Prime against whatever's already in awareness — discovery
        // probe responses arrive within ms of startup.
        let initial = rx.borrow_and_update().clone();
        reconcile(&announcer, &mut active, &initial);

        loop {
            if rx.changed().await.is_err() {
                tracing::warn!("multi-stone observer: awareness channel closed");
                break;
            }
            let snapshot = rx.borrow_and_update().clone();
            reconcile(&announcer, &mut active, &snapshot);
        }
    });

    fn reconcile(
        announcer: &Announcer,
        active: &mut std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
        snapshot: &[crate::awareness::AwareStone],
    ) {
        use crate::announce::observer::{spawn_storage_observer, ObserverTarget};

        let known: std::collections::HashSet<String> =
            snapshot.iter().map(|s| s.stone_id.clone()).collect();

        // Cancel observers for stones no longer in awareness.
        let stale: Vec<String> = active
            .keys()
            .filter(|id| !known.contains(*id))
            .cloned()
            .collect();
        for id in stale {
            if let Some(token) = active.remove(&id) {
                tracing::info!(stone_id = %id, "multi-stone observer: stone evicted, cancelling");
                token.cancel();
            }
        }

        // Spawn observers for newly-known stones.
        for stone in snapshot {
            if active.contains_key(&stone.stone_id) {
                continue;
            }
            tracing::info!(
                stone = %stone.stone_name,
                stone_id = %stone.stone_id,
                "multi-stone observer: stone joined, spawning observer"
            );
            let target = ObserverTarget {
                stone_name: stone.stone_name.clone(),
                endpoint: stone.endpoint.clone(),
            };
            let token = spawn_storage_observer(announcer.clone(), target);
            active.insert(stone.stone_id.clone(), token);
        }
    }
}

