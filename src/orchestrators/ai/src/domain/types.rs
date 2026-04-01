//! Core domain types for the AI Orchestrator.
//!
//! Pure data structures — no I/O, no async. Generalized from the Ollama
//! orchestrator's domain types. Every type here replaces its Ollama-specific
//! counterpart (`OllamaInstance` → `ServiceInstance`, etc.).

use std::collections::HashMap;
use std::fmt;
use std::time::Instant;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Offering Kind ───────────────────────────────────────────────

/// Offering type discriminator — enum, not String (code standard §8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfferingKind {
    Ollama,
    ComfyUi,
    Speaches,
    WhisperCpp,
    OpenedaiSpeech,
    Infinity,
    LibreTranslate,
    Docling,
    Kokoro,
    HuggingFace,
    // Cloud providers
    OpenAi,
    Anthropic,
    StabilityAi,
    ElevenLabs,
    Cohere,
    Deepgram,
    Google,
}

impl OfferingKind {
    /// Well-known proxy port for this offering type.
    /// Returns `None` for cloud providers (no proxy port).
    pub fn proxy_port(&self) -> Option<u16> {
        match self {
            Self::Ollama => Some(21434),
            Self::ComfyUi => Some(21435),
            Self::Speaches => Some(21436),
            Self::WhisperCpp => None,                // no proxy — custom /inference API
            Self::OpenedaiSpeech => Some(21437),
            Self::Infinity => Some(21438),
            Self::LibreTranslate => Some(21439),
            Self::Docling => None,                 // no proxy — custom convert API
            Self::Kokoro => None,                  // no proxy — OpenAI-compatible TTS
            Self::HuggingFace => None,             // cloud — no proxy
            Self::OpenAi => None,
            Self::Anthropic => None,
            Self::StabilityAi => None,
            Self::ElevenLabs => None,
            Self::Cohere => None,
            Self::Deepgram => None,
            Self::Google => None,
        }
    }

    /// Short string identifier (matches topology filter names).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ollama => "ollama",
            Self::ComfyUi => "comfyui",
            Self::Speaches => "speaches",
            Self::WhisperCpp => "whispercpp",
            Self::OpenedaiSpeech => "openedai-speech",
            Self::Infinity => "infinity",
            Self::LibreTranslate => "libretranslate",
            Self::Docling => "docling",
            Self::Kokoro => "kokoro",
            Self::HuggingFace => "huggingface",
            Self::OpenAi => "openai",
            Self::Anthropic => "anthropic",
            Self::StabilityAi => "stability-ai",
            Self::ElevenLabs => "elevenlabs",
            Self::Cohere => "cohere",
            Self::Deepgram => "deepgram",
            Self::Google => "google",
        }
    }

    /// Well-known service port for the offering's native API.
    /// Returns `None` for cloud providers (no local port).
    pub fn default_service_port(&self) -> Option<u16> {
        match self {
            Self::Ollama => Some(11434),
            Self::ComfyUi => Some(8188),
            Self::Speaches => Some(8000),
            Self::WhisperCpp => Some(8080),
            Self::OpenedaiSpeech => Some(8001),
            Self::Infinity => Some(7997),
            Self::LibreTranslate => Some(5000),
            Self::Docling => Some(5001),
            Self::Kokoro => Some(8880),
            Self::HuggingFace => None,
            Self::OpenAi => None,
            Self::Anthropic => None,
            Self::StabilityAi => None,
            Self::ElevenLabs => None,
            Self::Cohere => None,
            Self::Deepgram => None,
            Self::Google => None,
        }
    }

    /// Parse an offering name from topology into an `OfferingKind`.
    ///
    /// Matches the `offering` field in `TopologyServiceEntry` against
    /// known AI offering names. Returns `None` for unrecognized names.
    pub fn from_topology_name(name: &str) -> Option<Self> {
        match name {
            "ollama" | "ollama-cpu" => Some(Self::Ollama),
            "comfyui" => Some(Self::ComfyUi),
            "speaches" => Some(Self::Speaches),
            "whispercpp" | "whisper-cpp" => Some(Self::WhisperCpp),
            "openedai-speech" => Some(Self::OpenedaiSpeech),
            "infinity" => Some(Self::Infinity),
            "libretranslate" => Some(Self::LibreTranslate),
            "docling" => Some(Self::Docling),
            "kokoro" => Some(Self::Kokoro),
            "huggingface" => Some(Self::HuggingFace),
            _ => None,
        }
    }

    /// Parse an offering kind from its `as_str()` representation.
    /// Covers both local and cloud providers.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "ollama" => Some(Self::Ollama),
            "comfyui" => Some(Self::ComfyUi),
            "speaches" => Some(Self::Speaches),
            "whispercpp" => Some(Self::WhisperCpp),
            "openedai-speech" => Some(Self::OpenedaiSpeech),
            "infinity" => Some(Self::Infinity),
            "libretranslate" => Some(Self::LibreTranslate),
            "docling" => Some(Self::Docling),
            "kokoro" => Some(Self::Kokoro),
            "huggingface" => Some(Self::HuggingFace),
            "openai" => Some(Self::OpenAi),
            "anthropic" => Some(Self::Anthropic),
            "stability-ai" => Some(Self::StabilityAi),
            "elevenlabs" => Some(Self::ElevenLabs),
            "cohere" => Some(Self::Cohere),
            "deepgram" => Some(Self::Deepgram),
            "google" => Some(Self::Google),
            _ => None,
        }
    }

    /// All local (non-cloud) offering type names used for topology filtering.
    pub const LOCAL_OFFERING_NAMES: &[&str] = &[
        "ollama",
        "ollama-cpu",
        "comfyui",
        "speaches",
        "whispercpp",
        "openedai-speech",
        "infinity",
        "libretranslate",
        "docling",
        "kokoro",
    ];

    /// Whether this offering type is a cloud provider (priority -10 by default).
    pub fn is_cloud(&self) -> bool {
        matches!(
            self,
            Self::OpenAi
                | Self::Anthropic
                | Self::StabilityAi
                | Self::ElevenLabs
                | Self::Cohere
                | Self::Deepgram
                | Self::Google
                | Self::HuggingFace
        )
    }
}

