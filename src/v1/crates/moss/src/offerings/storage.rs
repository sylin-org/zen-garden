//! Storage banks (ADR-0005 §8): plugged drives are just news.
//!
//! A **bank** is a removable volume adopted into the garden — logical
//! identity is an FQN (`bank::default` communal, explicit instances
//! private, ADR-0003), the physical device carries its own GUIDv7. The
//! dotfolder law (STORAGE-0009) makes plug-and-recognize work: the
//! `.zen-garden/manifest.json` on the device IS the adoption record, so
//! the bank is recognized wherever it appears (L5: identity lives on the
//! media).
//!
//! State changes (mounted/ejected) bump the bank revision and sing;
//! capacity and used bytes are TELEMETRY — they ride along, they never
//! trigger frames (§8.2's anti-spam law). Liveness is inherited, never
//! timered (§8.3): the topology dims a bank with its stone.

use garden_glossary::bank as vocab;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::watch;

/// How often the mount watcher re-reads volume reality (R2.8: polling at
/// the edge — the OS offers no portable push for mounts).
pub const MOUNT_WATCH_SECS: u64 = 5;

/// The dotfolder on every adopted device (STORAGE-0009). Wire-adjacent
/// literal: it is the on-media contract, forever-compatible (R0.5).
pub const MANIFEST_DIR: &str = ".zen-garden";
/// The adoption record's filename inside [`MANIFEST_DIR`].
pub const MANIFEST_FILE: &str = "manifest.json";

/// The adoption manifest, riding the device (STORAGE-0009 dotfolder law).
/// Roles ride it too — the PoC's manifest carried `roles: []` and v1
/// keeps the field: a declaration that dies with a moss restart was a
/// silent lie (L3/L5 — the media is the record).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BankManifest {
    /// Physical device identity (GUIDv7, minted at first adoption).
    pub device_id: String,
    /// Logical bank identity (FQN, ADR-0003).
    pub fqn: String,
    /// The stone that performed the ceremony.
    pub stone_id: String,
    /// When the ceremony ran.
    pub adopted_at: chrono::DateTime<chrono::Utc>,
    /// Schema marker.
    pub proto: String,
    /// Declared duties (sink today). Absent on older records; written
    /// always, PoC-style.
    #[serde(default)]
    pub roles: Vec<String>,
}

/// A bank as this stone holds it.
#[derive(Debug, Clone, Serialize)]
pub struct Bank {
    /// Logical identity (FQN).
    pub fqn: String,
    /// Physical device identity.
    pub device_id: String,
    /// mounted | ejected (glossary::bank).
    pub state: String,
    /// Declared roles (sink today; the set grows with ADR-0005's tiers).
    pub roles: Vec<String>,
    /// Where the volume answers locally; meaningful while mounted.
    pub mount_point: String,
    /// TELEMETRY: total bytes, when measured.
    pub capacity_bytes: Option<u64>,
    /// TELEMETRY: used bytes, when measured.
    pub used_bytes: Option<u64>,
    /// Internal: the operator's eject holds for this slot until a true
    /// re-plug (different mount) or a fresh boot. A mere vanish-eject
    /// (yank/flake) does NOT hold — return means mounted again.
    #[serde(skip)]
    pub held_ejected: bool,
}

/// What the volume scan saw about one removable volume.
#[derive(Debug, Clone)]
pub struct VolumeFact {
    pub mount_point: PathBuf,
    /// The device's own identity — present iff a manifest rides it.
    pub device_id: Option<String>,
    /// The bank's logical name — present iff a manifest rides it.
    pub fqn: Option<String>,
    /// The roles the media holds — L5: declarations travel with the drive.
    pub roles: Vec<String>,
    pub capacity_bytes: u64,
    pub available_bytes: u64,
}

/// Why an eject refused (R3.3).
#[derive(Debug)]
pub enum EjectError {
    /// No bank by that name is adopted on this stone.
    UnknownBank(String),
    /// The bank is already at rest.
    AlreadyEjected(String),
}

impl std::fmt::Display for EjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownBank(n) => write!(
                f,
                "no bank '{n}' is adopted here — rake storage lists what this stone holds"
            ),
            Self::AlreadyEjected(n) => write!(f, "'{n}' is already ejected"),
        }
    }
}

/// Why an adoption refused. Errors answer three questions (R3.3).
#[derive(Debug)]
pub enum AdoptError {
    /// The volume already carries a garden manifest.
    AlreadyAdopted(String),
    /// The name failed the FQN grammar (glossary::fqn is the only grammar).
    BadName(String),
    /// The manifest could not be written onto the device.
    Io(String),
}

impl std::fmt::Display for AdoptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyAdopted(d) => write!(
                f,
                "'{d}' already carries a garden manifest — it is adopted; uproot or rename there"
            ),
            Self::BadName(n) => write!(
                f,
                "'{n}' is not a bank name — banks speak the FQN grammar (stem::instance)"
            ),
            Self::Io(e) => write!(f, "the adoption manifest could not be written: {e}"),
        }
    }
}

/// The stone's banks: the local half of the garden's storage estate.
/// Mutations bump a watch so the announcer's composer follows (L18).
pub struct Storage {
    banks: parking_lot::Mutex<HashMap<String, Bank>>,
    /// Mutation signal — the SOURCE owns the bank_rev; this only says
    /// "look again".
    bumps: watch::Sender<u64>,
}

impl Default for Storage {
    fn default() -> Self {
        Self::new()
    }
}

