//! Tauri app shell. Windows-only.
//!
//! M0 + DISC-0001 wiring scope: tray + window + chirp-driven topology
//! awareness + Rake-compatible tending. mDNS discovery (Linux),
//! Cloud Filter, and the full Lantern UI hosting come in later
//! milestones.

use std::sync::Arc;

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};

use crate::announce::{observer, policy, ActivityStore, Announcer};
use crate::awareness::Awareness;
use crate::commands;
use crate::integration::cloud_filter;
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
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let visible = window.is_visible().unwrap_or(false);
                            if visible {
                                let _ = window.hide();
                            } else {
                                let _ = window.show();
                                let _ = window.unminimize();
                                let _ = window.set_focus();
                            }
                        }
                    }
                })
                .build(app)?;

            // Announcer — coalesces and dedupes events fed from
            // Awareness and the SSE storage observer; activity rows
            // for the Activity view; toasts when the policy promotes.
            let activity_store = ActivityStore::default();
            let announcer = Announcer::new(app.handle().clone(), activity_store.clone());
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
            tauri::async_runtime::spawn(async move {
                let tending = Arc::new(Tending::new(app_handle.clone()).await);
                app_handle.manage(tending.clone());
                tending.clone().spawn_auto_tend(awareness);

                // Storage observer supervisor — rebinds the SSE
                // observer task to the currently tended stone.
                spawn_observer_supervisor(supervisor_announcer, tending.clone());

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
            // Close-to-tray. Pavilion only exits via the tray menu's Quit.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Pavilion");
}

/// Watch the tending channel and rebind the storage SSE observer to
/// the currently tended stone. Cancels the previous observer's token
/// before spawning a new one so streams don't overlap.
fn spawn_observer_supervisor(announcer: Announcer, tending: Arc<Tending>) {
    tauri::async_runtime::spawn(async move {
        let mut rx = tending.subscribe();
        let mut current_token: Option<tokio_util::sync::CancellationToken> = None;

        // Prime with the current value (`watch::Receiver` yields the
        // initial state on the first `borrow_and_update`).
        let initial = rx.borrow_and_update().clone();
        if let Some(stone) = initial {
            tracing::info!(stone = %stone.stone_name, "observer supervisor: starting initial observer");
            current_token = Some(observer::spawn_storage_observer(announcer.clone(), stone));
        }

        loop {
            if rx.changed().await.is_err() {
                tracing::warn!("observer supervisor: tending channel closed");
                break;
            }
            let next = rx.borrow_and_update().clone();
            if let Some(token) = current_token.take() {
                token.cancel();
            }
            if let Some(stone) = next {
                tracing::info!(stone = %stone.stone_name, "observer supervisor: rebinding to new tending");
                current_token = Some(observer::spawn_storage_observer(announcer.clone(), stone));
            } else {
                tracing::info!("observer supervisor: tending cleared");
            }
        }
    });
}

