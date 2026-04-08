//! The closed enum of orchestrator primitives.
//!
//! ORCH-0028 locks the v1 inventory at ten primitives across three
//! modalities. Adding a primitive requires an ADR amendment.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// A locked, closed enum of every primitive the orchestrator serves.
///
/// The dotted form (`text.chat`, `image.generate`, ...) is the canonical
/// identifier used in URLs, logs, and [`crate::domain::request::Action`].
///
/// Serialization emits the dotted form. Deserialization accepts both the
/// canonical dotted form *and* the legacy snake_case form (`text_chat`)
/// so existing on-disk job files written by earlier builds continue to
/// load.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Primitive {
    // ── Text ─────────────────────────────────────────────
    TextChat,
    TextTranslate,
    TextEmbed,
    TextRerank,
    // ── Image ────────────────────────────────────────────
    ImageGenerate,
    ImageEdit,
    ImageUpscale,
    ImageAnalyze,
    // ── Audio ────────────────────────────────────────────
    AudioGenerate,
    AudioTranscribe,
}

/// The three top-level modality namespaces recognized in v1. Video is
/// reserved (see ADR) and is not part of the enum until a video provider
/// exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modality {
    Text,
    Image,
    Audio,
}

impl Modality {
    pub const fn as_str(self) -> &'static str {
        match self {
            Modality::Text => "text",
            Modality::Image => "image",
            Modality::Audio => "audio",
        }
    }
}

impl fmt::Display for Modality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Primitive {
    /// All primitives, in declaration order. Used by the catalog builder
    /// and CI guards to enumerate the set.
    pub const ALL: &'static [Primitive] = &[
        Primitive::TextChat,
        Primitive::TextTranslate,
        Primitive::TextEmbed,
        Primitive::TextRerank,
        Primitive::ImageGenerate,
        Primitive::ImageEdit,
        Primitive::ImageUpscale,
        Primitive::ImageAnalyze,
        Primitive::AudioGenerate,
        Primitive::AudioTranscribe,
    ];

    /// The modality this primitive operates within.
    pub const fn modality(self) -> Modality {
        match self {
            Primitive::TextChat
            | Primitive::TextTranslate
            | Primitive::TextEmbed
            | Primitive::TextRerank => Modality::Text,
            Primitive::ImageGenerate
            | Primitive::ImageEdit
            | Primitive::ImageUpscale
            | Primitive::ImageAnalyze => Modality::Image,
            Primitive::AudioGenerate | Primitive::AudioTranscribe => Modality::Audio,
        }
    }

    /// The canonical dotted identifier used in action strings and URLs.
    pub const fn dotted(self) -> &'static str {
        match self {
            Primitive::TextChat => "text.chat",
            Primitive::TextTranslate => "text.translate",
            Primitive::TextEmbed => "text.embed",
            Primitive::TextRerank => "text.rerank",
            Primitive::ImageGenerate => "image.generate",
            Primitive::ImageEdit => "image.edit",
            Primitive::ImageUpscale => "image.upscale",
            Primitive::ImageAnalyze => "image.analyze",
            Primitive::AudioGenerate => "audio.generate",
            Primitive::AudioTranscribe => "audio.transcribe",
        }
    }

    /// The segment after the modality (e.g., `"chat"` in `"text.chat"`).
    pub const fn leaf(self) -> &'static str {
        match self {
            Primitive::TextChat => "chat",
            Primitive::TextTranslate => "translate",
            Primitive::TextEmbed => "embed",
            Primitive::TextRerank => "rerank",
            Primitive::ImageGenerate => "generate",
            Primitive::ImageEdit => "edit",
            Primitive::ImageUpscale => "upscale",
            Primitive::ImageAnalyze => "analyze",
            Primitive::AudioGenerate => "generate",
            Primitive::AudioTranscribe => "transcribe",
        }
    }

    /// Short human-readable summary suitable for catalog index listings.
    pub const fn summary(self) -> &'static str {
        match self {
            Primitive::TextChat => "Conversational text completion with optional tool calling.",
            Primitive::TextTranslate => "Translate text from one language to another.",
            Primitive::TextEmbed => "Produce an embedding vector for one or more text passages.",
            Primitive::TextRerank => "Score and order candidate documents against a query.",
            Primitive::ImageGenerate => "Generate an image from a text prompt.",
            Primitive::ImageEdit => "Edit an existing image guided by a prompt and optional mask.",
            Primitive::ImageUpscale => "Increase the resolution of an image.",
            Primitive::ImageAnalyze => "Describe or answer questions about the contents of an image.",
            Primitive::AudioGenerate => "Synthesize speech or audio from text.",
            Primitive::AudioTranscribe => "Transcribe audio into text.",
        }
    }

    /// Parse a dotted action string into a primitive.
    pub fn parse_dotted(s: &str) -> Result<Self, PrimitiveError> {
        for p in Self::ALL {
            if p.dotted() == s {
                return Ok(*p);
            }
        }
        Err(PrimitiveError::Unknown(s.to_string()))
    }

    /// Parse a modality and leaf into a primitive.
    pub fn from_segments(modality: &str, leaf: &str) -> Result<Self, PrimitiveError> {
        let dotted = format!("{}.{}", modality, leaf);
        Self::parse_dotted(&dotted)
    }
}

impl fmt::Display for Primitive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.dotted())
    }
}

impl FromStr for Primitive {
    type Err = PrimitiveError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Accept either the canonical dotted form or the legacy
        // snake_case form written by older builds.
        if let Ok(p) = Self::parse_dotted(s) {
            return Ok(p);
        }
        for p in Self::ALL {
            let snake = p.dotted().replace('.', "_");
            if snake == s {
                return Ok(*p);
            }
        }
        Err(PrimitiveError::Unknown(s.to_string()))
    }
}

impl Serialize for Primitive {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.dotted())
    }
}

impl<'de> Deserialize<'de> for Primitive {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PrimitiveError {
    #[error("unknown primitive `{0}`")]
    Unknown(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_primitives_have_distinct_dotted_identifiers() {
        let mut seen = std::collections::HashSet::new();
        for p in Primitive::ALL {
            assert!(
                seen.insert(p.dotted()),
                "duplicate dotted id: {}",
                p.dotted()
            );
        }
        assert_eq!(Primitive::ALL.len(), 10);
    }

    #[test]
    fn modality_mapping_is_correct() {
        assert_eq!(Primitive::TextChat.modality(), Modality::Text);
        assert_eq!(Primitive::ImageGenerate.modality(), Modality::Image);
        assert_eq!(Primitive::AudioGenerate.modality(), Modality::Audio);
    }

    #[test]
    fn dotted_is_modality_dot_leaf() {
        for p in Primitive::ALL {
            assert_eq!(p.dotted(), format!("{}.{}", p.modality(), p.leaf()));
        }
    }

    #[test]
    fn parse_dotted_roundtrip() {
        for p in Primitive::ALL {
            assert_eq!(Primitive::parse_dotted(p.dotted()).unwrap(), *p);
        }
    }

    #[test]
    fn parse_dotted_rejects_unknown() {
        assert!(Primitive::parse_dotted("text.unknown").is_err());
        assert!(Primitive::parse_dotted("video.generate").is_err());
    }

    #[test]
    fn from_segments_rejects_bad_modality() {
        assert!(Primitive::from_segments("video", "generate").is_err());
    }
}
