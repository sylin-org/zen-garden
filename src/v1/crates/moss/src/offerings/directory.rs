//! The Offering Directory (OFFERINGS.md, rehydration contract):
//! **an offering is constituted by its directory.** Record, plan, configs,
//! volumes — one self-contained unit; copy it and you hold the whole thing.
//!
//! Layout (canonical, under `{root}` = `~/.zen-garden/offerings`) mirrors
//! the FQN namespace (glossary::fqn): `{stem}/{instance}/` per offering —
//! identity parsing is path traversal, not string surgery:
//!   {root}/{stem}/{instance}/record.json      — active placed record
//!   {root}/{stem}/{instance}/candidate.json   — adopted candidate (unconfirmed)
//!   {root}/{stem}/{instance}/plan.json        — compiled PlacementPlan
//!   {root}/{stem}/{instance}/configs/         — materialized config files
//!   {root}/{stem}/{instance}/volumes/         — data volumes (nested by design)
//!
//! Migrations handled transparently on load (each renames its source so it
//! runs once):
//!   · legacy consolidated `offerings.json` → directories
//!   · pre-namespace flat directories `{slug}/` → `{stem}/default/`
//!     (their records gain the `::default` spelling)
//!   · flat v2 records → the sectioned v3 schema (`record.rs`;
//!     `*.json.migrated` keeps the evidence)

use super::registry::{Snapshot, SnapshotStore};
use garden_glossary::fqn;
use std::path::{Path, PathBuf};

