//! Ceremony core types
//!
//! Defines the ceremony lifecycle model for multi-phase, long-running
//! operations like nourishment (updates) and vacate (migration).

use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique ceremony identifier
pub type CeremonyId = String;

/// Ceremony type variants
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CeremonyType {
    /// Update a single offering
    NourishOffering { offering: String },
    /// Update a stone (firmware/BIOS)
    NourishStone { stone: String },
    /// Update all offerings on a stone
    NourishAll,
    /// Move all offerings off a stone
    Vacate { stone: String },
    /// Transfer an offering between stones
    Replant {
        offering: String,
        from: String,
        to: String,
    },
    /// Create a stored offering (portable backup)
    Store { offering: String },
}

impl CeremonyType {
    /// Get the ceremony type name
    pub fn name(&self) -> &'static str {
        match self {
            Self::NourishOffering { .. } => "nourish-offering",
            Self::NourishStone { .. } => "nourish-stone",
            Self::NourishAll => "nourish-all",
            Self::Vacate { .. } => "vacate",
            Self::Replant { .. } => "replant",
            Self::Store { .. } => "store",
        }
    }

    /// Get the primary target of this ceremony
    pub fn target(&self) -> Option<&str> {
        match self {
            Self::NourishOffering { offering } => Some(offering),
            Self::NourishStone { stone } => Some(stone),
            Self::NourishAll => None,
            Self::Vacate { stone } => Some(stone),
            Self::Replant { offering, .. } => Some(offering),
            Self::Store { offering } => Some(offering),
        }
    }
}

/// Ceremony lifecycle state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CeremonyState {
    /// Ceremony created, not yet started
    Initiated,
    /// Planning phase (determining steps)
    Planning,
    /// Actively executing phases
    Executing,
    /// Successfully completed
    Completed,
    /// Failed with error
    Failed,
    /// Rolled back after failure
    RolledBack,
    /// Cancelled by user
    Cancelled,
}

impl CeremonyState {
    /// Check if ceremony has finished (success or failure)
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::RolledBack | Self::Cancelled
        )
    }

    /// Check if ceremony is actively running
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Initiated | Self::Planning | Self::Executing)
    }
}

/// Phase execution state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PhaseState {
    /// Not yet started
    Pending,
    /// Currently executing
    Running,
    /// Finished successfully
    Completed,
    /// Failed with error
    Failed,
    /// Skipped (e.g., recklessly mode skips collect)
    Skipped,
}

/// A phase in a ceremony
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Phase {
    /// Phase name (e.g., "collect", "nourish", "water")
    pub name: String,
    /// Current state
    pub state: PhaseState,
    /// Associated job IDs
    pub jobs: Vec<String>,
    /// When phase started
    pub started_at: Option<DateTime<Utc>>,
    /// When phase completed
    pub completed_at: Option<DateTime<Utc>>,
    /// Error message if failed
    pub error: Option<String>,
}

impl Phase {
    /// Create a new pending phase
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            state: PhaseState::Pending,
            jobs: Vec::new(),
            started_at: None,
            completed_at: None,
            error: None,
        }
    }

    /// Mark phase as running
    pub fn start(&mut self) {
        self.state = PhaseState::Running;
        self.started_at = Some(Utc::now());
    }

    /// Mark phase as completed
    pub fn complete(&mut self) {
        self.state = PhaseState::Completed;
        self.completed_at = Some(Utc::now());
    }

    /// Mark phase as failed
    pub fn fail(&mut self, error: impl Into<String>) {
        self.state = PhaseState::Failed;
        self.error = Some(error.into());
        self.completed_at = Some(Utc::now());
    }

    /// Mark phase as skipped
    pub fn skip(&mut self) {
        self.state = PhaseState::Skipped;
        self.completed_at = Some(Utc::now());
    }
}

