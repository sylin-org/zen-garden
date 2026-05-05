//! Integration tests for `StoneApi.garden().storage()`.
//!
//! Pavilion's Cloud Filter provider depends on the wire contract of
//! `/api/v1/garden/storage/...` — these tests pin that contract by
//! standing up a real axum fixture, hitting it through the typed
//! client, and asserting on the parsed responses. No mocks: real
//! axum, real reqwest, real JSON.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::get;
use garden_common::client::StoneApi;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::Mutex;

// ────────────────────────────────────────────────────────────────────────────
// Fixture
// ────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct FixtureState {
    /// File contents keyed by `"{storage}/{path}"`.
    files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    /// History of DELETE invocations as `(storage, path)`.
    deletes: Arc<Mutex<Vec<(String, String)>>>,
}

#[derive(Deserialize)]
struct FsListQuery {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    depth: Option<String>,
}

async fn list_storages_handler() -> Json<serde_json::Value> {
    Json(json!({
        "data": [
            { "name": "storage", "replica_count": 1, "primary_stone": "stone-alpha", "roles": ["seed-bank"] },
            { "name": "personal", "replica_count": 2, "primary_stone": null, "roles": [] }
        ]
    }))
}

async fn list_fs_handler(
    Path(name): Path<String>,
    Query(q): Query<FsListQuery>,
) -> Json<serde_json::Value> {
    let path = q.path.unwrap_or_default();
    let entries = match (name.as_str(), path.as_str()) {
        ("storage", "") => json!([
            { "name": "photos", "type": "dir" },
            { "name": "readme.txt", "type": "file", "size": 54, "modified": "2026-01-01T00:00:00Z" }
        ]),
        ("storage", "photos") => json!([
            { "name": "vacation.jpg", "type": "file", "size": 1_500_000_u64 }
        ]),
        _ => json!([]),
    };

    let response_path = if path.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", path)
    };

    Json(json!({
        "data": {
            "path": response_path,
            "entries": entries,
            "truncated": false
        }
    }))
}

async fn get_or_delete_file_handler(
    State(state): State<FixtureState>,
    Path((name, path)): Path<(String, String)>,
    method: axum::http::Method,
    headers: HeaderMap,
) -> Response {
    let key = format!("{}/{}", name, path);

    if method == axum::http::Method::DELETE {
        state.deletes.lock().await.push((name, path));
        return StatusCode::NO_CONTENT.into_response();
    }

    let bytes = match state.files.lock().await.get(&key) {
        Some(b) => b.clone(),
        None => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };

    if let Some(range) = parse_range(&headers, bytes.len()) {
        let slice = bytes[range.0..=range.1].to_vec();
        let mut resp = (StatusCode::PARTIAL_CONTENT, slice).into_response();
        resp.headers_mut().insert(
            header::CONTENT_RANGE,
            HeaderValue::from_str(&format!("bytes {}-{}/{}", range.0, range.1, bytes.len()))
                .unwrap(),
        );
        return resp;
    }

    bytes.into_response()
}

/// Parse a `Range: bytes=start-end` header into an inclusive `(start, end)`
/// byte index pair, clamped to the resource length.
fn parse_range(headers: &HeaderMap, total_len: usize) -> Option<(usize, usize)> {
    let raw = headers.get(header::RANGE)?.to_str().ok()?;
    let rest = raw.strip_prefix("bytes=")?;
    let mut parts = rest.splitn(2, '-');
    let start: usize = parts.next()?.parse().ok()?;
    let end_str = parts.next()?;
    let end = if end_str.is_empty() {
        total_len.saturating_sub(1)
    } else {
        end_str.parse::<usize>().ok()?.min(total_len.saturating_sub(1))
    };
    Some((start, end))
}

