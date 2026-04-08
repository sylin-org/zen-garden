//! Canonical provider names. Adapters construct their
//! [`crate::domain::ids::ProviderName`] from these constants.
//!
//! Names are lowercase ASCII snake-case, stable over the lifetime of
//! the project. Renaming a provider is a breaking change.

pub const OLLAMA: &str = "ollama";
pub const ANTHROPIC: &str = "anthropic";
pub const OPENAI: &str = "openai";
pub const GOOGLE: &str = "google";
pub const LIBRETRANSLATE: &str = "libretranslate";
pub const INFINITY: &str = "infinity";
pub const DOCLING: &str = "docling";
pub const COMFYUI: &str = "comfyui";
pub const KOKORO: &str = "kokoro";
pub const OPENEDAI_SPEECH: &str = "openedai_speech";
pub const WHISPERCPP: &str = "whispercpp";
pub const SPEACHES: &str = "speaches";

/// All provider name constants, in declaration order.
pub const ALL: &[&str] = &[
    OLLAMA,
    ANTHROPIC,
    OPENAI,
    GOOGLE,
    LIBRETRANSLATE,
    INFINITY,
    DOCLING,
    COMFYUI,
    KOKORO,
    OPENEDAI_SPEECH,
    WHISPERCPP,
    SPEACHES,
];
