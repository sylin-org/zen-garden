//! Conformance test that runs every case in
//! `docs/specs/zen-garden-uri-test-vectors.json` against the
//! [`garden_common::uri::ZenGardenUri`] parser.
//!
//! The corpus is the cross-language contract: any parser implementation
//! (Rust here, C# in Koan framework) MUST pass every case. New URI
//! shapes added to URI-0003 add entries here in the same change.
//!
//! Run: `cargo test -p garden-common --test uri_corpus`

use garden_common::uri::ZenGardenUri;
use serde_json::Value;

const CORPUS_JSON: &str = include_str!("../../../docs/specs/zen-garden-uri-test-vectors.json");

#[test]
fn corpus() {
    let corpus: Value = serde_json::from_str(CORPUS_JSON).expect("corpus JSON parses");
    let cases = corpus["cases"]
        .as_array()
        .expect("corpus 'cases' is an array");

    let mut failures: Vec<String> = Vec::new();

    for case in cases {
        let name = case["name"].as_str().unwrap_or("<unnamed>");
        let uri = case["uri"]
            .as_str()
            .unwrap_or_else(|| panic!("case '{name}': missing 'uri'"));
        let should_parse = case["parses"]
            .as_bool()
            .unwrap_or_else(|| panic!("case '{name}': missing 'parses'"));

        let result = ZenGardenUri::parse(uri);

        if should_parse {
            match result {
                Ok(parsed) => {
                    if let Err(diff) = compare(case, &parsed) {
                        failures.push(format!("{name}: {diff}"));
                    }
                }
                Err(e) => {
                    failures.push(format!("{name}: expected parse, got error {e:?}"));
                }
            }
        } else {
            let expected_error = case["error"]
                .as_str()
                .unwrap_or_else(|| panic!("case '{name}': missing 'error'"));
            match result {
                Ok(parsed) => failures.push(format!(
                    "{name}: expected error '{expected_error}', parsed successfully: {:?}",
                    parsed.canonical()
                )),
                Err(e) => {
                    let actual = e.category();
                    if actual != expected_error {
                        failures.push(format!(
                            "{name}: expected error '{expected_error}', got '{actual}' ({e:?})"
                        ));
                    }
                }
            }
        }
    }

    if !failures.is_empty() {
        panic!(
            "{} corpus failure(s):\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}

fn compare(case: &Value, parsed: &ZenGardenUri) -> Result<(), String> {
    // kind_explicit
    let expected_kind_explicit = case["kind_explicit"]
        .as_bool()
        .ok_or("missing 'kind_explicit'")?;
    if parsed.kind_explicit() != expected_kind_explicit {
        return Err(format!(
            "kind_explicit: expected {expected_kind_explicit}, got {}",
            parsed.kind_explicit()
        ));
    }

    // kind
    let expected_kind = case["kind"].as_str();
    let actual_kind = parsed.kind.map(|k| k.as_str());
    if expected_kind != actual_kind {
        return Err(format!(
            "kind: expected {expected_kind:?}, got {actual_kind:?}"
        ));
    }

    // target_name
    if !str_eq(case.get("target_name"), parsed.target_name.as_deref()) {
        return Err(format!(
            "target_name: expected {:?}, got {:?}",
            case["target_name"], parsed.target_name
        ));
    }

    // target_instance
    if !str_eq(case.get("target_instance"), parsed.target_instance.as_deref()) {
        return Err(format!(
            "target_instance: expected {:?}, got {:?}",
            case["target_instance"], parsed.target_instance
        ));
    }

    // sub_path
    if !str_eq(case.get("sub_path"), parsed.sub_path.as_deref()) {
        return Err(format!(
            "sub_path: expected {:?}, got {:?}",
            case["sub_path"], parsed.sub_path
        ));
    }

    // capabilities
    let expected_caps = string_array(&case["capabilities"]);
    if expected_caps != parsed.capabilities {
        return Err(format!(
            "capabilities: expected {expected_caps:?}, got {:?}",
            parsed.capabilities
        ));
    }

    // action
    if !str_eq(case.get("action"), parsed.action.as_deref()) {
        return Err(format!(
            "action: expected {:?}, got {:?}",
            case["action"], parsed.action
        ));
    }

    // at
    if !str_eq(case.get("at"), parsed.at.as_deref()) {
        return Err(format!("at: expected {:?}, got {:?}", case["at"], parsed.at));
    }

    // tags
    let expected_tags = string_array(&case["tags"]);
    if expected_tags != parsed.tags {
        return Err(format!(
            "tags: expected {expected_tags:?}, got {:?}",
            parsed.tags
        ));
    }

    // protocol_hint
    if !str_eq(case.get("protocol_hint"), parsed.protocol_hint.as_deref()) {
        return Err(format!(
            "protocol_hint: expected {:?}, got {:?}",
            case["protocol_hint"], parsed.protocol_hint
        ));
    }

    // fragment
    if !str_eq(case.get("fragment"), parsed.fragment.as_deref()) {
        return Err(format!(
            "fragment: expected {:?}, got {:?}",
            case["fragment"], parsed.fragment
        ));
    }

    // canonical
    let expected_canonical = case["canonical"]
        .as_str()
        .ok_or("missing 'canonical'")?;
    let actual_canonical = parsed.canonical();
    if actual_canonical != expected_canonical {
        return Err(format!(
            "canonical: expected '{expected_canonical}', got '{actual_canonical}'"
        ));
    }

    Ok(())
}

/// Compare a JSON `string | null` against an `Option<&str>`.
fn str_eq(json: Option<&Value>, val: Option<&str>) -> bool {
    match (json, val) {
        (Some(Value::Null), None) => true,
        (Some(Value::String(s)), Some(v)) => s == v,
        (None, None) => true,
        _ => false,
    }
}

/// Extract a string array from JSON (defaulting to empty when missing).
fn string_array(json: &Value) -> Vec<String> {
    json.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}
