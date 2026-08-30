//! The pulse bus (ADR-0013): one broadcast channel of typed, seq'd
//! events, fed by adapters from the sources that already exist —
//! registry, topology, jobs, storage — plus two quiet samplers (load,
//! wire deltas). NO tap on the wire itself (R2.9): transport news is
//! dispatcher counters, and the heartbeat noise of chirps never reaches
//! the feed.

use garden_contract::pulse::{PulseEvent, LEVEL_ERROR, LEVEL_INFO, LEVEL_WARN};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

/// How often the quiet samplers speak (load, wire deltas).
pub const SAMPLER_INTERVAL_SECS: u64 = 10;

/// The stone's pulse. Clone freely; all clones share seq and channel.
#[derive(Clone)]
pub struct Bus {
    tx: Arc<broadcast::Sender<PulseEvent>>,
    seq: Arc<AtomicU64>,
}

impl Default for Bus {
    fn default() -> Self {
        Self::new()
    }
}

impl Bus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(256);
        Self { tx: Arc::new(tx), seq: Arc::new(AtomicU64::new(0)) }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<PulseEvent> {
        self.tx.subscribe()
    }

    /// Stamp the sequence and speak. Safe with no listeners.
    pub fn publish(&self, mut event: PulseEvent) -> PulseEvent {
        event.seq = self.seq.fetch_add(1, Ordering::Relaxed) + 1;
        event.ts = chrono::Utc::now().to_rfc3339();
        let _ = self.tx.send(event.clone());
        event
    }
}

/// The sources the adapters listen to — everything the room already
/// says, gathered once (R1.4: declared dependencies, then spawn).
pub struct Sources {
    pub garden: Arc<crate::garden::service::OfferingService>,
    pub topology: Arc<garden_kernel::topology::Topology>,
    pub jobs: crate::jobs::JobTracker,
    pub storage: Arc<crate::garden::storage::Storage>,
    pub dispatcher: garden_kernel::dispatch::Dispatcher,
    pub ingest: Arc<garden_kernel::ingress::IngestCounters>,
}

