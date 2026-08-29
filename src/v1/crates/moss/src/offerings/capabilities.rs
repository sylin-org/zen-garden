//! The capability sweep (docs/v1/design/capability-wishes.md, W1):
//! observe what declared offerings HOLD. Read-only channels only — exec
//! and http are inspection, not operation; an offering's lifecycle stays
//! exactly where the adoption and converge laws put it. Results feed the
//! offering record (`sub_capabilities`) and ride every chirp, so the
//! room can answer wishes (`ollama[model:llama3]`).

use crate::offerings::manifest::{CapabilityDecl, HttpList};
use crate::offerings::model::Offering;
use crate::offerings::service::OfferingService;
use std::collections::HashMap;
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
    let response = client
        .request(request)
        .await
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


#[cfg(test)]
mod tests {
    use super::*;

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
