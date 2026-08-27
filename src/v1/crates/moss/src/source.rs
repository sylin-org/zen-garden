//! The daemon's voice: what this stone says when it chirps or sings.
//!
//! [`DynamicChirpSource`] speaks the registry's AND the storage's truth in
//! TWO registers (ADR-0004 A2.1/A2.2; ADR-0005 §8): `body()` is the LEAN
//! heartbeat — anchors plus rev-only inventory blocks, because presence
//! must not amortize inventory; `song_blocks()` is the full voice — the
//! domain inventories the framer quantizes into songs, spoken on boot and
//! on change (the announcer's version watch fires; L18 — the machinery
//! existed since W1; this is the composer it waited for).

use garden_contract::chirp::{
    BankEntry, ChirpFrame, Inventory, InventoryMap, Moss, Network, PeerAddress, Presence,
    Reception, ServiceEntry, Stone, INVENTORY_CAP,
};
use garden_kernel::announce::ChirpSource;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Best-effort LAN address for the chirp: first eligible non-loopback IPv4,
/// loopback as honest fallback (a lone stone on a laptop still speaks).
pub fn local_lan_ip() -> IpAddr {
    if let Ok(ifaces) = if_addrs::get_if_addrs() {
        let lan = ifaces.into_iter().find_map(|iface| match iface.ip() {
            IpAddr::V4(v4) if !v4.is_loopback() && !v4.is_link_local() => Some(IpAddr::V4(v4)),
            _ => None,
        });
        if let Some(ip) = lan {
            return ip;
        }
    }
    IpAddr::from(std::net::Ipv4Addr::LOCALHOST)
}

/// What a chirp source needs to speak for this stone. Clone freely.
#[derive(Clone)]
pub struct Voice {
    pub stone_id: String,
    pub stone_name: String,
    pub http_port: u16,
    pub moss_version: String,
}

/// A chirp source that speaks the registry's truth. `rev` starts at the
/// boot snapshot's size (offerings loaded at boot are generation 1+) and
/// climbs on every OfferingChanged — monotonic per boot, per ADR-0004.
pub struct DynamicChirpSource {
    voice: Voice,
    boot_id: String,
    registry: Arc<crate::offerings::registry::Registry>,
    rev: AtomicU64,
    /// The bank revision (ADR-0005 §8.1): rides beside svc_rev, bumped on
    /// storage news — mount/eject/rename/visibility, never measurements.
    bank_rev: AtomicU64,
    storage: Arc<crate::offerings::storage::Storage>,
    version_tx: tokio::sync::watch::Sender<u64>,
}

impl DynamicChirpSource {
    /// Start from the CURRENT registry snapshot and follow
    /// OfferingChanged from here ([`follow_offering_changes`]).
    pub fn new(
        voice: Voice,
        boot_id: String,
        registry: Arc<crate::offerings::registry::Registry>,
        storage: Arc<crate::offerings::storage::Storage>,
    ) -> Arc<Self> {
        let initial = (registry.snapshot().len() as u64).max(1);
        let (version_tx, _) = tokio::sync::watch::channel(0);
        Arc::new(Self {
            voice,
            boot_id,
            registry,
            rev: AtomicU64::new(initial),
            bank_rev: AtomicU64::new(1),
            storage,
            version_tx,
        })
    }

    /// Storage news arrived: the bank_rev speaks (ADR-0005 §8.1) and the
    /// announcer's debounce turns it into a song. NB: send_modify under
    /// one lock — never borrow inside send_replace (the S2 idiom).
    pub fn bump_bank_rev(&self) {
        self.bank_rev.fetch_add(1, Ordering::Relaxed);
        self.version_tx.send_modify(|v| *v = v.wrapping_add(1));
    }

    /// The services inventory block, composed fresh from the aggregate.
    fn services_block(&self) -> Inventory<ServiceEntry> {
        let items: Vec<ServiceEntry> =
            self.registry.snapshot().iter().map(|o| o.service_entry()).collect();
        Inventory {
            rev: Some(self.rev.load(std::sync::atomic::Ordering::Relaxed)),
            // Cap truncation is the framer/cache's last resort (A2.3); the
            // composer declares totals only when it actually truncates.
            total: None,
            items,
        }
    }

    /// The LEAN register (A2.1): heartbeats speak the rev, not the items.
    /// A block present with a fresh rev is the truth "I know what I host;
    /// ask me if you're behind."
    fn lean_services(&self) -> Inventory<ServiceEntry> {
        Inventory {
            rev: Some(self.rev.load(std::sync::atomic::Ordering::Relaxed)),
            total: None,
            items: Vec::new(),
        }
    }

