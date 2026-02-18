//! Command routing — maps ArgMatches to command objects and dispatches them
//!
//! This module replaces the old enum-based pattern matching with string-based
//! routing on `clap::ArgMatches`. The command manifest is the single source of
//! truth for CLI structure; this module is the bridge between what Clap parses
//! and the actual command handlers.
//!
//! ## Design
//!
//! `route_command` receives the subcommand name as a `&str` and the associated
//! `ArgMatches`. It extracts arguments, constructs the appropriate command
//! struct, and delegates to `dispatch::dispatch` / `dispatch::dispatch_local` /
//! `dispatch::dispatch_full` which handle endpoint resolution, stone headers,
//! and error formatting.

use crate::dispatch;

use garden_common::ui::rendering as ui;
use garden_rake::commands;
use garden_rake::commands::Command;
use garden_rake::stone_cache::GLOBAL_CACHE;

use base64::Engine;
use std::time::Duration;

/// Route a parsed CLI subcommand to the appropriate command handler.
///
/// This is the core dispatcher: it extracts values from `ArgMatches`, constructs
/// the appropriate command objects, and delegates execution to
/// [`dispatch::dispatch`] (which handles endpoint resolution, stone headers,
/// etc.).
///
/// # Arguments
///
/// * `name`    — The matched subcommand name (e.g. `"status"`, `"offer"`)
/// * `matches` — The `ArgMatches` for that subcommand
/// * `global`  — Global flags extracted at the top level
/// * `client`  — Shared HTTP client
/// * `term`    — Terminal capabilities (colour support, width)
#[allow(clippy::too_many_arguments)]
pub async fn route_command(
    name: &str,
    matches: &clap::ArgMatches,
    global: &garden_rake::cli_build::GlobalFlags,
    client: &reqwest::Client,
    term: &garden_common::ui::rendering::TerminalInfo,
) -> anyhow::Result<()> {
    let output_format: garden_rake::context::OutputFormat =
        global.output.parse().unwrap_or_default();
    let field = global.field.clone();

    match name {
        // =================================================================
        // Discovery
        // =================================================================

        "status" => {
            let at = matches.get_one::<String>("at").cloned();
            let cmd = commands::discovery::StatusCommand::new(global.quiet);
            dispatch::dispatch(
                &cmd,
                client,
                at,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        "list" => {
            let at = matches.get_one::<String>("at").cloned();
            let cmd = commands::discovery::ListCommand::new(global.quiet);
            dispatch::dispatch(
                &cmd,
                client,
                at,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        "find" => {
            let query = matches
                .get_one::<String>("query")
                .cloned()
                .unwrap_or_default();
            let format_str = matches
                .get_one::<String>("format")
                .cloned()
                .unwrap_or_else(|| "human".to_string());
            let wishfully = matches.get_flag("wishfully");
            let at = matches.get_one::<String>("at").cloned();

            // Global --output/--field can override command-specific --format
            let find_format: commands::discovery::FindOutputFormat =
                if field.is_some() || output_format.is_json() {
                    commands::discovery::FindOutputFormat::Json
                } else {
                    format_str.parse().unwrap_or_default()
                };

            let cmd = commands::discovery::FindCommand::with_field(
                query,
                find_format,
                global.quiet,
                global.fresh,
                wishfully,
                field.clone(),
            );
            dispatch::dispatch(
                &cmd,
                client,
                at,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        "config" => {
            let service = matches
                .get_one::<String>("service")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Missing required argument: service"))?;
            let at = matches.get_one::<String>("at").cloned();
            let json_output = output_format.is_json() || field.is_some();
            let cmd = commands::discovery::ConfigCommand::new(
                service,
                global.quiet,
                json_output,
                field.clone(),
            );
            dispatch::dispatch(
                &cmd,
                client,
                at,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        "observe" => {
            let stone = matches.get_one::<String>("stone").cloned();
            let offering = matches.get_one::<String>("offering").cloned();
            let cmd =
                commands::discovery::ObserveCommand::new(stone, offering, global.quiet);
            dispatch::dispatch_local(
                &cmd,
                client,
                global.quiet,
                global.fresh,
                global.verbose,
            )
            .await?;
        }

        // =================================================================
        // Watch (subcommands: offering/logs, stone/logs, events)
        // =================================================================

        "watch" => {
            let until = matches.get_one::<String>("until").cloned();
            let at = matches.get_one::<String>("at").cloned();

            let cmd = match matches.subcommand() {
                Some(("offering", sub_m)) => {
                    let name = sub_m
                        .get_one::<String>("name")
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("Missing offering name"))?;
                    match sub_m.subcommand() {
                        Some(("logs", logs_m)) => {
                            // TODO: --timestamps flag not yet in manifest SubDef
                            let timestamps = logs_m.get_flag("timestamps");
                            commands::discovery::WatchCommand::offering_logs(
                                name, timestamps, global.quiet,
                            )
                        }
                        _ => {
                            anyhow::bail!(
                                "Usage: garden-rake watch offering <name> logs"
                            );
                        }
                    }
                }
                Some(("stone", sub_m)) => {
                    let name = sub_m
                        .get_one::<String>("name")
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("Missing stone name"))?;
                    match sub_m.subcommand() {
                        Some(("logs", logs_m)) => {
                            // TODO: --timestamps flag not yet in manifest SubDef
                            let timestamps = logs_m.get_flag("timestamps");
                            commands::discovery::WatchCommand::stone_logs(
                                name, timestamps, global.quiet,
                            )
                        }
                        _ => {
                            anyhow::bail!(
                                "Usage: garden-rake watch stone <name> logs"
                            );
                        }
                    }
                }
                _ => commands::discovery::WatchCommand::events(until, global.quiet),
            };

            dispatch::dispatch(
                &cmd,
                client,
                at,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        // =================================================================
        // Capabilities (subcommands: add, remove, refresh, mirror, list)
        // =================================================================

        "capabilities" => {
            let at = matches.get_one::<String>("at").cloned();

            match matches.subcommand() {
                Some(("add", sub_m)) => {
                    let offering = sub_m
                        .get_one::<String>("offering")
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("Missing offering"))?;
                    let name = sub_m
                        .get_one::<String>("name")
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("Missing capability name"))?;
                    let cap_type = sub_m.get_one::<String>("type").cloned();
                    let dry_run = sub_m.get_flag("dry-run");
                    let cmd = commands::discovery::AddCapabilityCommand::new(
                        offering, name, cap_type, dry_run, global.quiet,
                    );
                    dispatch::dispatch(
                        &cmd,
                        client,
                        at,
                        global.quiet,
                        global.fresh,
                        global.verbose,
                        Some(&*GLOBAL_CACHE),
                    )
                    .await?;
                }
                Some(("remove", sub_m)) => {
                    let offering = sub_m
                        .get_one::<String>("offering")
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("Missing offering"))?;
                    let name = sub_m
                        .get_one::<String>("name")
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("Missing capability name"))?;
                    let cap_type = sub_m.get_one::<String>("type").cloned();
                    let cmd = commands::discovery::RemoveCapabilityCommand::new(
                        offering, name, cap_type, global.quiet,
                    );
                    dispatch::dispatch(
                        &cmd,
                        client,
                        at,
                        global.quiet,
                        global.fresh,
                        global.verbose,
                        Some(&*GLOBAL_CACHE),
                    )
                    .await?;
                }
                Some(("refresh", sub_m)) => {
                    let offering = sub_m
                        .get_one::<String>("offering")
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("Missing offering"))?;
                    let cap_type = sub_m.get_one::<String>("type").cloned();
                    let dry_run = sub_m.get_flag("dry-run");
                    let cmd = commands::discovery::RefreshCapabilitiesCommand::new(
                        offering, cap_type, dry_run, global.quiet,
                    );
                    dispatch::dispatch(
                        &cmd,
                        client,
                        at,
                        global.quiet,
                        global.fresh,
                        global.verbose,
                        Some(&*GLOBAL_CACHE),
                    )
                    .await?;
                }
                Some(("mirror", sub_m)) => {
                    let offering = sub_m
                        .get_one::<String>("offering")
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("Missing offering"))?;
                    let args: Vec<String> = sub_m
                        .get_many::<String>("args")
                        .map(|v| v.cloned().collect())
                        .unwrap_or_default();
                    let cmd = commands::discovery::MirrorCapabilitiesCommand::new(
                        offering, args, global.quiet,
                    );
                    dispatch::dispatch(
                        &cmd,
                        client,
                        at,
                        global.quiet,
                        global.fresh,
                        global.verbose,
                        Some(&*GLOBAL_CACHE),
                    )
                    .await?;
                }
                _ => {
                    // No subcommand → list capabilities for the specified offering
                    let offering = matches
                        .get_one::<String>("offering")
                        .cloned()
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "Offering required: garden-rake capabilities <offering>"
                            )
                        })?;
                    let cmd = commands::discovery::CapabilitiesCommand::new(
                        offering,
                        global.quiet,
                    );
                    dispatch::dispatch(
                        &cmd,
                        client,
                        at,
                        global.quiet,
                        global.fresh,
                        global.verbose,
                        Some(&*GLOBAL_CACHE),
                    )
                    .await?;
                }
            }
        }

        // =================================================================
        // Offering (complex: placement, anywhere, info, install/query)
        // =================================================================

        "offer" => {
            let offering = matches.get_one::<String>("offering").cloned();
            let at = matches.get_one::<String>("at").cloned();
            let prefer: Vec<String> = matches
                .get_many::<String>("prefer")
                .map(|v| v.cloned().collect())
                .unwrap_or_default();
            let anywhere_on_fail = matches.get_flag("anywhere-on-fail");
            let placement_mode = matches.get_one::<String>("placement-mode").cloned();

            // Check for "info" subcommand
            let has_info_sub = matches.subcommand_matches("info").is_some();

            // ── Handle --placement-mode ──────────────────────────────
            if let Some(mode) = &placement_mode {
                if let Some(name) = &offering {
                    let is_quiet = mode == "auto" || global.quiet;
                    let cmd = commands::offering::OfferCommand::placement_recommend(
                        name.to_string(),
                        is_quiet,
                    );
                    dispatch::dispatch_local(
                        &cmd,
                        client,
                        global.quiet,
                        global.fresh,
                        global.verbose,
                    )
                    .await?;
                } else {
                    anyhow::bail!(
                        "Usage: garden-rake offer <offering> --placement-mode <interactive|auto>"
                    );
                }
                return Ok(());
            }

            // ── Handle --at anywhere (query across all stones) ───────
            if at.as_deref() == Some("anywhere") {
                if let Some(name) = &offering {
                    if name == "refresh" {
                        anyhow::bail!(
                            "'offer refresh' requires a specific stone (remove --at anywhere)"
                        );
                    }
                    let cmd = commands::offering::OfferCommand::query_anywhere(
                        name.to_string(),
                        prefer,
                        global.quiet,
                    );
                    dispatch::dispatch_local(
                        &cmd,
                        client,
                        global.quiet,
                        global.fresh,
                        global.verbose,
                    )
                    .await?;
                } else {
                    anyhow::bail!(
                        "Usage with --at anywhere: garden-rake offer <query> --at anywhere [--prefer <token>]"
                    );
                }
                return Ok(());
            }

            // ── Normal offer routing ─────────────────────────────────
            let cmd = match (offering.as_deref(), has_info_sub) {
                (None, _) => {
                    // List all offerings
                    commands::offering::OfferCommand::list(global.quiet)
                }
                (Some("refresh"), _) => {
                    // Refresh offerings index
                    commands::offering::OfferCommand::refresh(global.quiet)
                }
                (Some(name), true) => {
                    // Show offering info
                    commands::offering::OfferCommand::info(name.to_string(), global.quiet)
                }
                (Some(name), false) => {
                    // Could be install or query — check if known offering
                    let endpoint = dispatch::resolve_endpoint(
                        client,
                        at.clone(),
                        Some(&*GLOBAL_CACHE),
                    )
                    .await?;
                    let is_known = commands::offering::OfferCommand::is_known_offering(
                        client, &endpoint, name,
                    )
                    .await;

                    if name != "refresh" && !is_known {
                        // Treat as query
                        let cmd = commands::offering::OfferCommand::query(
                            name.to_string(),
                            prefer.clone(),
                            global.quiet,
                        );
                        let ctx = garden_rake::CommandContext::with_endpoint(
                            client.clone(),
                            endpoint.clone(),
                            None,
                            global.quiet,
                            false,
                            global.verbose,
                        );
                        cmd.execute(&ctx).await?;
                        return Ok(());
                    }

                    // Install the offering
                    commands::offering::OfferCommand::install(
                        name.to_string(),
                        prefer,
                        anywhere_on_fail,
                        global.quiet,
                    )
                }
            };

            dispatch::dispatch(
                &cmd,
                client,
                at,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        // =================================================================
        // Lifecycle
        // =================================================================

        "remove" => {
            let service = matches
                .get_one::<String>("service")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Missing required argument: service"))?;
            let force = matches.get_flag("force");
            let at = matches.get_one::<String>("at").cloned();
            let cmd = commands::lifecycle::RemoveCommand::new(service, force, global.quiet);
            dispatch::dispatch(
                &cmd,
                client,
                at,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        "uproot" => {
            let service = matches
                .get_one::<String>("service")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Missing required argument: service"))?;
            let force = matches.get_flag("force");
            let at = matches.get_one::<String>("at").cloned();
            let cmd = commands::lifecycle::UprootCommand::new(service, force, global.quiet);
            dispatch::dispatch(
                &cmd,
                client,
                at,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        "upgrade" => {
            let service = matches.get_one::<String>("service").cloned();
            let all = matches.get_flag("all");
            let at = matches.get_one::<String>("at").cloned();
            let cmd = commands::lifecycle::UpgradeCommand::new(service, all, global.quiet);
            dispatch::dispatch(
                &cmd,
                client,
                at,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        "rest" => {
            let service = matches
                .get_one::<String>("service")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Missing required argument: service"))?;
            let at = matches.get_one::<String>("at").cloned();
            let cmd = commands::lifecycle::RestCommand::new(service, global.quiet);
            dispatch::dispatch(
                &cmd,
                client,
                at,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        "wake" => {
            let service = matches
                .get_one::<String>("service")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Missing required argument: service"))?;
            let at = matches.get_one::<String>("at").cloned();
            let cmd = commands::lifecycle::WakeCommand::new(service, global.quiet);
            dispatch::dispatch(
                &cmd,
                client,
                at,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        // =================================================================
        // Adoption
        // =================================================================

        "adopt" => {
            let container = matches
                .get_one::<String>("target")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Missing required argument: target"))?;
            let at = matches.get_one::<String>("at").cloned();
            let cmd = commands::adoption::AdoptCommand::new(container, global.quiet);
            dispatch::dispatch(
                &cmd,
                client,
                at,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        "release" => {
            let service = matches
                .get_one::<String>("service")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Missing required argument: service"))?;
            let at = matches.get_one::<String>("at").cloned();
            let cmd = commands::adoption::ReleaseCommand::new(service, global.quiet);
            dispatch::dispatch(
                &cmd,
                client,
                at,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        "adopted" => {
            let at = matches.get_one::<String>("at").cloned();
            let cmd = commands::discovery::AdoptedCommand::new(global.quiet);
            dispatch::dispatch(
                &cmd,
                client,
                at,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        "borrowed" => {
            let at = matches.get_one::<String>("at").cloned();
            let cmd = commands::discovery::BorrowedCommand::new(global.quiet);
            dispatch::dispatch(
                &cmd,
                client,
                at,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        "borrow" => {
            let name = matches
                .get_one::<String>("name")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Missing required argument: name"))?;
            let from = matches.get_one::<String>("from").cloned();
            let at = matches.get_one::<String>("at").cloned();
            let from_url = from.ok_or_else(|| {
                anyhow::anyhow!(
                    "Missing URL. Use: garden-rake borrow {} from <url>",
                    name
                )
            })?;
            let cmd =
                commands::adoption::BorrowCommand::new(name, from_url, global.quiet);
            dispatch::dispatch(
                &cmd,
                client,
                at,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        "return" => {
            let name = matches
                .get_one::<String>("name")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Missing required argument: name"))?;
            let at = matches.get_one::<String>("at").cloned();
            let cmd = commands::adoption::ReturnCommand::new(name, global.quiet);
            dispatch::dispatch(
                &cmd,
                client,
                at,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        "locate" => {
            let at = matches.get_one::<String>("at").cloned();
            match matches.subcommand() {
                Some(("strays", _)) => {
                    let cmd =
                        commands::adoption::LocateStraysCommand::new(global.quiet);
                    dispatch::dispatch(
                        &cmd,
                        client,
                        at,
                        global.quiet,
                        global.fresh,
                        global.verbose,
                        Some(&*GLOBAL_CACHE),
                    )
                    .await?;
                }
                _ => {
                    anyhow::bail!("Usage: garden-rake locate strays");
                }
            }
        }

        // =================================================================
        // Management
        // =================================================================

        "place" => {
            let target = matches
                .get_one::<String>("target")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Missing required argument: target"))?;
            let code = matches.get_one::<String>("code").cloned();
            let passphrase = matches.get_one::<String>("passphrase").cloned();
            let at = matches.get_one::<String>("at").cloned();
            match commands::management::PlaceCommand::from_args(
                target, code, passphrase, global.quiet,
            ) {
                Ok(cmd) => {
                    dispatch::dispatch(
                        &cmd,
                        client,
                        at,
                        global.quiet,
                        global.fresh,
                        global.verbose,
                        Some(&*GLOBAL_CACHE),
                    )
                    .await?;
                }
                Err(e) => {
                    eprintln!(
                        "{}{} {}",
                        " ".repeat(ui::constants::DEFAULT_INDENT),
                        ui::status_indicator("error", term.supports_color),
                        e
                    );
                }
            }
        }

        "invite" => {
            let at = matches.get_one::<String>("at").cloned();
            let cmd = commands::management::InviteCommand::new(global.quiet);
            dispatch::dispatch(
                &cmd,
                client,
                at,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        "reconcile" => {
            let drop_invalid = matches.get_flag("drop-invalid");
            let at = matches.get_one::<String>("at").cloned();
            let cmd =
                commands::management::ReconcileCommand::new(drop_invalid, global.quiet);
            dispatch::dispatch(
                &cmd,
                client,
                at,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        "tend" => {
            let target = matches.get_one::<String>("target").cloned();
            let clear = matches.get_flag("clear");
            let verbose = global.verbose > 0;
            let cmd = commands::management::TendCommand::new(target, clear, verbose);
            dispatch::dispatch_local(
                &cmd,
                client,
                global.quiet,
                global.fresh,
                global.verbose,
            )
            .await?;
        }

        // =================================================================
        // Pond (subcommands)
        // =================================================================

        "pond" => {
            let at = matches.get_one::<String>("at").cloned();
            use commands::management::PondActionType;

            let action_type = match matches.subcommand() {
                Some(("init", sub_m)) => PondActionType::Init {
                    passphrase: sub_m.get_one::<String>("passphrase").cloned(),
                    profile: sub_m.get_one::<String>("profile").cloned(),
                },
                Some(("status", _)) => PondActionType::Status,
                Some(("invite", sub_m)) => PondActionType::Invite {
                    passphrase: sub_m.get_one::<String>("passphrase").cloned(),
                },
                Some(("join", sub_m)) => PondActionType::Join {
                    code: sub_m
                        .get_one::<String>("code")
                        .cloned()
                        .ok_or_else(|| {
                            anyhow::anyhow!("Missing required argument: code")
                        })?,
                },
                Some(("enroll", _)) => PondActionType::Enroll,
                Some(("trust", _)) => PondActionType::Trust,
                Some(("unlock", sub_m)) => PondActionType::Unlock {
                    passphrase: sub_m.get_one::<String>("passphrase").cloned(),
                    totp: sub_m.get_one::<String>("totp").cloned(),
                },
                Some(("remove", _)) => PondActionType::Remove,
                Some(("untrust", sub_m)) => PondActionType::Untrust {
                    stone_name: sub_m
                        .get_one::<String>("stone")
                        .cloned()
                        .ok_or_else(|| {
                            anyhow::anyhow!("Missing required argument: stone")
                        })?,
                },
                Some(("promote", sub_m)) => PondActionType::Promote {
                    passphrase: sub_m.get_one::<String>("passphrase").cloned(),
                },
                Some(("rename", sub_m)) => PondActionType::Rename {
                    name: sub_m.get_one::<String>("name").cloned(),
                },
                _ => {
                    anyhow::bail!(
                        "Usage: garden-rake pond <init|status|invite|join|enroll|trust|unlock|remove|untrust|promote|rename>"
                    );
                }
            };

            let cmd = commands::management::PondCommand::new(action_type, global.quiet);
            dispatch::dispatch_full(
                &cmd,
                client,
                at,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
                output_format,
                field.clone(),
            )
            .await?;
        }

        // =================================================================
        // Lift
        // =================================================================

        "lift" => {
            let target_type = matches
                .get_one::<String>("target_type")
                .cloned()
                .ok_or_else(|| {
                    anyhow::anyhow!("Missing required argument: target_type")
                })?;
            let stone_name = matches.get_one::<String>("stone_name").cloned();
            let at = matches.get_one::<String>("at").cloned();

            use commands::management::LiftTarget;
            let target = match target_type.as_str() {
                "keystone" => LiftTarget::Keystone,
                "stone" => {
                    if stone_name.is_none() {
                        eprintln!(
                            "{}{} Error: stone name required for 'lift stone'",
                            " ".repeat(ui::constants::DEFAULT_INDENT),
                            ui::status_indicator("error", term.supports_color)
                        );
                        eprintln!(
                            "{}Example: garden-rake lift stone stone-02",
                            " ".repeat(ui::constants::DEFAULT_INDENT)
                        );
                        return Ok(());
                    }
                    LiftTarget::Stone {
                        name: stone_name.unwrap(),
                    }
                }
                _ => {
                    eprintln!(
                        "{}{} Invalid target: '{}'. Use 'keystone' or 'stone'",
                        " ".repeat(ui::constants::DEFAULT_INDENT),
                        ui::status_indicator("error", term.supports_color),
                        target_type
                    );
                    return Ok(());
                }
            };

            let cmd = commands::management::LiftCommand::new(target, global.quiet);
            dispatch::dispatch(
                &cmd,
                client,
                at,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        // =================================================================
        // Make (subcommands: sing, quiet, silent, minimal)
        // =================================================================

        "make" => {
            let target = matches
                .get_one::<String>("target")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Missing required argument: target"))?;
            let at = matches.get_one::<String>("at").cloned();

            if target != "stone" {
                eprintln!(
                    "{}{} Invalid target: '{}'. Use 'stone'",
                    " ".repeat(ui::constants::DEFAULT_INDENT),
                    ui::status_indicator("error", term.supports_color),
                    target
                );
                eprintln!(
                    "{}Example: garden-rake make stone sing",
                    " ".repeat(ui::constants::DEFAULT_INDENT)
                );
                return Ok(());
            }

            use commands::management::MakeActionType;
            let action_type = match matches.subcommand() {
                Some(("sing", sub_m)) => {
                    let forever = sub_m.get_flag("forever");
                    MakeActionType::Sing { forever }
                }
                Some(("quiet", _)) => MakeActionType::Quiet,
                Some(("silent", _)) => MakeActionType::Silent,
                Some(("minimal", _)) => MakeActionType::Minimal,
                _ => {
                    anyhow::bail!(
                        "Usage: garden-rake make stone <sing|quiet|silent|minimal>"
                    );
                }
            };

            let cmd = commands::management::MakeCommand::new(action_type, global.quiet);
            dispatch::dispatch(
                &cmd,
                client,
                at,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        // =================================================================
        // Admin
        // =================================================================

        "take-root" => {
            // Zen syntax: "garden-rake take-root at windows-01"
            // In the old Clap derive, first positional was "at" keyword,
            // second was the stone name. Fall back to --at flag for regular usage.
            let at_keyword = matches.get_one::<String>("at_keyword").cloned();
            let stone = matches.get_one::<String>("stone").cloned();
            let at_flag = matches.get_one::<String>("at").cloned();

            let target = if at_keyword.as_deref() == Some("at") {
                stone.clone()
            } else {
                // at_keyword might itself be the stone name (backward compat)
                at_keyword.or(at_flag)
            };

            let cmd = commands::admin::InstallServiceCommand::take_root(global.quiet);
            dispatch::dispatch(
                &cmd,
                client,
                target,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        "install-service" => {
            let at = matches.get_one::<String>("at").cloned();
            let cmd =
                commands::admin::InstallServiceCommand::install_service(global.quiet);
            dispatch::dispatch(
                &cmd,
                client,
                at,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        "rouse" => {
            let stone = matches
                .get_one::<String>("stone")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Missing required argument: stone"))?;
            let at = matches.get_one::<String>("at").cloned();
            let cmd = commands::admin::RouseCommand::new(stone, global.quiet);
            dispatch::dispatch(
                &cmd,
                client,
                at,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        "slumber" => {
            // Merge: positional stone takes precedence, then --at
            let stone = matches.get_one::<String>("stone").cloned();
            let at = matches.get_one::<String>("at").cloned();
            let target = stone.or(at);
            let cmd = commands::admin::SlumberCommand::new(global.quiet);
            dispatch::dispatch(
                &cmd,
                client,
                target,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        "stir" => {
            // Merge: positional stone takes precedence, then --at
            let stone = matches.get_one::<String>("stone").cloned();
            let at = matches.get_one::<String>("at").cloned();
            let target = stone.or(at);
            let cmd = commands::admin::StirCommand::new(global.quiet);
            dispatch::dispatch(
                &cmd,
                client,
                target,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        // =================================================================
        // Hey (tell)
        // =================================================================

        "hey" => {
            let args: Vec<String> = matches
                .get_many::<String>("tell")
                .map(|v| v.cloned().collect())
                .unwrap_or_default();
            let at = matches.get_one::<String>("at").cloned();
            let cmd = commands::hey::HeyTellCommand { args };
            dispatch::dispatch(
                &cmd,
                client,
                at,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        // =================================================================
        // Nourish
        // =================================================================

        "nourish" => {
            // Manifest has positional "service" (Optional) and flag "all".
            // Constructor: NourishCommand::new(stone, updates_only, auto_confirm)
            // The positional "service" maps to the old "stone" parameter.
            // TODO: Manifest args (service, all) don't perfectly map to constructor
            // params (stone, updates_only, auto_confirm). Using best-effort mapping.
            let stone = matches.get_one::<String>("service").cloned();
            let _all = matches.get_flag("all");
            let updates_only = false;
            let auto_confirm = false;
            let cmd = commands::nourish::NourishCommand::new(
                stone,
                updates_only,
                auto_confirm,
            );
            dispatch::dispatch_local(
                &cmd,
                client,
                global.quiet,
                global.fresh,
                global.verbose,
            )
            .await?;
        }

        // =================================================================
        // Presence
        // =================================================================

        "presence" => {
            // NOTE: Manifest has "categories" option. Old handler expected positional "stone".
            // Passing categories as the first param; handler may need updating to match.
            let categories = matches.get_one::<String>("categories").cloned();
            let at = matches.get_one::<String>("at").cloned();
            commands::presence::presence_command(
                categories,
                at,
                client,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        // =================================================================
        // Storage
        // =================================================================

        "seed-banks" => {
            let at = matches.get_one::<String>("at").cloned();
            let cmd = commands::storage::ShowSeedBanksCommand::new();
            dispatch::dispatch(
                &cmd,
                client,
                at,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        "release-seed-bank" => {
            let name = matches
                .get_one::<String>("name")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Missing required argument: name"))?;
            let at = matches.get_one::<String>("at").cloned();
            let cmd = commands::storage::ReleaseSeedBankCommand::new(name);
            dispatch::dispatch(
                &cmd,
                client,
                at,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        "prepare" => {
            // The old code required target=="seed-bank" as first positional.
            // In the manifest, "device" is the first positional (the old "target"
            // was removed). We pass device directly to the command.
            // TODO: Validate alignment between manifest and old Prepare variant.
            let device = matches.get_one::<String>("device").cloned();
            let name = matches.get_one::<String>("name").cloned();
            let random = matches.get_flag("random");
            let fs = matches.get_one::<String>("fs").cloned();
            let encrypted = matches.get_flag("encrypted");
            let at = matches.get_one::<String>("at").cloned();
            let cmd = commands::storage::PrepareSeedBankCommand::new(
                device, name, random, fs, encrypted,
            );
            dispatch::dispatch(
                &cmd,
                client,
                at,
                global.quiet,
                global.fresh,
                global.verbose,
                Some(&*GLOBAL_CACHE),
            )
            .await?;
        }

        "store" => {
            let operation = matches
                .get_one::<String>("operation")
                .cloned()
                .ok_or_else(|| {
                    anyhow::anyhow!("Missing required argument: operation")
                })?;
            let bucket = matches
                .get_one::<String>("bucket")
                .cloned()
                .ok_or_else(|| {
                    anyhow::anyhow!("Missing required argument: bucket")
                })?;
            let key = matches.get_one::<String>("key").cloned();
            let file = matches.get_one::<String>("file").cloned();
            let prefix = matches.get_one::<String>("prefix").cloned();
            let delimiter = matches.get_one::<String>("delimiter").cloned();
            let app = matches.get_one::<String>("app").cloned();
            let at = matches.get_one::<String>("at").cloned();

            match operation.as_str() {
                "put" => {
                    let key = key.ok_or_else(|| {
                        anyhow::anyhow!("Key required for put operation")
                    })?;
                    let file = file.ok_or_else(|| {
                        anyhow::anyhow!("File required for put operation")
                    })?;
                    let cmd = commands::storage::StorePutCommand::new(
                        bucket,
                        key,
                        std::path::PathBuf::from(file),
                        app,
                    );
                    dispatch::dispatch(
                        &cmd,
                        client,
                        at,
                        global.quiet,
                        global.fresh,
                        global.verbose,
                        Some(&*GLOBAL_CACHE),
                    )
                    .await?;
                }
                "get" => {
                    let key = key.ok_or_else(|| {
                        anyhow::anyhow!("Key required for get operation")
                    })?;
                    let output = file.map(std::path::PathBuf::from);
                    let cmd = commands::storage::StoreGetCommand::new(
                        bucket, key, output, app,
                    );
                    dispatch::dispatch(
                        &cmd,
                        client,
                        at,
                        global.quiet,
                        global.fresh,
                        global.verbose,
                        Some(&*GLOBAL_CACHE),
                    )
                    .await?;
                }
                "ls" | "list" => {
                    // key is used as prefix if no --prefix flag
                    let prefix = prefix.or(key);
                    let cmd = commands::storage::StoreListCommand::new(
                        bucket, prefix, delimiter, app,
                    );
                    dispatch::dispatch(
                        &cmd,
                        client,
                        at,
                        global.quiet,
                        global.fresh,
                        global.verbose,
                        Some(&*GLOBAL_CACHE),
                    )
                    .await?;
                }
                "rm" | "delete" => {
                    let key = key.ok_or_else(|| {
                        anyhow::anyhow!("Key required for delete operation")
                    })?;
                    let cmd = commands::storage::StoreDeleteCommand::new(
                        bucket, key, app,
                    );
                    dispatch::dispatch(
                        &cmd,
                        client,
                        at,
                        global.quiet,
                        global.fresh,
                        global.verbose,
                        Some(&*GLOBAL_CACHE),
                    )
                    .await?;
                }
                "head" | "info" => {
                    let key = key.ok_or_else(|| {
                        anyhow::anyhow!("Key required for head operation")
                    })?;
                    let cmd = commands::storage::StoreHeadCommand::new(
                        bucket, key, app,
                    );
                    dispatch::dispatch(
                        &cmd,
                        client,
                        at,
                        global.quiet,
                        global.fresh,
                        global.verbose,
                        Some(&*GLOBAL_CACHE),
                    )
                    .await?;
                }
                _ => {
                    anyhow::bail!(
                        "Unknown store operation '{}'. Use: put, get, ls, rm, head",
                        operation
                    );
                }
            }
        }

        // =================================================================
        // Election
        // =================================================================

        "election" => {
            // The old code used commands::election::handle_election(ElectionCommand, client)
            // where ElectionCommand was a Clap-derived struct. We construct it manually
            // from ArgMatches here.
            let action = matches.get_one::<String>("action").cloned();
            let election_type = matches.get_one::<String>("election-type").cloned();
            let criteria = matches.get_one::<String>("criteria").cloned();
            let timeout = matches.get_one::<String>("timeout").cloned();

            match action.as_deref() {
                Some("start") => {
                    use garden_common::election::ElectionType;
                    let et = match election_type.as_deref().unwrap_or("update_source") {
                        "update_source" => ElectionType::UpdateSource,
                        "ceremony_coordinator" => ElectionType::CeremonyCoordinator,
                        "replica_target" => ElectionType::ReplicaTarget,
                        "backup_source" => ElectionType::BackupSource,
                        s if s.starts_with("offering_primary:") => {
                            let fqn = s.strip_prefix("offering_primary:").unwrap();
                            ElectionType::OfferingPrimary(fqn.to_string())
                        }
                        custom => ElectionType::Custom(custom.to_string()),
                    };
                    let timeout_secs: u64 = timeout
                        .as_deref()
                        .unwrap_or("10")
                        .parse()
                        .unwrap_or(10);

                    let cmd = commands::election::ElectionCommand {
                        action: commands::election::ElectionAction::Start(
                            commands::election::StartElection {
                                election_type: et,
                                criteria,
                                timeout: timeout_secs,
                            },
                        ),
                    };
                    commands::election::handle_election(cmd, client).await?;
                }
                Some(other) => {
                    anyhow::bail!(
                        "Unknown election action: '{}'. Use: start",
                        other
                    );
                }
                None => {
                    anyhow::bail!(
                        "Usage: garden-rake election start [--election-type <type>] [--criteria <json>] [--timeout <secs>]"
                    );
                }
            }
        }

        // =================================================================
        // Template (subcommands: list, show)
        // =================================================================

        "template" => {
            use commands::local::TemplateAction;
            let at_parent = matches.get_one::<String>("at").cloned();

            match matches.subcommand() {
                Some(("list", sub_m)) => {
                    let at = sub_m
                        .get_one::<String>("at")
                        .cloned()
                        .or(at_parent);
                    let cmd = commands::local::TemplateCommand::new(
                        TemplateAction::List,
                        global.quiet,
                    );
                    dispatch::dispatch(
                        &cmd,
                        client,
                        at,
                        global.quiet,
                        global.fresh,
                        global.verbose,
                        Some(&*GLOBAL_CACHE),
                    )
                    .await?;
                }
                Some(("show", sub_m)) => {
                    let name = sub_m
                        .get_one::<String>("name")
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("Missing template name"))?;
                    let at = sub_m
                        .get_one::<String>("at")
                        .cloned()
                        .or(at_parent);
                    let cmd = commands::local::TemplateCommand::new(
                        TemplateAction::Show { name },
                        global.quiet,
                    );
                    dispatch::dispatch(
                        &cmd,
                        client,
                        at,
                        global.quiet,
                        global.fresh,
                        global.verbose,
                        Some(&*GLOBAL_CACHE),
                    )
                    .await?;
                }
                _ => {
                    anyhow::bail!("Usage: garden-rake template <list|show>");
                }
            }
        }

        // =================================================================
        // Ceremony
        // =================================================================

        "ceremony" => {
            let name = matches.get_one::<String>("workflow").cloned();
            let cmd = commands::local::CeremonyCommand::new(name, global.quiet);
            dispatch::dispatch_local(
                &cmd,
                client,
                global.quiet,
                global.fresh,
                global.verbose,
            )
            .await?;
        }

        // =================================================================
        // Local: commands (browse), launch
        // =================================================================

        "commands" => {
            let name = matches.get_one::<String>("name").cloned();
            let category = matches.get_one::<String>("category").cloned();
            let zen = matches.get_flag("zen");
            let normative = matches.get_flag("normative");
            let cmd = commands::local::BrowseCommand::new(
                name, category, zen, normative,
            );
            dispatch::dispatch_local(
                &cmd,
                client,
                global.quiet,
                global.fresh,
                global.verbose,
            )
            .await?;
        }

        "launch" => {
            let at = matches.get_one::<String>("at").cloned();
            let endpoint =
                dispatch::resolve_endpoint(client, at, Some(&*GLOBAL_CACHE)).await?;
            let cmd = commands::local::LaunchCommand::new(Some(endpoint));
            dispatch::dispatch_local(
                &cmd,
                client,
                global.quiet,
                global.fresh,
                global.verbose,
            )
            .await?;
        }

        // =================================================================
        // API explorer
        // =================================================================

        "api" => {
            let endpoint_path = matches.get_one::<String>("endpoint").cloned();
            let category = matches.get_one::<String>("category").cloned();
            let examples = matches.get_flag("examples");
            let at = matches.get_one::<String>("at").cloned();
            let resolved =
                dispatch::resolve_endpoint(client, at, Some(&*GLOBAL_CACHE)).await?;
            commands::api::execute_api_command(
                &resolved,
                category,
                endpoint_path,
                examples,
            )
            .await?;
        }

        // =================================================================
        // Refresh (binary upload)
        // =================================================================

        "refresh" => {
            let component = matches
                .get_one::<String>("component")
                .cloned()
                .ok_or_else(|| {
                    anyhow::anyhow!("Missing required argument: component")
                })?;
            let from = matches
                .get_one::<String>("from")
                .cloned()
                .ok_or_else(|| {
                    anyhow::anyhow!("Missing required argument: from")
                })?;
            let at = matches.get_one::<String>("at").cloned();
            let endpoint =
                dispatch::resolve_endpoint(client, at, Some(&*GLOBAL_CACHE)).await?;
            println!("Refreshing {}...", component);
            refresh_component(
                client,
                &endpoint,
                &component,
                std::path::Path::new(&from),
            )
            .await?;
        }

        // =================================================================
        // Restore
        // =================================================================

        "restore" => {
            let offering = matches
                .get_one::<String>("offering")
                .cloned()
                .ok_or_else(|| {
                    anyhow::anyhow!("Missing required argument: offering")
                })?;
            let dry_run = matches.get_flag("dry-run");
            let harvest_id = matches.get_one::<String>("harvest-id").cloned();
            let at = matches.get_one::<String>("at").cloned();

            // Collect source words: could be a single --source option or
            // trailing var-args depending on manifest configuration.
            let source_words: Vec<String> = matches
                .get_many::<String>("source")
                .map(|v| v.cloned().collect())
                .unwrap_or_default();

            let source_str = source_words.join(" ").to_lowercase();

            if source_str.contains("seed-bank") || source_str.contains("seedbank") {
                // Remote restore from seed bank
                let seed_bank = source_words
                    .iter()
                    .skip_while(|s| {
                        s.to_lowercase() != "seed-bank"
                            && s.to_lowercase() != "seedbank"
                    })
                    .nth(1)
                    .cloned()
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Missing seed bank name. Usage: garden-rake restore {} from seed-bank <name>",
                            offering
                        )
                    })?;

                let cmd = commands::nurturing::RestoreRemoteCommand::new(
                    offering, seed_bank, harvest_id, dry_run,
                );
                dispatch::dispatch(
                    &cmd,
                    client,
                    at,
                    global.quiet,
                    global.fresh,
                    global.verbose,
                    Some(&*GLOBAL_CACHE),
                )
                .await?;
            } else {
                // Local restore from slot
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

                let cmd = commands::nurturing::RestoreLocalCommand::new(
                    offering, slot, dry_run,
                );
                dispatch::dispatch(
                    &cmd,
                    client,
                    at,
                    global.quiet,
                    global.fresh,
                    global.verbose,
                    Some(&*GLOBAL_CACHE),
                )
                .await?;
            }
        }

        // =================================================================
        // Nurturing (subcommands: status, list, trigger, trigger-all)
        // =================================================================

        "nurturing" => {
            let at = matches.get_one::<String>("at").cloned();

            match matches.subcommand() {
                Some(("status", sub_m)) => {
                    let offering = sub_m.get_one::<String>("offering").cloned();
                    let cmd =
                        commands::nurturing::NurturingStatusCommand::new(offering);
                    dispatch::dispatch(
                        &cmd,
                        client,
                        at,
                        global.quiet,
                        global.fresh,
                        global.verbose,
                        Some(&*GLOBAL_CACHE),
                    )
                    .await?;
                }
                Some(("list", sub_m)) => {
                    let offering = sub_m
                        .get_one::<String>("offering")
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("Missing offering name"))?;
                    let local = sub_m.get_flag("local");
                    let remote = sub_m.get_flag("remote");
                    let cmd = commands::nurturing::NurturingListCommand::new(
                        offering, local, remote,
                    );
                    dispatch::dispatch(
                        &cmd,
                        client,
                        at,
                        global.quiet,
                        global.fresh,
                        global.verbose,
                        Some(&*GLOBAL_CACHE),
                    )
                    .await?;
                }
                Some(("trigger", sub_m)) => {
                    let offering = sub_m
                        .get_one::<String>("offering")
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("Missing offering name"))?;
                    let cmd = commands::nurturing::NurturingTriggerCommand::new(
                        Some(offering),
                    );
                    dispatch::dispatch(
                        &cmd,
                        client,
                        at,
                        global.quiet,
                        global.fresh,
                        global.verbose,
                        Some(&*GLOBAL_CACHE),
                    )
                    .await?;
                }
                Some(("trigger-all", _)) => {
                    let cmd =
                        commands::nurturing::NurturingTriggerCommand::new(None);
                    dispatch::dispatch(
                        &cmd,
                        client,
                        at,
                        global.quiet,
                        global.fresh,
                        global.verbose,
                        Some(&*GLOBAL_CACHE),
                    )
                    .await?;
                }
                _ => {
                    anyhow::bail!(
                        "Usage: garden-rake nurturing <status|list|trigger|trigger-all>"
                    );
                }
            }
        }

        // =================================================================
        // Fallback
        // =================================================================

        _ => {
            anyhow::bail!(
                "Unknown command: '{}'. Run garden-rake --help for available commands.",
                name
            );
        }
    }

    Ok(())
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

    // Normalize component name
    let normalized_component = match component.to_lowercase().as_str() {
        "moss" => "moss",
        "rake" | "garden-rake" => garden_common::constants::RAKE_BINARY,
        _ => bail!("Unknown component '{}'. Use 'moss' or 'rake'", component),
    };

    // Read binary file
    println!("\u{1f4e4} Reading binary file...");
    let binary_data = std::fs::read(binary_path).context(format!(
        "Failed to read binary file: {}",
        binary_path.display()
    ))?;

    let size_mb = binary_data.len() as f64 / 1024.0 / 1024.0;
    println!("   Size: {:.2} MB", size_mb);

    // Basic validation: check for ELF header
    if binary_data.len() < 4 || &binary_data[0..4] != b"\x7fELF" {
        bail!("Not a valid ELF binary. Expected Linux executable.");
    }
    println!("   Format: ELF \u{2713}");

    // Encode to base64
    println!("\u{1f4e6} Encoding binary...");
    let encoded = base64::engine::general_purpose::STANDARD.encode(&binary_data);

    // Send to moss
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

    // Get response body as text first to see what we got
    let body_text = response
        .text()
        .await
        .context("Failed to read response body")?;

    // Try to parse as JSON
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

    // Success
    println!("\u{2705} {} refreshed successfully", normalized_component);

    if let Some(arch) = body.get("architecture").and_then(|v| v.as_str()) {
        println!("   Architecture: {}", arch);
    }

    if normalized_component == "moss" {
        println!("\u{23f3} Moss is restarting...");
        println!("   (This may take a few seconds)");

        // Wait a moment for moss to restart
        tokio::time::sleep(Duration::from_secs(3)).await;

        // Try to ping moss
        let health_url = format!("{}/health", endpoint.trim_end_matches('/'));
        for attempt in 1..=5 {
            tokio::time::sleep(Duration::from_secs(1)).await;
            match client
                .get(&health_url)
                .timeout(Duration::from_secs(2))
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    println!("\u{2705} Moss is back online");
                    return Ok(());
                }
                _ => {
                    if attempt < 5 {
                        print!(".");
                        std::io::Write::flush(&mut std::io::stdout()).ok();
                    }
                }
            }
        }

        println!(
            "\n\u{26a0}\u{fe0f}  Moss did not respond after restart (this may be normal)"
        );
        println!(
            "   Check garden-moss status: systemctl status garden-moss.service"
        );
    }

    Ok(())
}
