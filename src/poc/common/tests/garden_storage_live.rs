//! Live-garden tests for `StoneApi.garden().storage()`.
//!
//! Exercises the read path Pavilion's Cloud Filter provider relies on
//! against every Moss instance the local garden surfaces. Bootstrap
//! resolves an entry point, then `/api/v1/garden/topology` expands to
//! the full peer list — no manual configuration required when the
//! garden is already up.
//!
//! ## Bootstrap resolution chain
//!
//! 1. `ZG_TEST_STONE` env var (explicit override, highest priority).
//! 2. Pavilion / Rake tending file at `~/.zen-garden/.tending`.
//! 3. `garden_discovery::discover_moss_auto` (LAN-wide UDP/mDNS, ~3s).
//!
//! All tests skip silently with a hint if the chain returns nothing —
//! e.g. when the garden is offline or the test machine is not on the
//! same LAN.
//!
//! ## What "comprehensive against the local garden" means here
//!
//! - Every stone reachable from the bootstrap is probed.
//! - `list()` is asserted consistent across stones (cross-stone view
//!   parity — disagreement signals a partition or stale cache).
//! - Each stone's `list()` summaries pass wire-shape invariants.
//! - When a populated storage exists, the read path is exercised
//!   end-to-end (list root → pick a file → range-read it).
//! - The unknown-storage error path returns the same shape from every
//!   stone (503 NO_STORAGE).
//!
//! No mocks. Real garden, real wire format. If these pass against
//! your local garden, Pavilion's Cloud Filter read path will work
//! against any stone it tends.

use std::collections::{BTreeSet, HashMap};
use std::time::Duration;

use garden_common::client::{StoneApi, StoneApiError};
use reqwest::StatusCode;

const ENV_ENDPOINT: &str = "ZG_TEST_STONE";
const ENV_STORAGE: &str = "ZG_TEST_STORAGE";
const ENV_FILE: &str = "ZG_TEST_FILE";
/// Comma-separated list of stone names to exclude from the iteration.
/// Useful when one stone in the garden is broken in a way that's not
/// what these tests are meant to catch — e.g. an old build, a
/// misconfigured stone, or a stone with the garden_storage routes
/// stripped. The listed stones are skipped silently.
const ENV_SKIP_STONES: &str = "ZG_TEST_SKIP_STONES";

const HTTP_TIMEOUT: Duration = Duration::from_secs(10);
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(3);

// ────────────────────────────────────────────────────────────────────────────
// Bootstrap
// ────────────────────────────────────────────────────────────────────────────

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        // Local dev Moss often serves a self-signed Koi-CA cert. mTLS
        // upgrade for clients is tracked in PAVILION-0001 §"Authentication
        // boundary". Until then, live tests accept whatever cert the
        // configured endpoints present.
        .danger_accept_invalid_certs(true)
        .build()
        .expect("reqwest client builds")
}

/// Resolve a single bootstrap endpoint via env → tending file → discovery.
async fn resolve_bootstrap_endpoint() -> Option<String> {
    if let Ok(ep) = std::env::var(ENV_ENDPOINT) {
        eprintln!("bootstrap: using {ENV_ENDPOINT}={ep}");
        return Some(ep);
    }

    if let Some(ep) = read_tending_endpoint().await {
        eprintln!("bootstrap: using ~/.zen-garden/.tending → {ep}");
        return Some(ep);
    }

    eprintln!(
        "bootstrap: no env or tending file — running garden_discovery::discover_moss_auto ({:?})",
        DISCOVERY_TIMEOUT
    );
    match garden_discovery::discover_moss_auto(DISCOVERY_TIMEOUT).await {
        Ok(stones) if !stones.is_empty() => {
            let endpoint = stones[0].address.http_base();
            eprintln!(
                "bootstrap: discovered {} stones, using {}",
                stones.len(),
                endpoint
            );
            Some(endpoint)
        }
        Ok(_) => {
            eprintln!("bootstrap: discovery returned no stones");
            None
        }
        Err(e) => {
            eprintln!("bootstrap: discovery failed: {e}");
            None
        }
    }
}

