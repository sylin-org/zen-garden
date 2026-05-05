//! Sysfs probe for candidate-stage connectivity health (STORAGE-0019).
//!
//! Reads the same `/sys/block/<dev>/...` files STORAGE-0018's
//! [`probe_device_health`](super::super::platform::probe_device_health)
//! consumes, but interprets them through a different lens: instead of
//! "is this adopted volume still healthy?", the question here is
//! "should we attempt recovery before showing this device to the
//! user?".
//!
//! The probe itself is pure — no `/sys` writes, no subprocesses, no
//! blocking I/O beyond the sysfs reads themselves. Recovery actions
//! live in [`super::recovery`](super::recovery).
//!
//! ## Decision model
//!
//! ```text
//!   /sys/block/<dev>/size = 0
//!     ├── ioerr_cnt = 0  → empty enclosure (no media inserted)
//!     │                    No software recovery would help.
//!     └── ioerr_cnt > 0  → degraded bridge / SCSI handshake failure
//!                          Try SCSI rescan, then USB re-auth.
//!
//!   /sys/block/<dev>/size > 0
//!     ├── state = running     → healthy (warn if ioerr_cnt > 0)
//!     ├── state = offline / transport-offline
//!     │                       → kernel has it marked dead;
//!     │                          re-auth may revive it
//!     └── state = unrecognized → forward as healthy + warning
//! ```
//!
//! ## Testability
//!
//! Every read is parameterized on a `sysfs_root: &Path` so tests
//! point the probe at a `tempfile::TempDir` containing synthetic
//! `block/<dev>/...` trees. The production path passes `/sys`.

use std::path::Path;

use garden_common::storage::ConnectivityWarning;

/// Production sysfs root.
pub const SYSFS_ROOT: &str = "/sys";

/// What the probe observed from the device's sysfs entries.
///
/// All fields are raw OS facts; the interpretation lives in
/// [`ProbeVerdict`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeData {
    /// `/sys/block/<dev>/size` parsed as logical-block count
    /// (multiply by 512 for bytes).
    pub size_blocks: u64,
    /// `/sys/block/<dev>/device/ioerr_cnt`. `0` when unavailable.
    pub io_errors: u64,
    /// `/sys/block/<dev>/device/iotmo_cnt`. `0` when unavailable.
    pub io_timeouts: u64,
    /// `/sys/block/<dev>/device/state` trimmed. Common values:
    /// `running`, `offline`, `transport-offline`, `cancel`, `created`.
    /// `None` when the file doesn't exist (e.g. non-SCSI device).
    pub scsi_state: Option<String>,
    /// USB port path resolved from the device symlink, e.g. `2-3.4`.
    /// `None` when the device isn't on the USB bus.
    pub usb_port: Option<String>,
}

impl ProbeData {
    /// Convenience: total reported size in bytes (assumes 512-byte
    /// logical blocks, which is the SCSI sysfs convention).
    pub fn size_bytes(&self) -> u64 {
        self.size_blocks.saturating_mul(512)
    }

    /// `true` when the device returned a non-zero size to READ CAPACITY.
    pub fn responds_with_size(&self) -> bool {
        self.size_blocks > 0
    }

    /// `true` when the device's SCSI state is the normal "running" value
    /// or the file doesn't exist (non-SCSI device — assumed healthy).
    pub fn scsi_state_is_running(&self) -> bool {
        match &self.scsi_state {
            Some(s) => s == "running",
            None => true,
        }
    }
}

/// Probe-time verdict on whether the device needs recovery.
///
/// The classifier and recovery orchestrator both consume this. The
/// orchestrator decides which recovery action to attempt; the
/// classifier decides what state to surface to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeVerdict {
    /// Device looks healthy. Forward as-is. Warnings may still be
    /// attached (e.g. historical `ioerr_cnt > 0`).
    Healthy,
    /// Device reports zero size with I/O errors — almost certainly a
    /// transient bridge-firmware glitch. Try SCSI rescan first; if
    /// that doesn't help, re-authorize the USB endpoint.
    TryRescanThenReauth,
    /// Device's SCSI state is offline or transport-offline. Rescan
    /// won't help (device is marked dead at the SCSI layer); only
    /// USB re-authorization stands a chance.
    TryReauthOnly,
    /// Device reports zero size with no I/O errors — the bridge
    /// enumerated correctly but has no media inside. Software
    /// recovery cannot help; the user must insert a drive.
    EmptyEnclosure,
}

