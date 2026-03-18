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
    pub const UPGRADE: &str = "upgrade";

    // Adoption
    pub const ADOPT: &str = "adopt";
    pub const RELEASE: &str = "release";
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
    pub const MAKE: &str = "make";

    // Pond
    pub const POND: &str = "pond";

    // Scaffolded
    pub const CEREMONY: &str = "ceremony";
    pub const TEMPLATE: &str = "template";

    // Local/Meta commands (not requiring stone)
    pub const BROWSE: &str = "browse";
    pub const LAUNCH: &str = "launch";
    pub const COMMANDS: &str = "commands";

    // Stone admin (power management)

    // Monitoring
    pub const PULSE: &str = "pulse";

    // Test/Diagnostic
    pub const ELECTION: &str = "election";

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
    /// Visible aliases (e.g., "explore" for offer, "cap" for capabilities)
    pub aliases: &'static [&'static str],
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
        aliases: &[],
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
        aliases: &[],
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
        aliases: &[],
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
        aliases: &[],
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

    manifest.add(CommandDef {
        name: "logs",
        aliases: &[],
        category: CommandCategory::Discovery,
        description: "Stream service logs (shortcut for watch offering <name> logs)",
        long_description: "Stream container logs for a service.\n\n\
            Shortcut for 'watch offering <name> logs'. The most common streaming use case\n\
            deserves a short path.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("service", "Service name").required(),
            ArgSpec::flag("timestamps", "Include timestamps in output"),
            at_arg(),
        ],
        subcommands: vec![],
        examples: vec![
            CommandExample {
                description: "Stream mongodb logs",
                syntax: "garden-rake logs mongodb",
            },
            CommandExample {
                description: "Stream with timestamps",
                syntax: "garden-rake logs mongodb --timestamps",
            },
        ],
        see_also: vec!["watch", "status"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    // === LIFECYCLE COMMANDS ===

    manifest.add(CommandDef {
        name: "offer",
        aliases: &["explore"],
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
        aliases: &[],
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
        aliases: &[],
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
        aliases: &[],
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
        aliases: &[],
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
        name: cmd::UPGRADE,
        aliases: &[],
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
        see_also: vec![ "offer", "rest"],
        hidden: false,
        subcommand_negates_reqs: false,
    });



    // === ADOPTION COMMANDS ===

    manifest.add(CommandDef {
        name: "adopt",
        aliases: &[],
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
        see_also: vec!["release", "find",],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: "release",
        aliases: &[],
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
        see_also: vec!["adopt",],
        hidden: false,
        subcommand_negates_reqs: false,
    });



    manifest.add(CommandDef {
        name: "find",
        aliases: &[],
        category: CommandCategory::Discovery,
        description: "Find running services and get connection URIs",
        long_description: "Find running services across the garden and return connection URIs.\n\n\
            Supports search by name, category (c:prefix), or tags (t:prefix).\n\
            Results are returned instantly from topology cache.\n\n\
            Use 'ensure' modifier to auto-provision if service not found.",
        remote_capable: true,
        args: vec![
            ArgSpec::positional("query", "Service name, c:category, or t:tag")
                .required(),
            ArgSpec::option("format", "Output format: human, json, uri, uri-ip"),
            ArgSpec::flag("ensure", "Auto-provision if not found"),
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
                syntax: "garden-rake find mongodb ensure",
            },
        ],
        see_also: vec!["observe", "list", "offer", "config"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: "config",
        aliases: &[],
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
        name: "borrow",
        aliases: &[],
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
        see_also: vec!["return",],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: "return",
        aliases: &[],
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
        see_also: vec!["borrow",],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    manifest.add(CommandDef {
        name: "status",
        aliases: &[],
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
        aliases: &[],
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



    // === SYSTEM COMMANDS ===





    // === POND COMMANDS ===

    manifest.add(CommandDef {
        name: "pond",
        aliases: &[],
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
        see_also: vec![],
        hidden: false,
        subcommand_negates_reqs: false,
    });







    // === SCAFFOLDED COMMANDS ===
    // These commands are recognized but output placeholder messages until fully implemented

    manifest.add(CommandDef {
        name: cmd::CEREMONY,
        aliases: &[],
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
        aliases: &[],
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
        name: cmd::ELECTION,
        aliases: &[],
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



    // === Companion COMMANDS ===

    manifest.add(CommandDef {
        name: "hey",
        aliases: &[],
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
        see_also: vec!["watch",],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    // === STORAGE COMMANDS ===

    manifest.add(CommandDef {
        name: cmd::STORAGE,
        aliases: &[],
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
        aliases: &[],
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





    // === DEVELOPER TOOLS ===

    manifest.add(CommandDef {
        name: cmd::API,
        aliases: &[],
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
        aliases: &[],
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
        aliases: &[],
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
        aliases: &[],
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

    // =================================================================
    // STONE ADMINISTRATION (grouped)
    // =================================================================

    manifest.add(CommandDef {
        name: "stone",
        aliases: &[],
        category: CommandCategory::System,
        description: "Stone administration (power, service, diagnostics)",
        long_description: "Manage stone hardware: power control, service installation,\n\
            registry reconciliation, console verbosity, and binary updates.",
        remote_capable: true,
        args: vec![at_arg_global()],
        subcommands: vec![
            SubDef {
                name: "wake",
                description: "Wake a stone via Wake-on-LAN",
                args: vec![ArgSpec::positional("stone", "Stone name to wake").required()],
                subcommands: vec![],
            },
            SubDef {
                name: "shutdown",
                description: "Power off a stone",
                args: vec![ArgSpec::positional("stone", "Stone name (omit for tended)")],
                subcommands: vec![],
            },
            SubDef {
                name: "reboot",
                description: "Reboot a stone",
                args: vec![ArgSpec::positional("stone", "Stone name (omit for tended)")],
                subcommands: vec![],
            },
            SubDef {
                name: "verbosity",
                description: "Control console output level",
                args: vec![
                    ArgSpec::positional("level", "sing, quiet, silent, or minimal").required(),
                    ArgSpec::flag("forever", "No timeout (default: 30 min for sing)"),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "install",
                description: "Install moss as a system service",
                args: vec![
                    ArgSpec::flag("yes", "Accept all prompts").short('y'),
                    ArgSpec::flag("dry-run", "Show what would happen"),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "reconcile",
                description: "Force registry sync with containers",
                args: vec![ArgSpec::flag("drop-invalid", "Remove invalid zen-offering-* containers")],
                subcommands: vec![],
            },
            SubDef {
                name: "refresh",
                description: "Update moss or rake binary (dev)",
                args: vec![
                    ArgSpec::positional("component", "'moss' or 'rake'").required(),
                    ArgSpec::option("from", "Path to binary file").required(),
                ],
                subcommands: vec![],
            },
        ],
        examples: vec![
            CommandExample {
                description: "Wake a stone",
                syntax: "garden-rake stone wake oak",
            },
            CommandExample {
                description: "Power off tended stone",
                syntax: "garden-rake stone shutdown",
            },
            CommandExample {
                description: "Enable verbose console output",
                syntax: "garden-rake stone verbosity sing",
            },
            CommandExample {
                description: "Install moss as service",
                syntax: "garden-rake stone install --yes",
            },
            CommandExample {
                description: "Force registry reconciliation",
                syntax: "garden-rake stone reconcile",
            },
        ],
        see_also: vec!["status", "observe"],
        hidden: false,
        subcommand_negates_reqs: false,
    });

    // =================================================================
    // BACKUP (grouped, replaces nurturing + restore)
    // =================================================================

    manifest.add(CommandDef {
        name: "backup",
        aliases: &[],
        category: CommandCategory::Storage,
        description: "Manage snapshots and restore offerings",
        long_description: "Manage offering snapshots (backup/restore).\n\n\
            View backup status, list available snapshots, trigger backups,\n\
            and restore from local A/B slots or remote seed banks.",
        remote_capable: true,
        args: vec![at_arg_global()],
        subcommands: vec![
            SubDef {
                name: "status",
                description: "Show backup status for offerings",
                args: vec![ArgSpec::positional("offering", "Offering name (omit for all)")],
                subcommands: vec![],
            },
            SubDef {
                name: "list",
                description: "List available snapshots",
                args: vec![
                    ArgSpec::positional("offering", "Offering name").required(),
                    ArgSpec::flag("local", "Show only local snapshots"),
                    ArgSpec::flag("remote", "Show only remote snapshots"),
                ],
                subcommands: vec![],
            },
            SubDef {
                name: "trigger",
                description: "Trigger backup for an offering",
                args: vec![ArgSpec::positional("offering", "Offering name").required()],
                subcommands: vec![],
            },
            SubDef {
                name: "trigger-all",
                description: "Trigger backup for all offerings",
                args: vec![],
                subcommands: vec![],
            },
            SubDef {
                name: "restore",
                description: "Restore an offering from snapshot",
                args: vec![
                    ArgSpec::positional("offering", "Offering name").required(),
                    ArgSpec::trailing("source", "Source: 'from slot A' or 'from seed-bank <name>'"),
                    ArgSpec::flag("dry-run", "Preview without executing"),
                    ArgSpec::option("snapshot-id", "Specific snapshot ID"),
                ],
                subcommands: vec![],
            },
        ],
        examples: vec![
            CommandExample {
                description: "Show all backup status",
                syntax: "garden-rake backup status",
            },
            CommandExample {
                description: "List snapshots for mongodb",
                syntax: "garden-rake backup list mongodb",
            },
            CommandExample {
                description: "Trigger backup",
                syntax: "garden-rake backup trigger mongodb",
            },
            CommandExample {
                description: "Restore from seed bank",
                syntax: "garden-rake backup restore mongodb from seed-bank media",
            },
        ],
        see_also: vec!["storage"],
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
        "logs",
        "list",
        "status",
        "find",
        "config",
        // Lifecycle
        "offer",
        "rest",
        "wake",
        "remove",
        "uproot",
        "upgrade",
        "capabilities",
        // Adoption
        "adopt",
        "release",
        "borrow",
        "return",
        // Management
        "tend",
        "ceremony",
        "template",
        // Pond
        "pond",
        // Stone administration (grouped)
        "stone",
        // Backup (grouped, replaces nurturing + restore)
        "backup",
        // Test/Diagnostic
        "election",
        // Companions
        "hey",
        // Developer Tools
        "api",
        // Storage
        "storage",
        "store",
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