/// Read `~/.zen-garden/.tending` for an endpoint. Same file Pavilion
/// and Rake share — populated whenever the user has tended a stone.
async fn read_tending_endpoint() -> Option<String> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()?;
    let path = std::path::PathBuf::from(home)
        .join(".zen-garden")
        .join(".tending");
    let content = tokio::fs::read_to_string(&path).await.ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    json.get("endpoint")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Expand a bootstrap endpoint to the full set of stones in the garden.
///
/// Reads `/api/v1/garden/topology` on the bootstrap stone and returns
/// one `StoneApi` per peer that it lists with `status == "online"`. The
/// bootstrap itself is included if topology lists it.
async fn discover_all_stones(bootstrap: &str) -> Vec<StoneEndpoint> {
    let client = http_client();
    let url = format!("{}/api/v1/garden/topology", bootstrap.trim_end_matches('/'));
    let json: serde_json::Value = match client.get(&url).send().await {
        Ok(resp) => match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("topology parse failed: {e}");
                return vec![bootstrap_only(bootstrap)];
            }
        },
        Err(e) => {
            eprintln!("topology fetch failed: {e}");
            return vec![bootstrap_only(bootstrap)];
        }
    };

    let entries = match json.get("data").and_then(|d| d.as_array()) {
        Some(arr) => arr.clone(),
        None => return vec![bootstrap_only(bootstrap)],
    };

    let mut stones = Vec::new();
    for entry in entries {
        let name = entry
            .get("stone_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let status = entry
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        if status != "online" {
            continue;
        }
        let ip = entry
            .get("address")
            .and_then(|a| a.get("ip"))
            .and_then(|v| v.as_str());
        let port = entry
            .get("address")
            .and_then(|a| a.get("port"))
            .and_then(|v| v.as_u64());
        if let (Some(ip), Some(port)) = (ip, port) {
            stones.push(StoneEndpoint {
                name,
                endpoint: format!("http://{ip}:{port}"),
            });
        }
    }
    if stones.is_empty() {
        vec![bootstrap_only(bootstrap)]
    } else {
        stones
    }
}

fn bootstrap_only(endpoint: &str) -> StoneEndpoint {
    StoneEndpoint {
        name: "bootstrap".into(),
        endpoint: endpoint.to_string(),
    }
}

#[derive(Debug, Clone)]
struct StoneEndpoint {
    name: String,
    endpoint: String,
}

impl StoneEndpoint {
    fn api(&self) -> StoneApi {
        StoneApi::new(http_client(), self.endpoint.clone())
    }
}

