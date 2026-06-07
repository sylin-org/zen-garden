//! Privileged-command construction honoring `HostProfile.runtime.privilege_escalation`.
//!
//! Host operations that need root (mount, ip, chown of system paths) historically
//! hard-coded a `sudo` prefix. That breaks where `sudo` does not exist — a rooted
//! Android Stone (Moss already runs as uid 0) or a minimal host. HOST-0001 makes the
//! escalation strategy a profile knob; this module is the single place that turns it
//! into an actual command.
//!
//! Policy (with a `geteuid()==0` safety net so a mis-set profile never breaks host ops):
//! - already root → run the program directly (no `sudo`), regardless of profile;
//! - `PrivilegeMode::Sudo` → prefix `sudo`;
//! - `PrivilegeMode::Direct` → run directly;
//! - `PrivilegeMode::None` → run directly (the op fails naturally if it truly needs root).

use garden_common::host::{self, PrivilegeMode};

/// True when the process runs as uid 0.
fn is_root() -> bool {
    // SAFETY: getuid() always succeeds — it reads the caller's real uid and has no
    // preconditions or side effects.
    unsafe { libc::getuid() == 0 }
}

/// Whether privileged commands should be prefixed with `sudo`.
pub fn use_sudo() -> bool {
    !is_root() && matches!(host::profile().runtime.privilege_escalation, PrivilegeMode::Sudo)
}

/// Build an async [`tokio::process::Command`] for a privileged operation, prefixing
/// `sudo` only when the profile calls for it and we are not already root.
///
/// Replaces `tokio::process::Command::new("sudo").arg(program)`: pass the *real*
/// program (e.g. `"mount"`), then chain `.args([...])` as before.
pub fn command(program: &str) -> tokio::process::Command {
    if use_sudo() {
        let mut cmd = tokio::process::Command::new("sudo");
        cmd.arg(program);
        cmd
    } else {
        tokio::process::Command::new(program)
    }
}

/// `(program, args)` to run for a privileged *synchronous* command (e.g. via
/// `run_command_timed_sync`). Pass the real program + its args; this prepends `sudo`
/// only when required.
pub fn sync_command<'p>(program: &'p str, args: &[&'p str]) -> (&'p str, Vec<&'p str>) {
    if use_sudo() {
        let mut argv = Vec::with_capacity(args.len() + 1);
        argv.push(program);
        argv.extend_from_slice(args);
        ("sudo", argv)
    } else {
        (program, args.to_vec())
    }
}