async fn spawn_fixture() -> (StoneApi, FixtureState) {
    let state = FixtureState::default();
    state.files.lock().await.insert(
        "storage/readme.txt".to_string(),
        b"Hello, garden! This file lives in the storage replica.".to_vec(),
    );

    let app = Router::new()
        .route("/api/v1/garden/storage", get(list_storages_handler))
        .route("/api/v1/garden/storage/{name}/fs", get(list_fs_handler))
        .route(
            "/api/v1/garden/storage/{name}/fs/{*path}",
            get(get_or_delete_file_handler).delete(get_or_delete_file_handler),
        )
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();
    let api = StoneApi::new(client, format!("http://{}", addr));
    (api, state)
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_returns_summary_vec() {
    let (api, _) = spawn_fixture().await;
    let storages = api.garden().storage().list().await.expect("list");

    assert_eq!(storages.len(), 2);
    assert_eq!(storages[0].name, "storage");
    assert_eq!(storages[0].replica_count, 1);
    assert_eq!(storages[0].primary_stone.as_deref(), Some("stone-alpha"));
    assert_eq!(storages[0].roles, vec!["seed-bank".to_string()]);
    assert_eq!(storages[1].name, "personal");
    assert_eq!(storages[1].replica_count, 2);
    assert!(storages[1].primary_stone.is_none());
    assert!(storages[1].roles.is_empty());
}

#[tokio::test]
async fn list_directory_root() {
    let (api, _) = spawn_fixture().await;
    let listing = api
        .garden()
        .storage()
        .list_directory("storage", "", Some(1))
        .await
        .expect("list_directory root");

    assert_eq!(listing.path, "/");
    assert!(!listing.truncated);
    assert_eq!(listing.entries.len(), 2);

    let by_name: HashMap<&str, &garden_common::storage::DirectoryEntry> = listing
        .entries
        .iter()
        .map(|e| (e.name.as_str(), e))
        .collect();

    assert!(by_name["photos"].is_dir());
    assert!(!by_name["readme.txt"].is_dir());
    assert_eq!(by_name["readme.txt"].size, Some(54));
    assert_eq!(
        by_name["readme.txt"].modified.as_deref(),
        Some("2026-01-01T00:00:00Z")
    );
}

#[tokio::test]
async fn list_directory_subpath_default_depth() {
    let (api, _) = spawn_fixture().await;
    let listing = api
        .garden()
        .storage()
        .list_directory("storage", "photos", None)
        .await
        .expect("list_directory photos");

    assert_eq!(listing.path, "/photos");
    assert_eq!(listing.entries.len(), 1);
    assert_eq!(listing.entries[0].name, "vacation.jpg");
    assert_eq!(listing.entries[0].size, Some(1_500_000));
    assert!(!listing.entries[0].is_dir());
}

#[tokio::test]
async fn read_file_full_open_ended_range() {
    let (api, _) = spawn_fixture().await;
    // length=0 → open-ended `bytes=0-` request, which the fixture returns
    // as PARTIAL_CONTENT covering the whole file.
    let bytes = api
        .garden()
        .storage()
        .read_file_range("storage", "readme.txt", 0, 0)
        .await
        .expect("read_file_range full");

    let text = String::from_utf8(bytes).expect("utf-8");
    assert!(text.starts_with("Hello, garden!"));
    assert!(text.contains("storage replica"));
}

#[tokio::test]
async fn read_file_explicit_range() {
    let (api, _) = spawn_fixture().await;
    // "Hello, garden!"
    //  0123456789012
    //         ^
    //         start = 7, length = 6 → "garden"
    let bytes = api
        .garden()
        .storage()
        .read_file_range("storage", "readme.txt", 7, 6)
        .await
        .expect("read_file_range partial");

    assert_eq!(bytes, b"garden");
}

#[tokio::test]
async fn delete_file_invokes_endpoint() {
    let (api, state) = spawn_fixture().await;
    api.garden()
        .storage()
        .delete_file("storage", "readme.txt")
        .await
        .expect("delete_file");

    let calls = state.deletes.lock().await;
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0],
        ("storage".to_string(), "readme.txt".to_string())
    );
}

#[tokio::test]
async fn delete_file_with_nested_path() {
    let (api, state) = spawn_fixture().await;
    api.garden()
        .storage()
        .delete_file("storage", "photos/vacation.jpg")
        .await
        .expect("delete_file nested");

    let calls = state.deletes.lock().await;
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0],
        ("storage".to_string(), "photos/vacation.jpg".to_string())
    );
}

#[tokio::test]
async fn read_file_missing_returns_not_found_error() {
    let (api, _) = spawn_fixture().await;
    let err = api
        .garden()
        .storage()
        .read_file_range("storage", "missing.txt", 0, 10)
        .await
        .expect_err("expected not-found");

    assert!(err.is_not_found(), "got non-404 error: {err:?}");
}
