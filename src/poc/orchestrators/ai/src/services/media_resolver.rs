//! Media resolver — applies the chosen capability's declared
//! `MediaDelivery` mode to each referenced media entry.
//!
//! # ORCH-0030 R2 M3 changes
//!
//! The resolver now reads `CapabilityMediaInput` from
//! [`crate::services::directory_subscriber::CapabilityDirectory`]
//! instead of the legacy `Registration.media_inputs` list. The
//! delivery semantics (`ById` / `Base64` / `Transfer`) are
//! unchanged.

use std::collections::HashMap;
use std::sync::Arc;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde_json::{Map, Value};

use crate::domain::capability_announcement::CapabilityMediaInput;
use crate::domain::errors::{ErrorCode, OrchestratorError};
use crate::domain::field_path::FieldPath;
use crate::domain::ids::MediaId;
use crate::domain::media::{MediaDelivery, ResolvedMedia};
use crate::domain::request::OrchestratorRequest;
use crate::services::directory_subscriber::CapabilityDirectory;

/// Resolve every media reference according to the chosen
/// capability's media-input declarations.
pub struct MediaResolver;

impl MediaResolver {
    pub async fn resolve(
        &self,
        mut request: OrchestratorRequest,
        directory: &Arc<CapabilityDirectory>,
    ) -> Result<OrchestratorRequest, OrchestratorError> {
        let Some(provider_name) = request.resolved_provider.as_ref().cloned() else {
            return Ok(request);
        };
        let Some(capability) = directory
            .capability(&provider_name, request.action.primitive)
            .await
        else {
            return Ok(request);
        };

        // Nothing to do when neither side references media.
        if capability.media_inputs.is_empty() && request.media.referenced.is_empty() {
            return Ok(request);
        }

        // Per-request resolution cache for base64 reuse.
        let mut fetched: HashMap<String, bytes::Bytes> = HashMap::new();

        // Map field paths → media input specs.
        let spec_for_field: HashMap<String, &CapabilityMediaInput> = capability
            .media_inputs
            .iter()
            .map(|spec| (spec.field.clone(), spec))
            .collect();

        for reference in request.media.referenced.clone() {
            let spec = match spec_for_field.get(reference.field.as_str()) {
                Some(s) => *s,
                None => {
                    return Err(OrchestratorError::new(
                        ErrorCode::ValidationFailed,
                        format!(
                            "Provider `{provider_name}` does not accept media at field `{}`.",
                            reference.field
                        ),
                    )
                    .with_details(serde_json::json!({
                        "provider": provider_name.as_str(),
                        "field": reference.field.as_str(),
                    })));
                }
            };

            // Validate content type against the capability's accepted list.
            let entry = request
                .context
                .media_store
                .get_metadata(&reference.id)
                .await
                .map_err(|e| {
                    OrchestratorError::new(
                        ErrorCode::NotFound,
                        format!("Media `{}` not found: {e}", reference.id),
                    )
                })?;
            if !spec.accepted_types.is_empty()
                && !spec
                    .accepted_types
                    .iter()
                    .any(|t| content_type_matches(t, &entry.content_type))
            {
                return Err(OrchestratorError::new(
                    ErrorCode::ValidationFailed,
                    format!(
                        "Media `{}` has content-type `{}`; provider `{}` accepts {:?} at `{}`.",
                        reference.id,
                        entry.content_type,
                        provider_name,
                        spec.accepted_types,
                        reference.field
                    ),
                ));
            }

            // Apply delivery mode.
            match spec.delivery {
                MediaDelivery::ById => {
                    request
                        .media
                        .resolutions
                        .insert(reference.id.as_str().to_string(), ResolvedMedia::ById);
                }
                MediaDelivery::Base64 => {
                    let bytes = if let Some(b) = fetched.get(reference.id.as_str()) {
                        b.clone()
                    } else {
                        let b = request
                            .context
                            .media_store
                            .get_bytes(&reference.id)
                            .await
                            .map_err(|e| {
                                OrchestratorError::new(
                                    ErrorCode::UpstreamError,
                                    format!("Failed to read media `{}`: {e}", reference.id),
                                )
                            })?;
                        fetched.insert(reference.id.as_str().to_string(), b.clone());
                        b
                    };
                    let encoded = BASE64.encode(&bytes);
                    inline_base64_into_payload(
                        &mut request.payload,
                        &reference.field,
                        &reference.id,
                        &encoded,
                        &entry.content_type,
                        bytes.len() as u64,
                    )?;
                    request.media.resolutions.insert(
                        reference.id.as_str().to_string(),
                        ResolvedMedia::Base64Embedded,
                    );
                }
                MediaDelivery::Transfer => {
                    request.media.resolutions.insert(
                        reference.id.as_str().to_string(),
                        ResolvedMedia::DeferredToProvider,
                    );
                }
            }

            let _ = request.context.media_store.touch(&reference.id).await;
        }

        Ok(request)
    }
}

