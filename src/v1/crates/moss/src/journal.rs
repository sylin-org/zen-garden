//! The journal (ADR-0015): ONE typed, seq'd, persisted event stream —
//! the stone's memory and its coordination spine. Services append
//! atomic facts; everything else (the pulse, jobs, audit, delivery
//! retries, observe) is a projection or a reaction. Events are the
//! only way contexts coordinate: a context may call itself, contexts
//! talk in events.
//!
//! Durability is dumb-storage-friendly by the checkpoint's own law
//! (ADR-0005 §3): append-only JSONL, one line one event, replay at
//! boot. The seq is monotonic per stone; a gap means a line was lost,
//! and readers say so.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

/// The vocabulary (ADR-0015). One enum: a fact the stone can state.
/// Kinds ride as kebab-case strings on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "kind", content = "data")]
pub enum Kind {
    // room
    PeerSeen { stone: String },
    PeerExpired { stone: String },
    GoodbyeSpoken { stone: String },
    // garden
    OfferingPlanted { fqn: String },
    OfferingRested { fqn: String },
    OfferingWoke { fqn: String },
    OfferingUprooted { fqn: String },
    OfferingReplanted { fqn: String, predecessor: String },
    // will
    RunStarted { fqn: String, run: String },
    CheckpointCommitted { fqn: String, run: String },
    CheckpointDelivered { fqn: String, run: String, sink: String },
    RunAborted { fqn: String, run: String, reason: String },
    // stores
    BankAdopted { bank: String },
    BankEjected { bank: String },
    RolesDeclared { bank: String },
    FileWritten { bank: String, path: String },
}

impl Kind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PeerSeen { .. } => "peer-seen",
            Self::PeerExpired { .. } => "peer-expired",
            Self::GoodbyeSpoken { .. } => "goodbye-spoken",
            Self::OfferingPlanted { .. } => "offering-planted",
            Self::OfferingRested { .. } => "offering-rested",
            Self::OfferingWoke { .. } => "offering-woke",
            Self::OfferingUprooted { .. } => "offering-uprooted",
            Self::OfferingReplanted { .. } => "offering-replanted",
            Self::RunStarted { .. } => "run-started",
            Self::CheckpointCommitted { .. } => "checkpoint-committed",
            Self::CheckpointDelivered { .. } => "checkpoint-delivered",
            Self::RunAborted { .. } => "run-aborted",
            Self::BankAdopted { .. } => "bank-adopted",
            Self::BankEjected { .. } => "bank-ejected",
            Self::RolesDeclared { .. } => "roles-declared",
            Self::FileWritten { .. } => "file-written",
        }
    }
}

/// One atomic fact: what happened, to whom, when, in what order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub seq: u64,
    pub at: DateTime<Utc>,
    pub kind: Kind,
}

/// The stone's journal. Clone freely; all clones share seq, file, and
/// channel.
#[derive(Clone)]
pub struct Journal {
    file: Arc<Mutex<Option<std::fs::File>>>,
    path: Arc<PathBuf>,
    tx: Arc<broadcast::Sender<Event>>,
    seq: Arc<AtomicU64>,
}

impl Journal {
    /// Open (or create) the stream and re-seq from what survives.
    /// Boot convergence is replay, not amnesia.
    pub fn open(path: PathBuf) -> std::io::Result<Self> {
        let seq = Self::replay_seq(&path)?;
        let file = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
        let (tx, _) = broadcast::channel(1024);
        Ok(Self {
            file: Arc::new(Mutex::new(Some(file))),
            path: Arc::new(path),
            tx: Arc::new(tx),
            seq: Arc::new(AtomicU64::new(seq)),
        })
    }

    /// An in-memory journal (tests; throws durability away).
    pub fn memory() -> Self {
        let (tx, _) = broadcast::channel(256);
        Self {
            file: Arc::new(Mutex::new(None)),
            path: Arc::new(PathBuf::new()),
            tx: Arc::new(tx),
            seq: Arc::new(AtomicU64::new(0)),
        }
    }

    /// The highest seq on record for this stream (0 when fresh).
    fn replay_seq(path: &std::path::Path) -> std::io::Result<u64> {
        Ok(Self::replay(path)?
            .last()
            .map(|e| e.seq)
            .unwrap_or_default())
    }

