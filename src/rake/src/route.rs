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

use crate::dispatch::{self, CommandInvocation, Runtime};

use garden_common::ui::rendering as ui;
use garden_rake::cli_build::GlobalFlags;
use garden_rake::commands;
use garden_rake::commands::Command;
use garden_rake::stone_cache::GLOBAL_CACHE;

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

        "list" => Inv::remote(commands::discovery::ListCommand::new(g.quiet), m),

        "find" => {
            let query = opt(m, "query").unwrap_or_default();
            let format_str = opt(m, "format").unwrap_or_else(|| "human".into());
            let wishfully = m.get_flag("wishfully");
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
                    wishfully,
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
            commands::lifecycle::RemoveCommand::new(req(m, "service")?, m.get_flag("force"), g.quiet),
            m,
        ),

        "uproot" => Inv::remote(
            commands::lifecycle::UprootCommand::new(req(m, "service")?, m.get_flag("force"), g.quiet),
            m,
        ),

        "upgrade" => Inv::remote(
            commands::lifecycle::UpgradeCommand::new(opt(m, "service"), m.get_flag("all"), g.quiet),
            m,
        ),

        "rest" => Inv::remote(
            commands::lifecycle::RestCommand::new(req(m, "service")?, g.quiet),
            m,
        ),

        "wake" => Inv::remote(
            commands::lifecycle::WakeCommand::new(req(m, "service")?, g.quiet),
            m,
        ),

        "nourish" => {
            let stone = opt(m, "service");
            Inv::local(commands::nourish::NourishCommand::new(stone, false, false))
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

        "adopted" => Inv::remote(commands::discovery::AdoptedCommand::new(g.quiet), m),

        "borrowed" => Inv::remote(commands::discovery::BorrowedCommand::new(g.quiet), m),

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

        "locate" => match m.subcommand() {
            Some(("strays", _)) => Inv::remote(
                commands::adoption::LocateStraysCommand::new(g.quiet),
                m,
            ),
            _ => anyhow::bail!("Usage: garden-rake locate strays"),
        },

        // =================================================================
        // Management
        // =================================================================

        "place" => {
            match commands::management::PlaceCommand::from_args(
                req(m, "target")?,
                opt(m, "code"),
                opt(m, "passphrase"),
                g.quiet,
            ) {
                Ok(cmd) => Inv::remote(cmd, m),
                Err(e) => {
                    eprintln!(
                        "{}{} {}",
                        " ".repeat(ui::constants::DEFAULT_INDENT),
                        ui::status_indicator("error", rt.term.supports_color),
                        e
                    );
                    return Ok(None);
                }
            }
        }

        "invite" => Inv::remote(commands::management::InviteCommand::new(g.quiet), m),

        "reconcile" => Inv::remote(
            commands::management::ReconcileCommand::new(m.get_flag("drop-invalid"), g.quiet),
            m,
        ),

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

        // === Lift ===
        "lift" => {
            use commands::management::LiftTarget;
            let target_type = req(m, "target_type")?;
            let target = match target_type.as_str() {
                "keystone" => LiftTarget::Keystone,
                "stone" => LiftTarget::Stone {
                    name: opt(m, "stone_name").ok_or_else(|| {
                        anyhow::anyhow!("Stone name required: garden-rake lift stone <name>")
                    })?,
                },
                _ => anyhow::bail!(
                    "Invalid target: '{}'. Use 'keystone' or 'stone'",
                    target_type
                ),
            };
            Inv::remote(commands::management::LiftCommand::new(target, g.quiet), m)
        }

        // === Make (subcommands) ===
        "make" => {
            use commands::management::MakeActionType;
            let target = req(m, "target")?;
            if target != "stone" {
                anyhow::bail!(
                    "Invalid target: '{}'. Use: garden-rake make stone <sing|quiet|silent|minimal>",
                    target
                );
            }
            let action_type = match m.subcommand() {
                Some(("sing", sub)) => MakeActionType::Sing {
                    forever: sub.get_flag("forever"),
                },
                Some(("quiet", _)) => MakeActionType::Quiet,
                Some(("silent", _)) => MakeActionType::Silent,
                Some(("minimal", _)) => MakeActionType::Minimal,
                _ => anyhow::bail!("Usage: garden-rake make stone <sing|quiet|silent|minimal>"),
            };
            Inv::remote(commands::management::MakeCommand::new(action_type, g.quiet), m)
        }

        // =================================================================
        // Admin
        // =================================================================

        "take-root" => {
            let at_keyword = opt(m, "at_keyword");
            let stone = opt(m, "stone");
            let at_flag = opt(m, "at");
            let target = if at_keyword.as_deref() == Some("at") {
                stone.clone()
            } else {
                at_keyword.or(at_flag)
            };
            Inv::remote_at(
                commands::admin::InstallServiceCommand::take_root(g.quiet),
                target,
            )
        }

        "install-service" => Inv::remote(
            commands::admin::InstallServiceCommand::install_service(g.quiet),
            m,
        ),

        "rouse" => Inv::remote(
            commands::admin::RouseCommand::new(req(m, "stone")?, g.quiet),
            m,
        ),

        "slumber" => {
            let target = opt(m, "stone").or_else(|| opt(m, "at"));
            Inv::remote_at(commands::admin::SlumberCommand::new(g.quiet), target)
        }

        "stir" => {
            let target = opt(m, "stone").or_else(|| opt(m, "at"));
            Inv::remote_at(commands::admin::StirCommand::new(g.quiet), target)
        }

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
            m.get_flag("zen"),
            m.get_flag("normative"),
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

        "seed-banks" => Inv::remote(commands::storage::ShowSeedBanksCommand::new(), m),

        "release-seed-bank" => Inv::remote(
            commands::storage::ReleaseSeedBankCommand::new(req(m, "name")?),
            m,
        ),

        "prepare" => Inv::remote(
            commands::storage::PrepareSeedBankCommand::new(
                opt(m, "device"),
                opt(m, "name"),
                m.get_flag("random"),
                opt(m, "fs"),
                m.get_flag("encrypted"),
            ),
            m,
        ),

        "store" => return route_store(m, g),

        "restore" => return route_restore(m, g),

        // =================================================================
        // Nurturing (subcommands)
        // =================================================================

        "nurturing" => match m.subcommand() {
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
            _ => anyhow::bail!(
                "Usage: garden-rake nurturing <status|list|trigger|trigger-all>"
            ),
        },

        // =================================================================
        // Special cases — execute directly, bypass Runtime
        // =================================================================

        "launch" => {
            let at = opt(m, "at");
            let endpoint =
                dispatch::resolve_endpoint(&rt.client, at, Some(&*GLOBAL_CACHE)).await?;
            return Ok(Some(Inv::local(
                commands::local::LaunchCommand::new(Some(endpoint)),
            )));
        }

        "api" => {
            let at = opt(m, "at");
            let resolved =
                dispatch::resolve_endpoint(&rt.client, at, Some(&*GLOBAL_CACHE)).await?;
            commands::api::execute_api_command(
                &resolved,
                opt(m, "category"),
                opt(m, "endpoint"),
                m.get_flag("examples"),
            )
            .await?;
            return Ok(None);
        }

        "presence" => {
            commands::presence::presence_command(
                opt(m, "categories"),
                opt(m, "at"),
                &rt.client,
                g.quiet,
                g.fresh,
                g.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
            return Ok(None);
        }

        "election" => return route_election(m, &rt.client).await,

        "refresh" => {
            let component = req(m, "component")?;
            let from = req(m, "from")?;
            let endpoint =
                dispatch::resolve_endpoint(&rt.client, opt(m, "at"), Some(&*GLOBAL_CACHE))
                    .await?;
            println!("Refreshing {}...", component);
            refresh_component(&rt.client, &endpoint, &component, std::path::Path::new(&from))
                .await?;
            return Ok(None);
        }

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

    // ── --placement-mode ──
    if let Some(mode) = &placement_mode {
        if let Some(name) = &offering {
            let is_quiet = mode == "auto" || g.quiet;
            return Ok(Some(Inv::local(
                commands::offering::OfferCommand::placement_recommend(name.to_string(), is_quiet),
            )));
        }
        anyhow::bail!(
            "Usage: garden-rake offer <offering> --placement-mode <interactive|auto>"
        );
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
        (Some(name), true) => {
            commands::offering::OfferCommand::info(name.to_string(), g.quiet)
        }
        (Some(name), false) => {
            // Is it a known offering? Need pre-resolved endpoint to check.
            let endpoint =
                dispatch::resolve_endpoint(&rt.client, at.clone(), Some(&*GLOBAL_CACHE))
                    .await?;
            let is_known = commands::offering::OfferCommand::is_known_offering(
                &rt.client, &endpoint, name,
            )
            .await;

            if !is_known {
                // Query — execute directly with pre-resolved endpoint
                let cmd = commands::offering::OfferCommand::query(
                    name.to_string(),
                    prefer.clone(),
                    g.quiet,
                );
                let ctx = garden_rake::CommandContext::with_endpoint(
                    rt.client.clone(),
                    endpoint,
                    None,
                    g.quiet,
                    false,
                    g.verbose,
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
            .skip_while(|s| {
                s.to_lowercase() != "seed-bank" && s.to_lowercase() != "seedbank"
            })
            .nth(1)
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Missing seed bank name. Usage: garden-rake restore {} from seed-bank <name>",
                    offering
                )
            })?;
        Ok(Some(Inv::remote(
            commands::nurturing::RestoreRemoteCommand::new(offering, seed_bank, harvest_id, dry_run),
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
                s if s.starts_with("offering_primary:") => {
                    ElectionType::OfferingPrimary(
                        s.strip_prefix("offering_primary:").unwrap().to_string(),
                    )
                }
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
    client: &reqwest::Client,
    endpoint: &str,
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
    let url = format!(
        "{}/api/v1/system/refresh",
        endpoint.trim_end_matches('/')
    );
    let response = client
        .post(&url)
        .json(&serde_json::json!({
            "component": normalized_component,
            "binary_data": encoded,
        }))
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .context("Failed to send refresh request")?;

    let status = response.status();
    let body_text = response
        .text()
        .await
        .context("Failed to read response body")?;

    let body: serde_json::Value = match serde_json::from_str(&body_text) {
        Ok(json) => json,
        Err(e) => {
            println!("\u{2717} Invalid JSON response");
            println!("   Status: {}", status);
            println!(
                "   Response body: {}",
                body_text.chars().take(500).collect::<String>()
            );
            bail!("Failed to parse JSON response: {}", e);
        }
    };

    if !status.is_success() {
        println!("\u{2717} Refresh failed");
        println!("   Status: {}", status);
        if let Some(error) = body.get("error") {
            println!("   Error: {}", error);
        }
        if let Some(message) = body.get("message") {
            println!("   Message: {}", message);
        }
        bail!("Refresh request failed with status {}", status);
    }

    println!("\u{2705} {} refreshed successfully", normalized_component);
    if let Some(arch) = body.get("architecture").and_then(|v| v.as_str()) {
        println!("   Architecture: {}", arch);
    }

    if normalized_component == "moss" {
        println!("\u{23f3} Moss is restarting...");
        println!("   (This may take a few seconds)");
        tokio::time::sleep(Duration::from_secs(5)).await;

        let health_url = format!("{}/health", endpoint.trim_end_matches('/'));
        for attempt in 1..=12 {
            tokio::time::sleep(Duration::from_secs(2)).await;
            match client
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

        println!(
            "\n\u{26a0}\u{fe0f}  Moss did not respond after restart (this may be normal)"
        );
        println!("   Check garden-moss status: systemctl status garden-moss.service");
    }

    Ok(())
}
