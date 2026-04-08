//! Shared helpers for cloud adapters that resolve models against a
//! static supported list (Google/Gemini in M1; Anthropic and OpenAI
//! when they return in M2).
//!
//! ORCH-0030 R2 M3 deletes the recommendation engine and moves
//! model resolution into each adapter's `onboard`. Adapters with
//! instance pools (Ollama) own their own scoring matrix; cloud
//! adapters with static catalogs use this helper.
//!
//! See `docs/decisions/ORCH-0030/M3-CONTRACT.md` §5.5 for the
//! adapter-local model resolution contract.

use crate::domain::provider::ProviderError;

/// Resolve `selectors.model` against a static supported list.
///
/// - `None` → returns `default_model`.
/// - `Some("recommended:*")` → returns `default_model`. Cloud
///   adapters with static catalogs treat every `recommended:*`
///   moniker as "give me your default" because the recommendation
///   engine that mapped capabilities to specific cloud models is
///   gone in M3.
/// - `Some(concrete)` where `concrete` is in `supported_models` →
///   returns the matched name.
/// - `Some(concrete)` where `concrete` is **not** in
///   `supported_models` → returns
///   `Err(ProviderError::PinNotServable { model: concrete, reason })`.
pub fn resolve_cloud_model(
    model_input: Option<&str>,
    default_model: &'static str,
    supported_models: &[&'static str],
) -> Result<String, ProviderError> {
    let Some(input) = model_input else {
        return Ok(default_model.to_string());
    };
    if input.starts_with("recommended:") {
        return Ok(default_model.to_string());
    }
    if supported_models.iter().any(|m| *m == input) {
        return Ok(input.to_string());
    }
    Err(ProviderError::PinNotServable {
        model: input.to_string(),
        reason: format!(
            "model not in supported list (supported: {})",
            supported_models.join(", ")
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT: &str = "gemini-2.0-flash";
    const SUPPORTED: &[&str] = &[
        "gemini-2.0-flash",
        "gemini-2.0-pro",
        "gemini-1.5-pro",
    ];

    #[test]
    fn no_input_returns_default() {
        assert_eq!(
            resolve_cloud_model(None, DEFAULT, SUPPORTED).unwrap(),
            DEFAULT
        );
    }

    #[test]
    fn recommended_moniker_returns_default() {
        assert_eq!(
            resolve_cloud_model(Some("recommended:chat"), DEFAULT, SUPPORTED).unwrap(),
            DEFAULT
        );
        assert_eq!(
            resolve_cloud_model(Some("recommended:vision"), DEFAULT, SUPPORTED).unwrap(),
            DEFAULT
        );
    }

    #[test]
    fn supported_concrete_model_passes_through() {
        assert_eq!(
            resolve_cloud_model(Some("gemini-2.0-pro"), DEFAULT, SUPPORTED).unwrap(),
            "gemini-2.0-pro"
        );
    }

    #[test]
    fn unknown_concrete_model_is_pin_not_servable() {
        let err = resolve_cloud_model(Some("gpt-4o"), DEFAULT, SUPPORTED).unwrap_err();
        match err {
            ProviderError::PinNotServable { model, reason } => {
                assert_eq!(model, "gpt-4o");
                assert!(reason.contains("supported list"));
            }
            other => panic!("expected PinNotServable, got {other:?}"),
        }
    }
}
