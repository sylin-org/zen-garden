//! The audit ledger (OFFERINGS.md rehydration contract): every lifecycle
//! transition appends a hash-chained event to `{offering_dir}/events.jsonl`.
//!
//! Chain law: each link commits to `seq|prev_hash|kind|details` via FNV-1a
//! 64 — stable across processes; `validate` RECOMPUTES hashes, so any
//! tampered byte breaks every later link. Visible, not silent.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub seq: u64,
    pub at: chrono::DateTime<chrono::Utc>,
    pub kind: String,
    pub details: serde_json::Value,
    pub hash: String,
    pub prev_hash: String,
}

/// Stable hash (FNV-1a 64) — cross-process, cross-version persistence.
fn fnv64(input: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in input.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// The canonical payload a link's hash commits to.
fn commit_string(seq: u64, prev: &str, kind: &str, details: &serde_json::Value) -> String {
    format!("{seq}|{prev}|{kind}|{details}")
}

pub struct EventLog {
    file: PathBuf,
}

impl EventLog {
    /// The offering's audit ledger lives INSIDE its directory (ADR-0001),
    /// nested `{stem}/{instance}` under the namespace law — the same
    /// traversal every other artifact uses.
    pub fn for_dir(dir: &Path, offering_name: &str) -> Self {
        Self {
            file: dir
                .join(super::directory::OfferingDir::new(dir, offering_name).root)
                .join("events.jsonl"),
        }
    }

    /// The chain inside an offering directory ROOT (the saga knows the
    /// root; the namespace law already applied).
    pub fn for_root(root: &Path) -> Self {
        Self { file: root.join("events.jsonl") }
    }

    /// Append an event, chaining onto whatever exists.
    pub fn append(&self, kind: &str, details: serde_json::Value) -> Result<AuditEvent, String> {
        let (prev_hash, seq) = self.tail().unwrap_or((String::new(), 0));
        let seq = seq + 1;
        let hash =
            format!("{:016x}", fnv64(&commit_string(seq, &prev_hash, kind, &details)));
        let event =
            AuditEvent { seq, at: chrono::Utc::now(), kind: kind.into(), details, hash, prev_hash };
        if let Some(parent) = self.file.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("events dir {}: {e}", parent.display()))?;
        }
        use std::io::Write;
        let line = serde_json::to_string(&event).map_err(|e| e.to_string())?;
        let mut fh = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file)
            .map_err(|e| format!("events open {}: {e}", self.file.display()))?;
        writeln!(fh, "{line}").map_err(|e| format!("events write: {e}"))?;
        Ok(event)
    }

    /// Last event's (hash, seq); None when empty or missing.
    fn tail(&self) -> Option<(String, u64)> {
        let content = std::fs::read_to_string(&self.file).ok()?;
        let last = content.lines().last()?;
        let parsed: AuditEvent = serde_json::from_str(last).ok()?;
        Some((parsed.hash, parsed.seq))
    }

    /// Replay and verify by RECOMPUTING each hash from its fields.
    /// Returns the event count; Err names the first broken entry.
    #[allow(dead_code)] // consumed by integrity surfaces in O3
    pub fn validate(&self) -> Result<u64, String> {
        let content = std::fs::read_to_string(&self.file).map_err(|e| e.to_string())?;
        let mut prev = String::new();
        let mut count = 0u64;
        for (i, line) in content.lines().enumerate() {
            let ev: AuditEvent =
                serde_json::from_str(line).map_err(|e| format!("line {}: {e}", i + 1))?;
            if ev.seq != count + 1 {
                return Err(format!("sequence break at line {}", i + 1));
            }
            if ev.prev_hash != prev {
                return Err(format!("chain break at line {}", i + 1));
            }
            let recomputed =
                format!("{:016x}", fnv64(&commit_string(ev.seq, &prev, &ev.kind, &ev.details)));
            if recomputed != ev.hash {
                return Err(format!("hash mismatch at line {}", i + 1));
            }
            prev = ev.hash;
            count += 1;
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    // R4.1: unwrap/expect sanctioned in tests.
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    fn fresh_log(tag: &str) -> EventLog {
        let base =
            std::env::temp_dir().join(format!("ev-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        EventLog { file: base.join("events.jsonl") }
    }

    #[test]
    fn append_builds_a_continuous_chain() {
        let log = fresh_log("append");
        log.append("Placed", serde_json::json!({"image":"mongo:7"})).unwrap();
        log.append("Started", serde_json::json!({})).unwrap();
        log.append("Stopped", serde_json::json!({"reason":"rest"})).unwrap();
        assert_eq!(log.validate().unwrap(), 3);
    }

    /// The point of the chain: tamper one byte of history and validation
    /// breaks visibly at the edited entry.
    #[test]
    fn tampering_breaks_validation() {
        use std::io::Write;
        let log = fresh_log("tamper");
        log.append("Placed", serde_json::json!({"image":"mongo:7"})).unwrap();
        log.append("Started", serde_json::json!({})).unwrap();

        let raw = std::fs::read_to_string(&log.file).unwrap();
        let lines: Vec<String> = raw.lines().map(str::to_string).collect();
        assert!(lines.len() == 2, "expected 2 events");
        std::fs::write(&log.file, "").unwrap();
        for (i, l) in lines.iter().enumerate() {
            // Silently change history on line 1 — keep everything else.
            let l2 = if i == 0 { l.replace("mongo:7", "mongo:99") } else { l.clone() };
            let mut f = std::fs::OpenOptions::new().append(true).open(&log.file).unwrap();
            writeln!(f, "{l2}").unwrap();
        }

        let err = log.validate().unwrap_err();
        assert!(err.contains("hash mismatch") || err.contains("line 1"), "{err}");
    }
}

impl EventLog {
    /// Translate a chain kind into the stone journal's typed fact and
    /// append it. Unknown kinds are chain-local history and stay there.
    pub(crate) fn tail_kind_of(
        &self,
        kind: &str,
        name: &str,
        details: &serde_json::Value,
        journal: &crate::journal::Journal,
    ) {
        use crate::journal::Kind;
        let fact = match kind {
            "Placed" => Kind::OfferingPlanted { fqn: name.to_string() },
            "Stopped" => Kind::OfferingRested { fqn: name.to_string() },
            "Started" => Kind::OfferingWoke { fqn: name.to_string() },
            "Uprooted" => Kind::OfferingUprooted { fqn: name.to_string() },
            "Replanted" => Kind::OfferingReplanted {
                fqn: name.to_string(),
                predecessor: details["predecessor_offering_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            },
            _ => return, // chain-local history (Resurrected, Healed, ...)

        };
        journal.append(fact);
    }
}