impl Storage {
    pub fn new() -> Self {
        let (bumps, _) = watch::channel(0);
        Self {
            banks: parking_lot::Mutex::new(HashMap::new()),
            bumps,
        }
    }

    /// Follow storage mutations (L18: events, not polls).
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.bumps.subscribe()
    }

    fn bump(&self) {
        // NB: send_modify under one lock — never borrow inside send_replace
        // (the S2 deadlock idiom).
        self.bumps.send_modify(|v| *v = v.wrapping_add(1));
    }

    /// The banks, sorted by name (deterministic renderings).
    pub fn banks(&self) -> Vec<Bank> {
        let mut banks: Vec<Bank> = self.banks.lock().values().cloned().collect();
        banks.sort_by(|a, b| a.fqn.cmp(&b.fqn));
        banks
    }

    /// Declare a bank's roles (§4: sink today; the set grows with the
    /// tiers). Role news is state news - it bumps. Unknown roles refuse
    /// loudly (L12 - the glossary speaks once, everywhere).
    /// Declare a bank's roles (§4: sink today; the set grows with the
    /// tiers). Write-through: the declaration is real when the MEDIA
    /// holds it — the manifest is amended and rewritten before memory
    /// moves, so a moss restart (or another stone) hears the same
    /// duties. A refused write refuses the declaration loudly (L3: a
    /// sink that silently dies at restart is a lie). Role news is state
    /// news - it bumps. Unknown roles refuse loudly (L12).
    pub fn set_roles(&self, fqn: &str, roles: Vec<String>) -> Result<Option<Bank>, String> {
        for r in &roles {
            if !garden_glossary::bank::role::ALL.contains(&r.as_str()) {
                return Err(format!(
                    "unknown bank role '{r}' - the garden knows: {}",
                    garden_glossary::bank::role::ALL.join(", ")
                ));
            }
        }
        let canonical = garden_glossary::fqn::canonicalize(fqn).map_err(|_| {
            format!("'{fqn}' is not a bank name - banks speak the FQN grammar (stem::instance)")
        })?;
        let mut banks = self.banks.lock();
        let Some(bank) = banks.get_mut(&canonical) else {
            return Ok(None);
        };
        if bank.state != vocab::MOUNTED {
            return Err(format!(
                "'{canonical}' is ejected - its volume does not answer; a declaration needs the media"
            ));
        }
        let mount = PathBuf::from(&bank.mount_point);
        // Read-amend-write: the manifest holds the ceremony's truth
        // (stone_id, adopted_at) and must not be forged from memory.
        let mut manifest = read_manifest(&mount).ok_or_else(|| {
            format!(
                "'{canonical}' carries no readable adoption record on its media - roles live \
                 on the drive (R0.5); re-adopt or repair the drive"
            )
        })?;
        manifest.roles = roles.clone();
        write_manifest(&mount, &manifest)
            .map_err(|e| format!("the media refused the declaration: {e}"))?;
        bank.roles = roles;
        let updated = bank.clone();
        drop(banks);
        self.bump();
        Ok(Some(updated))
    }

    /// The adopt ceremony (ADR-0005): write the manifest onto the device,
    /// remember the bank mounted, sing. Detect first, claim only what
    /// answers (L25).
    pub fn adopt(&self, vol: &VolumeFact, name: &str, stone_id: &str) -> Result<Bank, AdoptError> {
        let fqn = garden_glossary::fqn::canonicalize(name).map_err(|_| AdoptError::BadName(name.to_string()))?;
        if vol.device_id.is_some() || vol.fqn.is_some() {
            return Err(AdoptError::AlreadyAdopted(vol.mount_point.display().to_string()));
        }
        let device_id = uuid::Uuid::now_v7().to_string();
        let manifest = BankManifest {
            device_id: device_id.clone(),
            fqn: fqn.clone(),
            stone_id: stone_id.to_string(),
            adopted_at: chrono::Utc::now(),
            proto: garden_contract::consts::PROTO_V1.into(),
            roles: Vec::new(),
        };
        write_manifest(&vol.mount_point, &manifest)
            .map_err(AdoptError::Io)?;

        let bank = Bank {
            fqn,
            device_id,
            state: vocab::MOUNTED.into(),
            roles: Vec::new(),
            mount_point: vol.mount_point.display().to_string(),
            capacity_bytes: Some(vol.capacity_bytes),
            used_bytes: Some(vol.capacity_bytes.saturating_sub(vol.available_bytes)),
            held_ejected: false,
        };
        self.banks.lock().insert(bank.fqn.clone(), bank.clone());
        self.bump();
        tracing::info!(bank = %bank.fqn, device = %bank.device_id, "bank adopted; the garden will hear it");
        Ok(bank)
    }

    /// The eject verb (ADR-0005 §8.3): announce authoritative absence.
    /// The bank is marked ejected this boot and the news sings; the
    /// reconciler respects the ruling — the same slot will not flip the
    /// bank back to mounted until its volume is seen at a DIFFERENT mount
    /// (a true re-plug). Physical removal stays the operator's hand; the
    /// song is the "safe to pull" signal.
    pub fn eject(&self, fqn: &str) -> Result<Bank, EjectError> {
        let canonical = garden_glossary::fqn::canonicalize(fqn)
            .map_err(|_| EjectError::UnknownBank(fqn.to_string()))?;
        let mut banks = self.banks.lock();
        let bank = banks
            .get_mut(&canonical)
            .ok_or_else(|| EjectError::UnknownBank(canonical.clone()))?;
        if bank.state == vocab::EJECTED {
            return Err(EjectError::AlreadyEjected(canonical));
        }
        bank.state = vocab::EJECTED.into();
        bank.held_ejected = true;
        let ejected = bank.clone();
        drop(banks);
        self.bump();
        tracing::info!(bank = %ejected.fqn, "bank ejected; the garden hears authoritative absence");
        Ok(ejected)
    }

    /// Reconcile current volume reality against the known banks — the
    /// watcher's step and the boot pass in one. Manifest-carrying volumes
    /// register or revive; vanished banks eject. Measurements refresh
    /// WITHOUT a bump (§8.2); state changes bump (they are news).
    pub fn reconcile(&self, volumes: &[VolumeFact]) {
        let mut changed = false;
        {
            let mut banks = self.banks.lock();
            let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
            for vol in volumes {
                let (Some(device_id), Some(fqn)) = (&vol.device_id, &vol.fqn) else {
                    continue; // unadopted volume: adoptable, not a bank
                };
                seen.insert(fqn.clone());
                let used = vol.capacity_bytes.saturating_sub(vol.available_bytes);
                match banks.get_mut(fqn) {
                    Some(bank) => {
                        // An operator's eject holds for the same slot (the
                        // boot's life); any other ejected bank that
                        // reappears — yanked, flaked, re-plugged — mounts
                        // anew. That is the L10 nourish-on-wake instinct.
                        let holds = bank.held_ejected
                            && bank.mount_point == vol.mount_point.display().to_string();
                        if bank.state != vocab::MOUNTED && !holds {
                            bank.state = vocab::MOUNTED.into();
                            bank.mount_point = vol.mount_point.display().to_string();
                            bank.held_ejected = false;
                            changed = true;
                        }
                        bank.capacity_bytes = Some(vol.capacity_bytes);
                        bank.used_bytes = Some(used);
                        // The media is the record (L5): roles travel with
                        // the drive, so a re-plug — or a drive moved from
                        // another stone — re-voices its duties here.
                        if bank.roles != vol.roles {
                            bank.roles = vol.roles.clone();
                            changed = true;
                        }
                    }
                    None => {
                        tracing::info!(bank = %fqn, "recognized an adopted bank at boot/plug-in");
                        banks.insert(
                            fqn.clone(),
                            Bank {
                                fqn: fqn.clone(),
                                device_id: device_id.clone(),
                                state: vocab::MOUNTED.into(),
                                roles: vol.roles.clone(),
                                mount_point: vol.mount_point.display().to_string(),
                                capacity_bytes: Some(vol.capacity_bytes),
                                used_bytes: Some(used),
                                held_ejected: false,
                            },
                        );
                        changed = true;
                    }
                }
            }
            // Banks whose volume vanished eject — loudly here, expired
            // quietly in the garden (liveness is inherited, §8.3).
            for bank in banks.values_mut() {
                if seen.contains(&bank.fqn) {
                    continue;
                }
                if bank.state == vocab::MOUNTED {
                    tracing::info!(bank = %bank.fqn, "bank volume gone; state ejected");
                    bank.state = vocab::EJECTED.into();
                    changed = true;
                }
                // Physical absence releases an operator's hold: once the
                // drive is out, the next appearance is a true return.
                bank.held_ejected = false;
            }
        }
        if changed {
            self.bump();
        }
    }

    /// Adoptable volumes: removable, mounted, carrying no manifest.
    pub fn adoptable(volumes: &[VolumeFact]) -> Vec<VolumeFact> {
        volumes
            .iter()
            .filter(|v| v.device_id.is_none() && v.fqn.is_none())
            .cloned()
            .collect()
    }

    /// Resolve a bank to its mounted volume root — the gate every file
    /// operation passes. The FQN is canonicalized (bare stems speak their
    /// communal ::default); an ejected bank refuses: its volume does not
    /// answer, whatever the map says.
    pub fn bank_root(&self, fqn: &str) -> Result<(Bank, PathBuf), FilesError> {
        let canonical = garden_glossary::fqn::canonicalize(fqn)
            .map_err(|_| FilesError::UnknownBank(fqn.to_string()))?;
        let banks = self.banks.lock();
        let bank = banks
            .get(&canonical)
            .ok_or_else(|| FilesError::UnknownBank(canonical.clone()))?;
        if bank.state != vocab::MOUNTED {
            return Err(FilesError::NotMounted(bank.fqn.clone()));
        }
        Ok((bank.clone(), PathBuf::from(&bank.mount_point)))
    }
}

