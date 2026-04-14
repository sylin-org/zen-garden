//! Command routing — thin dispatch layer
//!
//! `route()` receives a subcommand name and its ArgMatches, extracts arguments,
//! constructs the appropriate Command struct, and returns a `CommandInvocation`.
//! The `Runtime` handles all dispatch middleware (endpoint resolution, stone
//! headers, context building).
//!
//! For a handful of commands that don't implement the Command trait (presence,
//! election, api) or need pre-resolved endpoints (launch, refresh), the function
//! executes them directly and returns `None`.
//!
//! ### Before vs After
//!
//! Old route.rs: 1,917 lines, 66 dispatch calls, 44 `get_one("at")`, 107
//! `global.quiet` references, 7-argument dispatch function signatures.
//!
//! New route.rs: ~550 lines. `CommandInvocation::remote(cmd, m)` auto-extracts
//! `--at`; the `Runtime` encapsulates global flags.

use crate::dispatch::{CommandInvocation, Runtime};

use garden_rake::cli_build::GlobalFlags;
use garden_rake::commands;
use garden_rake::commands::Command;
use garden_rake::connection::resolution::{self, CachedStoneOps};
use garden_rake::stone_cache::STONE;

use base64::Engine;
use std::time::Duration;

/// Extract a required string arg from ArgMatches.
fn req(m: &clap::ArgMatches, name: &str) -> anyhow::Result<String> {
    m.get_one::<String>(name)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Missing required argument: {}", name))
}

/// Extract an optional string arg from ArgMatches.
fn opt(m: &clap::ArgMatches, name: &str) -> Option<String> {
    m.get_one::<String>(name).cloned()
}

