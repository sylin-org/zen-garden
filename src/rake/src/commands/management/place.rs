//! Place command - pond zen syntax for placement
//!
//! Zen syntax for pond security placement operations:
//! - place keystone: Initialize pond security (equivalent to pond init)
//! - place stone --code <code>: Join pond with invitation code (equivalent to pond join)

use crate::commands::{Command, CommandResult};
use crate::context::Runtime;
use crate::suggestions;
use crate::ui::rendering as ui;
use garden_common::client::StoneApi;

/// Place target type
pub enum PlaceTarget {
    /// Place the keystone (initialize pond)
    Keystone { passphrase: Option<String> },
    /// Place a stone in the pond (join with code)
    Stone { code: String },
}

/// Place command for pond zen syntax operations
pub struct PlaceCommand {
    pub target: PlaceTarget,
    pub quiet: bool,
}

impl PlaceCommand {
    pub fn new(target: PlaceTarget, quiet: bool) -> Self {
        Self { target, quiet }
    }

    /// Create from CLI args
    pub fn from_args(
        target_type: String,
        code: Option<String>,
        passphrase: Option<String>,
        quiet: bool,
    ) -> anyhow::Result<Self> {
        let target = match target_type.as_str() {
            "keystone" => PlaceTarget::Keystone { passphrase },
            "stone" => {
                let code = code.ok_or_else(|| {
                    anyhow::anyhow!("--code required for placing a stone\nExample: garden-rake place stone --code ABC123")
                })?;
                PlaceTarget::Stone { code }
            }
            _ => anyhow::bail!(
                "Invalid target: '{}'. Use 'keystone' or 'stone'",
                target_type
            ),
        };
        Ok(Self::new(target, quiet))
    }
}

impl Command for PlaceCommand {
    fn execute<'a>(&'a self, ctx: &'a Runtime) -> std::pin::Pin<Box<dyn std::future::Future<Output = CommandResult> + Send + 'a>> {
        Box::pin(async move {
            let api = ctx.stone_api()?;

            match &self.target {
                PlaceTarget::Keystone { passphrase } => {
                    execute_place_keystone(ctx, api, passphrase.clone()).await?;
                }
                PlaceTarget::Stone { code } => {
                    execute_place_stone(ctx, api, code).await?;
                }
            }

            // Self-teaching suggestions
            suggestions::print_suggestions("place", self.quiet);

            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        "place"
    }
}

async fn execute_place_keystone(
    ctx: &Runtime,
    api: &StoneApi,
    passphrase: Option<String>,
) -> anyhow::Result<()> {
    let pass = passphrase.unwrap_or_else(|| {
        // In a real implementation, prompt for passphrase securely
        println!(
            "{}{} Using default passphrase. Use --passphrase for custom encryption.",
            " ".repeat(ui::constants::DEFAULT_INDENT),
            ui::status_indicator("info", ctx.term.supports_color)
        );
        "changeme".to_string()
    });

    let payload = serde_json::json!({ "passphrase": pass });

    match api.pond().init(&payload).await {
        Ok(_) => {
            println!(
                "{}{} Pond initialized (keystone placed)",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("ok", ctx.term.supports_color)
            );
        }
        Err(
            garden_common::client::StoneApiError::HttpRaw { status, .. }
            | garden_common::client::StoneApiError::Http { status, .. },
        ) if status == reqwest::StatusCode::NOT_IMPLEMENTED => {
            println!(
                "{}{} Pond security not yet implemented (Phase 3b)",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("info", ctx.term.supports_color)
            );
            println!(
                "{}This command will initialize pond security with encrypted certificates.",
                " ".repeat(ui::constants::DEFAULT_INDENT)
            );
        }
        Err(e) => {
            eprintln!(
                "{}{} Failed to initialize pond: {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                e.display_message()
            );
        }
    }

    Ok(())
}

async fn execute_place_stone(ctx: &Runtime, api: &StoneApi, code: &str) -> anyhow::Result<()> {
    let payload = serde_json::json!({ "code": code });

    match api.pond().join(&payload).await {
        Ok(data) => {
            println!(
                "{}{} Joined pond successfully (stone placed)",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("ok", ctx.term.supports_color)
            );
            // StoneApi unwraps ApiResponse, so `data` is the inner payload
            if let Some(stone_name) = data.get("stone_name").and_then(|s| s.as_str()) {
                println!(
                    "{}Stone: {}",
                    " ".repeat(ui::constants::DEFAULT_INDENT + 3),
                    stone_name
                );
            }
            if let Some(cornerstone) = data.get("cornerstone").and_then(|c| c.as_str()) {
                println!(
                    "{}Cornerstone: {}",
                    " ".repeat(ui::constants::DEFAULT_INDENT + 3),
                    cornerstone
                );
            }
        }
        Err(
            garden_common::client::StoneApiError::HttpRaw { status, .. }
            | garden_common::client::StoneApiError::Http { status, .. },
        ) if status == reqwest::StatusCode::NOT_IMPLEMENTED => {
            println!(
                "{}{} Pond security not yet implemented (Phase 3b)",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("info", ctx.term.supports_color)
            );
            println!(
                "{}This command will join an existing pond using an invitation code.",
                " ".repeat(ui::constants::DEFAULT_INDENT)
            );
        }
        Err(e) => {
            eprintln!(
                "{}{} Failed to join pond: {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                e.display_message()
            );
        }
    }

    Ok(())
}
