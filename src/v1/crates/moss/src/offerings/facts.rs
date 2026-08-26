// Contributors + generations wire into boot and compile in O2 finish
// (facts census step + OfferingService.compile). Grammar pinned by tests.
#![allow(dead_code)]

//! The facts domain (OFFERINGS.md §6): contributors fire per-concern at
//! boot and report into an immutable generation; nobody probes directly —
//! readers consume the cheap in-memory sheet.
//!
//! Grammar law (§6.2): nouns first, canonical base units, generated unit
//! aliases. This module stores canonical keys (`ram.total.bytes`); the rule
//! evaluator converts human-unit operands at comparison time.

use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::watch;

/// A fact value: numbers live as u64/i64/f64/string/bool/list.
pub type FactValue = serde_json::Value;

/// One immutable snapshot of the stone's truth.
#[derive(Debug, Clone, Serialize)]
pub struct Generation {
    pub id: u64,
    pub collected_at: chrono::DateTime<chrono::Utc>,
    /// Canonical fact paths → values (e.g. "ram.total.bytes" → 8589934592).
    pub facts: BTreeMap<String, FactValue>,
}

impl Generation {
    /// Resolve a path with a human-unit operand suffix (.kb/.mb/.gb on
    /// byte-quantities) to the canonical bytes comparison space.
    pub fn resolve(&self, path: &str) -> Option<&FactValue> {
        self.facts.get(path)
    }
}

/// A concern's probe: owns exactly its nodes (one node, one writer).
#[async_trait::async_trait]
pub trait Contributor: Send + Sync {
    fn concern(&self) -> &'static str;
    async fn measure(&self) -> BTreeMap<String, FactValue>;
}

/// The cheap in-memory domain. Readers hold generations; refresh swaps in
/// the next one atomically (L18: change-driven, no polling of probes).
pub struct Factsheet {
    tx: watch::Sender<Arc<Generation>>,
    rx: watch::Receiver<Arc<Generation>>,
    next_id: parking_lot::Mutex<u64>,
}

impl Factsheet {
    pub fn empty() -> Self {
        let (tx, rx) = watch::channel(Arc::new(Generation {
            id: 0,
            collected_at: chrono::Utc::now(),
            facts: BTreeMap::new(),
        }));
        Self { tx, rx, next_id: parking_lot::Mutex::new(0) }
    }

    /// Run all contributors in parallel and publish the next generation.
    pub async fn collect(&self, contributors: &[Arc<dyn Contributor>]) -> Arc<Generation> {
        let mut facts = BTreeMap::new();
        let mut handles = Vec::with_capacity(contributors.len());
        for c in contributors {
            handles.push(tokio::spawn({
                let c = Arc::clone(c);
                async move { (c.concern(), c.measure().await) }
            }));
        }
        for handle in handles {
            match handle.await {
                Ok((_concern, nodes)) => {
                    for (k, v) in nodes {
                        facts.insert(k, v);
                    }
                }
                Err(e) => tracing::warn!(error = %e, "contributor task failed"),
            }
        }
        let mut next = self.next_id.lock();
        *next += 1;
        let generation = Arc::new(Generation { id: *next, collected_at: chrono::Utc::now(), facts });
        self.tx.send_replace(Arc::clone(&generation));
        generation
    }

    /// The current generation (cheap clone of an Arc).
    pub fn snapshot(&self) -> Arc<Generation> {
        Arc::clone(&self.rx.borrow())
    }
}

// ---------------------------------------------------------------------------
// Built-in contributors (OFFERINGS.md §6.2 grammar; canonical units only)
// ---------------------------------------------------------------------------

struct SimpleContributor {
    concern: &'static str,
    nodes: BTreeMap<String, FactValue>,
}

#[async_trait::async_trait]
impl Contributor for SimpleContributor {
    fn concern(&self) -> &'static str {
        self.concern
    }

    async fn measure(&self) -> BTreeMap<String, FactValue> {
        self.nodes.clone()
    }
}