/// Build a `CommandInvocation` from parsed CLI matches, or execute directly
/// for the few commands that bypass the Command trait.
///
/// Returns `Some(invocation)` for standard commands (Runtime handles dispatch),
/// or `None` for commands that already executed.
pub async fn route(
    name: &str,
    m: &clap::ArgMatches,
    g: &GlobalFlags,
    rt: &Runtime,
) -> anyhow::Result<Option<CommandInvocation>> {
    type Inv = CommandInvocation;

    let inv = match name {
        // =================================================================
        // Discovery
        // =================================================================

        "status" => Inv::remote(commands::discovery::StatusCommand::new(g.quiet), m),

        "inspect" => match m.subcommand() {
            Some(("all", sub)) => {
                let output_path = opt(sub, "file")
                    .unwrap_or_else(|| "garden-inspect.json".to_string());
                let json = sub.get_flag("json");
                let expanded = sub.get_flag("expanded");
                Inv::remote(
                    commands::discovery::InspectAllCommand::new(output_path, json, expanded, g.quiet),
                    m,
                )
            }
            _ => {
                let save_path = opt(m, "save");
                let json = m.get_flag("json");
                Inv::remote(
                    commands::discovery::InspectCommand::new(save_path, json, g.quiet),
                    m,
                )
            }
        }

        "list" => Inv::remote(commands::discovery::ListCommand::new(g.quiet), m),

        "find" => {
            let query = opt(m, "query").unwrap_or_default();
            let format_str = opt(m, "format").unwrap_or_else(|| "human".into());
            let ensure = m.get_flag("ensure");
            let find_format: commands::discovery::FindOutputFormat =
                if g.field.is_some() || g.output.as_str() == "json" {
                    commands::discovery::FindOutputFormat::Json
                } else {
                    format_str.parse().unwrap_or_default()
                };
            Inv::remote(
                commands::discovery::FindCommand::with_field(
                    query,
                    find_format,
                    g.quiet,
                    g.fresh,
                    ensure,
                    g.field.clone(),
                ),
                m,
            )
        }

        "config" => {
            let json_output = g.output.as_str() == "json" || g.field.is_some();
            Inv::remote(
                commands::discovery::ConfigCommand::new(
                    req(m, "service")?,
                    g.quiet,
                    json_output,
                    g.field.clone(),
                ),
                m,
            )
        }

        "observe" => {
            let cmd = commands::discovery::ObserveCommand::new(
                opt(m, "stone"),
                opt(m, "offering"),
                g.quiet,
            );
            Inv::local(cmd)
        }

        "pulse" => Inv::remote(commands::pulse::PulseCommand::new(g.quiet), m),

        "logs" => {
            let service = req(m, "service")?;
            let timestamps = m.get_flag("timestamps");
            Inv::remote(
                commands::discovery::WatchCommand::offering_logs(service, timestamps, g.quiet),
                m,
            )
        }

        // === Watch (subcommands) ===
        "watch" => {
            let cmd = match m.subcommand() {
                Some(("offering", sub)) => {
                    let name = req(sub, "name")?;
                    match sub.subcommand() {
                        Some(("logs", logs)) => {
                            commands::discovery::WatchCommand::offering_logs(
                                name,
                                logs.get_flag("timestamps"),
                                g.quiet,
                            )
                        }
                        _ => anyhow::bail!("Usage: garden-rake watch offering <name> logs"),
                    }
                }
                Some(("stone", sub)) => {
                    let name = req(sub, "name")?;
                    match sub.subcommand() {
                        Some(("logs", logs)) => {
                            commands::discovery::WatchCommand::stone_logs(
                                name,
                                logs.get_flag("timestamps"),
                                g.quiet,
                            )
                        }
                        _ => anyhow::bail!("Usage: garden-rake watch stone <name> logs"),
                    }
                }
                _ => commands::discovery::WatchCommand::events(opt(m, "until"), g.quiet),
            };
            Inv::remote(cmd, m)
        }

        // === Capabilities (subcommands) ===
        "capabilities" => match m.subcommand() {
            Some(("add", sub)) => Inv::remote(
                commands::discovery::AddCapabilityCommand::new(
                    req(sub, "offering")?,
                    req(sub, "name")?,
                    opt(sub, "type"),
                    sub.get_flag("dry-run"),
                    g.quiet,
                ),
                m,
            ),
            Some(("remove", sub)) => Inv::remote(
                commands::discovery::RemoveCapabilityCommand::new(
                    req(sub, "offering")?,
                    req(sub, "name")?,
                    opt(sub, "type"),
                    g.quiet,
                ),
                m,
            ),
            Some(("refresh", sub)) => Inv::remote(
                commands::discovery::RefreshCapabilitiesCommand::new(
                    req(sub, "offering")?,
                    opt(sub, "type"),
                    sub.get_flag("dry-run"),
                    g.quiet,
                ),
                m,
            ),
            Some(("mirror", sub)) => {
                let args: Vec<String> = sub
                    .get_many::<String>("args")
                    .map(|v| v.cloned().collect())
                    .unwrap_or_default();
                Inv::remote(
                    commands::discovery::MirrorCapabilitiesCommand::new(
                        req(sub, "offering")?,
                        args,
                        g.quiet,
                    ),
                    m,
                )
            }
            _ => Inv::remote(
                commands::discovery::CapabilitiesCommand::new(req(m, "offering")?, g.quiet),
                m,
            ),
        },

        // =================================================================
        // Offering (complex branching — extracted to helper)
        // =================================================================

        "offer" => return route_offer(m, g, rt).await,

        // =================================================================
        // Lifecycle
        // =================================================================

        "remove" => Inv::remote(
            commands::lifecycle::RemoveCommand::new(req(m, "service")?, m.get_flag("yes"), g.quiet),
            m,
        ),

        "uproot" => Inv::remote(
            commands::lifecycle::UprootCommand::new(req(m, "service")?, m.get_flag("yes"), g.quiet),
            m,
        ),

        "upgrade" => Inv::remote(
            commands::lifecycle::UpgradeCommand::new(opt(m, "service"), m.get_flag("all"), g.quiet),
            m,
        ),

        "nourish" => Inv::local(commands::nourish::NourishCommand::new(
            opt(m, "stone"),
            m.get_flag("updates-only"),
            m.get_flag("yes"),
        )),

        "rest" => Inv::remote(
            commands::lifecycle::RestCommand::new(req(m, "service")?, g.quiet),
            m,
        ),

        "wake" => Inv::remote(
            commands::lifecycle::WakeCommand::new(req(m, "service")?, g.quiet),
            m,
        ),

        // =================================================================
        // Manifest Authoring
        // =================================================================

        "manifest" => {
            use commands::manifest::ManifestCommand;
            match m.subcommand() {
                Some(("init", sub)) => {
                    let at = opt(sub, "at").or_else(|| opt(m, "at"));
                    Inv::remote_at(
                        ManifestCommand::init(
                            req(sub, "image-ref")?,
                            opt(sub, "output"),
                            opt(sub, "name"),
                            opt(sub, "category"),
                            g.quiet,
                        ),
                        at,
                    )
                }
                Some(("validate", sub)) => {
                    let path = opt(sub, "path").unwrap_or_else(|| ".".into());
                    Inv::local(ManifestCommand::validate(path, g.quiet))
                }
                Some(("test", sub)) => {
                    let path = opt(sub, "path").unwrap_or_else(|| ".".into());
                    let at = opt(sub, "at").or_else(|| opt(m, "at"));
                    Inv::remote_at(ManifestCommand::test(path, g.quiet), at)
                }
                Some(("export", sub)) => {
                    let at = opt(sub, "at").or_else(|| opt(m, "at"));
                    Inv::remote_at(
                        ManifestCommand::export(
                            req(sub, "offering")?,
                            opt(sub, "output"),
                            g.quiet,
                        ),
                        at,
                    )
                }
                Some(("enrich", sub)) => {
                    let path = opt(sub, "path").unwrap_or_else(|| ".".into());
                    let auto = sub.get_flag("auto");
                    Inv::local(ManifestCommand::enrich(path, auto, g.quiet))
                }
                _ => anyhow::bail!(
                    "Usage: garden-rake manifest <init|validate|test|export|enrich>"
                ),
            }
        }

        // =================================================================
        // Firefly operator tooling (FIREFLY-0004 Ch4)
        // =================================================================

        "firefly" => {
            use commands::firefly::FireflyCommand;
            match m.subcommand() {
                Some(("inventory", _)) => Inv::local(FireflyCommand::inventory(g.quiet)),
                Some(("roster", roster_m)) => match roster_m.subcommand() {
                    Some(("push", push_m)) => Inv::local(FireflyCommand::roster_push(
                        req(push_m, "stone")?,
                        g.quiet,
                    )),
                    _ => anyhow::bail!("Usage: garden-rake firefly roster push <stone>"),
                },
                _ => anyhow::bail!("Usage: garden-rake firefly <inventory|roster push <stone>>"),
            }
        }

        // =================================================================
        // Adoption
        // =================================================================

        "adopt" => Inv::remote(
            commands::adoption::AdoptCommand::new(req(m, "target")?, g.quiet),
            m,
        ),

        "release" => Inv::remote(
            commands::adoption::ReleaseCommand::new(req(m, "service")?, g.quiet),
            m,
        ),

        "borrow" => {
            let name = req(m, "name")?;
            let from_url = opt(m, "from").ok_or_else(|| {
                anyhow::anyhow!("Missing URL. Use: garden-rake borrow {} from <url>", name)
            })?;
            Inv::remote(
                commands::adoption::BorrowCommand::new(name, from_url, g.quiet),
                m,
            )
        }

        "return" => Inv::remote(
            commands::adoption::ReturnCommand::new(req(m, "name")?, g.quiet),
            m,
        ),

        // =================================================================
        // Management
        // =================================================================

        "tend" => Inv::local(commands::management::TendCommand::new(
            opt(m, "target"),
            m.get_flag("clear"),
            g.verbose > 0,
        )),

        // === Pond (subcommands) ===
        "pond" => {
            use commands::management::PondActionType;
            let action_type = match m.subcommand() {
                Some(("init", sub)) => PondActionType::Init {
                    passphrase: opt(sub, "passphrase"),
                    profile: opt(sub, "profile"),
                },
                Some(("status", _)) => PondActionType::Status,
                Some(("invite", sub)) => PondActionType::Invite {
                    passphrase: opt(sub, "passphrase"),
                },
                Some(("join", sub)) => PondActionType::Join {
                    code: req(sub, "code")?,
                },
                Some(("enroll", _)) => PondActionType::Enroll,
                Some(("trust", _)) => PondActionType::Trust,
                Some(("unlock", sub)) => PondActionType::Unlock {
                    passphrase: opt(sub, "passphrase"),
                    totp: opt(sub, "totp"),
                },
                Some(("drain", _)) => PondActionType::Remove,
                Some(("remove", sub)) => PondActionType::Untrust {
                    stone_name: req(sub, "stone")?,
                },
                Some(("untrust", sub)) => PondActionType::Untrust {
                    stone_name: req(sub, "stone")?,
                },
                Some(("promote", sub)) => PondActionType::Promote {
                    passphrase: opt(sub, "passphrase"),
                },
                Some(("rename", sub)) => PondActionType::Rename {
                    name: opt(sub, "name"),
                },
                _ => anyhow::bail!(
                    "Usage: garden-rake pond <init|status|invite|join|enroll|trust|unlock|drain|remove|untrust|promote|rename>"
                ),
            };
            Inv::remote(
                commands::management::PondCommand::new(action_type, g.quiet),
                m,
            )
        }

        // =================================================================
        // Stone Administration (grouped)
        // =================================================================

        "stone" => match m.subcommand() {
            Some(("wake", sub)) => Inv::remote(
                commands::admin::RouseCommand::new(req(sub, "stone")?, g.quiet),
                m,
            ),
            Some(("shutdown", sub)) => {
                let target = opt(sub, "stone").or_else(|| opt(m, "at"));
                Inv::remote_at(commands::admin::SlumberCommand::new(g.quiet), target)
            }
            Some(("reboot", sub)) => {
                let target = opt(sub, "stone").or_else(|| opt(m, "at"));
                Inv::remote_at(commands::admin::StirCommand::new(g.quiet), target)
            }
            Some(("verbosity", sub)) => {
                use commands::management::MakeActionType;
                let level = req(sub, "level")?;
                let action_type = match level.as_str() {
                    "sing" => MakeActionType::Sing {
                        forever: sub.get_flag("forever"),
                    },
                    "quiet" => MakeActionType::Quiet,
                    "silent" => MakeActionType::Silent,
                    "minimal" => MakeActionType::Minimal,
                    _ => anyhow::bail!(
                        "Invalid verbosity level: '{}'. Use: sing, quiet, silent, minimal",
                        level
                    ),
                };
                Inv::remote(commands::management::MakeCommand::new(action_type, g.quiet), m)
            }
            Some(("install", _sub)) => {
                Inv::remote_at(
                    commands::admin::InstallServiceCommand::take_root(g.quiet),
                    opt(m, "at"),
                )
            }
            Some(("reconcile", sub)) => Inv::remote(
                commands::management::ReconcileCommand::new(
                    sub.get_flag("drop-invalid"),
                    g.quiet,
                ),
                m,
            ),
            Some(("refresh", sub)) => {
                let component = req(sub, "component")?;
                let from = req(sub, "from")?;
                let endpoint =
                    resolution::resolve(&rt.client, opt(m, "at").as_deref(), Some(&*STONE as &dyn CachedStoneOps), None).await?.endpoint;
                println!("Refreshing {}...", component);
                let api = garden_common::client::StoneApi::new(rt.client.clone(), endpoint);
                refresh_component(&api, &component, std::path::Path::new(&from))
                    .await?;
                return Ok(None);
            }
            _ => anyhow::bail!(
                "Usage: garden-rake stone <wake|shutdown|reboot|verbosity|install|reconcile|refresh>"
            ),
        },

        // =================================================================
        // Hey (companion)
        // =================================================================

        "hey" => {
            let args: Vec<String> = m
                .get_many::<String>("tell")
                .map(|v| v.cloned().collect())
                .unwrap_or_default();
            Inv::remote(commands::hey::HeyTellCommand { args }, m)
        }

        // =================================================================
        // Local / Meta
        // =================================================================

        "ceremony" => Inv::local(
            commands::local::CeremonyCommand::new(opt(m, "workflow"), g.quiet),
        ),

        "commands" => Inv::local(commands::local::BrowseCommand::new(
            opt(m, "name"),
            opt(m, "category"),
            false,
            false,
        )),

        // =================================================================
        // Template (subcommands)
        // =================================================================

        "template" => {
            use commands::local::TemplateAction;
            let at_parent = opt(m, "at");
            match m.subcommand() {
                Some(("list", sub)) => {
                    let at = opt(sub, "at").or(at_parent);
                    Inv::remote_at(
                        commands::local::TemplateCommand::new(TemplateAction::List, g.quiet),
                        at,
                    )
                }
                Some(("show", sub)) => {
                    let at = opt(sub, "at").or(at_parent);
                    Inv::remote_at(
                        commands::local::TemplateCommand::new(
                            TemplateAction::Show { name: req(sub, "name")? },
                            g.quiet,
                        ),
                        at,
                    )
                }
                _ => anyhow::bail!("Usage: garden-rake template <list|show>"),
            }
        }

        // =================================================================
        // Storage
        // =================================================================

        "storage" => match m.subcommand() {
            Some(("add", sub)) => {
                let roles: Vec<String> = sub
                    .get_many::<String>("roles")
                    .map(|v| v.cloned().collect())
                    .unwrap_or_default();
                Inv::remote(
                    commands::storage::AddStorageCommand::new(
                        opt(sub, "target"),
                        opt(sub, "name"),
                        roles,
                        sub.get_flag("format"),
                        opt(sub, "fs"),
                        sub.get_flag("encrypted"),
                        sub.get_flag("yes"),
                    ),
                    m,
                )
            }
            Some(("list", _)) => {
                Inv::remote(commands::storage::ListStorageCommand::new(), m)
            }
            Some(("status", _)) => {
                Inv::remote(commands::storage::StorageStatusCommand::new(), m)
            }
            Some(("release", sub)) => {
                Inv::remote(
                    commands::storage::ReleaseStorageCommand::new(req(sub, "name")?),
                    m,
                )
            }
            Some(("pin", sub)) => {
                Inv::remote(
                    commands::storage::PinStorageCommand::new(req(sub, "name")?),
                    m,
                )
            }
            Some(("unpin", sub)) => {
                Inv::remote(
                    commands::storage::UnpinStorageCommand::new(req(sub, "name")?),
                    m,
                )
            }
            // Bare `storage` — list all storages (same as `storage list`)
            _ => Inv::remote(commands::storage::ListStorageCommand::new(), m),
        },

        "store" => return route_store(m, g),

        // =================================================================
        // Backup (grouped, replaces nurturing + restore)
        // =================================================================

        "backup" => match m.subcommand() {
            Some(("status", sub)) => Inv::remote(
                commands::nurturing::NurturingStatusCommand::new(opt(sub, "offering")),
                m,
            ),
            Some(("list", sub)) => Inv::remote(
                commands::nurturing::NurturingListCommand::new(
                    req(sub, "offering")?,
                    sub.get_flag("local"),
                    sub.get_flag("remote"),
                ),
                m,
            ),
            Some(("trigger", sub)) => Inv::remote(
                commands::nurturing::NurturingTriggerCommand::new(Some(req(sub, "offering")?)),
                m,
            ),
            Some(("trigger-all", _)) => Inv::remote(
                commands::nurturing::NurturingTriggerCommand::new(None),
                m,
            ),
            Some(("restore", sub)) => {
                return route_restore(sub, g);
            }
            _ => anyhow::bail!(
                "Usage: garden-rake backup <status|list|trigger|trigger-all|restore>"
            ),
        },

        // =================================================================
        // Special cases — execute directly, bypass Runtime
        // =================================================================

        "launch" => {
            let at = opt(m, "at");
            let endpoint =
                resolution::resolve(&rt.client, at.as_deref(), Some(&*STONE as &dyn CachedStoneOps), None).await?.endpoint;
            return Ok(Some(Inv::local(
                commands::local::LaunchCommand::new(Some(endpoint)),
            )));
        }

        "api" => {
            let at = opt(m, "at");
            let resolved =
                resolution::resolve(&rt.client, at.as_deref(), Some(&*STONE as &dyn CachedStoneOps), None).await?.endpoint;
            let api = garden_common::client::StoneApi::new(rt.client.clone(), resolved);
            commands::api::execute_api_command(
                &api,
                opt(m, "category"),
                opt(m, "endpoint"),
                m.get_flag("examples"),
            )
            .await?;
            return Ok(None);
        }

        "election" => return route_election(m, &rt.client).await,

        // =================================================================
        // Fallback
        // =================================================================

        _ => anyhow::bail!(
            "Unknown command: '{}'. Run garden-rake --help for available commands.",
            name
        ),
    };

    Ok(Some(inv))
}