/// Outcome of a single probe pass on one block device.
///
/// Carries both the raw observations and the interpreted verdict so
/// downstream code can render either at its discretion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeOutcome {
    pub verdict: ProbeVerdict,
    pub data: ProbeData,
    pub warnings: Vec<ConnectivityWarning>,
}

impl ProbeOutcome {
    /// `true` when the verdict suggests at least one recovery action
    /// is worth attempting.
    pub fn recoverable(&self) -> bool {
        matches!(
            self.verdict,
            ProbeVerdict::TryRescanThenReauth | ProbeVerdict::TryReauthOnly
        )
    }
}

/// Probe a block device for connectivity health.
///
/// `sysfs_root` is normally `/sys`; tests pass a tempdir.
/// `device_basename` is the bare device name like `"sdc"` (no
/// `/dev/` prefix, no path).
///
/// Always returns a `ProbeOutcome` — missing sysfs entries are
/// treated as "no signal" and produce a `Healthy` verdict with
/// best-effort data, never a panic or error. The probe should be
/// resilient to unfamiliar devices (loopback, ramdisk, virtual).
pub fn probe(sysfs_root: &Path, device_basename: &str) -> ProbeOutcome {
    let data = read_probe_data(sysfs_root, device_basename);

    let mut warnings = Vec::new();
    if data.io_errors > 0 {
        warnings.push(ConnectivityWarning::PriorIoErrors {
            count: data.io_errors,
        });
    }

    let verdict = classify(&data);
    ProbeOutcome {
        verdict,
        data,
        warnings,
    }
}

/// Apply the verdict decision tree to the raw probe data.
fn classify(data: &ProbeData) -> ProbeVerdict {
    // SCSI state takes precedence: if the kernel marked it offline,
    // size doesn't matter — we need a re-auth to bring it back.
    match data.scsi_state.as_deref() {
        Some("offline") | Some("transport-offline") => return ProbeVerdict::TryReauthOnly,
        _ => {}
    }

    if data.responds_with_size() {
        // Device responded to READ CAPACITY. Healthy. Warnings
        // (e.g. prior I/O errors) ride alongside the verdict.
        return ProbeVerdict::Healthy;
    }

    // Size is zero. The discriminator is whether anything went
    // wrong: a clean READ CAPACITY returning zero with no error
    // counter is an empty enclosure; a zero with I/O errors is a
    // bridge glitch that recovery can usually fix.
    if data.io_errors > 0 || data.io_timeouts > 0 {
        ProbeVerdict::TryRescanThenReauth
    } else {
        ProbeVerdict::EmptyEnclosure
    }
}

// ============================================================================
// Sysfs reads
// ============================================================================

/// Read all probe inputs for one device under `sysfs_root`.
///
/// Missing files are tolerated — each field defaults to a "no signal"
/// value so the verdict layer sees a coherent picture even on
/// non-SCSI devices.
fn read_probe_data(sysfs_root: &Path, device_basename: &str) -> ProbeData {
    let block_dir = sysfs_root.join("block").join(device_basename);
    let device_dir = block_dir.join("device");

    ProbeData {
        size_blocks: read_u64(&block_dir.join("size")).unwrap_or(0),
        io_errors: read_u64(&device_dir.join("ioerr_cnt")).unwrap_or(0),
        io_timeouts: read_u64(&device_dir.join("iotmo_cnt")).unwrap_or(0),
        scsi_state: read_trimmed(&device_dir.join("state")),
        usb_port: resolve_usb_port(sysfs_root, device_basename),
    }
}

/// Read a sysfs file as a `u64`, trimming whitespace.
///
/// Tolerates both decimal (`"123"`) and hex (`"0x7b"`) representations.
/// The kernel emits `ioerr_cnt` and `iotmo_cnt` as `%x` on some
/// versions (Debian 13's 6.12.x, observed) and as `%lu` on others;
/// supporting both is the only way to get a correct count without
/// pinning a kernel version.
fn read_u64(path: &Path) -> Option<u64> {
    let raw = std::fs::read_to_string(path).ok()?;
    parse_u64_hex_or_dec(raw.trim())
}