/// Filesystem-safe slugging of arbitrary strings (`ollama::prod` →
/// `ollama_prod`). Container names still ride this rule (PoC compat);
/// OFFERING DIRECTORIES no longer do — they nest by FQN segments instead,
/// which are grammar-guaranteed fs-safe.
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
    /// `{base}/{stem}/{instance}`. Names failing the grammar fall back to
    /// slugged flat storage with a loud warning — they cannot come from a
    /// validated surface, but on-disk tolerance beats panic during recovery.
    pub fn new(base: &Path, name: &str) -> Self {
        Self {
            root: leaf_dir(base, name),
        }
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

/// The offering's leaf directory: `{base}/{stem}/{instance}`, resolved
/// through the FQN grammar. Invalid names degrade to one slugged flat
/// directory (visible in logs, never a panic).
fn leaf_dir(base: &Path, name: &str) -> PathBuf {
    match fqn::parse(name) {
        Ok((stem, instance)) => base.join(stem).join(instance),
        Err(e) => {
            tracing::warn!(name = %name, error = %e, "offering name off-grammar; using slug fallback");
            base.join(slug(name))
        }
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
/// - `load`: reads every record/candidate under the root; auto-migrates
///   legacy layouts once (consolidated file; flat pre-namespace dirs).
/// - `save`: writes each active into `record.json`, each candidate into
///   `candidate.json`; leaf directories that no longer appear lose their
///   JSON but KEEP their volumes — data outlives registration by law.
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
        let record = match serde_json::to_vec_pretty(&super::record::OfferingRecord::from_domain(o)) {
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

    /// Read ONE offering-leaf directory's registration files into the
    /// snapshot vectors (record.json → active, candidate.json → candidates).
    /// Shared by canonical traversal and same-pass migrated trees.
    fn push_leaf(
        dir: &Path,
        active: &mut Vec<super::model::Offering>,
        candidates: &mut Vec<super::model::Offering>,
    ) {
        Self::read_registration(&dir.join("record.json"), false, active, candidates);
        Self::read_registration(&dir.join("candidate.json"), true, active, candidates);
    }

    /// Read one registration file. v3 (sectioned) parses directly; the v2
    /// flat shape migrates in place — source renamed `*.json.migrated`
    /// (the evidence-preserving pattern), sectioned truth written fresh —
    /// and still lands in this pass (a migration never costs an offering
    /// a boot of visibility).
    fn read_registration(
        path: &Path,
        is_candidate: bool,
        active: &mut Vec<super::model::Offering>,
        candidates: &mut Vec<super::model::Offering>,
    ) {
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };
        let parsed = if let Ok(rec) = serde_json::from_slice::<super::record::OfferingRecord>(&bytes)
        {
            Some(rec.into_domain())
        } else {
            match serde_json::from_slice::<super::model::Offering>(&bytes) {
                Ok(mut o) => {
                    Self::migrate_registration(path, &mut o, is_candidate);
                    Some(o)
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "registration unreadable; skipping");
                    None
                }
            }
        };
        if let Some(o) = parsed {
            if is_candidate {
                candidates.push(o);
            } else {
                active.push(o);
            }
        }
    }

    /// One-time v2→v3 move for a registration file and its plan sidecar:
    /// the old bytes are renamed aside (never destroyed), the sectioned
    /// record — embedded plan re-sectioned — is written fresh.
    fn migrate_registration(path: &Path, o: &mut super::model::Offering, is_candidate: bool) {
        super::record::migrate_embedded_plan(o);
        let migrated = path.with_extension("json.migrated");
        if let Err(e) = std::fs::rename(path, &migrated) {
            tracing::warn!(path = %path.display(), error = %e, "record migration rename failed");
            return;
        }
        // The sidecar rides along: old plan aside, fresh sectioned one
        // written by persist_one below (when this offering carries a plan).
        let plan_path = path.with_file_name("plan.json");
        if plan_path.is_file() {
            let _ = std::fs::rename(&plan_path, plan_path.with_extension("json.migrated"));
        }
        if let Some(parent) = path.parent() {
            let dir = OfferingDir {
                root: parent.to_path_buf(),
            };
            Self::persist_one(&dir, o, is_candidate);
        }
        tracing::info!(
            from = %migrated.display(),
            name = %o.name,
            "registration migrated to the sectioned v3 schema"
        );
    }

    /// Read every instance-leaf under one STEM dir.
    fn read_instances(
        stem_dir: &Path,
        active: &mut Vec<super::model::Offering>,
        candidates: &mut Vec<super::model::Offering>,
    ) {
        let instances = match std::fs::read_dir(stem_dir) {
            Ok(r) => r,
            Err(_) => return,
        };
        for inst in instances.flatten() {
            let dir = inst.path();
            if !dir.is_dir() {
                continue;
            }
            Self::push_leaf(&dir, active, candidates);
        }
    }

    /// One-time move of a PRE-NAMESPACE flat directory into
    /// `{stem}/{default}`, rewriting the record's name to the FQN spelling.
    /// Returns the relocated path (the original parent stem dir).
    fn migrate_flat(flat: &Path) -> Option<PathBuf> {
        let base = flat.parent()?;
        let old_stem = flat.file_name()?.to_str()?.to_string();

        // Identity comes from the record when readable — it is machine truth.
        let record_path = flat.join("record.json");
        let candidate_path = flat.join("candidate.json");
        let (canonical_fqn, json_bytes, is_candidate) = if let Ok(bytes) =
            std::fs::read(&record_path)
        {
            match serde_json::from_slice::<super::model::Offering>(&bytes) {
                Ok(o) => (
                    fqn::canonicalize(&o.name)
                        .unwrap_or_else(|_| fqn::join(&old_stem, fqn::DEFAULT_INSTANCE)),
                    bytes,
                    false,
                ),
                Err(_) => (
                    fqn::join(&old_stem, fqn::DEFAULT_INSTANCE),
                    bytes,
                    false,
                ),
            }
        } else if let Ok(bytes) = std::fs::read(&candidate_path) {
            (
                fqn::join(&old_stem, fqn::DEFAULT_INSTANCE),
                bytes,
                true,
            )
        } else {
            // No registration files: nothing to represent, leave untouched.
            return None;
        };

        let (stem, instance) = fqn::parse(&canonical_fqn)
            .unwrap_or_else(|_| (old_stem.clone(), fqn::DEFAULT_INSTANCE.to_string()));
        let target = base.join(&stem).join(&instance);
        if target.exists() {
            tracing::warn!(
                from = %flat.display(),
                to = %target.display(),
                "flat migration skipped - target exists"
            );
            return None;
        }
        // The target NESTS INSIDE the source (`mongodb/` → `mongodb/default/`),
        // so the directory cannot be renamed as a whole — no engine allows
        // moving a dir into its own subtree. Relocate children one level
        // deeper instead: every artifact rides along, the shell vanishes.
        if let Err(e) = std::fs::create_dir_all(&target) {
            tracing::warn!(error = %e, "migration target create failed");
            return None;
        }
        // Materialize the child list FIRST: we are about to delete entries
        // of this very directory, and a lazily-streamed ReadDir over a
        // directory being mutated yields skips/replays on Windows — the
        // record could silently miss its own move.
        let children: Vec<std::fs::DirEntry> = match std::fs::read_dir(flat) {
            Ok(r) => r.flatten().collect(),
            Err(e) => {
                tracing::warn!(error = %e, "flat migration read failed");
                let _ = std::fs::remove_dir_all(&target);
                return None;
            }
        };
        let mut moved_any = false;
        for child in children {
            let from = child.path();
            if from == target {
                // The landing zone lives INSIDE the source; it is not cargo.
                continue;
            }
            if std::fs::rename(&from, target.join(child.file_name())).is_ok() {
                moved_any = true;
            } else {
                tracing::warn!(path = %from.display(), "flat migration left an entry behind");
            }
        }
        if !moved_any {
            let _ = std::fs::remove_dir_all(&target);
            return None;
        }
        // Remove ONLY the emptied shell — never recurse, because the target
        // sits inside it (`mongodb/` became mongodb{default/…}). A shell with
        // stragglers stays; save()'s prune and next passes settle it.
        let _ = std::fs::remove_dir(flat);
        // Rewrite the stored name to canonical FQN (records carry monikers).
        // Either record shape parses; persist_one writes the sectioned v3.
        let mut o = if let Ok(rec) = serde_json::from_slice::<super::record::OfferingRecord>(&json_bytes)
        {
            Some(rec.into_domain())
        } else {
            serde_json::from_slice::<super::model::Offering>(&json_bytes).ok()
        };
        if let Some(ref mut o) = o {
            super::record::migrate_embedded_plan(o);
            o.name = canonical_fqn.clone();
            o.offering = fqn::stem_of(&canonical_fqn);
            let dir = OfferingDir { root: target.clone() };
            Self::persist_one(&dir, o, is_candidate);
        }
        tracing::info!(
            from = %flat.display(),
            to = %target.display(),
            name = %canonical_fqn,
            "flat directory migrated to namespaced layout"
        );
        Some(target)
    }
}

impl SnapshotStore for DirectoryStore {
    fn load(&self) -> Option<Snapshot> {
        // One-time legacy migrations, transparent to callers.
        let root = OfferingsRoot::new(self.base.clone());
        let legacy = root.legacy_file();
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
            let top = entry.path();
            if !top.is_dir() {
                continue;
            }
            // Pre-namespace flat layout? (registration files directly here)
            let flat_record = top.join("record.json").is_file();
            let flat_candidate = top.join("candidate.json").is_file();
            if flat_record || flat_candidate {
                match Self::migrate_flat(&top) {
                    Some(moved) => {
                        // `moved` IS the offering leaf — read it directly in
                        // this pass (a migration must never cost an offering
                        // a boot of visibility).
                        Self::push_leaf(&moved, &mut active, &mut candidates);
                        continue;
                    }
                    None => continue, // stuck mid-move; logged at the site
                }
            }
            // Canonical nesting: top is a STEM holding instance dirs.
            Self::read_instances(&top, &mut active, &mut candidates);
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

        // Prune stale registration files across BOTH nesting levels; never
        // touch volumes/configs data.
        if let Ok(stems) = std::fs::read_dir(&self.base) {
            for stem_entry in stems.flatten() {
                let stem_dir = stem_entry.path();
                if !stem_dir.is_dir() {
                    continue;
                }
                // Legacy flotsam at stem level (post-migration leftovers).
                if !live_dirs.contains(&stem_dir) {
                    let had_record = stem_dir.join("record.json").is_file();
                    let had_candidate = stem_dir.join("candidate.json").is_file();
                    if had_record || had_candidate {
                        let _ = std::fs::remove_file(stem_dir.join("record.json"));
                        let _ = std::fs::remove_file(stem_dir.join("candidate.json"));
                        let _ = std::fs::remove_file(stem_dir.join("plan.json"));
                        tracing::info!(dir = %stem_dir.display(), "unregistered; volumes preserved");
                    }
                }
                let instances = match std::fs::read_dir(&stem_dir) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                for inst_entry in instances.flatten() {
                    let p = inst_entry.path();
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
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::offerings::model::{Location, ManagedData, ModeData, Status};

    fn sample(name: &str) -> crate::offerings::model::Offering {
        let now = chrono::Utc::now();
        crate::offerings::model::Offering {
            offering_id: uuid::Uuid::now_v7().to_string(),
            name: name.to_string(),
            offering: "redis".into(),
            category: "data".into(),
            status: Status::Running,
            location: Location {
                host: "localhost".into(),
                port: 7300,
                protocol: "http".into(),
            },
            sub_capabilities: Default::default(),
            mode_data: ModeData::Managed(ManagedData {
                runtime_kind: "oci".into(),
                spec: Default::default(),
                port_map: Default::default(),
                plan: None,
            }),
            registered_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn directories_nest_by_fqn_segments() {
        let tmp = std::env::temp_dir().join(format!("zg-dir-nest-{}", uuid::Uuid::now_v7()));
        let root = OfferingsRoot::new(tmp.clone());
        assert_eq!(
            root.dir_for("memcached::default").root,
            tmp.join("memcached").join("default")
        );
        assert_eq!(
            root.dir_for("redis::prod").root,
            tmp.join("redis").join("prod")
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The rehydration round trip under the nested layout, including a
    /// moniker-spelled offering that must land at `{stem}/default`.
    #[test]
    fn store_round_trips_through_nested_layout() {
        let tmp = std::env::temp_dir().join(format!("zg-dir-store-{}", uuid::Uuid::now_v7()));
        let store = DirectoryStore::new(tmp.clone());
        let snap = Snapshot {
            active: vec![sample("memcached")],
            candidates: vec![],
        };
        store.save(&snap);

        let expected = tmp.join("memcached").join("default").join("record.json");
        assert!(expected.is_file(), "{} missing", expected.display());

        let loaded = store.load().unwrap();
        assert_eq!(loaded.active[0].name, "memcached");

        // Registration removal keeps data by law (configs/volumes survive).
        let empty = Snapshot { active: vec![], candidates: vec![] };
        store.save(&empty);
        assert!(!expected.is_file());
        assert!(tmp.join("memcached").join("default").join("volumes").is_dir());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Pre-namespace flat dirs migrate once into `{stem}/default`, records
    /// rewritten to FQN spelling; a second load is stable and idempotent.
    #[test]
    fn flat_layouts_migrate_and_names_gain_the_namespace() {
        use crate::offerings::registry::Snapshot as Snap2;

        let tmp = std::env::temp_dir().join(format!("zg-dir-flat-{}", uuid::Uuid::now_v7()));
        let flat = tmp.join("mongodb");
        std::fs::create_dir_all(&flat).unwrap();
        let legacy_snapshot = Snap2 {
            active: vec![sample("mongodb")], // legacy name: no :: suffix
            candidates: vec![],
        };
        // Stage a legacy-style dir via old-style direct write.
        std::fs::create_dir_all(flat.join("configs")).unwrap();
        std::fs::create_dir_all(flat.join("volumes")).unwrap();
        std::fs::write(
            flat.join("record.json"),
            serde_json::to_vec_pretty(&legacy_snapshot.active[0]).unwrap(),
        )
        .unwrap();

        let store = DirectoryStore::new(tmp.clone());
        let loaded = store.load().unwrap();

        assert_eq!(loaded.active.len(), 1);
        assert_eq!(loaded.active[0].name, "mongodb::default");
        assert_eq!(loaded.active[0].offering, "mongodb");
        assert!(
            tmp.join("mongodb").join("default").join("record.json").is_file(),
            "relocated under the reserved default instance"
        );

        // Idempotent second pass.
        let again = store.load().unwrap();
        assert_eq!(again.active.len(), 1);
        assert_eq!(again.active[0].name, "mongodb::default");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// S5.5: a flat v2 record (with a flat embedded plan and a v2 plan
    /// sidecar) migrates on load — source bytes renamed `*.json.migrated`,
    /// the sectioned v3 record written fresh, plan scalars re-homed under
    /// `meta`. Idempotent; nothing is lost.
    #[test]
    fn v2_records_migrate_to_the_sectioned_schema() {
        let tmp = std::env::temp_dir().join(format!("zg-dir-v3-{}", uuid::Uuid::now_v7()));
        let leaf = tmp.join("redis").join("default");
        std::fs::create_dir_all(&leaf).unwrap();

        // Stage v2: flat record (Offering's legacy serde) + v2 plan, both
        // embedded and as sidecar.
        let mut v2 = sample("redis::default");
        v2.mode_data = crate::offerings::model::ModeData::Managed(
            crate::offerings::model::ManagedData {
                runtime_kind: "oci".into(),
                spec: Default::default(),
                port_map: Default::default(),
                plan: Some(serde_json::json!({
                    "workload": {}, "decisions": [],
                    "plan_hash": 42_u64, "facts_generation": 3_u64
                })),
            },
        );
        std::fs::write(
            leaf.join("record.json"),
            serde_json::to_vec_pretty(&v2).unwrap(),
        )
        .unwrap();
        std::fs::write(
            leaf.join("plan.json"),
            serde_json::to_vec_pretty(v2.managed().unwrap().plan.as_ref().unwrap()).unwrap(),
        )
        .unwrap();

        let store = DirectoryStore::new(tmp.clone());
        let loaded = store.load().unwrap();
        assert_eq!(loaded.active.len(), 1, "the migration never hides an offering");
        assert_eq!(loaded.active[0].name, "redis::default");
        let plan = loaded.active[0]
            .managed()
            .unwrap()
            .plan
            .as_ref()
            .unwrap();
        assert_eq!(plan["meta"]["plan_hash"], 42, "embedded plan re-sectioned");
        assert!(plan.get("plan_hash").is_none(), "no flat scalars remain");

        // The sectioned truth is on disk; the v2 evidence is aside, intact.
        let record: crate::offerings::record::OfferingRecord =
            serde_json::from_slice(&std::fs::read(leaf.join("record.json")).unwrap()).unwrap();
        assert_eq!(record.identity.name, "redis::default");
        assert_eq!(record.state.status, Status::Running);
        assert!(leaf.join("record.json.migrated").is_file());
        assert!(leaf.join("plan.json.migrated").is_file());
        let sidecar: serde_json::Value =
            serde_json::from_slice(&std::fs::read(leaf.join("plan.json")).unwrap()).unwrap();
        assert_eq!(sidecar["meta"]["facts_generation"], 3);

        // Idempotent second pass.
        let again = store.load().unwrap();
        assert_eq!(again.active.len(), 1);
        assert_eq!(again.active[0].name, "redis::default");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
