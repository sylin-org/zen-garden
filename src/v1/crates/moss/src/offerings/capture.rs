//! The living will's grammar (ADR-0005 §1): how an offering asks to be
//! remembered.
//!
//! `capture` rides the manifest but sits OUTSIDE `WorkloadSpec` and outside
//! `plan_hash` — backup policy is lifecycle intent, not desired execution;
//! hashing it would flip plans on policy edits (§7's structural note).
//!
//! Modes: **stateless** (nothing to preserve beyond signature+configs),
//! **lock-and-copy** (consistent under an application lock — quiesce/resume
//! hooks, raw copy inside the window), **export** (byte-copy wrong even
//! under lock; the offering produces its own dump). Defaults lean safe: an
//! undeclared `capture` on something with volumes is UNTRUSTED for
//! consistency and surfaces honestly ([`readiness`]) — it is never
//! silently tarred.
//!
//! Hooks are manifest-sourced only, never API-supplied; they run
//! in-container and may use the closed template vocabulary — `{fqn}`,
//! `{stem}`, `{instance}`, `{workspace}`, `{volume.<name>}`,
//! `{port.<role>}` — with the volume/role names checked against the
//! manifest's own declarations at load. Unknown variables are load errors
//! (OFFERINGS.md §5.1's `${input.k}` precedent).
//!
//! A lock is a statement the application makes; a freeze is something done
//! to it. Quiesce failure aborts cleanly; resume executes finally-style on
//! every exit path — a stranded `fsyncLock` outranks every other disaster.

// Readiness/constants land with their consuming slices (pipeline, explain;
// ADR-0005 §1-§6 land slice by slice).
#![allow(dead_code)]

use super::manifest::Manifest;
use serde::Deserialize;

/// Default hook budget (§1 examples).
pub const DEFAULT_HOOK_TIMEOUT_S: u64 = 30;
/// Hard bound on the lock window; exceed = abort + loud degradation.
pub const DEFAULT_MAX_LOCKED_S: u64 = 120;
/// Exports may be slow; the budget is generous but real.
pub const DEFAULT_EXPORT_TIMEOUT_S: u64 = 900;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CaptureMode {
    Stateless,
    LockAndCopy,
    Export,
}

impl CaptureMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stateless => "stateless",
            Self::LockAndCopy => "lock-and-copy",
            Self::Export => "export",
        }
    }
}

/// One in-container command window: the command and how long it may run.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Hook {
    /// argv, run inside the offering's container (servers must be live to
    /// be told anything).
    pub exec: Vec<String>,
    #[serde(default = "default_hook_timeout")]
    pub timeout_s: u64,
}

fn default_hook_timeout() -> u64 {
    DEFAULT_HOOK_TIMEOUT_S
}

/// The capture policy, validated at manifest load — one machine-truth
/// parse; nothing here is API-supplied.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CapturePolicy {
    pub mode: CaptureMode,
    /// The lock: run before the imprint window opens.
    #[serde(default)]
    pub quiesce: Option<Hook>,
    /// The release: executes finally-style on EVERY exit path.
    #[serde(default)]
    pub resume: Option<Hook>,
    /// Hard bound on the lock window (lock-and-copy only).
    #[serde(default = "default_max_locked")]
    pub max_locked_s: u64,
    /// The export command (export mode only).
    #[serde(default)]
    pub export: Option<Hook>,
}

fn default_max_locked() -> u64 {
    DEFAULT_MAX_LOCKED_S
}