/// Ceremony execution options
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CeremonyOptions {
    /// Skip safety backups (user explicitly requested)
    pub recklessly: bool,
    /// Dry run - plan but don't execute
    pub dry_run: bool,
    /// Auto-rollback on failure (default: true)
    #[serde(default = "default_auto_rollback")]
    pub auto_rollback: bool,
}

fn default_auto_rollback() -> bool {
    true
}

/// Information about who initiated the ceremony
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CeremonyInitiator {
    /// Source of initiation (e.g., "cli", "api", "scheduled")
    pub source: String,
    /// Stone ID that initiated (if from CLI/API)
    pub stone_id: Option<String>,
    /// Original command (for audit)
    pub command: Option<String>,
}

/// A ceremony instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ceremony {
    /// Unique identifier
    pub id: CeremonyId,
    /// Type of ceremony
    pub ceremony_type: CeremonyType,
    /// Current state
    pub state: CeremonyState,
    /// Coordinator stone ID
    pub coordinator: String,
    /// Participating stone IDs
    pub participants: Vec<String>,

    /// Execution phases
    pub phases: Vec<Phase>,
    /// Current phase index
    pub current_phase: usize,

    /// When ceremony was initiated
    pub initiated_at: DateTime<Utc>,
    /// When execution started
    pub started_at: Option<DateTime<Utc>>,
    /// When ceremony completed (success or failure)
    pub completed_at: Option<DateTime<Utc>>,

    /// Who initiated
    pub initiator: CeremonyInitiator,
    /// Execution options
    pub options: CeremonyOptions,

    /// Artifacts created (harvest IDs, stored offering IDs)
    pub artifacts: HashMap<String, String>,

    /// Error details if failed
    pub error: Option<String>,
}

impl Ceremony {
    /// Create a new ceremony
    pub fn new(
        ceremony_type: CeremonyType,
        coordinator: String,
        initiator: CeremonyInitiator,
        options: CeremonyOptions,
    ) -> Self {
        // Use timestamp + random suffix to ensure unique IDs
        let now = Utc::now();
        let random_suffix: u16 = rand::thread_rng().gen();
        let target_part = ceremony_type
            .target()
            .map(|t| format!("-{}", &t[..t.len().min(8)]))
            .unwrap_or_default();
        let id = format!(
            "{}{}-{}-{}-{:04x}",
            ceremony_type.name(),
            target_part,
            &coordinator[..coordinator.len().min(8)],
            now.format("%Y%m%d%H%M%S"),
            random_suffix
        );

        Self {
            id,
            ceremony_type,
            state: CeremonyState::Initiated,
            coordinator,
            participants: Vec::new(),
            phases: Vec::new(),
            current_phase: 0,
            initiated_at: Utc::now(),
            started_at: None,
            completed_at: None,
            initiator,
            options,
            artifacts: HashMap::new(),
            error: None,
        }
    }

    /// Get the current phase (if any)
    pub fn current_phase(&self) -> Option<&Phase> {
        self.phases.get(self.current_phase)
    }

    /// Get mutable reference to current phase
    pub fn current_phase_mut(&mut self) -> Option<&mut Phase> {
        self.phases.get_mut(self.current_phase)
    }

    /// Calculate progress percentage
    pub fn progress_percent(&self) -> u8 {
        if self.phases.is_empty() {
            return 0;
        }
        let completed = self
            .phases
            .iter()
            .filter(|p| matches!(p.state, PhaseState::Completed | PhaseState::Skipped))
            .count();
        ((completed * 100) / self.phases.len()) as u8
    }

    /// Advance to next phase
    pub fn advance_phase(&mut self) -> bool {
        if self.current_phase + 1 < self.phases.len() {
            self.current_phase += 1;
            true
        } else {
            false
        }
    }

    /// Mark ceremony as started
    pub fn start(&mut self) {
        self.state = CeremonyState::Executing;
        self.started_at = Some(Utc::now());
    }

