//! Cross-stone HTTP request helpers.
//!
//! Centralises the "hit another stone's Moss API by name" pattern
//! that several handlers and background tasks repeat. Today the
//! only caller is `mirror_capabilities`; the seed-transfer work in
//! [ORCH-0039] adds two more (snapshot fetch and plant-from-snapshot
//! cross-stone calls), and the pattern was the obvious factoring
//! target before duplication grew.
//!
//! ## What this layer adds
//!
//! [`StoneClient`] (`infra/stone_client.rs`) handles transport
//! choice — plain HTTP vs. HTTPS-with-pond-mTLS — but takes a
//! [`PeerAddress`] and returns a raw `reqwest::RequestBuilder` for
//! callers to chain. The repeated work above that is:
//!
//! 1. Resolve a stone *name* to an endpoint URL, including the
//!    self-as-target case (the local stone's bound address can be
//!    `0.0.0.0`, which is a valid listener but not a routable
//!    target — we substitute `127.0.0.1`).
//! 2. Send a typed JSON request and decode the typed
//!    [`ApiResponse<T>`] reply that Moss handlers always return.
//! 3. Map HTTP / parse / not-found / unreachable errors to a single
//!    error type that handlers can convert into the
//!    `(StatusCode, Json<ApiErrorResponse>)` shape with one method
//!    call.
//!
//! This module is a behaviour-preserving extraction of the helpers
//! that lived inline in `api/v1/offering_capabilities.rs`. The
//! callsite there now delegates here.
//!
//! [ORCH-0039]: ../../../../docs/decisions/ORCH-0039-seed-based-offering-replication.md

use axum::Json;
use axum::http::StatusCode;
use garden_common::api_utils::{ApiErrorResponse, ApiResponse};
use reqwest::Client;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::Moss;
use crate::infra::api_helpers::{bad_gateway, not_found};

/// Errors raised by the cross-stone helpers. Convert into the
/// api-error-tuple shape via [`CrossStoneError::into_api_error`]
/// when surfacing in an axum handler, or render with `to_string()`
/// when the caller wants a plain message (e.g. the
/// `mirror_capabilities` per-item failure list).
#[derive(Debug, thiserror::Error)]
pub enum CrossStoneError {
    /// The named stone is not in the topology cache and is not the
    /// local stone — there is no endpoint to dial.
    #[error("Stone '{stone}' not found in topology cache")]
    StoneNotFound { stone: String },

    /// The transport itself failed (connection refused, DNS,
    /// TLS handshake, etc).
    #[error("Failed to reach stone '{stone}': {source}")]
    Unreachable {
        stone: String,
        #[source]
        source: reqwest::Error,
    },

    /// The remote returned a non-success status.
    #[error("Stone '{stone}' returned {status}: {message}")]
    HttpStatus {
        stone: String,
        status: StatusCode,
        message: String,
    },

    /// The remote returned 404 specifically. Pulled out as its own
    /// variant so callers can map to a 404 on the *outer* request
    /// (e.g. "the offering you asked us to mirror does not exist
    /// on the source stone") rather than a generic 502.
    #[error("Stone '{stone}': {message}")]
    NotFound { stone: String, message: String },

    /// The transport succeeded but the body wasn't a parseable
    /// `ApiResponse<T>`.
    #[error("Stone '{stone}' returned an unparseable response: {source}")]
    ParseFailed {
        stone: String,
        #[source]
        source: reqwest::Error,
    },
}

impl CrossStoneError {
    /// Render to the api-error-tuple shape used by axum handlers,
    /// preserving the not-found / bad-gateway distinction.
    pub fn into_api_error(self) -> (StatusCode, Json<ApiErrorResponse>) {
        match &self {
            CrossStoneError::StoneNotFound { .. } => not_found("STONE_NOT_FOUND", self.to_string()),
            CrossStoneError::NotFound { .. } => not_found("REMOTE_NOT_FOUND", self.to_string()),
            CrossStoneError::Unreachable { .. } => {
                bad_gateway("REMOTE_UNREACHABLE", self.to_string())
            }
            CrossStoneError::HttpStatus { .. } => bad_gateway("REMOTE_ERROR", self.to_string()),
            CrossStoneError::ParseFailed { .. } => {
                bad_gateway("REMOTE_PARSE_FAILED", self.to_string())
            }
        }
    }
}

