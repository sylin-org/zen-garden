//! Cloud Filter contract tests — real Moss router, real `StoneApi`.
//!
//! Mounts the canonical [`garden_moss::testing::build_test_router`] on
//! a real TCP listener and exercises every method that Pavilion's
//! Cloud Filter provider relies on. This is the "what does Moss
//! actually serve" tier of the test pyramid: any drift between
//! Moss's handlers and `StoneApi`'s deserializers fails immediately
//! here, with no fixture in the way to mask the disagreement.
//!
//! Empty-state paths (no storage configured) are the focus — that's
//! the shape Pavilion sees on first launch before any stone has
//! claimed a Primary. Populated-storage tests live in
//! `garden_storage_live.rs` (gated on `ZG_TEST_STONE`).

use garden_common::client::{StoneApi, StoneApiError};
use reqwest::StatusCode;

/// Spin up Moss's test router on a random TCP port and return a
/// `StoneApi` pointed at it. The router task runs detached for the
/// lifetime of the test process; tokio cleans it up at exit.
async fn spawn_moss() -> StoneApi {
    let app = garden_moss::testing::build_test_router().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    StoneApi::new(client, format!("http://{}", addr))
}

// ────────────────────────────────────────────────────────────────────────────
// list() — empty Moss returns an empty array, not 404 or null
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn garden_list_returns_empty_array_for_fresh_moss() {
    let api = spawn_moss().await;
    let storages = api
        .garden()
        .storage()
        .list()
        .await
        .expect("list should not fail on empty Moss");
    assert!(
        storages.is_empty(),
        "fresh Moss should have zero storages, got {:?}",
        storages
    );
}

#[tokio::test]
async fn garden_list_response_is_apiresponse_envelope() {
    // Verify the wire envelope by hitting the raw HTTP layer once —
    // catches any ApiResponse<T> drift that StoneApi would silently
    // unwrap. We use the underlying reqwest client rather than the
    // typed method to inspect the whole envelope.
    let api = spawn_moss().await;
    let url = format!("{}/api/v1/garden/storage", api.endpoint());
    let raw = api
        .http()
        .get(&url)
        .send()
        .await
        .expect("raw GET")
        .json::<serde_json::Value>()
        .await
        .expect("parse JSON");

    assert!(
        raw.get("data").is_some(),
        "expected ApiResponse `data` field, got: {raw}"
    );
    assert!(
        raw["data"].is_array(),
        "expected `data` to be an array, got: {}",
        raw["data"]
    );
}

// ────────────────────────────────────────────────────────────────────────────
// list_directory() on an unknown storage — 503 NO_STORAGE with structured body
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn garden_list_directory_unknown_storage_returns_503_no_storage() {
    let api = spawn_moss().await;
    let err = api
        .garden()
        .storage()
        .list_directory("does-not-exist", "", None)
        .await
        .expect_err("missing storage should fail");

    match err {
        StoneApiError::Http {
            status,
            code,
            message,
        } => {
            assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
            assert_eq!(code, "NO_STORAGE");
            assert!(
                !message.is_empty(),
                "NO_STORAGE error should carry a non-empty message"
            );
        }
        other => panic!("expected StoneApiError::Http, got {other:?}"),
    }
}

