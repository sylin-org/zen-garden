//! Pavilion — Windows tray client for the Zen Garden.
//!
//! See [PAVILION-0001](../../docs/decisions/PAVILION-0001-windows-client-separation.md).
//!
//! Pavilion is intentionally Windows-only. The crate compiles on other
//! platforms (so the workspace stays coherent) but the binary exits
//! with an explanatory message instead of starting a Tauri shell.

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

#[cfg(target_os = "windows")]
mod announce;
#[cfg(target_os = "windows")]
mod app;
#[cfg(target_os = "windows")]
mod awareness;
#[cfg(target_os = "windows")]
mod ceremony;
#[cfg(target_os = "windows")]
mod commands;
#[cfg(target_os = "windows")]
mod connection;
#[cfg(target_os = "windows")]
mod facilitators;
#[cfg(target_os = "windows")]
mod integration;
#[cfg(target_os = "windows")]
mod jobs;
#[cfg(target_os = "windows")]
mod settings;
#[cfg(target_os = "windows")]
mod tending;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    #[cfg(target_os = "windows")]
    app::run();

    #[cfg(not(target_os = "windows"))]
    {
        eprintln!(
            "Pavilion is a Windows-only client (see PAVILION-0001).\n\
             For non-Windows access, use Lantern's web dashboard."
        );
        std::process::exit(1);
    }
}
