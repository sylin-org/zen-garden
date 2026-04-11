//! Typed client for Stone REST APIs (ARCH-0012)
//!
//! Replaces raw `reqwest::Client` usage with typed endpoint methods.
//! All `ApiResponse<T>` unwrapping happens inside the client — callers get `T`.
//!
//! # Usage
//! ```no_run
//! use garden_common::client::StoneApi;
//!
//! let api = StoneApi::new(reqwest::Client::new(), "http://stone:7185".into());
//! // let services = api.services().list().await?;
//! // let caps = api.stone().capabilities().await?;
//! ```

use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::api_utils::responses::ApiResponse;
use crate::api_utils::ApiErrorResponse;

// ────────────────────────────────────────────────────────────────────────────
// Error type
// ────────────────────────────────────────────────────────────────────────────

/// Structured error from Stone API calls.
///
/// Centralizes connection failures, HTTP errors, and parse errors into a
/// single type that callers can match on when finer-grained handling is needed.
#[derive(Debug, thiserror::Error)]
pub enum StoneApiError {
    /// Transport / connection failure (DNS, timeout, TLS, etc.)
    #[error("connection error: {0}")]
    Connection(#[from] reqwest::Error),

    /// Server returned a non-2xx status with a structured error body
    #[error("HTTP {status}: {message}")]
    Http {
        status: StatusCode,
        code: String,
        message: String,
    },

    /// Server returned a non-2xx status with an unstructured body
    #[error("HTTP {status}: {body}")]
    HttpRaw { status: StatusCode, body: String },

    /// Response body could not be parsed as the expected type
    #[error("failed to parse response: {0}")]
    Parse(#[source] reqwest::Error),

    /// Resource was not found (404)
    #[error("not found: {0}")]
    NotFound(String),
}

impl StoneApiError {
    /// True when the stone returned 404.
    pub fn is_not_found(&self) -> bool {
        matches!(
            self,
            StoneApiError::NotFound(_)
                | StoneApiError::Http {
                    status: StatusCode::NOT_FOUND,
                    ..
                }
                | StoneApiError::HttpRaw {
                    status: StatusCode::NOT_FOUND,
                    ..
                }
        )
    }

    /// Extract a human-readable error message suitable for CLI display.
    pub fn display_message(&self) -> String {
        match self {
            StoneApiError::Connection(e) => format!("Connection failed: {e}"),
            StoneApiError::Http { message, .. } => message.clone(),
            StoneApiError::HttpRaw { status, body } => {
                if body.is_empty() {
                    format!("Request failed with status {status}")
                } else {
                    format!("{status}: {body}")
                }
            }
            StoneApiError::Parse(e) => format!("Failed to parse response: {e}"),
            StoneApiError::NotFound(resource) => format!("Not found: {resource}"),
        }
    }
}

// StoneApiError already implements std::error::Error via thiserror,
// so it converts into anyhow::Error automatically via anyhow's blanket impl.

// ────────────────────────────────────────────────────────────────────────────
// Core client
// ────────────────────────────────────────────────────────────────────────────

/// Typed client for the Stone (Moss) REST API.
///
/// Wraps a `reqwest::Client` and endpoint URL, providing typed methods
/// for every API family. `ApiResponse<T>` unwrapping and error handling
/// are centralized here — callers receive `T` directly.
#[derive(Debug, Clone)]
pub struct StoneApi {
    client: reqwest::Client,
    endpoint: String,
}

impl StoneApi {
    /// Create a new `StoneApi` targeting the given stone endpoint.
    ///
    /// Trailing slashes are trimmed for consistent URL construction.
    pub fn new(client: reqwest::Client, endpoint: String) -> Self {
        Self {
            client,
            endpoint: endpoint.trim_end_matches('/').to_string(),
        }
    }

    /// The endpoint URL this client targets.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// The underlying `reqwest::Client` for escape-hatch usage (SSE, raw).
    pub fn http(&self) -> &reqwest::Client {
        &self.client
    }

    // ── Endpoint families ────────────────────────────────────────────

    /// Service lifecycle operations.
    pub fn services(&self) -> ServicesApi<'_> {
        ServicesApi { api: self }
    }

    /// Offering catalog operations.
    pub fn offerings(&self) -> OfferingsApi<'_> {
        OfferingsApi { api: self }
    }