impl std::fmt::Display for OfferingKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── Capability ──────────────────────────────────────────────────

/// AI capability enum — names describe the output type.
///
/// Output-named: Image, Video, Speech, Music, Embed
/// Action-named: Chat, Transcribe, Translate, Rerank (output is text, action distinguishes)
/// Sense-named: Vision (understanding, not generation)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    // Text
    Chat,       // text conversation
    Think,      // extended reasoning
    Tools,      // function calling / agent workflows
    Translate,  // text + target → translated text

    // Understanding
    Vision,     // image/video + text → text
    Ocr,        // document/image → structured text extraction
    Transcribe, // audio → text

    // Vectors
    Embed,  // text → vector
    Rerank, // query + docs → scored docs

    // Generation
    Image,  // text → image (includes editing with input image)
    Video,  // text → video
    Speech, // text → spoken audio
    Music,  // text → musical audio
}

impl Capability {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Think => "think",
            Self::Tools => "tools",
            Self::Translate => "translate",
            Self::Vision => "vision",
            Self::Ocr => "ocr",
            Self::Transcribe => "transcribe",
            Self::Embed => "embed",
            Self::Rerank => "rerank",
            Self::Image => "image",
            Self::Video => "video",
            Self::Speech => "speech",
            Self::Music => "music",
        }
    }

    /// All known capabilities.
    pub const ALL: &[Self] = &[
        Self::Chat,
        Self::Think,
        Self::Tools,
        Self::Translate,
        Self::Vision,
        Self::Ocr,
        Self::Transcribe,
        Self::Embed,
        Self::Rerank,
        Self::Image,
        Self::Video,
        Self::Speech,
        Self::Music,
    ];
}

impl std::fmt::Display for Capability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── Model FQN (ORCH-0015) ──────────────────────────────────────

/// Error when parsing a Model FQN or ModelFilter string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelFqnError {
    /// Empty input string.
    Empty,
    /// Too many pipe-delimited segments (max 4).
    TooManySegments(usize),
    /// A required segment is blank.
    BlankSegment { position: &'static str },
}

impl fmt::Display for ModelFqnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "empty MFQN string"),
            Self::TooManySegments(n) => write!(f, "MFQN has {n} segments (max 4)"),
            Self::BlankSegment { position } => write!(f, "blank segment at {position}"),
        }
    }
}

impl std::error::Error for ModelFqnError {}

/// Model Fully Qualified Name: `source|locator|model|parameters`.
///
/// Every model instance in the directory is identified by a four-part name.
/// The pipe separator `|` does not appear in model names, location names,
/// or provider names — it is unambiguous.
///
/// Parameters are optional: 3 pipes = with parameters, 2 pipes = without.
///
/// Examples:
/// ```text
/// ollama|stone-azure-pool|qwen3.5:9b|Q4_K_M
/// anthropic|prod|claude-sonnet-4
/// infinity|stone-azure-pool|all-MiniLM-L6-v2
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModelFqn {
    pub source: String,
    pub locator: String,
    pub model: String,
    pub parameters: Option<String>,
}

impl ModelFqn {
    /// Parse a pipe-delimited MFQN string.
    ///
    /// Requires exactly 3 or 4 segments: `source|locator|model[|parameters]`.
    pub fn parse(input: &str) -> Result<Self, ModelFqnError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(ModelFqnError::Empty);
        }

        let parts: Vec<&str> = input.split('|').collect();
        match parts.len() {
            3 => {
                if parts[0].is_empty() {
                    return Err(ModelFqnError::BlankSegment { position: "source" });
                }
                if parts[1].is_empty() {
                    return Err(ModelFqnError::BlankSegment { position: "locator" });
                }
                if parts[2].is_empty() {
                    return Err(ModelFqnError::BlankSegment { position: "model" });
                }
                Ok(Self {
                    source: parts[0].to_string(),
                    locator: parts[1].to_string(),
                    model: parts[2].to_string(),
                    parameters: None,
                })
            }
            4 => {
                if parts[0].is_empty() {
                    return Err(ModelFqnError::BlankSegment { position: "source" });
                }
                if parts[1].is_empty() {
                    return Err(ModelFqnError::BlankSegment { position: "locator" });
                }
                if parts[2].is_empty() {
                    return Err(ModelFqnError::BlankSegment { position: "model" });
                }
                let params = if parts[3].is_empty() {
                    None
                } else {
                    Some(parts[3].to_string())
                };
                Ok(Self {
                    source: parts[0].to_string(),
                    locator: parts[1].to_string(),
                    model: parts[2].to_string(),
                    parameters: params,
                })
            }
            n => Err(ModelFqnError::TooManySegments(n)),
        }
    }

    /// Construct directly (no parsing).
    pub fn new(
        source: impl Into<String>,
        locator: impl Into<String>,
        model: impl Into<String>,
        parameters: Option<String>,
    ) -> Self {
        Self {
            source: source.into(),
            locator: locator.into(),
            model: model.into(),
            parameters,
        }
    }

    /// Canonical pipe-delimited string.
    pub fn fqn(&self) -> String {
        match &self.parameters {
            Some(p) => format!("{}|{}|{}|{}", self.source, self.locator, self.model, p),
            None => format!("{}|{}|{}", self.source, self.locator, self.model),
        }
    }

    /// Whether this FQN refers to a cloud provider instance.
    pub fn is_cloud(&self) -> bool {
        matches!(
            self.source.as_str(),
            "openai" | "anthropic" | "google" | "cohere" | "deepgram"
                | "stability-ai" | "elevenlabs" | "huggingface"
        )
    }

    /// Model identity: `"model"` or `"model|parameters"`.
    /// Used as the key in `ModelDirectory.entries`.
    pub fn model_identity(&self) -> String {
        match &self.parameters {
            Some(p) => format!("{}|{}", self.model, p),
            None => self.model.clone(),
        }
    }

    /// Short display: `"qwen3.5:9b (Q4_K_M)"` or `"claude-sonnet-4"`.
    pub fn display_short(&self) -> String {
        match &self.parameters {
            Some(p) => format!("{} ({})", self.model, p),
            None => self.model.clone(),
        }
    }
}

impl fmt::Display for ModelFqn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.fqn())
    }
}

impl Serialize for ModelFqn {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.fqn())
    }
}