// ============================================================================
// Complex sub-routers
// ============================================================================

/// Route `offer` — placement, query-anywhere, info, install/query branching.
async fn route_offer(
    m: &clap::ArgMatches,
    g: &GlobalFlags,
    rt: &Runtime,
) -> anyhow::Result<Option<CommandInvocation>> {
    type Inv = CommandInvocation;

    let offering = opt(m, "offering");
    let at = opt(m, "at");
    let prefer: Vec<String> = m
        .get_many::<String>("prefer")
        .map(|v| v.cloned().collect())
        .unwrap_or_default();
    let anywhere_on_fail = m.get_flag("anywhere-on-fail");
    let placement_mode = opt(m, "placement-mode");
    let has_info_sub = m.subcommand_matches("info").is_some();

    // ── image subcommand ──
    if let Some(image_m) = m.subcommand_matches("image") {
        let image_ref = image_m
            .get_one::<String>("image-ref")
            .expect("image-ref required")
            .clone();
        let instance = image_m.get_one::<String>("instance").cloned();
        let info_only = image_m.get_flag("info-only");
        return Ok(Some(Inv::remote_at(
            commands::offering::OfferCommand::image(image_ref, instance, info_only, g.quiet),
            at,
        )));
    }

    // ── --placement-mode ──
    if let Some(mode) = &placement_mode {
        if let Some(name) = &offering {
            let is_quiet = mode == "auto" || g.quiet;
            return Ok(Some(Inv::local(
                commands::offering::OfferCommand::placement_recommend(name.to_string(), is_quiet),
            )));
        }
        anyhow::bail!("Usage: garden-rake offer <offering> --placement-mode <interactive|auto>");
    }

    // ── --at anywhere ──
    if at.as_deref() == Some("anywhere") {
        if let Some(name) = &offering {
            if name == "refresh" {
                anyhow::bail!("'offer refresh' requires a specific stone (remove --at anywhere)");
            }
            return Ok(Some(Inv::local(
                commands::offering::OfferCommand::query_anywhere(name.to_string(), prefer, g.quiet),
            )));
        }
        anyhow::bail!(
            "Usage with --at anywhere: garden-rake offer <query> --at anywhere [--prefer <token>]"
        );
    }

    // ── Normal routing ──
    let cmd = match (offering.as_deref(), has_info_sub) {
        (None, _) => commands::offering::OfferCommand::list(g.quiet),
        (Some("refresh"), _) => commands::offering::OfferCommand::refresh(g.quiet),
        (Some(name), true) => commands::offering::OfferCommand::info(name.to_string(), g.quiet),
        (Some(name), false) => {
            // Is it a known offering? Need pre-resolved endpoint to check.
            let endpoint =
                resolution::resolve(&rt.client, at.clone().as_deref(), Some(&*STONE as &dyn CachedStoneOps), None).await?.endpoint;
            let is_known =
                commands::offering::OfferCommand::is_known_offering(&rt.client, &endpoint, name)
                    .await;

            if !is_known {
                // Query — execute directly with pre-resolved endpoint
                let cmd = commands::offering::OfferCommand::query(
                    name.to_string(),
                    prefer.clone(),
                    g.quiet,
                );
                let resolved = garden_rake::connection::resolution::Resolved {
                    endpoint: endpoint.clone(),
                    origin: garden_rake::connection::resolution::Origin::Flag,
                };
                let stone = garden_rake::connection::stone::Stone::bind(rt.client.clone(), resolved);
                let ctx = garden_rake::context::Context::from_stone(
                    &stone,
                    None,
                    rt.client.clone(),
                    g.quiet,
                    false,
                    g.verbose,
                    garden_rake::context::OutputFormat::Human,
                    None,
                );
                cmd.execute(&ctx).await?;
                return Ok(None);
            }

            commands::offering::OfferCommand::install(
                name.to_string(),
                prefer,
                anywhere_on_fail,
                g.quiet,
            )
        }
    };

    Ok(Some(Inv::remote_at(cmd, at)))
}

