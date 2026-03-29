//! Cloud provider offering adapters — bounded context for cloud API access.
//!
//! Cloud providers (OpenAI, Anthropic, Groq, Together, etc.) implement
//! the `Offering` trait just like local offerings, but with:
//! - `DiscoveryConfig::Configured` (not auto-discovered)
//! - `priority: -10` (cloud fallback behind local instances)
//! - No proxy port (traffic routes through existing proxy ports)
//! - API key stored in `CloudProviderConfig`
//!
//! The `CloudProviderStore` manages provider configs and persists them
//! to `{data_dir}/providers.json`.

pub mod anthropic;
pub mod openai;
pub mod types;

pub use anthropic::AnthropicProvider;
pub use openai::OpenAiProvider;
pub use types::{CloudProviderConfig, CloudProviderStore};