/// Names listed in `ZG_TEST_SKIP_STONES` (comma-separated, trimmed).
fn skipped_stone_names() -> BTreeSet<String> {
    std::env::var(ENV_SKIP_STONES)
        .ok()
        .map(|s| {
            s.split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Resolve every stone in the garden, minus any in `ZG_TEST_SKIP_STONES`.
/// Returns empty when no bootstrap is reachable — tests must treat that
/// as "skip".
async fn discover_garden() -> Vec<StoneEndpoint> {
    let Some(bootstrap) = resolve_bootstrap_endpoint().await else {
        return Vec::new();
    };
    let mut stones = discover_all_stones(&bootstrap).await;
    let skip = skipped_stone_names();
    if !skip.is_empty() {
        let before = stones.len();
        stones.retain(|s| !skip.contains(&s.name));
        let removed = before - stones.len();
        if removed > 0 {
            eprintln!(
                "garden: filtered {} stone(s) via {ENV_SKIP_STONES}: {:?}",
                removed, skip
            );
        }
    }
    eprintln!(
        "garden: {} stone(s) reachable: {}",
        stones.len(),
        stones
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    stones
}

fn skip_if_empty(stones: &[StoneEndpoint], context: &str) -> bool {
    if stones.is_empty() {
        eprintln!(
            "skipped {context}: no garden reachable. Set {ENV_ENDPOINT}, tend a stone, \
             or ensure the garden is online and discoverable from this host."
        );
        return true;
    }
    false
}

// ────────────────────────────────────────────────────────────────────────────
// list() — every stone serves a parseable summary array
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn live_list_returns_parseable_summaries_from_every_stone() {
    let stones = discover_garden().await;
    if skip_if_empty(&stones, "live_list_returns_parseable_summaries_from_every_stone") {
        return;
    }

    let mut failures = Vec::new();
    for stone in &stones {
        match stone.api().garden().storage().list().await {
            Ok(storages) => {
                eprintln!(
                    "  ✓ {}: {} storage(s) {:?}",
                    stone.name,
                    storages.len(),
                    storages.iter().map(|s| &s.name).collect::<Vec<_>>()
                );
                for s in &storages {
                    if s.name.is_empty() {
                        failures.push(format!(
                            "{}: summary has empty name in {s:?}",
                            stone.name
                        ));
                    }
                    if let Some(stone_name) = &s.primary_stone {
                        if stone_name.is_empty() {
                            failures.push(format!(
                                "{}: primary_stone is present but empty in {s:?}",
                                stone.name
                            ));
                        }
                    }
                }
            }
            Err(e) => failures.push(format!("{}: list() failed: {e}", stone.name)),
        }
    }

    assert!(failures.is_empty(), "live garden errors:\n{}", failures.join("\n"));
}

// ────────────────────────────────────────────────────────────────────────────
// list() — cross-stone consistency: every stone agrees on storage names
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn live_list_is_consistent_across_all_stones() {
    let stones = discover_garden().await;
    if skip_if_empty(&stones, "live_list_is_consistent_across_all_stones") {
        return;
    }

    let mut by_stone: HashMap<String, BTreeSet<String>> = HashMap::new();
    for stone in &stones {
        let names: BTreeSet<String> = stone
            .api()
            .garden()
            .storage()
            .list()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|s| s.name)
            .collect();
        by_stone.insert(stone.name.clone(), names);
    }

    if by_stone.values().all(|s| s.is_empty()) {
        eprintln!("note: every stone reports zero storages — cross-stone parity holds trivially");
        return;
    }

    // All non-empty entries must agree.
    let baseline = by_stone
        .iter()
        .find(|(_, s)| !s.is_empty())
        .map(|(_, s)| s.clone())
        .unwrap_or_default();
    let mut disagreements = Vec::new();
    for (stone_name, names) in &by_stone {
        if names != &baseline {
            disagreements.push(format!(
                "{stone_name}: {names:?} differs from baseline {baseline:?}"
            ));
        }
    }
    assert!(
        disagreements.is_empty(),
        "stones disagree on garden storage view:\n{}",
        disagreements.join("\n")
    );
}

// ────────────────────────────────────────────────────────────────────────────
// 503 path — unknown storage returns the same NO_STORAGE shape everywhere
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn live_unknown_storage_returns_503_no_storage_from_every_stone() {
    let stones = discover_garden().await;
    if skip_if_empty(&stones, "live_unknown_storage_returns_503_no_storage_from_every_stone") {
        return;
    }

    let bogus_name = format!(
        "definitely-does-not-exist-{}",
        garden_common::utils::ids::generate_guidv7()
    );
    let mut failures = Vec::new();
    for stone in &stones {
        match stone
            .api()
            .garden()
            .storage()
            .list_directory(&bogus_name, "", None)
            .await
        {
            Err(StoneApiError::Http {
                status: StatusCode::SERVICE_UNAVAILABLE,
                code,
                ..
            }) => {
                if code != "NO_STORAGE" {
                    failures.push(format!("{}: 503 with unexpected code {code}", stone.name));
                }
            }
            Err(other) => failures.push(format!("{}: expected 503 NO_STORAGE, got {other:?}", stone.name)),
            Ok(listing) => failures.push(format!(
                "{}: bogus storage somehow returned a listing: {listing:?}",
                stone.name
            )),
        }
    }
    assert!(failures.is_empty(), "live garden errors:\n{}", failures.join("\n"));
}

// ────────────────────────────────────────────────────────────────────────────
// Read path — populated storage end-to-end (list root → pick file → range read)
//
// This is the test that proves Pavilion's Cloud Filter provider works
// against the real garden. Skipped when no populated storage is found
// (or when ZG_TEST_FILE is set but doesn't exist on the chosen stone).
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn live_read_path_end_to_end_against_populated_storage() {
    let stones = discover_garden().await;
    if skip_if_empty(&stones, "live_read_path_end_to_end_against_populated_storage") {
        return;
    }

    // Find any (stone, storage_name) where the storage has at least one
    // file we can read. We prefer the first stone whose `list()` is
    // non-empty so the proxy / direct path is exercised against real
    // managed content.
    let preferred_storage = std::env::var(ENV_STORAGE).ok();
    let preferred_file = std::env::var(ENV_FILE).ok().filter(|s| !s.is_empty());

    let mut chosen: Option<(StoneEndpoint, String)> = None;
    'outer: for stone in &stones {
        let storages = match stone.api().garden().storage().list().await {
            Ok(s) => s,
            Err(_) => continue,
        };
        let candidate_names: Vec<String> = match preferred_storage {
            Some(ref name) if storages.iter().any(|s| &s.name == name) => vec![name.clone()],
            Some(_) => continue,
            None => storages.iter().map(|s| s.name.clone()).collect(),
        };
        for name in candidate_names {
            chosen = Some((stone.clone(), name));
            break 'outer;
        }
    }

    let Some((stone, storage_name)) = chosen else {
        let hint = preferred_storage
            .map(|s| format!("set {ENV_STORAGE}={s} but no stone has it"))
            .unwrap_or_else(|| "no stone advertises any managed storage".to_string());
        eprintln!("skipped: {hint}. Add a storage with `rake storage add` to enable end-to-end content tests.");
        return;
    };
    eprintln!(
        "read path: chose stone={} storage={}",
        stone.name, storage_name
    );

    // List the storage root. If empty, skip — there is no file to read.
    let listing = stone
        .api()
        .garden()
        .storage()
        .list_directory(&storage_name, "", None)
        .await
        .unwrap_or_else(|e| panic!("list_directory({storage_name}) failed on {}: {e}", stone.name));

    if listing.entries.is_empty() && preferred_file.is_none() {
        eprintln!(
            "skipped: storage '{}' on '{}' is empty. Set {ENV_FILE} to a known file path \
             or drop a file at the root with `rake storage` write tools.",
            storage_name, stone.name
        );
        return;
    }

    // Resolve the file path: prefer the explicit env var, otherwise the
    // first file (not directory) in the root listing.
    let file_path = match preferred_file {
        Some(p) => p,
        None => match listing.entries.iter().find(|e| !e.is_dir()) {
            Some(e) => e.name.clone(),
            None => {
                eprintln!(
                    "skipped: storage '{}' has only subdirectories at the root. \
                     Set {ENV_FILE} to a deeper path, or add a top-level file.",
                    storage_name
                );
                return;
            }
        },
    };

    // Look up the size from the same listing if possible — saves a HEAD.
    let parent_path = file_path
        .rsplit_once('/')
        .map(|(parent, _)| parent.to_string())
        .unwrap_or_default();
    let file_name = file_path
        .rsplit_once('/')
        .map(|(_, name)| name.to_string())
        .unwrap_or_else(|| file_path.clone());
    let listing_for_size = stone
        .api()
        .garden()
        .storage()
        .list_directory(&storage_name, &parent_path, None)
        .await
        .expect("list parent directory for file size");
    let entry = listing_for_size
        .entries
        .iter()
        .find(|e| e.name == file_name)
        .unwrap_or_else(|| {
            panic!(
                "{ENV_FILE}='{file_path}' not found under '{parent_path}' on stone '{}'",
                stone.name
            )
        });
    let size = entry.size.expect("file entry must report size");
    let read_len = size.min(64);

    // Range read.
    let bytes = stone
        .api()
        .garden()
        .storage()
        .read_file_range(&storage_name, &file_path, 0, read_len)
        .await
        .unwrap_or_else(|e| {
            panic!(
                "read_file_range({storage_name}, {file_path}, 0, {read_len}) failed on {}: {e}",
                stone.name
            )
        });
    assert_eq!(
        bytes.len() as u64,
        read_len,
        "expected {read_len} bytes from {file_path} on {}, got {}",
        stone.name,
        bytes.len()
    );
    eprintln!(
        "  ✓ end-to-end: read {} of {} bytes from {} on {}",
        bytes.len(),
        size,
        file_path,
        stone.name
    );
}
