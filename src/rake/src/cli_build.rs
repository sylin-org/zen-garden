//! CLI builder — generates Clap application from the command manifest
//!
//! This module bridges the declarative CommandManifest with Clap's builder API.
//! Instead of derive macros, the Clap command tree is built programmatically
//! from manifest data, making the manifest the single source of truth.

use crate::arg_spec::{ArgKind, ArgSpec, SubDef};
use crate::command_manifest::{CommandDef, CommandManifest, OnStoneMapping};
use std::collections::{HashMap, HashSet};

/// Global flags extracted from top-level Clap parsing
#[derive(Debug, Clone)]
pub struct GlobalFlags {
    pub quiet: bool,
    pub fresh: bool,
    pub verbose: u8,
    pub output: String,
    pub field: Option<String>,
}

impl Default for GlobalFlags {
    fn default() -> Self {
        Self {
            quiet: false,
            fresh: false,
            verbose: 0,
            output: "human".to_string(),
            field: None,
        }
    }
}

/// Build the complete Clap application from the manifest
pub fn build_clap_app(manifest: &CommandManifest) -> clap::Command {
    let mut app = clap::Command::new("garden-rake")
        .about("Zen Garden management CLI - run without arguments to see command directory")
        .version(concat!(env!("CARGO_PKG_VERSION"), ".", env!("BUILD_NUMBER")))
        .subcommand_required(false)
        .arg_required_else_help(false)
        .disable_help_subcommand(true);

    // === Global args ===
    app = app
        .arg(
            clap::Arg::new("quiet")
                .short('q')
                .long("quiet")
                .global(true)
                .action(clap::ArgAction::SetTrue)
                .help("Suppress suggestions (zen: quietly, env: GARDEN_QUIET)"),
        )
        .arg(
            clap::Arg::new("fresh")
                .long("fresh")
                .global(true)
                .action(clap::ArgAction::SetTrue)
                .help("Clear cached tending and force fresh discovery (zen: fresh)"),
        )
        .arg(
            clap::Arg::new("verbose")
                .short('v')
                .long("verbose")
                .global(true)
                .action(clap::ArgAction::Count)
                .help("Increase verbosity (-v info, -vv debug, -vvv trace)"),
        )
        .arg(
            clap::Arg::new("output")
                .short('o')
                .long("output")
                .global(true)
                .default_value("human")
                .help("Output format for automation (human, json)"),
        )
        .arg(
            clap::Arg::new("field")
                .long("field")
                .global(true)
                .help("Extract a specific field from the output (dot notation)"),
        );

    // === Build subcommands from manifest ===
    for cmd_def in manifest.all_sorted() {
        let sub = build_subcommand(cmd_def);
        app = app.subcommand(sub);
    }

    app
}

/// Build a Clap subcommand from a CommandDef
fn build_subcommand(def: &CommandDef) -> clap::Command {
    let mut cmd = clap::Command::new(def.name)
        .about(def.description)
        .long_about(def.long_description);

    // Set command-level flags
    if def.hidden {
        cmd = cmd.hide(true);
    }
    if def.subcommand_negates_reqs {
        cmd = cmd.subcommand_negates_reqs(true);
    }

    // Add zen name as alias if different from canonical name
    if def.zen_name != def.name {
        cmd = cmd.visible_alias(def.zen_name);
    }

    // Add normative name as alias if different
    // (Only simple single-word normative names work as Clap aliases)
    if let Some(norm) = def.normative_name {
        if !norm.contains(' ') && norm != def.name {
            cmd = cmd.visible_alias(norm);
        }
    }

    // Add arguments from ArgSpec
    for arg_spec in &def.args {
        cmd = cmd.arg(build_arg(arg_spec));
    }

    // Add nested subcommands from SubDef
    for sub_def in &def.subcommands {
        cmd = cmd.subcommand(build_sub(sub_def));
    }

    cmd
}