/// Why a bank-file operation refused. Errors answer three questions (R3.3).
#[derive(Debug)]
pub enum FilesError {
    /// No bank by that name is adopted on this stone.
    UnknownBank(String),
    /// The bank is adopted but its volume is not present (ejected).
    NotMounted(String),
    /// The path does not name a file under the bank: traversal, escape,
    /// or the garden's own adoption record.
    BadPath(String),
    /// The path names a directory but the verb needs a file (or the
    /// reverse: a file where the verb needs a directory).
    NotThatKind(String),
    /// The path names nothing on the volume.
    Missing(String),
    /// The filesystem refused.
    Io(String),
}

impl std::fmt::Display for FilesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownBank(n) => write!(
                f,
                "no bank '{n}' is adopted here — rake storage lists what this stone holds"
            ),
            Self::NotMounted(n) => write!(
                f,
                "'{n}' is ejected — its volume does not answer; replug it (rake storage shows the state)"
            ),
            Self::BadPath(p) => write!(f, "{p}"),
            Self::NotThatKind(p) => write!(f, "{p}"),
            Self::Missing(p) => write!(f, "nothing answers at '{p}' on this bank"),
            Self::Io(e) => write!(f, "the bank's filesystem refused: {e}"),
        }
    }
}

/// One row of a bank directory listing.
#[derive(Debug, Clone, Serialize)]
pub struct FileEntry {
    /// Name within its directory (no separators).
    pub name: String,
    /// file | dir (the two kinds the files API speaks).
    pub kind: String,
    /// Bytes on disk; files only.
    pub size_bytes: Option<u64>,
    /// Last modification, when the filesystem knows.
    pub modified_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Join a bank root with a wire path (`a/b/c`, `/`-separated, relative),
/// refusing every escape before the filesystem sees it:
///   · no `..` — the single law that makes the rest decoration,
///   · no absolute forms (leading `/`, Windows prefixes) — the wire path
///     is relative by definition,
///   · no [`MANIFEST_DIR`] — the adoption record is ceremony-owned
///     (STORAGE-0009); file operations never see it,
///   · empties and `.` collapse; a path that collapses to nothing is not
///     a file.
/// A final canonicalize-and-prefix check runs at call sites for anything
/// that touches the disk (symlinks planted on the media cannot launder a
/// path past it).
pub fn safe_join(mount: &Path, rel: &str) -> Result<PathBuf, FilesError> {
    let refuse = |why: String| Err(FilesError::BadPath(why));
    if rel.starts_with('/') || rel.starts_with('\\') {
        return refuse(format!(
            "'{rel}' is absolute — bank paths are relative to the bank's root"
        ));
    }
    let mut joined = mount.to_path_buf();
    for part in rel.split(['/', '\\']) {
        match part {
            "" | "." => continue,
            ".." => {
                return refuse(format!(
                    "'{rel}' climbs out of the bank ('..') — paths stay under the bank's root"
                ));
            }
            MANIFEST_DIR => {
                return refuse(format!(
                    "'.{MANIFEST_DIR}' is the adoption record — ceremony-owned, always closed"
                ));
            }
            p => joined.push(p),
        }
    }
    if joined == mount {
        return refuse(format!(
            "'{rel}' names no file — give a path under the bank's root"
        ));
    }
    Ok(joined)
}

/// The canonicalize-and-prefix check: the target (or its deepest existing
/// ancestor, for writes) must truly live under the mount. Catches what
/// lexicals cannot see — a symlink on the media pointing outward.
fn verify_under_mount(mount: &Path, target: &Path) -> Result<(), FilesError> {
    let mount_real = std::fs::canonicalize(mount)
        .map_err(|e| FilesError::Io(format!("{}: {e}", mount.display())))?;
    let mut probe = target.to_path_buf();
    while !probe.as_os_str().is_empty() {
        if let Ok(real) = std::fs::canonicalize(&probe) {
            if !real.starts_with(&mount_real) {
                return Err(FilesError::BadPath(format!(
                    "'{}' resolves outside the bank — the path refuses",
                    rel_of(mount, target)
                )));
            }
            return Ok(());
        }
        probe = probe
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
    }
    Ok(())
}

/// The wire spelling of a resolved path (for error messages).
fn rel_of(mount: &Path, target: &Path) -> String {
    target
        .strip_prefix(mount)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| target.display().to_string())
}

