//! CLI builder — generates Clap application from the command manifest
//!
//! Bridges the declarative CommandManifest with Clap's builder API.
//! Single grammar: `verb [noun] [--flags]`. No dual syntax, no normalization.

use crate::arg_spec::{ArgKind, ArgSpec, SubDef};
use crate::command_manifest::{CommandDef, CommandManifest};

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
                .help("Suppress suggestions (env: GARDEN_QUIET)"),
        )
        .arg(
            clap::Arg::new("fresh")
                .long("fresh")
                .global(true)
                .action(clap::ArgAction::SetTrue)
                .help("Clear cached tending and force fresh discovery"),
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
                .help("Output format (human, json)"),
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

    if def.hidden {
        cmd = cmd.hide(true);
    }
    if def.subcommand_negates_reqs {
        cmd = cmd.subcommand_negates_reqs(true);
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

/// Count verbosity flags from raw args (before Clap parsing, for tracing init)
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