    /// Storage and S3 operations.
    pub fn storage(&self) -> StorageApi<'_> {
        StorageApi { api: self }
    }

    /// Pond (security / trust) operations.
    pub fn pond(&self) -> PondApi<'_> {
        PondApi { api: self }
    }

    /// Companion management operations.
    pub fn companions(&self) -> CompanionsApi<'_> {
        CompanionsApi { api: self }
    }

    /// Stone identity, capabilities, and system info.
    pub fn stone(&self) -> StoneInfoApi<'_> {
        StoneInfoApi { api: self }
    }

    /// Garden-wide (orchestrated) operations.
    pub fn garden(&self) -> GardenApi<'_> {
        GardenApi { api: self }
    }

    // ── Core HTTP verbs (private) ────────────────────────────────────

    /// GET returning `T` unwrapped from `ApiResponse<T>`.
    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, StoneApiError> {
        let url = self.url(path);
        let response = self.client.get(&url).send().await?;
        self.parse_api_response(response, &url).await
    }

    /// GET with query parameters returning `T` unwrapped from `ApiResponse<T>`.
    async fn get_query<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<T, StoneApiError> {
        let url = self.url_with_query(path, query);
        let response = self.client.get(&url).send().await?;
        self.parse_api_response(response, &url).await
    }

    /// POST with JSON body, returning `T` unwrapped from `ApiResponse<T>`.
    async fn post<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, StoneApiError> {
        let url = self.url(path);
        let response = self.client.post(&url).json(body).send().await?;
        self.parse_api_response(response, &url).await
    }

    /// POST without body, returning `T` unwrapped from `ApiResponse<T>`.
    async fn post_empty<T: DeserializeOwned>(&self, path: &str) -> Result<T, StoneApiError> {
        let url = self.url(path);
        let response = self.client.post(&url).send().await?;
        self.parse_api_response(response, &url).await
    }

    /// POST without body, returning raw `reqwest::Response`.
    async fn post_raw(&self, path: &str) -> Result<reqwest::Response, StoneApiError> {
        let url = self.url(path);
        let response = self.client.post(&url).send().await?;
        self.check_status(response, &url).await
    }

    /// POST with JSON body, returning raw `reqwest::Response`.
    async fn post_raw_with_body<B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<reqwest::Response, StoneApiError> {
        let url = self.url(path);
        let response = self.client.post(&url).json(body).send().await?;
        self.check_status(response, &url).await
    }

    /// DELETE returning raw `reqwest::Response`.
    async fn delete_raw(&self, path: &str) -> Result<reqwest::Response, StoneApiError> {
        let url = self.url(path);
        let response = self.client.delete(&url).send().await?;
        self.check_status(response, &url).await
    }

    /// PATCH with JSON body, returning `T` unwrapped from `ApiResponse<T>`.
    async fn patch<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, StoneApiError> {
        let url = self.url(path);
        let response = self.client.patch(&url).json(body).send().await?;
        self.parse_api_response(response, &url).await
    }

    /// PUT with JSON body, returning `T` unwrapped from `ApiResponse<T>`.
    async fn put<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, StoneApiError> {
        let url = self.url(path);
        let response = self.client.put(&url).json(body).send().await?;
        self.parse_api_response(response, &url).await
    }

    /// PUT with raw bytes, returning raw `reqwest::Response`.
    #[expect(dead_code, reason = "used by endpoint families as they are migrated")]
    async fn put_bytes(
        &self,
        path: &str,
        content_type: &str,
        body: Vec<u8>,
    ) -> Result<reqwest::Response, StoneApiError> {
        let url = self.url(path);
        let response = self
            .client
            .put(&url)
            .header("Content-Type", content_type)
            .body(body)
            .send()
            .await?;
        self.check_status(response, &url).await
    }

    /// HEAD returning raw `reqwest::Response`.
    #[expect(dead_code, reason = "used by endpoint families as they are migrated")]
    async fn head_raw(&self, path: &str) -> Result<reqwest::Response, StoneApiError> {
        let url = self.url(path);
        let response = self.client.head(&url).send().await?;
        self.check_status(response, &url).await
    }

    /// GET returning raw `reqwest::Response` (for SSE streams, binary, etc.).
    async fn get_raw(&self, path: &str) -> Result<reqwest::Response, StoneApiError> {
        let url = self.url(path);
        let response = self.client.get(&url).send().await?;
        self.check_status(response, &url).await
    }

    /// GET returning the response parsed directly as `T` (no `ApiResponse` wrapper).
    async fn get_bare<T: DeserializeOwned>(&self, path: &str) -> Result<T, StoneApiError> {
        let url = self.url(path);
        let response = self.client.get(&url).send().await?;
        let response = self.check_status(response, &url).await?;
        response.json::<T>().await.map_err(StoneApiError::Parse)
    }

    // ── Internal helpers ─────────────────────────────────────────────

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.endpoint, path)
    }

    fn url_with_query(&self, path: &str, query: &[(&str, &str)]) -> String {
        if query.is_empty() {
            return self.url(path);
        }
        let qs: Vec<String> = query
            .iter()
            .map(|(k, v)| {
                format!(
                    "{}={}",
                    urlencoding::encode(k),
                    urlencoding::encode(v)
                )
            })
            .collect();
        format!("{}{}?{}", self.endpoint, path, qs.join("&"))
    }

    /// Parse a response expected to contain `ApiResponse<T>`, unwrapping to `T`.
    async fn parse_api_response<T: DeserializeOwned>(
        &self,
        response: reqwest::Response,
        url: &str,
    ) -> Result<T, StoneApiError> {
        let status = response.status();

        if !status.is_success() {
            return Err(self.build_error(response, status).await);
        }

        let api_response: ApiResponse<T> = response.json().await.map_err(|e| {
            tracing::warn!(url = url, error = %e, "Failed to parse ApiResponse");
            StoneApiError::Parse(e)
        })?;

        Ok(api_response.data)
    }

    /// Verify 2xx status, returning the response for further processing.
    async fn check_status(
        &self,
        response: reqwest::Response,
        _url: &str,
    ) -> Result<reqwest::Response, StoneApiError> {
        let status = response.status();
        if !status.is_success() {
            return Err(self.build_error(response, status).await);
        }
        Ok(response)
    }

    /// Build a `StoneApiError` from a non-2xx response.
    async fn build_error(
        &self,
        response: reqwest::Response,
        status: StatusCode,
    ) -> StoneApiError {
        // Try to parse structured error first
        let body = response.text().await.unwrap_or_default();

        if status == StatusCode::NOT_FOUND {
            return StoneApiError::NotFound(body);
        }

        if let Ok(api_error) = serde_json::from_str::<ApiErrorResponse>(&body) {
            return StoneApiError::Http {
                status,
                code: api_error.error.code,
                message: api_error.error.message,
            };
        }

        StoneApiError::HttpRaw { status, body }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Services API
// ────────────────────────────────────────────────────────────────────────────

/// Service lifecycle operations.
pub struct ServicesApi<'a> {
    api: &'a StoneApi,
}