impl<'de> Deserialize<'de> for ModelFqn {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

// ── Skill FQN ────────────────────────────────────────────────

/// Error when parsing a Skill FQN string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillFqnError {
    Empty,
    BlankSegment { position: &'static str },
    WrongSegmentCount(usize),
}

impl fmt::Display for SkillFqnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "empty skill FQN"),
            Self::BlankSegment { position } => write!(f, "blank segment: {position}"),
            Self::WrongSegmentCount(n) => write!(f, "expected 2 or 4 segments, got {n}"),
        }
    }
}

impl std::error::Error for SkillFqnError {}

/// Fully-qualified skill name.
///
/// Two forms:
/// - **Identity** (2 segments): `capability|moniker` — e.g., `image|upscale`
/// - **Instance** (4 segments): `location|provider|capability|moniker` — e.g.,
///   `stone-indigo-nave|comfyui|image|upscale`
///
/// The identity form identifies a skill across the garden.
/// The instance form identifies a specific provider instance that can serve the skill.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SkillFqn {
    /// Stone name (instance form only).
    pub location: Option<String>,
    /// Provider / offering kind (instance form only).
    pub provider: Option<String>,
    /// Capability: "image", "speech", "ocr", etc.
    pub capability: String,
    /// Skill moniker: "upscale", "generate", "transform", etc.
    pub moniker: String,
}

impl SkillFqn {
    /// Construct a skill identity (capability + moniker).
    pub fn identity(capability: impl Into<String>, moniker: impl Into<String>) -> Self {
        Self {
            location: None,
            provider: None,
            capability: capability.into(),
            moniker: moniker.into(),
        }
    }

    /// Construct a skill instance (all 4 segments).
    pub fn instance(
        location: impl Into<String>,
        provider: impl Into<String>,
        capability: impl Into<String>,
        moniker: impl Into<String>,
    ) -> Self {
        Self {
            location: Some(location.into()),
            provider: Some(provider.into()),
            capability: capability.into(),
            moniker: moniker.into(),
        }
    }

    /// Parse a pipe-delimited skill FQN string.
    ///
    /// Accepts 2 segments (identity) or 4 segments (instance).
    pub fn parse(input: &str) -> Result<Self, SkillFqnError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(SkillFqnError::Empty);
        }

        let parts: Vec<&str> = input.split('|').collect();
        match parts.len() {
            2 => {
                if parts[0].is_empty() {
                    return Err(SkillFqnError::BlankSegment { position: "capability" });
                }
                if parts[1].is_empty() {
                    return Err(SkillFqnError::BlankSegment { position: "moniker" });
                }
                Ok(Self::identity(parts[0], parts[1]))
            }
            4 => {
                if parts[0].is_empty() {
                    return Err(SkillFqnError::BlankSegment { position: "location" });
                }
                if parts[1].is_empty() {
                    return Err(SkillFqnError::BlankSegment { position: "provider" });
                }
                if parts[2].is_empty() {
                    return Err(SkillFqnError::BlankSegment { position: "capability" });
                }
                if parts[3].is_empty() {
                    return Err(SkillFqnError::BlankSegment { position: "moniker" });
                }
                Ok(Self::instance(parts[0], parts[1], parts[2], parts[3]))
            }
            n => Err(SkillFqnError::WrongSegmentCount(n)),
        }
    }

    /// Canonical pipe-delimited string.
    pub fn fqn(&self) -> String {
        match (&self.location, &self.provider) {
            (Some(loc), Some(prov)) => {
                format!("{}|{}|{}|{}", loc, prov, self.capability, self.moniker)
            }
            _ => format!("{}|{}", self.capability, self.moniker),
        }
    }

    /// Whether this is an instance FQN (has location + provider).
    pub fn is_instance(&self) -> bool {
        self.location.is_some() && self.provider.is_some()
    }

    /// Extract the identity (capability + moniker), dropping location + provider.
    pub fn to_identity(&self) -> Self {
        Self::identity(&self.capability, &self.moniker)
    }

    /// The dotted name used in skill registries and API paths: "image.upscale".
    pub fn dotted(&self) -> String {
        format!("{}.{}", self.capability, self.moniker)
    }

    /// Parse a dotted name ("image.upscale") into an identity FQN.
    pub fn from_dotted(input: &str) -> Result<Self, SkillFqnError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(SkillFqnError::Empty);
        }
        match input.split_once('.') {
            Some((cap, mon)) if !cap.is_empty() && !mon.is_empty() => {
                Ok(Self::identity(cap, mon))
            }
            _ => Err(SkillFqnError::WrongSegmentCount(1)),
        }
    }
}

impl fmt::Display for SkillFqn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.fqn())
    }
}

impl Serialize for SkillFqn {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.fqn())
    }
}

impl<'de> Deserialize<'de> for SkillFqn {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// Partial FQN for pins and queries. Missing fields match everything.
///
/// Parse by pipe count:
/// - 0 pipes → model only (`"qwen3.5:9b"`)
/// - 1 pipe  → source + model (`"ollama|qwen3.5:9b"`)
/// - 2 pipes → source + locator + model (`"ollama|stone-azure-pool|qwen3.5:9b"`)
/// - 3 pipes → full FQN (exact match)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelFilter {
    pub source: Option<String>,
    pub locator: Option<String>,
    pub model: Option<String>,
    pub parameters: Option<String>,
}

