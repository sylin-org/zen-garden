//! Recovery actions for the connectivity-recovery pipeline (STORAGE-0019).
//!
//! Implements the two recovery primitives the probe verdicts call for:
//!
//! - **SCSI rescan** — `echo 1 > /sys/block/<dev>/device/rescan`. Cheap,
//!   safe, often enough on its own. Re-issues `INQUIRY` and
//!   `READ CAPACITY` without disturbing the USB endpoint.
//!
//! - **USB re-authorization** — `echo 0 > /sys/bus/usb/devices/<port>/authorized`,
//!   wait, `echo 1 > .../authorized`. The kernel disconnects and
//!   re-enumerates the USB device from scratch. Equivalent to pulling
//!   the cable in software.
//!
//! Both actions need root. Moss's systemd unit runs as root, so the
//! direct write succeeds; a fallback through `sudo sh -c 'echo …'`
//! covers dev environments where Moss runs as the `stone` user.
//!
//! ## Boundary conditions
//!
//! - **Adopted devices are off-limits.** Re-authorizing a device with
//!   active mounts kills inflight I/O. The orchestrator MUST verify the
//!   device is not in the managed `Volumes` map before invoking these
//!   actions.
//! - **Per-device retry budget.** One rescan + one re-auth per device
//!   per minute, keyed on USB port path so retry state survives the
//!   `sdc → sdd` renumbering that re-auth causes.
//! - **Cancellation safety.** `usb_reauth` is atomic — once the "off"
//!   write succeeds, the corresponding "on" write completes regardless
//!   of cancellation. Otherwise we'd leave devices unauthorized.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use garden_common::storage::{ConnectivityStatus, ConnectivityWarning, RecoveryAction};

use super::probe::{probe, ProbeData, ProbeOutcome, ProbeVerdict, SYSFS_ROOT};

// ============================================================================
// Configuration
// ============================================================================

/// Tunable knobs for recovery actions and the retry budget.
///
/// Defaults match STORAGE-0019's recommendations: 1 rescan + 1 reauth
/// per device per minute, with conservative settle times that give
/// USB re-enumeration room to complete.
#[derive(Debug, Clone)]
pub struct RecoveryConfig {
    /// Maximum attempts per action per device within `budget_window`.
    pub max_attempts_per_action: u32,
    /// Sliding window for the retry budget.
    pub budget_window: Duration,
    /// Settle time after a SCSI rescan before re-probing.
    pub rescan_settle: Duration,
    /// How long to keep the USB endpoint deauthorized before
    /// re-authorizing. The bridge needs a beat to drop its state.
    pub reauth_off: Duration,
    /// Settle time after re-authorizing before re-probing. USB
    /// re-enumeration takes a couple of seconds on most hubs.
    pub reauth_settle: Duration,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            max_attempts_per_action: 1,
            budget_window: Duration::from_secs(60),
            rescan_settle: Duration::from_secs(1),
            reauth_off: Duration::from_secs(2),
            reauth_settle: Duration::from_secs(3),
        }
    }
}

// ============================================================================
// Retry budget
// ============================================================================

/// Per-device action history, keyed on a stable identifier.
///
/// Devices are keyed on USB port path when available (`"2-3.4"`),
/// falling back to the block device basename when not. The USB port
/// stays stable across the `sdc → sdd` re-numbering that re-auth
/// causes; the basename is used for non-USB devices that we wouldn't
/// re-auth anyway.
#[derive(Debug, Default)]
struct DeviceHistory {
    rescans: Vec<Instant>,
    reauths: Vec<Instant>,
}

impl DeviceHistory {
    fn record_rescan(&mut self) {
        self.rescans.push(Instant::now());
    }
    fn record_reauth(&mut self) {
        self.reauths.push(Instant::now());
    }

    fn rescans_in_window(&self, window: Duration) -> usize {
        let cutoff = Instant::now().checked_sub(window);
        self.rescans
            .iter()
            .filter(|t| cutoff.map(|c| **t >= c).unwrap_or(true))
            .count()
    }

    fn reauths_in_window(&self, window: Duration) -> usize {
        let cutoff = Instant::now().checked_sub(window);
        self.reauths
            .iter()
            .filter(|t| cutoff.map(|c| **t >= c).unwrap_or(true))
            .count()
    }
}

