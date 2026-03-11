//! PlatformRuntime trait — cross-cutting platform concerns (ARCH-0002).
//!
//! Single abstraction for console/ribbon output and system lifecycle signals.
//! Implemented by `LinuxRuntime` (→ /dev/tty1, sd_notify) and
//! `WindowsRuntime` (→ stdout, SCM). Injected into `AppState` at startup.
//!
//! No `#[cfg]` above the injection point in `bootstrap/run.rs`.

use crate::console::{ribbon_art, BootBannerInfo, ShutdownBannerInfo, UpdateBannerInfo, RIBBON_DIVIDER};

/// Cross-cutting platform concerns: console output and system lifecycle signals.
///
/// Implemented by `LinuxRuntime` (→ /dev/tty1, sd_notify) and
/// `WindowsRuntime` (→ stdout, SCM). Injected into `AppState` at startup.
/// No `#[cfg]` above this layer.
///
/// See: ARCH-0002
pub trait PlatformRuntime: Send + Sync {
    // ---- Console output ----

    /// Write a single line to the platform console.
    fn write_line(&self, text: &str);

    /// Print a multi-line ribbon with standard dividers.
    fn print_ribbon(&self, lines: &[&str]) {
        self.write_line("");
        self.write_line(RIBBON_DIVIDER);
        for line in lines {
            self.write_line(line);
        }
        self.write_line(RIBBON_DIVIDER);
        self.write_line("");
    }

    // ---- System lifecycle signals ----

    /// Signal readiness to the process supervisor.
    fn notify_ready(&self);

    /// Signal graceful shutdown to the process supervisor.
    fn notify_stopping(&self);

    /// Ping the watchdog timer.
    fn notify_watchdog(&self);

    /// Report a status message, optionally extending the startup timeout.
    fn notify_status(&self, status: &str, extend_timeout_usec: Option<u64>);

    // ---- First-boot display helpers ----

    /// Display a bordered header box.
    fn display_header(&self, title: &str) {
        let width = 40usize;
        let content_len = title.len().min(width.saturating_sub(4));
        let padding = (width - 2 - content_len) / 2;
        let extra = if (width - 2 - content_len) % 2 == 1 { 1 } else { 0 };
        self.write_line("");
        self.write_line(&format!("╔{}╗", "═".repeat(width - 2)));
        self.write_line(&format!(
            "║{}{}{}║",
            " ".repeat(padding),
            &title[..content_len],
            " ".repeat(padding + extra)
        ));
        self.write_line(&format!("╚{}╝", "═".repeat(width - 2)));
        self.write_line("");
    }

    /// Display a labelled item: `  label: value`.
    fn display_item(&self, label: &str, value: &str) {
        self.write_line(&format!("  {}: {}", label, value));
    }

    /// Display a success line: `  [OK] message`.
    fn display_success(&self, message: &str) {
        self.write_line(&format!("  [OK] {}", message));
    }

    /// Display a failure line: `  [FAIL] message`.
    fn display_error(&self, message: &str) {
        self.write_line(&format!("  [FAIL] {}", message));
    }

    /// Display a progress line: `  [WAIT] message`.
    fn display_wait(&self, message: &str) {
        self.write_line(&format!("  [WAIT] {}", message));
    }

    // ---- Storage ribbons ----

    /// Storage bank connected and live.
    fn print_storage_connected(&self, name: &str, roles: &[String], used_bytes: u64) {
        let used = crate::utils::format_bytes(used_bytes);
        let role_display = if roles.is_empty() {
            "Primary".to_string()
        } else {
            roles.join(", ")
        };
        self.print_ribbon(&[
            &format!("{}🌱  ✓       Storage \"{}\" connected", ribbon_art::USB_TOP, name),
            &format!(
                "{}            {}, {} used",
                ribbon_art::USB_BODY_ACTIVE, role_display, used
            ),
            ribbon_art::USB_BOTTOM_CONN,
        ]);
    }

    /// Storage bank released — safe to remove.
    fn print_storage_released(&self, name: &str) {
        self.print_ribbon(&[
            &format!("{}↓           Storage released: {}", ribbon_art::USB_TOP, name),
            &format!("{}            Safe to remove device", ribbon_art::USB_BODY_EMPTY),
            ribbon_art::USB_BOTTOM,
        ]);
    }

    // ---- Boot / shutdown / update banners ----

    /// Boot banner: waking cat with stone identity.
    fn print_boot_banner(&self, info: &BootBannerInfo) {
        let symbol = boot_symbol();
        self.print_ribbon(&[
            &format!("{}{:9} Stone: {}", ribbon_art::CAT_HEAD, symbol, info.stone_name),
            &format!("{}          This stone awakens!", ribbon_art::CAT_WAKING),
        ]);
    }

    /// Shutdown banner: sleeping cat with uptime.
    fn print_shutdown_banner(&self, info: &ShutdownBannerInfo) {
        let uptime_secs = info.start_time.elapsed().as_secs();
        let uptime_str = crate::utils::format_uptime(uptime_secs);
        self.print_ribbon(&[
            &format!("{}ZZZzzz    Uptime: {}", ribbon_art::CAT_HEAD, uptime_str),
            &format!("{}          This stone rests...", ribbon_art::CAT_SLEEPING),
        ]);
    }

    /// Update banner: alert cat with version info.
    fn print_update_banner(&self, info: &UpdateBannerInfo) {
        let version_msg = info
            .new_version
            .as_ref()
            .map(|v| format!(" -> v{}", v))
            .unwrap_or_default();
        self.print_ribbon(&[
            &format!(
                "{}UPDATING  Stone: {}{}",
                ribbon_art::CAT_HEAD, info.stone_name, version_msg
            ),
            &format!("{}          This stone transforms...", ribbon_art::CAT_UPDATING),
        ]);
    }
}

// ── Private helpers (shared by default trait implementations) ────────────────

fn is_daytime() -> bool {
    use chrono::{Local, Timelike};
    let hour = Local::now().hour();
    (6..18).contains(&hour)
}

fn boot_symbol() -> &'static str {
    use chrono::{Local, Timelike};
    let symbols_day = ["    *    ", "  c[_]   ", "~stretch~"];
    let symbols_night = ["    c    ", " *dimly* ", "  ~yawn~ "];
    let idx = (Local::now().second() as usize) % 3;
    if is_daytime() {
        symbols_day[idx]
    } else {
        symbols_night[idx]
    }
}