impl ModelFilter {
    /// Parse a partial FQN string into a filter.
    pub fn parse(input: &str) -> Result<Self, ModelFqnError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(ModelFqnError::Empty);
        }

        let parts: Vec<&str> = input.split('|').collect();
        let non_empty = |s: &str| -> Option<String> {
            if s.is_empty() { None } else { Some(s.to_string()) }
        };

        match parts.len() {
            // "qwen3.5:9b" → model only
            1 => Ok(Self {
                source: None,
                locator: None,
                model: non_empty(parts[0]),
                parameters: None,
            }),
            // "ollama|qwen3.5:9b" → source + model
            2 => Ok(Self {
                source: non_empty(parts[0]),
                locator: None,
                model: non_empty(parts[1]),
                parameters: None,
            }),
            // "ollama|stone-azure-pool|qwen3.5:9b" → source + locator + model
            3 => Ok(Self {
                source: non_empty(parts[0]),
                locator: non_empty(parts[1]),
                model: non_empty(parts[2]),
                parameters: None,
            }),
            // "ollama|stone-azure-pool|qwen3.5:9b|Q4_K_M" → exact match
            4 => Ok(Self {
                source: non_empty(parts[0]),
                locator: non_empty(parts[1]),
                model: non_empty(parts[2]),
                parameters: non_empty(parts[3]),
            }),
            n => Err(ModelFqnError::TooManySegments(n)),
        }
    }

    /// Check if a full FQN matches this filter. `None` fields match anything.
    pub fn matches(&self, fqn: &ModelFqn) -> bool {
        if let Some(ref s) = self.source {
            if s != &fqn.source {
                return false;
            }
        }
        if let Some(ref l) = self.locator {
            if l != &fqn.locator {
                return false;
            }
        }
        if let Some(ref m) = self.model {
            if m != &fqn.model {
                return false;
            }
        }
        if let Some(ref p) = self.parameters {
            match &fqn.parameters {
                Some(fp) if fp == p => {}
                _ => return false,
            }
        }
        true
    }

    /// Whether this filter specifies all four fields (exact match, no wildcards).
    pub fn is_exact(&self) -> bool {
        self.source.is_some()
            && self.locator.is_some()
            && self.model.is_some()
            && self.parameters.is_some()
    }
}

impl fmt::Display for ModelFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Reconstruct the shortest unambiguous form
        let s = self.source.as_deref().unwrap_or("");
        let l = self.locator.as_deref().unwrap_or("");
        let m = self.model.as_deref().unwrap_or("");
        let p = self.parameters.as_deref().unwrap_or("");

        if !p.is_empty() {
            write!(f, "{s}|{l}|{m}|{p}")
        } else if !l.is_empty() {
            write!(f, "{s}|{l}|{m}")
        } else if !s.is_empty() {
            write!(f, "{s}|{m}")
        } else {
            write!(f, "{m}")
        }
    }
}

// ── Model Directory (ORCH-0015) ────────────────────────────────

/// Metadata about a model entry in the directory.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelMetadata {
    pub parameter_count: Option<u64>,
    pub parameter_size: Option<String>,
    pub quantization_level: Option<String>,
    pub family: Option<String>,
    pub families: Vec<String>,
    pub format: Option<String>,
    pub size_disk: u64,
    pub vram_bytes: Option<u64>,
    pub context_length: Option<u64>,
}

/// A single model in the directory — may be served by multiple instances.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    /// Model name (e.g., "qwen3.5:9b").
    pub model: String,
    /// Quantization or variant (e.g., "Q4_K_M").
    pub parameters: Option<String>,
    /// Capability tags.
    pub capabilities: Vec<Capability>,
    /// Specialization tags (ocr, reasoning, coding, etc.).
    pub specializations: Vec<String>,
    /// Model metadata.
    pub metadata: ModelMetadata,
    /// All instances that can serve this model.
    pub instances: Vec<ModelFqn>,
}

/// The model directory — single source of truth for what models exist.
///
/// Keyed by `model_identity` (model name + optional parameters).
/// Replaces `AppState.models: HashMap<String, ModelInfo>`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelDirectory {
    entries: HashMap<String, ModelEntry>,
}

impl ModelDirectory {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Upsert a model into the directory. If the model_identity already
    /// exists, the instance FQN is added (deduped). Capabilities and
    /// metadata are merged (union for capabilities, latest wins for metadata).
    pub fn upsert(
        &mut self,
        fqn: ModelFqn,
        capabilities: Vec<Capability>,
        specializations: Vec<String>,
        metadata: ModelMetadata,
    ) {
        let identity = fqn.model_identity();

        let entry = self.entries.entry(identity).or_insert_with(|| ModelEntry {
            model: fqn.model.clone(),
            parameters: fqn.parameters.clone(),
            capabilities: Vec::new(),
            specializations: Vec::new(),
            metadata: ModelMetadata::default(),
            instances: Vec::new(),
        });

        // Add instance FQN if not already present
        if !entry.instances.contains(&fqn) {
            entry.instances.push(fqn);
        }

        // Merge capabilities (union)
        for cap in &capabilities {
            if !entry.capabilities.contains(cap) {
                entry.capabilities.push(*cap);
            }
        }

        // Merge specializations (union)
        for spec in &specializations {
            if !entry.specializations.contains(spec) {
                entry.specializations.push(spec.clone());
            }
        }

        // Update metadata (latest wins for non-default fields)
        if metadata.parameter_count.is_some() {
            entry.metadata.parameter_count = metadata.parameter_count;
        }
        if metadata.parameter_size.is_some() {
            entry.metadata.parameter_size = metadata.parameter_size;
        }
        if metadata.quantization_level.is_some() {
            entry.metadata.quantization_level = metadata.quantization_level;
        }
        if metadata.family.is_some() {
            entry.metadata.family = metadata.family;
        }
        if !metadata.families.is_empty() {
            entry.metadata.families = metadata.families;
        }
        if metadata.format.is_some() {
            entry.metadata.format = metadata.format;
        }
        if metadata.size_disk > 0 {
            entry.metadata.size_disk = metadata.size_disk;
        }
        if metadata.vram_bytes.is_some() {
            entry.metadata.vram_bytes = metadata.vram_bytes;
        }
        if metadata.context_length.is_some() {
            entry.metadata.context_length = metadata.context_length;
        }
    }

    /// Remove all instances from a specific provider (source + locator).
    /// Entries with zero instances remaining are removed.
    pub fn remove_provider(&mut self, source: &str, locator: &str) {
        self.entries.retain(|_, entry| {
            entry
                .instances
                .retain(|fqn| !(fqn.source == source && fqn.locator == locator));
            !entry.instances.is_empty()
        });
    }

    /// Remove a specific FQN from the directory.
    pub fn remove_fqn(&mut self, fqn: &ModelFqn) {
        let identity = fqn.model_identity();
        if let Some(entry) = self.entries.get_mut(&identity) {
            entry.instances.retain(|f| f != fqn);
            if entry.instances.is_empty() {
                self.entries.remove(&identity);
            }
        }
    }

