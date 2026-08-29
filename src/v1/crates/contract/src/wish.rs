//! The capability wish: how a consumer asks for CONTENT, not just a
//! service (J1's deep end; docs/v1/design/capability-wishes.md).
//!
//! Grammar — bracket syntax only:
//!   `ollama[model:llama3]`                     stem + one selector
//!   `ollama::dev[model:llama3,multi:10]`       instance + selectors
//!   `ollama[model:llama3|mistral]`             `|` inside brackets is a
//!                                              separator too (types never
//!                                              contain it)
//! The PoC's `stem:item` shorthand is deliberately NOT grammar here: a
//! single colon is v1's loud typo shape for FQNs (`redis:7` → refused
//! with the `::` hint), and a heuristic to tell wishes from typos would
//! be two meanings for one spelling (R1.3). Brackets are the wish marker.
//!
//! Parse results are three-valued on purpose: `Ok(None)` — no bracket,
//! not a wish at all (plain offering reference); `Ok(Some)` — a wish;
//! `Err` — bracket present but malformed, and the error TEACHES (R3.3).

use serde::{Deserialize, Serialize};

/// One typed selector inside a wish: `model:llama3`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilitySelector {
    /// Capability type, lowercase ("model").
    pub kind: String,
    /// The capability item ("llama3").
    pub item: String,
}

/// A parsed wish: an offering reference plus the capabilities wanted of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wish {
    /// The offering part as typed — a stem (`ollama`) or an FQN
    /// (`ollama::dev`). Canonicalization is the caller's move (glossary).
    pub offering: String,
    pub selectors: Vec<CapabilitySelector>,
}

/// Why the input could not be read as a wish. Answers what happened,
/// what it means, and what to try (R3.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WishError(pub String);

impl std::fmt::Display for WishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for WishError {}

/// Parse a capability wish. `Ok(None)` when the input carries no bracket
/// (the caller treats it as a plain offering reference).
pub fn parse_wish(input: &str) -> Result<Option<Wish>, WishError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(WishError(
            "empty reference — name an offering, with [type:item] to wish for capabilities"
                .into(),
        ));
    }
    let Some(open) = trimmed.find('[') else {
        // A stray close-bracket alone is still malformed input.
        if trimmed.contains(']') {
            return Err(WishError(
                "']' without '[' — capability selectors ride in [type:item] brackets".into(),
            ));
        }
        return Ok(None);
    };
    if !trimmed.ends_with(']') {
        return Err(WishError(format!(
            "capability selectors must end with ']' — e.g. {}[model:llama3]",
            &trimmed[..open]
        )));
    }
    let offering = trimmed[..open].trim();
    if offering.is_empty() {
        return Err(WishError(
            "name the offering before the brackets — e.g. ollama[model:llama3]".into(),
        ));
    }
    let body = &trimmed[open + 1..trimmed.len() - 1];
    if body.trim().is_empty() {
        return Err(WishError(format!(
            "no selectors in the brackets — use type:item, e.g. {offering}[model:llama3]"
        )));
    }
    let mut selectors = Vec::new();
    // Comma separates type:item pairs; a pipe continues the SAME type
    // (`model:llama3|mistral` = two models) — the PoC's shorthand, kept.
    for group in body.split(',') {
        let mut parts = group.split('|');
        let first = parts.next().unwrap_or("").trim();
        if first.is_empty() {
            return Err(WishError(format!(
                "empty selector in [{body}] — use type:item, e.g. model:llama3"
            )));
        }
        let Some((kind, item)) = first.split_once(':') else {
            return Err(WishError(format!(
                "selector '{first}' needs a type — use type:item, e.g. model:llama3"
            )));
        };
        let kind = kind.trim().to_ascii_lowercase();
        let item = item.trim().to_string();
        validate_part(&kind, &item, first)?;
        selectors.push(CapabilitySelector { kind: kind.clone(), item });
        for rest in parts {
            let item = rest.trim().to_string();
            if item.is_empty() {
                return Err(WishError(format!(
                    "empty selector after '|' in [{body}] — name the item, e.g. model:llama3|mistral"
                )));
            }
            validate_part(&kind, &item, &item)?;
            selectors.push(CapabilitySelector { kind: kind.clone(), item });
        }
    }
    Ok(Some(Wish { offering: offering.to_string(), selectors }))
}

/// Types and items are plain names — reserved grammar characters refuse
/// with a teaching error (R3.3).
fn validate_part(kind: &str, item: &str, shown: &str) -> Result<(), WishError> {
    if kind.is_empty() || item.is_empty() {
        return Err(WishError(format!(
            "selector '{shown}' needs both a type and an item — e.g. model:llama3"
        )));
    }
    for part in [kind, item] {
        if part.contains(['[', ']', ',', '|']) || part.contains("::") {
            return Err(WishError(format!(
                "selector '{shown}' carries a reserved character — types and items are plain names"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    // R4.1: unwrap/expect sanctioned in tests.
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    /// The grammar's shape, pinned: brackets are the wish marker; kinds
    /// lowercase; comma and pipe both separate (R1.7 fixtures).
    #[test]
    fn parses_the_documented_shapes() {
        let w = parse_wish("ollama[model:llama3]").unwrap().unwrap();
        assert_eq!(w.offering, "ollama");
        assert_eq!(
            w.selectors,
            vec![CapabilitySelector { kind: "model".into(), item: "llama3".into() }]
        );

        let w = parse_wish("ollama::dev [ model:llama3 , multi:10 ]").unwrap().unwrap();
        assert_eq!(w.offering, "ollama::dev");
        assert_eq!(w.selectors.len(), 2);
        assert_eq!(w.selectors[1].kind, "multi");

        let w = parse_wish("ollama[model:llama3|mistral]").unwrap().unwrap();
        assert_eq!(w.selectors.len(), 2);

        // Kind case folds; items keep their case.
        let w = parse_wish("Ollama[MODEL:LLaMA3]").unwrap().unwrap();
        assert_eq!(w.selectors[0].kind, "model");
        assert_eq!(w.selectors[0].item, "LLaMA3");
    }

    /// No bracket: not a wish — the plain-offering path, untouched.
    #[test]
    fn plain_references_are_not_wishes() {
        assert_eq!(parse_wish("ollama").unwrap(), None);
        assert_eq!(parse_wish("ollama::dev").unwrap(), None);
        // Single colon stays the FQN typo shape — never a wish.
        assert_eq!(parse_wish("ollama:llama3").unwrap(), None);
    }

    /// Malformed brackets teach (R3.3): what happened, what it means,
    /// what to try.
    #[test]
    fn malformed_wishes_answer_what_to_try() {
        for (bad, fragment) in [
            ("ollama[model:llama3", "must end with ']'"),
            ("[model:llama3]", "name the offering"),
            ("ollama[]", "no selectors"),
            ("ollama[model]", "needs a type"),
            ("ollama[model:]", "both a type and an item"),
            ("ollama[ , model:x]", "empty selector"),
            ("ollama]model:x[", "must end with ']'"),
            ("", "empty reference"),
        ] {
            let err = parse_wish(bad).unwrap_err();
            assert!(err.0.contains(fragment), "'{bad}' -> '{}' lacks '{fragment}'", err.0);
        }
    }
}