/// Tracks recovery-attempt history across all devices the orchestrator
/// has seen. Thread-safe; intended to be wrapped in `Arc` for sharing
/// across the storage pipeline.
#[derive(Debug, Default)]
pub struct RecoveryBudget {
    config: RecoveryConfig,
    history: Mutex<HashMap<String, DeviceHistory>>,
}

impl RecoveryBudget {
    pub fn new(config: RecoveryConfig) -> Self {
        Self {
            config,
            history: Mutex::new(HashMap::new()),
        }
    }

    pub fn config(&self) -> &RecoveryConfig {
        &self.config
    }

    /// Whether a SCSI rescan is allowed for this device key right now.
    pub fn rescan_allowed(&self, device_key: &str) -> bool {
        let history = self.history.lock().expect("history mutex poisoned");
        let attempts = history
            .get(device_key)
            .map(|h| h.rescans_in_window(self.config.budget_window))
            .unwrap_or(0);
        (attempts as u32) < self.config.max_attempts_per_action
    }

    /// Whether a USB re-auth is allowed for this device key right now.
    pub fn reauth_allowed(&self, device_key: &str) -> bool {
        let history = self.history.lock().expect("history mutex poisoned");
        let attempts = history
            .get(device_key)
            .map(|h| h.reauths_in_window(self.config.budget_window))
            .unwrap_or(0);
        (attempts as u32) < self.config.max_attempts_per_action
    }

    fn record_rescan(&self, device_key: &str) {
        let mut history = self.history.lock().expect("history mutex poisoned");
        history.entry(device_key.to_string()).or_default().record_rescan();
    }

    fn record_reauth(&self, device_key: &str) {
        let mut history = self.history.lock().expect("history mutex poisoned");
        history.entry(device_key.to_string()).or_default().record_reauth();
    }
}

// ============================================================================
// Action primitives
// ============================================================================

/// Issue a SCSI rescan against `/sys/block/<dev>/device/rescan`.
///
/// Synchronous write; the kernel re-issues SCSI commands immediately.
/// The orchestrator should sleep `rescan_settle` afterwards before
/// re-probing.
pub fn scsi_rescan(sysfs_root: &Path, device_basename: &str) -> Result<()> {
    let path = sysfs_root
        .join("block")
        .join(device_basename)
        .join("device")
        .join("rescan");
    write_one(&path).with_context(|| format!("scsi rescan on /dev/{device_basename}"))?;
    debug!(device = %device_basename, path = %path.display(), "scsi rescan issued");
    Ok(())
}

/// Re-authorize a USB device, soft-replug style.
///
/// Writes `0` to `<sysfs_root>/bus/usb/devices/<usb_port>/authorized`,
/// waits `off_duration`, writes `1`, waits `settle_duration`, then
/// returns. The two writes are atomic with respect to cancellation —
/// once the deauthorize write succeeds, the reauthorize write
/// completes even if the cancellation token fires, so the device
/// never gets stuck unauthorized.
///
/// `cancel` is honored only at the boundaries (before deauth and after
/// reauth). The 2-3 second window between writes is short enough that
/// uncancellable behavior is acceptable.
pub async fn usb_reauth(
    sysfs_root: &Path,
    usb_port: &str,
    off_duration: Duration,
    settle_duration: Duration,
    cancel: &CancellationToken,
) -> Result<()> {
    if cancel.is_cancelled() {
        anyhow::bail!("usb reauth cancelled before start");
    }

    let auth_path = authorized_path(sysfs_root, usb_port);
    if !auth_path.exists() {
        anyhow::bail!(
            "usb authorized sysfs entry not found for port {usb_port}: {}",
            auth_path.display()
        );
    }

    // Deauthorize. After this point, we MUST reauthorize regardless of
    // cancellation — leaving the device unauthorized would silently
    // disable it until the next reboot or external replug.
    write_authorized(&auth_path, false)
        .with_context(|| format!("deauthorize usb port {usb_port}"))?;
    info!(port = %usb_port, "usb device deauthorized");

    tokio::time::sleep(off_duration).await;

    write_authorized(&auth_path, true)
        .with_context(|| format!("reauthorize usb port {usb_port}"))?;
    info!(port = %usb_port, "usb device reauthorized");

    // Settle wait IS cancellable — the device is back in the
    // authorized state, so cancellation here just means "skip the
    // settle and re-probe immediately".
    tokio::select! {
        _ = tokio::time::sleep(settle_duration) => {}
        _ = cancel.cancelled() => {
            debug!(port = %usb_port, "usb reauth settle cancelled, returning early");
        }
    }

    Ok(())
}