/// The boot census: machine/os/cpu/ram/disk/worlds. GPU + sysctl land with
/// their own contributors later (DEBT D17) — absent facts stay unknown.
pub fn builtin_contributors(worlds_present: &[String]) -> Vec<Arc<dyn Contributor>> {
    use sysinfo::System;

    let mut out: Vec<Arc<dyn Contributor>> = Vec::new();

    // machine + os
    let mut m = BTreeMap::new();
    m.insert("machine.architecture".into(), serde_json::json!(std::env::consts::ARCH));
    m.insert("os.family".into(), serde_json::json!(std::env::consts::OS));
    out.push(Arc::new(SimpleContributor { concern: "machine", nodes: m }));

    // cpu + ram
    let mut sys = System::new();
    sys.refresh_cpu_all();
    sys.refresh_memory();
    std::thread::sleep(std::time::Duration::from_millis(200)); // cpu usage needs a delta; brand/cores do not
    sys.refresh_cpu_all();

    let mut c = BTreeMap::new();
    c.insert("cpu.cores".into(), serde_json::json!(sys.cpus().len() as u64));
    if let Some(first) = sys.cpus().first() {
        let brand = first.brand().trim();
        if !brand.is_empty() {
            c.insert("cpu.model".into(), serde_json::json!(brand));
        }
    }
    // Linux cpu flags from /proc/cpuinfo (Windows leaves them unknown).
    #[cfg(target_os = "linux")]
    if let Ok(info) = std::fs::read_to_string("/proc/cpuinfo") {
        if let Some(line) = info.lines().find(|l| l.starts_with("flags")) {
            let flags: Vec<serde_json::Value> = line
                .split(':')
                .nth(1)
                .unwrap_or("")
                .split_whitespace()
                .map(serde_json::Value::from)
                .collect();
            c.insert("cpu.features".into(), serde_json::Value::Array(flags));
        }
    }
    out.push(Arc::new(SimpleContributor { concern: "cpu", nodes: c }));

    let mut r = BTreeMap::new();
    r.insert("ram.total.bytes".into(), serde_json::json!(sys.total_memory()));
    r.insert(
        "ram.available.bytes".into(),
        serde_json::json!(sys.available_memory()),
    );
    out.push(Arc::new(SimpleContributor { concern: "ram", nodes: r }));

    // disks: aggregate free/total; kind of the disk with most free space.
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mut free_min: Option<u64> = None;
    let mut total_max: Option<u64> = None;
    let mut best_kind = "unknown";
    for d in disks.list() {
        let avail = d.available_space();
        free_min = Some(free_min.map_or(avail, |f: u64| f.min(avail)));
        total_max = Some(total_max.map_or(d.total_space(), |t: u64| t.max(d.total_space())));
        if matches!(d.kind(), sysinfo::DiskKind::SSD) && free_min == Some(avail) {
            best_kind = "ssd";
        } else if matches!(d.kind(), sysinfo::DiskKind::HDD) && free_min == Some(avail) {
            best_kind = "hdd";
        }
    }
    let mut dk = BTreeMap::new();
    if let Some(f) = free_min {
        dk.insert("disk.space.free.bytes".into(), serde_json::json!(f));
    }
    if let Some(t) = total_max {
        dk.insert("disk.space.total.bytes".into(), serde_json::json!(t));
    }
    dk.insert("disk.kind".into(), serde_json::json!(best_kind));
    out.push(Arc::new(SimpleContributor { concern: "disk", nodes: dk }));

    // worlds: which runtimes answered at boot (L25 adoption feeds facts).
    let mut w = BTreeMap::new();
    for kind in worlds_present {
        w.insert(format!("runtime.{kind}.present"), serde_json::json!(true));
    }
    out.push(Arc::new(SimpleContributor { concern: "worlds", nodes: w }));

    out
}