    /// Read every surviving event, oldest first. A torn tail line is
    /// skipped with a loud count — facts are only facts when they parse.
    pub fn replay(path: &std::path::Path) -> std::io::Result<Vec<Event>> {
        let Ok(file) = std::fs::File::open(path) else {
            return Ok(Vec::new());
        };
        let mut events = Vec::new();
        let mut torn = 0u64;
        for line in std::io::BufReader::new(file).lines() {
            let Ok(line) = line else { break };
            match serde_json::from_str::<Event>(&line) {
                Ok(ev) => events.push(ev),
                Err(_) => torn += 1,
            }
        }
        if torn > 0 {
            tracing::warn!(path = %path.display(), torn, "journal tail skipped unreadable lines");
        }
        Ok(events)
    }

    /// Append one fact: stamped, persisted (when durable), broadcast.
    /// Safe with no listeners; the journal is the truth, not the echo.
    pub fn append(&self, kind: Kind) -> Event {
        let event = Event {
            seq: self.seq.fetch_add(1, Ordering::Relaxed) + 1,
            at: Utc::now(),
            kind,
        };
        if let Ok(mut guard) = self.file.lock() {
            if let Some(file) = guard.as_mut() {
                if let Ok(mut line) = serde_json::to_string(&event) {
                    line.push('\n');
                    let _ = file.write_all(line.as_bytes());
                    let _ = file.flush();
                }
            }
        }
        let _ = self.tx.send(event.clone());
        event
    }

    /// Live subscription: every event from now on.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }

    /// The stream's current length (the last seq stamped).
    pub fn seq(&self) -> u64 {
        self.seq.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[test]
    fn append_stamps_seqs_and_broadcasts() {
        let journal = Journal::memory();
        let mut rx = journal.subscribe();
        let a = journal.append(Kind::OfferingPlanted { fqn: "ntfy::default".into() });
        let b = journal.append(Kind::CheckpointCommitted {
            fqn: "ntfy::default".into(),
            run: "r1".into(),
        });
        assert_eq!((a.seq, b.seq), (1, 2), "seqs are the stone's order");
        assert_eq!(rx.try_recv().unwrap().kind, a.kind, "live listeners hear it");
        assert_eq!(journal.seq(), 2);
    }

    #[test]
    fn durable_journal_replays_oldest_first_and_reseqs() {
        let tmp = std::env::temp_dir().join(format!("zg-journal-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("journal.jsonl");

        let journal = Journal::open(path.clone()).unwrap();
        journal.append(Kind::PeerSeen { stone: "stone-a".into() });
        journal.append(Kind::GoodbyeSpoken { stone: "stone-a".into() });
        drop(journal);

        let events = Journal::replay(&path).unwrap();
        assert_eq!(events.len(), 2, "both facts survive");
        assert_eq!(events[0].seq, 1);
        assert_eq!(events[1].kind, Kind::GoodbyeSpoken { stone: "stone-a".into() });

        // Reopening continues the record; the stone does not forget.
        let reopened = Journal::open(path.clone()).unwrap();
        let next = reopened.append(Kind::PeerExpired { stone: "stone-a".into() });
        assert_eq!(next.seq, 3, "re-seq from what survives");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn a_torn_tail_is_skipped_loudly_not_lethal() {
        let tmp = std::env::temp_dir().join(format!("zg-journal-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("journal.jsonl");
        std::fs::write(&path, "{\"seq\":1,\"at\":\"x\",\"kind\":\"peer-seen\"}\n{torn\n").unwrap();

        let events = Journal::replay(&path).unwrap();
        assert_eq!(events.len(), 0, "the torn line is not a fact");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn kinds_keep_their_wire_names() {
        assert_eq!(
            Kind::OfferingReplanted { fqn: String::new(), predecessor: String::new() }.as_str(),
            "offering-replanted"
        );
        assert_eq!(Kind::CheckpointDelivered {
            fqn: String::new(),
            run: String::new(),
            sink: String::new()
        }
        .as_str(), "checkpoint-delivered");
    }
}