impl ServicesApi<'_> {
    /// List all local services.
    pub async fn list(&self) -> Result<Vec<crate::ServiceInfo>, StoneApiError> {
        self.api.get("/api/v1/stone/services").await
    }

    /// Create a service from an offering. Returns raw response for status inspection.
    pub async fn create<B: Serialize>(
        &self,
        body: &B,
    ) -> Result<reqwest::Response, StoneApiError> {
        self.api
            .post_raw_with_body("/api/v1/stone/services", body)
            .await
    }

    /// Get details for a single service.
    pub async fn get(&self, name: &str) -> Result<crate::ServiceInfo, StoneApiError> {
        let path = format!("/api/v1/stone/services/{}", urlencoding::encode(name));
        self.api.get(&path).await
    }

    /// Start (wake) a stopped service. Returns raw response for status inspection.
    pub async fn wake(&self, name: &str) -> Result<reqwest::Response, StoneApiError> {
        let path = format!("/api/v1/stone/services/{}/wake", urlencoding::encode(name));
        self.api.post_raw(&path).await
    }

    /// Stop (rest) a running service. Returns raw response for status inspection.
    pub async fn rest(&self, name: &str) -> Result<reqwest::Response, StoneApiError> {
        let path = format!("/api/v1/stone/services/{}/rest", urlencoding::encode(name));
        self.api.post_raw(&path).await
    }

    /// Restart a service. Returns raw response for status inspection.
    pub async fn restart(&self, name: &str) -> Result<reqwest::Response, StoneApiError> {
        let path = format!(
            "/api/v1/stone/services/{}/restart",
            urlencoding::encode(name)
        );
        self.api.post_raw(&path).await
    }

    /// Upgrade a service. Returns raw response for status inspection.
    pub async fn upgrade(&self, name: &str) -> Result<reqwest::Response, StoneApiError> {
        let path = format!(
            "/api/v1/stone/services/{}/upgrade",
            urlencoding::encode(name)
        );
        self.api.post_raw(&path).await
    }

    /// Remove (delete) a service. Returns raw response.
    pub async fn remove(&self, name: &str) -> Result<reqwest::Response, StoneApiError> {
        let path = format!("/api/v1/stone/services/{}", urlencoding::encode(name));
        self.api.delete_raw(&path).await
    }

    /// Stream logs for a service (SSE). Returns raw response for streaming.
    pub async fn logs(&self, name: &str) -> Result<reqwest::Response, StoneApiError> {
        let path = format!("/api/v1/stone/services/{}/logs", urlencoding::encode(name));
        self.api.get_raw(&path).await
    }

    /// Read environment variables for a service.
    pub async fn env(&self, name: &str) -> Result<serde_json::Value, StoneApiError> {
        let path = format!("/api/v1/stone/services/{}/env", urlencoding::encode(name));
        self.api.get(&path).await
    }

    /// Set/delete environment variables for a service.
    pub async fn set_env<B: Serialize>(
        &self,
        name: &str,
        body: &B,
    ) -> Result<serde_json::Value, StoneApiError> {
        let path = format!("/api/v1/stone/services/{}/env", urlencoding::encode(name));
        self.api.patch(&path, body).await
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Offerings API
// ────────────────────────────────────────────────────────────────────────────

/// Offering catalog operations.
pub struct OfferingsApi<'a> {
    api: &'a StoneApi,
}

