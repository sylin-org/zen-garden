//! API key validation for cloud providers.
//!
//! Each provider has a free metadata endpoint that validates the key
//! without consuming tokens or credits.

use axum::extract::State;
use axum::Json;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::AppState;

const TEST_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Deserialize)]
pub struct TestKeyRequest {
    pub provider: String,
    pub api_key: String,
    pub base_url: Option<String>,
}

#[derive(Serialize)]
pub struct TestKeyResponse {
    pub valid: bool,
    pub provider: String,
    pub message: String,
    /// Number of models found (if key is valid and endpoint returns models).
    pub model_count: Option<usize>,
    /// Model IDs found (if key is valid).
    pub model_names: Vec<String>,
}

/// `POST /api/providers/test` — validate an API key without saving it.
pub async fn test_key(
    State(_state): State<AppState>,
    Json(req): Json<TestKeyRequest>,
) -> Json<TestKeyResponse> {
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    let result = match req.provider.as_str() {
        "openai" => test_openai(&client, &req.api_key, req.base_url.as_deref()).await,
        "anthropic" => test_anthropic(&client, &req.api_key, req.base_url.as_deref()).await,
        "google" => test_google(&client, &req.api_key, req.base_url.as_deref()).await,
        "cohere" => test_cohere(&client, &req.api_key, req.base_url.as_deref()).await,
        "deepgram" => test_deepgram(&client, &req.api_key, req.base_url.as_deref()).await,
        "stability-ai" => test_stability(&client, &req.api_key, req.base_url.as_deref()).await,
        "elevenlabs" => test_elevenlabs(&client, &req.api_key, req.base_url.as_deref()).await,
        other => Err(format!("unknown provider: {other}")),
    };

    match result {
        Ok((message, model_names)) => Json(TestKeyResponse {
            valid: true,
            provider: req.provider,
            message,
            model_count: Some(model_names.len()),
            model_names,
        }),
        Err(message) => Json(TestKeyResponse {
            valid: false,
            provider: req.provider,
            message,
            model_count: None,
            model_names: vec![],
        }),
    }
}

/// OpenAI: GET /v1/models with Bearer auth.
async fn test_openai(
    client: &Client,
    api_key: &str,
    base_url: Option<&str>,
) -> Result<(String, Vec<String>), String> {
    let base = base_url.unwrap_or("https://api.openai.com");
    let resp = client
        .get(format!("{base}/v1/models"))
        .header("Authorization", format!("Bearer {api_key}"))
        .timeout(TEST_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("connection failed: {e}"))?;

    let status = resp.status();
    if status.as_u16() == 401 {
        return Err("invalid API key".to_string());
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {}", truncate(&body, 200)));
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| format!("parse error: {e}"))?;
    let names: Vec<String> = body
        .get("data")
        .and_then(|d| d.as_array())
        .map(|a| a.iter().filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(String::from)).collect())
        .unwrap_or_default();

    let count = names.len();
    Ok((format!("valid — {count} models available"), names))
}

/// Anthropic: GET /v1/models with x-api-key + anthropic-version.
async fn test_anthropic(
    client: &Client,
    api_key: &str,
    base_url: Option<&str>,
) -> Result<(String, Vec<String>), String> {
    let base = base_url.unwrap_or("https://api.anthropic.com");
    let resp = client
        .get(format!("{base}/v1/models"))
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .timeout(TEST_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("connection failed: {e}"))?;

    let status = resp.status();
    if status.as_u16() == 401 {
        return Err("invalid API key".to_string());
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {}", truncate(&body, 200)));
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| format!("parse error: {e}"))?;
    let names: Vec<String> = body
        .get("data")
        .and_then(|d| d.as_array())
        .map(|a| a.iter().filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(String::from)).collect())
        .unwrap_or_default();

    let count = names.len();
    Ok((format!("valid — {count} models available"), names))
}