/// Route `store` — branches on operation (put/get/ls/rm/head).
fn route_store(
    m: &clap::ArgMatches,
    _g: &GlobalFlags,
) -> anyhow::Result<Option<CommandInvocation>> {
    type Inv = CommandInvocation;

    let operation = req(m, "operation")?;
    let bucket = req(m, "bucket")?;
    let key = opt(m, "key");
    let file = opt(m, "file");
    let prefix = opt(m, "prefix");
    let delimiter = opt(m, "delimiter");
    let app = opt(m, "app");

    let inv = match operation.as_str() {
        "put" => Inv::remote(
            commands::storage::StorePutCommand::new(
                bucket,
                key.ok_or_else(|| anyhow::anyhow!("Key required for put operation"))?,
                std::path::PathBuf::from(
                    file.ok_or_else(|| anyhow::anyhow!("File required for put operation"))?,
                ),
                app,
            ),
            m,
        ),
        "get" => Inv::remote(
            commands::storage::StoreGetCommand::new(
                bucket,
                key.ok_or_else(|| anyhow::anyhow!("Key required for get operation"))?,
                file.map(std::path::PathBuf::from),
                app,
            ),
            m,
        ),
        "ls" | "list" => Inv::remote(
            commands::storage::StoreListCommand::new(bucket, prefix.or(key), delimiter, app),
            m,
        ),
        "rm" | "delete" => Inv::remote(
            commands::storage::StoreDeleteCommand::new(
                bucket,
                key.ok_or_else(|| anyhow::anyhow!("Key required for delete operation"))?,
                app,
            ),
            m,
        ),
        "head" | "info" => Inv::remote(
            commands::storage::StoreHeadCommand::new(
                bucket,
                key.ok_or_else(|| anyhow::anyhow!("Key required for head operation"))?,
                app,
            ),
            m,
        ),
        _ => anyhow::bail!(
            "Unknown store operation '{}'. Use: put, get, ls, rm, head",
            operation
        ),
    };

    Ok(Some(inv))
}

