//! Command manifest system for Zen Garden Rake
//!
//! Single source of truth for ALL command metadata: identity, arguments,
//! examples, descriptions, and relationships. The Clap CLI is generated
//! from this manifest via the builder API (see cli_build.rs).
//!
//! Adding a new command requires exactly TWO changes:
//! 1. Add a CommandDef entry here
//! 2. Add a handler in commands/ and wire it in route.rs

use crate::arg_spec::{at_arg, at_arg_global, yes_flag, ArgSpec, SubDef};
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

    // Monitoring
    pub const PULSE: &str = "pulse";

    // Test/Diagnostic
    pub const ELECTION: &str = "election";
    pub const PRESENCE: &str = "presence";

    // Companions
    pub const HEY: &str = "hey";

    // Developer Tools
    pub const API: &str = "api";

    // Manifest authoring
    pub const MANIFEST_CMD: &str = "manifest";

    // Storage
    pub const STORAGE: &str = "storage";
    pub const STORE: &str = "store";

    // Nurturing (Backup/Restore)
    pub const RESTORE: &str = "restore";
    pub const NURTURING: &str = "nurturing";
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
    /// Storage commands: storage, store
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
    pub syntax: &'static str,
}

#[derive(Debug, Clone)]
pub struct CommandDef {
    /// Primary command name (used for lookup and Clap subcommand name)
    pub name: &'static str,
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

    /// Find a command by name. Used by help query syntax (?command / command?).
    pub fn find_by_name(&self, name: &str) -> Option<&CommandDef> {
        self.commands.get(name)
    }
}

