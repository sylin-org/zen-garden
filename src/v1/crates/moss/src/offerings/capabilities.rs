//! The capability sweep (docs/v1/design/capability-wishes.md, W1):
//! observe what declared offerings HOLD. Read-only channels only — exec
//! and http are inspection, not operation; an offering's lifecycle stays
//! exactly where the adoption and converge laws put it. Results feed the
//! offering record (`sub_capabilities`) and ride every chirp, so the
//! room can answer wishes (`ollama[model:llama3]`).

use crate::offerings::manifest::{CapabilityDecl, HttpList};
use crate::offerings::model::Offering;
use crate::offerings::service::OfferingService;
use std::collections::{HashMap, VecDeque};
use std::time::Duration;

/// Wire cap (contract::consts) — a model store with thousands of entries
/// must not bloat every chirp.
pub const MAX_ITEMS: usize = garden_contract::consts::MAX_CAPABILITY_ITEMS;
/// One list command's budget (PoC parity: 10s).
pub const LIST_TIMEOUT_SECS: u64 = 10;

/// What one offering holds right now: capability type → items.
pub type CapabilityMap = HashMap<String, Vec<String>>;

/// Why a discovery refused. `Unsupported` means the offering declares no
/// capability types (asking is the caller's mistake); the rest mean the
/// offering answered poorly — the cache simply stays as it was.
#[derive(Debug)]
pub enum DiscoverError {
    Unsupported(String),
    Channel(String),
    Transform(String),
}

impl std::fmt::Display for DiscoverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(m) => write!(f, "{m}"),
            Self::Channel(m) => write!(f, "capability channel failed: {m}"),
            Self::Transform(m) => write!(f, "capability answer unreadable: {m}"),
        }
    }
}

/// Observe everything the offering's manifest declares, live.
pub async fn discover(
    service: &OfferingService,
    offering: &Offering,
) -> Result<CapabilityMap, DiscoverError> {
    let manifest = service.catalog.get(&offering.offering).ok_or_else(|| {
        DiscoverError::Unsupported(format!("'{}' has no catalog manifest", offering.offering))
    })?;
    if manifest.capabilities.is_empty() {
        return Err(DiscoverError::Unsupported(format!(
            "'{}' declares no capability types",
            offering.offering
        )));
    }
    let mut map = CapabilityMap::new();
    for decl in &manifest.capabilities {
        let items = read_channel(service, offering, decl).await?;
        map.insert(decl.r#type.clone(), items);
    }
    Ok(map)
}

/// One channel, one read. Exactly one channel exists per declaration
/// (validated at catalog load); the other arm is unreachable.
async fn read_channel(
    service: &OfferingService,
    offering: &Offering,
    decl: &CapabilityDecl,
) -> Result<Vec<String>, DiscoverError> {
    if let Some(argv) = &decl.list.exec {
        let hooks = service.hooks().ok_or_else(|| {
            DiscoverError::Channel("no container runtime on this stone can run it".into())
        })?;
        let out = hooks
            .exec(&container_name_of(offering), argv, Duration::from_secs(LIST_TIMEOUT_SECS))
            .await
            .map_err(DiscoverError::Channel)?;
        Ok(out
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(String::from)
            .take(MAX_ITEMS)
            .collect())
    } else if let Some(http) = &decl.list.http {
        let port = offering.location.port;
        if port == 0 {
            return Err(DiscoverError::Channel(format!(
                "'{}' has no known address yet — nothing to ask",
                offering.name
            )));
        }
        let body = http_get_json(port, &http.path).await?;
        Ok(items_from_json(&body, http).take(MAX_ITEMS).collect())
    } else {
        Err(DiscoverError::Transform(
            "declaration has neither exec nor http — the catalog load should have refused"
                .into(),
        ))
    }
}

/// The exec target: an adopted offering is bound to its remembered
/// container (L25); a managed one carries the garden's own name.
fn container_name_of(offering: &Offering) -> String {
    if let Some(adopted) = offering.adopted() {
        return adopted.container_name.clone();
    }
    crate::offerings::docker::DockerRuntime::container_name(&offering.name)
}

/// The dot path reader: items live at `item_path`; each element is
/// either a plain string or an object whose `value_path` names it.
fn items_from_json<'a>(
    body: &'a serde_json::Value,
    http: &HttpList,
) -> impl Iterator<Item = String> + 'a {
    let items = resolve_path(body, &http.item_path)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|el| match el {
                    serde_json::Value::String(s) => Some(s.clone()),
                    obj => resolve_path(obj, &http.value_path)
                        .and_then(|v| v.as_str())
                        .map(String::from),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    items.into_iter()
}