/// Route `restore` — branches on source (seed-bank vs local slot).
fn route_restore(
    m: &clap::ArgMatches,
    _g: &GlobalFlags,
) -> anyhow::Result<Option<CommandInvocation>> {
    type Inv = CommandInvocation;

    let offering = req(m, "offering")?;
    let dry_run = m.get_flag("dry-run");
    let harvest_id = opt(m, "harvest-id");

    let source_words: Vec<String> = m
        .get_many::<String>("source")
        .map(|v| v.cloned().collect())
        .unwrap_or_default();
    let source_str = source_words.join(" ").to_lowercase();

    if source_str.contains("seed-bank") || source_str.contains("seedbank") {
        let seed_bank = source_words
            .iter()
            .skip_while(|s| s.to_lowercase() != "seed-bank" && s.to_lowercase() != "seedbank")
            .nth(1)
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Missing seed bank name. Usage: garden-rake restore {} from seed-bank <name>",
                    offering
                )
            })?;
        Ok(Some(Inv::remote(
            commands::nurturing::RestoreRemoteCommand::new(
                offering, seed_bank, harvest_id, dry_run,
            ),
            m,
        )))
    } else {
        let slot = if source_str.contains("slot") {
            source_words
                .iter()
                .skip_while(|s| s.to_lowercase() != "slot")
                .nth(1)
                .cloned()
        } else if source_words
            .iter()
            .any(|s| s.to_uppercase() == "A" || s.to_uppercase() == "B")
        {
            source_words
                .iter()
                .find(|s| s.to_uppercase() == "A" || s.to_uppercase() == "B")
                .cloned()
        } else {
            None
        };
        Ok(Some(Inv::remote(
            commands::nurturing::RestoreLocalCommand::new(offering, slot, dry_run),
            m,
        )))
    }
}

