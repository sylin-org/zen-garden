//! Argument specification types for the command manifest
//!
//! These types describe CLI arguments declaratively, enabling both:
//! - Clap builder API generation (parsing)  
//! - Help system display (documentation)
//!
//! This replaces the old `CommandParam` with richer type information,
//! making the manifest the single source of truth for ALL command metadata.

/// How an argument is parsed by Clap
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgKind {
    /// Positional argument (no `--` prefix): `<service>`
    Positional,
    /// Boolean flag: `--force`, `--quiet`
    Flag,
    /// Named option with value: `--at <stone>`, `--name <name>`
    Option,
    /// Count flag: `-v`, `-vv`, `-vvv`
    Count,
    /// Option that accumulates multiple values: `--prefer a --prefer b`
    MultiOption,
    /// Trailing variable args: captures all remaining args
    Trailing,
}

/// Full argument specification — single source of truth for both parsing and help
#[derive(Debug, Clone)]
pub struct ArgSpec {
    // === Identity ===
    /// Argument name (used as key in ArgMatches and in help)
    pub name: &'static str,
    /// How Clap should parse this argument
    pub kind: ArgKind,

    // === Clap builder properties ===
    /// Short flag character (e.g., `'q'` for `-q`)
    pub short: Option<char>,
    /// Whether the argument is required
    pub required: bool,
    /// Default value if not provided
    pub default_value: Option<&'static str>,
    /// Allowed values for enum-like args
    pub possible_values: &'static [&'static str],
    /// Visible alias (e.g., `"on"` for `--at`)
    pub visible_alias: Option<&'static str>,
    /// Value delimiter for comma-separated lists
    pub value_delimiter: Option<char>,
    /// Whether this arg is global (available in subcommands)
    pub global: bool,
    /// Whether to allow values starting with `-`
    pub allow_hyphen_values: bool,

    // === Help display properties ===
    /// Description shown in help text
    pub description: &'static str,
    /// Zen syntax display: `"at <stone>"`, `"<service>"`
    pub zen_syntax: &'static str,
    /// Normative syntax display: `"--at <stone>"`
    pub normative_syntax: Option<&'static str>,
}

impl ArgSpec {
    // === Factory methods ===

    /// Create a positional argument: `<name>`
    pub fn positional(name: &'static str, description: &'static str) -> Self {
        Self {
            name,
            kind: ArgKind::Positional,
            short: None,
            required: false,
            default_value: None,
            possible_values: &[],
            visible_alias: None,
            value_delimiter: None,
            global: false,
            allow_hyphen_values: false,
            description,
            zen_syntax: "",
            normative_syntax: None,
        }
    }

    /// Create a boolean flag: `--force`
    pub fn flag(name: &'static str, description: &'static str) -> Self {
        Self {
            kind: ArgKind::Flag,
            ..Self::positional(name, description)
        }
    }

    /// Create a named option with value: `--at <stone>`
    pub fn option(name: &'static str, description: &'static str) -> Self {
        Self {
            kind: ArgKind::Option,
            ..Self::positional(name, description)
        }
    }

    /// Create a count flag: `-v`, `-vv`, `-vvv`
    pub fn count(name: &'static str, description: &'static str) -> Self {
        Self {
            kind: ArgKind::Count,
            ..Self::positional(name, description)
        }
    }

    /// Create a multi-value option: `--prefer a --prefer b`
    pub fn multi_option(name: &'static str, description: &'static str) -> Self {
        Self {
            kind: ArgKind::MultiOption,
            ..Self::positional(name, description)
        }
    }

    /// Create a trailing var-arg: captures all remaining args
    pub fn trailing(name: &'static str, description: &'static str) -> Self {
        Self {
            kind: ArgKind::Trailing,
            ..Self::positional(name, description)
        }
    }

    // === Builder methods (fluent API) ===

    /// Set short flag character
    pub fn short(mut self, c: char) -> Self {
        self.short = Some(c);
        self
    }

    /// Mark as required
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Set default value
    pub fn default(mut self, value: &'static str) -> Self {
        self.default_value = Some(value);
        self
    }

    /// Set visible alias (e.g., `"on"` for `--at`)
    pub fn alias(mut self, alias: &'static str) -> Self {
        self.visible_alias = Some(alias);
        self
    }

    /// Set zen syntax display string
    pub fn zen(mut self, syntax: &'static str) -> Self {
        self.zen_syntax = syntax;
        self
    }

    /// Set normative syntax display string
    pub fn normative(mut self, syntax: &'static str) -> Self {
        self.normative_syntax = Some(syntax);
        self
    }

    /// Set value delimiter
    pub fn delimiter(mut self, d: char) -> Self {
        self.value_delimiter = Some(d);
        self
    }

    /// Set allowed values
    pub fn values(mut self, vs: &'static [&'static str]) -> Self {
        self.possible_values = vs;
        self
    }

    /// Mark as global (available in subcommands)
    pub fn global(mut self) -> Self {
        self.global = true;
        self
    }

    /// Allow values starting with `-`
    pub fn hyphen_values(mut self) -> Self {
        self.allow_hyphen_values = true;
        self
    }
}

/// Subcommand definition for nested commands (e.g., `pond init`, `capabilities add`)
#[derive(Debug, Clone)]
pub struct SubDef {
    /// Subcommand name
    pub name: &'static str,
    /// Short description
    pub description: &'static str,
    /// Arguments for this subcommand
    pub args: Vec<ArgSpec>,
    /// Nested subcommands (e.g., `watch offering logs`)
    pub subcommands: Vec<SubDef>,
}

// === Common arg patterns (DRY helpers) ===

/// Standard `--at <stone>` argument used by most remote-capable commands
pub fn at_arg() -> ArgSpec {
    ArgSpec::option("at", "Target stone (omit to use tended stone)")
        .zen("at <stone>")
        .normative("--at <stone>")
        .alias("on")
}

/// Standard `--at <stone>` as a global arg (for commands with subcommands)
pub fn at_arg_global() -> ArgSpec {
    at_arg().global()
}

/// Standard `--force` flag
pub fn force_flag() -> ArgSpec {
    ArgSpec::flag("force", "Skip confirmation prompt").zen("--force")
}