impl CapturePolicy {
    /// Cross-field rules serde can't express. Every violation is a LOUD
    /// load error — an invalid will is worse than none.
    pub fn validate(&self, stem: &str, volumes: &[String], port_roles: &[String]) -> Result<(), String> {
        let fail = |msg: &str| format!("manifest '{stem}': capture: {msg}");
        match self.mode {
            CaptureMode::Stateless => {
                if self.quiesce.is_some() || self.resume.is_some() || self.export.is_some() {
                    return Err(fail("stateless preserves nothing; declare no hooks, or pick the mode that matches them"));
                }
            }
            CaptureMode::LockAndCopy => {
                if self.quiesce.is_none() || self.resume.is_none() {
                    return Err(fail(
                        "lock-and-copy requires BOTH quiesce and resume — a lock that \
                         cannot be released outranks every other disaster",
                    ));
                }
                if self.export.is_some() {
                    return Err(fail("export hook belongs to mode: export"));
                }
                if self.max_locked_s == 0 {
                    return Err(fail("max_locked_s must be at least 1 second"));
                }
            }
            CaptureMode::Export => {
                if self.export.is_none() {
                    return Err(fail("export requires an export hook — the offering must produce its own dump"));
                }
                if self.quiesce.is_some() || self.resume.is_some() {
                    return Err(fail("quiesce/resume belong to mode: lock-and-copy"));
                }
            }
        }
        let hooks: [Option<&Hook>; 3] = [&self.quiesce, &self.resume, &self.export]
            .map(|h| h.as_ref());
        for hook in hooks.into_iter().flatten() {
            if hook.exec.is_empty() {
                return Err(fail("hook exec must name at least one argv element"));
            }
            for arg in &hook.exec {
                Self::check_templates(stem, arg, volumes, port_roles)
                    .map_err(|e| fail(&e))?;
            }
        }
        Ok(())
    }

    /// The closed template vocabulary; unknown variables are LOAD errors.
    fn check_templates(
        stem: &str,
        arg: &str,
        volumes: &[String],
        port_roles: &[String],
    ) -> Result<(), String> {
        let mut rest = arg;
        while let Some(open) = rest.find('{') {
            let Some(close) = rest[open..].find('}') else {
                return Err(format!("hook arg '{arg}' opens a template brace and never closes it"));
            };
            let token = &rest[open + 1..open + close];
            let known = match token {
                "fqn" | "stem" | "instance" | "workspace" => true,
                v if v.starts_with("volume.") => volumes.contains(&v["volume.".len()..].to_string()),
                p if p.starts_with("port.") => port_roles.contains(&p["port.".len()..].to_string()),
                _ => false,
            };
            if !known {
                return Err(format!(
                    "hook arg '{arg}' uses unknown template '{{{token}}}' — the closed \
                     vocabulary is fqn, stem, instance, workspace, volume.<name>, port.<role>"
                ));
            }
            rest = &rest[open + close + 1..];
        }
        let _ = stem;
        Ok(())
    }
}

/// How honestly this offering can be preserved — for surfaces (`explain`,
/// the portal) that must never silently tar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Readiness {
    /// Nothing lives here beyond the signature (no managed, no volumes).
    NothingToPreserve,
    /// A declared, consistent capture mode is on file.
    Trusted(CaptureMode),
    /// Volumes exist but no trusted capture is declared: raw copy would be
    /// a lie for this workload. Surfaces must say so.
    Untrusted,
}

/// Classify a manifest's preservation readiness (§1's lean-safe default).
pub fn readiness(m: &Manifest) -> Readiness {
    let Some(managed) = &m.managed else {
        return Readiness::NothingToPreserve;
    };
    match &m.capture {
        Some(policy) => Readiness::Trusted(policy.mode),
        None if managed.volumes.is_empty() => Readiness::NothingToPreserve,
        None => Readiness::Untrusted,
    }
}

#[cfg(test)]
mod tests {
    // R4.1: unwrap/expect sanctioned in tests.
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::{CaptureMode, DEFAULT_HOOK_TIMEOUT_S, Readiness};
    use crate::offerings::manifest::Catalog;

    const BASE: &str = r#"
kind: software
name: witnessdb
category: data
description: fixture
managed:
  image: witnessdb:7
  ports: { client: 6379 }
  volumes:
    - { name: data, mount: /data }
"#;