fn authorized_path(sysfs_root: &Path, usb_port: &str) -> PathBuf {
    sysfs_root
        .join("bus")
        .join("usb")
        .join("devices")
        .join(usb_port)
        .join("authorized")
}

fn write_authorized(path: &Path, authorized: bool) -> Result<()> {
    let value = if authorized { "1" } else { "0" };
    write_value(path, value)
}

fn write_one(path: &Path) -> Result<()> {
    write_value(path, "1")
}

/// Write a value to a sysfs file, falling back to `sudo` when direct
/// write is denied by permissions. Mirrors STORAGE-0018's pattern in
/// `remove_stale_device` for consistency.
fn write_value(path: &Path, value: &str) -> Result<()> {
    if std::fs::write(path, value).is_ok() {
        return Ok(());
    }

    // Direct write failed — likely permissions. Try sudo.
    let cmd = format!("echo {value} > {}", path.display());
    let output = crate::infra::storage::subprocess::run_command_timed_sync(
        "sudo",
        &["sh", "-c", &cmd],
        Duration::from_secs(5),
    )
    .with_context(|| format!("sudo write {value} to {}", path.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "sudo write {value} to {} failed: {}",
            path.display(),
            stderr.trim()
        );
    }
    Ok(())
}

// ============================================================================
// Orchestrator
// ============================================================================

/// Run the recovery escalation for one device based on its initial
/// probe verdict. Returns a [`ConnectivityStatus`] describing what
/// happened, suitable for attaching to the device's `MediumSnapshot`.
///
/// The escalation:
///
/// 1. If the verdict is `Healthy` or `EmptyEnclosure`, return
///    immediately — recovery either isn't needed or wouldn't help.
/// 2. If `TryRescanThenReauth`, attempt SCSI rescan first; if the
///    re-probe still fails, attempt USB re-auth.
/// 3. If `TryReauthOnly`, skip rescan and go straight to re-auth.
/// 4. Each action consumes a slot in the retry budget; if the budget
///    is exhausted, the action is skipped and the next escalation step
///    runs.
///
/// `device_basename` is `"sdc"`, `"sdb1"`, etc. The orchestrator uses
/// it for SCSI rescan and as a fallback identity for the budget when
/// no USB port is available.
pub async fn run_recovery(
    sysfs_root: &Path,
    device_basename: &str,
    initial: ProbeOutcome,
    budget: &RecoveryBudget,
    cancel: &CancellationToken,
) -> ConnectivityStatus {
    let started = Instant::now();
    let initial_warnings = initial.warnings.clone();

    // Happy path — return without modifying anything.
    if !initial.recoverable() {
        return ConnectivityStatus {
            recoveries_attempted: 0,
            recovered_via: None,
            duration_ms: 0,
            residual_warnings: initial_warnings,
        };
    }

    let device_key = identity_for_budget(&initial, device_basename);
    let mut attempts: u32 = 0;
    let mut recovered_via: Option<RecoveryAction> = None;

    let try_rescan = matches!(initial.verdict, ProbeVerdict::TryRescanThenReauth);

    if try_rescan {
        if cancel.is_cancelled() {
            return finalize(started, attempts, recovered_via, initial_warnings);
        }
        if budget.rescan_allowed(&device_key) {
            attempts += 1;
            budget.record_rescan(&device_key);
            match scsi_rescan(sysfs_root, device_basename) {
                Ok(()) => {
                    tokio::select! {
                        _ = tokio::time::sleep(budget.config.rescan_settle) => {}
                        _ = cancel.cancelled() => {
                            return finalize(started, attempts, recovered_via, initial_warnings);
                        }
                    }
                    let after = probe(sysfs_root, device_basename);
                    if !after.recoverable() {
                        recovered_via = Some(RecoveryAction::ScsiRescan);
                        return finalize(
                            started,
                            attempts,
                            recovered_via,
                            merge_warnings(&initial_warnings, &after.warnings),
                        );
                    }
                }
                Err(e) => {
                    warn!(device = %device_basename, error = %e, "scsi rescan failed");
                }
            }
        } else {
            debug!(device = %device_basename, "scsi rescan budget exhausted, skipping");
        }
    }

    // Reauth path: applies to both TryReauthOnly and the fallthrough
    // from a failed rescan.
    if cancel.is_cancelled() {
        return finalize(started, attempts, recovered_via, initial_warnings);
    }

    let Some(usb_port) = initial.data.usb_port.as_deref() else {
        debug!(
            device = %device_basename,
            "no USB port available, cannot reauth — surfacing as unrecovered"
        );
        return finalize(started, attempts, recovered_via, initial_warnings);
    };

    if !budget.reauth_allowed(&device_key) {
        debug!(
            device = %device_basename,
            port = %usb_port,
            "usb reauth budget exhausted, skipping"
        );
        return finalize(started, attempts, recovered_via, initial_warnings);
    }

    attempts += 1;
    budget.record_reauth(&device_key);
    match usb_reauth(
        sysfs_root,
        usb_port,
        budget.config.reauth_off,
        budget.config.reauth_settle,
        cancel,
    )
    .await
    {
        Ok(()) => {
            // Re-probe after reauth. Note: device basename may have
            // changed (sdc → sdd). The orchestrator's caller is
            // responsible for re-discovering the device by topology
            // identity if needed; here we just re-probe under the
            // original name and report what we see.
            let after = probe(sysfs_root, device_basename);
            if !after.recoverable() {
                recovered_via = Some(RecoveryAction::UsbReauth);
            }
            finalize(
                started,
                attempts,
                recovered_via,
                merge_warnings(&initial_warnings, &after.warnings),
            )
        }
        Err(e) => {
            warn!(
                device = %device_basename,
                port = %usb_port,
                error = %e,
                "usb reauth failed"
            );
            finalize(started, attempts, recovered_via, initial_warnings)
        }
    }
}

