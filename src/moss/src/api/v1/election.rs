//! Election API endpoints - distributed election protocol testing

use axum::{extract::State, http::StatusCode, Json};
use garden_common::election::{ElectionType, ScoreMechanism};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::app_state::AppState;

#[derive(Debug, Deserialize)]
pub struct StartElectionRequest {
    pub election_type: ElectionType,
    #[serde(default)]
    pub criteria: Value,
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    #[serde(default)]
    pub score_mechanism: ScoreMechanism,
}

fn default_timeout() -> u64 {
    10
}

#[derive(Debug, Serialize)]
pub struct StartElectionResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub winner: Option<ElectionWinnerInfo>,
    pub election_id: String,
    pub candidates_count: usize,
}

#[derive(Debug, Serialize)]
pub struct ElectionWinnerInfo {
    pub stone_id: String,
    pub stone_name: String,
}

/// POST /api/v1/election/start - Start a distributed election
pub async fn start_election(
    State(state): State<AppState>,
    Json(req): Json<StartElectionRequest>,
) -> Result<Json<StartElectionResponse>, (StatusCode, Json<Value>)> {
    use garden_common::utils::ids::generate_guidv7;

    tracing::info!(
        election_type = ?req.election_type,
        criteria = ?req.criteria,
        timeout = req.timeout,
        "Election API: Starting election"
    );

    // Generate election ID
    let election_id = generate_guidv7();

    // Get election service from app state (it's Arc, no lock needed)
    let election_service = state.election_service.clone();

    // Start election
    match election_service
        .start_election(
            election_id.clone(),
            req.election_type,
            req.criteria,
            req.timeout,
            req.score_mechanism,
        )
        .await
    {
        Ok(winner) => {
            let response = StartElectionResponse {
                winner: winner.as_ref().map(|w| ElectionWinnerInfo {
                    stone_id: w.stone_id.clone(),
                    stone_name: w.stone_name.clone(),
                }),
                election_id: election_id.clone(),
                candidates_count: if winner.is_some() { 1 } else { 0 },
            };

            if let Some(ref w) = winner {
                tracing::info!(
                    election_id = %election_id,
                    winner_id = %w.stone_id,
                    winner_name = %w.stone_name,
                    "Election completed with winner"
                );
            } else {
                tracing::warn!(
                    election_id = %election_id,
                    "Election completed with no candidates"
                );
            }

            Ok(Json(response))
        }
        Err(e) => {
            tracing::error!(
                election_id = %election_id,
                error = ?e,
                "Election failed"
            );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Election failed",
                    "message": e.to_string()
                })),
            ))
        }
    }
}