/// Parse a u64 that may be plain decimal (`"123"`) or hex with the
/// conventional `0x`/`0X` prefix (`"0x7b"`). Returns `None` for any
/// other format.
fn parse_u64_hex_or_dec(s: &str) -> Option<u64> {
    if let Some(rest) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(rest, 16).ok()
    } else {
        s.parse::<u64>().ok()
    }
}

/// Read a sysfs file as a trimmed `String`.
fn read_trimmed(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

/// Resolve the USB port path of a block device by walking sysfs.
///
/// `/sys/block/<dev>` is a symlink that, for a USB-attached SCSI
/// device, points into `/sys/devices/pci.../usb<N>/<port-path>/...`.
/// The deepest path component matching `<bus>-<port>[.<port>]*` is
/// the USB port path (e.g. `2-3.4`) — exactly what
/// `/sys/bus/usb/devices/<port>/authorized` accepts for re-auth.
///
/// Returns `None` for non-USB devices.
fn resolve_usb_port(sysfs_root: &Path, device_basename: &str) -> Option<String> {
    let block_link = sysfs_root.join("block").join(device_basename);
    let resolved = std::fs::canonicalize(&block_link).ok()?;

    // Walk components looking for the USB port path token. The token
    // has the form `<digit>-<digit>(.<digit>)*` and lives directly
    // under a `usb<N>` ancestor. We pick the deepest match — that's
    // the most-specific port (the one we want to write to).
    let mut deepest: Option<String> = None;
    let mut saw_usb_root = false;
    for comp in resolved.components() {
        let s = comp.as_os_str().to_string_lossy();
        if s.starts_with("usb") && s[3..].chars().all(|c| c.is_ascii_digit()) {
            saw_usb_root = true;
            continue;
        }
        if saw_usb_root && is_usb_port_token(&s) {
            deepest = Some(s.into_owned());
        }
    }
    deepest
}

/// Is this string a USB port path component like `2-3.4`?
fn is_usb_port_token(s: &str) -> bool {
    // Format: <bus>-<port>[.<port>]*
    let Some((bus, rest)) = s.split_once('-') else {
        return false;
    };
    if bus.is_empty() || !bus.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    if rest.is_empty() {
        return false;
    }
    rest.split('.').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Build a synthetic sysfs tree under a tempdir containing one
    /// block device with arbitrary attribute files.
    struct SyntheticSysfs {
        _root: TempDir,
        path: PathBuf,
    }

    impl SyntheticSysfs {
        fn new() -> Self {
            let root = TempDir::new().expect("tempdir");
            let path = root.path().to_path_buf();
            fs::create_dir_all(path.join("block")).unwrap();
            Self { _root: root, path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        /// Create `block/<dev>/size` and `block/<dev>/device/...` files.
        fn add_device(
            &self,
            dev: &str,
            size_blocks: u64,
            ioerr_cnt: Option<u64>,
            iotmo_cnt: Option<u64>,
            state: Option<&str>,
        ) {
            let block_dir = self.path.join("block").join(dev);
            fs::create_dir_all(&block_dir).unwrap();
            fs::write(block_dir.join("size"), size_blocks.to_string()).unwrap();
            let device_dir = block_dir.join("device");
            fs::create_dir_all(&device_dir).unwrap();
            if let Some(c) = ioerr_cnt {
                fs::write(device_dir.join("ioerr_cnt"), c.to_string()).unwrap();
            }
            if let Some(c) = iotmo_cnt {
                fs::write(device_dir.join("iotmo_cnt"), c.to_string()).unwrap();
            }
            if let Some(s) = state {
                fs::write(device_dir.join("state"), s).unwrap();
            }
        }

        /// Add a USB port path to the device by canonicalizing
        /// `block/<dev>` to a path containing a usb topology.
        ///
        /// Layout produced:
        /// ```text
        ///   <root>/devices/usb2/2-3.4/host1/target.../block/<dev>
        ///   <root>/block/<dev> -> <root>/devices/.../block/<dev>
        /// ```
        ///
        /// Unix-only because Windows symlinks need admin or developer
        /// mode. The two tests that exercise USB topology resolution
        /// are gated on `#[cfg(unix)]` for the same reason.
        #[cfg(unix)]
        fn add_usb_topology(&self, dev: &str, port: &str) {
            let target_dir = self
                .path
                .join("devices")
                .join("usb2")
                .join(port)
                .join("host1")
                .join("target1:0:0")
                .join("1:0:0:0")
                .join("block")
                .join(dev);
            fs::create_dir_all(&target_dir).unwrap();
            // Move the existing attribute files into the canonical
            // location so the symlink target has them.
            let block_link = self.path.join("block").join(dev);
            if block_link.exists() {
                for entry in fs::read_dir(&block_link).unwrap() {
                    let e = entry.unwrap();
                    let name = e.file_name();
                    let from = e.path();
                    let to = target_dir.join(&name);
                    if from.is_dir() {
                        copy_dir_recursive(&from, &to);
                    } else {
                        fs::copy(&from, &to).unwrap();
                    }
                }
                fs::remove_dir_all(&block_link).unwrap();
            }
            symlink(&target_dir, &block_link).unwrap();
        }
    }

    #[cfg(unix)]
    fn copy_dir_recursive(src: &Path, dst: &Path) {
        fs::create_dir_all(dst).unwrap();
        for entry in fs::read_dir(src).unwrap() {
            let e = entry.unwrap();
            let from = e.path();
            let to = dst.join(e.file_name());
            if from.is_dir() {
                copy_dir_recursive(&from, &to);
            } else {
                fs::copy(&from, &to).unwrap();
            }
        }
    }

    #[test]
    fn healthy_device_with_real_size_and_running_state() {
        let sysfs = SyntheticSysfs::new();
        sysfs.add_device("sdb", 976773168, Some(0), Some(0), Some("running"));
        let outcome = probe(sysfs.path(), "sdb");
        assert_eq!(outcome.verdict, ProbeVerdict::Healthy);
        assert_eq!(outcome.data.size_bytes(), 976773168 * 512);
        assert!(outcome.warnings.is_empty());
        assert!(!outcome.recoverable());
    }

    #[test]
    fn healthy_device_with_historical_io_errors_emits_warning() {
        let sysfs = SyntheticSysfs::new();
        sysfs.add_device("sdb", 976773168, Some(1), Some(0), Some("running"));
        let outcome = probe(sysfs.path(), "sdb");
        assert_eq!(outcome.verdict, ProbeVerdict::Healthy);
        assert_eq!(outcome.warnings.len(), 1);
        assert!(matches!(
            outcome.warnings[0],
            ConnectivityWarning::PriorIoErrors { count: 1 }
        ));
    }

    #[test]
    fn zero_size_with_io_errors_classifies_as_recoverable() {
        // The exact RTL9210C scenario from STORAGE-0019:
        // Read Capacity(10) failed → 0 logical blocks, ioerr_cnt > 0.
        let sysfs = SyntheticSysfs::new();
        sysfs.add_device("sdc", 0, Some(2), Some(0), Some("running"));
        let outcome = probe(sysfs.path(), "sdc");
        assert_eq!(outcome.verdict, ProbeVerdict::TryRescanThenReauth);
        assert!(outcome.recoverable());
        assert_eq!(outcome.warnings.len(), 1);
    }

    #[test]
    fn zero_size_with_io_timeouts_also_classifies_as_recoverable() {
        let sysfs = SyntheticSysfs::new();
        sysfs.add_device("sdc", 0, Some(0), Some(3), Some("running"));
        let outcome = probe(sysfs.path(), "sdc");
        assert_eq!(outcome.verdict, ProbeVerdict::TryRescanThenReauth);
    }

    #[test]
    fn zero_size_clean_classifies_as_empty_enclosure() {
        // Bridge enumerates fine, no errors, just no media inside.
        // Recovery cannot help here.
        let sysfs = SyntheticSysfs::new();
        sysfs.add_device("sdc", 0, Some(0), Some(0), Some("running"));
        let outcome = probe(sysfs.path(), "sdc");
        assert_eq!(outcome.verdict, ProbeVerdict::EmptyEnclosure);
        assert!(!outcome.recoverable());
        assert!(outcome.warnings.is_empty());
    }

    #[test]
    fn offline_state_classifies_as_reauth_only() {
        // Kernel marked the SCSI device offline. Rescan won't help —
        // only a USB re-authorization will revive it.
        let sysfs = SyntheticSysfs::new();
        sysfs.add_device("sdc", 0, Some(0), Some(0), Some("offline"));
        let outcome = probe(sysfs.path(), "sdc");
        assert_eq!(outcome.verdict, ProbeVerdict::TryReauthOnly);
        assert!(outcome.recoverable());
    }

    #[test]
    fn transport_offline_state_classifies_as_reauth_only() {
        let sysfs = SyntheticSysfs::new();
        sysfs.add_device("sdc", 0, Some(0), Some(0), Some("transport-offline"));
        let outcome = probe(sysfs.path(), "sdc");
        assert_eq!(outcome.verdict, ProbeVerdict::TryReauthOnly);
    }

    #[test]
    fn missing_state_file_treats_device_as_running() {
        // Non-SCSI devices (loopback, ramdisk) don't expose a state
        // file. Probe should treat them as healthy when size is set.
        let sysfs = SyntheticSysfs::new();
        sysfs.add_device("loop0", 1024, None, None, None);
        let outcome = probe(sysfs.path(), "loop0");
        assert_eq!(outcome.verdict, ProbeVerdict::Healthy);
    }

    #[test]
    fn missing_size_file_returns_zero_blocks() {
        let sysfs = SyntheticSysfs::new();
        let block_dir = sysfs.path().join("block").join("ghost");
        fs::create_dir_all(&block_dir).unwrap();
        // No size file — read_u64 returns None → defaults to 0.
        let outcome = probe(sysfs.path(), "ghost");
        // Treated as empty enclosure since no errors signal otherwise.
        assert_eq!(outcome.verdict, ProbeVerdict::EmptyEnclosure);
        assert_eq!(outcome.data.size_blocks, 0);
    }

    #[test]
    fn unknown_state_value_does_not_block_classification() {
        // Defensive: a kernel version emitting an unfamiliar state
        // string should not crash the probe; size becomes the
        // discriminator.
        let sysfs = SyntheticSysfs::new();
        sysfs.add_device("sdb", 976773168, Some(0), Some(0), Some("recovery"));
        let outcome = probe(sysfs.path(), "sdb");
        assert_eq!(outcome.verdict, ProbeVerdict::Healthy);
    }

    #[cfg(unix)]
    #[test]
    fn usb_port_resolved_from_topology_symlink() {
        let sysfs = SyntheticSysfs::new();
        sysfs.add_device("sdc", 500118192, Some(0), Some(0), Some("running"));
        sysfs.add_usb_topology("sdc", "2-3.4");
        let outcome = probe(sysfs.path(), "sdc");
        assert_eq!(outcome.data.usb_port.as_deref(), Some("2-3.4"));
    }

    #[cfg(unix)]
    #[test]
    fn usb_port_resolution_returns_deepest_match_for_chained_hubs() {
        // /devices/usb2/2-3/2-3.4 — the deeper port (2-3.4) is the
        // one that maps to the actual device endpoint.
        let sysfs = SyntheticSysfs::new();
        let target = sysfs
            .path()
            .join("devices")
            .join("usb2")
            .join("2-3")
            .join("2-3.4")
            .join("host1")
            .join("target1:0:0")
            .join("1:0:0:0")
            .join("block")
            .join("sdc");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("size"), "500118192").unwrap();
        let device_dir = target.join("device");
        fs::create_dir_all(&device_dir).unwrap();
        fs::write(device_dir.join("ioerr_cnt"), "0").unwrap();
        fs::write(device_dir.join("iotmo_cnt"), "0").unwrap();
        fs::write(device_dir.join("state"), "running").unwrap();
        symlink(&target, sysfs.path().join("block").join("sdc")).unwrap();

        let outcome = probe(sysfs.path(), "sdc");
        assert_eq!(outcome.data.usb_port.as_deref(), Some("2-3.4"));
    }

    #[test]
    fn non_usb_device_has_no_usb_port() {
        let sysfs = SyntheticSysfs::new();
        sysfs.add_device("sda", 976773168, Some(0), Some(0), Some("running"));
        let outcome = probe(sysfs.path(), "sda");
        assert_eq!(outcome.data.usb_port, None);
    }

    #[test]
    fn is_usb_port_token_recognizes_common_shapes() {
        assert!(is_usb_port_token("2-3"));
        assert!(is_usb_port_token("2-3.4"));
        assert!(is_usb_port_token("2-3.4.1"));
        assert!(is_usb_port_token("1-1"));

        assert!(!is_usb_port_token("usb2"));
        assert!(!is_usb_port_token("host1"));
        assert!(!is_usb_port_token("2-"));
        assert!(!is_usb_port_token("-3"));
        assert!(!is_usb_port_token("a-b"));
        assert!(!is_usb_port_token("2-3."));
        assert!(!is_usb_port_token("2-3.4."));
    }

    #[test]
    fn parse_u64_accepts_decimal() {
        assert_eq!(parse_u64_hex_or_dec("0"), Some(0));
        assert_eq!(parse_u64_hex_or_dec("123"), Some(123));
        assert_eq!(parse_u64_hex_or_dec("18446744073709551615"), Some(u64::MAX));
    }

    #[test]
    fn parse_u64_accepts_hex_with_lowercase_prefix() {
        // Debian 13's 6.12 kernel emits ioerr_cnt as `0x1`. Live stone
        // verification surfaced this; the parser must handle it.
        assert_eq!(parse_u64_hex_or_dec("0x0"), Some(0));
        assert_eq!(parse_u64_hex_or_dec("0x1"), Some(1));
        assert_eq!(parse_u64_hex_or_dec("0xff"), Some(255));
        assert_eq!(parse_u64_hex_or_dec("0xdeadbeef"), Some(0xdeadbeef));
    }

    #[test]
    fn parse_u64_accepts_hex_with_uppercase_prefix() {
        assert_eq!(parse_u64_hex_or_dec("0X1"), Some(1));
        assert_eq!(parse_u64_hex_or_dec("0XFF"), Some(255));
    }

    #[test]
    fn parse_u64_rejects_garbage() {
        assert_eq!(parse_u64_hex_or_dec(""), None);
        assert_eq!(parse_u64_hex_or_dec("abc"), None);
        assert_eq!(parse_u64_hex_or_dec("0x"), None);
        assert_eq!(parse_u64_hex_or_dec("0xZZZ"), None);
        assert_eq!(parse_u64_hex_or_dec("-1"), None);
    }

    #[test]
    fn probe_reads_hex_formatted_io_counters() {
        // Mirror what the live stone's kernel emits: hex with 0x prefix.
        let sysfs = SyntheticSysfs::new();
        let block_dir = sysfs.path().join("block").join("sdc");
        fs::create_dir_all(&block_dir).unwrap();
        fs::write(block_dir.join("size"), "0").unwrap();
        let device_dir = block_dir.join("device");
        fs::create_dir_all(&device_dir).unwrap();
        fs::write(device_dir.join("ioerr_cnt"), "0x2").unwrap();
        fs::write(device_dir.join("iotmo_cnt"), "0x0").unwrap();
        fs::write(device_dir.join("state"), "running").unwrap();
        let outcome = probe(sysfs.path(), "sdc");
        assert_eq!(outcome.data.io_errors, 2);
        assert_eq!(outcome.data.io_timeouts, 0);
        assert_eq!(outcome.verdict, ProbeVerdict::TryRescanThenReauth);
    }

    #[test]
    fn missing_device_dir_does_not_crash() {
        // A `/sys/block/foo` directory with no `device/` subdir
        // (rare but possible for virtual block devices). Probe should
        // still return a coherent ProbeOutcome.
        let sysfs = SyntheticSysfs::new();
        fs::create_dir_all(sysfs.path().join("block").join("foo")).unwrap();
        fs::write(sysfs.path().join("block").join("foo").join("size"), "0").unwrap();
        let outcome = probe(sysfs.path(), "foo");
        assert_eq!(outcome.data.io_errors, 0);
        assert_eq!(outcome.data.scsi_state, None);
        assert_eq!(outcome.verdict, ProbeVerdict::EmptyEnclosure);
    }
}