    #[test]
    fn all_three_modes_parse_and_validate() {
        let stateless = Catalog::parse("witnessdb", &format!("{BASE}\ncapture:\n  mode: stateless\n")).unwrap();
        assert_eq!(stateless.capture.as_ref().unwrap().mode, CaptureMode::Stateless);

        let lockcopy = Catalog::parse("witnessdb", &format!(
            "{BASE}\ncapture:\n  mode: lock-and-copy\n  quiesce: {{exec: [\"db\", \"lock\"], timeout_s: 20}}\n  resume: {{exec: [\"db\", \"unlock\"]}}\n  max_locked_s: 90\n"
        ))
        .unwrap();
        let policy = lockcopy.capture.as_ref().unwrap();
        assert_eq!(policy.mode, CaptureMode::LockAndCopy);
        assert_eq!(policy.max_locked_s, 90, "declared budget wins");
        assert_eq!(policy.quiesce.as_ref().unwrap().timeout_s, DEFAULT_HOOK_TIMEOUT_S - 10);
        assert_eq!(policy.resume.as_ref().unwrap().timeout_s, DEFAULT_HOOK_TIMEOUT_S);

        let export = Catalog::parse("witnessdb", &format!(
            "{BASE}\ncapture:\n  mode: export\n  export: {{exec: [\"dump\", \"{{workspace}}/db.dump\"]}}\n"
        ))
        .unwrap();
        assert_eq!(export.capture.as_ref().unwrap().mode, CaptureMode::Export);
    }

    #[test]
    fn broken_wills_refuse_loudly() {
        // lock-and-copy without release: a stranded lock outranks every disaster.
        let err = Catalog::parse("witnessdb", &format!(
            "{BASE}\ncapture:\n  mode: lock-and-copy\n  quiesce: {{exec: [\"lock\"]}}\n"
        ))
        .unwrap_err();
        assert!(err.contains("BOTH quiesce and resume"), "{err}");

        // export without a dump command.
        let err = Catalog::parse("witnessdb", &format!("{BASE}\ncapture:\n  mode: export\n")).unwrap_err();
        assert!(err.contains("requires an export hook"), "{err}");

        // stateless with hooks: the mode and the hooks disagree.
        let err = Catalog::parse("witnessdb", &format!(
            "{BASE}\ncapture:\n  mode: stateless\n  export: {{exec: [\"dump\"]}}\n"
        ))
        .unwrap_err();
        assert!(err.contains("preserves nothing"), "{err}");

        // capture on an offering with no managed section.
        let bare = "kind: software\nname: witnessdb\ncategory: data\ndescription: x\ncapture:\n  mode: stateless\n";
        let err = Catalog::parse("witnessdb", bare).unwrap_err();
        assert!(err.contains("requires a managed section"), "{err}");
    }

    #[test]
    fn unknown_templates_are_load_errors_and_known_ones_pass() {
        let err = Catalog::parse("witnessdb", &format!(
            "{BASE}\ncapture:\n  mode: export\n  export: {{exec: [\"dump\", \"{{volume.nope}}/d\"]}}\n"
        ))
        .unwrap_err();
        assert!(err.contains("unknown template"), "{err}");

        let ok = Catalog::parse("witnessdb", &format!(
            "{BASE}\ncapture:\n  mode: export\n  export: {{exec: [\"dump\", \"--out\", \"{{workspace}}/{{fqn}}/{{volume.data}}-{{port.client}}.dump\"]}}\n"
        ))
        .unwrap();
        assert_eq!(ok.capture.as_ref().unwrap().mode, CaptureMode::Export);

        let err = Catalog::parse("witnessdb", &format!(
            "{BASE}\ncapture:\n  mode: export\n  export: {{exec: [\"dump\", \"{{open brace no close\"]}}\n"
        ))
        .unwrap_err();
        assert!(err.contains("never closes"), "{err}");
    }

    #[test]
    fn readiness_is_honest_about_untrusted_volumes() {
        use super::readiness;
        // Volumes + no capture = untrusted (never silently tarred).
        assert_eq!(readiness(&Catalog::parse("witnessdb", BASE).unwrap()), Readiness::Untrusted);
        // Declared capture = trusted.
        let trusted = Catalog::parse(
            "witnessdb",
            &format!("{BASE}\ncapture:\n  mode: stateless\n"),
        )
        .unwrap();
        assert_eq!(readiness(&trusted), Readiness::Trusted(CaptureMode::Stateless));
        // No volumes + no capture = nothing to preserve.
        let bare = BASE.replace(
            "  volumes:\n    - { name: data, mount: /data }\n",
            "",
        );
        assert_eq!(
            readiness(&Catalog::parse("witnessdb", &bare).unwrap()),
            Readiness::NothingToPreserve
        );
    }
}