    /// All entries in the directory.
    pub fn entries(&self) -> &HashMap<String, ModelEntry> {
        &self.entries
    }

    /// Find an entry by model identity.
    pub fn get(&self, model_identity: &str) -> Option<&ModelEntry> {
        self.entries.get(model_identity)
    }

    /// All models that have a given capability.
    pub fn models_with_capability(&self, cap: Capability) -> Vec<&ModelEntry> {
        self.entries
            .values()
            .filter(|e| e.capabilities.contains(&cap))
            .collect()
    }

    /// All FQNs matching a filter that also have a given capability.
    pub fn matching_fqns(&self, filter: &ModelFilter, cap: Option<Capability>) -> Vec<&ModelFqn> {
        let mut results = Vec::new();
        for entry in self.entries.values() {
            if let Some(c) = cap {
                if !entry.capabilities.contains(&c) {
                    continue;
                }
            }
            for fqn in &entry.instances {
                if filter.matches(fqn) {
                    results.push(fqn);
                }
            }
        }
        results
    }

    /// Find all entries whose model name matches (for routing by model name).
    pub fn find_by_model_name(&self, model_name: &str) -> Vec<&ModelEntry> {
        self.entries
            .values()
            .filter(|e| e.model == model_name)
            .collect()
    }

    /// Total number of unique model identities.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ── Service Instance (generalized from OllamaInstance) ──────────

/// Whether an instance uses GPU or CPU for inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ComputeType {
    Gpu,
    Cpu,
}

impl std::fmt::Display for ComputeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gpu => write!(f, "gpu"),
            Self::Cpu => write!(f, "cpu"),
        }
    }
}

/// Stone identity (code standard §7 — value object, not flat strings).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stone {
    pub id: String,
    pub name: String,
}

/// GPU identity (code standard §1 — namespace, not prefix).
#[derive(Debug, Clone)]
pub struct Gpu {
    pub name: Option<String>,
    pub compute: ComputeType,
}

/// VRAM state (code standard §1 — namespace, not prefix).
#[derive(Debug, Clone)]
pub struct Vram {
    pub total_bytes: u64,
    pub budget_bytes: u64,
    /// Real-time VRAM free bytes (ComfyUI provides this; Ollama does not).
    pub free_bytes: Option<u64>,
}

/// A discovered AI service instance and its hardware profile.
///
/// Replaces `OllamaInstance`. Field naming follows code standards:
/// struct nesting for namespaces (§1), no type-in-name (§2).
#[derive(Debug, Clone)]
pub struct ServiceInstance {
    // Identity
    pub stone: Stone,
    pub endpoint: String,
    pub kind: OfferingKind,

    // Hardware
    pub gpu: Gpu,
    pub vram: Vram,

    // Service state
    pub health: InstanceHealth,
    pub models_available: Vec<String>,
    pub models_loaded: Vec<LoadedModel>,
    pub capabilities: Vec<Capability>,
    pub queue_depth: u32,
    pub last_seen: Instant,

    // Offering-specific metadata (opaque to routing)
    pub metadata: serde_json::Value,

    // Priority (0 = default, -10 = cloud, +10 = pinned)
    pub priority: i32,
}

impl ServiceInstance {
    /// Whether this instance participates in routing.
    pub fn is_routable(&self) -> bool {
        self.health.is_routable()
    }
}

/// Instance health as observed by the orchestrator.
#[derive(Debug, Clone, PartialEq)]
pub enum InstanceHealth {
    /// Just discovered, profiling in progress.
    Profiling,
    /// Responding normally.
    Healthy,
    /// Unreachable or erroring — removed from routing pool.
    Unhealthy { since: Instant, reason: String },
}

impl InstanceHealth {
    pub fn is_routable(&self) -> bool {
        matches!(self, Self::Healthy)
    }
}

/// A model currently loaded in VRAM on an instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedModel {
    pub name: String,
    /// Exact VRAM consumption (bytes).
    pub size_vram: u64,
    /// When the service will auto-unload (ISO-8601, offering-specific).
    pub expires_at: Option<String>,
}

// ── Tiers ────────────────────────────────────────────────────────

/// A VRAM capacity tier — emergent from discovered hardware.
#[derive(Debug, Clone)]
pub struct Tier {
    pub vram_bytes: u64,
    pub label: String,
    pub instance_endpoints: Vec<String>,
}

// ── Lease ────────────────────────────────────────────────────────

/// Lease-on-demand reservation for a high-tier instance.
#[derive(Debug, Clone)]
pub struct Lease {
    pub instance_endpoint: String,
    pub model_name: String,
    pub granted_at: Instant,
    pub duration: std::time::Duration,
}

impl Lease {
    pub fn is_expired(&self) -> bool {
        self.granted_at.elapsed() > self.duration
    }

    pub fn extend(&mut self) {
        let new_dur = self
            .duration
            .mul_f64(1.25)
            .min(std::time::Duration::from_secs(300));
        self.duration = new_dur;
        self.granted_at = Instant::now();
    }
}

// ── Routing Decision ─────────────────────────────────────────────

/// The result of a routing decision.
#[derive(Debug, Clone)]
pub struct RoutingDecision {
    pub target_endpoint: String,
    pub stone_name: String,
    pub model_name: String,
    pub tier_label: Option<String>,
    pub offering_kind: OfferingKind,
    pub was_overflow: bool,
    pub lease_acquired: bool,
}

/// Why a routing decision failed.
#[derive(Debug, Clone)]
pub enum RoutingError {
    ModelNotFound(String),
    ModelBlocked(String),
    AllInstancesBusy { model: String },
    NoHealthyInstances,
    /// No offering serves this capability.
    CapabilityUnavailable(Capability),
}

impl std::fmt::Display for RoutingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ModelNotFound(m) => write!(f, "model '{m}' not found in any instance"),
            Self::ModelBlocked(m) => {
                write!(f, "model '{m}' is blocked on all available stones (benchmark errors)")
            }
            Self::AllInstancesBusy { model } => write!(f, "all instances busy for '{model}'"),
            Self::NoHealthyInstances => write!(f, "no healthy instances"),
            Self::CapabilityUnavailable(c) => write!(f, "no offering serves capability '{c}'"),
        }
    }
}

// ── Auto-Pull Mode ──────────────────────────────────────────────