/// Global command manifest - initialized at program start
pub static MANIFEST: Lazy<CommandManifest> = Lazy::new(|| {
    let mut manifest = CommandManifest::new();

    // === DISCOVERY COMMANDS ===

    manifest.add(CommandDef {
        name: "observe",
        category: CommandCategory::Discovery,
        description: "View garden state snapshot",
        long_description: "Observe garden state with optional filtering.\n\n\
            Shows current state of all stones and their offerings in a formatted table.\n\
            Provides snapshot view of the entire garden or filtered by stone/offering.",
        remote_capable: false,
        args: vec![
            ArgSpec::positional("stone", "Filter by specific stone name"),
            ArgSpec::option("offering", "Filter by offering name (comma-separated)"),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Observe all stones in garden",
                syntax: "garden-rake observe",
            },
            CommandExample {
                description: "Observe specific stone with all offerings",
                syntax: "garden-rake observe stone-01",
            },
            CommandExample {
                description: "Filter by specific offerings across all stones",
                syntax: "garden-rake observe --offering mongodb,redis",
            },
            CommandExample {
                description: "Observe stone with offering filter",
                syntax: "garden-rake observe stone-01 --offering mongodb",
            },
        ],
        see_also: vec!["watch", "list", "pulse"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: "pulse",
        category: CommandCategory::Discovery,
        description: "Live terminal monitor for stone observability",
        long_description: "Permanent, unattended terminal display for stone vitals.\n\n\
            Shows live gauges (CPU, MEM, DSK, GPU, NET), offering status, and a scrolling \
            event feed. Adapts to any terminal size. Designed for dedicated screens \
            (tty1, OLED sidecar, wall monitor). Reconnects automatically on stone restart.\n\n\
            Exit with Ctrl+C.",
        remote_capable: true,
        args: vec![
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Monitor tended stone",
                syntax: "garden-rake pulse",
            },
            CommandExample {
                description: "Monitor a specific stone",
                syntax: "garden-rake pulse on stone-crystal-forest",
            },
        ],
        see_also: vec!["observe", "watch"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: "watch",
        category: CommandCategory::Discovery,
        description: "Stream real-time events from stone",
        long_description: "Stream real-time events from moss operations.\n\n\
            Watch provides live updates on container lifecycle, offering installations, and system events.\n\
            Can monitor general events or specific offering logs.",
        remote_capable: true,
        args: vec![
            at_arg_global(),
            ArgSpec::option("until", "Exit when string appears in event stream"),
        ],
        subcommands: vec![
            SubDef {
                name: "offering",
                description: "Watch offering events",
                args: vec![
                    ArgSpec::positional("name", "Offering name")
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
                syntax: "garden-rake watch",
            },
            CommandExample {
                description: "Watch specific stone until completion",
                syntax: "garden-rake watch at stone-01 until 'completed'",
            },
            CommandExample {
                description: "Watch with explicit endpoint",
                syntax: "garden-rake watch at http://192.168.1.108:7185",
            },
            CommandExample {
                description: "Watch offering logs",
                syntax: "garden-rake watch offering mongodb logs",
            },
        ],
        see_also: vec!["observe", "make"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: "list",
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
                syntax: "garden-rake list",
            },
            CommandExample {
                description: "List services on specific stone",
                syntax: "garden-rake list at stone-01",
            },
        ],
        see_also: vec!["observe", "status"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    // === LIFECYCLE COMMANDS ===

    manifest.add(CommandDef {
        name: "offer",
        category: CommandCategory::Lifecycle,
        description: "Install or list offerings",
        long_description: "Manage offerings (services) - list available offerings or install specific ones.\n\n\
            Offerings are validated container templates. Installation includes compatibility checks,\n\
            hardware requirements validation, and automatic fallback recommendations.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("offering", "Offering name (omit to list all)"),
            at_arg_global(),
            ArgSpec::multi_option("prefer", "Bias recommendations (e.g., ssd, nvme)")
                .delimiter(','),
            ArgSpec::flag("anywhere-on-fail", "Fall back to any available stone if target fails"),
            ArgSpec::option("placement-mode", "Placement strategy (interactive, auto)"),
        ],
        subcommands: vec![
            SubDef {
                name: "info",
                description: "Show offering details and compatibility",
                args: vec![],
                subcommands: vec![],
            },
            SubDef {
                name: "image",
                description: "Deploy a Docker image directly (without manifest)",
                args: vec![
                    ArgSpec::positional("image-ref", "Docker image reference (e.g., nginx:latest)")
                        .required(),
                    ArgSpec::option("instance", "Named instance (e.g., staging)"),
                    ArgSpec::flag("info-only", "Inspect image without deploying"),
                ],
                subcommands: vec![],
            },
        ],
        examples: vec![
            CommandExample {
                description: "List all available offerings by category",
                syntax: "garden-rake offer",
            },
            CommandExample {
                description: "Install offering on tended stone",
                syntax: "garden-rake offer mongodb",
            },
            CommandExample {
                description: "Install on specific stone with hardware preference",
                syntax: "garden-rake offer mongodb at stone-01 --prefer ssd",
            },
            CommandExample {
                description: "Show offering details and compatibility",
                syntax: "garden-rake offer mongodb info",
            },
            CommandExample {
                description: "Deploy a Docker image directly",
                syntax: "garden-rake offer image nginx:latest",
            },
            CommandExample {
                description: "Inspect a Docker image without deploying",
                syntax: "garden-rake offer image nginx:latest --info",
            },
            CommandExample {
                description: "Install with automatic fallback to any stone",
                syntax: "garden-rake offer mongodb --anywhere-on-fail",
            },
        ],
        see_also: vec!["release", "list"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: "rest",
        category: CommandCategory::Lifecycle,
        description: "Stop a service (rest mode)",
        long_description: "Stop a running service without removing it.\n\n\
            Service enters rest mode and can be woken later with all data preserved.\n\
            Container is stopped but not removed.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("service", "Service name to stop")
                .required(),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Put service to rest on tended stone",
                syntax: "garden-rake rest mongodb",
            },
            CommandExample {
                description: "Put service to rest on specific stone",
                syntax: "garden-rake rest mongodb at stone-01",
            },
        ],
        see_also: vec!["wake", "release"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: "wake",
        category: CommandCategory::Lifecycle,
        description: "Start a service (wake from rest)",
        long_description: "Start a service that is in rest mode.\n\n\
            Service resumes with all previous data and configuration intact.\n\
            Container is started from stopped state.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("service", "Service name to start")
                .required(),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Wake service on tended stone",
                syntax: "garden-rake wake mongodb",
            },
            CommandExample {
                description: "Wake service on specific stone",
                syntax: "garden-rake wake mongodb at stone-01",
            },
        ],
        see_also: vec!["rest", "offer"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: "remove",
        category: CommandCategory::Lifecycle,
        description: "Remove service from registry (soft delete)",
        long_description:
            "Remove a service from moss registry without destroying the container.\n\n\
            The container becomes a 'stray' - still running but unmanaged.\n\
            Use 'uproot' for hard delete (destroy container and data).",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("service", "Service name to remove from registry")
                .required(),
            at_arg(),
            yes_flag(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Remove service (stops and removes container, preserves volumes)",
                syntax: "garden-rake remove mongodb",
            },
            CommandExample {
                description: "Remove service on specific stone",
                syntax: "garden-rake remove mongodb on stone-01",
            },
        ],
        see_also: vec!["uproot", "adopt", "find"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: "uproot",
        category: CommandCategory::Lifecycle,
        description: "Destroy service completely (hard delete)",
        long_description: "Permanently destroy a service including container and data.\n\n\
            This is irreversible - container and volumes are deleted.\n\
            Use --force to skip confirmation prompt.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("service", "Service name to destroy")
                .required(),
            yes_flag(),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Destroy service with confirmation",
                syntax: "garden-rake uproot mongodb",
            },
            CommandExample {
                description: "Destroy service without confirmation",
                syntax: "garden-rake uproot mongodb --force",
            },
            CommandExample {
                description: "Destroy service on specific stone",
                syntax: "garden-rake uproot mongodb on stone-01",
            },
        ],
        see_also: vec!["remove", "rest"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: "nourish",
        category: CommandCategory::Lifecycle,
        description: "Upgrade service to latest version",
        long_description: "Upgrade one or all services to their latest versions.\n\n\
            Pulls latest container images and recreates services with data preserved.\n\
            Use --all to upgrade all services on stone.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("service", "Service name (omit with --all)"),
            ArgSpec::flag("all", "Upgrade all services on stone"),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Upgrade specific service",
                syntax: "garden-rake nourish mongodb",
            },
            CommandExample {
                description: "Upgrade all services on stone",
                syntax: "garden-rake nourish --all",
            },
            CommandExample {
                description: "Upgrade service on specific stone",
                syntax: "garden-rake nourish mongodb at stone-01",
            },
        ],
        see_also: vec!["offer", "reconcile"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: cmd::UPGRADE,
        category: CommandCategory::Lifecycle,
        description: "Upgrade a service to latest image",
        long_description: "Upgrade a service container to the latest available image.\n\n\
            This pulls the latest image and recreates the container with the same configuration.\n\
            Use --all to upgrade all services on the stone.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("service", "Service name to upgrade (required unless --all)"),
            ArgSpec::flag("all", "Upgrade all services"),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Upgrade specific service",
                syntax: "garden-rake upgrade mongodb",
            },
            CommandExample {
                description: "Upgrade all services",
                syntax: "garden-rake upgrade --all",
            },
        ],
        see_also: vec!["nourish", "offer", "rest"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: cmd::CAPABILITIES,
        category: CommandCategory::Lifecycle,
        description: "Manage offering capabilities (models, extensions)",
        long_description:
            "Manage capabilities for an offering such as AI models or database extensions.\n\n\
            For AI offerings like Ollama, this manages available models.\n\
            For databases, this could manage extensions or plugins.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("offering", "Offering name to manage capabilities for")
                .required(),
            at_arg_global(),
        ],
        subcommands: vec![
            SubDef {
                name: "add",
                description: "Add a capability to an offering",
                args: vec![
                    ArgSpec::positional("offering", "Offering name")
                        .required(),
                    ArgSpec::positional("name", "Capability name to add")
                        .required(),
                    ArgSpec::option("type", "Capability type")
                        .short('t'),
                    ArgSpec::flag("dry-run", "Validate only without adding"),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "remove",
                description: "Remove a capability from an offering",
                args: vec![
                    ArgSpec::positional("offering", "Offering name")
                        .required(),
                    ArgSpec::positional("name", "Capability name to remove")
                        .required(),
                    ArgSpec::option("type", "Capability type")
                        .short('t'),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "refresh",
                description: "Refresh capabilities for an offering",
                args: vec![
                    ArgSpec::positional("offering", "Offering name")
                        .required(),
                    ArgSpec::option("type", "Capability type")
                        .short('t'),
                    ArgSpec::flag("dry-run", "Validate only without refreshing"),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "mirror",
                description: "Mirror capabilities between stones",
                args: vec![
                    ArgSpec::positional("offering", "Offering name")
                        .required(),
                    ArgSpec::trailing("args", "Mirror arguments (from <stone> to <stone>)"),
                ],
                subcommands: vec![],
            },
        ],
        examples: vec![
            CommandExample {
                description: "List capabilities for ollama",
                syntax: "garden-rake capabilities ollama",
            },
            CommandExample {
                description: "Add a model to ollama",
                syntax: "garden-rake capabilities add ollama llama3",
            },
            CommandExample {
                description: "Remove a model",
                syntax: "garden-rake capabilities remove ollama phi",
            },
            CommandExample {
                description: "Mirror capabilities between stones",
                syntax: "garden-rake capabilities ollama mirror --from stone-01 --to stone-02",
            },
        ],
        see_also: vec!["offer", "status"],
        hidden: false,
        subcommand_negates_reqs: true,
    });

    // === ADOPTION COMMANDS ===

    manifest.add(CommandDef {
        name: "adopt",
        category: CommandCategory::Adoption,
        description: "Adopt a stray container or detected service",
        long_description:
            "Claim an existing container or detected service into moss management.\n\n\
            Strays are containers that exist but aren't in moss registry.\n\
            Adopted services are external services (not containers) that moss monitors.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("target", "Container name or offering name to claim")
                .required(),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Adopt a stray container",
                syntax: "garden-rake adopt my-mongodb",
            },
            CommandExample {
                description: "Adopt offering on specific stone",
                syntax: "garden-rake adopt mongodb on stone-01",
            },
        ],
        see_also: vec!["release", "find", "adopted"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: "release",
        category: CommandCategory::Adoption,
        description: "Release an adopted service from management",
        long_description: "Release an adopted service from moss management.\n\n\
            The service continues running but is no longer monitored by moss.\n\
            Does not affect borrowed services - use 'return' for those.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("service", "Adopted service name to release")
                .required(),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Release adopted service",
                syntax: "garden-rake release mongodb",
            },
            CommandExample {
                description: "Release on specific stone",
                syntax: "garden-rake release mongodb on stone-01",
            },
        ],
        see_also: vec!["adopt", "adopted"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: "locate",
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
                syntax: "garden-rake locate strays",
            },
            CommandExample {
                description: "Locate strays on specific stone",
                syntax: "garden-rake locate strays on stone-01",
            },
        ],
        see_also: vec!["adopt", "adopted"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: "find",
        category: CommandCategory::Discovery,
        description: "Find running services and get connection URIs",
        long_description: "Find running services across the garden and return connection URIs.\n\n\
            Supports search by name, category (c:prefix), or tags (t:prefix).\n\
            Results are returned instantly from topology cache.\n\n\
            Use 'wishfully' modifier to auto-provision if service not found.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("query", "Service name, c:category, or t:tag")
                .required(),
            ArgSpec::option("format", "Output format: human, json, uri, uri-ip"),
            ArgSpec::flag("wishfully", "Auto-provision if not found"),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Find mongodb service",
                syntax: "garden-rake find mongodb",
            },
            CommandExample {
                description: "Find any database",
                syntax: "garden-rake find c:database",
            },
            CommandExample {
                description: "Get connection URI only",
                syntax: "garden-rake find mongodb --format uri",
            },
            CommandExample {
                description: "Auto-provision if not found",
                syntax: "garden-rake find mongodb wishfully",
            },
        ],
        see_also: vec!["observe", "list", "offer", "config"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: "config",
        category: CommandCategory::Discovery,
        description: "Get service configuration for automation",
        long_description: "Query detailed configuration for a service by name.\n\n\
            Designed for automation and scripting scenarios.\n\
            Returns connection URIs, ports, hostname, and protocol information.\n\n\
            Use --field to extract specific values for scripts.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("service", "Service name to query")
                .required(),
            ArgSpec::option("output", "Output format: human (default) or json"),
            ArgSpec::option("field", "Extract specific field (dot notation: connection.uri)"),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Get full config (human-readable)",
                syntax: "garden-rake config mongodb",
            },
            CommandExample {
                description: "Get config as JSON",
                syntax: "garden-rake config mongodb --output json",
            },
            CommandExample {
                description: "Extract connection URI",
                syntax: "garden-rake config mongodb --field connection.uri",
            },
            CommandExample {
                description: "Extract port number",
                syntax: "garden-rake config mongodb --field connection.port",
            },
        ],
        see_also: vec!["find", "list", "status"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: "adopted",
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
                syntax: "garden-rake adopted",
            },
            CommandExample {
                description: "List adopted on specific stone",
                syntax: "garden-rake adopted on stone-01",
            },
        ],
        see_also: vec!["adopt", "release", "borrowed"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: "borrowed",
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
                syntax: "garden-rake borrowed",
            },
            CommandExample {
                description: "List borrowed on specific stone",
                syntax: "garden-rake borrowed on stone-01",
            },
        ],
        see_also: vec!["borrow", "return", "adopted"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: "borrow",
        category: CommandCategory::Adoption,
        description: "Register an external service",
        long_description:
            "Register an external (borrowed) service for reference and discovery.\n\n\
            Borrowed services are external network services not managed by this stone.\n\
            They're registered so other services can discover and connect to them.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("name", "Name for this borrowed service")
                .required(),
            ArgSpec::option("from", "URL/connection string for the external service")
                .required(),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Borrow external Redis",
                syntax: "garden-rake borrow redis from redis://cache.corp:6379",
            },
            CommandExample {
                description: "Borrow external PostgreSQL",
                syntax: "garden-rake borrow prod-db from postgres://db.corp:5432/main",
            },
            CommandExample {
                description: "Borrow on specific stone",
                syntax: "garden-rake borrow redis from redis://cache:6379 on stone-01",
            },
        ],
        see_also: vec!["return", "borrowed"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: "return",
        category: CommandCategory::Adoption,
        description: "Unregister a borrowed service",
        long_description:
            "Unregister a borrowed service (doesn't affect the external service).\n\n\
            Removes the service from moss's borrowed registry.\n\
            The external service continues running unaffected.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("name", "Name of the borrowed service to return")
                .required(),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Return (unregister) borrowed service",
                syntax: "garden-rake return redis",
            },
            CommandExample {
                description: "Return on specific stone",
                syntax: "garden-rake return redis on stone-01",
            },
        ],
        see_also: vec!["borrow", "borrowed"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: "status",
        category: CommandCategory::Discovery,
        description: "Show service status",
        long_description: "Show detailed status of a specific service.\n\n\
            Includes health, ports, resource usage, and recent events.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("service", "Service name to query")
                .required(),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Show MongoDB status",
                syntax: "garden-rake status mongodb",
            },
            CommandExample {
                description: "Show status on specific stone",
                syntax: "garden-rake status mongodb on stone-01",
            },
        ],
        see_also: vec!["list", "observe"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    // === MANAGEMENT COMMANDS ===

    manifest.add(CommandDef {
        name: "tend",
        category: CommandCategory::Management,
        description: "Set which stone rake commands target",
        long_description: "Manage which stone garden-rake commands target.\n\n\
            Tending establishes a context that persists for 90 seconds and affects all subsequent commands.\n\
            Commands with --at/at will override the tended context temporarily.",
        remote_capable: false,
        args: vec![
            ArgSpec::positional("target", "'this', 'local', 'auto', or explicit endpoint URL"),
            ArgSpec::flag("clear", "Clear tending state"),
            ArgSpec::count("verbose", "Show verbose tending information")
                .short('v'),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Show current tending state",
                syntax: "garden-rake tend",
            },
            CommandExample {
                description: "Tend to localhost",
                syntax: "garden-rake tend this",
            },
            CommandExample {
                description: "Auto-discover and tend to nearest stone",
                syntax: "garden-rake tend auto",
            },
            CommandExample {
                description: "Tend to explicit endpoint",
                syntax: "garden-rake tend http://192.168.1.108:7185",
            },
            CommandExample {
                description: "Stop tending (clear state)",
                syntax: "garden-rake tend --clear",
            },
        ],
        see_also: vec!["observe", "watch"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: "reconcile",
        category: CommandCategory::Management,
        description: "Adopt existing containers",
        long_description:
            "Force moss to reconcile its registry with existing zen-offering containers.\n\n\
            Useful after moss restart/update, or if containers were created externally.\n\
            Can optionally remove invalid zen-offering-* containers.",
        remote_capable: true,
        args: vec![
            ArgSpec::flag("drop-invalid", "Remove invalid zen-offering-* containers"),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Adopt any missing containers",
                syntax: "garden-rake reconcile",
            },
            CommandExample {
                description: "Reconcile and remove invalid containers",
                syntax: "garden-rake reconcile --drop-invalid",
            },
            CommandExample {
                description: "Reconcile specific stone",
                syntax: "garden-rake reconcile at stone-01",
            },
        ],
        see_also: vec!["nourish", "refresh"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: "refresh",
        category: CommandCategory::Management,
        description: "Update moss or rake binary",
        long_description:
            "Update garden-moss or garden-rake binary on a stone (development use).\n\n\
            Binary is validated for architecture compatibility before installation.\n\
            Garden-Moss automatically restarts after update.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("component", "'moss' or 'rake'")
                .required(),
            ArgSpec::option("from", "Path to binary file")
                .required(),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Update moss binary",
                syntax: "garden-rake refresh moss --from ./target/release/garden-moss",
            },
            CommandExample {
                description: "Update rake binary",
                syntax: "garden-rake refresh rake --from ./dist/linux-x64/garden-rake",
            },
            CommandExample {
                description: "Update moss on specific stone",
                syntax: "garden-rake refresh moss --from ./garden-moss at stone-01",
            },
        ],
        see_also: vec!["reconcile"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    // === SYSTEM COMMANDS ===

    manifest.add(CommandDef {
        name: "take-root",
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
                syntax: "garden-rake take-root",
            },
            CommandExample {
                description: "Install service on specific stone",
                syntax: "garden-rake take-root at windows-01",
            },
            CommandExample {
                description: "Local installation (on Windows machine running moss)",
                syntax: "garden-moss take-root",
            },
            CommandExample {
                description: "Verify service installation",
                syntax: "sc query ZenGardenMoss",
            },
        ],
        see_also: vec!["lift", "make"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: "make",
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
                .required(),
            at_arg_global(),
        ],
        subcommands: vec![
            SubDef {
                name: "sing",
                description: "Enable verbose output",
                args: vec![
                    ArgSpec::flag("forever", "Enable verbose output permanently (no timeout)"),
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
                syntax: "garden-rake make stone sing",
            },
            CommandExample {
                description: "Enable verbose output permanently",
                syntax: "garden-rake make stone sing forever",
            },
            CommandExample {
                description: "Reset to default (informative)",
                syntax: "garden-rake make stone quiet",
            },
            CommandExample {
                description: "Disable console output",
                syntax: "garden-rake make stone silent",
            },
            CommandExample {
                description: "Control specific stone output",
                syntax: "garden-rake make stone sing at stone-01",
            },
        ],
        see_also: vec!["watch", "take-root"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    // === POND COMMANDS ===

    manifest.add(CommandDef {
        name: "pond",
        category: CommandCategory::Pond,
        description: "Manage pond security network",
        long_description: "Manage multi-stone pond security network.\n\n\
            Pond security enables encrypted trust relationships between stones.\n\
            Subcommands: init, status, invite, join, drain, remove, untrust.\n\
            Phase 3b feature - implementation pending.",
        remote_capable: true,
        args: vec![at_arg_global()],
        subcommands: vec![
            SubDef {
                name: "init",
                description: "Initialize pond security",
                args: vec![
                    ArgSpec::option("passphrase", "Encrypt pond certificate"),
                    ArgSpec::option("profile", "Pond security profile"),
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
                    ArgSpec::option("passphrase", "Passphrase to protect the invitation"),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "join",
                description: "Join pond with invitation code",
                args: vec![
                    ArgSpec::positional("code", "Invitation code")
                        .required(),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "enroll",
                description: "Enroll a stone into the pond",
                args: vec![
                    ArgSpec::positional("stone", "Stone to enroll")
                        .required(),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "trust",
                description: "Trust a stone in the pond",
                args: vec![
                    ArgSpec::positional("stone", "Stone to trust")
                        .required(),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "unlock",
                description: "Unlock pond certificate",
                args: vec![
                    ArgSpec::option("passphrase", "Certificate passphrase"),
                    ArgSpec::option("totp", "TOTP code for two-factor unlock"),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "drain",
                description: "Drain pond (destroy CA and all certificates)",
                args: vec![],
                subcommands: vec![],
            },
            SubDef {
                name: "remove",
                description: "Remove a stone from the pond",
                args: vec![
                    ArgSpec::positional("stone", "Stone to remove")
                        .required(),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "untrust",
                description: "Revoke trust for a stone",
                args: vec![
                    ArgSpec::positional("stone", "Stone to untrust")
                        .required(),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "promote",
                description: "Promote a stone to keystone",
                args: vec![
                    ArgSpec::positional("stone", "Stone to promote")
                        .required(),
                    ArgSpec::option("passphrase", "Passphrase for keystone promotion"),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "rename",
                description: "Rename the pond",
                args: vec![
                    ArgSpec::positional("name", "New pond name")
                        .required(),
                ],
                subcommands: vec![],
            },
        ],
        examples: vec![
            CommandExample {
                description: "Initialize pond security",
                syntax: "garden-rake pond init",
            },
            CommandExample {
                description: "Show pond status",
                syntax: "garden-rake pond status",
            },
            CommandExample {
                description: "Generate invitation code",
                syntax: "garden-rake pond invite",
            },
            CommandExample {
                description: "Join pond with code",
                syntax: "garden-rake pond join ABC123",
            },
        ],
        see_also: vec!["place", "invite", "lift"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: "place",
        category: CommandCategory::Pond,
        description: "Initialize pond or join pond",
        long_description:
            "Initialize pond (place keystone) or join existing pond (place stone).\n\n\
            Pond security enables multi-stone trust relationships with encrypted certificates.\n\
            Phase 3b feature - implementation pending.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("target", "'keystone' or 'stone'")
                .required(),
            ArgSpec::option("code", "Invitation code (required for 'stone')"),
            ArgSpec::option("passphrase", "Encrypt pond certificate (keystone only)"),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Initialize pond (place keystone)",
                syntax: "garden-rake place keystone",
            },
            CommandExample {
                description: "Initialize with passphrase",
                syntax: "garden-rake place keystone --passphrase mypass",
            },
            CommandExample {
                description: "Join pond (place stone)",
                syntax: "garden-rake place stone --code ABC123",
            },
            CommandExample {
                description: "Join pond on specific stone",
                syntax: "garden-rake place stone --code ABC123 at stone-02",
            },
        ],
        see_also: vec!["invite", "lift"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: "invite",
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
                syntax: "garden-rake invite",
            },
            CommandExample {
                description: "Generate code from specific keystone",
                syntax: "garden-rake invite at stone-01",
            },
        ],
        see_also: vec!["place", "lift"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: "lift",
        category: CommandCategory::Pond,
        description: "Remove stone from pond",
        long_description: "Remove a stone from pond or drain entire pond.\n\n\
            Can remove specific stone (untrust) or drain keystone (destroy pond).\n\
            Phase 3b feature - implementation pending.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("target_type", "'keystone' or 'stone'")
                .required(),
            ArgSpec::positional("stone_name", "Stone name (required if type is 'stone')"),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Remove specific stone from pond",
                syntax: "garden-rake lift stone stone-02",
            },
            CommandExample {
                description: "Drain pond (destroy CA)",
                syntax: "garden-rake lift keystone",
            },
            CommandExample {
                description: "Untrust stone from specific keystone",
                syntax: "garden-rake lift stone stone-02 at stone-01",
            },
        ],
        see_also: vec!["place", "invite"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    // === SCAFFOLDED COMMANDS ===
    // These commands are recognized but output placeholder messages until fully implemented

    manifest.add(CommandDef {
        name: cmd::CEREMONY,
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
            ArgSpec::positional("workflow", "Workflow name (bootstrap, migrate, backup)"),
        ],
        subcommands: vec![],
        examples: vec![CommandExample {
            description: "Run bootstrap ceremony",
            syntax: "garden-rake ceremony bootstrap",
        }],
        see_also: vec!["offer", "tend"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: cmd::TEMPLATE,
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
                        .required(),
                ],
                subcommands: vec![],
            },
        ],
        examples: vec![CommandExample {
            description: "List templates",
            syntax: "garden-rake template list",
        }],
        see_also: vec!["offer"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    // === STONE ADMIN COMMANDS ===
    // Power management for physical stones

    manifest.add(CommandDef {
        name: cmd::ROUSE,
        category: CommandCategory::System,
        description: "Wake a stone via Wake-on-LAN",
        long_description: "Send a Wake-on-LAN magic packet to wake a sleeping stone.\n\n\
            Requires the stone's MAC address to be cached from previous discovery.\n\
            The stone must have WoL enabled in BIOS/UEFI and NIC configuration.\n\
            MAC addresses are preserved even when stones go offline (up to 64 offline stones, 24h retention).",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("stone", "Stone name to wake")
                .required(),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Wake a stone by name",
                syntax: "garden-rake rouse oak",
            },
            CommandExample {
                description: "Wake stone from specific moss instance",
                syntax: "garden-rake rouse oak at cedar",
            },
        ],
        see_also: vec!["slumber", "stir", "observe"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: cmd::SLUMBER,
        category: CommandCategory::System,
        description: "Shut down a stone (power off)",
        long_description: "Power off the target stone machine.\n\n\
            Uses systemctl poweroff on Linux and shutdown /s /t 0 on Windows.\n\
            The stone's MAC address is preserved in topology cache for future Wake-on-LAN.\n\
            If no stone is specified, operates on the tended stone.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("stone", "Stone name to shut down (omit for tended stone)"),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Shut down tended stone",
                syntax: "garden-rake slumber",
            },
            CommandExample {
                description: "Shut down specific stone",
                syntax: "garden-rake slumber oak",
            },
        ],
        see_also: vec!["rouse", "stir"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: cmd::STIR,
        category: CommandCategory::System,
        description: "Reboot a stone",
        long_description: "Restart the target stone machine.\n\n\
            Uses systemctl reboot on Linux and shutdown /r /t 0 on Windows.\n\
            If no stone is specified, operates on the tended stone.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("stone", "Stone name to reboot (omit for tended stone)"),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Reboot tended stone",
                syntax: "garden-rake stir",
            },
            CommandExample {
                description: "Reboot specific stone",
                syntax: "garden-rake stir oak",
            },
        ],
        see_also: vec!["slumber", "rouse"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: cmd::ELECTION,
        category: CommandCategory::System,
        description: "Test distributed election protocol",
        long_description: "Test the distributed election protocol for garden operations.\n\n\
            Starts an election across all stones in the garden with optional criteria.\n\
            Used for testing leader selection for coordinated operations like updates.",
        remote_capable: false,
        args: vec![
            ArgSpec::positional("action", "Election action (start)")
                .required(),
            ArgSpec::option("election-type", "Election type (default: update_source; options: ceremony_coordinator, replica_target, backup_source)"),
            ArgSpec::option("criteria", "Selection criteria as BSON-style JSON"),
            ArgSpec::option("timeout", "Election timeout in seconds (default: 10)"),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Start election with default type (update_source)",
                syntax: "garden-rake election start",
            },
            CommandExample {
                description: "Start election for ceremony coordinator",
                syntax: "garden-rake election start --election-type ceremony_coordinator",
            },
            CommandExample {
                description: "Start election with custom timeout",
                syntax: "garden-rake election start --election-type backup_source --timeout 20",
            },
        ],
        see_also: vec!["observe", "status"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: cmd::PRESENCE,
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
            ArgSpec::option("categories", "Filter by event categories (comma-separated: service,stone,offering,ceremony,nourishment,firmware)"),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Monitor all presence events",
                syntax: "garden-rake presence",
            },
            CommandExample {
                description: "Monitor only service and stone events",
                syntax: "garden-rake presence --categories service,stone",
            },
            CommandExample {
                description: "Monitor offering state changes",
                syntax: "garden-rake presence --categories offering",
            },
        ],
        see_also: vec!["watch", "observe"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    // === Companion COMMANDS ===

    manifest.add(CommandDef {
        name: "hey",
        category: CommandCategory::Companion,
        description: "Communicate with Companions (Cricket, Firefly, etc.)",
        long_description: "Send commands to registered Zen Garden Companions.\n\n\
            Companions extend Moss with additional capabilities like audio feedback (Cricket),\n\
            LED displays (Firefly), and more. Use 'hey tell' to interact with them.\n\n\
            Rake passes commands through to Moss, which forwards them to the Companion.",
        remote_capable: true,
        args: vec![
            at_arg(),
            ArgSpec::trailing("tell", "Send command to Companion with raw arguments"),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "List all registered Companions",
                syntax: "garden-rake hey tell",
            },
            CommandExample {
                description: "Show Companion commands",
                syntax: "garden-rake hey tell cricket?",
            },
            CommandExample {
                description: "Change Cricket tune",
                syntax: "garden-rake hey tell cricket select mr-robot",
            },
            CommandExample {
                description: "Set Cricket volume",
                syntax: "garden-rake hey tell cricket volume 50",
            },
            CommandExample {
                description: "Send command to specific stone",
                syntax: "garden-rake hey stone-01 tell cricket volume 80",
            },
        ],
        see_also: vec!["watch", "presence"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    // === STORAGE COMMANDS ===

    manifest.add(CommandDef {
        name: cmd::STORAGE,
        category: CommandCategory::Storage,
        description: "Manage storage devices and directories",
        long_description: "Unified storage management for the Zen Garden ecosystem.\n\n\
            Run bare to list all storages across the garden. Use subcommands to add,\n\
            inspect, release, pin/unpin, or view detailed status.\n\n\
            Subcommands:\n\
            - add: Add a storage device or directory\n\
            - list: List all storages and eligible devices\n\
            - status: Detailed capacity/health breakdown\n\
            - release: Safely unmount for removal\n\
            - pin: Claim Primary role on this stone\n\
            - unpin: Release Primary role",
        remote_capable: true,
        args: vec![at_arg()],
        subcommands: vec![
            SubDef {
                name: "add",
                description: "Add a storage device or directory",
                args: vec![
                    ArgSpec::positional("target", "Device path or directory (e.g., /dev/sdb, /mnt/nas)"),
                    ArgSpec::option("name", "Storage name"),
                    ArgSpec::multi_option("roles", "Roles to assign (e.g., seed-bank)")
                        .delimiter(','),
                    ArgSpec::flag("format", "Format the device before adding"),
                    ArgSpec::option("fs", "Filesystem type when formatting (ext4, btrfs)")
                        .default("btrfs"),
                    ArgSpec::flag("encrypted", "Enable encryption (pond-scoped)"),
                    ArgSpec::flag("yes", "Skip confirmation (non-interactive)")
                        .short('y'),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "list",
                description: "List all storages and eligible devices",
                args: vec![],
                subcommands: vec![],
            },
            SubDef {
                name: "status",
                description: "Show storage capacity and health breakdown",
                args: vec![],
                subcommands: vec![],
            },
            SubDef {
                name: "release",
                description: "Safely unmount storage for removal",
                args: vec![
                    ArgSpec::positional("name", "Storage name to release (or 'all')")
                        .required(),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "pin",
                description: "Claim Primary role on this stone",
                args: vec![
                    ArgSpec::positional("name", "Storage name to pin")
                        .required(),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "unpin",
                description: "Release Primary role",
                args: vec![
                    ArgSpec::positional("name", "Storage name to unpin")
                        .required(),
                ],
                subcommands: vec![],
            },
        ],
        examples: vec![
            CommandExample {
                description: "List all storages in the garden",
                syntax: "garden-rake storage",
            },
            CommandExample {
                description: "Add a USB drive with zen syntax",
                syntax: "garden-rake storage add /dev/sdb as photos with role seed-bank",
            },
            CommandExample {
                description: "Add a NAS mount",
                syntax: "garden-rake storage add /mnt/nas-media as media",
            },
            CommandExample {
                description: "View detailed storage status",
                syntax: "garden-rake storage status",
            },
            CommandExample {
                description: "Release storage for safe removal",
                syntax: "garden-rake storage release my-seeds",
            },
            CommandExample {
                description: "Pin Primary role to this stone",
                syntax: "garden-rake storage pin photos",
            },
            CommandExample {
                description: "List storages on a specific stone",
                syntax: "garden-rake storage on stone-01",
            },
        ],
        see_also: vec!["store", "nurturing", "restore"],
        hidden: false,
        subcommand_negates_reqs: true,
    });


    manifest.add(CommandDef {
        name: cmd::STORE,
        category: CommandCategory::Storage,
        description: "Object storage operations on seed banks",
        long_description: "S3-compatible object storage on seed banks.\n\n\
            Provides put, get, list (ls), delete (rm), and head operations for storing\n\
            objects in seed bank buckets. Objects are stored under garden/storage/{bucket}/{key}.\n\
            Use --app to prefix keys as {app}/{bucket}/... (default: zen-garden).",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("operation", "Storage operation to perform")
                .required(),
            ArgSpec::positional("bucket", "Bucket name")
                .required(),
            ArgSpec::positional("key", "Object key (required for put/get/rm/head)"),
            ArgSpec::positional("file", "Local file path (source for put, destination for get)"),
            ArgSpec::option("prefix", "Prefix for list operations"),
            ArgSpec::option("app", "Application namespace (default: zen-garden)"),
            ArgSpec::option("delimiter", "Delimiter for list output"),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Upload file",
                syntax: "garden-rake store put mydata config.json ./config.json",
            },
            CommandExample {
                description: "Download file",
                syntax: "garden-rake store get mydata config.json ./config.json",
            },
            CommandExample {
                description: "Print to stdout",
                syntax: "garden-rake store get mydata config.json",
            },
            CommandExample {
                description: "List bucket contents",
                syntax: "garden-rake store ls mydata",
            },
            CommandExample {
                description: "List with prefix",
                syntax: "garden-rake store ls mydata --prefix logs/",
            },
            CommandExample {
                description: "Delete object",
                syntax: "garden-rake store rm mydata config.json",
            },
            CommandExample {
                description: "Show object metadata",
                syntax: "garden-rake store head mydata config.json",
            },
        ],
        see_also: vec!["storage"],
        hidden: false,
        subcommand_negates_reqs: false,
    });


    // === NURTURING (BACKUP/RESTORE) COMMANDS ===

    manifest.add(CommandDef {
        name: cmd::RESTORE,
        category: CommandCategory::Storage,
        description: "Restore an offering from backup",
        long_description: "Restore an offering from a nurturing backup.\n\n\
            Supports restoring from local A/B slots or remote seed banks.\n\
            Use --dry-run to preview without executing.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("offering", "Offering name to restore")
                .required(),
            ArgSpec::trailing("source", "Source: e.g. 'from slot A' or 'from seed-bank <name>'"),
            ArgSpec::flag("dry-run", "Preview without executing"),
            ArgSpec::option("harvest-id", "Specific harvest ID (for seed bank restore)"),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Restore from current slot",
                syntax: "garden-rake restore mongodb",
            },
            CommandExample {
                description: "Restore from specific slot",
                syntax: "garden-rake restore mongodb from slot A",
            },
            CommandExample {
                description: "Restore from seed bank",
                syntax: "garden-rake restore mongodb from seed-bank garden-data",
            },
            CommandExample {
                description: "Dry run preview",
                syntax: "garden-rake restore mongodb --dry-run",
            },
        ],
        see_also: vec!["nurturing", "storage"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: cmd::NURTURING,
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
                    ArgSpec::positional("offering", "Offering name (omit for all)"),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "list",
                description: "List available backups",
                args: vec![
                    ArgSpec::positional("offering", "Offering name")
                        .required(),
                    ArgSpec::flag("local", "Show only local backups"),
                    ArgSpec::flag("remote", "Show only remote backups"),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "trigger",
                description: "Trigger backup for an offering",
                args: vec![
                    ArgSpec::positional("offering", "Offering name")
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
                syntax: "garden-rake nurturing status",
            },
            CommandExample {
                description: "Show detailed status for specific offering",
                syntax: "garden-rake nurturing status mongodb",
            },
            CommandExample {
                description: "List all backups for an offering",
                syntax: "garden-rake nurturing list mongodb",
            },
            CommandExample {
                description: "List local backups only",
                syntax: "garden-rake nurturing list mongodb --local",
            },
            CommandExample {
                description: "Trigger backup for an offering",
                syntax: "garden-rake nurturing trigger mongodb",
            },
            CommandExample {
                description: "Trigger backup for all offerings",
                syntax: "garden-rake nurturing trigger-all",
            },
        ],
        see_also: vec!["restore", "storage", "nourish"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    // === DEVELOPER TOOLS ===

    manifest.add(CommandDef {
        name: cmd::API,
        category: CommandCategory::Discovery,
        description: "Display Moss HTTP API reference",
        long_description: "Query and display Moss HTTP API documentation.\n\n\
            Fetches live API manifest from Moss and displays formatted endpoint reference\n\
            with methods, paths, parameters, and curl examples.\n\n\
            Filter by category (health, offerings, services, stone, garden, admin) or\n\
            view detailed documentation for a specific endpoint path.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("endpoint", "Specific endpoint path to show details for (e.g., /api/v1/stone/services)"),
            at_arg(),
            ArgSpec::option("category", "Filter by API category (health, offerings, services, stone, garden, events, admin)"),
            ArgSpec::flag("examples", "Show curl examples for each endpoint"),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Show all endpoints by category",
                syntax: "garden-rake api",
            },
            CommandExample {
                description: "Show only offerings API",
                syntax: "garden-rake api --category offerings",
            },
            CommandExample {
                description: "Detailed docs for specific endpoint",
                syntax: "garden-rake api /api/v1/stone/services",
            },
            CommandExample {
                description: "Include curl examples",
                syntax: "garden-rake api --examples",
            },
            CommandExample {
                description: "SSE endpoint documentation",
                syntax: "garden-rake api /api/v1/stone/presence/stream",
            },
        ],
        see_also: vec!["hey", "observe"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    // === LOCAL UTILITY COMMANDS ===

    manifest.add(CommandDef {
        name: cmd::LAUNCH,
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
                syntax: "garden-rake launch",
            },
            CommandExample {
                description: "Open specific stone's portrait",
                syntax: "garden-rake launch at stone-01",
            },
            CommandExample {
                description: "Open by endpoint",
                syntax: "garden-rake launch --at http://192.168.1.100:7185",
            },
        ],
        see_also: vec!["observe", "status", "tend"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: cmd::COMMANDS,
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
            ArgSpec::positional("name", "Specific command to show detailed help for"),
            ArgSpec::option("category", "Filter by category (discovery, lifecycle, management, system, pond)"),
            ArgSpec::flag("zen", "Show only zen syntax"),
            ArgSpec::flag("normative", "Show only normative syntax"),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Show all commands by category",
                syntax: "garden-rake commands",
            },
            CommandExample {
                description: "Show detailed help for a command",
                syntax: "garden-rake commands take-root",
            },
            CommandExample {
                description: "Filter by category",
                syntax: "garden-rake commands --category system",
            },
            CommandExample {
                description: "Show zen syntax only",
                syntax: "garden-rake commands --zen",
            },
        ],
        see_also: vec!["api", "launch"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    // === MANIFEST AUTHORING COMMANDS ===

    manifest.add(CommandDef {
        name: cmd::MANIFEST_CMD,
        category: CommandCategory::Lifecycle,
        description: "Scaffold, validate, test, and export offering manifests",
        long_description:
            "Author and manage offering manifest files.\n\n\
            Subcommands:\n\
            - init: Scaffold manifest files from a Docker image via inspection\n\
            - validate: Check manifest files for errors (runs locally)\n\
            - test: Validate and test-deploy a manifest on a stone\n\
            - export: Download manifest files for an installed offering\n\
            - enrich: Add missing compatibility/guidance templates",
        remote_capable: true,
        args: vec![],
        subcommands: vec![
            SubDef {
                name: "init",
                description: "Scaffold manifest files from a Docker image",
                args: vec![
                    ArgSpec::positional("image-ref", "Docker image reference (e.g., nginx:latest)")
                        .required(),
                    ArgSpec::option("output", "Output directory (default: ./<name>)"),
                    ArgSpec::option("name", "Override offering name"),
                    ArgSpec::option("category", "Override category (default: custom)"),
                    at_arg(),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "validate",
                description: "Check manifest files for errors (local)",
                args: vec![
                    ArgSpec::positional("path", "Path to manifest directory (default: .)"),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "test",
                description: "Validate and test-deploy manifest on a stone",
                args: vec![
                    ArgSpec::positional("path", "Path to manifest directory (default: .)"),
                    at_arg(),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "export",
                description: "Download manifest files for an installed offering",
                args: vec![
                    ArgSpec::positional("offering", "Offering name to export")
                        .required(),
                    ArgSpec::option("output", "Output directory (default: ./<offering>)"),
                    at_arg(),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "enrich",
                description: "Add missing compatibility/guidance templates",
                args: vec![
                    ArgSpec::positional("path", "Path to manifest directory (default: .)"),
                    ArgSpec::flag("auto", "Auto-generate without prompting"),
                ],
                subcommands: vec![],
            },
        ],
        examples: vec![
            CommandExample {
                description: "Scaffold manifest from Docker image",
                syntax: "garden-rake manifest init nginx:latest on stone-01",
            },
            CommandExample {
                description: "Validate manifest files in current directory",
                syntax: "garden-rake manifest validate",
            },
            CommandExample {
                description: "Test-deploy manifest on a stone",
                syntax: "garden-rake manifest test . on stone-01",
            },
            CommandExample {
                description: "Export installed offering's manifest",
                syntax: "garden-rake manifest export mongodb on stone-01",
            },
            CommandExample {
                description: "Add missing templates automatically",
                syntax: "garden-rake manifest enrich . --auto",
            },
        ],
        see_also: vec!["offer"],
        hidden: false,
        subcommand_negates_reqs: false,
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
        "pulse",
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
        "storage",
        "store",
        // Nurturing
        "restore",
        "nurturing",
        // Manifest authoring
        "manifest",
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