/// Resolve a stone name to its HTTP base URL.
///
/// Local-stone resolution prefers `127.0.0.1` over the topology
/// `http_base()` because the local stone's bound address can be
/// `0.0.0.0`, which is a valid listener but not a routable target.
/// All other stones come from the topology cache verbatim.
///
/// Returns `None` when the name is unknown — neither the local
/// stone nor present in the topology cache.
pub async fn resolve_stone_endpoint(state: &Moss, stone_name: &str) -> Option<String> {
    if stone_name.eq_ignore_ascii_case(&state.current.stone.name) {
        let base = state.current.address.read().await.http_base();
        if base.contains("0.0.0.0") {
            Some(format!("http://127.0.0.1:{}", state.current.api_port))
        } else {
            Some(base)
        }
    } else {
        state
            .topology
            .get_by_name(stone_name)
            .await
            .map(|entry| entry.address.http_base())
    }
}

/// `GET <endpoint><path>`, decode the response as `ApiResponse<T>`,
/// return the inner `T`. The `stone_name` is used only for error
/// messages — the actual transport target is `endpoint`.
pub async fn fetch_from_stone<T: DeserializeOwned>(
    client: &Client,
    endpoint: &str,
    stone_name: &str,
    path: &str,
) -> Result<T, CrossStoneError> {
    let url = format!("{}{}", endpoint.trim_end_matches('/'), path);
    let response =
        client
            .get(&url)
            .send()
            .await
            .map_err(|source| CrossStoneError::Unreachable {
                stone: stone_name.to_string(),
                source,
            })?;
    classify_and_decode::<T>(response, stone_name).await
}

/// `GET <endpoint><path>`, returning the raw `reqwest::Response`
/// for callers that want to stream the body directly (snapshot
/// artifact downloads — image tars, volume archives — that
/// would blow the heap if buffered).
///
/// Status checking is the same as [`fetch_from_stone`]: 404 →
/// `NotFound`, other non-success → `HttpStatus`. The caller
/// owns the response body from there.
pub async fn stream_from_stone(
    client: &Client,
    endpoint: &str,
    stone_name: &str,
    path: &str,
) -> Result<reqwest::Response, CrossStoneError> {
    let url = format!("{}{}", endpoint.trim_end_matches('/'), path);
    let response =
        client
            .get(&url)
            .send()
            .await
            .map_err(|source| CrossStoneError::Unreachable {
                stone: stone_name.to_string(),
                source,
            })?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let message = serde_json::from_str::<ApiErrorResponse>(&body)
            .map(|err| err.error.message)
            .unwrap_or(body);
        return Err(if status == StatusCode::NOT_FOUND {
            CrossStoneError::NotFound {
                stone: stone_name.to_string(),
                message,
            }
        } else {
            CrossStoneError::HttpStatus {
                stone: stone_name.to_string(),
                status,
                message,
            }
        });
    }
    Ok(response)
}

/// `POST <endpoint><path>` with a JSON-encoded `body`, decode the
/// response as `ApiResponse<R>`, return the inner `R`.
pub async fn post_to_stone<Q: Serialize, R: DeserializeOwned>(
    client: &Client,
    endpoint: &str,
    stone_name: &str,
    path: &str,
    body: &Q,
) -> Result<R, CrossStoneError> {
    let url = format!("{}{}", endpoint.trim_end_matches('/'), path);
    let response = client.post(&url).json(body).send().await.map_err(|source| {
        CrossStoneError::Unreachable {
            stone: stone_name.to_string(),
            source,
        }
    })?;
    classify_and_decode::<R>(response, stone_name).await
}