/// Route `election` — special case (doesn't use Command trait).
async fn route_election(
    m: &clap::ArgMatches,
    client: &reqwest::Client,
) -> anyhow::Result<Option<CommandInvocation>> {
    let action = opt(m, "action");
    let election_type = opt(m, "election-type");
    let criteria = opt(m, "criteria");
    let timeout = opt(m, "timeout");

    match action.as_deref() {
        Some("start") => {
            use garden_common::election::ElectionType;
            let et = match election_type.as_deref().unwrap_or("update_source") {
                "update_source" => ElectionType::UpdateSource,
                "ceremony_coordinator" => ElectionType::CeremonyCoordinator,
                "replica_target" => ElectionType::ReplicaTarget,
                "backup_source" => ElectionType::BackupSource,
                s if s.starts_with("offering_primary:") => ElectionType::OfferingPrimary(
                    s["offering_primary:".len()..].to_string(),
                ),
                custom => ElectionType::Custom(custom.to_string()),
            };
            let timeout_secs: u64 = timeout.as_deref().unwrap_or("10").parse().unwrap_or(10);
            commands::election::handle_election(
                commands::election::ElectionCommand {
                    action: commands::election::ElectionAction::Start(
                        commands::election::StartElection {
                            election_type: et,
                            criteria,
                            timeout: timeout_secs,
                        },
                    ),
                },
                client,
            )
            .await?;
            Ok(None)
        }
        Some(other) => anyhow::bail!("Unknown election action: '{}'. Use: start", other),
        None => anyhow::bail!(
            "Usage: garden-rake election start [--election-type <type>] [--criteria <json>] [--timeout <secs>]"
        ),
    }
}

