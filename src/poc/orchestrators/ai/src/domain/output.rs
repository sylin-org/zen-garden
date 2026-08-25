//! `Output` — a flat dotted-key map that serializes to nested JSON.
//!
//! Providers populate an [`Output`] with canonical key constants. On the
//! wire, the map is serialized as a nested JSON object per modality
//! (`{"text": {"prompt": {"user": "..."}}}`).
//!
//! Design note (§7 ADR): the output is a namespaced map instead of a
//! discriminated enum because providers sometimes produce field
//! combinations the enum would not anticipate. The tradeoff is that
//! runtime key access is stringly-typed; this is mitigated by the
//! canonical-key constants in [`crate::domain::keys`] and the CI guard
//! that rejects magic strings.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::domain::field_path::{FieldPath, FieldPathError};

/// An orchestrator output: a flat dotted-key map keyed by canonical
/// field paths.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Output {
    fields: BTreeMap<String, Value>,
}

impl Output {
    /// An empty output.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of populated keys.
    pub fn len(&self) -> usize {
        self.fields.len()
    }

    /// `true` if the output has no populated keys.
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Set a field. The previous value, if any, is overwritten.
    pub fn set(&mut self, key: &FieldPath, value: impl Into<Value>) -> &mut Self {
        self.fields.insert(key.as_str().to_string(), value.into());
        self
    }

    /// Set a field only if not already present.
    pub fn set_if_absent(&mut self, key: &FieldPath, value: impl Into<Value>) -> &mut Self {
        self.fields
            .entry(key.as_str().to_string())
            .or_insert_with(|| value.into());
        self
    }

    /// Get a field by canonical path.
    pub fn get(&self, key: &FieldPath) -> Option<&Value> {
        self.fields.get(key.as_str())
    }

    /// Remove a field by canonical path.
    pub fn remove(&mut self, key: &FieldPath) -> Option<Value> {
        self.fields.remove(key.as_str())
    }

    /// `true` if the field is populated.
    pub fn has(&self, key: &FieldPath) -> bool {
        self.fields.contains_key(key.as_str())
    }

    /// Iterate over populated keys (as `&str`).
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.fields.keys().map(|s| s.as_str())
    }

    /// Iterate over `(key, value)` pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.fields.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Merge another output into this one; conflicting keys are
    /// overwritten by `other`.
    pub fn merge(&mut self, other: Output) {
        for (k, v) in other.fields {
            self.fields.insert(k, v);
        }
    }

    /// Expand the flat map into a nested `serde_json::Value`.
    ///
    /// For a key `text.prompt.user`, the output places `user` inside
    /// `{"text": {"prompt": {...}}}`. Segments that collide with an
    /// existing scalar on the same path are dropped and a warning is
    /// logged (the provider produced inconsistent output).
    pub fn to_nested(&self) -> Value {
        let mut root = Map::new();
        for (flat_key, value) in &self.fields {
            insert_nested(&mut root, flat_key, value.clone());
        }
        Value::Object(root)
    }

    /// Parse a nested JSON object back into a flat [`Output`]. Used by
    /// the idempotency cache when rehydrating stored responses.
    pub fn from_nested(value: Value) -> Result<Self, OutputError> {
        let mut out = Output::new();
        match value {
            Value::Object(map) => {
                flatten_into(&mut out.fields, "", &map);
                Ok(out)
            }
            other => Err(OutputError::NotObject(value_kind(&other).to_string())),
        }
    }
}

impl Serialize for Output {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_nested().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Output {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        Output::from_nested(value).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for Output {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_nested())
    }
}

