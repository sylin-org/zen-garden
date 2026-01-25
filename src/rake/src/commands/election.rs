//! Election command - test distributed election protocol

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
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
    /// Election type
    #[arg(long, value_parser = parse_election_type)]
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
        custom => Ok(ElectionType::Custom(custom.to_string())),
    }
}

pub async fn handle_election(
    cmd: ElectionCommand,
    client: &reqwest::Client,
) -> Result<()> {
    match cmd.action {
        ElectionAction::Start(start) => handle_start(start, client).await,
    }
}

async fn handle_start(start: StartElection, client: &reqwest::Client) -> Result<()> {
    use crate::tending;
    use garden_common::cli_colors::CliFormatter;
    use std::time::Duration;

    let formatter = CliFormatter::new();

    // Parse criteria
    let criteria: Value = if let Some(criteria_str) = &start.criteria {
        serde_json::from_str(criteria_str)
            .context("Failed to parse criteria JSON")?
    } else {
        serde_json::json!({}) // Empty criteria = match all
    };

    println!(
        "Starting election: {:?}",
        start.election_type
    );
    
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
                
                let url = format!("{}/api/v1/election/start", endpoint.trim_end_matches('/'));
                let payload = serde_json::json!({
                    "election_type": election_type,
                    "criteria": criteria,
                    "timeout": timeout
                });

                // Make HTTP request
                let response = client
                    .post(&url)
                    .json(&payload)
                    .timeout(Duration::from_secs(timeout + 5))
                    .send()
                    .await
                    .map_err(|e| StoneError::ConnectionFailed(format!("HTTP request failed: {}", e)))?;

                let status = response.status();
                
                // Check response status
                if !status.is_success() {
                    let body = response.text().await.unwrap_or_default();
                    return Err(StoneError::ResponseError(
                        status.as_u16(),
                        format!("Endpoint returned {} - {}", status, body)
                    ));
                }

                // Parse JSON
                response.json::<Value>().await
                    .map_err(|e| StoneError::ProcessingError(format!("Failed to parse response: {}", e)))
            }
        },
    )
    .await?;

    println!("Requesting election from {}", responding_stone.stone_name);

    // Display result
    println!();
    if let Some(winner) = result.get("winner") {
        let stone_id = winner.get("stone_id").and_then(|v| v.as_str()).unwrap_or("unknown");
        let stone_name = winner.get("stone_name").and_then(|v| v.as_str()).unwrap_or("unknown");
        
        println!("{} Election winner: {}", formatter.success("✓"), stone_name);
        println!("Stone ID: {}", stone_id);
    } else {
        println!("{}", formatter.warning("No candidates responded within timeout"));
    }

    Ok(())
}
