//! The Checkpoint entity (ADR-0005 §3): an offering's state, committed
//! atomically onto dumb storage. A directory is NOT a checkpoint until
//! its `manifest.json` exists beside a verified archive — the manifest
//! is the commit marker, on this stone and on any sink.
//!
//! Identity: (offering fqn, run). Commit is one rename: everything
//! lands in `{run}.partial/`, ONE rename makes it `{run}/`.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Checkpoints kept per offering per location (§3 rotation default).
pub const CHECKPOINT_KEEP: usize = 5;
/// Directory under a sink bank that receives ferried checkpoints.
pub const SINK_CHECKPOINT_DIR: &str = "zen-garden/checkpoints";

/// Local checkpoint ledger: `~/.zen-garden/checkpoints/{fqn}/{run}/`.
pub fn checkpoints_root() -> PathBuf {
    dirs_or_home().join(".zen-garden").join("checkpoints")
}

fn dirs_or_home() -> PathBuf {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// The slugged offering directory under any checkpoint root.
pub fn dir_for(root: &Path, fqn: &str) -> PathBuf {
    root.join(crate::garden::directory::slug(fqn))
}

/// What opening a directory reveals. A checkpoint is committed or it
/// is nothing — `.partial` staging is nobody's source of truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opened {
    Committed,
    NotACheckpoint,
}

/// A committed checkpoint, opened and provable. Construct through
/// [`Checkpoint::open`] — the type cannot exist for a directory that
/// has not committed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint {
    dir: PathBuf,
}

impl Checkpoint {
    /// Open a directory as a checkpoint: committed only when the
    /// manifest (the commit marker) is present.
    pub fn open(dir: &Path) -> Result<Self, Opened> {
        let is_partial = dir
            .file_name()
            .map(|n| n.to_string_lossy().ends_with(".partial"))
            .unwrap_or(false);
        if dir.is_dir() && !is_partial && dir.join("manifest.json").is_file() {
            Ok(Self { dir: dir.to_path_buf() })
        } else {
            Err(Opened::NotACheckpoint)
        }
    }

    /// The run directory's name (the run id).
    pub fn run(&self) -> String {
        self.dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// The directory itself (for copies and restores).
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The parsed manifest, if it parses. A committed checkpoint whose
    /// manifest does not parse is corruption, said out loud.
    pub fn manifest(&self) -> Result<serde_json::Value, String> {
        serde_json::from_slice(
            &std::fs::read(self.dir.join("manifest.json"))
                .map_err(|e| format!("manifest unreadable: {e}"))?,
        )
        .map_err(|e| format!("manifest unparsable: {e}"))
    }
}

/// What verifying a checkpoint found.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct VerifyReport {
    pub files: usize,
    pub bytes: u64,
}

/// Verify a checkpoint against its embedded manifest (§3): the archive's
/// own hash (decoded), then every tar entry's hash against the manifest.
/// A checkpoint that cannot prove itself is not a checkpoint — refuse
/// loudly, restore nothing.
pub fn verify(checkpoint: &Path) -> Result<VerifyReport, String> {
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(checkpoint.join("manifest.json"))
            .map_err(|e| format!("manifest unreadable: {e}"))?,
    )
    .map_err(|e| format!("manifest unparsable: {e}"))?;

    let archive_file = manifest["archive"]["file"]
        .as_str()
        .unwrap_or("checkpoint.tar.zst")
        .to_string();
    let want_archive = manifest["archive"]["sha256"]
        .as_str()
        .ok_or("manifest declares no archive hash")?
        .to_string();
    let archive_bytes = std::fs::read(checkpoint.join(&archive_file))
        .map_err(|e| format!("archive unreadable: {e}"))?;
    // The manifest hashes the TAR (pack hashed it before compressing):
    // decode first, then prove the decoded bytes.
    let tar_bytes = zstd::stream::decode_all(&archive_bytes[..])
        .map_err(|e| format!("archive decompress: {e}"))?;
    let mut h = Sha256::new();
    h.update(&tar_bytes);
    let got = hex(&h.finalize());
    if got != want_archive {
        return Err(format!(
            "archive hash mismatch: manifest says {want_archive}, archive proves {got}"
        ));
    }

    // The expected file set, by path -> hash.
    let mut expected: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for f in manifest["files"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
        if let (Some(p), Some(s)) = (f["path"].as_str(), f["sha256"].as_str()) {
            expected.insert(p.to_string(), s.to_string());
        }
    }

    let mut archive = tar::Archive::new(&tar_bytes[..]);
    let mut files = 0usize;
    let mut bytes = 0u64;
    for entry in archive.entries().map_err(|e| format!("archive walk: {e}"))? {
        let mut entry = entry.map_err(|e| format!("archive walk: {e}"))?;
        let path = entry
            .path()
            .map_err(|e| format!("archive path: {e}"))?
            .to_string_lossy()
            .into_owned();
        let Some(want) = expected.get(&path) else {
            return Err(format!("archive carries unlisted file '{path}'"));
        };
        let mut data = Vec::new();
        use std::io::Read;
        entry.read_to_end(&mut data).map_err(|e| format!("{path}: {e}"))?;
        let mut fh = Sha256::new();
        fh.update(&data);
        if hex(&fh.finalize()) != want.as_str() {
            return Err(format!("file '{path}' fails its hash"));
        }
        files += 1;
        bytes += data.len() as u64;
    }
    if files != expected.len() {
        return Err(format!(
            "manifest lists {} files, the archive holds {files}",
            expected.len()
        ));
    }
    Ok(VerifyReport { files, bytes })
}

