//! The Offering Directory (OFFERINGS.md, rehydration contract):
//! **an offering is constituted by its directory.** Record, plan, configs,
//! volumes — one self-contained unit; copy it and you hold the whole thing.
//!
//! Layout (canonical, under `{root}` = `~/.zen-garden/offerings`):
//!   {root}/{slug}/record.json      — active placed record
//!   {root}/{slug}/candidate.json   — adopted candidate (not yet confirmed)
//!   {root}/{slug}/plan.json        — compiled PlacementPlan (when managed)
//!   {root}/{slug}/configs/         — materialized config files
//!   {root}/{slug}/volumes/         — data volumes (nested by design)
//!
//! The legacy consolidated `offerings.json` migrates automatically on
//! first load; its file is renamed `.migrated` after conversion.

use super::registry::{Snapshot, SnapshotStore};
use std::path::{Path, PathBuf};

/// Filesystem-safe slugging of an offering FQN (`ollama::adopted` →
/// `ollama_adopted`). The offering_id inside the record stays the true
/// key; the slug names the directory.
pub fn slug(fqn_or_name: &str) -> String {
    fqn_or_name
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
            _ => '_',
        })
        .collect()
}

/// Canonical paths for one offering's directory.
#[derive(Debug, Clone)]
pub struct OfferingDir {
    pub root: PathBuf,
}

impl OfferingDir {
    pub fn new(base: &Path, name: &str) -> Self {
        Self { root: base.join(slug(name)) }
    }

    pub fn record_json(&self) -> PathBuf {
        self.root.join("record.json")
    }

    pub fn candidate_json(&self) -> PathBuf {
        self.root.join("candidate.json")
    }

    pub fn plan_json(&self) -> PathBuf {
        self.root.join("plan.json")
    }

    pub fn configs(&self) -> PathBuf {
        self.root.join("configs")
    }

    pub fn volumes(&self) -> PathBuf {
        self.root.join("volumes")
    }
}

/// Root of every offering directory, plus knowledge of the legacy file.
#[derive(Debug, Clone)]
pub struct OfferingsRoot {
    pub base: PathBuf,
}

impl OfferingsRoot {
    pub fn new(base: PathBuf) -> Self {
        Self { base }
    }

    pub fn dir_for(&self, name: &str) -> OfferingDir {
        OfferingDir::new(&self.base, name)
    }

    /// Historical pre-directory storage (~/.zen-garden/offerings.json).
    pub fn legacy_file(&self) -> PathBuf {
        let mut p = self.base.clone();
        p.pop();
        p.join("offerings.json")
    }
}

/// SnapshotStore backed by the directory tree.
///
/// - `load`: reads every record/candidate under the root; auto-migrates a
///   legacy consolidated file once (renames it `.migrated`).
/// - `save`: writes each active into `record.json`, each candidate into
///   `candidate.json`; directories that no longer appear lose their JSON
///   but KEEP their volumes — data outlives registration by law.
pub struct DirectoryStore {
    base: PathBuf,
}

impl DirectoryStore {
    pub fn new(base: PathBuf) -> Self {
        Self { base }
    }

    fn persist_one(dir: &OfferingDir, o: &super::model::Offering, is_candidate: bool) {
        if let Err(e) = std::fs::create_dir_all(dir.configs()) {
            tracing::warn!(error = %e, "configs dir create failed");
        }
        if let Err(e) = std::fs::create_dir_all(dir.volumes()) {
            tracing::warn!(error = %e, "volumes dir create failed");
        }
        let record = match serde_json::to_vec_pretty(o) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(error = %e, "record encode failed");
                return;
            }
        };
        let target = if is_candidate { dir.candidate_json() } else { dir.record_json() };
        if let Err(e) = std::fs::write(target, record) {
            tracing::warn!(error = %e, "record write failed");
        }
        // Plan sidecar when managed.
        if let super::model::ModeData::Managed(m) = &o.mode_data
            && let Some(plan) = &m.plan
        {
            match serde_json::to_vec_pretty(plan) {
                Ok(b) => {
                    let _ = std::fs::write(dir.plan_json(), b);
                }
                Err(e) => tracing::warn!(error = %e, "plan encode failed"),
            }
        }
    }
}

impl SnapshotStore for DirectoryStore {
    fn load(&self) -> Option<Snapshot> {
        // One-time legacy migration, transparent to callers.
        let legacy = OfferingsRoot::new(self.base.clone()).legacy_file();
        if legacy.is_file() {
            tracing::info!(path = %legacy.display(), "migrating legacy offerings.json -> directories");
            if let Ok(bytes) = std::fs::read(&legacy)
                && let Ok(snapshot) = serde_json::from_slice::<Snapshot>(&bytes)
            {
                Self { base: self.base.clone() }.save(&snapshot);
            }
            let _ = std::fs::rename(&legacy, legacy.with_extension("json.migrated"));
        }

        let mut active = Vec::new();
        let mut candidates = Vec::new();
        let read = match std::fs::read_dir(&self.base) {
            Ok(r) => r,
            Err(_) => return None,
        };
        for entry in read.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let record = dir.join("record.json");
            let candidate = dir.join("candidate.json");
            if let Ok(bytes) = std::fs::read(&record) {
                match serde_json::from_slice::<super::model::Offering>(&bytes) {
                    Ok(o) => active.push(o),
                    Err(e) => tracing::warn!(
                        path = %record.display(), error = %e, "record unreadable; skipping"
                    ),
                }
            } else if let Ok(bytes) = std::fs::read(&candidate) {
                match serde_json::from_slice::<super::model::Offering>(&bytes) {
                    Ok(o) => candidates.push(o),
                    Err(e) => tracing::warn!(
                        path = %candidate.display(), error = %e, "candidate unreadable; skipping"
                    ),
                }
            }
        }
        Some(Snapshot { active, candidates })
    }

    fn save(&self, snapshot: &Snapshot) {
        let mut live_dirs: Vec<PathBuf> = Vec::new();

        for o in &snapshot.active {
            let dir = OfferingDir::new(&self.base, &o.name);
            Self::persist_one(&dir, o, false);
            live_dirs.push(dir.root);
        }
        for o in &snapshot.candidates {
            let dir = OfferingDir::new(&self.base, &o.name);
            Self::persist_one(&dir, o, true);
            live_dirs.push(dir.root);
        }

        // Prune stale registration files; never touch volumes/configs data.
        if let Ok(read) = std::fs::read_dir(&self.base) {
            for entry in read.flatten() {
                let p = entry.path();
                if !p.is_dir() || live_dirs.contains(&p) {
                    continue;
                }
                let had_record = p.join("record.json").is_file();
                let had_candidate = p.join("candidate.json").is_file();
                if had_record || had_candidate {
                    let _ = std::fs::remove_file(p.join("record.json"));
                    let _ = std::fs::remove_file(p.join("candidate.json"));
                    let _ = std::fs::remove_file(p.join("plan.json"));
                    tracing::info!(dir = %p.display(), "unregistered; volumes preserved");
                }
            }
        }
    }
}