impl OfferingsApi<'_> {
    /// List all offerings.
    pub async fn list(&self) -> Result<serde_json::Value, StoneApiError> {
        self.api.get("/api/v1/stone/offerings").await
    }

    /// Search offerings.
    pub async fn search(
        &self,
        query: &str,
        prefer: Option<&str>,
        limit: Option<u32>,
    ) -> Result<crate::offerings::OfferingSearchResponse, StoneApiError> {
        let limit_str = limit.unwrap_or(5).to_string();
        let mut params: Vec<(&str, &str)> = vec![("q", query), ("limit", &limit_str)];
        if let Some(p) = prefer {
            params.push(("prefer", p));
        }
        self.api
            .get_query("/api/v1/stone/offerings/search", &params)
            .await
    }

    /// Get offering details.
    pub async fn get(&self, name: &str) -> Result<serde_json::Value, StoneApiError> {
        let path = format!("/api/v1/stone/offerings/{}", urlencoding::encode(name));
        self.api.get(&path).await
    }

    /// Plant (install) an offering.
    pub async fn plant<B: Serialize>(&self, body: &B) -> Result<serde_json::Value, StoneApiError> {
        self.api.post("/api/v1/stone/offerings", body).await
    }

    /// Remove an offering.
    pub async fn remove(&self, name: &str) -> Result<reqwest::Response, StoneApiError> {
        let path = format!("/api/v1/stone/offerings/{}", urlencoding::encode(name));
        self.api.delete_raw(&path).await
    }

    /// Refresh the offering catalog.
    pub async fn refresh(&self) -> Result<serde_json::Value, StoneApiError> {
        self.api
            .post_empty("/api/v1/stone/offerings/refresh")
            .await
    }

    /// Inspect a Docker image.
    pub async fn inspect(&self, image: &str) -> Result<serde_json::Value, StoneApiError> {
        self.api
            .get_query(
                "/api/v1/stone/offerings/inspect",
                &[("image", image)],
            )
            .await
    }

    /// Heal (adopt orphaned containers).
    pub async fn heal(&self) -> Result<serde_json::Value, StoneApiError> {
        self.api.post_empty("/api/v1/stone/offerings/heal").await
    }

    /// Get capabilities for an offering (models, extensions, modules).
    pub async fn capabilities(
        &self,
        name: &str,
    ) -> Result<serde_json::Value, StoneApiError> {
        let path = format!(
            "/api/v1/stone/offerings/{}/capabilities",
            urlencoding::encode(name)
        );
        self.api.get(&path).await
    }

    /// Add a capability to an offering.
    pub async fn add_capability<B: Serialize>(
        &self,
        name: &str,
        body: &B,
    ) -> Result<serde_json::Value, StoneApiError> {
        let path = format!(
            "/api/v1/stone/offerings/{}/capabilities",
            urlencoding::encode(name)
        );
        self.api.post(&path, body).await
    }

    /// Remove a capability from an offering.
    pub async fn remove_capability(
        &self,
        name: &str,
        cap: &str,
        cap_type: Option<&str>,
    ) -> Result<reqwest::Response, StoneApiError> {
        let encoded_name = urlencoding::encode(name);
        let encoded_cap = urlencoding::encode(cap);
        let path = match cap_type {
            Some(t) => format!(
                "/api/v1/stone/offerings/{}/capabilities/{}?type={}",
                encoded_name,
                encoded_cap,
                urlencoding::encode(t)
            ),
            None => format!(
                "/api/v1/stone/offerings/{}/capabilities/{}",
                encoded_name, encoded_cap
            ),
        };
        self.api.delete_raw(&path).await
    }

    /// Refresh capabilities for an offering.
    pub async fn refresh_capabilities<B: Serialize>(
        &self,
        name: &str,
        body: &B,
    ) -> Result<serde_json::Value, StoneApiError> {
        let path = format!(
            "/api/v1/stone/offerings/{}/capabilities/refresh",
            urlencoding::encode(name)
        );
        self.api.post(&path, body).await
    }

    /// Mirror capabilities from another stone.
    pub async fn mirror_capabilities<B: Serialize>(
        &self,
        name: &str,
        body: &B,
    ) -> Result<serde_json::Value, StoneApiError> {
        let path = format!(
            "/api/v1/stone/offerings/{}/capabilities/mirror",
            urlencoding::encode(name)
        );
        self.api.post(&path, body).await
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Storage API
// ────────────────────────────────────────────────────────────────────────────

/// Storage management operations.
pub struct StorageApi<'a> {
    api: &'a StoneApi,
}