/// Dot-notation lookup ("models.nested") over JSON objects.
fn resolve_path<'a>(
    value: &'a serde_json::Value,
    path: &str,
) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

/// GET localhost only — a capability endpoint is the offering's own
/// self-description, on its own published port.
async fn http_get_json(port: u16, path: &str) -> Result<serde_json::Value, DiscoverError> {
    use hyper_util::client::legacy::connect::HttpConnector;
    use hyper_util::client::legacy::Client;
    use hyper_util::rt::TokioExecutor;
    let client: Client<HttpConnector, http_body_util::Empty<bytes::Bytes>> =
        Client::builder(TokioExecutor::new()).build_http();
    let uri: hyper::Uri = format!("http://127.0.0.1:{port}{path}")
        .parse()
        .map_err(|e| DiscoverError::Channel(format!("bad capability uri: {e}")))?;
    let request = hyper::Request::builder()
        .method(hyper::Method::GET)
        .uri(uri)
        .body(http_body_util::Empty::<bytes::Bytes>::new())
        .map_err(|e| DiscoverError::Channel(format!("build request: {e}")))?;
    let response = tokio::time::timeout(std::time::Duration::from_secs(LIST_TIMEOUT_SECS), client.request(request))
        .await
        .map_err(|_| DiscoverError::Channel(format!("exceeded its {LIST_TIMEOUT_SECS}s budget")))?
        .map_err(|e| DiscoverError::Channel(format!("{e}")))?;
    if !response.status().is_success() {
        return Err(DiscoverError::Channel(format!(
            "http {} from {path}",
            response.status().as_u16()
        )));
    }
    let bytes = http_body_util::BodyExt::collect(response.into_body())
        .await
        .map_err(|e| DiscoverError::Channel(format!("read failed: {e}")))?
        .to_bytes();
    serde_json::from_slice(&bytes)
        .map_err(|e| DiscoverError::Transform(format!("not json: {e}")))
}

/// The sweep leg: refresh every declared offering's cache. One sweep per
/// converge tick; the cache keeps its last honest answer when a channel
/// fails (a flapping endpoint must not flicker the room's wishes).
pub async fn refresh_once(service: &OfferingService) -> usize {
    let mut updated = 0;
    for offering in service.registry().snapshot() {
        let declares = service
            .catalog
            .get(&offering.offering)
            .map(|m| !m.capabilities.is_empty())
            .unwrap_or(false);
        if !declares {
            continue;
        }
        match discover(service, &offering).await {
            Ok(map) if map != offering.sub_capabilities => {
                let mut fresh = offering.clone();
                fresh.sub_capabilities = map;
                fresh.updated_at = chrono::Utc::now();
                service.registry().replace(fresh);
                updated += 1;
            }
            Ok(_) => {}
            Err(e) => {
                tracing::debug!(offering = %offering.name, error = %e, "capability refresh");
            }
        }
    }
    updated
}


