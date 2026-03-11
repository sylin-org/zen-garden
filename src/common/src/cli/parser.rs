//! Zen syntax parser for CLI commands
//!
//! Parses command-line arguments to detect zen syntax and extract positional keywords
//! before they reach Clap. Verb recognition is driven by caller-provided sets
//! (typically built from the command manifest — single source of truth).
//!
//! Supports:
//! - `on <stone>` / `at <stone>` - target stone (on is preferred, at is legacy alias)
//! - `from <url>` - source URL for borrow command
//! - `quietly` - suppress non-essential output
//! - `fresh` - clear cache and force fresh discovery
//! - `until <condition>` - stream termination condition
//! - `somewhere` - intelligent placement
//! - `wishfully` - auto-provision if not found
//!
//! # Example
//! ```ignore
//! use garden_common::cli::parser::parse_args;
//! use std::collections::HashSet;
//!
//! let zen: HashSet<&str> = ["offer", "observe"].into_iter().collect();
//! let norm: HashSet<&str> = ["services"].into_iter().collect();
//! let args = vec!["offer".to_string(), "mongodb".to_string(), "on".to_string(), "stone-02".to_string()];
//! let parsed = parse_args(args, &zen, &norm)?;
//! assert_eq!(parsed.keywords.on_stone, Some("stone-02".to_string()));
//! ```

use std::collections::HashSet;