fn insert_nested(root: &mut Map<String, Value>, dotted: &str, value: Value) {
    let segments: Vec<&str> = dotted.split('.').collect();
    if segments.is_empty() {
        return;
    }
    let last_idx = segments.len() - 1;
    let mut current: &mut Map<String, Value> = root;
    for (idx, segment) in segments.iter().enumerate() {
        if idx == last_idx {
            current.insert((*segment).to_string(), value);
            return;
        }
        // Ensure the slot is an object before descending.
        let slot = current
            .entry((*segment).to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if !slot.is_object() {
            tracing::warn!(
                key = dotted,
                conflict_segment = *segment,
                "output nesting collision; replacing scalar with object"
            );
            *slot = Value::Object(Map::new());
        }
        // Now the slot is guaranteed to be an object; descend.
        current = match current.get_mut(*segment) {
            Some(Value::Object(inner)) => inner,
            _ => unreachable!("just inserted or normalized to an object above"),
        };
    }
}

fn flatten_into(out: &mut BTreeMap<String, Value>, prefix: &str, map: &Map<String, Value>) {
    for (key, value) in map {
        let full = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match value {
            Value::Object(inner) if is_flat_candidate(inner) => {
                flatten_into(out, &full, inner);
            }
            _ => {
                out.insert(full, value.clone());
            }
        }
    }
}

/// Only flatten objects whose keys are all valid field-path segments
/// AND which are not media-reference objects (which have a `media_id`
/// key and should be treated as leaves so they serialize back as
/// `{media_id: "..."}` references).
fn is_flat_candidate(map: &Map<String, Value>) -> bool {
    if map.is_empty() {
        return false;
    }
    if map.contains_key("media_id") {
        return false;
    }
    map.keys().all(|k| FieldPath::validate(k).is_ok())
}

fn value_kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OutputError {
    #[error("cannot parse output from non-object JSON (got {0})")]
    NotObject(String),
    #[error("field path is invalid: {0}")]
    InvalidField(#[from] FieldPathError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::keys;
    use serde_json::json;

    #[test]
    fn set_and_get_roundtrip() {
        let mut out = Output::new();
        out.set(&keys::text::RESPONSE, "Hello");
        out.set(&keys::usage::TOKENS_INPUT, 12);
        out.set(&keys::usage::TOKENS_OUTPUT, 8);

        assert_eq!(out.get(&keys::text::RESPONSE), Some(&json!("Hello")));
        assert_eq!(out.get(&keys::usage::TOKENS_INPUT), Some(&json!(12)));
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn to_nested_produces_nested_json() {
        let mut out = Output::new();
        out.set(&keys::text::RESPONSE, "Hi");
        out.set(&keys::text::FINISH_REASON, "stop");
        out.set(&keys::usage::TOKENS_INPUT, 5);
        out.set(&keys::usage::TOKENS_OUTPUT, 2);
        out.set(&keys::timing::TOTAL_MS, 340);

        let nested = out.to_nested();
        assert_eq!(
            nested,
            json!({
                "text": {"response": "Hi", "finish_reason": "stop"},
                "usage": {"tokens": {"input": 5, "output": 2}},
                "timing": {"total_ms": 340}
            })
        );
    }

    #[test]
    fn from_nested_inverse_of_to_nested() {
        let mut out = Output::new();
        out.set(&keys::text::RESPONSE, "Hi");
        out.set(&keys::usage::TOKENS_INPUT, 5);

        let nested = out.to_nested();
        let rebuilt = Output::from_nested(nested).unwrap();

        assert_eq!(rebuilt.get(&keys::text::RESPONSE), Some(&json!("Hi")));
        assert_eq!(rebuilt.get(&keys::usage::TOKENS_INPUT), Some(&json!(5)));
    }

    #[test]
    fn opaque_scalar_arrays_are_preserved() {
        let mut out = Output::new();
        out.set(&keys::text::TOOL_CALLS, json!([{"name": "search", "arguments": {"q": "rust"}}]));

        let nested = out.to_nested();
        let rebuilt = Output::from_nested(nested).unwrap();
        assert_eq!(
            rebuilt.get(&keys::text::TOOL_CALLS),
            Some(&json!([{"name": "search", "arguments": {"q": "rust"}}]))
        );
    }

    #[test]
    fn serialize_is_nested_form() {
        let mut out = Output::new();
        out.set(&keys::text::RESPONSE, "Hi");
        let s = serde_json::to_string(&out).unwrap();
        assert_eq!(s, r#"{"text":{"response":"Hi"}}"#);
    }

    #[test]
    fn merge_overwrites_conflicts() {
        let mut a = Output::new();
        a.set(&keys::text::RESPONSE, "old");
        a.set(&keys::usage::TOKENS_INPUT, 1);

        let mut b = Output::new();
        b.set(&keys::text::RESPONSE, "new");
        b.set(&keys::timing::TOTAL_MS, 100);

        a.merge(b);
        assert_eq!(a.get(&keys::text::RESPONSE), Some(&json!("new")));
        assert_eq!(a.get(&keys::usage::TOKENS_INPUT), Some(&json!(1)));
        assert_eq!(a.get(&keys::timing::TOTAL_MS), Some(&json!(100)));
    }

    #[test]
    fn deep_nesting_roundtrip() {
        let mut out = Output::new();
        out.set(&keys::image::DIMENSIONS_WIDTH, 1024);
        out.set(&keys::image::DIMENSIONS_HEIGHT, 768);
        let nested = out.to_nested();
        assert_eq!(
            nested,
            json!({"image": {"dimensions": {"width": 1024, "height": 768}}})
        );
        let rebuilt = Output::from_nested(nested).unwrap();
        assert_eq!(rebuilt, out);
    }
}