/// Build a Clap Arg from an ArgSpec
fn build_arg(spec: &ArgSpec) -> clap::Arg {
    let mut arg = clap::Arg::new(spec.name).help(spec.description);

    match spec.kind {
        ArgKind::Positional => {
            // No .long() — positional args don't have flag names
            if spec.required {
                arg = arg.required(true);
            }
        }
        ArgKind::Flag => {
            arg = arg.long(spec.name).action(clap::ArgAction::SetTrue);
        }
        ArgKind::Option => {
            arg = arg.long(spec.name);
        }
        ArgKind::Count => {
            arg = arg.long(spec.name).action(clap::ArgAction::Count);
        }
        ArgKind::MultiOption => {
            arg = arg
                .long(spec.name)
                .action(clap::ArgAction::Append);
            if let Some(d) = spec.value_delimiter {
                arg = arg.value_delimiter(d);
            }
        }
        ArgKind::Trailing => {
            arg = arg
                .trailing_var_arg(true)
                .num_args(0..);
            if spec.allow_hyphen_values {
                arg = arg.allow_hyphen_values(true);
            }
        }
    }

    // Common properties
    if let Some(c) = spec.short {
        arg = arg.short(c);
    }
    if let Some(default) = spec.default_value {
        arg = arg.default_value(default);
    }
    if !spec.possible_values.is_empty() {
        arg = arg.value_parser(spec.possible_values.to_vec());
    }
    if let Some(alias) = spec.visible_alias {
        arg = arg.visible_alias(alias);
    }
    if spec.global {
        arg = arg.global(true);
    }

    arg
}

/// Build a Clap subcommand from a SubDef (recursive)
fn build_sub(sub: &SubDef) -> clap::Command {
    let mut cmd = clap::Command::new(sub.name).about(sub.description);

    for arg_spec in &sub.args {
        cmd = cmd.arg(build_arg(arg_spec));
    }

    for nested in &sub.subcommands {
        cmd = cmd.subcommand(build_sub(nested));
    }

    cmd
}

// ============================================================================
// Alias Index — unified lookup for all command names (Proposal D)
// ============================================================================

/// Unified name resolution index — single source for all alias lookups.
///
/// Replaces the old `build_zen_lookup` + `build_normative_lookup` +
/// `find_by_any_name` with a single O(1) index built once at startup.
pub struct AliasIndex {
    /// Maps any known name → canonical command name
    to_canonical: HashMap<&'static str, &'static str>,
    /// Set of zen verbs (for parser style detection)
    zen_verbs: HashSet<&'static str>,
}

impl AliasIndex {
    /// Build from the manifest — consolidates zen names, aliases, and normative names.
    pub fn build(manifest: &CommandManifest) -> Self {
        let mut to_canonical = HashMap::new();
        let mut zen_verbs = HashSet::new();

        for cmd in manifest.all_sorted() {
            // Primary name always maps to itself
            to_canonical.insert(cmd.name, cmd.name);

            // Zen name
            to_canonical.insert(cmd.zen_name, cmd.name);
            zen_verbs.insert(cmd.zen_name);

            // Zen aliases
            for alias in cmd.zen_aliases {
                to_canonical.insert(alias, cmd.name);
                zen_verbs.insert(alias);
            }

            // Normative name (single-word only — multi-word like "services status" aren't Clap subcommands)
            if let Some(norm) = cmd.normative_name {
                if !norm.contains(' ') {
                    to_canonical.insert(norm, cmd.name);
                }
            }
        }

        Self {
            to_canonical,
            zen_verbs,
        }
    }

    /// Resolve any name (zen, alias, normative, canonical) to the canonical command name.
    pub fn resolve(&self, name: &str) -> Option<&'static str> {
        self.to_canonical.get(name).copied()
    }

    /// Check if a word is a zen verb (for parser style detection).
    pub fn is_zen_verb(&self, word: &str) -> bool {
        self.zen_verbs.contains(word)
    }

    /// Get the full set of zen verbs.
    pub fn zen_verbs(&self) -> &HashSet<&'static str> {
        &self.zen_verbs
    }

    /// Get all known verbs (zen + normative + canonical) for parser detection.
    pub fn all_known_verbs(&self) -> HashSet<&'static str> {
        self.to_canonical.keys().copied().collect()
    }
}