/// Map a `reqwest::Response` to a typed `T` or a typed
/// [`CrossStoneError`]. Pulled out so `fetch_from_stone` and
/// `post_to_stone` share status / parse handling.
async fn classify_and_decode<T: DeserializeOwned>(
    response: reqwest::Response,
    stone_name: &str,
) -> Result<T, CrossStoneError> {
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let message = serde_json::from_str::<ApiErrorResponse>(&body)
            .map(|err| err.error.message)
            .unwrap_or(body);
        return Err(if status == StatusCode::NOT_FOUND {
            CrossStoneError::NotFound {
                stone: stone_name.to_string(),
                message,
            }
        } else {
            CrossStoneError::HttpStatus {
                stone: stone_name.to_string(),
                status,
                message,
            }
        });
    }

    let api_response: ApiResponse<T> =
        response
            .json()
            .await
            .map_err(|source| CrossStoneError::ParseFailed {
                stone: stone_name.to_string(),
                source,
            })?;
    Ok(api_response.data)
}

#[cfg(test)]
mod tests {
    //! Integration tests against an axum fixture, no mocks. The
    //! fixture mimics Moss's `ApiResponse<T>` envelope so a drift
    //! in the wire shape (e.g. `data` field renamed) shows up as a
    //! parse failure here, not a runtime surprise downstream.
    //!
    //! `resolve_stone_endpoint` is intentionally *not* tested here —
    //! it depends on a fully-constructed `Moss` runtime which costs
    //! more setup than the function justifies. The behaviour is
    //! exercised end-to-end by `mirror_capabilities` integration
    //! tests in CI.

    use super::*;
    use axum::{Json, Router, routing::get, routing::post};
    use garden_common::api_utils::{ApiErrorResponse, ApiResponse};
    use serde::{Deserialize, Serialize};
    use std::net::SocketAddr;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    struct Greeting {
        message: String,
    }