use anyhow::{anyhow, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum CommandStyle {
    Zen,
    Normative,
}

#[derive(Debug, Clone, Default)]
pub struct ParsedKeywords {
    pub on_stone: Option<String>,       // `on <stone>` or `at <stone>` (legacy)
    pub from_url: Option<String>,       // `from <url>` for borrow
    pub as_name: Option<String>,        // `as <name>` for storage add (STORAGE-0010)
    pub role: Option<String>,           // `role <role>` for storage add (STORAGE-0010)
    pub quietly: bool,
    pub fresh: bool,                    // clear cache and force fresh discovery
    pub until_condition: Option<String>,
    pub somewhere: bool,                // intelligent placement
    pub wishfully: bool,                // auto-provision if not found
}

#[derive(Debug, Clone)]
pub struct ParsedCommand {
    pub style: CommandStyle,
    pub verb: String,
    pub args: Vec<String>,
    pub keywords: ParsedKeywords,
}

/// Parse raw args to detect zen vs normative and extract positional keywords.
///
/// Verb recognition is driven by the caller-provided sets — typically built
/// from the command manifest (single source of truth).
pub fn parse_args(
    args: Vec<String>,
    zen_verbs: &HashSet<&str>,
    normative_verbs: &HashSet<&str>,
) -> Result<ParsedCommand> {
    if args.is_empty() {
        return Err(anyhow!("No command provided"));
    }

    let first_arg = &args[0];

    // Detect style based on first argument
    let style = if zen_verbs.contains(first_arg.as_str()) {
        CommandStyle::Zen
    } else if first_arg.starts_with("--")
        || first_arg.starts_with("-")
        || normative_verbs.contains(first_arg.as_str())
    {
        CommandStyle::Normative
    } else {
        // Unknown verb, let Clap handle the error
        return Err(anyhow!("Unknown command: {}", first_arg));
    };

    // Extract keywords and filter out from args
    let (keywords, filtered_args) = extract_keywords(&args[1..], &style, first_arg)?;

    // Validate: no mixing of zen positional keywords with normative flags
    if style == CommandStyle::Normative && has_zen_keywords(&args, first_arg) {
        return Err(anyhow!(
            "Cannot mix normative syntax with zen positional keywords. Use either:\n  \
             Zen:       {} {} quietly\n  \
             Normative: {} {} --quiet",
            first_arg,
            filtered_args.join(" "),
            first_arg,
            filtered_args.join(" ")
        ));
    }

    Ok(ParsedCommand {
        style,
        verb: first_arg.clone(),
        args: filtered_args,
        keywords,
    })
}

/// Check if args contain zen positional keywords
fn has_zen_keywords(args: &[String], verb: &str) -> bool {
    args.iter().any(|arg| {
        matches!(
            arg.as_str(),
            "on" | "at" | "as" | "role" | "with" | "quietly" | "fresh" | "until" | "somewhere"
                | "wishfully"
        ) || (verb == "borrow" && arg == "from")
    })
}

/// Extract positional keywords from args
fn extract_keywords(
    args: &[String],
    style: &CommandStyle,
    verb: &str,
) -> Result<(ParsedKeywords, Vec<String>)> {
    let mut keywords = ParsedKeywords::default();
    let mut filtered_args = Vec::new();
    let mut i = 0;

    while i < args.len() {
        let arg = &args[i];

        match arg.as_str() {
            // "on" is the preferred keyword, "at" is legacy alias
            "on" | "at" if *style == CommandStyle::Zen => {
                // Next arg is stone name
                i += 1;
                if i >= args.len() {
                    return Err(anyhow!("'{}' keyword requires stone name", arg));
                }
                keywords.on_stone = Some(args[i].clone());
            }
            // "from" for borrow command
            "from" if *style == CommandStyle::Zen && verb == "borrow" => {
                // Next arg is URL
                i += 1;
                if i >= args.len() {
                    return Err(anyhow!("'from' keyword requires URL"));
                }
                keywords.from_url = Some(args[i].clone());
            }
            // "as <name>" for storage add (STORAGE-0010)
            "as" if *style == CommandStyle::Zen => {
                i += 1;
                if i >= args.len() {
                    return Err(anyhow!("'as' keyword requires a name"));
                }
                keywords.as_name = Some(args[i].clone());
            }
            // "role <role>" for storage add (STORAGE-0010)
            "role" if *style == CommandStyle::Zen => {
                i += 1;
                if i >= args.len() {
                    return Err(anyhow!("'role' keyword requires a role name"));
                }
                keywords.role = Some(args[i].clone());
            }
            // "with" — semantic noise word, consumed and discarded (STORAGE-0010)
            "with" if *style == CommandStyle::Zen => {}
            "quietly" if *style == CommandStyle::Zen => {
                keywords.quietly = true;
            }
            "somewhere" if *style == CommandStyle::Zen => {
                keywords.somewhere = true;
            }
            "wishfully" if *style == CommandStyle::Zen => {
                keywords.wishfully = true;
            }
            "fresh" if *style == CommandStyle::Zen => {
                keywords.fresh = true;
            }
            "until" if *style == CommandStyle::Zen => {
                // Next arg is condition
                i += 1;
                if i >= args.len() {
                    return Err(anyhow!("'until' keyword requires condition"));
                }
                keywords.until_condition = Some(args[i].clone());
            }
            _ => {
                // Keep non-keyword args
                filtered_args.push(arg.clone());
            }
        }

        i += 1;
    }

    Ok((keywords, filtered_args))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test fixture: minimal zen verbs for parser behavior tests
    fn zen() -> HashSet<&'static str> {
        [
            "offer",
            "observe",
            "watch",
            "borrow",
            "capabilities",
            "rest",
            "wake",
            "status",
            "add",
            "release",
        ]
        .into_iter()
        .collect()
    }

    /// Test fixture: minimal normative verbs for parser behavior tests
    fn norm() -> HashSet<&'static str> {
        ["services", "offerings", "stones", "storage"]
            .into_iter()
            .collect()
    }

    #[test]
    fn test_zen_offer_with_on() {
        let args = vec![
            "offer".to_string(),
            "mongodb".to_string(),
            "on".to_string(),
            "stone-02".to_string(),
        ];
        let parsed = parse_args(args, &zen(), &norm()).unwrap();
        assert_eq!(parsed.style, CommandStyle::Zen);
        assert_eq!(parsed.verb, "offer");
        assert_eq!(parsed.args, vec!["mongodb"]);
        assert_eq!(parsed.keywords.on_stone, Some("stone-02".to_string()));
    }

    #[test]
    fn test_zen_offer_with_at_legacy() {
        let args = vec![
            "offer".to_string(),
            "mongodb".to_string(),
            "at".to_string(),
            "stone-02".to_string(),
        ];
        let parsed = parse_args(args, &zen(), &norm()).unwrap();
        assert_eq!(parsed.keywords.on_stone, Some("stone-02".to_string()));
    }

    #[test]
    fn test_zen_borrow_with_from() {
        let args = vec![
            "borrow".to_string(),
            "redis".to_string(),
            "from".to_string(),
            "redis://cache:6379".to_string(),
        ];
        let parsed = parse_args(args, &zen(), &norm()).unwrap();
        assert_eq!(parsed.style, CommandStyle::Zen);
        assert_eq!(parsed.verb, "borrow");
        assert_eq!(parsed.args, vec!["redis"]);
        assert_eq!(
            parsed.keywords.from_url,
            Some("redis://cache:6379".to_string())
        );
    }

    #[test]
    fn test_zen_non_borrow_keeps_from() {
        let args = vec![
            "capabilities".to_string(),
            "ollama".to_string(),
            "mirror".to_string(),
            "from".to_string(),
            "stone-02".to_string(),
        ];
        let parsed = parse_args(args, &zen(), &norm()).unwrap();
        assert_eq!(parsed.verb, "capabilities");
        assert_eq!(parsed.args, vec!["ollama", "mirror", "from", "stone-02"]);
        assert!(parsed.keywords.from_url.is_none());
    }

    #[test]
    fn test_zen_quietly() {
        let args = vec![
            "observe".to_string(),
            "all".to_string(),
            "quietly".to_string(),
        ];
        let parsed = parse_args(args, &zen(), &norm()).unwrap();
        assert!(parsed.keywords.quietly);
        assert_eq!(parsed.args, vec!["all"]);
    }

    #[test]
    fn test_zen_until() {
        let args = vec![
            "watch".to_string(),
            "mongodb".to_string(),
            "until".to_string(),
            "ready".to_string(),
        ];
        let parsed = parse_args(args, &zen(), &norm()).unwrap();
        assert_eq!(parsed.keywords.until_condition, Some("ready".to_string()));
    }

    #[test]
    fn test_normative_services() {
        let args = vec!["services".to_string(), "list".to_string()];
        let parsed = parse_args(args, &zen(), &norm()).unwrap();
        assert_eq!(parsed.style, CommandStyle::Normative);
    }

    #[test]
    fn test_mixing_rejected() {
        let args = vec![
            "services".to_string(),
            "list".to_string(),
            "quietly".to_string(),
        ];
        let result = parse_args(args, &zen(), &norm());
        assert!(result.is_err());
    }

    #[test]
    fn test_zen_fresh() {
        let args = vec!["observe".to_string(), "fresh".to_string()];
        let parsed = parse_args(args, &zen(), &norm()).unwrap();
        assert!(parsed.keywords.fresh);
    }

    // ── STORAGE-0010: as / role / with keywords ─────────────────────────

    #[test]
    fn test_zen_as_name() {
        let args = vec![
            "add".to_string(),
            "/dev/sdb".to_string(),
            "as".to_string(),
            "photos".to_string(),
        ];
        let parsed = parse_args(args, &zen(), &norm()).unwrap();
        assert_eq!(parsed.keywords.as_name, Some("photos".to_string()));
        assert_eq!(parsed.args, vec!["/dev/sdb"]);
    }

    #[test]
    fn test_zen_role() {
        let args = vec![
            "add".to_string(),
            "/dev/sdb".to_string(),
            "role".to_string(),
            "seed-bank".to_string(),
        ];
        let parsed = parse_args(args, &zen(), &norm()).unwrap();
        assert_eq!(parsed.keywords.role, Some("seed-bank".to_string()));
        assert_eq!(parsed.args, vec!["/dev/sdb"]);
    }

    #[test]
    fn test_zen_with_is_noise() {
        let args = vec![
            "add".to_string(),
            "/dev/sdb".to_string(),
            "as".to_string(),
            "photos".to_string(),
            "with".to_string(),
            "role".to_string(),
            "seed-bank".to_string(),
        ];
        let parsed = parse_args(args, &zen(), &norm()).unwrap();
        assert_eq!(parsed.keywords.as_name, Some("photos".to_string()));
        assert_eq!(parsed.keywords.role, Some("seed-bank".to_string()));
        assert_eq!(parsed.args, vec!["/dev/sdb"]);
    }

    #[test]
    fn test_zen_as_requires_value() {
        let args = vec!["add".to_string(), "as".to_string()];
        let result = parse_args(args, &zen(), &norm());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("'as' keyword requires"));
    }

    #[test]
    fn test_zen_role_requires_value() {
        let args = vec!["add".to_string(), "role".to_string()];
        let result = parse_args(args, &zen(), &norm());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("'role' keyword requires"));
    }
}