impl StorageApi<'_> {
    /// Storage overview (garden-wide).
    pub async fn overview(&self) -> Result<serde_json::Value, StoneApiError> {
        self.api.get("/api/v1/stone/storage").await
    }

    /// Storage health status.
    pub async fn health(&self) -> Result<serde_json::Value, StoneApiError> {
        self.api.get("/api/v1/stone/storage/health").await
    }

    /// Eligible storage candidates.
    pub async fn candidates(&self) -> Result<crate::storage::CandidatesResponse, StoneApiError> {
        self.api.get_bare("/api/v1/stone/storage/candidates").await
    }

    /// Add a storage device or directory.
    pub async fn add(
        &self,
        request: &crate::storage::AddStorageRequest,
    ) -> Result<serde_json::Value, StoneApiError> {
        self.api.post("/api/v1/stone/storage/add", request).await
    }

    /// List local storage banks.
    pub async fn banks(&self) -> Result<Vec<crate::storage::StorageInfo>, StoneApiError> {
        self.api.get("/api/v1/stone/storage/banks").await
    }

    /// Get a specific storage bank's details.
    pub async fn bank(&self, name: &str) -> Result<serde_json::Value, StoneApiError> {
        let path = format!("/api/v1/stone/storage/banks/{}", urlencoding::encode(name));
        self.api.get(&path).await
    }

    /// Remove a storage bank.
    pub async fn remove(&self, name: &str) -> Result<reqwest::Response, StoneApiError> {
        let path = format!("/api/v1/stone/storage/banks/{}", urlencoding::encode(name));
        self.api.delete_raw(&path).await
    }

    /// Release (unmount) a storage bank.
    pub async fn release(&self, name: &str) -> Result<serde_json::Value, StoneApiError> {
        let path = format!(
            "/api/v1/stone/storage/banks/{}/release",
            urlencoding::encode(name)
        );
        self.api.post_empty(&path).await
    }

    /// Release all storage banks.
    pub async fn release_all(&self) -> Result<serde_json::Value, StoneApiError> {
        self.api.post_empty("/api/v1/stone/storage/release-all").await
    }

    /// Pin (claim Primary role).
    pub async fn pin(&self, name: &str) -> Result<serde_json::Value, StoneApiError> {
        let path = format!(
            "/api/v1/stone/storage/banks/{}/pin",
            urlencoding::encode(name)
        );
        self.api.post_empty(&path).await
    }

    /// Unpin (release Primary role).
    pub async fn unpin(&self, name: &str) -> Result<serde_json::Value, StoneApiError> {
        let path = format!(
            "/api/v1/stone/storage/banks/{}/unpin",
            urlencoding::encode(name)
        );
        self.api.post_empty(&path).await
    }

    /// Rename a storage bank.
    pub async fn rename<B: Serialize>(
        &self,
        name: &str,
        body: &B,
    ) -> Result<serde_json::Value, StoneApiError> {
        let path = format!(
            "/api/v1/stone/storage/banks/{}/rename",
            urlencoding::encode(name)
        );
        self.api.patch(&path, body).await
    }

    /// Set visibility for a storage bank.
    pub async fn set_visibility<B: Serialize>(
        &self,
        name: &str,
        body: &B,
    ) -> Result<serde_json::Value, StoneApiError> {
        let path = format!(
            "/api/v1/stone/storage/banks/{}/visibility",
            urlencoding::encode(name)
        );
        self.api.patch(&path, body).await
    }

    /// Set roles for a storage bank.
    pub async fn set_roles<B: Serialize>(
        &self,
        name: &str,
        body: &B,
    ) -> Result<serde_json::Value, StoneApiError> {
        let path = format!(
            "/api/v1/stone/storage/banks/{}/roles",
            urlencoding::encode(name)
        );
        self.api.patch(&path, body).await
    }

    /// Replication changelog for a storage bank.
    pub async fn changes(&self, name: &str) -> Result<serde_json::Value, StoneApiError> {
        let path = format!(
            "/api/v1/stone/storage/banks/{}/changes",
            urlencoding::encode(name)
        );
        self.api.get(&path).await
    }

    /// SSE replication stream. Returns raw response for streaming.
    pub async fn stream(&self) -> Result<reqwest::Response, StoneApiError> {
        self.api.get_raw("/api/v1/stone/storage/stream").await
    }

    /// List offerings that have snapshots on a bank.
    pub async fn snapshots(
        &self,
        bank: &str,
    ) -> Result<serde_json::Value, StoneApiError> {
        let path = format!(
            "/api/v1/stone/storage/banks/{}/snapshots",
            urlencoding::encode(bank)
        );
        self.api.get(&path).await
    }

    /// List snapshots for a specific offering on a bank.
    pub async fn offering_snapshots(
        &self,
        bank: &str,
        offering: &str,
    ) -> Result<serde_json::Value, StoneApiError> {
        let path = format!(
            "/api/v1/stone/storage/banks/{}/snapshots/{}",
            urlencoding::encode(bank),
            urlencoding::encode(offering)
        );
        self.api.get(&path).await
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Pond API
// ────────────────────────────────────────────────────────────────────────────

/// Pond (security / trust mesh) operations.
pub struct PondApi<'a> {
    api: &'a StoneApi,
}

