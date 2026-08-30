//! Restore rehearsal (J2): does the newest checkpoint actually boot?
//! The proof loop of "backs up to proof" — select the checkpoint, unpack
//! into scratch, run the offering's image against the restored volumes
//! in isolation (no published ports, a `zen-rehearsal-` container that
//! never touches the registry), hold the window, report green/red, and
//! clean up after itself whatever the verdict.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;

/// One rehearsal's full verdict.
#[derive(Debug, Clone, Serialize)]
pub struct RehearsalReport {
    pub name: String,
    /// green = checkpoint verified, restored, and the service booted.
    pub green: bool,
    /// found | planted — where the proof ran.
    pub checkpoint: String,
    pub files: usize,
    pub bytes: u64,
    pub hash: String,
    pub container_ran_secs: u64,
    pub container_state: String,
    pub duration_secs: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Checkpoint selection (the capture pipeline's ledger).
pub type SelectFn = Box<dyn Fn(&str) -> Result<PathBuf, String> + Send>;
/// Restore a checkpoint into a scratch dir.
pub type RestoreFn = Box<dyn Fn(&Path, &Path) -> Result<(usize, String), String> + Send>;

pub struct RehearsalDeps {
    /// The placed offering's world (docker rehearses; null refuses).
    pub world: Arc<dyn crate::garden::runtime::Runtime>,
    pub select_checkpoint: SelectFn,
    pub restore_into: RestoreFn,
}

pub async fn rehearse(
    name: &str,
    spec: &crate::garden::model::WorkloadSpec,
    deps: RehearsalDeps,
    scratch_root: &Path,
    wait_secs: u64,
) -> RehearsalReport {
    let started = std::time::Instant::now();
    let run_id = uuid::Uuid::now_v7().to_string();
    let mut report = RehearsalReport {
        name: name.to_string(),
        green: false,
        checkpoint: String::new(),
        files: 0,
        bytes: 0,
        hash: String::new(),
        container_ran_secs: 0,
        container_state: String::new(),
        duration_secs: 0,
        error: None,
    };

    // 1. Select the newest checkpoint.
    let checkpoint = match (deps.select_checkpoint)(name) {
        Ok(cp) => cp,
        Err(e) => {
            report.error = Some(e);
            report.duration_secs = started.elapsed().as_secs();
            return report;
        }
    };
    report.checkpoint = checkpoint.display().to_string();

    // 2. Restore into scratch (the proof never touches the live offering).
    let scratch = scratch_root.join(&run_id);
    match (deps.restore_into)(&checkpoint, &scratch) {
        Ok((files, hash)) => {
            report.files = files;
            report.bytes = scratch_size(&scratch);
            report.hash = hash;
        }
        Err(e) => {
            report.error = Some(format!("restore failed: {e}"));
            report.duration_secs = started.elapsed().as_secs();
            let _ = std::fs::remove_dir_all(scratch_root.join(&run_id));
            return report;
        }
    }
    if report.files == 0 {
        report.error = Some("the checkpoint restored nothing — nothing to boot".into());
        report.duration_secs = started.elapsed().as_secs();
        let _ = std::fs::remove_dir_all(scratch_root.join(&run_id));
        return report;
    }

    // 3. Boot the image against the restored volumes, isolated.
    let fate = deps
        .world
        .rehearse_run(name, spec, &scratch, wait_secs)
        .await;
    match fate {
        Some(fate) => {
            report.container_ran_secs = fate.ran_secs;
            report.container_state = fate.state.clone();
            report.green = fate.stayed_running && fate.exit_code.unwrap_or(0) == 0;
            if !fate.stayed_running {
                report.error = Some(format!(
                    "the container did not stay up (state: {}, exit: {:?})",
                    fate.state, fate.exit_code
                ));
            }
        }
        None => {
            report.error = Some("the offering's world cannot rehearse".into());
        }
    }

    // 4. The proof never lingers: scratch cleaned whatever the verdict.
    let _ = std::fs::remove_dir_all(scratch_root.join(&run_id));
    report.duration_secs = started.elapsed().as_secs();
    tracing::info!(offering = %name, green = report.green, secs = report.duration_secs, "restore rehearsal complete");
    report
}

fn scratch_size(root: &Path) -> u64 {
    let mut total = 0u64;
    fn walk(dir: &Path, total: &mut u64) {
        let Ok(read) = std::fs::read_dir(dir) else { return };
        for entry in read.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                walk(&entry.path(), total);
            } else {
                *total += meta.len();
            }
        }
    }
    walk(root, &mut total);
    total
}