fn content_type_matches(pattern: &str, actual: &str) -> bool {
    if pattern == "*/*" || pattern == actual {
        return true;
    }
    if let Some((family, rest)) = pattern.split_once('/') {
        if rest == "*" {
            if let Some((actual_family, _)) = actual.split_once('/') {
                return actual_family.eq_ignore_ascii_case(family);
            }
        }
    }
    false
}

fn inline_base64_into_payload(
    payload: &mut Value,
    field: &FieldPath,
    expected_id: &MediaId,
    encoded: &str,
    content_type: &str,
    size_bytes: u64,
) -> Result<(), OrchestratorError> {
    let Value::Object(root) = payload else {
        return Err(OrchestratorError::new(
            ErrorCode::InternalError,
            "Payload is not a JSON object during media resolution.",
        ));
    };
    let segments: Vec<&str> = field.as_str().split('.').collect();
    let slot = descend_mut(root, &segments).ok_or_else(|| {
        OrchestratorError::new(
            ErrorCode::InternalError,
            format!("Cannot locate `{field}` in payload for base64 substitution."),
        )
    })?;

    let current = slot
        .as_object()
        .and_then(|m| m.get("media_id"))
        .and_then(|v| v.as_str());
    if current != Some(expected_id.as_str()) {
        return Err(OrchestratorError::new(
            ErrorCode::InternalError,
            format!(
                "Media reference at `{field}` does not match expected id `{expected_id}`."
            ),
        ));
    }

    let mut replacement = Map::new();
    replacement.insert("base64".to_string(), Value::String(encoded.to_string()));
    replacement.insert(
        "content_type".to_string(),
        Value::String(content_type.to_string()),
    );
    replacement.insert(
        "size_bytes".to_string(),
        Value::Number(serde_json::Number::from(size_bytes)),
    );
    *slot = Value::Object(replacement);
    Ok(())
}

fn descend_mut<'a>(root: &'a mut Map<String, Value>, segments: &[&str]) -> Option<&'a mut Value> {
    if segments.is_empty() {
        return None;
    }
    let last = segments.len() - 1;
    let mut current_map: &mut Map<String, Value> = root;
    for (idx, segment) in segments.iter().enumerate() {
        if idx == last {
            return current_map.get_mut(*segment);
        }
        current_map = match current_map.get_mut(*segment) {
            Some(Value::Object(inner)) => inner,
            _ => return None,
        };
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_exact_content_type() {
        assert!(content_type_matches("image/png", "image/png"));
    }

    #[test]
    fn matches_wildcard() {
        assert!(content_type_matches("image/*", "image/png"));
        assert!(!content_type_matches("image/*", "audio/wav"));
        assert!(content_type_matches("*/*", "audio/wav"));
    }

    #[test]
    fn inline_base64_replaces_ref() {
        let mut payload = serde_json::json!({
            "image": {"source": {"media_id": "m1"}}
        });
        let id = MediaId::from_string("m1");
        let path = FieldPath::parse("image.source").unwrap();
        inline_base64_into_payload(&mut payload, &path, &id, "AAAA", "image/png", 3).unwrap();
        assert_eq!(
            payload,
            serde_json::json!({
                "image": {"source": {"base64": "AAAA", "content_type": "image/png", "size_bytes": 3}}
            })
        );
    }
}