/// Three-way auto-pull policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoPullMode {
    Off,
    #[default]
    Sync,
    OnDemand,
}

impl std::fmt::Display for AutoPullMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Off => write!(f, "off"),
            Self::Sync => write!(f, "sync"),
            Self::OnDemand => write!(f, "on_demand"),
        }
    }
}

// ── Orchestrator Jobs ───────────────────────────────────────────

/// What kind of work a job performs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    ModelPull { model: String, targets: Vec<String> },
    ModelDelete { model: String, targets: Vec<String> },
    ModelSync { model: String, targets: Vec<String> },
    ResourceSync { resource: String, offering: String, targets: Vec<String> },
    InstanceProfile { endpoint: String, stone_name: String },
    OnDemandPull { model: String },
    Benchmark { scope: String, stones: Vec<String> },
}

impl JobKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::ModelPull { .. } => "pull",
            Self::ModelDelete { .. } => "delete",
            Self::ModelSync { .. } => "sync",
            Self::ResourceSync { .. } => "resource-sync",
            Self::InstanceProfile { .. } => "profile",
            Self::OnDemandPull { .. } => "on-demand",
            Self::Benchmark { .. } => "benchmark",
        }
    }

    pub fn subject(&self) -> &str {
        match self {
            Self::ModelPull { model, .. } => model,
            Self::ModelDelete { model, .. } => model,
            Self::ModelSync { model, .. } => model,
            Self::ResourceSync { resource, .. } => resource,
            Self::InstanceProfile { stone_name, .. } => stone_name,
            Self::OnDemandPull { model, .. } => model,
            Self::Benchmark { scope, .. } => scope,
        }
    }
}

/// Current status of a job.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed,
}

/// A tracked orchestrator job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorJob {
    pub id: String,
    pub kind: JobKind,
    pub status: JobStatus,
    pub progress: Option<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

// ── Configuration ────────────────────────────────────────────────

/// Orchestrator configuration persisted as TOML.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrchestratorConfig {
    #[serde(default)]
    pub features: FeatureConfig,
    #[serde(default)]
    pub stones: HashMap<String, StoneConfig>,
    /// Per-offering proxy enable/disable overrides.
    #[serde(default)]
    pub proxies: HashMap<String, bool>,
    /// Default inference parameters per capability.
    /// The proxy injects these as defaults when the client doesn't specify them.
    /// Key is the capability name ("chat", "embed", "think", etc.).
    #[serde(default)]
    pub defaults: HashMap<String, InferenceDefaults>,
}

/// Default inference parameters for a capability.
/// Only non-None fields are injected into requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceDefaults {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureConfig {
    #[serde(default)]
    pub auto_pull_mode: AutoPullMode,
    #[serde(default)]
    pub delete_on_idle: bool,
    #[serde(default = "default_true")]
    pub metrics_enabled: bool,
    /// User-pinned model overrides per capability.
    #[serde(default)]
    pub pins: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoneConfig {
    pub vram_budget_mb: Option<u64>,
}

fn default_true() -> bool {
    true
}

impl Default for FeatureConfig {
    fn default() -> Self {
        Self {
            auto_pull_mode: AutoPullMode::default(),
            delete_on_idle: false,
            metrics_enabled: true,
            pins: HashMap::new(),
        }
    }
}

// ── Metrics ──────────────────────────────────────────────────────

/// Per-stone counters.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoneMetrics {
    pub requests: u64,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub errors: u64,
    pub total_duration_ns: u64,
    pub eval_duration_ns: u64,
}

/// Serializable metrics snapshot (persisted as JSON).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub requests_total: u64,
    pub tokens_in_total: u64,
    pub tokens_out_total: u64,
    pub errors_total: u64,
    pub per_stone: HashMap<String, StoneMetrics>,
    pub per_model: HashMap<String, u64>,
    pub started_at: Option<String>,
    pub snapshot_at: Option<String>,
}

// ── Metric Events ───────────────────────────────────────────────

/// Metric event sent from the proxy to the metrics processing task.
#[derive(Debug, Clone)]
pub enum MetricEvent {
    Request {
        stone: String,
        model: String,
        capability: Capability,
        tokens_in: u64,
        tokens_out: u64,
        duration_ns: u64,
        eval_duration_ns: u64,
    },
    Error {
        stone: String,
        model: Option<String>,
        status_code: Option<u16>,
        reason: Option<String>,
    },
}

// ── Placement ────────────────────────────────────────────────────

/// Demand-weighted placement plan: ideal model→stone assignment.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlacementPlan {
    pub assignments: HashMap<String, Vec<String>>,
    pub computed_at: Option<String>,
    pub stable: bool,
}

// ── Cross-Offering VRAM Accounting ──────────────────────────────

/// Aggregate VRAM state for one stone across all offerings.
#[derive(Debug, Clone)]
pub struct StoneVramBudget {
    pub stone_id: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub per_offering: Vec<OfferingVramUsage>,
}

#[derive(Debug, Clone)]
pub struct OfferingVramUsage {
    pub kind: OfferingKind,
    pub used_bytes: u64,
    pub model_count: usize,
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── ModelFqn ────────────────────────────────────────────────

    #[test]
    fn parse_full_fqn_with_params() {
        let fqn = ModelFqn::parse("ollama|stone-azure-pool|qwen3.5:9b|Q4_K_M").unwrap();
        assert_eq!(fqn.source, "ollama");
        assert_eq!(fqn.locator, "stone-azure-pool");
        assert_eq!(fqn.model, "qwen3.5:9b");
        assert_eq!(fqn.parameters.as_deref(), Some("Q4_K_M"));
    }

    #[test]
    fn parse_fqn_without_params() {
        let fqn = ModelFqn::parse("anthropic|prod|claude-sonnet-4").unwrap();
        assert_eq!(fqn.source, "anthropic");
        assert_eq!(fqn.locator, "prod");
        assert_eq!(fqn.model, "claude-sonnet-4");
        assert_eq!(fqn.parameters, None);
    }

    #[test]
    fn parse_fqn_empty_params_treated_as_none() {
        let fqn = ModelFqn::parse("ollama|s1|m1|").unwrap();
        assert_eq!(fqn.parameters, None);
    }