impl PondApi<'_> {
    /// Initialize the pond (place keystone / create CA).
    pub async fn init<B: Serialize>(&self, body: &B) -> Result<serde_json::Value, StoneApiError> {
        self.api.post("/api/v1/pond/init", body).await
    }

    /// Get pond status and membership.
    pub async fn status(&self) -> Result<serde_json::Value, StoneApiError> {
        self.api.get("/api/v1/pond/status").await
    }

    /// Join a pond with TOTP code.
    pub async fn join<B: Serialize>(&self, body: &B) -> Result<serde_json::Value, StoneApiError> {
        self.api.post("/api/v1/pond/join", body).await
    }

    /// Open enrollment / rotate auth.
    pub async fn invite<B: Serialize>(
        &self,
        body: &B,
    ) -> Result<serde_json::Value, StoneApiError> {
        self.api.post("/api/v1/pond/invite", body).await
    }

    /// Unlock CA after restart.
    pub async fn unlock<B: Serialize>(
        &self,
        body: &B,
    ) -> Result<serde_json::Value, StoneApiError> {
        self.api.post("/api/v1/pond/unlock", body).await
    }

    /// Drain pond (destroy CA).
    pub async fn drain(&self) -> Result<reqwest::Response, StoneApiError> {
        self.api.delete_raw("/api/v1/pond").await
    }

    /// Untrust / revoke a stone.
    pub async fn revoke(&self, stone_name: &str) -> Result<reqwest::Response, StoneApiError> {
        let path = format!(
            "/api/v1/pond/stones/{}",
            urlencoding::encode(stone_name)
        );
        self.api.delete_raw(&path).await
    }

    /// Promote to standby CA.
    pub async fn promote<B: Serialize>(
        &self,
        body: &B,
    ) -> Result<serde_json::Value, StoneApiError> {
        self.api.post("/api/v1/pond/promote", body).await
    }

    /// Rename the pond (decorative).
    pub async fn rename<B: Serialize>(
        &self,
        body: &B,
    ) -> Result<serde_json::Value, StoneApiError> {
        self.api.put("/api/v1/pond/name", body).await
    }

    /// Download CA public certificate. Returns raw response.
    pub async fn ca_cert(&self) -> Result<reqwest::Response, StoneApiError> {
        self.api.get_raw("/api/v1/pond/ca.pem").await
    }

    /// Join a pond — returns raw response for custom status handling.
    pub async fn join_raw<B: Serialize>(
        &self,
        body: &B,
    ) -> Result<reqwest::Response, StoneApiError> {
        let url = self.api.url("/api/v1/pond/join");
        let response = self.api.client.post(&url).json(body).send().await?;
        Ok(response)
    }

    /// Invite — returns raw response for custom status handling.
    pub async fn invite_raw<B: Serialize>(
        &self,
        body: &B,
    ) -> Result<reqwest::Response, StoneApiError> {
        let url = self.api.url("/api/v1/pond/invite");
        let response = self.api.client.post(&url).json(body).send().await?;
        Ok(response)
    }

    /// Run a pond ceremony (guided workflow).
    pub async fn ceremony<B: Serialize>(
        &self,
        body: &B,
    ) -> Result<serde_json::Value, StoneApiError> {
        self.api.post("/api/v1/pond/ceremony", body).await
    }

    /// Get ceremony status.
    pub async fn ceremony_status(&self) -> Result<serde_json::Value, StoneApiError> {
        self.api.get("/api/v1/pond/ceremony").await
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Companions API
// ────────────────────────────────────────────────────────────────────────────

/// Companion management operations.
pub struct CompanionsApi<'a> {
    api: &'a StoneApi,
}

impl CompanionsApi<'_> {
    /// List all companions.
    pub async fn list(&self) -> Result<serde_json::Value, StoneApiError> {
        self.api.get("/api/v1/stone/companions").await
    }

    /// Get companion details.
    pub async fn get(&self, id: &str) -> Result<serde_json::Value, StoneApiError> {
        let path = format!("/api/v1/stone/companions/{}", urlencoding::encode(id));
        self.api.get(&path).await
    }

    /// Forward a command to a companion.
    pub async fn command<B: Serialize>(
        &self,
        id: &str,
        body: &B,
    ) -> Result<serde_json::Value, StoneApiError> {
        let path = format!(
            "/api/v1/stone/companions/{}/command",
            urlencoding::encode(id)
        );
        self.api.post(&path, body).await
    }

    /// Start a companion.
    pub async fn up(&self, id: &str) -> Result<serde_json::Value, StoneApiError> {
        let path = format!("/api/v1/stone/companions/{}/up", urlencoding::encode(id));
        self.api.post_empty(&path).await
    }

    /// Stop a companion.
    pub async fn down(&self, id: &str) -> Result<serde_json::Value, StoneApiError> {
        let path = format!("/api/v1/stone/companions/{}/down", urlencoding::encode(id));
        self.api.post_empty(&path).await
    }

    /// Rescan companion directory.
    pub async fn refresh(&self) -> Result<serde_json::Value, StoneApiError> {
        self.api
            .post_empty("/api/v1/stone/companions/refresh")
            .await
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Stone Info API
// ────────────────────────────────────────────────────────────────────────────

/// Stone identity, capabilities, and system operations.
pub struct StoneInfoApi<'a> {
    api: &'a StoneApi,
}