/// Run the adapters until cancelled: translate existing events into
/// pulse news, and run the two quiet samplers.
pub async fn run(bus: Bus, sources: Sources, token: CancellationToken) {
    let mut garden = sources.garden.events();
    let mut topology = sources.topology.events();
    let mut jobs = sources.jobs.changes();
    let mut storage = sources.storage.subscribe();
    let mut sampler = tokio::time::interval(std::time::Duration::from_secs(
        SAMPLER_INTERVAL_SECS,
    ));
    sampler.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    sampler.tick().await; // first tick immediate: consumed
    // Storage baseline: the first version is the world as found, not news.
    storage.mark_changed(); // watch fires immediately; consume the synthetic one
    let mut storage_version = *storage.borrow();
    let mut last_wire: Option<(u64, u64, u64, u64, u64)> = None;
    // Job progress dedup: only NEW lines are news.
    let mut last_progress: HashMap<String, Option<String>> = HashMap::new();

    loop {
        tokio::select! {
            _ = token.cancelled() => return,

            ev = garden.recv() => match ev {
                Ok(change) => match change.offering {
                    None => {
                        bus.publish(PulseEvent::new(
                            "offering.removed", "offering", LEVEL_INFO,
                            format!("{} uprooted - removed from the stone", change.name),
                        ).with_offering(change.name));
                    }
                    Some(o) => {
                        let (kind, level) = match o.status.as_str() {
                            "degraded" => ("offering.degraded", LEVEL_WARN),
                            "stopped" => ("offering.stopped", LEVEL_INFO),
                            "running" => ("offering.running", LEVEL_INFO),
                            _ => ("offering.updated", LEVEL_INFO),
                        };
                        bus.publish(PulseEvent::new(
                            kind, "offering", level,
                            format!("{} is {}", o.name, o.status.as_str()),
                        ).with_offering(o.name));
                    }
                },
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    bus.publish(PulseEvent::new(
                        "pulse.lagged", "pulse", LEVEL_WARN,
                        format!("the bus ran {n} events behind - some news was dropped"),
                    ));
                }
                Err(_) => return,
            },

            ev = topology.recv() => match ev {
                Ok(garden_kernel::topology::TopologyEvent::Seen(v)) => {
                    bus.publish(PulseEvent::new(
                        "topology.seen", "topology", LEVEL_INFO,
                        format!("{} is here", v.body.stone.name),
                    ).with_stone(v.body.stone.name.clone()));
                }
                Ok(garden_kernel::topology::TopologyEvent::Goodbye { stone_name, .. }) => {
                    bus.publish(PulseEvent::new(
                        "topology.goodbye", "topology", LEVEL_WARN,
                        format!("{stone_name} said goodbye - removed from the room"),
                    ).with_stone(stone_name));
                }
                Ok(garden_kernel::topology::TopologyEvent::Expired { stone_name, .. }) => {
                    bus.publish(PulseEvent::new(
                        "topology.expired", "topology", LEVEL_WARN,
                        format!("{stone_name} expired - silent past the threshold"),
                    ).with_stone(stone_name));
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    bus.publish(PulseEvent::new(
                        "pulse.lagged", "pulse", LEVEL_WARN,
                        format!("the bus ran {n} events behind - some news was dropped"),
                    ));
                }
                Err(_) => return,
            },

            id = jobs.recv() => match id {
                Ok(id) => {
                    if let Some(job) = sources.jobs.get(&id) {
                        // Subject rides the data sections - one object,
                        // never two writers (a clobber hid progress from
                        // the wall once; never again).
                        let with_subject = |mut e: PulseEvent| {
                            let obj = e.data.get_or_insert_with(|| serde_json::json!({}));
                            if let Some(o) = obj.as_object_mut() {
                                o.insert("subject".into(), serde_json::json!(job.subject));
                            }
                            e
                        };
                        let event = match job.status {
                            crate::jobs::JobStatus::Done => with_subject(PulseEvent::new(
                                "job.done", "job", LEVEL_INFO,
                                format!("{} - done", job.subject),
                            )),
                            crate::jobs::JobStatus::Failed => with_subject(PulseEvent::new(
                                "job.failed", "job", LEVEL_ERROR,
                                format!("{} - failed: {}",
                                    job.subject,
                                    job.error.as_deref().unwrap_or("unknown error")),
                            )),
                            crate::jobs::JobStatus::Interrupted => with_subject(PulseEvent::new(
                                "job.interrupted", "job", LEVEL_WARN,
                                format!("{} - interrupted by restart; ask again", job.subject),
                            )),
                            crate::jobs::JobStatus::Running => {
                                // Progress only when the line is NEW - the
                                // throttle lives in the caller, the dedup here.
                                if last_progress.get(&id) == Some(&job.progress) {
                                    continue;
                                }
                                let event = with_subject(PulseEvent::new(
                                    "job.progress", "job", LEVEL_INFO,
                                    format!("{} - {}",
                                        job.subject,
                                        job.progress.as_deref().unwrap_or("working")),
                                ).with_data(serde_json::json!({
                                    "progress": job.progress,
                                })));
                                last_progress.insert(id.clone(), job.progress.clone());
                                event
                            }
                        };
                        if matches!(job.status, crate::jobs::JobStatus::Done
                            | crate::jobs::JobStatus::Failed
                            | crate::jobs::JobStatus::Interrupted)
                        {
                            last_progress.remove(&id);
                        }
                        bus.publish(event);
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(_) => return,
            },

            v = storage.changed() => {
                if v.is_err() {
                    return;
                }
                let version = *storage.borrow();
                if version == storage_version {
                    continue;
                }
                storage_version = version;
                for bank in sources.storage.banks() {
                    let (kind, level) = match bank.state.as_str() {
                        "ejected" => ("storage.ejected", LEVEL_WARN),
                        "mounted" => ("storage.mounted", LEVEL_INFO),
                        _ => ("storage.changed", LEVEL_INFO),
                    };
                    bus.publish(PulseEvent::new(
                        kind, "storage", level,
                        format!("{} is {}", bank.fqn, bank.state),
                    ).with_data(serde_json::json!({ "roles": bank.roles })));
                }
            },

            _ = sampler.tick() => {
                // The wire's story, at the only seam allowed to tell it:
                // dispatcher and ingest counters (R2.9).
                let d = sources.dispatcher.stats();
                let wire = (d.delivered, d.dropped, d.unclaimed,
                    sources.ingest.parsed(), sources.ingest.deduped());
                if last_wire.is_some_and(|prev| prev != wire) {
                    bus.publish(PulseEvent::new(
                        "wire.delta", "wire", LEVEL_INFO,
                        format!("{} datagrams dispatched", wire.0.saturating_sub(last_wire.map(|w| w.0).unwrap_or(0))),
                    ).with_data(serde_json::json!({
                        "delivered": wire.0, "dropped": wire.1, "unclaimed": wire.2,
                        "parsed": wire.3, "deduped": wire.4,
                    })));
                }
                last_wire = Some(wire);

                // The stone's own load - the gauges' food.
                if let Some(load) = sample_load() {
                    bus.publish(PulseEvent::new(
                        "stone.load", "stone", LEVEL_INFO,
                        format!("cpu {}% · memory {}%", load.cpu, load.memory),
                    ).with_data(serde_json::json!({
                        "cpu_percent": load.cpu, "memory_percent": load.memory,
                    })));
                }
            },
        }
    }
}

struct Load {
    cpu: f32,
    memory: f64,
}

/// One sample of the stone's own vitals.
fn sample_load() -> Option<Load> {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_cpu_usage();
    sys.refresh_memory();
    // CPU usage needs a settle between refreshes to be meaningful.
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    sys.refresh_cpu_usage();
    let cpus = sys.cpus();
    if cpus.is_empty() {
        return None;
    }
    let cpu: f32 = cpus.iter().map(|c| c.cpu_usage()).sum::<f32>() / cpus.len() as f32;
    let total = sys.total_memory() as f64;
    if total == 0.0 {
        return None;
    }
    let memory = (sys.used_memory() as f64 / total) * 100.0;
    Some(Load { cpu, memory })
}

/// The world as the stone sees it, for the feed's snapshot-first
/// opener. `self_row` is the caller's self view — the SAME shape the
/// GardenStones face speaks (B1: one shape, wire to wall).
pub fn snapshot(
    garden: &crate::garden::service::OfferingService,
    topology: &garden_kernel::topology::Topology,
    jobs: &crate::jobs::JobTracker,
    self_row: serde_json::Value,
) -> serde_json::Value {
    let mut stones = vec![self_row];
    for peer in topology.snapshot() {
        let mut v = serde_json::to_value(&peer.body).unwrap_or_default();
        if let Some(obj) = v.as_object_mut() {
            obj.insert("chirps".into(), serde_json::json!(peer.chirps));
        }
        stones.push(v);
    }
    serde_json::json!({
        "stones": stones,
        "offerings": garden.snapshot(),
        "jobs": jobs.list(),
    })
}
