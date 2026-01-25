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
    tended_stone: &crate::tending::TendedStone,
) -> Result<()> {
    match cmd.action {
        ElectionAction::Start(start) => handle_start(start, tended_stone).await,
    }
}

async fn handle_start(start: StartElection, tended_stone: &crate::tending::TendedStone) -> Result<()> {
    use garden_common::cli_colors::{AnsiColor, CliFormatter};

    let formatter = CliFormatter::new();

    // Parse criteria
    let criteria: Value = if let Some(criteria_str) = &start.criteria {
        serde_json::from_str(criteria_str)
            .context("Failed to parse criteria JSON")?
    } else {
        serde_json::json!({}) // Empty criteria = match all
    };

    formatter.print_info(&format!(
        "Starting election: {:?}",
        start.election_type
    ));
    
    if !criteria.as_object().map(|o| o.is_empty()).unwrap_or(true) {
        formatter.print_detail(&format!("Criteria: {}", serde_json::to_string_pretty(&criteria)?));
    }
    
    formatter.print_detail(&format!("Timeout: {}s", start.timeout));

    // Send request to tended Moss
    let url = format!("{}/api/v1/election/start", tended_stone.endpoint);
    let payload = serde_json::json!({
        "election_type": start.election_type,
        "criteria": criteria,
        "timeout": start.timeout
    });

    formatter.print_detail(&format!("Requesting election from {}", tended_stone.name));

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .json(&payload)
        .timeout(std::time::Duration::from_secs(start.timeout + 5))
        .send()
        .await
        .context("Failed to send election request")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Election request failed: {} - {}", status, body);
    }

    let result: serde_json::Value = response.json().await
        .context("Failed to parse election response")?;

    // Display result
    println!();
    if let Some(winner) = result.get("winner") {
        let stone_id = winner.get("stone_id").and_then(|v| v.as_str()).unwrap_or("unknown");
        let stone_name = winner.get("stone_name").and_then(|v| v.as_str()).unwrap_or("unknown");
        
        formatter.print_with_icon(
            "✓",
            AnsiColor::Green,
            &format!("Election winner: {}", formatter.bold(stone_name))
        );
        formatter.print_detail(&format!("Stone ID: {}", stone_id));
    } else {
        formatter.print_warning("No candidates responded within timeout");
    }

    Ok(())
}