/// List a directory on a bank, sorted by name. The adoption record's
/// dotfolder is invisible — the ceremony's business stays off the wire.
pub fn list_dir(mount: &Path, dir: &Path) -> Result<Vec<FileEntry>, FilesError> {
    verify_under_mount(mount, dir)?;
    let mut out = Vec::new();
    let io_err = |e: std::io::Error| match e.kind() {
        std::io::ErrorKind::NotFound => FilesError::Missing(rel_of(mount, dir)),
        _ => FilesError::Io(format!("{}: {e}", dir.display())),
    };
    for entry in std::fs::read_dir(dir).map_err(io_err)? {
        let entry = entry.map_err(io_err)?;
        if entry.file_name() == MANIFEST_DIR {
            continue;
        }
        let meta = entry.metadata().map_err(io_err)?;
        out.push(FileEntry {
            name: entry.file_name().display().to_string(),
            kind: if meta.is_dir() { "dir" } else { "file" }.into(),
            size_bytes: meta.is_file().then_some(meta.len()),
            modified_at: meta
                .modified()
                .ok()
                .map(chrono::DateTime::<chrono::Utc>::from),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Read one file from a bank.
pub fn read_file(mount: &Path, path: &Path) -> Result<Vec<u8>, FilesError> {
    verify_under_mount(mount, path)?;
    let meta = std::fs::metadata(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => FilesError::Missing(rel_of(mount, path)),
        _ => FilesError::Io(format!("{}: {e}", path.display())),
    })?;
    if meta.is_dir() {
        return Err(FilesError::NotThatKind(format!(
            "'{}' is a directory — the files face lists directories, it does not read them",
            rel_of(mount, path)
        )));
    }
    std::fs::read(path).map_err(|e| FilesError::Io(format!("{}: {e}", path.display())))
}

/// Write one file onto a bank, creating parent directories. Returns the
/// bytes written (the file's new size).
pub fn write_file(mount: &Path, path: &Path, bytes: &[u8]) -> Result<u64, FilesError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| FilesError::Io(format!("{}: {e}", parent.display())))?;
    }
    verify_under_mount(mount, path)?;
    if std::fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false) {
        return Err(FilesError::NotThatKind(format!(
            "'{}' is a directory — a file cannot be written over it",
            rel_of(mount, path)
        )));
    }
    std::fs::write(path, bytes).map_err(|e| FilesError::Io(format!("{}: {e}", path.display())))?;
    Ok(bytes.len() as u64)
}