    #[test]
    fn parse_fqn_rejects_empty() {
        assert_eq!(ModelFqn::parse(""), Err(ModelFqnError::Empty));
    }

    #[test]
    fn parse_fqn_rejects_too_few_segments() {
        // 1 segment = too few for FQN (valid as filter, not as FQN)
        assert!(ModelFqn::parse("just-a-model").is_err());
    }

    #[test]
    fn parse_fqn_rejects_blank_segment() {
        assert_eq!(
            ModelFqn::parse("|locator|model"),
            Err(ModelFqnError::BlankSegment { position: "source" })
        );
        assert_eq!(
            ModelFqn::parse("source||model"),
            Err(ModelFqnError::BlankSegment { position: "locator" })
        );
    }

    #[test]
    fn fqn_roundtrip() {
        let fqn = ModelFqn::new("ollama", "s1", "qwen3.5:9b", Some("Q4_K_M".into()));
        assert_eq!(fqn.fqn(), "ollama|s1|qwen3.5:9b|Q4_K_M");
        assert_eq!(ModelFqn::parse(&fqn.fqn()).unwrap(), fqn);
    }

    #[test]
    fn fqn_model_identity() {
        let with_params = ModelFqn::new("ollama", "s1", "m1", Some("Q4".into()));
        assert_eq!(with_params.model_identity(), "m1|Q4");

        let without = ModelFqn::new("anthropic", "prod", "claude-sonnet-4", None);
        assert_eq!(without.model_identity(), "claude-sonnet-4");
    }

    #[test]
    fn fqn_is_cloud() {
        assert!(ModelFqn::new("anthropic", "prod", "m", None).is_cloud());
        assert!(ModelFqn::new("google", "personal", "m", None).is_cloud());
        assert!(!ModelFqn::new("ollama", "s1", "m", None).is_cloud());
        assert!(!ModelFqn::new("infinity", "s1", "m", None).is_cloud());
    }

    #[test]
    fn fqn_serde_roundtrip() {
        let fqn = ModelFqn::new("ollama", "s1", "qwen3.5:9b", Some("Q4_K_M".into()));
        let json = serde_json::to_string(&fqn).unwrap();
        assert_eq!(json, r#""ollama|s1|qwen3.5:9b|Q4_K_M""#);
        let parsed: ModelFqn = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, fqn);
    }

    // ── ModelFilter ────────────────────────────────────────────

    #[test]
    fn filter_model_only() {
        let f = ModelFilter::parse("qwen3.5:9b").unwrap();
        assert_eq!(f.source, None);
        assert_eq!(f.locator, None);
        assert_eq!(f.model.as_deref(), Some("qwen3.5:9b"));
        assert_eq!(f.parameters, None);
    }

    #[test]
    fn filter_source_and_model() {
        let f = ModelFilter::parse("ollama|qwen3.5:9b").unwrap();
        assert_eq!(f.source.as_deref(), Some("ollama"));
        assert_eq!(f.model.as_deref(), Some("qwen3.5:9b"));
    }

    #[test]
    fn filter_full() {
        let f = ModelFilter::parse("ollama|s1|m1|Q4").unwrap();
        assert!(f.is_exact());
    }

    #[test]
    fn filter_matches_model_only() {
        let f = ModelFilter::parse("qwen3.5:9b").unwrap();
        let fqn1 = ModelFqn::new("ollama", "s1", "qwen3.5:9b", Some("Q4".into()));
        let fqn2 = ModelFqn::new("ollama", "s2", "qwen3.5:9b", None);
        let fqn3 = ModelFqn::new("anthropic", "prod", "claude-sonnet-4", None);

        assert!(f.matches(&fqn1));
        assert!(f.matches(&fqn2));
        assert!(!f.matches(&fqn3));
    }

    #[test]
    fn filter_matches_source_and_model() {
        let f = ModelFilter::parse("ollama|qwen3.5:9b").unwrap();
        let fqn_ollama = ModelFqn::new("ollama", "s1", "qwen3.5:9b", None);
        let fqn_other = ModelFqn::new("infinity", "s1", "qwen3.5:9b", None);

        assert!(f.matches(&fqn_ollama));
        assert!(!f.matches(&fqn_other));
    }

    #[test]
    fn filter_matches_exact() {
        let f = ModelFilter::parse("ollama|s1|m1|Q4").unwrap();
        let exact = ModelFqn::new("ollama", "s1", "m1", Some("Q4".into()));
        let wrong_loc = ModelFqn::new("ollama", "s2", "m1", Some("Q4".into()));
        let no_params = ModelFqn::new("ollama", "s1", "m1", None);

        assert!(f.matches(&exact));
        assert!(!f.matches(&wrong_loc));
        assert!(!f.matches(&no_params));
    }

    // ── ModelDirectory ─────────────────────────────────────────

    fn make_fqn(source: &str, loc: &str, model: &str) -> ModelFqn {
        ModelFqn::new(source, loc, model, None)
    }

    #[test]
    fn directory_upsert_and_find() {
        let mut dir = ModelDirectory::new();
        let fqn1 = make_fqn("ollama", "s1", "qwen3.5:9b");
        let fqn2 = make_fqn("ollama", "s2", "qwen3.5:9b");

        dir.upsert(
            fqn1.clone(),
            vec![Capability::Chat, Capability::Vision],
            vec![],
            ModelMetadata::default(),
        );
        dir.upsert(
            fqn2.clone(),
            vec![Capability::Chat],
            vec![],
            ModelMetadata::default(),
        );

        assert_eq!(dir.len(), 1); // same model_identity
        let entry = dir.get("qwen3.5:9b").unwrap();
        assert_eq!(entry.instances.len(), 2);
        assert_eq!(entry.capabilities.len(), 2); // Chat + Vision (union)
    }

    #[test]
    fn directory_models_with_capability() {
        let mut dir = ModelDirectory::new();
        dir.upsert(
            make_fqn("ollama", "s1", "qwen3.5:9b"),
            vec![Capability::Chat],
            vec![],
            ModelMetadata::default(),
        );
        dir.upsert(
            make_fqn("infinity", "s1", "all-MiniLM-L6-v2"),
            vec![Capability::Embed],
            vec![],
            ModelMetadata::default(),
        );

        let chat_models = dir.models_with_capability(Capability::Chat);
        assert_eq!(chat_models.len(), 1);
        assert_eq!(chat_models[0].model, "qwen3.5:9b");

        let embed_models = dir.models_with_capability(Capability::Embed);
        assert_eq!(embed_models.len(), 1);
    }

