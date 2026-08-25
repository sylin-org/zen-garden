//! Election command - test distributed election protocol

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use garden_common::client::StoneApi;
use garden_common::election::ElectionType;
use serde_json::Value;

#[derive(Debug, Args)]
pub struct ElectionCommand {
    #[command(subcommand)]
    pub action: ElectionAction,
}

#[derive(Debug, Subcommand)]
pub enum ElectionAction {
    /// Start an election and await winner
    Start(StartElection),
}

#[derive(Debug, Args)]
pub struct StartElection {
    /// Election type (default: update_source)
    #[arg(long, value_parser = parse_election_type, default_value = "update_source")]
    pub election_type: ElectionType,

    /// Criteria JSON (BSON-style operators)
    #[arg(long)]
    pub criteria: Option<String>,

    /// Timeout in seconds
    #[arg(long, default_value = "10")]
    pub timeout: u64,
}

fn parse_election_type(s: &str) -> Result<ElectionType> {
    match s {
        "update_source" => Ok(ElectionType::UpdateSource),
        "ceremony_coordinator" => Ok(ElectionType::CeremonyCoordinator),
        "replica_target" => Ok(ElectionType::ReplicaTarget),
        "backup_source" => Ok(ElectionType::BackupSource),
        s if s.starts_with("offering_primary:") => {
            let fqn = s.strip_prefix("offering_primary:").unwrap();
            Ok(ElectionType::OfferingPrimary(fqn.to_string()))
        }
        custom => Ok(ElectionType::Custom(custom.to_string())),
    }
}

pub async fn handle_election(cmd: ElectionCommand, client: &reqwest::Client) -> Result<()> {
    match cmd.action {
        ElectionAction::Start(start) => handle_start(start, client).await,
    }
}

async fn handle_start(start: StartElection, client: &reqwest::Client) -> Result<()> {
    use crate::tending;
    use crate::ui::colors::CliFormatter;
    use std::time::Duration;

    let formatter = CliFormatter::new();

    // Parse criteria
    let criteria: Value = if let Some(criteria_str) = &start.criteria {
        serde_json::from_str(criteria_str).context("Failed to parse criteria JSON")?
    } else {
        serde_json::json!({}) // Empty criteria = match all
    };

    println!("Starting election: {:?}", start.election_type);

    if !criteria.as_object().map(|o| o.is_empty()).unwrap_or(true) {
        println!("Criteria: {}", serde_json::to_string_pretty(&criteria)?);
    }

    println!("Timeout: {}s", start.timeout);

    // Use execute_on_stone pattern to hit tended stone
    let (result, responding_stone) = tending::execute_on_stone(
        Duration::from_secs(3),
        Some(|stone_name: &str| {
            println!("Stone '{}' is offline, trying fallback...", stone_name);
        }),
        |candidate| {
            let client = client.clone();
            let endpoint = candidate.endpoint.clone();
            let election_type = start.election_type.clone();
            let criteria = criteria.clone();
            let timeout = start.timeout;

            async move {
                use crate::tending::StoneError;

                let api = StoneApi::new(client, endpoint);

                let payload = serde_json::json!({
                    "election_type": election_type,
                    "criteria": criteria,
                    "timeout": timeout
                });

                api.stone().election_start(&payload).await.map_err(|e| {
                    match e {
                        garden_common::client::StoneApiError::Connection(ce) => {
                            StoneError::ConnectionFailed(format!("HTTP request failed: {}", ce))
                        }
                        garden_common::client::StoneApiError::Http { status, message, .. } => {
                            StoneError::ResponseError(
                                status.as_u16(),
                                format!("Endpoint returned {} - {}", status, message),
                            )
                        }
                        garden_common::client::StoneApiError::HttpRaw { status, body } => {
                            StoneError::ResponseError(
                                status.as_u16(),
                                format!("Endpoint returned {} - {}", status, body),
                            )
                        }
                        other => StoneError::ProcessingError(format!("Failed: {}", other)),
                    }
                })
            }
        },
    )
    .await?;

    println!("Requesting election from {}", responding_stone.stone_name);

    // Display result
    println!();
    if let Some(winner) = result.get("winner") {
        let stone_id = winner
            .get("stone_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let stone_name = winner
            .get("stone_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        println!("{} Election winner: {}", formatter.success("✓"), stone_name);
        println!("Stone ID: {}", stone_id);
    } else {
        println!(
            "{}",
            formatter.warning("No candidates responded within timeout")
        );
    }

    Ok(())
}
