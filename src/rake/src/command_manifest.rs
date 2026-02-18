//! Command manifest system for Zen Garden Rake
//!
//! Single source of truth for ALL command metadata: identity, arguments,
//! examples, descriptions, and relationships. The Clap CLI is generated
//! from this manifest via the builder API (see cli_build.rs).
//!
//! Adding a new command requires exactly TWO changes:
//! 1. Add a CommandDef entry here
//! 2. Add a handler in commands/ and wire it in route.rs

use crate::arg_spec::{at_arg, at_arg_global, force_flag, ArgSpec, SubDef};
use once_cell::sync::Lazy;
use std::collections::HashMap;

/// Command name constants - single source of truth for command identifiers.
/// Use these constants instead of string literals when referencing commands.
#[allow(dead_code)]
pub mod cmd {
    /// Tend subcommand targets
    pub mod tend_target {
        pub const THIS: &str = "this";
        pub const LOCAL: &str = "local";
        pub const AUTO: &str = "auto";
        pub const ANOTHER: &str = "another";
    }

    // Discovery
    pub const OBSERVE: &str = "observe";
    pub const WATCH: &str = "watch";
    pub const LIST: &str = "list";
    pub const STATUS: &str = "status";
    pub const CAPABILITIES: &str = "capabilities";

    // Lifecycle
    pub const OFFER: &str = "offer";
    pub const REST: &str = "rest";
    pub const WAKE: &str = "wake";
    pub const REMOVE: &str = "remove";
    pub const UPROOT: &str = "uproot";
    pub const NOURISH: &str = "nourish";
    pub const UPGRADE: &str = "upgrade";

    // Adoption
    pub const ADOPT: &str = "adopt";
    pub const RELEASE: &str = "release";
    pub const LOCATE: &str = "locate";
    pub const ADOPTED: &str = "adopted";
    pub const BORROWED: &str = "borrowed";
    pub const BORROW: &str = "borrow";
    pub const RETURN: &str = "return";

    // Discovery (service find/config)
    pub const FIND: &str = "find";
    pub const CONFIG: &str = "config";

    // Management
    pub const TEND: &str = "tend";
    pub const RECONCILE: &str = "reconcile";
    pub const REFRESH: &str = "refresh";

    // System
    pub const TAKE_ROOT: &str = "take-root";
    pub const INSTALL_SERVICE: &str = "install-service";
    pub const MAKE: &str = "make";

    // Pond
    pub const POND: &str = "pond";
    pub const PLACE: &str = "place";
    pub const INVITE: &str = "invite";
    pub const LIFT: &str = "lift";

    // Scaffolded
    pub const CEREMONY: &str = "ceremony";
    pub const TEMPLATE: &str = "template";

    // Local/Meta commands (not requiring stone)
    pub const BROWSE: &str = "browse";
    pub const LAUNCH: &str = "launch";
    pub const COMMANDS: &str = "commands";

    // Stone admin (power management)
    pub const ROUSE: &str = "rouse";
    pub const SLUMBER: &str = "slumber";
    pub const STIR: &str = "stir";

    // Test/Diagnostic
    pub const ELECTION: &str = "election";
    pub const PRESENCE: &str = "presence";

    // Companions
    pub const HEY: &str = "hey";

    // Developer Tools
    pub const API: &str = "api";

    // Storage (Seed Bank operations)
    pub const PREPARE: &str = "prepare";
    pub const RELEASE_SEED_BANK: &str = "release-seed-bank";
    pub const SEED_BANKS: &str = "seed-banks";
    pub const STORE: &str = "store";

    // Nurturing (Backup/Restore)
    pub const RESTORE: &str = "restore";
    pub const NURTURING: &str = "nurturing";
}

/// How zen `on <stone>` keyword maps in normative form.
///
/// Used by [`normalize_zen_to_clap`] so the mapping is manifest-driven
/// instead of hardcoded in a match statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OnStoneMapping {
    /// `on <stone>` maps to `--at <stone>` flag (most remote commands)
    #[default]
    ToAtFlag,
    /// `on <stone>` maps to a positional arg (e.g., observe uses `[stone]`)
    ToPositional,
    /// `on <stone>` is not applicable (local commands)
    Ignore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandCategory {
    /// Discovery commands: explore, observe, watch, list, status
    Discovery,
    /// Lifecycle commands: offer, rest, wake, nourish, remove, uproot
    Lifecycle,
    /// Adoption commands: adopt, release, find, adopted, borrowed, borrow, return
    Adoption,
    /// Management commands: reconcile, refresh, tend
    Management,
    /// System commands: take-root, make
    System,
    /// Pond security commands: place, invite, lift
    Pond,
    /// Companion commands: hey tell
    Companion,
    /// Storage commands: prepare, release-seed-bank, seed-banks, store
    Storage,
}

impl CommandCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Discovery => "Discovery",
            Self::Lifecycle => "Lifecycle",
            Self::Adoption => "Adoption",
            Self::Management => "Management",
            Self::System => "System",
            Self::Pond => "Pond",
            Self::Companion => "Companion",
            Self::Storage => "Storage",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommandExample {
    pub description: &'static str,
    pub zen_syntax: Option<&'static str>,
    pub normative_syntax: Option<&'static str>,
}

#[derive(Debug, Clone)]
pub struct CommandDef {
    /// Primary command name (used for lookup and Clap subcommand name)
    pub name: &'static str,
    /// Zen command name (e.g., "take-root")
    pub zen_name: &'static str,
    /// Additional zen verb aliases (e.g., "explore" → "offer", "touch" → "status")
    pub zen_aliases: &'static [&'static str],
    /// Normative command name (e.g., "install-service"), if different from zen
    pub normative_name: Option<&'static str>,
    /// Command category for grouping
    pub category: CommandCategory,
    /// Short description (one line)
    pub description: &'static str,
    /// Long description (multiple paragraphs)
    pub long_description: &'static str,
    /// Whether command supports --at/at for remote execution
    pub remote_capable: bool,
    /// Command arguments (single source of truth for parsing AND help)
    pub args: Vec<ArgSpec>,
    /// Nested subcommands (e.g., pond init/status/invite)
    pub subcommands: Vec<SubDef>,
    /// Usage examples
    pub examples: Vec<CommandExample>,
    /// Related commands
    pub see_also: Vec<&'static str>,
    /// Hide from default command listing
    pub hidden: bool,
    /// Whether subcommand presence negates parent required args
    pub subcommand_negates_reqs: bool,
    /// How zen `on <stone>` keyword maps for this command
    pub on_stone_mapping: OnStoneMapping,
}

pub struct CommandManifest {
    commands: HashMap<&'static str, CommandDef>,
}

impl Default for CommandManifest {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandManifest {
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }

    pub fn add(&mut self, cmd: CommandDef) {
        self.commands.insert(cmd.name, cmd);
    }

    pub fn get(&self, name: &str) -> Option<&CommandDef> {
        self.commands.get(name)
    }

    pub fn by_category(&self, category: &CommandCategory) -> Vec<&CommandDef> {
        self.commands
            .values()
            .filter(|cmd| &cmd.category == category)
            .collect()
    }

    pub fn all(&self) -> Vec<&CommandDef> {
        self.commands.values().collect()
    }

    /// Get all commands sorted by name (deterministic ordering for builder)
    pub fn all_sorted(&self) -> Vec<&CommandDef> {
        let mut cmds: Vec<&CommandDef> = self.commands.values().collect();
        cmds.sort_by_key(|c| c.name);
        cmds
    }

    /// Find a command by any name: primary name, zen_name, zen_aliases, or normative_name.
    /// Used by help query syntax (?command / command?) so aliases resolve correctly.
    pub fn find_by_any_name(&self, name: &str) -> Option<&CommandDef> {
        // Direct primary key lookup first (fast path)
        if let Some(cmd) = self.commands.get(name) {
            return Some(cmd);
        }
        // Search zen_name, zen_aliases, and normative_name
        self.commands.values().find(|cmd| {
            cmd.zen_name == name
                || cmd.zen_aliases.contains(&name)
                || cmd.normative_name == Some(name)
        })
    }
}

