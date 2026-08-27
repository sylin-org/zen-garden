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
    /// tiers). Role news is state news - it bumps. #[cfg(test)] until the
    /// sink-role declaration slice surfaces it on the API/rake pair.
    #[cfg(test)]
    pub fn set_roles(&self, fqn: &str, roles: Vec<String>) -> Option<Bank> {
        let mut banks = self.banks.lock();
        let bank = banks.get_mut(fqn)?;
        bank.roles = roles;
        let updated = bank.clone();
        drop(banks);
        self.bump();
        Some(updated)
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
        };
        let dir = vol.mount_point.join(MANIFEST_DIR);
        std::fs::create_dir_all(&dir).map_err(|e| AdoptError::Io(format!("{}: {e}", dir.display())))?;
        let file = dir.join(MANIFEST_FILE);
        let bytes = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| AdoptError::Io(format!("encode: {e}")))?;
        std::fs::write(&file, bytes)
            .map_err(|e| AdoptError::Io(format!("{}: {e}", file.display())))?;

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
                    }
                    None => {
                        tracing::info!(bank = %fqn, "recognized an adopted bank at boot/plug-in");
                        banks.insert(
                            fqn.clone(),
                            Bank {
                                fqn: fqn.clone(),
                                device_id: device_id.clone(),
                                state: vocab::MOUNTED.into(),
                                roles: Vec::new(),
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
}

/// Read the garden manifest off a volume, if one rides it.
fn read_manifest(mount: &Path) -> Option<BankManifest> {
    let file = mount.join(MANIFEST_DIR).join(MANIFEST_FILE);
    let bytes = std::fs::read(file).ok()?;
    serde_json::from_slice(&bytes).ok()
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
}
