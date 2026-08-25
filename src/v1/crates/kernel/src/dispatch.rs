//! Handler dispatch (CODE-RULES R2.9).
//!
//! Handlers register for the announcement types they claim and receive a
//! bounded queue to pull from. Types nobody claims are ignored — visibly,
//! with counters. Ingress never blocks on a slow handler: a full queue
//! drops and counts (posture, B3), it never stalls the socket.

use crate::ingress::Ingested;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// What it takes to claim a message type: an async closure over the
/// ingested datagram. Handlers own their concurrency; we only deliver.
pub type HandlerFn =
    Arc<dyn Fn(Ingested) -> futures::future::BoxFuture<'static, ()> + Send + Sync>;

/// Per-handler queue capacity. Falling further behind drops — counted, warned.
pub const HANDLER_QUEUE: usize = 256;

/// Dispatch health, for posture surfaces (B3).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DispatchStats {
    pub delivered: u64,
    pub dropped: u64,
    pub unclaimed: u64,
}

#[derive(Default)]
struct Counters {
    delivered: AtomicU64,
    dropped: AtomicU64,
    unclaimed: AtomicU64,
}

type Registry = Arc<std::sync::Mutex<HashMap<String, Vec<mpsc::Sender<Ingested>>>>>;

/// The dispatch half. Clone freely; all clones feed one registry.
#[derive(Clone)]
pub struct Dispatcher {
    tx: mpsc::Sender<Ingested>,
    registry: Registry,
    counters: Arc<Counters>,
}

/// The consuming half: drive it to move queued datagrams to handlers.
pub struct DispatcherHandle {
    rx: mpsc::Receiver<Ingested>,
    registry: Registry,
    counters: Arc<Counters>,
}

impl Dispatcher {
    /// Create the pair; `capacity` bounds the ingress queue.
    pub fn new(capacity: usize) -> (Self, DispatcherHandle) {
        let (tx, rx) = mpsc::channel(capacity);
        let registry = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let counters = Arc::new(Counters::default());
        let handle = DispatcherHandle {
            rx,
            registry: registry.clone(),
            counters: counters.clone(),
        };
        (Self { tx, registry, counters }, handle)
    }

    /// Queue for ingestion. Awaiting here is the only backpressure path.
    pub async fn ingest(&self, msg: Ingested) {
        let _ = self.tx.send(msg).await;
    }

    /// Claim `kind`. Returns the handler's pull queue.
    pub fn claim(&self, kind: &str) -> mpsc::Receiver<Ingested> {
        let (tx, rx) = mpsc::channel(HANDLER_QUEUE);
        self.registry.lock().expect("registry lock").entry(kind.into()).or_default().push(tx);
        rx
    }

    /// Current counters (B3 posture).
    pub fn stats(&self) -> DispatchStats {
        self.counters.snapshot()
    }
}

impl DispatcherHandle {
    /// Deliver queued datagrams to claimants until cancelled. A type with
    /// no claimants is ignored — counted, so silence is measurable.
    pub async fn run(mut self, token: CancellationToken) -> DispatchStats {
        loop {
            tokio::select! {
                _ = token.cancelled() => return self.counters.snapshot(),
                msg = self.rx.recv() => {
                    let Some(msg) = msg else { return self.counters.snapshot() };
                    let claimants = self.registry.lock().expect("registry lock")
                        .get(&msg.announcement.kind).cloned().unwrap_or_default();
                    if claimants.is_empty() {
                        self.counters.unclaimed.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                    for tx in claimants {
                        match tx.try_send(msg.clone()) {
                            Ok(()) => { self.counters.delivered.fetch_add(1, Ordering::Relaxed); }
                            Err(mpsc::error::TrySendError::Full(_)) => {
                                self.counters.dropped.fetch_add(1, Ordering::Relaxed);
                                tracing::warn!(kind = %msg.announcement.kind, "handler queue full; dropped");
                            }
                            Err(mpsc::error::TrySendError::Closed(_)) => {
                                tracing::debug!(kind = %msg.announcement.kind, "handler gone");
                            }
                        }
                    }
                }
            }
        }
    }
}