/// Delete one file from a bank. Directories refuse: wholesale removal is
/// the operator's hand (or the will's pipeline), not a stray verb.
pub fn delete_file(mount: &Path, path: &Path) -> Result<(), FilesError> {
    verify_under_mount(mount, path)?;
    let meta = std::fs::metadata(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => FilesError::Missing(rel_of(mount, path)),
        _ => FilesError::Io(format!("{}: {e}", path.display())),
    })?;
    if meta.is_dir() {
        return Err(FilesError::NotThatKind(format!(
            "'{}' is a directory — delete its files, or eject the bank to release the whole volume",
            rel_of(mount, path)
        )));
    }
    std::fs::remove_file(path)
        .map_err(|e| FilesError::Io(format!("{}: {e}", path.display())))
}

/// Read the garden manifest off a volume, if one rides it.
/// R0.5 (on-media is forever): a PoC-era manifest (version 4: {id, name,
/// origin_stone, created_at, ...}) is RECOGNIZED, not ignored - the drive
/// keeps its lineage and the garden regains its old banks.
fn read_manifest(mount: &Path) -> Option<BankManifest> {
    let file = mount.join(MANIFEST_DIR).join(MANIFEST_FILE);
    let bytes = std::fs::read(file).ok()?;
    if let Ok(m) = serde_json::from_slice::<BankManifest>(&bytes) {
        return Some(m);
    }
    let poc: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let device_id = poc["id"].as_str()?.to_string();
    let name = poc["name"].as_str()?;
    // The PoC named banks poetically without instances; v1's grammar
    // gives every name its communal ::default (ADR-0003).
    let fqn = garden_glossary::fqn::canonicalize(name).ok()?;
    let adopted_at = poc["created_at"]
        .as_str()
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
        .map(|t| t.with_timezone(&chrono::Utc))
        .unwrap_or_else(chrono::Utc::now);
    tracing::info!(
        bank = %fqn,
        device = %device_id,
        origin = poc["origin_stone"].as_str().unwrap_or("?"),
        "recognized a PoC-era bank manifest; its lineage rides along"
    );
    let roles = poc["roles"]
        .as_array()
        .map(|r| {
            r.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    Some(BankManifest {
        device_id,
        fqn,
        stone_id: poc["origin_stone"].as_str().unwrap_or("unknown").to_string(),
        adopted_at,
        proto: format!("poc/{}", poc["version"].as_u64().unwrap_or(4)),
        roles,
    })
}

/// Write the adoption record onto the media (STORAGE-0009). The record
/// is the contract: identity AND duties (roles) ride it, so a restart,
/// a re-plug, or another stone hears the same truth (L5/R0.5).
fn write_manifest(mount: &Path, manifest: &BankManifest) -> Result<(), String> {
    let dir = mount.join(MANIFEST_DIR);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let file = dir.join(MANIFEST_FILE);
    let bytes =
        serde_json::to_vec_pretty(manifest).map_err(|e| format!("encode: {e}"))?;
    std::fs::write(&file, bytes).map_err(|e| format!("{}: {e}", file.display()))
}

/// Scan removable volumes (the edge: sysinfo refresh). Returns facts for
/// every removable, mounted volume — adopted or not.
pub fn scan_volumes() -> Vec<VolumeFact> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mut out = Vec::new();
    for d in disks.list() {
        if !d.is_removable() {
            continue;
        }
        let mount = d.mount_point().to_path_buf();
        let manifest = read_manifest(&mount);
        out.push(VolumeFact {
            device_id: manifest.as_ref().map(|m| m.device_id.clone()),
            fqn: manifest.as_ref().map(|m| m.fqn.clone()),
            roles: manifest.as_ref().map(|m| m.roles.clone()).unwrap_or_default(),
            capacity_bytes: d.total_space(),
            available_bytes: d.available_space(),
            mount_point: mount,
        });
    }
    out
}

/// The mount watcher: reconcile volume reality on the clock (R2.8 — the
/// edge poll the OS leaves us). State changes bump; the announcer sings.
pub async fn watch_mounts(storage: Arc<Storage>, token: tokio_util::sync::CancellationToken) {
    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(MOUNT_WATCH_SECS));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = token.cancelled() => return,
            _ = ticker.tick() => {
                let volumes = scan_volumes();
                storage.reconcile(&volumes);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    fn vol(mount: &str, adopted: bool) -> VolumeFact {
        VolumeFact {
            mount_point: PathBuf::from(mount),
            device_id: adopted.then(|| "dev-1".to_string()),
            fqn: adopted.then(|| "seed-vault::default".to_string()),
            roles: Vec::new(),
            capacity_bytes: 1000,
            available_bytes: 400,
        }
    }

    #[test]
    fn adopt_writes_the_manifest_and_remembers_the_bank() {
        let tmp = std::env::temp_dir().join(format!("zg-adopt-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&tmp).unwrap();
        let vol = vol(tmp.to_str().unwrap(), false);
        let storage = Storage::new();

        let mut signal = storage.subscribe();
        let before = *signal.borrow_and_update();
        let bank = storage.adopt(&vol, "seed-vault", "stone-1").unwrap();

        assert_eq!(bank.fqn, "seed-vault::default");
        assert_eq!(bank.state, vocab::MOUNTED);
        assert_eq!(bank.capacity_bytes, Some(1000));
        assert_eq!(bank.used_bytes, Some(600));

        // The manifest rides the device (STORAGE-0009): plug-and-recognize.
        let manifest: BankManifest = serde_json::from_slice(
            &std::fs::read(tmp.join(".zen-garden").join("manifest.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest.fqn, "seed-vault::default");
        assert_eq!(manifest.device_id, bank.device_id);

        assert_ne!(*signal.borrow(), before, "adoption must bump");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn poc_era_manifests_are_recognized_across_generations() {
        // R0.5: a drive adopted by the POc carries {version, id, name,
        // origin_stone, ...} - v1 reads what the PoC wrote, in the field.
        let tmp = std::env::temp_dir().join(format!("zg-poc-man-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(tmp.join(".zen-garden")).unwrap();
        std::fs::write(
            tmp.join(".zen-garden").join("manifest.json"),
            br#"{
  "version": 4,
  "id": "019cd3c0-fe39-7dd0-a8f8-3c805a0a23aa",
  "name": "seed-gentle-valley",
  "visibility": "open",
  "origin_stone": "stone-azure-pool",
  "filesystem": "unknown",
  "created_at": "2026-03-09T17:59:26.521282400Z",
  "encrypted": false,
  "roles": []
}"#,
        )
        .unwrap();

        let vol = vol(tmp.to_str().unwrap(), false);
        let vol = VolumeFact {
            roles: Vec::new(),
            device_id: Some("019cd3c0-fe39-7dd0-a8f8-3c805a0a23aa".into()),
            fqn: Some("seed-gentle-valley::default".into()),
            ..vol
        };
        let storage = Storage::new();
        storage.reconcile(&[vol]);

        let banks = storage.banks();
        assert_eq!(banks.len(), 1, "the old bank is recognized");
        assert_eq!(banks[0].fqn, "seed-gentle-valley::default", "lineage restored");
        assert_eq!(banks[0].state, garden_glossary::bank::MOUNTED);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn adoption_refuses_the_already_claimed_and_the_off_grammar() {
        let tmp = std::env::temp_dir().join(format!("zg-adopt2-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&tmp).unwrap();
        let storage = Storage::new();

        let err = storage
            .adopt(&vol(tmp.to_str().unwrap(), true), "anything", "s")
            .unwrap_err();
        assert!(matches!(err, AdoptError::AlreadyAdopted(_)));

        let err = storage
            .adopt(&vol(tmp.to_str().unwrap(), false), "bad name!!", "s")
            .unwrap_err();
        assert!(matches!(err, AdoptError::BadName(_)));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn role_declarations_refuse_the_unknown_and_sing_the_known() {
        let storage = Storage::new();
        storage.reconcile(&[vol("E:\\", true)]);

        let signal = storage.subscribe();
        let before = *signal.borrow();
        let err = storage
            .set_roles("seed-vault::default", vec!["warden".into()])
            .unwrap_err();
        assert!(err.contains("unknown bank role"), "{err}");
        assert_eq!(*signal.borrow(), before, "a refusal is not news");

        let bank = storage
            .set_roles("seed-vault::default", vec![garden_glossary::bank::role::SINK.into()])
            .unwrap()
            .unwrap();
        assert_eq!(bank.roles, vec![garden_glossary::bank::role::SINK]);
        assert_ne!(*signal.borrow(), before, "role news is state news");
    }

    /// The declaration is WRITE-THROUGH (the trap the live room taught):
    /// declaring roles amends the manifest ON THE MEDIA, so a moss
    /// restart — a fresh Storage reconciling the same volume — hears the
    /// same duties. Roles travel with the drive (L5), PoC lineage too.
    #[test]
    fn roles_ride_the_media_and_survive_a_restart() {
        let tmp = std::env::temp_dir().join(format!("zg-roles-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&tmp).unwrap();
        let storage = Storage::new();
        storage
            .adopt(
                &VolumeFact {
                    mount_point: tmp.clone(),
                    device_id: None,
                    fqn: None,
                    roles: Vec::new(),
                    capacity_bytes: 1_000_000,
                    available_bytes: 900_000,
                },
                "seed-vault",
                "stone-1",
            )
            .unwrap();

        storage
            .set_roles("seed-vault", vec![garden_glossary::bank::role::SINK.into()])
            .unwrap()
            .unwrap();
        // The media holds it.
        let manifest: BankManifest = serde_json::from_slice(
            &std::fs::read(tmp.join(MANIFEST_DIR).join(MANIFEST_FILE)).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest.roles, vec![garden_glossary::bank::role::SINK]);
        assert_eq!(manifest.fqn, "seed-vault::default", "lineage preserved");

        // A restart: a fresh Storage scans the same volume — the scan
        // reads the manifest's roles, reconcile seats the bank WITH its
        // duties. No re-declaration, no silent sink loss.
        let rescanned = VolumeFact {
            device_id: Some(manifest.device_id.clone()),
            fqn: Some(manifest.fqn.clone()),
            roles: manifest.roles.clone(),
            capacity_bytes: 1_000_000,
            available_bytes: 900_000,
            mount_point: tmp.clone(),
        };
        let restarted = Storage::new();
        restarted.reconcile(&[rescanned]);
        let banks = restarted.banks();
        assert_eq!(banks.len(), 1);
        assert_eq!(
            banks[0].roles,
            vec![garden_glossary::bank::role::SINK],
            "the declaration survived the restart"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// A declaration needs the media: an ejected bank refuses (its
    /// volume does not answer), and a bank whose record vanished from
    /// the media refuses rather than pretending (L3/L7).
    #[test]
    fn declarations_refuse_without_the_media() {
        let tmp = std::env::temp_dir().join(format!("zg-roles2-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&tmp).unwrap();
        let storage = Storage::new();
        // Adopted for real: the ceremony wrote the record on the media.
        storage
            .adopt(
                &VolumeFact {
                    mount_point: tmp.clone(),
                    device_id: None,
                    fqn: None,
                    roles: Vec::new(),
                    capacity_bytes: 1000,
                    available_bytes: 400,
                },
                "seed-vault",
                "stone-1",
            )
            .unwrap();

        // Ejected: no volume answers, no declaration.
        storage.eject("seed-vault").unwrap();
        let err = storage
            .set_roles("seed-vault", vec![garden_glossary::bank::role::SINK.into()])
            .unwrap_err();
        assert!(err.contains("ejected"), "{err}");

        // Record gone: the manifest is the record; without it, refuse.
        // (The return from the void releases the operator's hold: the
        // drive is back, mounted, with its record missing.)
        storage.reconcile(&[]);
        storage.reconcile(&[vol(tmp.to_str().unwrap(), true)]);
        std::fs::remove_file(tmp.join(MANIFEST_DIR).join(MANIFEST_FILE)).unwrap();
        let err = storage
            .set_roles("seed-vault", vec![garden_glossary::bank::role::SINK.into()])
            .unwrap_err();
        assert!(
            err.contains("no readable adoption record"),
            "the refusal teaches: {err}"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The eject laws: eject announces and holds the same slot; a vanish
    /// does NOT hold (return remounts); a different slot is a true re-plug.
    #[test]
    fn eject_holds_until_a_true_replug() {
        let storage = Storage::new();
        storage.reconcile(&[vol("E:\\", true)]);
        assert_eq!(storage.banks()[0].state, vocab::MOUNTED);

        // The operator ejects: news.
        let signal = storage.subscribe();
        let before = *signal.borrow();
        storage.eject("seed-vault").unwrap();
        assert_ne!(*signal.borrow(), before);
        assert_eq!(storage.banks()[0].state, vocab::EJECTED);

        // Same slot still present: the ruling holds, no flip-flop.
        let before = *signal.borrow();
        storage.reconcile(&[vol("E:\\", true)]);
        assert_eq!(*signal.borrow(), before, "no fight with the operator");
        assert_eq!(storage.banks()[0].state, vocab::EJECTED);

        // A vanish does not hold: return (same slot) remounts. The
        // operator's hold was released by seeing the volume gone once.
        storage.reconcile(&[]);
        storage.reconcile(&[vol("E:\\", true)]);
        assert_eq!(storage.banks()[0].state, vocab::MOUNTED);

        // Refusals: ejecting a ghost or the already-at-rest.
        assert!(matches!(
            storage.eject("ghost::default"),
            Err(EjectError::UnknownBank(_))
        ));
        storage.eject("seed-vault").unwrap();
        assert!(matches!(
            storage.eject("seed-vault"),
            Err(EjectError::AlreadyEjected(_))
        ));
    }

    /// The watcher's law in one test: mount→eject→mount are news (bumps);
    /// capacity drift is telemetry (no bump); a vanished bank ejects.
    #[test]
    fn reconcile_bumps_state_and_keeps_measurements_quiet() {
        let storage = Storage::new();
        let signal = storage.subscribe();
        // Watch values land synchronously with the mutation: bump-ness is
        // just a counter delta across each step.

        // Boot: an adopted volume registers (news).
        let before = *signal.borrow();
        storage.reconcile(&[vol("E:\\", true)]);
        assert_ne!(*signal.borrow(), before);
        assert_eq!(storage.banks()[0].state, vocab::MOUNTED);

        // Capacity-only change: telemetry rides, no bump.
        let mut fatter = vol("E:\\", true);
        fatter.available_bytes = 100;
        let before = *signal.borrow();
        storage.reconcile(&[fatter.clone()]);
        assert_eq!(*signal.borrow(), before, "measurements never trigger frames (§8.2)");
        assert_eq!(storage.banks()[0].used_bytes, Some(900));

        // Volume gone: ejected (news).
        let before = *signal.borrow();
        storage.reconcile(&[]);
        assert_ne!(*signal.borrow(), before);
        assert_eq!(storage.banks()[0].state, vocab::EJECTED);

        // It returns: mounted again (news).
        let before = *signal.borrow();
        storage.reconcile(&[fatter]);
        assert_ne!(*signal.borrow(), before);
        assert_eq!(storage.banks()[0].state, vocab::MOUNTED);
    }

    // ---- bank file operations -----------------------------------------

    /// The safe_join law: relative in, under-the-bank out; every escape
    /// refuses with a message that teaches (R3.3).
    #[test]
    fn safe_join_refuses_every_escape() {
        let mount = Path::new("/media/vault");
        assert_eq!(
            safe_join(mount, "dumps/redis.rdb").unwrap(),
            mount.join("dumps").join("redis.rdb")
        );
        assert_eq!(safe_join(mount, "a/./b").unwrap(), mount.join("a").join("b"));

        for bad in [
            "../sibling",
            "a/../../b",
            "/etc/passwd",
            "\\windows\\escape",
            ".zen-garden/manifest.json",
            "seeds/.zen-garden/x",
            "",
            ".",
        ] {
            assert!(
                matches!(safe_join(mount, bad), Err(FilesError::BadPath(_))),
                "'{bad}' must refuse"
            );
        }
    }

    /// The CRUD roundtrip over a real mounted bank: write, list, read,
    /// delete — and the adoption record stays invisible the whole way.
    #[test]
    fn bank_files_crud_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("zg-files-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&tmp).unwrap();
        let storage = Storage::new();
        storage
            .adopt(
                &VolumeFact {
                    roles: Vec::new(),
                    mount_point: tmp.clone(),
                    device_id: None,
                    fqn: None,
                    capacity_bytes: 1_000_000,
                    available_bytes: 900_000,
                },
                "seed-vault",
                "stone-1",
            )
            .unwrap();
        let (_, root) = storage.bank_root("seed-vault").unwrap();

        // Write through a directory that does not exist yet.
        let n = write_file(&root, &safe_join(&root, "dumps/redis.rdb").unwrap(), b"RDBDATA").unwrap();
        assert_eq!(n, 7);
        // Overwrite is a write too.
        let n = write_file(&root, &safe_join(&root, "dumps/redis.rdb").unwrap(), b"RDBDATA2").unwrap();
        assert_eq!(n, 8);

        let entries = list_dir(&root, &root).unwrap();
        assert_eq!(entries.len(), 1, "the adoption record is invisible");
        assert_eq!(entries[0].name, "dumps");
        assert_eq!(entries[0].kind, "dir");

        let dumps = safe_join(&root, "dumps").unwrap();
        let entries = list_dir(&root, &dumps).unwrap();
        assert_eq!(entries[0].name, "redis.rdb");
        assert_eq!(entries[0].kind, "file");
        assert_eq!(entries[0].size_bytes, Some(8));

        let bytes = read_file(&root, &safe_join(&root, "dumps/redis.rdb").unwrap()).unwrap();
        assert_eq!(bytes, b"RDBDATA2");

        delete_file(&root, &safe_join(&root, "dumps/redis.rdb").unwrap()).unwrap();
        assert!(matches!(
            read_file(&root, &safe_join(&root, "dumps/redis.rdb").unwrap()),
            Err(FilesError::Missing(_))
        ));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Verbs and kinds agree: reading a directory refuses with the way
    /// out, deleting one refuses louder, and the manifest dotfolder is
    /// ceremony-owned at every level.
    #[test]
    fn file_verbs_refuse_the_wrong_kind_and_the_manifest() {
        let tmp = std::env::temp_dir().join(format!("zg-files2-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(tmp.join("sub")).unwrap();
        let storage = Storage::new();
        storage
            .adopt(
                &VolumeFact {
                    roles: Vec::new(),
                    mount_point: tmp.clone(),
                    device_id: None,
                    fqn: None,
                    capacity_bytes: 1,
                    available_bytes: 1,
                },
                "seed-vault",
                "stone-1",
            )
            .unwrap();
        let (_, root) = storage.bank_root("seed-vault::default").unwrap();

        assert!(matches!(
            read_file(&root, &safe_join(&root, "sub").unwrap()),
            Err(FilesError::NotThatKind(_))
        ));
        assert!(matches!(
            delete_file(&root, &safe_join(&root, "sub").unwrap()),
            Err(FilesError::NotThatKind(_))
        ));
        assert!(matches!(
            safe_join(&root, "sub/.zen-garden/thing").unwrap_err(),
            FilesError::BadPath(_)
        ));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// A symlink on the media pointing outward cannot launder a read past
    /// the prefix check (the lexical law's second line of defense).
    #[cfg(unix)]
    #[test]
    fn symlink_escape_refuses() {
        let outside = std::env::temp_dir().join(format!("zg-outside-{}", uuid::Uuid::now_v7()));
        let tmp = std::env::temp_dir().join(format!("zg-files3-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(outside.join("secret"), b"nope").unwrap();
        std::os::unix::fs::symlink(&outside, tmp.join("door")).unwrap();

        let storage = Storage::new();
        storage
            .adopt(
                &VolumeFact {
                    roles: Vec::new(),
                    mount_point: tmp.clone(),
                    device_id: None,
                    fqn: None,
                    capacity_bytes: 1,
                    available_bytes: 1,
                },
                "seed-vault",
                "stone-1",
            )
            .unwrap();
        let (_, root) = storage.bank_root("seed-vault").unwrap();
        let smuggled = safe_join(&root, "door/secret").unwrap();
        assert!(
            matches!(read_file(&root, &smuggled), Err(FilesError::BadPath(_))),
            "the symlink must not read outside the bank"
        );
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&outside);
    }

    /// The bank_root gate: unknown banks and ejected banks refuse before
    /// any filesystem question is asked.
    #[test]
    fn bank_root_gates_unknown_and_ejected() {
        let tmp = std::env::temp_dir().join(format!("zg-files4-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&tmp).unwrap();
        let storage = Storage::new();
        storage.reconcile(&[vol(tmp.to_str().unwrap(), true)]);

        assert!(matches!(
            storage.bank_root("ghost"),
            Err(FilesError::UnknownBank(_))
        ));
        storage.eject("seed-vault").unwrap();
        assert!(matches!(
            storage.bank_root("seed-vault"),
            Err(FilesError::NotMounted(_))
        ));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
