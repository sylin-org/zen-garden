//! User-facing settings — wire types and policy helpers.
//!
//! Persisted to `~/.zen-garden/.pavilion-settings.json` and consumed
//! by the Announcer (quiet hours, per-source suppression) and by
//! anything that surfaces user-controlled OS state (autostart).
//!
//! Defaults aim for "calm by default, present when needed":
//! - quiet hours **off** (user opts in)
//! - no suppressions
//! - autostart **off** (user opts in via the install flow)
//!
//! `SettingsPatch` is the Tauri IPC shape — every field optional so
//! the frontend can update one knob without round-tripping the rest.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub quiet_hours: QuietHours,
    /// Event kinds the user has dismissed permanently (e.g.
    /// `"storage_activity"`). Compared against
    /// [`crate::announce::GardenEvent::kind_str`].
    #[serde(default)]
    pub suppressed_kinds: Vec<String>,
    #[serde(default)]
    pub autostart_enabled: bool,
    /// Whether the first-launch onboarding flow has been
    /// completed. The frontend shows the Onboarding view until
    /// this flips true (either by tending a stone explicitly or
    /// by clicking Skip — both paths set this).
    #[serde(default)]
    pub onboarded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuietHours {
    #[serde(default)]
    pub enabled: bool,
    /// `"HH:MM"` in 24-hour local time. Start of the quiet window.
    #[serde(default = "default_start")]
    pub start: String,
    /// `"HH:MM"` in 24-hour local time. End of the quiet window.
    /// Wraps over midnight when `end < start` (e.g. 22:00 → 07:00).
    #[serde(default = "default_end")]
    pub end: String,
}

fn default_start() -> String {
    "22:00".to_string()
}

fn default_end() -> String {
    "07:00".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            quiet_hours: QuietHours::default(),
            suppressed_kinds: Vec::new(),
            autostart_enabled: false,
            onboarded: false,
        }
    }
}

impl Default for QuietHours {
    fn default() -> Self {
        Self {
            enabled: false,
            start: default_start(),
            end: default_end(),
        }
    }
}

/// IPC shape for `set_settings` — every field optional so the
/// frontend can flip a single switch without echoing the rest.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SettingsPatch {
    pub quiet_hours: Option<QuietHoursPatch>,
    pub suppressed_kinds: Option<Vec<String>>,
    pub autostart_enabled: Option<bool>,
    pub onboarded: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct QuietHoursPatch {
    pub enabled: Option<bool>,
    pub start: Option<String>,
    pub end: Option<String>,
}

impl Settings {
    /// Apply a partial update in place.
    pub fn apply(&mut self, patch: SettingsPatch) {
        if let Some(qh) = patch.quiet_hours {
            if let Some(v) = qh.enabled {
                self.quiet_hours.enabled = v;
            }
            if let Some(v) = qh.start {
                self.quiet_hours.start = v;
            }
            if let Some(v) = qh.end {
                self.quiet_hours.end = v;
            }
        }
        if let Some(v) = patch.suppressed_kinds {
            self.suppressed_kinds = v;
        }
        if let Some(v) = patch.autostart_enabled {
            self.autostart_enabled = v;
        }
        if let Some(v) = patch.onboarded {
            self.onboarded = v;
        }
    }

    /// Whether the given event kind has been permanently dismissed.
    pub fn is_suppressed(&self, kind: &str) -> bool {
        self.suppressed_kinds.iter().any(|k| k == kind)
    }

    /// Whether the local clock falls inside the configured quiet
    /// window. Returns `false` when quiet hours are disabled or
    /// when the configured times don't parse — fail-open so a
    /// malformed setting doesn't silently mute every toast.
    pub fn is_quiet_now(&self, now: chrono::NaiveTime) -> bool {
        if !self.quiet_hours.enabled {
            return false;
        }
        let Some(start) = parse_hhmm(&self.quiet_hours.start) else {
            return false;
        };
        let Some(end) = parse_hhmm(&self.quiet_hours.end) else {
            return false;
        };
        if start == end {
            // Empty window — never quiet.
            false
        } else if start < end {
            now >= start && now < end
        } else {
            // Wraps midnight (e.g. 22:00 start, 07:00 end).
            now >= start || now < end
        }
    }
}

fn parse_hhmm(s: &str) -> Option<chrono::NaiveTime> {
    chrono::NaiveTime::parse_from_str(s.trim(), "%H:%M").ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveTime;

    fn t(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    #[test]
    fn defaults_keep_calm() {
        let s = Settings::default();
        assert!(!s.quiet_hours.enabled);
        assert!(s.suppressed_kinds.is_empty());
        assert!(!s.autostart_enabled);
    }

    #[test]
    fn quiet_hours_disabled_is_never_quiet() {
        let mut s = Settings::default();
        s.quiet_hours.enabled = false;
        s.quiet_hours.start = "00:00".into();
        s.quiet_hours.end = "23:59".into();
        assert!(!s.is_quiet_now(t(12, 0)));
    }

    #[test]
    fn quiet_hours_same_day_window() {
        let mut s = Settings::default();
        s.quiet_hours.enabled = true;
        s.quiet_hours.start = "10:00".into();
        s.quiet_hours.end = "12:00".into();
        assert!(!s.is_quiet_now(t(9, 59)));
        assert!(s.is_quiet_now(t(10, 0)));
        assert!(s.is_quiet_now(t(11, 30)));
        assert!(!s.is_quiet_now(t(12, 0)));
        assert!(!s.is_quiet_now(t(15, 0)));
    }

    #[test]
    fn quiet_hours_wraps_midnight() {
        let mut s = Settings::default();
        s.quiet_hours.enabled = true;
        s.quiet_hours.start = "22:00".into();
        s.quiet_hours.end = "07:00".into();
        assert!(s.is_quiet_now(t(22, 0)));
        assert!(s.is_quiet_now(t(23, 30)));
        assert!(s.is_quiet_now(t(0, 0)));
        assert!(s.is_quiet_now(t(6, 59)));
        assert!(!s.is_quiet_now(t(7, 0)));
        assert!(!s.is_quiet_now(t(15, 0)));
    }

    #[test]
    fn quiet_hours_empty_window_is_never_quiet() {
        let mut s = Settings::default();
        s.quiet_hours.enabled = true;
        s.quiet_hours.start = "10:00".into();
        s.quiet_hours.end = "10:00".into();
        assert!(!s.is_quiet_now(t(10, 0)));
    }

    #[test]
    fn quiet_hours_malformed_fails_open() {
        let mut s = Settings::default();
        s.quiet_hours.enabled = true;
        s.quiet_hours.start = "not a time".into();
        s.quiet_hours.end = "07:00".into();
        // Malformed → never quiet, so toasts still fire.
        assert!(!s.is_quiet_now(t(23, 0)));
    }

    #[test]
    fn patch_applies_partial_updates() {
        let mut s = Settings::default();
        s.apply(SettingsPatch {
            quiet_hours: Some(QuietHoursPatch {
                enabled: Some(true),
                start: None,
                end: None,
            }),
            suppressed_kinds: None,
            autostart_enabled: None,
            onboarded: None,
        });
        assert!(s.quiet_hours.enabled);
        assert_eq!(s.quiet_hours.start, "22:00"); // unchanged default
    }

    #[test]
    fn suppression_lookup_is_exact() {
        let s = Settings {
            suppressed_kinds: vec!["storage_activity".into()],
            ..Default::default()
        };
        assert!(s.is_suppressed("storage_activity"));
        assert!(!s.is_suppressed("stone_joined"));
        assert!(!s.is_suppressed("storage")); // no prefix matching
    }
}