#[tokio::test]
async fn garden_list_directory_unknown_storage_with_subpath_still_503() {
    let api = spawn_moss().await;
    let err = api
        .garden()
        .storage()
        .list_directory("does-not-exist", "photos", Some(2))
        .await
        .expect_err("missing storage should fail");
    assert!(
        matches!(
            err,
            StoneApiError::Http {
                status: StatusCode::SERVICE_UNAVAILABLE,
                ..
            }
        ),
        "expected 503, got {err:?}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// read_file_range() on unknown storage — 503 NO_STORAGE before any byte read
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn garden_read_file_range_unknown_storage_returns_503() {
    let api = spawn_moss().await;
    let err = api
        .garden()
        .storage()
        .read_file_range("does-not-exist", "readme.txt", 0, 100)
        .await
        .expect_err("missing storage should fail");
    assert!(
        matches!(
            err,
            StoneApiError::Http {
                status: StatusCode::SERVICE_UNAVAILABLE,
                ..
            }
        ),
        "expected 503, got {err:?}"
    );
}

#[tokio::test]
async fn garden_read_file_range_full_file_request_unknown_storage_returns_503() {
    // `length = 0` triggers the open-ended `bytes={start}-` range
    // request. The 503 short-circuits before the server even reads
    // the Range header, so the behavior is identical to a sized read.
    let api = spawn_moss().await;
    let err = api
        .garden()
        .storage()
        .read_file_range("does-not-exist", "readme.txt", 0, 0)
        .await
        .expect_err("missing storage should fail");
    assert!(
        matches!(
            err,
            StoneApiError::Http {
                status: StatusCode::SERVICE_UNAVAILABLE,
                ..
            }
        ),
        "expected 503, got {err:?}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// delete_file() on unknown storage — same 503 shape as the read path
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn garden_delete_file_unknown_storage_returns_503() {
    let api = spawn_moss().await;
    let err = api
        .garden()
        .storage()
        .delete_file("does-not-exist", "readme.txt")
        .await
        .expect_err("missing storage should fail");
    assert!(
        matches!(
            err,
            StoneApiError::Http {
                status: StatusCode::SERVICE_UNAVAILABLE,
                ..
            }
        ),
        "expected 503, got {err:?}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// Stone identity middleware — every garden_storage response must carry
// X-Stone-Id and X-Stone-Name. Pavilion can use these to detect when
// it's been redirected through a proxy or when the tended endpoint
// has changed underneath it.
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn garden_responses_carry_stone_identity_headers() {
    let api = spawn_moss().await;
    let url = format!("{}/api/v1/garden/storage", api.endpoint());
    let response = api.http().get(&url).send().await.expect("GET");

    let stone_id = response
        .headers()
        .get("x-stone-id")
        .expect("X-Stone-Id present")
        .to_str()
        .unwrap()
        .to_string();
    let stone_name = response
        .headers()
        .get("x-stone-name")
        .expect("X-Stone-Name present")
        .to_str()
        .unwrap()
        .to_string();

    assert_eq!(stone_id, "test-stone-id");
    assert_eq!(stone_name, "stone-test");
}

// ────────────────────────────────────────────────────────────────────────────
// Path encoding — storage names and file paths with special characters
// must round-trip through the request URL without losing structure or
// double-encoding slashes.
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn garden_storage_with_spaces_in_name_routes_cleanly() {
    let api = spawn_moss().await;
    // Should still hit the no-storage path (no spaces-named storage
    // exists), but the URL must be percent-encoded on the way in
    // and decoded by the handler — so we get 503, not 400 or 404.
    let err = api
        .garden()
        .storage()
        .list_directory("my storage", "", None)
        .await
        .expect_err("unknown storage should fail");
    assert!(
        matches!(
            err,
            StoneApiError::Http {
                status: StatusCode::SERVICE_UNAVAILABLE,
                ..
            }
        ),
        "expected 503 for unknown space-named storage, got {err:?}"
    );
}

#[tokio::test]
async fn garden_nested_path_with_special_chars_does_not_break_routing() {
    let api = spawn_moss().await;
    let err = api
        .garden()
        .storage()
        .read_file_range("does-not-exist", "photos/Summer 2026/IMG#01.jpg", 0, 10)
        .await
        .expect_err("unknown storage should fail");
    assert!(
        matches!(
            err,
            StoneApiError::Http {
                status: StatusCode::SERVICE_UNAVAILABLE,
                ..
            }
        ),
        "expected 503 with space- and hash-laden subpath, got {err:?}"
    );
}