// ============================================================================
// Refresh component (binary upload to stone)
// ============================================================================

async fn refresh_component(
    api: &garden_common::client::StoneApi,
    component: &str,
    binary_path: &std::path::Path,
) -> anyhow::Result<()> {
    use anyhow::{bail, Context};

    let normalized_component = match component.to_lowercase().as_str() {
        "moss" => "moss",
        "rake" | "garden-rake" => garden_common::constants::RAKE_BINARY,
        _ => bail!("Unknown component '{}'. Use 'moss' or 'rake'", component),
    };

    println!("\u{1f4e4} Reading binary file...");
    let binary_data = std::fs::read(binary_path).context(format!(
        "Failed to read binary file: {}",
        binary_path.display()
    ))?;

    let size_mb = binary_data.len() as f64 / 1024.0 / 1024.0;
    println!("   Size: {:.2} MB", size_mb);

    if binary_data.len() < 4 || &binary_data[0..4] != b"\x7fELF" {
        bail!("Not a valid ELF binary. Expected Linux executable.");
    }
    println!("   Format: ELF \u{2713}");

    println!("\u{1f4e6} Encoding binary...");
    let encoded = base64::engine::general_purpose::STANDARD.encode(&binary_data);

    println!("\u{1f680} Uploading to stone...");
    let payload = serde_json::json!({
        "component": normalized_component,
        "binary_data": encoded,
    });

    let body = match api.stone().refresh_binary(&payload).await {
        Ok(data) => data,
        Err(garden_common::client::StoneApiError::Http { status, message, .. }) => {
            println!("\u{2717} Refresh failed");
            println!("   Status: {}", status);
            println!("   Error: {}", message);
            bail!("Refresh request failed with status {}", status);
        }
        Err(garden_common::client::StoneApiError::HttpRaw { status, body }) => {
            println!("\u{2717} Refresh failed");
            println!("   Status: {}", status);
            if !body.is_empty() {
                println!("   Response: {}", body.chars().take(500).collect::<String>());
            }
            bail!("Refresh request failed with status {}", status);
        }
        Err(e) => {
            bail!("Refresh request failed: {}", e);
        }
    };

    println!("\u{2705} {} refreshed successfully", normalized_component);
    if let Some(arch) = body.get("architecture").and_then(|v| v.as_str()) {
        println!("   Architecture: {}", arch);
    }

    if normalized_component == "moss" {
        println!("\u{23f3} Moss is restarting...");
        println!("   (This may take a few seconds)");
        tokio::time::sleep(Duration::from_secs(5)).await;

        let health_url = format!("{}/health", api.endpoint());
        for attempt in 1..=12 {
            tokio::time::sleep(Duration::from_secs(2)).await;
            match api.http()
                .get(&health_url)
                .timeout(Duration::from_secs(5))
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    println!("\u{2705} Moss is back online");
                    return Ok(());
                }
                _ => {
                    if attempt < 12 {
                        print!(".");
                        std::io::Write::flush(&mut std::io::stdout()).ok();
                    }
                }
            }
        }

        println!("\n\u{26a0}\u{fe0f}  Moss did not respond after restart (this may be normal)");
        println!("   Check garden-moss status: systemctl status garden-moss.service");
    }

    Ok(())
}