/// Google AI: GET /v1/models?key={key} (key in query param, not header).
async fn test_google(
    client: &Client,
    api_key: &str,
    base_url: Option<&str>,
) -> Result<(String, Vec<String>), String> {
    let base = base_url.unwrap_or("https://generativelanguage.googleapis.com");
    let resp = client
        .get(format!("{base}/v1/models"))
        .query(&[("key", api_key)])
        .timeout(TEST_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("connection failed: {e}"))?;

    // Google returns 400 (not 401) for invalid keys
    let status = resp.status();
    if status.as_u16() == 400 || status.as_u16() == 401 {
        let body = resp.text().await.unwrap_or_default();
        if body.contains("API_KEY_INVALID") {
            return Err("invalid API key".to_string());
        }
        return Err(format!("rejected: {}", truncate(&body, 200)));
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {}", truncate(&body, 200)));
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| format!("parse error: {e}"))?;
    let names: Vec<String> = body
        .get("models")
        .and_then(|m| m.as_array())
        .map(|a| a.iter().filter_map(|m| {
            m.get("name").and_then(|v| v.as_str())
                .or_else(|| m.get("id").and_then(|v| v.as_str()))
                .map(String::from)
        }).collect())
        .unwrap_or_default();

    let count = names.len();
    Ok((format!("valid — {count} models available"), names))
}

/// Cohere: GET /v2/models with Bearer auth.
async fn test_cohere(
    client: &Client,
    api_key: &str,
    base_url: Option<&str>,
) -> Result<(String, Vec<String>), String> {
    let base = base_url.unwrap_or("https://api.cohere.com");
    let resp = client
        .get(format!("{base}/v2/models"))
        .header("Authorization", format!("Bearer {api_key}"))
        .timeout(TEST_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("connection failed: {e}"))?;

    let status = resp.status();
    if status.as_u16() == 401 {
        return Err("invalid API key".to_string());
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {}", truncate(&body, 200)));
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| format!("parse error: {e}"))?;
    let names: Vec<String> = body
        .get("models")
        .and_then(|m| m.as_array())
        .map(|a| a.iter().filter_map(|m| {
            m.get("name").and_then(|v| v.as_str())
                .or_else(|| m.get("id").and_then(|v| v.as_str()))
                .map(String::from)
        }).collect())
        .unwrap_or_default();

    let count = names.len();
    Ok((format!("valid — {count} models available"), names))
}

/// Deepgram: GET /v1/projects with Token auth.
async fn test_deepgram(
    client: &Client,
    api_key: &str,
    base_url: Option<&str>,
) -> Result<(String, Vec<String>), String> {
    let base = base_url.unwrap_or("https://api.deepgram.com");
    let resp = client
        .get(format!("{base}/v1/projects"))
        .header("Authorization", format!("Token {api_key}"))
        .timeout(TEST_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("connection failed: {e}"))?;

    let status = resp.status();
    if status.as_u16() == 401 {
        return Err("invalid API key".to_string());
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {}", truncate(&body, 200)));
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| format!("parse error: {e}"))?;
    let names: Vec<String> = body
        .get("projects")
        .and_then(|p| p.as_array())
        .map(|a| a.iter().filter_map(|p| p.get("name").and_then(|v| v.as_str()).map(String::from)).collect())
        .unwrap_or_default();

    let count = names.len();
    Ok((format!("valid — {count} projects"), names))
}

/// Stability AI: GET /v1/user/account with Bearer auth.
async fn test_stability(
    client: &Client,
    api_key: &str,
    base_url: Option<&str>,
) -> Result<(String, Vec<String>), String> {
    let base = base_url.unwrap_or("https://api.stability.ai");
    let resp = client
        .get(format!("{base}/v1/user/account"))
        .header("Authorization", format!("Bearer {api_key}"))
        .timeout(TEST_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("connection failed: {e}"))?;

    let status = resp.status();
    if status.as_u16() == 401 {
        return Err("invalid API key".to_string());
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {}", truncate(&body, 200)));
    }

    Ok(("valid — account verified".to_string(), vec![]))
}

/// ElevenLabs: GET /v1/user with xi-api-key header.
async fn test_elevenlabs(
    client: &Client,
    api_key: &str,
    base_url: Option<&str>,
) -> Result<(String, Vec<String>), String> {
    let base = base_url.unwrap_or("https://api.elevenlabs.io");
    let resp = client
        .get(format!("{base}/v1/user"))
        .header("xi-api-key", api_key)
        .timeout(TEST_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("connection failed: {e}"))?;

    let status = resp.status();
    if status.as_u16() == 401 {
        return Err("invalid API key".to_string());
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {}", truncate(&body, 200)));
    }

    Ok(("valid — account verified".to_string(), vec![]))
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}...", &s[..max])
    } else {
        s.to_string()
    }
}