    /// The banks' lean register: rev-only, same law (ADR-0005 §8.1).
    fn lean_banks(&self) -> Inventory<BankEntry> {
        Inventory {
            rev: Some(self.bank_rev.load(Ordering::Relaxed)),
            total: None,
            items: Vec::new(),
        }
    }
}

impl DynamicChirpSource {
    /// The banks inventory, composed fresh from storage (ADR-0005 §8.5:
    /// {fqn, device_id, state, roles[], capacity_bytes, used_bytes} —
    /// telemetry rides, it never leads).
    fn banks_block(&self) -> Inventory<BankEntry> {
        let items: Vec<BankEntry> = self
            .storage
            .banks()
            .into_iter()
            .map(|b| BankEntry {
                fqn: b.fqn,
                device_id: b.device_id,
                state: b.state,
                roles: b.roles,
                capacity_bytes: b.capacity_bytes,
                used_bytes: b.used_bytes,
            })
            .collect();
        Inventory {
            rev: Some(self.bank_rev.load(Ordering::Relaxed)),
            total: None,
            items,
        }
    }

}

impl ChirpSource for DynamicChirpSource {
    fn body(&self) -> ChirpFrame {
        let now = chrono::Utc::now();
        ChirpFrame {
            stone: Stone {
                id: self.voice.stone_id.clone(),
                name: self.voice.stone_name.clone(),
                moss: Moss { version: self.voice.moss_version.clone() },
                network: Network {
                    address: PeerAddress {
                        ip: local_lan_ip(),
                        port: self.voice.http_port,
                        tls_port: None,
                    },
                    mac: None,
                },
            },
            presence: Presence {
                health: garden_glossary::health::THRIVING.into(),
                status: garden_glossary::presence::ONLINE.into(),
            },
            inventory: InventoryMap {
                services: Some(self.lean_services()),
                banks: Some(self.lean_banks()),
                ..Default::default()
            },
            meta: garden_contract::chirp::FrameMeta {
                boot_id: Some(self.boot_id.clone()),
                ..Default::default()
            },
            received: Reception { discovered_at: now, last_seen: now },
        }
    }

    /// The full voice: the services and banks blocks, whole (A2.3 — a
    /// block rides entire or waits). The 24-item alphabetical cap is the
    /// LAST resort; truncation is declared by `total`, never silent.
    fn song_blocks(&self) -> Vec<(String, serde_json::Value)> {
        let mut blocks: Vec<(String, serde_json::Value)> = Vec::new();
        let mut services = self.services_block();
        if services.items.len() > INVENTORY_CAP {
            services.items.sort_by(|a, b| a.name.cmp(&b.name));
            services.total = Some(services.items.len() as u32);
            services.items.truncate(INVENTORY_CAP);
        }
        match serde_json::to_value(&services) {
            Ok(v) => blocks.push((garden_contract::chirp::DOMAIN_SERVICES.into(), v)),
            Err(e) => tracing::warn!(error = %e, "services block encode failed; singing silence"),
        }
        let mut banks = self.banks_block();
        if banks.items.len() > INVENTORY_CAP {
            banks.items.sort_by(|a, b| a.fqn.cmp(&b.fqn));
            banks.total = Some(banks.items.len() as u32);
            banks.items.truncate(INVENTORY_CAP);
        }
        match serde_json::to_value(&banks) {
            Ok(v) => blocks.push((garden_contract::chirp::DOMAIN_BANKS.into(), v)),
            Err(e) => tracing::warn!(error = %e, "banks block encode failed; singing silence"),
        }
        blocks
    }

    fn version(&self) -> tokio::sync::watch::Receiver<u64> {
        self.version_tx.subscribe()
    }
}

/// Listen to storage mutations for the life of the stone: every bump is
/// bank news (ADR-0005 §8.1) — rev up, version watch fired, song follows.
/// Call once at startup.
pub fn follow_storage_changes(
    source: &Arc<DynamicChirpSource>,
    storage: &Arc<crate::offerings::storage::Storage>,
    token: tokio_util::sync::CancellationToken,
) {
    let source = Arc::clone(source);
    let mut signal = storage.subscribe();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = token.cancelled() => return,
                changed = signal.changed() => match changed {
                    Ok(()) => source.bump_bank_rev(),
                    Err(_) => return,
                },
            }
        }
    });
}

