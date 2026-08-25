//! The attachment memory: rake tends to the same stone across sessions.
//!
//! Harvested from the PoC (rake/src/tending.rs): one small JSON file,
//! no TTL — rake stays attached until the stone goes unreachable. A soft
//! origin: flushed when it matches a failed connection.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Where rake's heart currently rests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tending {
    pub stone_name: String,
    /// `ip:port` of the moss HTTP surface.
    pub endpoint: String,
    /// Unix seconds of the last successful conversation.
    pub last_seen_secs: u64,
}

/// Default location: `~/.zen-garden/.tending`.
pub fn default_path() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(home).join(".zen-garden").join(".tending"))
}

pub fn read_from(path: &std::path::Path) -> Option<Tending> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn write_to(path: &std::path::Path, tending: &Tending) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(tending)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, bytes)
}

pub fn clear_at(path: &std::path::Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

impl Tending {
    pub fn now(stone_name: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            stone_name: stone_name.into(),
            endpoint: endpoint.into(),
            last_seen_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn temp_path(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rake-tending-test-{}-{tag}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(".tending")
    }

    #[test]
    fn roundtrips_and_clears() {
        let path = temp_path("roundtrip");
        let t = Tending::now("stone-a", "192.168.1.5:7285");
        write_to(&path, &t).unwrap();
        let back = read_from(&path).unwrap();
        assert_eq!(back.stone_name, "stone-a");
        assert_eq!(back.endpoint, "192.168.1.5:7285");
        clear_at(&path).unwrap();
        assert!(read_from(&path).is_none());
        clear_at(&path).unwrap(); // idempotent
    }
}