    #[test]
    fn directory_remove_provider() {
        let mut dir = ModelDirectory::new();
        dir.upsert(
            make_fqn("ollama", "s1", "m1"),
            vec![Capability::Chat],
            vec![],
            ModelMetadata::default(),
        );
        dir.upsert(
            make_fqn("ollama", "s2", "m1"),
            vec![Capability::Chat],
            vec![],
            ModelMetadata::default(),
        );

        dir.remove_provider("ollama", "s1");
        let entry = dir.get("m1").unwrap();
        assert_eq!(entry.instances.len(), 1);
        assert_eq!(entry.instances[0].locator, "s2");
    }

    #[test]
    fn directory_remove_last_instance_removes_entry() {
        let mut dir = ModelDirectory::new();
        dir.upsert(
            make_fqn("ollama", "s1", "m1"),
            vec![Capability::Chat],
            vec![],
            ModelMetadata::default(),
        );

        dir.remove_provider("ollama", "s1");
        assert!(dir.is_empty());
    }

    #[test]
    fn directory_matching_fqns_with_filter() {
        let mut dir = ModelDirectory::new();
        let fqn1 = make_fqn("ollama", "s1", "qwen3.5:9b");
        let fqn2 = make_fqn("ollama", "s2", "qwen3.5:9b");
        let fqn3 = make_fqn("anthropic", "prod", "claude-sonnet-4");

        dir.upsert(fqn1, vec![Capability::Chat], vec![], ModelMetadata::default());
        dir.upsert(fqn2, vec![Capability::Chat], vec![], ModelMetadata::default());
        dir.upsert(fqn3, vec![Capability::Chat], vec![], ModelMetadata::default());

        let filter = ModelFilter::parse("qwen3.5:9b").unwrap();
        let matches = dir.matching_fqns(&filter, Some(Capability::Chat));
        assert_eq!(matches.len(), 2);

        let filter2 = ModelFilter::parse("ollama|qwen3.5:9b").unwrap();
        let matches2 = dir.matching_fqns(&filter2, None);
        assert_eq!(matches2.len(), 2);
    }

    // ── SkillFqn ──────────────────────────────────────────────

    #[test]
    fn skill_fqn_identity() {
        let fqn = SkillFqn::identity("image", "upscale");
        assert_eq!(fqn.capability, "image");
        assert_eq!(fqn.moniker, "upscale");
        assert!(!fqn.is_instance());
        assert_eq!(fqn.fqn(), "image|upscale");
        assert_eq!(fqn.dotted(), "image.upscale");
    }

    #[test]
    fn skill_fqn_instance() {
        let fqn = SkillFqn::instance("stone-indigo-nave", "comfyui", "image", "upscale");
        assert!(fqn.is_instance());
        assert_eq!(fqn.fqn(), "stone-indigo-nave|comfyui|image|upscale");
        assert_eq!(fqn.dotted(), "image.upscale");
        assert_eq!(fqn.to_identity().fqn(), "image|upscale");
    }

    #[test]
    fn skill_fqn_parse_identity() {
        let fqn = SkillFqn::parse("image|upscale").unwrap();
        assert_eq!(fqn.capability, "image");
        assert_eq!(fqn.moniker, "upscale");
        assert!(!fqn.is_instance());
    }

    #[test]
    fn skill_fqn_parse_instance() {
        let fqn = SkillFqn::parse("stone-indigo-nave|comfyui|image|generate").unwrap();
        assert_eq!(fqn.location.as_deref(), Some("stone-indigo-nave"));
        assert_eq!(fqn.provider.as_deref(), Some("comfyui"));
        assert_eq!(fqn.capability, "image");
        assert_eq!(fqn.moniker, "generate");
    }

    #[test]
    fn skill_fqn_parse_rejects_empty() {
        assert_eq!(SkillFqn::parse(""), Err(SkillFqnError::Empty));
    }

    #[test]
    fn skill_fqn_parse_rejects_wrong_count() {
        assert!(matches!(SkillFqn::parse("a|b|c"), Err(SkillFqnError::WrongSegmentCount(3))));
        assert!(matches!(SkillFqn::parse("a"), Err(SkillFqnError::WrongSegmentCount(1))));
    }

    #[test]
    fn skill_fqn_parse_rejects_blank_segment() {
        assert!(matches!(SkillFqn::parse("|upscale"), Err(SkillFqnError::BlankSegment { position: "capability" })));
        assert!(matches!(SkillFqn::parse("image|"), Err(SkillFqnError::BlankSegment { position: "moniker" })));
    }

    #[test]
    fn skill_fqn_from_dotted() {
        let fqn = SkillFqn::from_dotted("image.upscale").unwrap();
        assert_eq!(fqn.capability, "image");
        assert_eq!(fqn.moniker, "upscale");
        assert!(!fqn.is_instance());
    }

    #[test]
    fn skill_fqn_from_dotted_rejects_invalid() {
        assert!(SkillFqn::from_dotted("").is_err());
        assert!(SkillFqn::from_dotted("noDot").is_err());
        assert!(SkillFqn::from_dotted(".upscale").is_err());
        assert!(SkillFqn::from_dotted("image.").is_err());
    }

    #[test]
    fn skill_fqn_serde_roundtrip() {
        let fqn = SkillFqn::instance("stone-indigo-nave", "comfyui", "image", "upscale");
        let json = serde_json::to_string(&fqn).unwrap();
        assert_eq!(json, "\"stone-indigo-nave|comfyui|image|upscale\"");

        let parsed: SkillFqn = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, fqn);
    }

    #[test]
    fn skill_fqn_display() {
        let identity = SkillFqn::identity("speech", "clone_voice");
        assert_eq!(format!("{identity}"), "speech|clone_voice");

        let instance = SkillFqn::instance("stone-azure-pool", "speaches", "speech", "synthesize");
        assert_eq!(format!("{instance}"), "stone-azure-pool|speaches|speech|synthesize");
    }
}