impl StoneInfoApi<'_> {
    /// Get full capabilities (Tier 1 core + Tier 2 topology).
    pub async fn capabilities(
        &self,
    ) -> Result<crate::types::hardware_topology::FullCapabilities, StoneApiError> {
        self.api.get("/api/v1/stone/capabilities").await
    }

    /// Get Tier 1 core capabilities only (fast, offering compatibility).
    pub async fn capabilities_core(
        &self,
    ) -> Result<crate::HardwareCapabilities, StoneApiError> {
        self.api.get("/api/v1/stone/capabilities/core").await
    }

    /// Get Tier 2 hardware topology only (deep, cached, ARCH-0014).
    pub async fn capabilities_topology(
        &self,
    ) -> Result<crate::types::hardware_topology::HardwareTopology, StoneApiError> {
        self.api.get("/api/v1/stone/capabilities/topology").await
    }

    /// Trigger immediate topology re-probe (flushes cache).
    /// Returns 202 Accepted — the probe runs asynchronously.
    pub async fn capabilities_refresh(&self) -> Result<(), StoneApiError> {
        self.api.post_raw("/api/v1/stone/capabilities/refresh").await?;
        Ok(())
    }

    /// Health check.
    pub async fn health(&self) -> Result<reqwest::Response, StoneApiError> {
        self.api.get_raw("/health").await
    }

    /// Stone hardware resource snapshot (CPU, memory, disk, network, uptime).
    /// Returns raw response — the endpoint returns JSON (`ResourcesSnapshot`).
    ///
    /// Renamed from `metrics()` in ARCH-0018 Book I Chapter 2. The old
    /// name collided with software observability metrics (see
    /// `metrics_snapshot()` for that).
    pub async fn resources(&self) -> Result<reqwest::Response, StoneApiError> {
        self.api.get_raw("/api/v1/stone/resources").await
    }

    /// Pending updates.
    pub async fn updates(&self) -> Result<serde_json::Value, StoneApiError> {
        self.api.get("/api/v1/stone/updates").await
    }

    /// Execute updates.
    pub async fn execute_updates<B: Serialize>(
        &self,
        body: &B,
    ) -> Result<serde_json::Value, StoneApiError> {
        self.api
            .post("/api/v1/stone/updates/execute", body)
            .await
    }

    /// SSE stream for update job. Returns raw response.
    pub async fn update_stream(
        &self,
        job_id: &str,
    ) -> Result<reqwest::Response, StoneApiError> {
        let path = format!(
            "/api/v1/stone/updates/stream/{}",
            urlencoding::encode(job_id)
        );
        self.api.get_raw(&path).await
    }

    /// Recent log lines.
    pub async fn logs(
        &self,
        lines: Option<u32>,
        level: Option<&str>,
    ) -> Result<serde_json::Value, StoneApiError> {
        let mut params: Vec<(&str, String)> = Vec::new();
        if let Some(n) = lines {
            params.push(("lines", n.to_string()));
        }
        if let Some(l) = level {
            params.push(("level", l.to_string()));
        }
        let refs: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
        self.api.get_query("/api/v1/stone/logs", &refs).await
    }

    /// Live log stream (SSE). Returns raw response.
    pub async fn log_stream(&self) -> Result<reqwest::Response, StoneApiError> {
        self.api.get_raw("/api/v1/stone/logs/stream").await
    }

    /// Maintenance history.
    pub async fn maintenance_history(&self) -> Result<serde_json::Value, StoneApiError> {
        self.api.get("/api/v1/stone/maintenance/history").await
    }

    /// Trigger immediate maintenance sweep.
    pub async fn sweep(&self) -> Result<serde_json::Value, StoneApiError> {
        self.api
            .post_empty("/api/v1/stone/maintenance/sweep")
            .await
    }

    /// SSE presence event stream. Returns raw response for streaming consumption.
    pub async fn events(&self) -> Result<reqwest::Response, StoneApiError> {
        self.api.get_raw("/api/v1/stone/presence/stream").await
    }

    /// Get status of a background job.
    pub async fn job_status(&self, job_id: &str) -> Result<serde_json::Value, StoneApiError> {
        let path = format!("/api/v1/jobs/{}", urlencoding::encode(job_id));
        self.api.get(&path).await
    }

    /// Set console output mode (sing/quiet/silent/minimal).
    pub async fn set_console_mode<B: Serialize>(
        &self,
        body: &B,
    ) -> Result<serde_json::Value, StoneApiError> {
        self.api.post("/api/v1/console/mode", body).await
    }

    /// Force registry reconciliation with running containers.
    pub async fn reconcile<B: Serialize>(
        &self,
        body: &B,
    ) -> Result<serde_json::Value, StoneApiError> {
        self.api.post("/api/v1/stone/services/reconcile", body).await
    }

    /// Start a distributed election.
    pub async fn election_start<B: Serialize>(
        &self,
        body: &B,
    ) -> Result<serde_json::Value, StoneApiError> {
        self.api.post("/api/v1/election/start", body).await
    }

    /// Shutdown the stone (power off).
    pub async fn shutdown(&self) -> Result<serde_json::Value, StoneApiError> {
        self.api.post_empty("/api/v1/admin/stone/shutdown").await
    }

    /// Reboot the stone.
    pub async fn reboot(&self) -> Result<serde_json::Value, StoneApiError> {
        self.api.post_empty("/api/v1/admin/stone/reboot").await
    }

    /// Wake a stone via Wake-on-LAN.
    pub async fn wake(&self, name: &str) -> Result<serde_json::Value, StoneApiError> {
        let path = format!("/api/v1/admin/stone/{}/wake", urlencoding::encode(name));
        self.api.post_empty(&path).await
    }

    /// Upload a binary for refresh (dev tool).
    ///
    /// **Note:** Server-side handler is not yet implemented.
    /// Calls will return 404 until the admin refresh endpoint is added to Moss.
    pub async fn refresh_binary<B: Serialize>(
        &self,
        body: &B,
    ) -> Result<serde_json::Value, StoneApiError> {
        self.api.post("/api/v1/admin/moss/refresh", body).await
    }

    /// Get the API manifest (endpoint documentation).
    pub async fn api_manifest(&self) -> Result<serde_json::Value, StoneApiError> {
        self.api.get("/api/v1/manifest").await
    }

    /// List snapshots for offerings (nurturing/backup).
    pub async fn snapshots(&self) -> Result<serde_json::Value, StoneApiError> {
        self.api.get("/api/v1/stone/snapshots").await
    }

    /// Notify stone of tending (visual feedback for companions).
    pub async fn notify_tending<B: Serialize>(
        &self,
        body: &B,
    ) -> Result<reqwest::Response, StoneApiError> {
        self.api.post_raw_with_body("/api/v1/stone/presence/notify", body).await
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Garden API
// ────────────────────────────────────────────────────────────────────────────

/// Garden-wide (orchestrated) operations.
pub struct GardenApi<'a> {
    api: &'a StoneApi,
}