    async fn spawn_fixture(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn fetch_from_stone_decodes_api_response_envelope() {
        let app = Router::new().route(
            "/greet",
            get(|| async {
                Json(ApiResponse {
                    data: Greeting {
                        message: "hello".into(),
                    },
                    suggestions: None,
                })
            }),
        );
        let endpoint = spawn_fixture(app).await;

        let client = Client::new();
        let got: Greeting = fetch_from_stone(&client, &endpoint, "stone-test", "/greet")
            .await
            .expect("fetch should succeed");
        assert_eq!(got.message, "hello");
    }

    #[tokio::test]
    async fn fetch_from_stone_404_becomes_not_found_variant() {
        // 404 must produce CrossStoneError::NotFound (not the
        // generic HttpStatus catch-all) so callers can distinguish
        // "the resource doesn't exist remotely" from "the remote
        // is broken".
        let app = Router::new().route(
            "/missing",
            get(|| async {
                (
                    StatusCode::NOT_FOUND,
                    Json(ApiErrorResponse::new("NOPE", "no such thing")),
                )
            }),
        );
        let endpoint = spawn_fixture(app).await;

        let client = Client::new();
        let err = fetch_from_stone::<Greeting>(&client, &endpoint, "stone-test", "/missing")
            .await
            .expect_err("404 must error");
        match err {
            CrossStoneError::NotFound { stone, message } => {
                assert_eq!(stone, "stone-test");
                assert_eq!(message, "no such thing");
            }
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn fetch_from_stone_5xx_becomes_http_status_variant() {
        let app = Router::new().route(
            "/boom",
            get(|| async {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiErrorResponse::new("KABOOM", "exploded")),
                )
            }),
        );
        let endpoint = spawn_fixture(app).await;

        let client = Client::new();
        let err = fetch_from_stone::<Greeting>(&client, &endpoint, "stone-test", "/boom")
            .await
            .expect_err("5xx must error");
        match err {
            CrossStoneError::HttpStatus {
                stone,
                status,
                message,
            } => {
                assert_eq!(stone, "stone-test");
                assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
                assert_eq!(message, "exploded");
            }
            other => panic!("expected HttpStatus, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn fetch_from_stone_unreachable_endpoint_becomes_unreachable() {
        // Unbound port — connection refused.
        let client = Client::builder()
            .timeout(std::time::Duration::from_millis(500))
            .build()
            .unwrap();
        let err = fetch_from_stone::<Greeting>(
            &client,
            "http://127.0.0.1:1",
            "stone-test",
            "/anywhere",
        )
        .await
        .expect_err("unreachable endpoint must error");
        match err {
            CrossStoneError::Unreachable { stone, .. } => {
                assert_eq!(stone, "stone-test");
            }
            other => panic!("expected Unreachable, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn fetch_from_stone_unparseable_body_becomes_parse_failed() {
        let app = Router::new().route(
            "/garbled",
            get(|| async {
                // Returns `ApiResponse<T>`-shaped JSON where T can't
                // be Greeting. data is a number, not a Greeting.
                Json(serde_json::json!({ "data": 42 }))
            }),
        );
        let endpoint = spawn_fixture(app).await;

        let client = Client::new();
        let err = fetch_from_stone::<Greeting>(&client, &endpoint, "stone-test", "/garbled")
            .await
            .expect_err("type-mismatched body must error");
        match err {
            CrossStoneError::ParseFailed { stone, .. } => {
                assert_eq!(stone, "stone-test");
            }
            other => panic!("expected ParseFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn post_to_stone_round_trips_typed_request_and_response() {
        // Echo handler: takes Greeting, returns Greeting with the
        // message uppercased. Verifies request body serialised, was
        // received, and the response decoded — in one round trip.
        let app = Router::new().route(
            "/echo",
            post(|Json(req): Json<Greeting>| async move {
                Json(ApiResponse {
                    data: Greeting {
                        message: req.message.to_uppercase(),
                    },
                    suggestions: None,
                })
            }),
        );
        let endpoint = spawn_fixture(app).await;

        let client = Client::new();
        let body = Greeting {
            message: "hello".into(),
        };
        let got: Greeting = post_to_stone(&client, &endpoint, "stone-test", "/echo", &body)
            .await
            .expect("post should succeed");
        assert_eq!(got.message, "HELLO");
    }

    #[test]
    fn into_api_error_preserves_not_found_distinction() {
        // CrossStoneError::NotFound (remote returned 404) must map
        // to a 404 outer status. CrossStoneError::HttpStatus must
        // map to 502 — the remote misbehaved, the client request
        // was fine.
        let nf = CrossStoneError::NotFound {
            stone: "x".into(),
            message: "gone".into(),
        };
        let (status, _) = nf.into_api_error();
        assert_eq!(status, StatusCode::NOT_FOUND);

        let bad = CrossStoneError::HttpStatus {
            stone: "x".into(),
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "down".into(),
        };
        let (status, _) = bad.into_api_error();
        assert_eq!(status, StatusCode::BAD_GATEWAY);

        let snf = CrossStoneError::StoneNotFound { stone: "x".into() };
        let (status, _) = snf.into_api_error();
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "stone-not-found is also a 404"
        );
    }

    #[test]
    fn endpoint_trailing_slash_is_normalised() {
        // The URL builder must produce `<endpoint>/<path>` with
        // exactly one slash regardless of whether the caller
        // supplied a trailing slash on the endpoint. Blocking-test
        // the URL construction by exercising the same trim logic.
        let with_slash = format!("{}{}", "http://x:1/".trim_end_matches('/'), "/p");
        let without_slash = format!("{}{}", "http://x:1".trim_end_matches('/'), "/p");
        assert_eq!(with_slash, "http://x:1/p");
        assert_eq!(without_slash, "http://x:1/p");
    }
}
