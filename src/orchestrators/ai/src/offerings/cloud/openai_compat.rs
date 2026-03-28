//! OpenAI-compatible cloud provider factory.
//!
//! Creates `CloudProviderOffering` instances for providers whose APIs
//! are OpenAI-compatible (or close enough that the standard adapter works).
//!
//! ## Supported Providers
//!
//! | Provider | Base URL | API Key Env | Capabilities |
//! |----------|----------|-------------|-------------|
//! | OpenAI | `https://api.openai.com/v1` | `OPENAI_API_KEY` | Chat, Embed, Imagine, Transcribe, Speak |
//! | Anthropic | `https://api.anthropic.com/v1` | `ANTHROPIC_API_KEY` | Chat |
//! | Google | `https://generativelanguage.googleapis.com/v1beta` | `GOOGLE_API_KEY` | Chat, Embed, Imagine |
//! | Cohere | `https://api.cohere.com/v2` | `COHERE_API_KEY` | Chat, Embed, Rerank |
//! | Deepgram | `https://api.deepgram.com/v1` | `DEEPGRAM_API_KEY` | Transcribe, Speak |
//!
//! Providers with no API key in the environment are silently skipped.

use std::sync::Arc;

use super::CloudProviderOffering;
use crate::catalog::Offering;
use crate::domain::types::{Capability, OfferingKind};

/// Provider configuration entry.
struct ProviderDef {
    kind: OfferingKind,
    base_url: &'static str,
    api_key_env: &'static str,
    capabilities: &'static [Capability],
}

/// All known OpenAI-compatible cloud providers.
const PROVIDERS: &[ProviderDef] = &[
    ProviderDef {
        kind: OfferingKind::OpenAi,
        base_url: "https://api.openai.com/v1",
        api_key_env: "OPENAI_API_KEY",
        capabilities: &[
            Capability::Chat,
            Capability::Embed,
            Capability::Imagine,
            Capability::Transcribe,
            Capability::Speak,
        ],
    },
    ProviderDef {
        kind: OfferingKind::Anthropic,
        base_url: "https://api.anthropic.com/v1",
        api_key_env: "ANTHROPIC_API_KEY",
        capabilities: &[Capability::Chat],
    },
    ProviderDef {
        kind: OfferingKind::Google,
        base_url: "https://generativelanguage.googleapis.com/v1beta",
        api_key_env: "GOOGLE_API_KEY",
        capabilities: &[Capability::Chat, Capability::Embed, Capability::Imagine],
    },
    ProviderDef {
        kind: OfferingKind::Cohere,
        base_url: "https://api.cohere.com/v2",
        api_key_env: "COHERE_API_KEY",
        capabilities: &[Capability::Chat, Capability::Embed, Capability::Rerank],
    },
    ProviderDef {
        kind: OfferingKind::Deepgram,
        base_url: "https://api.deepgram.com/v1",
        api_key_env: "DEEPGRAM_API_KEY",
        capabilities: &[Capability::Transcribe, Capability::Speak],
    },
];

/// Register all cloud providers that have API keys configured.
///
/// Returns a list of `Arc<dyn Offering>` for providers whose env vars are set.
/// Providers without API keys are silently skipped.
pub fn register_cloud_providers() -> Vec<Arc<dyn Offering>> {
    let mut providers = Vec::new();

    // Anthropic gets its own adapter with Messages API translation.
    if let Some(anthropic) = super::anthropic::AnthropicOffering::from_env() {
        tracing::info!("cloud provider registered: Anthropic (Messages API)");
        providers.push(Arc::new(anthropic) as Arc<dyn Offering>);
    }

    // All other providers use the generic OpenAI-compatible adapter.
    for def in PROVIDERS {
        // Skip Anthropic — handled above with dedicated adapter.
        if def.kind == OfferingKind::Anthropic {
            continue;
        }

        if let Some(offering) = CloudProviderOffering::from_env(
            def.kind,
            def.capabilities.to_vec(),
            def.base_url,
            def.api_key_env,
        ) {
            tracing::info!(
                provider = ?def.kind,
                api_key_env = def.api_key_env,
                capabilities = def.capabilities.len(),
                "cloud provider registered"
            );
            providers.push(Arc::new(offering) as Arc<dyn Offering>);
        } else {
            tracing::debug!(
                provider = ?def.kind,
                api_key_env = def.api_key_env,
                "cloud provider skipped (no API key)"
            );
        }
    }

    providers
}