/// Commit the collected file set as a checkpoint: tar → zstd → hash →
/// manifest LAST → one rename (§3). The manifest rides inside the set
/// when the caller includes it; either way the rename is the moment it
/// becomes a checkpoint.
pub fn commit(
    fqn: &str,
    mut files: Vec<(PathBuf, String)>,
    workspace_files: &[(PathBuf, String)],
    checkpoints_root: &Path,
    run_id: &str,
) -> Result<PathBuf, String> {
    files.extend_from_slice(workspace_files);
    // Deterministic order for stable manifests.
    files.sort_by(|a, b| a.1.cmp(&b.1));

    let staged = dir_for(checkpoints_root, fqn).join(format!("{run_id}.partial"));
    std::fs::create_dir_all(&staged).map_err(|e| format!("checkpoint stage failed: {e}"))?;

    // tar the file set, zstd the stream, hash the bytes as they flow.
    let archive_path = staged.join("checkpoint.tar.zst");
    let tar_buffer = tar::Builder::new(Vec::new());
    let mut tar_buffer = tar_buffer;
    for (abs, rel) in &files {
        tar_buffer
            .append_path_with_name(abs, rel)
            .map_err(|e| format!("pack: {}: {e}", rel))?;
    }
    let tar_bytes = tar_buffer.into_inner().map_err(|e| format!("pack: {e}"))?;
    let mut hasher = Sha256::new();
    hasher.update(&tar_bytes);
    let archive_sha = hex(&hasher.finalize());
    let compressed = zstd::stream::encode_all(&tar_bytes[..], 3)
        .map_err(|e| format!("pack compress: {e}"))?;
    std::fs::write(&archive_path, &compressed).map_err(|e| format!("archive write: {e}"))?;

    // The embedded manifest: per-file hashes plus the archive's own.
    let mut manifest = serde_json::json!({
        "fqn": fqn,
        "run": run_id,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "archive": {
            "file": "checkpoint.tar.zst",
            "sha256": archive_sha,
            "bytes": compressed.len(),
        },
        "files": [],
    });
    let file_records: Vec<serde_json::Value> = files
        .iter()
        .map(|(abs, rel)| {
            let mut h = Sha256::new();
            let bytes = std::fs::read(abs).unwrap_or_default();
            h.update(&bytes);
            serde_json::json!({
                "path": rel,
                "sha256": hex(&h.finalize()),
                "bytes": bytes.len(),
            })
        })
        .collect();
    manifest["files"] = serde_json::Value::Array(file_records);
    let manifest_path = staged.join("manifest.json");
    // fs::write closes the handle before we rename the directory
    // (Windows denies renaming a tree with an open file in it).
    let manifest_bytes =
        serde_json::to_vec_pretty(&manifest).map_err(|e| format!("manifest encode: {e}"))?;
    std::fs::write(&manifest_path, manifest_bytes).map_err(|e| format!("manifest write: {e}"))?;

    // The commit: one rename makes it a checkpoint (§3).
    let final_dir = staged.with_file_name(run_id);
    std::fs::rename(&staged, &final_dir).map_err(|e| format!("checkpoint commit rename: {e}"))?;
    Ok(final_dir)
}

/// Keep the newest [`CHECKPOINT_KEEP`] checkpoints in one location.
pub fn rotate(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut runs: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && !p.file_name()
                    .map(|n| n.to_string_lossy().ends_with(".partial"))
                    .unwrap_or(false)
        })
        .collect();
    runs.sort();
    while runs.len() > CHECKPOINT_KEEP {
        let oldest = runs.remove(0);
        let _ = std::fs::remove_dir_all(&oldest);
    }
}

/// Every regular file under `root`, as (absolute, relative-to-root).
pub fn collect_files(root: &Path, rel_root: &Path, out: &mut Vec<(PathBuf, String)>) -> Result<(), String> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_files(&p, rel_root, out)?;
        } else {
            // Forward slashes always: the manifest's paths must match the
            // tar's on every platform (B1 - one shape, many mouths).
            let rel = p
                .strip_prefix(rel_root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR.to_string().as_str(), "/");
            out.push((p.clone(), rel));
        }
    }
    Ok(())
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[test]
    fn a_directory_without_its_manifest_is_not_a_checkpoint() {
        let tmp = std::env::temp_dir().join(format!("zg-cp-{}", uuid::Uuid::now_v7()));
        let run = tmp.join("r1");
        std::fs::create_dir_all(&run).unwrap();
        assert_eq!(Checkpoint::open(&run), Err(Opened::NotACheckpoint), "no manifest, no checkpoint");

        std::fs::write(run.join("manifest.json"), "{}").unwrap();
        let cp = Checkpoint::open(&run).expect("the manifest commits");
        assert_eq!(cp.run(), "r1");

        let partial = tmp.join("r2.partial");
        std::fs::create_dir_all(&partial).unwrap();
        std::fs::write(partial.join("manifest.json"), "{}").unwrap();
        assert_eq!(Checkpoint::open(&partial), Err(Opened::NotACheckpoint), "staging is never truth");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