/// Normalize zen syntax to Clap-parseable args using manifest data.
///
/// Converts zen verb + positional keywords into normative args that Clap can parse.
/// The `on <stone>` mapping is now driven by `CommandDef.on_stone_mapping` instead
/// of a hardcoded match statement.
pub fn normalize_zen_to_clap(
    parsed: &garden_common::cli::parser::ParsedCommand,
    alias_index: &AliasIndex,
    manifest: &CommandManifest,
) -> anyhow::Result<Vec<String>> {
    let canonical = alias_index
        .resolve(&parsed.verb)
        .ok_or_else(|| anyhow::anyhow!("Unknown zen verb: {}", parsed.verb))?;

    // Look up the command def for on_stone_mapping
    let cmd_def = manifest.get(canonical);
    let on_stone = cmd_def
        .map(|d| d.on_stone_mapping)
        .unwrap_or(OnStoneMapping::ToAtFlag);

    let mut args = Vec::new();
    args.push(canonical.to_string());

    // Verb-specific arg structure transformations.
    // Most verbs just pass args through; a few need special handling.
    match parsed.verb.as_str() {
        // "explore" → "offer" with no args (list mode)
        "explore" => {}
        // "capabilities" zen syntax: `capabilities ollama mirror from stone-02`
        // Clap expects: `capabilities mirror ollama`
        "capabilities" if parsed.args.len() >= 2 && parsed.args[1] == "mirror" => {
            let offering = parsed.args[0].clone();
            args.push("mirror".to_string());
            args.push(offering);
            args.extend(parsed.args[2..].to_vec());
        }
        // Default: pass all positional args through
        _ => {
            args.extend(parsed.args.clone());
        }
    }

    // Map `on <stone>` according to the manifest's on_stone_mapping (Proposal C)
    if let Some(stone) = &parsed.keywords.on_stone {
        match on_stone {
            OnStoneMapping::ToAtFlag => {
                args.push("--at".to_string());
                args.push(stone.clone());
            }
            OnStoneMapping::ToPositional => {
                args.push(stone.clone());
            }
            OnStoneMapping::Ignore => {}
        }
    }

    // Handle "somewhere" → --placement-mode
    if parsed.keywords.somewhere {
        let mode = if parsed.keywords.quietly {
            "auto"
        } else {
            "interactive"
        };
        args.push("--placement-mode".to_string());
        args.push(mode.to_string());
    }

    // Handle "from" → --from (for borrow command)
    if let Some(url) = &parsed.keywords.from_url {
        args.push("--from".to_string());
        args.push(url.clone());
    }

    // Handle "wishfully" → --wishful
    if parsed.keywords.wishfully {
        args.push("--wishful".to_string());
    }

    // Handle "quietly" → --quiet
    if parsed.keywords.quietly {
        args.push("--quiet".to_string());
    }

    // Handle "fresh" → --fresh
    if parsed.keywords.fresh {
        args.push("--fresh".to_string());
    }

    Ok(args)
}

/// Extract global flags from top-level ArgMatches
pub fn extract_global_flags(matches: &clap::ArgMatches) -> GlobalFlags {
    GlobalFlags {
        quiet: matches.get_flag("quiet")
            || std::env::var("GARDEN_QUIET")
                .map(|v| v == "1" || v.to_lowercase() == "true")
                .unwrap_or(false),
        fresh: matches.get_flag("fresh"),
        verbose: matches.get_count("verbose"),
        output: matches
            .get_one::<String>("output")
            .cloned()
            .unwrap_or_else(|| "human".to_string()),
        field: matches.get_one::<String>("field").cloned(),
    }
}

/// Count verbosity flags from raw args (before Clap parsing)
///
/// Supports: -v, -vv, -vvv, -vvvv, --verbose (counted per occurrence)
pub fn count_verbosity(args: &[String]) -> u8 {
    let mut count = 0u8;
    for arg in args {
        if arg == "--verbose" {
            count = count.saturating_add(1);
        } else if arg.starts_with('-') && !arg.starts_with("--") {
            let flag_chars: String = arg
                .chars()
                .skip(1)
                .take_while(|c| c.is_alphabetic())
                .collect();
            count = count.saturating_add(flag_chars.matches('v').count() as u8);
        }
    }
    count
}
