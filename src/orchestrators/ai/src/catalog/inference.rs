//! Canonical inference types — our stable API contract.
//!
//! OpenAI-shaped, version-locked to us. If OpenAI changes their API,
//! only the OpenAI provider adapter changes — our clients don't break.
//!
//! These types are used by:
//! - The unified API handlers (`api/unified.rs`)
//! - All provider implementations (`providers/*.rs`)
//! - The `Provider` trait in `catalog/traits.rs`

use std::pin::Pin;

use anyhow::Result;
use bytes::Bytes;
use futures_util::Stream;
use serde::{Deserialize, Serialize};

// ── Boxed Stream ────────────────────────────────────────────────

pub type BoxStream<'a, T> = Pin<Box<dyn Stream<Item = T> + Send + 'a>>;

// ── Chat Inference ──────────────────────────────────────────────

/// A single message in the conversation.
///
/// Content is `serde_json::Value` (not `String`) because OpenAI content
/// can be a string or an array of content parts (text, image_url, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Pass-through for fields we don't explicitly model (name, etc.).
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Canonical chat inference request (OpenAI `/v1/chat/completions` shape).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
    #[serde(default)]
    pub stream: bool,
    /// Catch-all for provider-specific fields the client passes through.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Canonical chat inference response (non-streaming).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceResponse {
    pub id: String,
    pub object: String,
    pub model: String,
    pub choices: Vec<InferenceChoice>,
    pub usage: Usage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceChoice {
    pub index: u32,
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
}

/// A single SSE chunk in a streaming response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceChunk {
    pub id: String,
    pub object: String,
    pub model: String,
    pub choices: Vec<ChunkChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkChoice {
    pub index: u32,
    pub delta: ChatMessage,
    pub finish_reason: Option<String>,
}

// ── Embeddings ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedRequest {
    pub model: String,
    /// String or `Vec<String>` — matches OpenAI's polymorphic input.
    pub input: serde_json::Value,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedResponse {
    pub object: String,
    pub data: Vec<EmbeddingData>,
    pub model: String,
    pub usage: Usage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingData {
    pub object: String,
    pub index: u32,
    pub embedding: Vec<f64>,
}

// ── Speech (TTS) ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeechRequest {
    pub model: String,
    pub input: String,
    pub voice: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
}

/// TTS response — raw audio bytes with content type.
/// Audio may be complete or chunked-streamed (both OpenAI and OpenedAI Speech stream).
pub struct SpeechResponse {
    pub content_type: String,
    pub audio: SpeechAudio,
}

impl std::fmt::Debug for SpeechResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpeechResponse")
            .field("content_type", &self.content_type)
            .finish_non_exhaustive()
    }
}

pub enum SpeechAudio {
    Complete(Vec<u8>),
    Stream(BoxStream<'static, Result<Bytes>>),
}

// ── Transcription (STT) ─────────────────────────────────────────

/// STT request. The unified API handler parses multipart/form-data into
/// this struct before passing to the adapter.
pub struct TranscribeRequest {
    pub model: String,
    pub audio: Vec<u8>,
    pub filename: String,
    pub language: Option<String>,
    pub response_format: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscribeResponse {
    pub text: String,
}

// ── Shared ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub prompt_tokens: u64,
    #[serde(default)]
    pub completion_tokens: u64,
    #[serde(default)]
    pub total_tokens: u64,
}