/// Listen to the registry for the life of the stone: every OfferingChanged
/// bumps the rev and fires the source's version watch (which the announcer
/// debounces into a change chirp). Lagged events still bump once — the next
/// body() recomposes the whole set anyway. Call once at startup.
pub fn follow_offering_changes(
    source: &Arc<DynamicChirpSource>,
    registry: &Arc<crate::offerings::registry::Registry>,
    token: tokio_util::sync::CancellationToken,
) {
    let mut events = registry.events();
    let source = Arc::clone(source);
    tokio::spawn(async move {
        loop {
            // NB: never `.borrow()` inside `send_replace` — the read guard
            // alive across the write attempt deadlocks the watch (the S2
            // blocker). `send_modify` runs under one lock.
            let bump = || {
                source.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                source.version_tx.send_modify(|v| *v = v.wrapping_add(1));
            };
            tokio::select! {
                _ = token.cancelled() => return,
                changed = events.recv() => match changed {
                    Ok(_) => bump(),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(missed = n, "offering events lagged; rev bumped once");
                        bump();
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                },
            }
        }
    });
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::offerings::registry::MemorySnapshotStore;
    use crate::offerings::model::{
        Location, ManagedData, ModeData, Offering, Status,
    };
    use std::collections::HashMap;

    fn voice() -> Voice {
        Voice {
            stone_id: "sid-1".into(),
            stone_name: "stone-test".into(),
            http_port: 7285,
            moss_version: "1.0.0".into(),
        }
    }

    fn source_with(registry: Arc<crate::offerings::registry::Registry>) -> Arc<DynamicChirpSource> {
        DynamicChirpSource::new(
            voice(),
            "boot-1".into(),
            registry,
            Arc::new(crate::offerings::storage::Storage::new()),
        )
    }

    fn planted(name: &str) -> Offering {
        let now = chrono::Utc::now();
        Offering {
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
            mode_data: ModeData::Managed(ManagedData {
                runtime_kind: "oci".into(),
                spec: Default::default(),
                port_map: HashMap::from([("default".to_string(), 7300u16)]),
                plan: None,
            }),
            registered_at: now,
            updated_at: now,
        }
    }

    fn registry_with(items: Vec<Offering>) -> Arc<crate::offerings::registry::Registry> {
        let registry = Arc::new(crate::offerings::registry::Registry::new(Arc::new(
            MemorySnapshotStore::default(),
        )));
        for o in items {
            registry.register(o);
        }
        registry
    }

    #[test]
    fn body_is_lean_and_songs_carry_the_set() {
        let registry = registry_with(vec![planted("redis::default")]);
        let source = source_with(Arc::clone(&registry));

        // The heartbeat: anchors plus a rev-only block (A2.1).
        let frame = source.body();
        assert_eq!(frame.stone.id, "sid-1");
        assert_eq!(frame.meta.boot_id.as_deref(), Some("boot-1"));
        let lean = frame.inventory.services.expect("services block present");
        assert_eq!(lean.rev, Some(1), "boot snapshot = generation 1");
        assert!(lean.items.is_empty(), "heartbeats speak revs, not items");

        // The full voice: the same rev, the actual set (A2.2). Both domains
        // sing — services and banks (ADR-0005 §8).
        let blocks = source.song_blocks();
        assert_eq!(blocks.len(), 2, "services and banks ride the song");
        let services_block = blocks
            .iter()
            .find(|(d, _)| d == garden_contract::chirp::DOMAIN_SERVICES)
            .expect("services block present");
        let full: Inventory<ServiceEntry> =
            serde_json::from_value(services_block.1.clone()).expect("services block decodes");
        assert_eq!(full.items.len(), 1);
        assert_eq!(full.items[0].name, "redis::default");
        assert_eq!(full.items[0].stem, "redis");
        assert_eq!(full.items[0].state.status, "running");
        assert_eq!(full.items[0].ports["default"], 7300);
        assert_eq!(full.rev, Some(1));
    }

    /// The wire cap is the last resort: past 24 items the song carries the
    /// alphabetical head with `total` declared (ADR-0004 §1).
    #[test]
    fn oversized_sets_cap_alphabetically_and_declare_total() {
        let items: Vec<Offering> =
            (0..30).map(|i| planted(&format!("svc{:02}::default", i))).collect();
        let registry = registry_with(items);
        let source = source_with(Arc::clone(&registry));

        let full: Inventory<ServiceEntry> =
            serde_json::from_value(source.song_blocks()[0].1.clone()).expect("decodes");
        assert_eq!(full.items.len(), INVENTORY_CAP, "capped at the wire cap");
        assert_eq!(full.total, Some(30), "truncation declared, never silent");
        let names: Vec<&str> = full.items.iter().map(|e| e.name.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "the alphabetical head rides");
        assert_eq!(names[0], "svc00::default");
    }

    /// The S2 heartbeat: a plant bumps the rev; the version watch fires so
    /// the announcer's existing debounce turns it into a song.
    #[tokio::test]
    async fn offering_change_bumps_rev_and_version() {
        let registry = registry_with(vec![]);
        let source = source_with(Arc::clone(&registry));
        follow_offering_changes(
            &Arc::clone(&source),
            &Arc::clone(&registry),
            tokio_util::sync::CancellationToken::new(),
        );

        let mut version = source.version();
        let before = *version.borrow_and_update();

        registry.register(planted("redis::default"));

        tokio::time::timeout(std::time::Duration::from_secs(2), version.changed())
            .await
            .expect("version watch must fire on OfferingChanged")
            .expect("watch alive");
        assert!(*version.borrow() > before, "version advanced");

        let full: Inventory<ServiceEntry> =
            serde_json::from_value(source.song_blocks()[0].1.clone()).expect("decodes");
        assert_eq!(full.items.len(), 1, "recomposed from the registry");
        assert_eq!(full.rev, Some(2), "second generation");
        assert_eq!(
            source.body().inventory.services.expect("lean block").rev,
            Some(2),
            "the lean register speaks the same generation"
        );
    }

    /// ADR-0005 §8: a plugged bank is news. Adopt bumps the storage watch;
    /// follow_storage_changes raises the bank_rev; the song carries the
    /// banks block with the bank's entries. Lean bodies stay rev-only.
    #[tokio::test]
    async fn storage_news_bumps_bank_rev_and_sings_the_banks() {
        let registry = registry_with(vec![]);
        let storage = Arc::new(crate::offerings::storage::Storage::new());
        let source = DynamicChirpSource::new(
            voice(),
            "boot-1".into(),
            Arc::clone(&registry),
            Arc::clone(&storage),
        );
        follow_storage_changes(
            &source,
            &storage,
            tokio_util::sync::CancellationToken::new(),
        );

        let mut version = source.version();
        let before = *version.borrow_and_update();

        storage
            .adopt(
                &crate::offerings::storage::VolumeFact {
                    mount_point: std::path::PathBuf::from("E:\\"),
                    device_id: None,
                    fqn: None,
                    capacity_bytes: 1000,
                    available_bytes: 500,
                },
                "seed-vault",
                "sid-1",
            )
            .unwrap();

        tokio::time::timeout(std::time::Duration::from_secs(2), version.changed())
            .await
            .expect("storage news must fire the version watch")
            .expect("watch alive");
        assert!(*version.borrow() > before);

        let banks_domain = garden_contract::chirp::DOMAIN_BANKS;
        let banks_block = source
            .song_blocks()
            .into_iter()
            .find(|(domain, _)| domain == banks_domain)
            .expect("banks ride the song");
        let inv: Inventory<BankEntry> =
            serde_json::from_value(banks_block.1).expect("banks block decodes");
        assert_eq!(inv.items.len(), 1);
        assert_eq!(inv.items[0].fqn, "seed-vault::default");
        assert_eq!(inv.items[0].state, garden_glossary::bank::MOUNTED);
        assert_eq!(inv.items[0].capacity_bytes, Some(1000));
        assert!(inv.rev >= Some(2), "the bank_rev spoke");

        // The lean register speaks the rev, not the items.
        let lean = source.body().inventory.banks.expect("lean banks block");
        assert!(lean.items.is_empty());
        assert!(lean.rev >= Some(2));
    }

    /// Uproot also bumps: the SET changed even though a stone may now host
    /// nothing — a block with empty items and a fresh rev defends that truth.
    #[tokio::test]
    async fn removal_bumps_rev_and_empties_items() {
        let registry = registry_with(vec![planted("redis::default")]);
        let source = source_with(Arc::clone(&registry));
        let id = registry.snapshot()[0].offering_id.clone();
        follow_offering_changes(
            &Arc::clone(&source),
            &Arc::clone(&registry),
            tokio_util::sync::CancellationToken::new(),
        );

        registry.remove(&id);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let full: Inventory<ServiceEntry> =
            serde_json::from_value(source.song_blocks()[0].1.clone()).expect("decodes");
        assert!(full.items.is_empty(), "empty set is still a set");
        assert_eq!(full.rev, Some(2), "removal is a generation too");
    }
}