impl GardenApi<'_> {
    /// Find services across the garden.
    pub async fn services(
        &self,
        query: Option<&str>,
    ) -> Result<serde_json::Value, StoneApiError> {
        match query {
            Some(q) => {
                self.api
                    .get_query("/api/v1/garden/services", &[("q", q)])
                    .await
            }
            None => self.api.get("/api/v1/garden/services").await,
        }
    }

    /// Aggregate topology.
    pub async fn observe(
        &self,
    ) -> Result<Vec<crate::TopologyEntry>, StoneApiError> {
        self.api.get("/api/v1/garden/topology").await
    }

    /// Aggregate updates.
    pub async fn updates(&self) -> Result<serde_json::Value, StoneApiError> {
        self.api.get("/api/v1/garden/updates").await
    }

    /// Dispatch updates to affected stones.
    pub async fn execute_updates<B: Serialize>(
        &self,
        body: &B,
    ) -> Result<serde_json::Value, StoneApiError> {
        self.api
            .post("/api/v1/garden/updates/execute", body)
            .await
    }

    /// Garden-wide storage.
    pub async fn storage(&self) -> Result<serde_json::Value, StoneApiError> {
        self.api.get("/api/v1/garden/storage").await
    }

    /// Raw topology (all stones with full detail).
    pub async fn topology(&self) -> Result<serde_json::Value, StoneApiError> {
        self.api.get("/api/v1/garden/topology").await
    }

    /// Placement recommendations for an offering.
    pub async fn recommend<B: Serialize>(
        &self,
        body: &B,
    ) -> Result<serde_json::Value, StoneApiError> {
        self.api.post("/api/v1/garden/recommend", body).await
    }

    /// Garden-wide hardware inspection (fan-out to all stones).
    pub async fn inspect(
        &self,
    ) -> Result<crate::types::hardware_topology::GardenInspection, StoneApiError> {
        self.api.get("/api/v1/garden/inspect").await
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_construction() {
        let api = StoneApi::new(reqwest::Client::new(), "http://localhost:7185".into());
        assert_eq!(
            api.url("/api/v1/stone/services"),
            "http://localhost:7185/api/v1/stone/services"
        );
    }

    #[test]
    fn test_endpoint_trimming() {
        let api = StoneApi::new(reqwest::Client::new(), "http://localhost:7185/".into());
        assert_eq!(api.endpoint(), "http://localhost:7185");
    }

    #[test]
    fn test_trailing_slashes() {
        let api = StoneApi::new(reqwest::Client::new(), "http://localhost:7185///".into());
        assert_eq!(api.endpoint(), "http://localhost:7185");
    }

    #[test]
    fn test_error_display_message() {
        let err = StoneApiError::Http {
            status: StatusCode::BAD_REQUEST,
            code: "INVALID_REQUEST".into(),
            message: "Missing field".into(),
        };
        assert_eq!(err.display_message(), "Missing field");

        let err = StoneApiError::NotFound("service/mongodb".into());
        assert!(err.is_not_found());
        assert!(err.display_message().contains("Not found"));
    }

    #[test]
    fn test_error_is_not_found() {
        assert!(StoneApiError::NotFound("x".into()).is_not_found());
        assert!(StoneApiError::Http {
            status: StatusCode::NOT_FOUND,
            code: "NOT_FOUND".into(),
            message: "gone".into(),
        }
        .is_not_found());
        assert!(!StoneApiError::HttpRaw {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            body: "oops".into(),
        }
        .is_not_found());
    }
}