/// Why the garden refuses to touch (or cannot touch) a capability.
/// Managed-only is the trust law (L25): adoption observes, it never
/// operates — and growing content is operating.
#[derive(Debug)]
pub enum GrowRefusal {
    NotFound(String),
    NotOperable(String),
    UnknownType { asked: String, declared: Vec<String> },
    NoChannel { kind: String, op: &'static str },
    NoWorld(String),
}

impl std::fmt::Display for GrowRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(m) => write!(f, "{m}"),
            Self::NotOperable(m) => write!(f, "{m}"),
            Self::UnknownType { asked, declared } => write!(
                f,
                "'{asked}' is not a capability type this offering declares — it speaks: {}",
                if declared.is_empty() { "(none)".into() } else { declared.join(", ") }
            ),
            Self::NoChannel { kind, op } => write!(
                f,
                "the manifest declares no {op} channel for '{kind}' — the garden cannot {op} it"
            ),
            Self::NoWorld(m) => write!(f, "{m}"),
        }
    }
}

fn declared_types_of(manifest: &crate::offerings::manifest::Manifest) -> Vec<String> {
    manifest.capabilities.iter().map(|c| c.r#type.clone()).collect()
}

/// Grow one capability item on a MANAGED offering (W2): validate, open a
/// journaled job, run the manifest's add command inside the container,
/// then re-observe and complete. Returns the job id immediately (202).
#[allow(clippy::too_many_arguments)]
pub fn grow(
    service: std::sync::Arc<OfferingService>,
    tracker: crate::jobs::JobTracker,
    name: &str,
    kind: &str,
    item: &str,
) -> Result<String, GrowRefusal> {
    mutate(service, tracker, name, kind, item, Mutation::Add)
}

/// Remove one capability item (W2): the same law, the remove channel.
pub fn prune(
    service: std::sync::Arc<OfferingService>,
    tracker: crate::jobs::JobTracker,
    name: &str,
    kind: &str,
    item: &str,
) -> Result<String, GrowRefusal> {
    mutate(service, tracker, name, kind, item, Mutation::Remove)
}

enum Mutation {
    Add,
    Remove,
}

#[allow(clippy::too_many_arguments)]
fn mutate(
    service: std::sync::Arc<OfferingService>,
    tracker: crate::jobs::JobTracker,
    name: &str,
    kind: &str,
    item: &str,
    op: Mutation,
) -> Result<String, GrowRefusal> {
    let op_word = match op {
        Mutation::Add => "add",
        Mutation::Remove => "remove",
    };
    let fqn = garden_glossary::fqn::canonicalize(name)
        .map_err(|e| GrowRefusal::NotFound(format!("'{name}' does not speak the name grammar: {e}")))?;
    let offering = service
        .registry()
        .get_by_name(&fqn)
        .ok_or_else(|| GrowRefusal::NotFound(format!("'{fqn}' is not planted here")))?;
    // THE TRUST LAW. Adopted work stays the host's; the garden grows
    // only what it planted.
    if offering.adopted().is_some() {
        return Err(GrowRefusal::NotOperable(format!(
            "'{fqn}' is ADOPTED — the garden observes it and never operates it; grow it there yourself, then ask again"
        )));
    }
    if offering.managed().is_none() {
        return Err(GrowRefusal::NotOperable(format!(
            "'{fqn}' is not managed work — capabilities grow on managed offerings"
        )));
    }
    let manifest = service.catalog.get(&offering.offering).ok_or_else(|| {
        GrowRefusal::NotFound(format!("'{}' has no catalog manifest", offering.offering))
    })?;
    if manifest.capabilities.is_empty() {
        return Err(GrowRefusal::UnknownType {
            asked: kind.to_string(),
            declared: Vec::new(),
        });
    }
    let decl = manifest
        .capabilities
        .iter()
        .find(|c| c.r#type == kind)
        .ok_or_else(|| GrowRefusal::UnknownType {
            asked: kind.to_string(),
            declared: declared_types_of(manifest),
        })?;
    let mutation = match op {
        Mutation::Add => decl.add.as_ref(),
        Mutation::Remove => decl.remove.as_ref(),
    }
    .ok_or_else(|| GrowRefusal::NoChannel { kind: kind.to_string(), op: op_word })?;
    let hooks = service.hooks().ok_or_else(|| {
        GrowRefusal::NoWorld("no container runtime on this stone can run it".into())
    })?;
    let container = container_name_of(&offering);
    let argv: Vec<String> =
        mutation.exec.iter().map(|a| a.replace("{{item}}", item)).collect();
    let timeout = std::time::Duration::from_secs(mutation.timeout_secs);
    let subject = format!("{fqn}/{kind}:{item}");

    let job_id = tracker.start(
        match op {
            Mutation::Add => "capability-install",
            Mutation::Remove => "capability-remove",
        },
        &subject,
    );
    let kind_owned = kind.to_string();
    let item_owned = item.to_string();
    let job_ref = job_id.clone();
    tokio::spawn(async move {
        let job_id = job_ref;
        let outcome = hooks.exec_lines(&container, &argv).await;
        let mut lines = match outcome {
            Ok(lines) => lines,
            Err(e) => {
                tracker.fail(&job_id, &e);
                tracing::warn!(offering = %offering.name, kind = %kind_owned, item = %item_owned, error = %e, "capability mutation failed");
                return;
            }
        };

        // Consume to the deadline, reporting progress as the operation
        // speaks (percent lines only — the quiet cadence, not the churn).
        let deadline = tokio::time::Instant::now() + timeout;
        let mut tail: VecDeque<String> = VecDeque::with_capacity(8);
        let mut last_report = tokio::time::Instant::now() - std::time::Duration::from_secs(1);
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            match tokio::time::timeout(remaining, futures::StreamExt::next(&mut lines)).await {
                Err(_) => {
                    tracker.fail(&job_id, &format!("exceeded its {}s budget", timeout.as_secs()));
                    return;
                }
                Ok(None) => break,
                Ok(Some(line)) => {
                    if tail.len() == 8 {
                        tail.pop_front();
                    }
                    tail.push_back(line.clone());
                    if let Some(pct) = extract_percent(&line)
                        && last_report.elapsed() >= std::time::Duration::from_secs(1)
                    {
                        tracker.progress(&job_id, format!("{item_owned}: {pct}%"));
                        last_report = tokio::time::Instant::now();
                    }
                }
            }
        }

        let refreshed = discover(&service, &offering).await;
        let capabilities = match &refreshed {
            Ok(map) => serde_json::json!(map),
            Err(e) => serde_json::json!({ "refresh_error": e.to_string() }),
        };
        tracing::info!(offering = %offering.name, kind = %kind_owned, item = %item_owned, "capability mutated");
        tracker.complete(
            &job_id,
            serde_json::json!({
                "item": item_owned,
                "output_tail": tail.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("
"),
                "capabilities": capabilities,
            }),
        );
    });
    Ok(job_id)
}

/// The first percentage in an operation's output line, if any — the
/// universal progress dialect (`pulling x: 45%`).
fn extract_percent(line: &str) -> Option<u8> {
    static PCT: std::sync::LazyLock<Option<regex::Regex>> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"([0-9]{1,3})%").ok());
    let re = PCT.as_ref()?;
    re.captures(line)?
        .get(1)?
        .as_str()
        .parse()
        .ok()
        .filter(|p| *p <= 100)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::offerings::directory::OfferingsRoot;
    use crate::offerings::facts::Factsheet;
    use crate::offerings::manifest::Catalog;
    use crate::offerings::model::{Location, ModeData, Offering, Status};
    use crate::offerings::ports::Pool;
    use crate::offerings::registry::{MemorySnapshotStore, Registry};
    use std::sync::Arc;

    const OLLAMA: &str = "kind: software
name: ollama
category: ai
description: Local LLM runtime
adopted:
  container_name_pattern: '^ollama(-.+)?$'
managed:
  world: oci
  image: ollama/ollama:latest
  ports: { default: 11434 }
capabilities:
  - type: model
    default: true
    list:
      http: { path: /api/tags, item_path: models, value_path: name }
    add:
      exec: [ollama, pull, \"{{item}}\"]
    remove:
      exec: [ollama, rm, \"{{item}}\"]
      timeout_secs: 60
";

    /// A hooks double that records argv — proves {{item}} substitution
    /// and lets the grow job complete instantly.
    struct ScriptedHooks {
        calls: Arc<parking_lot::Mutex<Vec<Vec<String>>>>,
    }

    #[async_trait::async_trait]
    impl crate::offerings::capture_run::HookRunner for ScriptedHooks {
        async fn exec(
            &self,
            _container: &str,
            argv: &[String],
            _timeout: std::time::Duration,
        ) -> Result<String, String> {
            self.calls.lock().push(argv.to_vec());
            Ok("pulled \"test\"
success".into())
        }

        async fn exec_lines(
            &self,
            _container: &str,
            argv: &[String],
        ) -> Result<crate::offerings::capture_run::ExecLines, String> {
            self.calls.lock().push(argv.to_vec());
            let item = argv.last().cloned().unwrap_or_default();
            let lines: Vec<String> = vec![
                "pulling manifest".into(),
                format!("pulling {item}: 45%"),
                format!("pulling {item}: 100%"),
                "success".into(),
            ];
            Ok(Box::pin(futures::stream::iter(lines)))
        }
    }

    type Calls = Arc<parking_lot::Mutex<Vec<Vec<String>>>>;

    fn rig(mode: ModeData, name: &str) -> (Arc<OfferingService>, crate::jobs::JobTracker, Calls) {
        let catalog = Catalog::embedded([("ollama", OLLAMA)]).unwrap();
        let registry = Arc::new(Registry::new(Arc::new(MemorySnapshotStore::default())));
        let now = chrono::Utc::now();
        let offering = Offering {
            offering_id: "test-1".into(),
            name: name.into(),
            offering: "ollama".into(),
            category: "ai".into(),
            status: Status::Running,
            location: Location { host: "localhost".into(), port: 11434, protocol: "http".into() },
            sub_capabilities: Default::default(),
            mode_data: mode,
            registered_at: now,
            updated_at: now,
        };
        registry.register(offering.clone());
        // Fresh adopted registers land as candidates (ghost law); the
        // detector's confirm promotes — the rig replays exactly that.
        if offering.adopted().is_some() {
            registry.promote("test-1", Status::Running);
        }
        let calls = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let service = Arc::new(OfferingService::new(
            registry,
            Arc::new(crate::offerings::runtime::RuntimeRegistry::build(vec![])),
            "null".into(),
            Arc::new(catalog),
            Arc::new(Factsheet::empty()),
            OfferingsRoot::new(std::env::temp_dir().join(format!("zg-cap-{}", uuid::Uuid::now_v7()))),
            Pool::default(),
            Some(Arc::new(ScriptedHooks { calls: Arc::clone(&calls) })),
        ));
        (service, crate::jobs::JobTracker::new(), calls)
    }

    fn managed() -> ModeData {
        ModeData::Managed(crate::offerings::model::ManagedData {
            runtime_kind: "oci".into(),
            spec: Default::default(),
            port_map: Default::default(),
            plan: None,
        })
    }

    /// THE TRUST LAW, tested: adopted work is observed, never operated.
    #[tokio::test]
    async fn adopted_work_is_never_operated() {
        let (service, tracker, _calls) = rig(
            ModeData::Adopted(crate::offerings::model::AdoptedData {
                control_level: "monitor".into(),
                start_command: None,
                stop_command: None,
                health_path: None,
                container_name: "ollama".into(),
            }),
            "ollama::adopted",
        );
        let err = grow(Arc::clone(&service), tracker, "ollama::adopted", "model", "llama3")
            .err()
            .unwrap();
        assert!(matches!(err, GrowRefusal::NotOperable(_)), "{err}");
        assert!(err.to_string().contains("never operates"), "{err}");
    }

    /// An unknown type teaches the declared ones (R3.3 + F3).
    #[tokio::test]
    async fn unknown_types_teach_what_the_offering_declares() {
        let (service, tracker, _calls) = rig(managed(), "ollama::default");
        let err = grow(Arc::clone(&service), tracker, "ollama", "plugin", "pdf")
            .err()
            .unwrap();
        match err {
            GrowRefusal::UnknownType { asked, declared } => {
                assert_eq!(asked, "plugin");
                assert_eq!(declared, vec!["model".to_string()]);
            }
            other => panic!("wrong refusal: {other:?}"),
        }
    }

    /// The happy path: the add command runs with {{item}} substituted,
    /// the job completes, and the record's cache is refreshed.
    #[tokio::test]
    async fn growth_runs_the_add_command_and_refreshes_the_cache() {
        let (service, tracker, calls) = rig(managed(), "ollama::default");
        // A list channel that cannot answer (no real container) — the
        // refresh then records an honest refresh_error, job still done.
        let job_id = grow(Arc::clone(&service), tracker.clone(), "ollama", "model", "llama3")
            .unwrap();
        for _ in 0..500 {
            if let crate::jobs::JobStatus::Done | crate::jobs::JobStatus::Failed =
                job_status(&tracker, &job_id)
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let argv = calls.lock()[0].clone();
        assert_eq!(argv, vec!["ollama".to_string(), "pull".to_string(), "llama3".to_string()]);
        let job = tracker.get(&job_id).unwrap();
        assert_eq!(job.status, crate::jobs::JobStatus::Done, "{:?}", job.error);
    }

    fn job_status(tracker: &crate::jobs::JobTracker, id: &str) -> crate::jobs::JobStatus {
        tracker.get(id).map(|j| j.status).unwrap()
    }

    /// Progress speaks while the operation runs: the last percent line
    /// lands on the job, throttled and honest.
    #[tokio::test]
    async fn growth_reports_progress_while_running() {
        let (service, tracker, _calls) = rig(managed(), "ollama::default");
        let job_id = grow(Arc::clone(&service), tracker.clone(), "ollama", "model", "llama3")
            .unwrap();
        for _ in 0..500 {
            if let Some(j) = tracker.get(&job_id)
                && j.status != crate::jobs::JobStatus::Running
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let job = tracker.get(&job_id).unwrap();
        assert_eq!(job.status, crate::jobs::JobStatus::Done);
        // The 1s throttle: the first percent line is the one that spoke.
        assert_eq!(job.progress.as_deref(), Some("llama3: 45%"));
    }


    /// The dot-path reader: plain strings, object fields, nested paths.
    #[test]
    fn json_paths_read_the_shapes_offering_apis_actually_speak() {
        let ollama: serde_json::Value = serde_json::json!({
            "models": [
                {"name": "llama3:latest", "size": 4661224676i64},
                {"name": "all-minilm:latest", "size": 45853513}
            ]
        });
        let http = HttpList {
            path: "/api/tags".into(),
            item_path: "models".into(),
            value_path: "name".into(),
        };
        let items: Vec<String> = items_from_json(&ollama, &http).collect();
        assert_eq!(items, vec!["llama3:latest", "all-minilm:latest"]);

        // String arrays need no value path.
        let plain: serde_json::Value =
            serde_json::json!({"items": ["a", "b"]});
        let http2 = HttpList {
            path: "/x".into(),
            item_path: "items".into(),
            value_path: "name".into(),
        };
        assert_eq!(items_from_json(&plain, &http2).collect::<Vec<_>>(), ["a", "b"]);

        // Missing paths answer empty — never invent.
        let http3 = HttpList {
            path: "/x".into(),
            item_path: "nope".into(),
            value_path: "name".into(),
        };
        assert_eq!(items_from_json(&ollama, &http3).count(), 0);
    }
}