    /// Mark ceremony as completed
    pub fn complete(&mut self) {
        self.state = CeremonyState::Completed;
        self.completed_at = Some(Utc::now());
    }

    /// Mark ceremony as failed
    pub fn fail(&mut self, error: impl Into<String>) {
        self.state = CeremonyState::Failed;
        self.error = Some(error.into());
        self.completed_at = Some(Utc::now());
    }

    /// Mark ceremony as rolled back
    pub fn rollback(&mut self, error: impl Into<String>) {
        self.state = CeremonyState::RolledBack;
        self.error = Some(error.into());
        self.completed_at = Some(Utc::now());
    }

    /// Mark ceremony as cancelled
    pub fn cancel(&mut self) {
        self.state = CeremonyState::Cancelled;
        self.completed_at = Some(Utc::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ceremony_type_name() {
        let ct = CeremonyType::NourishOffering {
            offering: "mongodb".to_string(),
        };
        assert_eq!(ct.name(), "nourish-offering");
        assert_eq!(ct.target(), Some("mongodb"));
    }

    #[test]
    fn test_ceremony_state_terminal() {
        assert!(!CeremonyState::Initiated.is_terminal());
        assert!(!CeremonyState::Executing.is_terminal());
        assert!(CeremonyState::Completed.is_terminal());
        assert!(CeremonyState::Failed.is_terminal());
        assert!(CeremonyState::RolledBack.is_terminal());
    }

    #[test]
    fn test_phase_lifecycle() {
        let mut phase = Phase::new("collect");
        assert_eq!(phase.state, PhaseState::Pending);

        phase.start();
        assert_eq!(phase.state, PhaseState::Running);
        assert!(phase.started_at.is_some());

        phase.complete();
        assert_eq!(phase.state, PhaseState::Completed);
        assert!(phase.completed_at.is_some());
    }

    #[test]
    fn test_ceremony_progress() {
        let mut ceremony = Ceremony::new(
            CeremonyType::NourishOffering {
                offering: "test".to_string(),
            },
            "stone-01".to_string(),
            CeremonyInitiator {
                source: "test".to_string(),
                stone_id: None,
                command: None,
            },
            CeremonyOptions::default(),
        );

        ceremony.phases = vec![
            Phase::new("collect"),
            Phase::new("nourish"),
            Phase::new("water"),
        ];

        assert_eq!(ceremony.progress_percent(), 0);

        ceremony.phases[0].complete();
        assert_eq!(ceremony.progress_percent(), 33);

        ceremony.phases[1].complete();
        assert_eq!(ceremony.progress_percent(), 66);

        ceremony.phases[2].complete();
        assert_eq!(ceremony.progress_percent(), 100);
    }

    #[test]
    fn test_ceremony_id_format() {
        let ceremony = Ceremony::new(
            CeremonyType::Vacate {
                stone: "stone-01".to_string(),
            },
            "stone-coordinator".to_string(),
            CeremonyInitiator {
                source: "cli".to_string(),
                stone_id: Some("stone-01".to_string()),
                command: Some("garden-rake vacate stone-01".to_string()),
            },
            CeremonyOptions::default(),
        );

        // Format: {type}-{target}-{coordinator}-{timestamp}-{random}
        assert!(ceremony.id.starts_with("vacate-stone-01-stone-co"));
    }

    #[test]
    fn test_ceremony_serialization() {
        let ceremony = Ceremony::new(
            CeremonyType::NourishOffering {
                offering: "mongodb".to_string(),
            },
            "stone-01".to_string(),
            CeremonyInitiator {
                source: "api".to_string(),
                stone_id: None,
                command: None,
            },
            CeremonyOptions {
                recklessly: false,
                dry_run: false,
                auto_rollback: true,
            },
        );

        let json = serde_json::to_string(&ceremony).unwrap();
        assert!(json.contains("nourish_offering"));
        assert!(json.contains("mongodb"));

        let parsed: Ceremony = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, ceremony.id);
    }
}
