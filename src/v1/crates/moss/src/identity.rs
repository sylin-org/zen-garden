//! Stone identity: minted once, then persistent and immutable (D6).
//!
//! First boot mints a GUIDv7 and a poetical name (glossary::naming),
//! collision-checked against the room the PoC way — ten attempts, then a
//! hex-suffix fallback. Every later boot loads the same identity. An
//! explicit `--stone-name` renames by operator intent (the id never
//! changes); otherwise the flag is absent and the well speaks.

use crate::room::probe;
use garden_glossary::naming;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

/// How many poetic candidates get a collision check before falling back.
const MINT_ATTEMPTS: usize = 10;
/// Collision-check listen window per attempt.
const CHECK_WINDOW_MS: u64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum HostModality {
    /// Moss is a guest on a cohabiting machine (workstation, laptop):
    /// the garden name lives in the garden only; the host's own name is
    /// never touched.
    #[serde(rename = "companion")]
    #[default]
    Companion,
    /// Dedicated hardware: the stone IS the box, so the host takes the
    /// stone's name (PoC first-boot parity: hostname + hosts file).
    /// Set by the appliance installer, not by moss itself.
    #[serde(rename = "appliance")]
    Appliance,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Identity {
    pub stone_id: String,
    pub stone_name: String,
    /// How this stone relates to its host machine (L23). Recorded at mint;
    /// flipped to `appliance` by the dedicated-hardware installer.
    #[serde(default)]
    pub host_modality: HostModality,
}

fn path() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(home).join(".zen-garden").join("identity.json"))
}

fn read() -> Option<Identity> {
    let bytes = std::fs::read(path()?).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn write(identity: &Identity) -> Result<(), String> {
    let path = path().ok_or_else(|| "no home directory known".to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    let bytes = serde_json::to_vec_pretty(identity)
        .map_err(|e| format!("encode identity: {e}"))?;
    std::fs::write(&path, bytes).map_err(|e| format!("write {}: {e}", path.display()))
}

/// Load this stone's identity or mint one. Pipeline step shape: Err aborts
/// startup loudly (L17) — an unwritable identity is a real failure, not a
/// quirk to shrug off.
pub async fn load_or_mint(
    explicit_name: Option<&str>,
    discovery: &crate::room::config::DiscoveryConfig,
) -> Result<Identity, String> {
    if let Some(existing) = read() {
        match explicit_name {
            Some(want) if want != existing.stone_name => {
                // Rename is operator intent; the id is untouched (D6).
                let renamed =
                    Identity {
                        stone_id: existing.stone_id.clone(),
                        stone_name: want.to_string(),
                        host_modality: existing.host_modality,
                    };
                write(&renamed)?;
                tracing::info!(from = %existing.stone_name, to = %want, "stone renamed");
                Ok(renamed)
            }
            _ => Ok(existing),
        }
    } else {
        let stone_id = Uuid::now_v7().to_string();
        let stone_name = match explicit_name {
            Some(want) => want.to_string(),
            None => poetic_mint(discovery.port, discovery.group).await,
        };
        let identity = Identity { stone_id, stone_name, host_modality: HostModality::default() };
        write(&identity)?;
        tracing::info!(name = %identity.stone_name, "identity minted");
        Ok(identity)
    }
}

/// Ten random combinations, each checked against the room's current names;
/// then a hex-suffix fallback (PoC parity, ask/tell edition).
async fn poetic_mint(port: u16, group: Ipv4Addr) -> String {
    let mut last = String::new();
    for attempt in 0..MINT_ATTEMPTS {
        let entropy = Uuid::now_v7();
        let b = entropy.as_bytes();
        let adj_idx = u16::from_be_bytes([b[5], b[6]]) as usize;
        let noun_idx = u16::from_be_bytes([b[10], b[11]]) as usize;
        let candidate = naming::compose(adj_idx, noun_idx);
        last.clone_from(&candidate);

        let taken = probe::ask_the_room(
            port,
            Some(group),
            Duration::from_millis(CHECK_WINDOW_MS),
            &format!("naming-{attempt}"),
        )
        .await
        .unwrap_or_default()
        .iter()
        .any(|r| r.stone.name == candidate);

        if !taken {
            return candidate;
        }
        tracing::info!(candidate = %candidate, attempt = attempt + 1, "name taken");
    }
    let hex4 = &Uuid::now_v7().simple().to_string()[..4];
    naming::with_hex_suffix(&last, hex4)
}