/// Global command manifest - initialized at program start
pub static MANIFEST: Lazy<CommandManifest> = Lazy::new(|| {
    let mut manifest = CommandManifest::new();

    // === DISCOVERY COMMANDS ===

    manifest.add(CommandDef {
        name: "observe",
        zen_name: "observe",
        zen_aliases: &["garden"],
        normative_name: None,
        category: CommandCategory::Discovery,
        description: "View garden state snapshot",
        long_description: "Observe garden state with optional filtering.\n\n\
            Shows current state of all stones and their offerings in a formatted table.\n\
            Provides snapshot view of the entire garden or filtered by stone/offering.",
        remote_capable: false,
        args: vec![
            ArgSpec::positional("stone", "Filter by specific stone name")
                .zen("<stone>"),
            ArgSpec::option("offering", "Filter by offering name (comma-separated)")
                .zen("--offering <name>"),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Observe all stones in garden",
                zen_syntax: Some("garden-rake observe"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Observe specific stone with all offerings",
                zen_syntax: Some("garden-rake observe stone-01"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Filter by specific offerings across all stones",
                zen_syntax: Some("garden-rake observe --offering mongodb,redis"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Observe stone with offering filter",
                zen_syntax: Some("garden-rake observe stone-01 --offering mongodb"),
                normative_syntax: None,
            },
        ],
        see_also: vec!["watch", "list"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToPositional,
    });


    manifest.add(CommandDef {
        name: "watch",
        zen_name: "watch",
        zen_aliases: &[],
        normative_name: None,
        category: CommandCategory::Discovery,
        description: "Stream real-time events from stone",
        long_description: "Stream real-time events from moss operations.\n\n\
            Watch provides live updates on container lifecycle, offering installations, and system events.\n\
            Can monitor general events or specific offering logs.",
        remote_capable: true,
        args: vec![
            at_arg_global(),
            ArgSpec::option("until", "Exit when string appears in event stream")
                .zen("until <condition>")
                .normative("--until <condition>"),
        ],
        subcommands: vec![
            SubDef {
                name: "offering",
                description: "Watch offering events",
                args: vec![
                    ArgSpec::positional("name", "Offering name")
                        .zen("<name>")
                        .required(),
                ],
                subcommands: vec![SubDef {
                    name: "logs",
                    description: "Stream offering container logs",
                    args: vec![
                        ArgSpec::flag("timestamps", "Show timestamps in log output"),
                    ],
                    subcommands: vec![],
                }],
            },
            SubDef {
                name: "stone",
                description: "Watch stone events",
                args: vec![
                    ArgSpec::positional("name", "Stone name")
                        .zen("<name>")
                        .required(),
                ],
                subcommands: vec![SubDef {
                    name: "logs",
                    description: "Stream stone logs",
                    args: vec![
                        ArgSpec::flag("timestamps", "Show timestamps in log output"),
                    ],
                    subcommands: vec![],
                }],
            },
        ],
        examples: vec![
            CommandExample {
                description: "Watch all events from tended stone",
                zen_syntax: Some("garden-rake watch"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Watch specific stone until completion",
                zen_syntax: Some("garden-rake watch at stone-01 until 'completed'"),
                normative_syntax: Some("garden-rake watch --at stone-01 --until 'completed'"),
            },
            CommandExample {
                description: "Watch with explicit endpoint",
                zen_syntax: Some("garden-rake watch at http://192.168.1.108:7185"),
                normative_syntax: Some("garden-rake watch --at http://192.168.1.108:7185"),
            },
            CommandExample {
                description: "Watch offering logs",
                zen_syntax: Some("garden-rake watch offering mongodb logs"),
                normative_syntax: None,
            },
        ],
        see_also: vec!["observe", "make"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    manifest.add(CommandDef {
        name: "list",
        zen_name: "list",
        zen_aliases: &[],
        normative_name: None,
        category: CommandCategory::Discovery,
        description: "List services on stone",
        long_description: "List all services (offerings) currently running on a stone.\n\n\
            Shows service names, status, ports, and basic health information.",
        remote_capable: true,
        args: vec![at_arg()],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "List services on tended stone",
                zen_syntax: Some("garden-rake list"),
                normative_syntax: None,
            },
            CommandExample {
                description: "List services on specific stone",
                zen_syntax: Some("garden-rake list at stone-01"),
                normative_syntax: Some("garden-rake list --at stone-01"),
            },
        ],
        see_also: vec!["observe", "status"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    // === LIFECYCLE COMMANDS ===

    manifest.add(CommandDef {
        name: "offer",
        zen_name: "offer",
        zen_aliases: &["explore"],
        normative_name: None,
        category: CommandCategory::Lifecycle,
        description: "Install or list offerings",
        long_description: "Manage offerings (services) - list available offerings or install specific ones.\n\n\
            Offerings are validated container templates. Installation includes compatibility checks,\n\
            hardware requirements validation, and automatic fallback recommendations.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("offering", "Offering name (omit to list all)")
                .zen("[offering]"),
            at_arg_global(),
            ArgSpec::multi_option("prefer", "Bias recommendations (e.g., ssd, nvme)")
                .zen("--prefer <hardware>")
                .delimiter(','),
            ArgSpec::flag("anywhere-on-fail", "Fall back to any available stone if target fails")
                .zen("--anywhere-on-fail"),
            ArgSpec::option("placement-mode", "Placement strategy (interactive, auto)")
                .zen("--placement-mode <mode>"),
        ],
        subcommands: vec![SubDef {
            name: "info",
            description: "Show offering details and compatibility",
            args: vec![],
            subcommands: vec![],
        }],
        examples: vec![
            CommandExample {
                description: "List all available offerings by category",
                zen_syntax: Some("garden-rake offer"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Install offering on tended stone",
                zen_syntax: Some("garden-rake offer mongodb"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Install on specific stone with hardware preference",
                zen_syntax: Some("garden-rake offer mongodb at stone-01 --prefer ssd"),
                normative_syntax: Some("garden-rake offer mongodb --at stone-01 --prefer ssd"),
            },
            CommandExample {
                description: "Show offering details and compatibility",
                zen_syntax: Some("garden-rake offer mongodb info"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Install with automatic fallback to any stone",
                zen_syntax: Some("garden-rake offer mongodb --anywhere-on-fail"),
                normative_syntax: None,
            },
        ],
        see_also: vec!["release", "list"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    manifest.add(CommandDef {
        name: "rest",
        zen_name: "rest",
        zen_aliases: &[],
        normative_name: None,
        category: CommandCategory::Lifecycle,
        description: "Stop a service (rest mode)",
        long_description: "Stop a running service without removing it.\n\n\
            Service enters rest mode and can be woken later with all data preserved.\n\
            Container is stopped but not removed.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("service", "Service name to stop")
                .zen("<service>")
                .required(),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Put service to rest on tended stone",
                zen_syntax: Some("garden-rake rest mongodb"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Put service to rest on specific stone",
                zen_syntax: Some("garden-rake rest mongodb at stone-01"),
                normative_syntax: Some("garden-rake rest mongodb --at stone-01"),
            },
        ],
        see_also: vec!["wake", "release"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    manifest.add(CommandDef {
        name: "wake",
        zen_name: "wake",
        zen_aliases: &[],
        normative_name: None,
        category: CommandCategory::Lifecycle,
        description: "Start a service (wake from rest)",
        long_description: "Start a service that is in rest mode.\n\n\
            Service resumes with all previous data and configuration intact.\n\
            Container is started from stopped state.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("service", "Service name to start")
                .zen("<service>")
                .required(),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Wake service on tended stone",
                zen_syntax: Some("garden-rake wake mongodb"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Wake service on specific stone",
                zen_syntax: Some("garden-rake wake mongodb at stone-01"),
                normative_syntax: Some("garden-rake wake mongodb --at stone-01"),
            },
        ],
        see_also: vec!["rest", "offer"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    manifest.add(CommandDef {
        name: "remove",
        zen_name: "remove",
        zen_aliases: &[],
        normative_name: Some("services delete"),
        category: CommandCategory::Lifecycle,
        description: "Remove service from registry (soft delete)",
        long_description:
            "Remove a service from moss registry without destroying the container.\n\n\
            The container becomes a 'stray' - still running but unmanaged.\n\
            Use 'uproot' for hard delete (destroy container and data).",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("service", "Service name to remove from registry")
                .zen("<service>")
                .required(),
            at_arg(),
            force_flag(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Remove service (stops and removes container, preserves volumes)",
                zen_syntax: Some("garden-rake remove mongodb"),
                normative_syntax: Some("garden-rake services delete mongodb"),
            },
            CommandExample {
                description: "Remove service on specific stone",
                zen_syntax: Some("garden-rake remove mongodb on stone-01"),
                normative_syntax: Some("garden-rake services delete mongodb --at stone-01"),
            },
        ],
        see_also: vec!["uproot", "adopt", "find"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    manifest.add(CommandDef {
        name: "uproot",
        zen_name: "uproot",
        zen_aliases: &[],
        normative_name: Some("services destroy"),
        category: CommandCategory::Lifecycle,
        description: "Destroy service completely (hard delete)",
        long_description: "Permanently destroy a service including container and data.\n\n\
            This is irreversible - container and volumes are deleted.\n\
            Use --force to skip confirmation prompt.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("service", "Service name to destroy")
                .zen("<service>")
                .required(),
            force_flag(),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Destroy service with confirmation",
                zen_syntax: Some("garden-rake uproot mongodb"),
                normative_syntax: Some("garden-rake services destroy mongodb"),
            },
            CommandExample {
                description: "Destroy service without confirmation",
                zen_syntax: Some("garden-rake uproot mongodb --force"),
                normative_syntax: Some("garden-rake services destroy mongodb --force"),
            },
            CommandExample {
                description: "Destroy service on specific stone",
                zen_syntax: Some("garden-rake uproot mongodb on stone-01"),
                normative_syntax: Some("garden-rake services destroy mongodb --at stone-01"),
            },
        ],
        see_also: vec!["remove", "rest"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    manifest.add(CommandDef {
        name: "nourish",
        zen_name: "nourish",
        zen_aliases: &[],
        normative_name: Some("services upgrade"),
        category: CommandCategory::Lifecycle,
        description: "Upgrade service to latest version",
        long_description: "Upgrade one or all services to their latest versions.\n\n\
            Pulls latest container images and recreates services with data preserved.\n\
            Use --all to upgrade all services on stone.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("service", "Service name (omit with --all)")
                .zen("[service]"),
            ArgSpec::flag("all", "Upgrade all services on stone").zen("--all"),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Upgrade specific service",
                zen_syntax: Some("garden-rake nourish mongodb"),
                normative_syntax: Some("garden-rake services upgrade mongodb"),
            },
            CommandExample {
                description: "Upgrade all services on stone",
                zen_syntax: Some("garden-rake nourish --all"),
                normative_syntax: Some("garden-rake services upgrade --all"),
            },
            CommandExample {
                description: "Upgrade service on specific stone",
                zen_syntax: Some("garden-rake nourish mongodb at stone-01"),
                normative_syntax: Some("garden-rake services upgrade mongodb --at stone-01"),
            },
        ],
        see_also: vec!["offer", "reconcile"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::Ignore,
    });

    manifest.add(CommandDef {
        name: cmd::UPGRADE,
        zen_name: "upgrade",
        zen_aliases: &[],
        normative_name: Some("services upgrade"),
        category: CommandCategory::Lifecycle,
        description: "Upgrade a service to latest image",
        long_description: "Upgrade a service container to the latest available image.\n\n\
            This pulls the latest image and recreates the container with the same configuration.\n\
            Use --all to upgrade all services on the stone.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("service", "Service name to upgrade (required unless --all)")
                .zen("[service]"),
            ArgSpec::flag("all", "Upgrade all services").zen("--all"),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Upgrade specific service",
                zen_syntax: Some("garden-rake upgrade mongodb"),
                normative_syntax: Some("garden-rake services upgrade mongodb"),
            },
            CommandExample {
                description: "Upgrade all services",
                zen_syntax: Some("garden-rake upgrade --all"),
                normative_syntax: Some("garden-rake services upgrade --all"),
            },
        ],
        see_also: vec!["nourish", "offer", "rest"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    manifest.add(CommandDef {
        name: cmd::CAPABILITIES,
        zen_name: "capabilities",
        zen_aliases: &[],
        normative_name: Some("services capabilities"),
        category: CommandCategory::Lifecycle,
        description: "Manage offering capabilities (models, extensions)",
        long_description:
            "Manage capabilities for an offering such as AI models or database extensions.\n\n\
            For AI offerings like Ollama, this manages available models.\n\
            For databases, this could manage extensions or plugins.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("offering", "Offering name to manage capabilities for")
                .zen("<offering>")
                .required(),
            at_arg_global(),
        ],
        subcommands: vec![
            SubDef {
                name: "add",
                description: "Add a capability to an offering",
                args: vec![
                    ArgSpec::positional("offering", "Offering name")
                        .zen("<offering>")
                        .required(),
                    ArgSpec::positional("name", "Capability name to add")
                        .zen("<name>")
                        .required(),
                    ArgSpec::option("type", "Capability type")
                        .zen("--type <type>")
                        .short('t'),
                    ArgSpec::flag("dry-run", "Validate only without adding").zen("--dry-run"),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "remove",
                description: "Remove a capability from an offering",
                args: vec![
                    ArgSpec::positional("offering", "Offering name")
                        .zen("<offering>")
                        .required(),
                    ArgSpec::positional("name", "Capability name to remove")
                        .zen("<name>")
                        .required(),
                    ArgSpec::option("type", "Capability type")
                        .zen("--type <type>")
                        .short('t'),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "refresh",
                description: "Refresh capabilities for an offering",
                args: vec![
                    ArgSpec::positional("offering", "Offering name")
                        .zen("<offering>")
                        .required(),
                    ArgSpec::option("type", "Capability type")
                        .zen("--type <type>")
                        .short('t'),
                    ArgSpec::flag("dry-run", "Validate only without refreshing")
                        .zen("--dry-run"),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "mirror",
                description: "Mirror capabilities between stones",
                args: vec![
                    ArgSpec::positional("offering", "Offering name")
                        .zen("<offering>")
                        .required(),
                    ArgSpec::trailing("args", "Mirror arguments (from <stone> to <stone>)")
                        .zen("from <stone> to <stone>"),
                ],
                subcommands: vec![],
            },
        ],
        examples: vec![
            CommandExample {
                description: "List capabilities for ollama",
                zen_syntax: Some("garden-rake capabilities ollama"),
                normative_syntax: Some("garden-rake services capabilities ollama"),
            },
            CommandExample {
                description: "Add a model to ollama",
                zen_syntax: Some("garden-rake capabilities add ollama llama3"),
                normative_syntax: Some("garden-rake services capabilities add ollama llama3"),
            },
            CommandExample {
                description: "Remove a model",
                zen_syntax: Some("garden-rake capabilities remove ollama phi"),
                normative_syntax: Some("garden-rake services capabilities remove ollama phi"),
            },
            CommandExample {
                description: "Mirror capabilities between stones",
                zen_syntax: Some(
                    "garden-rake capabilities ollama mirror from stone-01 to stone-02",
                ),
                normative_syntax: Some(
                    "garden-rake services capabilities mirror ollama from stone-01 to stone-02",
                ),
            },
        ],
        see_also: vec!["offer", "status"],
        hidden: false,
        subcommand_negates_reqs: true,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    // === ADOPTION COMMANDS ===

    manifest.add(CommandDef {
        name: "adopt",
        zen_name: "adopt",
        zen_aliases: &[],
        normative_name: Some("adoption claim"),
        category: CommandCategory::Adoption,
        description: "Adopt a stray container or detected service",
        long_description:
            "Claim an existing container or detected service into moss management.\n\n\
            Strays are containers that exist but aren't in moss registry.\n\
            Adopted services are external services (not containers) that moss monitors.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("target", "Container name or offering name to claim")
                .zen("<container|offering>")
                .required(),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Adopt a stray container",
                zen_syntax: Some("garden-rake adopt my-mongodb"),
                normative_syntax: Some("garden-rake adoption claim my-mongodb"),
            },
            CommandExample {
                description: "Adopt offering on specific stone",
                zen_syntax: Some("garden-rake adopt mongodb on stone-01"),
                normative_syntax: Some("garden-rake adoption claim mongodb --at stone-01"),
            },
        ],
        see_also: vec!["release", "find", "adopted"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    manifest.add(CommandDef {
        name: "release",
        zen_name: "release",
        zen_aliases: &[],
        normative_name: Some("adoption release"),
        category: CommandCategory::Adoption,
        description: "Release an adopted service from management",
        long_description: "Release an adopted service from moss management.\n\n\
            The service continues running but is no longer monitored by moss.\n\
            Does not affect borrowed services - use 'return' for those.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("service", "Adopted service name to release")
                .zen("<service>")
                .required(),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Release adopted service",
                zen_syntax: Some("garden-rake release mongodb"),
                normative_syntax: Some("garden-rake adoption release mongodb"),
            },
            CommandExample {
                description: "Release on specific stone",
                zen_syntax: Some("garden-rake release mongodb on stone-01"),
                normative_syntax: Some("garden-rake adoption release mongodb --at stone-01"),
            },
        ],
        see_also: vec!["adopt", "adopted"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    manifest.add(CommandDef {
        name: "locate",
        zen_name: "locate",
        zen_aliases: &[],
        normative_name: Some("adoption locate"),
        category: CommandCategory::Adoption,
        description: "Locate adoptable containers (strays)",
        long_description: "Locate containers that are not managed by Zen Garden (strays).\n\n\
            Strays are containers running on the stone but not in moss registry.\n\
            Use 'adopt <name>' to claim a stray container.",
        remote_capable: true,
        args: vec![at_arg_global()],
        subcommands: vec![SubDef {
            name: "strays",
            description: "Locate unmanaged containers",
            args: vec![],
            subcommands: vec![],
        }],
        examples: vec![
            CommandExample {
                description: "Locate stray containers",
                zen_syntax: Some("garden-rake locate strays"),
                normative_syntax: Some("garden-rake adoption locate strays"),
            },
            CommandExample {
                description: "Locate strays on specific stone",
                zen_syntax: Some("garden-rake locate strays on stone-01"),
                normative_syntax: Some("garden-rake adoption locate strays --at stone-01"),
            },
        ],
        see_also: vec!["adopt", "adopted"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    manifest.add(CommandDef {
        name: "find",
        zen_name: "find",
        zen_aliases: &[],
        normative_name: Some("services find"),
        category: CommandCategory::Discovery,
        description: "Find running services and get connection URIs",
        long_description: "Find running services across the garden and return connection URIs.\n\n\
            Supports search by name, category (c:prefix), or tags (t:prefix).\n\
            Results are returned instantly from topology cache.\n\n\
            Use 'wishfully' modifier to auto-provision if service not found.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("query", "Service name, c:category, or t:tag")
                .zen("<query>")
                .required(),
            ArgSpec::option("format", "Output format: human, json, uri, uri-ip")
                .zen("--format <format>"),
            ArgSpec::flag("wishfully", "Auto-provision if not found")
                .zen("wishfully")
                .normative("--wishful"),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Find mongodb service",
                zen_syntax: Some("garden-rake find mongodb"),
                normative_syntax: Some("garden-rake services find --name mongodb"),
            },
            CommandExample {
                description: "Find any database",
                zen_syntax: Some("garden-rake find c:database"),
                normative_syntax: Some("garden-rake services find --category database"),
            },
            CommandExample {
                description: "Get connection URI only",
                zen_syntax: Some("garden-rake find mongodb --format uri"),
                normative_syntax: Some("garden-rake services find --name mongodb --format uri"),
            },
            CommandExample {
                description: "Auto-provision if not found",
                zen_syntax: Some("garden-rake find mongodb wishfully"),
                normative_syntax: Some("garden-rake services find --name mongodb --wishful"),
            },
        ],
        see_also: vec!["observe", "list", "offer", "config"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    manifest.add(CommandDef {
        name: "config",
        zen_name: "config",
        zen_aliases: &[],
        normative_name: Some("services config"),
        category: CommandCategory::Discovery,
        description: "Get service configuration for automation",
        long_description: "Query detailed configuration for a service by name.\n\n\
            Designed for automation and scripting scenarios.\n\
            Returns connection URIs, ports, hostname, and protocol information.\n\n\
            Use --field to extract specific values for scripts.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("service", "Service name to query")
                .zen("<service>")
                .required(),
            ArgSpec::option("output", "Output format: human (default) or json")
                .zen("--output <format>"),
            ArgSpec::option("field", "Extract specific field (dot notation: connection.uri)")
                .zen("--field <path>"),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Get full config (human-readable)",
                zen_syntax: Some("garden-rake config mongodb"),
                normative_syntax: Some("garden-rake services config mongodb"),
            },
            CommandExample {
                description: "Get config as JSON",
                zen_syntax: Some("garden-rake config mongodb --output json"),
                normative_syntax: Some("garden-rake services config mongodb --output json"),
            },
            CommandExample {
                description: "Extract connection URI",
                zen_syntax: Some("garden-rake config mongodb --field connection.uri"),
                normative_syntax: Some(
                    "garden-rake services config mongodb --field connection.uri",
                ),
            },
            CommandExample {
                description: "Extract port number",
                zen_syntax: Some("garden-rake config mongodb --field connection.port"),
                normative_syntax: Some(
                    "garden-rake services config mongodb --field connection.port",
                ),
            },
        ],
        see_also: vec!["find", "list", "status"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    manifest.add(CommandDef {
        name: "adopted",
        zen_name: "adopted",
        zen_aliases: &[],
        normative_name: Some("adoption list"),
        category: CommandCategory::Adoption,
        description: "List adopted services",
        long_description:
            "List all services currently adopted (external services under moss management).\n\n\
            Adopted services are not containers - they're external services moss monitors.",
        remote_capable: true,
        args: vec![at_arg()],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "List adopted services",
                zen_syntax: Some("garden-rake adopted"),
                normative_syntax: Some("garden-rake adoption list"),
            },
            CommandExample {
                description: "List adopted on specific stone",
                zen_syntax: Some("garden-rake adopted on stone-01"),
                normative_syntax: Some("garden-rake adoption list --at stone-01"),
            },
        ],
        see_also: vec!["adopt", "release", "borrowed"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    manifest.add(CommandDef {
        name: "borrowed",
        zen_name: "borrowed",
        zen_aliases: &[],
        normative_name: Some("adoption list-borrowed"),
        category: CommandCategory::Adoption,
        description: "List borrowed (external) services",
        long_description:
            "List all borrowed services (external network services registered for reference).\n\n\
            Borrowed services are external services not managed by this stone,\n\
            but registered for service discovery and reference.",
        remote_capable: true,
        args: vec![at_arg()],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "List borrowed services",
                zen_syntax: Some("garden-rake borrowed"),
                normative_syntax: Some("garden-rake adoption list-borrowed"),
            },
            CommandExample {
                description: "List borrowed on specific stone",
                zen_syntax: Some("garden-rake borrowed on stone-01"),
                normative_syntax: Some("garden-rake adoption list-borrowed --at stone-01"),
            },
        ],
        see_also: vec!["borrow", "return", "adopted"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    manifest.add(CommandDef {
        name: "borrow",
        zen_name: "borrow",
        zen_aliases: &[],
        normative_name: Some("adoption borrow"),
        category: CommandCategory::Adoption,
        description: "Register an external service",
        long_description:
            "Register an external (borrowed) service for reference and discovery.\n\n\
            Borrowed services are external network services not managed by this stone.\n\
            They're registered so other services can discover and connect to them.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("name", "Name for this borrowed service")
                .zen("<name>")
                .required(),
            ArgSpec::option("from", "URL/connection string for the external service")
                .zen("from <url>")
                .normative("--url <url>")
                .required(),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Borrow external Redis",
                zen_syntax: Some("garden-rake borrow redis from redis://cache.corp:6379"),
                normative_syntax: Some(
                    "garden-rake adoption borrow redis --url redis://cache.corp:6379",
                ),
            },
            CommandExample {
                description: "Borrow external PostgreSQL",
                zen_syntax: Some("garden-rake borrow prod-db from postgres://db.corp:5432/main"),
                normative_syntax: Some(
                    "garden-rake adoption borrow prod-db --url postgres://db.corp:5432/main",
                ),
            },
            CommandExample {
                description: "Borrow on specific stone",
                zen_syntax: Some("garden-rake borrow redis from redis://cache:6379 on stone-01"),
                normative_syntax: Some(
                    "garden-rake adoption borrow redis --url redis://cache:6379 --at stone-01",
                ),
            },
        ],
        see_also: vec!["return", "borrowed"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    manifest.add(CommandDef {
        name: "return",
        zen_name: "return",
        zen_aliases: &[],
        normative_name: Some("adoption unborrow"),
        category: CommandCategory::Adoption,
        description: "Unregister a borrowed service",
        long_description:
            "Unregister a borrowed service (doesn't affect the external service).\n\n\
            Removes the service from moss's borrowed registry.\n\
            The external service continues running unaffected.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("name", "Name of the borrowed service to return")
                .zen("<name>")
                .required(),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Return (unregister) borrowed service",
                zen_syntax: Some("garden-rake return redis"),
                normative_syntax: Some("garden-rake adoption unborrow redis"),
            },
            CommandExample {
                description: "Return on specific stone",
                zen_syntax: Some("garden-rake return redis on stone-01"),
                normative_syntax: Some("garden-rake adoption unborrow redis --at stone-01"),
            },
        ],
        see_also: vec!["borrow", "borrowed"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    manifest.add(CommandDef {
        name: "status",
        zen_name: "status",
        zen_aliases: &["touch"],
        normative_name: Some("services status"),
        category: CommandCategory::Discovery,
        description: "Show service status",
        long_description: "Show detailed status of a specific service.\n\n\
            Includes health, ports, resource usage, and recent events.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("service", "Service name to query")
                .zen("<service>")
                .required(),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Show MongoDB status",
                zen_syntax: Some("garden-rake status mongodb"),
                normative_syntax: Some("garden-rake services status mongodb"),
            },
            CommandExample {
                description: "Show status on specific stone",
                zen_syntax: Some("garden-rake status mongodb on stone-01"),
                normative_syntax: Some("garden-rake services status mongodb --at stone-01"),
            },
        ],
        see_also: vec!["list", "observe"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    // === MANAGEMENT COMMANDS ===

    manifest.add(CommandDef {
        name: "tend",
        zen_name: "tend",
        zen_aliases: &[],
        normative_name: None,
        category: CommandCategory::Management,
        description: "Set which stone rake commands target",
        long_description: "Manage which stone garden-rake commands target.\n\n\
            Tending establishes a context that persists for 90 seconds and affects all subsequent commands.\n\
            Commands with --at/at will override the tended context temporarily.",
        remote_capable: false,
        args: vec![
            ArgSpec::positional("target", "'this', 'local', 'auto', or explicit endpoint URL")
                .zen("[target]"),
            ArgSpec::flag("clear", "Clear tending state").zen("--clear"),
            ArgSpec::count("verbose", "Show verbose tending information")
                .zen("-v / --verbose")
                .short('v'),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Show current tending state",
                zen_syntax: Some("garden-rake tend"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Tend to localhost",
                zen_syntax: Some("garden-rake tend this"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Auto-discover and tend to nearest stone",
                zen_syntax: Some("garden-rake tend auto"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Tend to explicit endpoint",
                zen_syntax: Some("garden-rake tend http://192.168.1.108:7185"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Stop tending (clear state)",
                zen_syntax: Some("garden-rake tend --clear"),
                normative_syntax: None,
            },
        ],
        see_also: vec!["observe", "watch"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::Ignore,
    });

    manifest.add(CommandDef {
        name: "reconcile",
        zen_name: "reconcile",
        zen_aliases: &[],
        normative_name: None,
        category: CommandCategory::Management,
        description: "Adopt existing containers",
        long_description:
            "Force moss to reconcile its registry with existing zen-offering containers.\n\n\
            Useful after moss restart/update, or if containers were created externally.\n\
            Can optionally remove invalid zen-offering-* containers.",
        remote_capable: true,
        args: vec![
            ArgSpec::flag("drop-invalid", "Remove invalid zen-offering-* containers")
                .zen("--drop-invalid"),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Adopt any missing containers",
                zen_syntax: Some("garden-rake reconcile"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Reconcile and remove invalid containers",
                zen_syntax: Some("garden-rake reconcile --drop-invalid"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Reconcile specific stone",
                zen_syntax: Some("garden-rake reconcile at stone-01"),
                normative_syntax: Some("garden-rake reconcile --at stone-01"),
            },
        ],
        see_also: vec!["nourish", "refresh"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    manifest.add(CommandDef {
        name: "refresh",
        zen_name: "refresh",
        zen_aliases: &[],
        normative_name: None,
        category: CommandCategory::Management,
        description: "Update moss or rake binary",
        long_description:
            "Update garden-moss or garden-rake binary on a stone (development use).\n\n\
            Binary is validated for architecture compatibility before installation.\n\
            Garden-Moss automatically restarts after update.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("component", "'moss' or 'rake'")
                .zen("<component>")
                .required(),
            ArgSpec::option("from", "Path to binary file")
                .zen("--from <path>")
                .required(),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Update moss binary",
                zen_syntax: Some("garden-rake refresh moss --from ./target/release/garden-moss"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Update rake binary",
                zen_syntax: Some("garden-rake refresh rake --from ./dist/linux-x64/garden-rake"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Update moss on specific stone",
                zen_syntax: Some("garden-rake refresh moss --from ./garden-moss at stone-01"),
                normative_syntax: Some(
                    "garden-rake refresh moss --from ./garden-moss --at stone-01",
                ),
            },
        ],
        see_also: vec!["reconcile"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    // === SYSTEM COMMANDS ===

    manifest.add(CommandDef {
        name: "take-root",
        zen_name: "take-root",
        zen_aliases: &[],
        normative_name: Some("install-service"),
        category: CommandCategory::System,
        description: "Install moss as a system service",
        long_description: "Install moss as a Windows system service (zen: take-root).\n\n\
            The stone will install itself as a system service and start automatically.\n\
            Requires administrator privileges on the target Windows machine.\n\
            If running from removable media (USB), automatically copies to C:\\ProgramData\\ZenGarden.\n\n\
            To uninstall: sc delete ZenGardenMoss",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("at_keyword", "Zen keyword 'at' (optional)"),
            ArgSpec::positional("stone", "Stone name for remote installation"),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Install service on tended stone",
                zen_syntax: Some("garden-rake take-root"),
                normative_syntax: Some("garden-rake install-service"),
            },
            CommandExample {
                description: "Install service on specific stone",
                zen_syntax: Some("garden-rake take-root at windows-01"),
                normative_syntax: Some("garden-rake install-service --at windows-01"),
            },
            CommandExample {
                description: "Local installation (on Windows machine running moss)",
                zen_syntax: Some("garden-moss take-root"),
                normative_syntax: Some("garden-moss install-service"),
            },
            CommandExample {
                description: "Verify service installation",
                zen_syntax: Some("sc query ZenGardenMoss"),
                normative_syntax: None,
            },
        ],
        see_also: vec!["lift", "make"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    manifest.add(CommandDef {
        name: "make",
        zen_name: "make",
        zen_aliases: &[],
        normative_name: None,
        category: CommandCategory::System,
        description: "Control stone console output",
        long_description: "Control stone console output verbosity.\n\n\
            Modes:\n\
            silent       - No console output (systemd/service use)\n\
            minimal      - Critical events only\n\
            informative  - Major lifecycle events (default)\n\
            verbose      - Full debug output (sing mode)",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("target", "'stone'")
                .zen("<target>")
                .required(),
            at_arg_global(),
        ],
        subcommands: vec![
            SubDef {
                name: "sing",
                description: "Enable verbose output",
                args: vec![
                    ArgSpec::flag("forever", "Enable verbose output permanently (no timeout)")
                        .zen("forever"),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "quiet",
                description: "Reset to default (informative) output",
                args: vec![],
                subcommands: vec![],
            },
            SubDef {
                name: "silent",
                description: "Disable console output",
                args: vec![],
                subcommands: vec![],
            },
            SubDef {
                name: "minimal",
                description: "Critical events only",
                args: vec![],
                subcommands: vec![],
            },
        ],
        examples: vec![
            CommandExample {
                description: "Enable verbose output (30min timeout)",
                zen_syntax: Some("garden-rake make stone sing"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Enable verbose output permanently",
                zen_syntax: Some("garden-rake make stone sing forever"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Reset to default (informative)",
                zen_syntax: Some("garden-rake make stone quiet"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Disable console output",
                zen_syntax: Some("garden-rake make stone silent"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Control specific stone output",
                zen_syntax: Some("garden-rake make stone sing at stone-01"),
                normative_syntax: Some("garden-rake make stone sing --at stone-01"),
            },
        ],
        see_also: vec!["watch", "take-root"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    // === POND COMMANDS ===

    manifest.add(CommandDef {
        name: "pond",
        zen_name: "pond",
        zen_aliases: &[],
        normative_name: None,
        category: CommandCategory::Pond,
        description: "Manage pond security network",
        long_description: "Manage multi-stone pond security network.\n\n\
            Pond security enables encrypted trust relationships between stones.\n\
            Subcommands: init, status, invite, join, remove, untrust.\n\
            Phase 3b feature - implementation pending.",
        remote_capable: true,
        args: vec![at_arg_global()],
        subcommands: vec![
            SubDef {
                name: "init",
                description: "Initialize pond security",
                args: vec![
                    ArgSpec::option("passphrase", "Encrypt pond certificate")
                        .zen("--passphrase <pass>"),
                    ArgSpec::option("profile", "Pond security profile")
                        .zen("--profile <name>"),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "status",
                description: "Show pond status",
                args: vec![],
                subcommands: vec![],
            },
            SubDef {
                name: "invite",
                description: "Generate invitation code",
                args: vec![
                    ArgSpec::option("passphrase", "Passphrase to protect the invitation")
                        .zen("--passphrase <pass>"),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "join",
                description: "Join pond with invitation code",
                args: vec![
                    ArgSpec::positional("code", "Invitation code")
                        .zen("<code>")
                        .required(),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "enroll",
                description: "Enroll a stone into the pond",
                args: vec![
                    ArgSpec::positional("stone", "Stone to enroll")
                        .zen("<stone>")
                        .required(),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "trust",
                description: "Trust a stone in the pond",
                args: vec![
                    ArgSpec::positional("stone", "Stone to trust")
                        .zen("<stone>")
                        .required(),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "unlock",
                description: "Unlock pond certificate",
                args: vec![
                    ArgSpec::option("passphrase", "Certificate passphrase")
                        .zen("--passphrase <pass>"),
                    ArgSpec::option("totp", "TOTP code for two-factor unlock")
                        .zen("--totp <code>"),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "remove",
                description: "Remove a stone from the pond",
                args: vec![
                    ArgSpec::positional("stone", "Stone to remove")
                        .zen("<stone>")
                        .required(),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "untrust",
                description: "Revoke trust for a stone",
                args: vec![
                    ArgSpec::positional("stone", "Stone to untrust")
                        .zen("<stone>")
                        .required(),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "promote",
                description: "Promote a stone to keystone",
                args: vec![
                    ArgSpec::positional("stone", "Stone to promote")
                        .zen("<stone>")
                        .required(),
                    ArgSpec::option("passphrase", "Passphrase for keystone promotion")
                        .zen("--passphrase <pass>"),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "rename",
                description: "Rename the pond",
                args: vec![
                    ArgSpec::positional("name", "New pond name")
                        .zen("<name>")
                        .required(),
                ],
                subcommands: vec![],
            },
        ],
        examples: vec![
            CommandExample {
                description: "Initialize pond security",
                zen_syntax: Some("garden-rake pond init"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Show pond status",
                zen_syntax: Some("garden-rake pond status"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Generate invitation code",
                zen_syntax: Some("garden-rake pond invite"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Join pond with code",
                zen_syntax: Some("garden-rake pond join ABC123"),
                normative_syntax: None,
            },
        ],
        see_also: vec!["place", "invite", "lift"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    manifest.add(CommandDef {
        name: "place",
        zen_name: "place",
        zen_aliases: &[],
        normative_name: Some("pond init / pond join"),
        category: CommandCategory::Pond,
        description: "Initialize pond or join pond",
        long_description:
            "Initialize pond (place keystone) or join existing pond (place stone).\n\n\
            Pond security enables multi-stone trust relationships with encrypted certificates.\n\
            Phase 3b feature - implementation pending.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("target", "'keystone' or 'stone'")
                .zen("<target>")
                .required(),
            ArgSpec::option("code", "Invitation code (required for 'stone')")
                .zen("--code <code>"),
            ArgSpec::option("passphrase", "Encrypt pond certificate (keystone only)")
                .zen("--passphrase <pass>"),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Initialize pond (place keystone)",
                zen_syntax: Some("garden-rake place keystone"),
                normative_syntax: Some("garden-rake pond init"),
            },
            CommandExample {
                description: "Initialize with passphrase",
                zen_syntax: Some("garden-rake place keystone --passphrase mypass"),
                normative_syntax: Some("garden-rake pond init --passphrase mypass"),
            },
            CommandExample {
                description: "Join pond (place stone)",
                zen_syntax: Some("garden-rake place stone --code ABC123"),
                normative_syntax: Some("garden-rake pond join ABC123"),
            },
            CommandExample {
                description: "Join pond on specific stone",
                zen_syntax: Some("garden-rake place stone --code ABC123 at stone-02"),
                normative_syntax: Some("garden-rake pond join ABC123 --at stone-02"),
            },
        ],
        see_also: vec!["invite", "lift"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    manifest.add(CommandDef {
        name: "invite",
        zen_name: "invite",
        zen_aliases: &[],
        normative_name: Some("pond invite"),
        category: CommandCategory::Pond,
        description: "Generate pond invitation code",
        long_description: "Generate pond invitation code for adding stones to pond.\n\n\
            Invitation codes expire after 24 hours or first use.\n\
            Phase 3b feature - implementation pending.",
        remote_capable: true,
        args: vec![at_arg()],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Generate invitation code",
                zen_syntax: Some("garden-rake invite"),
                normative_syntax: Some("garden-rake pond invite"),
            },
            CommandExample {
                description: "Generate code from specific keystone",
                zen_syntax: Some("garden-rake invite at stone-01"),
                normative_syntax: Some("garden-rake pond invite --at stone-01"),
            },
        ],
        see_also: vec!["place", "lift"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    manifest.add(CommandDef {
        name: "lift",
        zen_name: "lift",
        zen_aliases: &[],
        normative_name: Some("pond untrust / pond remove"),
        category: CommandCategory::Pond,
        description: "Remove stone from pond",
        long_description: "Remove a stone from pond or remove entire pond from stone.\n\n\
            Can remove specific stone (untrust) or remove keystone (destroy pond).\n\
            Phase 3b feature - implementation pending.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("target_type", "'keystone' or 'stone'")
                .zen("<type>")
                .required(),
            ArgSpec::positional("stone_name", "Stone name (required if type is 'stone')")
                .zen("[stone]"),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Remove specific stone from pond",
                zen_syntax: Some("garden-rake lift stone stone-02"),
                normative_syntax: Some("garden-rake pond untrust stone-02"),
            },
            CommandExample {
                description: "Remove pond from stone (leave pond)",
                zen_syntax: Some("garden-rake lift keystone"),
                normative_syntax: Some("garden-rake pond remove"),
            },
            CommandExample {
                description: "Untrust stone from specific keystone",
                zen_syntax: Some("garden-rake lift stone stone-02 at stone-01"),
                normative_syntax: Some("garden-rake pond untrust stone-02 --at stone-01"),
            },
        ],
        see_also: vec!["place", "invite"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    // === SCAFFOLDED COMMANDS ===
    // These commands are recognized but output placeholder messages until fully implemented

    manifest.add(CommandDef {
        name: cmd::CEREMONY,
        zen_name: "ceremony",
        zen_aliases: &[],
        normative_name: None,
        category: CommandCategory::Management,
        description: "Run guided workflows (coming soon)",
        long_description:
            "Ceremony provides guided workflows for common multi-step operations.\n\n\
            Scaffolded - implementation pending. Will include:\n\
            - ceremony bootstrap: First-time setup wizard\n\
            - ceremony migrate: Service migration workflow\n\
            - ceremony backup: Guided backup configuration",
        remote_capable: false,
        args: vec![
            ArgSpec::positional("workflow", "Workflow name (bootstrap, migrate, backup)")
                .zen("[workflow]"),
        ],
        subcommands: vec![],
        examples: vec![CommandExample {
            description: "Run bootstrap ceremony",
            zen_syntax: Some("garden-rake ceremony bootstrap"),
            normative_syntax: None,
        }],
        see_also: vec!["offer", "tend"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::Ignore,
    });

    manifest.add(CommandDef {
        name: cmd::TEMPLATE,
        zen_name: "template",
        zen_aliases: &[],
        normative_name: None,
        category: CommandCategory::Management,
        description: "Manage offering templates (coming soon)",
        long_description: "Template operations for custom offering definitions.\n\n\
            Scaffolded - implementation pending. Will include:\n\
            - template list: List available templates\n\
            - template show: Display template details\n\
            - template create: Create custom template",
        remote_capable: true,
        args: vec![at_arg_global()],
        subcommands: vec![
            SubDef {
                name: "list",
                description: "List available templates",
                args: vec![],
                subcommands: vec![],
            },
            SubDef {
                name: "show",
                description: "Display template details",
                args: vec![
                    ArgSpec::positional("name", "Template name")
                        .zen("<name>")
                        .required(),
                ],
                subcommands: vec![],
            },
        ],
        examples: vec![CommandExample {
            description: "List templates",
            zen_syntax: Some("garden-rake template list"),
            normative_syntax: None,
        }],
        see_also: vec!["offer"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    // === STONE ADMIN COMMANDS ===
    // Power management for physical stones

    manifest.add(CommandDef {
        name: cmd::ROUSE,
        zen_name: "rouse",
        zen_aliases: &[],
        normative_name: Some("admin stone wake"),
        category: CommandCategory::System,
        description: "Wake a stone via Wake-on-LAN",
        long_description: "Send a Wake-on-LAN magic packet to wake a sleeping stone.\n\n\
            Requires the stone's MAC address to be cached from previous discovery.\n\
            The stone must have WoL enabled in BIOS/UEFI and NIC configuration.\n\
            MAC addresses are preserved even when stones go offline (up to 64 offline stones, 24h retention).",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("stone", "Stone name to wake")
                .zen("<stone>")
                .required(),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Wake a stone by name",
                zen_syntax: Some("garden-rake rouse oak"),
                normative_syntax: Some("garden-rake admin stone wake oak"),
            },
            CommandExample {
                description: "Wake stone from specific moss instance",
                zen_syntax: Some("garden-rake rouse oak at cedar"),
                normative_syntax: Some("garden-rake admin stone wake oak --at cedar"),
            },
        ],
        see_also: vec!["slumber", "stir", "observe"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    manifest.add(CommandDef {
        name: cmd::SLUMBER,
        zen_name: "slumber",
        zen_aliases: &[],
        normative_name: Some("admin stone shutdown"),
        category: CommandCategory::System,
        description: "Shut down a stone (power off)",
        long_description: "Power off the target stone machine.\n\n\
            Uses systemctl poweroff on Linux and shutdown /s /t 0 on Windows.\n\
            The stone's MAC address is preserved in topology cache for future Wake-on-LAN.\n\
            If no stone is specified, operates on the tended stone.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("stone", "Stone name to shut down (omit for tended stone)")
                .zen("[stone]")
                .normative("--target <stone>"),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Shut down tended stone",
                zen_syntax: Some("garden-rake slumber"),
                normative_syntax: Some("garden-rake admin stone shutdown"),
            },
            CommandExample {
                description: "Shut down specific stone",
                zen_syntax: Some("garden-rake slumber oak"),
                normative_syntax: Some("garden-rake admin stone shutdown --target oak"),
            },
        ],
        see_also: vec!["rouse", "stir"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    manifest.add(CommandDef {
        name: cmd::STIR,
        zen_name: "stir",
        zen_aliases: &[],
        normative_name: Some("admin stone reboot"),
        category: CommandCategory::System,
        description: "Reboot a stone",
        long_description: "Restart the target stone machine.\n\n\
            Uses systemctl reboot on Linux and shutdown /r /t 0 on Windows.\n\
            If no stone is specified, operates on the tended stone.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("stone", "Stone name to reboot (omit for tended stone)")
                .zen("[stone]")
                .normative("--target <stone>"),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Reboot tended stone",
                zen_syntax: Some("garden-rake stir"),
                normative_syntax: Some("garden-rake admin stone reboot"),
            },
            CommandExample {
                description: "Reboot specific stone",
                zen_syntax: Some("garden-rake stir oak"),
                normative_syntax: Some("garden-rake admin stone reboot --target oak"),
            },
        ],
        see_also: vec!["slumber", "rouse"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    manifest.add(CommandDef {
        name: cmd::ELECTION,
        zen_name: "election",
        zen_aliases: &[],
        normative_name: None,
        category: CommandCategory::System,
        description: "Test distributed election protocol",
        long_description: "Test the distributed election protocol for garden operations.\n\n\
            Starts an election across all stones in the garden with optional criteria.\n\
            Used for testing leader selection for coordinated operations like updates.",
        remote_capable: false,
        args: vec![
            ArgSpec::positional("action", "Election action (start)")
                .zen("<action>")
                .required(),
            ArgSpec::option("election-type", "Election type (default: update_source; options: ceremony_coordinator, replica_target, backup_source)")
                .zen("--election-type <type>"),
            ArgSpec::option("criteria", "Selection criteria as BSON-style JSON")
                .zen("--criteria <json>"),
            ArgSpec::option("timeout", "Election timeout in seconds (default: 10)")
                .zen("--timeout <seconds>"),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Start election with default type (update_source)",
                zen_syntax: Some("garden-rake election start"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Start election for ceremony coordinator",
                zen_syntax: Some("garden-rake election start --election-type ceremony_coordinator"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Start election with custom timeout",
                zen_syntax: Some("garden-rake election start --election-type backup_source --timeout 20"),
                normative_syntax: None,
            },
        ],
        see_also: vec!["observe", "status"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::Ignore,
    });

    manifest.add(CommandDef {
        name: cmd::PRESENCE,
        zen_name: "presence",
        zen_aliases: &[],
        normative_name: None,
        category: CommandCategory::Discovery,
        description: "Monitor stone presence events in real-time",
        long_description: "Subscribe to stone presence protocol events via Server-Sent Events (SSE).\n\n\
            Displays real-time events from the stone including:\n\
            - Stone lifecycle: boot, shutdown, tending\n\
            - Offering state changes: up, down, maintenance\n\
            - Service events: adoption, removal\n\n\
            Optional filtering by event category (service, stone, offering, ceremony, nourishment, etc.)",
        remote_capable: true,
        args: vec![
            at_arg(),
            ArgSpec::option("categories", "Filter by event categories (comma-separated: service,stone,offering,ceremony,nourishment,firmware)")
                .zen("--categories <types>"),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Monitor all presence events",
                zen_syntax: Some("garden-rake presence"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Monitor only service and stone events",
                zen_syntax: Some("garden-rake presence --categories service,stone"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Monitor offering state changes",
                zen_syntax: Some("garden-rake presence --categories offering"),
                normative_syntax: None,
            },
        ],
        see_also: vec!["watch", "observe"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    // === Companion COMMANDS ===

    manifest.add(CommandDef {
        name: "hey",
        zen_name: "hey",
        zen_aliases: &[],
        normative_name: None,
        category: CommandCategory::Companion,
        description: "Communicate with Companions (Cricket, Firefly, etc.)",
        long_description: "Send commands to registered Zen Garden Companions.\n\n\
            Companions extend Moss with additional capabilities like audio feedback (Cricket),\n\
            LED displays (Firefly), and more. Use 'hey tell' to interact with them.\n\n\
            Rake passes commands through to Moss, which forwards them to the Companion.",
        remote_capable: true,
        args: vec![
            at_arg(),
            ArgSpec::trailing("tell", "Send command to Companion with raw arguments")
                .zen("tell <Companion> [args...]"),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "List all registered Companions",
                zen_syntax: Some("garden-rake hey tell"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Show Companion commands",
                zen_syntax: Some("garden-rake hey tell cricket?"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Change Cricket tune",
                zen_syntax: Some("garden-rake hey tell cricket select mr-robot"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Set Cricket volume",
                zen_syntax: Some("garden-rake hey tell cricket volume 50"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Send command to specific stone",
                zen_syntax: Some("garden-rake hey stone-01 tell cricket volume 80"),
                normative_syntax: None,
            },
        ],
        see_also: vec!["watch", "presence"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    // === STORAGE COMMANDS ===

    manifest.add(CommandDef {
        name: cmd::PREPARE,
        zen_name: "prepare",
        zen_aliases: &[],
        normative_name: None,
        category: CommandCategory::Storage,
        description: "Prepare a USB device as a seed bank",
        long_description: "Initialize a USB storage device as a Zen Garden seed bank.\n\n\
            Creates the required directory structure and metadata for the device\n\
            to be used as portable storage in the Zen Garden ecosystem.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("device", "Device name (e.g., sdb1)")
                .zen("<device>")
                .required(),
            ArgSpec::option("name", "Custom seed bank name").zen("--name <name>"),
            ArgSpec::flag("random", "Generate random seed bank name").zen("--random"),
            ArgSpec::option("fs", "Filesystem type (ext4, btrfs)").zen("--fs <type>"),
            ArgSpec::flag("encrypted", "Enable LUKS encryption").zen("--encrypted"),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Prepare USB device",
                zen_syntax: Some("garden-rake prepare sdb1"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Prepare with custom name",
                zen_syntax: Some("garden-rake prepare sdb1 --name my-seeds"),
                normative_syntax: None,
            },
        ],
        see_also: vec!["seed-banks", "release-seed-bank", "store"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    manifest.add(CommandDef {
        name: cmd::RELEASE_SEED_BANK,
        zen_name: "release-seed-bank",
        zen_aliases: &[],
        normative_name: None,
        category: CommandCategory::Storage,
        description: "Release a seed bank for safe removal",
        long_description: "Safely unmount a seed bank, ensuring all writes are complete\n\
            before the USB device is physically removed.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("name", "Seed bank name to release")
                .zen("<name>")
                .required(),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![CommandExample {
            description: "Release seed bank",
            zen_syntax: Some("garden-rake release-seed-bank my-seeds"),
            normative_syntax: None,
        }],
        see_also: vec!["seed-banks", "prepare"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    manifest.add(CommandDef {
        name: cmd::SEED_BANKS,
        zen_name: "seed-banks",
        zen_aliases: &[],
        normative_name: None,
        category: CommandCategory::Storage,
        description: "Show seed banks on a stone",
        long_description: "List all seed banks and eligible USB storage devices on a stone.\n\n\
            Shows both actively mounted seed banks and available devices that can be prepared.",
        remote_capable: true,
        args: vec![at_arg()],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "List seed banks",
                zen_syntax: Some("garden-rake seed-banks"),
                normative_syntax: None,
            },
            CommandExample {
                description: "List on specific stone",
                zen_syntax: Some("garden-rake seed-banks --at stone-01"),
                normative_syntax: None,
            },
        ],
        see_also: vec!["prepare", "release-seed-bank", "store"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    manifest.add(CommandDef {
        name: cmd::STORE,
        zen_name: "store",
        zen_aliases: &[],
        normative_name: None,
        category: CommandCategory::Storage,
        description: "Object storage operations on seed banks",
        long_description: "S3-compatible object storage on seed banks.\n\n\
            Provides put, get, list (ls), delete (rm), and head operations for storing\n\
            objects in seed bank buckets. Objects are stored under garden/storage/{bucket}/{key}.\n\
            Use --app to prefix keys as {app}/{bucket}/... (default: zen-garden).",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("operation", "Storage operation to perform")
                .zen("<put|get|ls|rm|head>")
                .required(),
            ArgSpec::positional("bucket", "Bucket name")
                .zen("<bucket>")
                .required(),
            ArgSpec::positional("key", "Object key (required for put/get/rm/head)")
                .zen("[key]"),
            ArgSpec::positional("file", "Local file path (source for put, destination for get)")
                .zen("[file]"),
            ArgSpec::option("prefix", "Prefix for list operations").zen("--prefix <prefix>"),
            ArgSpec::option("app", "Application namespace (default: zen-garden)")
                .zen("--app <name>"),
            ArgSpec::option("delimiter", "Delimiter for list output").zen("--delimiter <char>"),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Upload file",
                zen_syntax: Some("garden-rake store put mydata config.json ./config.json"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Download file",
                zen_syntax: Some("garden-rake store get mydata config.json ./config.json"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Print to stdout",
                zen_syntax: Some("garden-rake store get mydata config.json"),
                normative_syntax: None,
            },
            CommandExample {
                description: "List bucket contents",
                zen_syntax: Some("garden-rake store ls mydata"),
                normative_syntax: None,
            },
            CommandExample {
                description: "List with prefix",
                zen_syntax: Some("garden-rake store ls mydata --prefix logs/"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Delete object",
                zen_syntax: Some("garden-rake store rm mydata config.json"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Show object metadata",
                zen_syntax: Some("garden-rake store head mydata config.json"),
                normative_syntax: None,
            },
        ],
        see_also: vec!["seed-banks", "prepare"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    // === NURTURING (BACKUP/RESTORE) COMMANDS ===

    manifest.add(CommandDef {
        name: cmd::RESTORE,
        zen_name: "restore",
        zen_aliases: &[],
        normative_name: None,
        category: CommandCategory::Storage,
        description: "Restore an offering from backup",
        long_description: "Restore an offering from a nurturing backup.\n\n\
            Supports restoring from local A/B slots or remote seed banks.\n\
            Use --dry-run to preview without executing.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("offering", "Offering name to restore")
                .zen("<offering>")
                .required(),
            ArgSpec::trailing("source", "Source: e.g. 'from slot A' or 'from seed-bank <name>'")
                .zen("from slot A|B | from seed-bank <name>"),
            ArgSpec::flag("dry-run", "Preview without executing").zen("--dry-run"),
            ArgSpec::option("harvest-id", "Specific harvest ID (for seed bank restore)")
                .zen("--harvest-id <id>"),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Restore from current slot",
                zen_syntax: Some("garden-rake restore mongodb"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Restore from specific slot",
                zen_syntax: Some("garden-rake restore mongodb from slot A"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Restore from seed bank",
                zen_syntax: Some("garden-rake restore mongodb from seed-bank garden-data"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Dry run preview",
                zen_syntax: Some("garden-rake restore mongodb --dry-run"),
                normative_syntax: None,
            },
        ],
        see_also: vec!["nurturing", "seed-banks"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    manifest.add(CommandDef {
        name: cmd::NURTURING,
        zen_name: "nurturing",
        zen_aliases: &[],
        normative_name: None,
        category: CommandCategory::Storage,
        description: "Manage nurturing (backup) operations",
        long_description: "Manage nurturing (backup) operations for offerings.\n\n\
            View backup status, list available snapshots, and trigger backup workflows.\n\
            Nurturing provides A/B local slots plus remote seed bank replication.",
        remote_capable: true,
        args: vec![at_arg_global()],
        subcommands: vec![
            SubDef {
                name: "status",
                description: "Show backup status for offerings",
                args: vec![
                    ArgSpec::positional("offering", "Offering name (omit for all)")
                        .zen("[offering]"),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "list",
                description: "List available backups",
                args: vec![
                    ArgSpec::positional("offering", "Offering name")
                        .zen("<offering>")
                        .required(),
                    ArgSpec::flag("local", "Show only local backups").zen("--local"),
                    ArgSpec::flag("remote", "Show only remote backups").zen("--remote"),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "trigger",
                description: "Trigger backup for an offering",
                args: vec![
                    ArgSpec::positional("offering", "Offering name")
                        .zen("<offering>")
                        .required(),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "trigger-all",
                description: "Trigger backup for all offerings",
                args: vec![],
                subcommands: vec![],
            },
        ],
        examples: vec![
            CommandExample {
                description: "Show backup status for all offerings",
                zen_syntax: Some("garden-rake nurturing status"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Show detailed status for specific offering",
                zen_syntax: Some("garden-rake nurturing status mongodb"),
                normative_syntax: None,
            },
            CommandExample {
                description: "List all backups for an offering",
                zen_syntax: Some("garden-rake nurturing list mongodb"),
                normative_syntax: None,
            },
            CommandExample {
                description: "List local backups only",
                zen_syntax: Some("garden-rake nurturing list mongodb --local"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Trigger backup for an offering",
                zen_syntax: Some("garden-rake nurturing trigger mongodb"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Trigger backup for all offerings",
                zen_syntax: Some("garden-rake nurturing trigger-all"),
                normative_syntax: None,
            },
        ],
        see_also: vec!["restore", "seed-banks", "nourish"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    // === DEVELOPER TOOLS ===

    manifest.add(CommandDef {
        name: cmd::API,
        zen_name: "api",
        zen_aliases: &[],
        normative_name: None,
        category: CommandCategory::Discovery,
        description: "Display Moss HTTP API reference",
        long_description: "Query and display Moss HTTP API documentation.\n\n\
            Fetches live API manifest from Moss and displays formatted endpoint reference\n\
            with methods, paths, parameters, and curl examples.\n\n\
            Filter by category (health, offerings, services, stone, garden, admin) or\n\
            view detailed documentation for a specific endpoint path.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("endpoint", "Specific endpoint path to show details for (e.g., /api/v1/stone/services)")
                .zen("[endpoint]"),
            at_arg(),
            ArgSpec::option("category", "Filter by API category (health, offerings, services, stone, garden, events, admin)")
                .zen("--category <name>"),
            ArgSpec::flag("examples", "Show curl examples for each endpoint")
                .zen("--examples"),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Show all endpoints by category",
                zen_syntax: Some("garden-rake api"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Show only offerings API",
                zen_syntax: Some("garden-rake api --category offerings"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Detailed docs for specific endpoint",
                zen_syntax: Some("garden-rake api /api/v1/stone/services"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Include curl examples",
                zen_syntax: Some("garden-rake api --examples"),
                normative_syntax: None,
            },
            CommandExample {
                description: "SSE endpoint documentation",
                zen_syntax: Some("garden-rake api /api/v1/stone/presence/stream"),
                normative_syntax: None,
            },
        ],
        see_also: vec!["hey", "observe"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    // === LOCAL UTILITY COMMANDS ===

    manifest.add(CommandDef {
        name: cmd::LAUNCH,
        zen_name: "launch",
        zen_aliases: &[],
        normative_name: None,
        category: CommandCategory::System,
        description: "Open stone portrait in browser",
        long_description: "Open the stone's portrait page in the default web browser.\n\n\
            Works on Windows, macOS, and Linux with graphical environment.\n\
            If no stone is specified, opens the tended stone's portrait.",
        remote_capable: false,
        args: vec![at_arg()],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Open tended stone's portrait",
                zen_syntax: Some("garden-rake launch"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Open specific stone's portrait",
                zen_syntax: Some("garden-rake launch at stone-01"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Open by endpoint",
                zen_syntax: Some("garden-rake launch --at http://192.168.1.100:7185"),
                normative_syntax: None,
            },
        ],
        see_also: vec!["observe", "status", "tend"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::ToAtFlag,
    });

    manifest.add(CommandDef {
        name: cmd::COMMANDS,
        zen_name: "commands",
        zen_aliases: &[],
        normative_name: None,
        category: CommandCategory::System,
        description: "Browse command directory",
        long_description:
            "Browse the command directory with descriptions, examples, and syntax.\n\n\
            This is a meta-command that displays information about available commands.\n\
            Filter by category or view detailed help for specific commands.\n\n\
            Categories:\n\
            - discovery: Commands for exploring the garden and finding services\n\
            - lifecycle: Commands for managing service state\n\
            - management: Commands for garden administration\n\
            - system: Commands for stone and system operations\n\
            - pond: Commands for distributed operations",
        remote_capable: false,
        args: vec![
            ArgSpec::positional("name", "Specific command to show detailed help for")
                .zen("[command-name]"),
            ArgSpec::option("category", "Filter by category (discovery, lifecycle, management, system, pond)")
                .zen("--category <name>"),
            ArgSpec::flag("zen", "Show only zen syntax").zen("--zen"),
            ArgSpec::flag("normative", "Show only normative syntax").zen("--normative"),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Show all commands by category",
                zen_syntax: Some("garden-rake commands"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Show detailed help for a command",
                zen_syntax: Some("garden-rake commands take-root"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Filter by category",
                zen_syntax: Some("garden-rake commands --category system"),
                normative_syntax: None,
            },
            CommandExample {
                description: "Show zen syntax only",
                zen_syntax: Some("garden-rake commands --zen"),
                normative_syntax: None,
            },
        ],
        see_also: vec!["api", "launch"],
        hidden: false,
        subcommand_negates_reqs: false,
        on_stone_mapping: OnStoneMapping::Ignore,
    });

    manifest
});

/// Validate that the manifest contains expected commands
/// This is called at startup in debug builds to catch inconsistencies
#[cfg(debug_assertions)]
pub fn validate_manifest() {
    let expected_commands = vec![
        // Discovery
        "observe",
        "watch",
        "list",
        "status",
        "find",
        "presence",
        "config",
        // Lifecycle
        "offer",
        "rest",
        "wake",
        "remove",
        "uproot",
        "nourish",
        "upgrade",
        "capabilities",
        // Adoption
        "adopt",
        "release",
        "locate",
        "adopted",
        "borrowed",
        "borrow",
        "return",
        // Management
        "tend",
        "reconcile",
        "refresh",
        "ceremony",
        "template",
        // System
        "take-root",
        "make",
        // Stone admin (power management)
        "rouse",
        "slumber",
        "stir",
        // Pond
        "pond",
        "place",
        "invite",
        "lift",
        // Test/Diagnostic
        "election",
        // Companions
        "hey",
        // Developer Tools
        "api",
        // Storage
        "prepare",
        "release-seed-bank",
        "seed-banks",
        "store",
        // Nurturing
        "restore",
        "nurturing",
        // Local utility
        "launch",
        "commands",
    ];

    for cmd_name in expected_commands {
        assert!(
            MANIFEST.get(cmd_name).is_some(),
            "Command '{}' missing from manifest",
            cmd_name
        );
    }

    println!(
        "✓ Command manifest validated: {} commands registered",
        MANIFEST.all().len()
    );
}