fn identity_for_budget(initial: &ProbeOutcome, device_basename: &str) -> String {
    initial
        .data
        .usb_port
        .clone()
        .unwrap_or_else(|| format!("dev:{device_basename}"))
}

fn finalize(
    started: Instant,
    attempts: u32,
    recovered_via: Option<RecoveryAction>,
    warnings: Vec<ConnectivityWarning>,
) -> ConnectivityStatus {
    ConnectivityStatus {
        recoveries_attempted: attempts,
        recovered_via,
        duration_ms: started.elapsed().as_millis() as u64,
        residual_warnings: warnings,
    }
}

fn merge_warnings(
    initial: &[ConnectivityWarning],
    after: &[ConnectivityWarning],
) -> Vec<ConnectivityWarning> {
    // Prefer the post-recovery warning set when available — it
    // reflects the device's current state. The initial warnings are
    // preserved as historical context only when the post-recovery
    // probe didn't surface them.
    if !after.is_empty() {
        after.to_vec()
    } else {
        initial.to_vec()
    }
}

// ============================================================================
// Production entry point
// ============================================================================

/// Convenience wrapper that runs recovery against the real `/sys`.
///
/// Production callers use this; tests use [`run_recovery`] directly
/// with a synthetic sysfs tree.
pub async fn run_recovery_production(
    device_basename: &str,
    initial: ProbeOutcome,
    budget: &RecoveryBudget,
    cancel: &CancellationToken,
) -> ConnectivityStatus {
    run_recovery(Path::new(SYSFS_ROOT), device_basename, initial, budget, cancel).await
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Build a synthetic sysfs tree with both `block/<dev>/...` and
    /// `bus/usb/devices/<port>/authorized` entries so recovery has
    /// something to write to. Tests mirror the production layout
    /// exactly so the same code paths exercise the test fixtures.
    struct SyntheticSysfs {
        _root: TempDir,
        path: PathBuf,
    }

    impl SyntheticSysfs {
        fn new() -> Self {
            let root = TempDir::new().unwrap();
            let path = root.path().to_path_buf();
            fs::create_dir_all(path.join("block")).unwrap();
            fs::create_dir_all(path.join("bus").join("usb").join("devices")).unwrap();
            Self { _root: root, path }
        }

        fn add_device(
            &self,
            dev: &str,
            size_blocks: u64,
            ioerr_cnt: u64,
            iotmo_cnt: u64,
            state: &str,
            usb_port: Option<&str>,
        ) {
            let block_dir = self.path.join("block").join(dev);
            fs::create_dir_all(&block_dir).unwrap();
            fs::write(block_dir.join("size"), size_blocks.to_string()).unwrap();
            let device_dir = block_dir.join("device");
            fs::create_dir_all(&device_dir).unwrap();
            fs::write(device_dir.join("ioerr_cnt"), ioerr_cnt.to_string()).unwrap();
            fs::write(device_dir.join("iotmo_cnt"), iotmo_cnt.to_string()).unwrap();
            fs::write(device_dir.join("state"), state).unwrap();
            // Rescan target.
            fs::write(device_dir.join("rescan"), "0").unwrap();

            if let Some(port) = usb_port {
                let port_dir = self.path.join("bus").join("usb").join("devices").join(port);
                fs::create_dir_all(&port_dir).unwrap();
                fs::write(port_dir.join("authorized"), "1").unwrap();
            }
        }

        fn read_authorized(&self, port: &str) -> Option<String> {
            let p = self
                .path
                .join("bus")
                .join("usb")
                .join("devices")
                .join(port)
                .join("authorized");
            fs::read_to_string(p).ok().map(|s| s.trim().to_string())
        }

        fn set_size(&self, dev: &str, size_blocks: u64) {
            fs::write(
                self.path.join("block").join(dev).join("size"),
                size_blocks.to_string(),
            )
            .unwrap();
        }

        fn set_ioerr(&self, dev: &str, count: u64) {
            fs::write(
                self.path
                    .join("block")
                    .join(dev)
                    .join("device")
                    .join("ioerr_cnt"),
                count.to_string(),
            )
            .unwrap();
        }
    }

    fn fast_config() -> RecoveryConfig {
        // Speed up tests by collapsing the settle/off durations.
        RecoveryConfig {
            max_attempts_per_action: 1,
            budget_window: Duration::from_secs(60),
            rescan_settle: Duration::from_millis(1),
            reauth_off: Duration::from_millis(1),
            reauth_settle: Duration::from_millis(1),
        }
    }

    /// Construct a `ProbeOutcome` with explicit fields, bypassing the
    /// probe's sysfs-walking USB-port resolution. Probe-side tests
    /// cover that path on Unix; here we focus on the orchestrator.
    fn make_outcome(
        verdict: ProbeVerdict,
        size_blocks: u64,
        io_errors: u64,
        scsi_state: Option<&str>,
        usb_port: Option<&str>,
    ) -> ProbeOutcome {
        ProbeOutcome {
            verdict,
            data: ProbeData {
                size_blocks,
                io_errors,
                io_timeouts: 0,
                scsi_state: scsi_state.map(|s| s.to_string()),
                usb_port: usb_port.map(|s| s.to_string()),
            },
            warnings: if io_errors > 0 {
                vec![ConnectivityWarning::PriorIoErrors { count: io_errors }]
            } else {
                vec![]
            },
        }
    }

    #[test]
    fn scsi_rescan_writes_one_to_rescan_file() {
        let sysfs = SyntheticSysfs::new();
        sysfs.add_device("sdc", 0, 1, 0, "running", Some("2-3.4"));
        scsi_rescan(&sysfs.path, "sdc").unwrap();
        let rescan = fs::read_to_string(
            sysfs
                .path
                .join("block")
                .join("sdc")
                .join("device")
                .join("rescan"),
        )
        .unwrap();
        assert_eq!(rescan, "1");
    }

    #[tokio::test]
    async fn usb_reauth_toggles_authorized_off_then_on() {
        let sysfs = SyntheticSysfs::new();
        sysfs.add_device("sdc", 0, 1, 0, "running", Some("2-3.4"));
        let cancel = CancellationToken::new();

        usb_reauth(
            &sysfs.path,
            "2-3.4",
            Duration::from_millis(5),
            Duration::from_millis(1),
            &cancel,
        )
        .await
        .unwrap();

        assert_eq!(sysfs.read_authorized("2-3.4").as_deref(), Some("1"));
    }

    #[tokio::test]
    async fn usb_reauth_completes_reauthorize_even_if_settle_cancelled() {
        // Cancellation during the settle wait must NOT leave the
        // device unauthorized — the second write happens before the
        // settle.
        let sysfs = SyntheticSysfs::new();
        sysfs.add_device("sdc", 0, 1, 0, "running", Some("2-3.4"));
        let cancel = CancellationToken::new();

        let port = "2-3.4";
        let sysfs_path = sysfs.path.clone();
        let cancel_for_task = cancel.clone();
        let task = tokio::spawn(async move {
            usb_reauth(
                &sysfs_path,
                port,
                Duration::from_millis(5),
                Duration::from_millis(500),
                &cancel_for_task,
            )
            .await
        });

        // Let the reauth start, complete its toggle, and enter settle.
        tokio::time::sleep(Duration::from_millis(20)).await;
        cancel.cancel();

        let result = task.await.unwrap();
        result.unwrap();
        assert_eq!(
            sysfs.read_authorized("2-3.4").as_deref(),
            Some("1"),
            "device must end in authorized state after cancelled settle"
        );
    }

    #[tokio::test]
    async fn usb_reauth_refuses_when_cancelled_before_start() {
        let sysfs = SyntheticSysfs::new();
        sysfs.add_device("sdc", 0, 1, 0, "running", Some("2-3.4"));
        let cancel = CancellationToken::new();
        cancel.cancel();
        let err = usb_reauth(
            &sysfs.path,
            "2-3.4",
            Duration::from_millis(1),
            Duration::from_millis(1),
            &cancel,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("cancelled"));
        // Device never deauthorized.
        assert_eq!(sysfs.read_authorized("2-3.4").as_deref(), Some("1"));
    }

    #[tokio::test]
    async fn usb_reauth_errors_when_port_does_not_exist() {
        let sysfs = SyntheticSysfs::new();
        let cancel = CancellationToken::new();
        let err = usb_reauth(
            &sysfs.path,
            "1-1",
            Duration::from_millis(1),
            Duration::from_millis(1),
            &cancel,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("authorized sysfs entry not found"));
    }

    #[test]
    fn budget_allows_first_action_then_blocks_second_in_window() {
        let budget = RecoveryBudget::new(fast_config());
        assert!(budget.rescan_allowed("2-3.4"));
        budget.record_rescan("2-3.4");
        assert!(!budget.rescan_allowed("2-3.4"));
        // Different device: independent budget.
        assert!(budget.rescan_allowed("2-3.2"));
    }

    #[test]
    fn budget_is_per_action_independent() {
        let budget = RecoveryBudget::new(fast_config());
        budget.record_rescan("2-3.4");
        // Reauth budget for the same device should still have room.
        assert!(budget.reauth_allowed("2-3.4"));
        budget.record_reauth("2-3.4");
        assert!(!budget.reauth_allowed("2-3.4"));
    }

    #[tokio::test]
    async fn happy_path_no_recovery_needed() {
        let sysfs = SyntheticSysfs::new();
        sysfs.add_device("sdb", 976773168, 0, 0, "running", Some("2-3.2"));
        let budget = RecoveryBudget::new(fast_config());
        let cancel = CancellationToken::new();

        let initial = probe(&sysfs.path, "sdb");
        let status = run_recovery(&sysfs.path, "sdb", initial, &budget, &cancel).await;
        assert_eq!(status.recoveries_attempted, 0);
        assert!(status.recovered_via.is_none());
    }

    #[tokio::test]
    async fn rescan_succeeds_when_size_appears_after_action() {
        // Probe sees size=0 + ioerr_cnt>0 → TryRescanThenReauth.
        // After "rescan" we simulate the kernel re-reading capacity
        // by writing a real size to the synthetic tree — and the
        // re-probe sees the real size, so verdict becomes Healthy.
        let sysfs = SyntheticSysfs::new();
        sysfs.add_device("sdc", 0, 1, 0, "running", Some("2-3.4"));
        let budget = RecoveryBudget::new(fast_config());
        let cancel = CancellationToken::new();

        let initial = probe(&sysfs.path, "sdc");
        assert_eq!(initial.verdict, ProbeVerdict::TryRescanThenReauth);

        // Simulate: the rescan in the orchestrator will write 1 to
        // rescan; the kernel would then re-read capacity. We can't
        // emulate that side-effect in a pure-fs test, so we
        // pre-arrange the post-rescan size right before the call.
        // The orchestrator runs synchronously enough that the value
        // is in place when it re-probes.
        sysfs.set_size("sdc", 500118192);
        sysfs.set_ioerr("sdc", 0);

        let status = run_recovery(&sysfs.path, "sdc", initial, &budget, &cancel).await;
        assert_eq!(status.recoveries_attempted, 1);
        assert_eq!(status.recovered_via, Some(RecoveryAction::ScsiRescan));
    }

    #[tokio::test]
    async fn rescan_failure_falls_through_to_reauth() {
        // size stays 0 after rescan → orchestrator escalates to reauth.
        // Construct the ProbeOutcome directly so the test doesn't
        // depend on sysfs-walking USB-port resolution (which needs
        // symlinks not portable to Windows).
        let sysfs = SyntheticSysfs::new();
        sysfs.add_device("sdc", 0, 1, 0, "running", Some("2-3.4"));
        let budget = RecoveryBudget::new(fast_config());
        let cancel = CancellationToken::new();

        let initial = make_outcome(
            ProbeVerdict::TryRescanThenReauth,
            0,
            1,
            Some("running"),
            Some("2-3.4"),
        );

        let status = run_recovery(&sysfs.path, "sdc", initial, &budget, &cancel).await;
        // Rescan + reauth both attempted because the synthetic fs
        // doesn't model real kernel state, so size stays 0 across
        // both probes — the orchestrator can't conclude success.
        assert_eq!(status.recoveries_attempted, 2);
        assert!(status.recovered_via.is_none());
        // Reauth completed — device is back to authorized.
        assert_eq!(sysfs.read_authorized("2-3.4").as_deref(), Some("1"));
    }

    #[tokio::test]
    async fn reauth_only_path_skips_rescan() {
        // SCSI state offline → TryReauthOnly. Orchestrator goes
        // straight to reauth without consuming a rescan slot.
        let sysfs = SyntheticSysfs::new();
        sysfs.add_device("sdc", 0, 0, 0, "offline", Some("2-3.4"));
        let budget = RecoveryBudget::new(fast_config());
        let cancel = CancellationToken::new();

        let initial = make_outcome(
            ProbeVerdict::TryReauthOnly,
            0,
            0,
            Some("offline"),
            Some("2-3.4"),
        );

        let status = run_recovery(&sysfs.path, "sdc", initial, &budget, &cancel).await;
        assert_eq!(status.recoveries_attempted, 1);
        assert!(budget.rescan_allowed("2-3.4"), "rescan budget untouched");
        assert!(!budget.reauth_allowed("2-3.4"), "reauth budget consumed");
    }

    #[tokio::test]
    async fn empty_enclosure_does_not_attempt_recovery() {
        let sysfs = SyntheticSysfs::new();
        sysfs.add_device("sdc", 0, 0, 0, "running", Some("2-3.4"));
        let budget = RecoveryBudget::new(fast_config());
        let cancel = CancellationToken::new();

        let initial = probe(&sysfs.path, "sdc");
        assert_eq!(initial.verdict, ProbeVerdict::EmptyEnclosure);

        let status = run_recovery(&sysfs.path, "sdc", initial, &budget, &cancel).await;
        assert_eq!(status.recoveries_attempted, 0);
        assert!(budget.rescan_allowed("2-3.4"), "no slot consumed");
        assert!(budget.reauth_allowed("2-3.4"));
    }

    #[tokio::test]
    async fn no_usb_port_skips_reauth_phase() {
        // Device on a non-USB bus (e.g. SATA) reports no usb_port.
        // The orchestrator must still try rescan but cannot reauth.
        let sysfs = SyntheticSysfs::new();
        sysfs.add_device("sda", 0, 1, 0, "running", None);
        let budget = RecoveryBudget::new(fast_config());
        let cancel = CancellationToken::new();

        let initial = probe(&sysfs.path, "sda");
        assert_eq!(initial.verdict, ProbeVerdict::TryRescanThenReauth);

        let status = run_recovery(&sysfs.path, "sda", initial, &budget, &cancel).await;
        // Rescan attempted (1), reauth not attempted (no port).
        assert_eq!(status.recoveries_attempted, 1);
    }

    #[tokio::test]
    async fn budget_exhausted_skips_action_and_continues() {
        let sysfs = SyntheticSysfs::new();
        sysfs.add_device("sdc", 0, 1, 0, "running", Some("2-3.4"));
        let budget = RecoveryBudget::new(fast_config());
        let cancel = CancellationToken::new();

        // Pre-consume the rescan budget for this device.
        budget.record_rescan("2-3.4");

        let initial = make_outcome(
            ProbeVerdict::TryRescanThenReauth,
            0,
            1,
            Some("running"),
            Some("2-3.4"),
        );
        let status = run_recovery(&sysfs.path, "sdc", initial, &budget, &cancel).await;
        // Rescan skipped, but reauth still attempted.
        assert_eq!(status.recoveries_attempted, 1);
        assert_eq!(sysfs.read_authorized("2-3.4").as_deref(), Some("1"));
    }
}
