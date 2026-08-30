//! The Run aggregate: ONE execution of a declared will. Identity is
//! (offering fqn, run id). A run moves FORWARD through its phases —
//! imprint → pack → ferry → done — or fails loudly; there is no
//! backward, and the terminal phases never run again (the scheduler
//! never doubles an in-flight run, and a finished run is history).
//!
//! `RunInfo` is the wire projection — the capture-last face's contract.
//! Its field names do not move.

use serde::Serialize;
use std::path::Path;

/// The legal phases of a run, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Imprint,
    Pack,
    Ferry,
    Done,
    Failed,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Imprint => "imprint",
            Self::Pack => "pack",
            Self::Ferry => "ferry",
            Self::Done => "done",
            Self::Failed => "failed",
        }
    }

    /// Parse the wire form (the projection stores phases as strings).
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "imprint" => Some(Self::Imprint),
            "pack" => Some(Self::Pack),
            "ferry" => Some(Self::Ferry),
            "done" => Some(Self::Done),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }

    /// Terminal: the run is history. Only history can be replaced.
    pub fn terminal(self) -> bool {
        matches!(self, Self::Done | Self::Failed)
    }

    /// Forward-only: a phase may never move backward.
    fn rank(self) -> u8 {
        match self {
            Self::Imprint => 0,
            Self::Pack => 1,
            Self::Ferry => 2,
            Self::Done => 3,
            Self::Failed => 3,
        }
    }
}

/// The wire projection of a run (capture-last's contract). Field names
/// do not move.
#[derive(Debug, Clone, Serialize)]
pub struct RunInfo {
    pub fqn: String,
    pub run_id: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub phase: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<String>,
    /// Where the checkpoint landed (local ledger always; sinks — local
    /// mounted banks, then the room's heard sinks through their holder).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ferried_to: Option<Vec<String>>,
}

impl RunInfo {
    /// True while the run is still someone's business.
    pub fn in_flight(&self) -> bool {
        Phase::parse(&self.phase).is_none_or(|p| !p.terminal())
    }
}

/// One execution of a will.
#[derive(Debug, Clone)]
pub struct Run {
    info: RunInfo,
}

impl Run {
    /// Wrap an existing projection (announced records, replays) without
    /// transition checks — the caller vouches for it.
    pub fn from_snapshot(info: RunInfo) -> Self {
        Self { info }
    }

    /// A run begins at imprint — the first phase there is.
    pub fn begin(fqn: &str, run_id: &str) -> Self {
        Self {
            info: RunInfo {
                fqn: fqn.to_string(),
                run_id: run_id.to_string(),
                started_at: chrono::Utc::now(),
                phase: Phase::Imprint.as_str().into(),
                error: None,
                checkpoint: None,
                ferried_to: None,
            },
        }
    }

    /// Move forward. Backward is refused — a run never re-enters a
    /// phase it has left.
    pub fn advance(&mut self, phase: Phase) {
        let current = Phase::parse(&self.info.phase).expect("a stored run carries a legal phase");
        assert!(
            phase.rank() >= current.rank(),
            "a run moves forward: {} → {} refused",
            current.as_str(),
            phase.as_str()
        );
        self.info.phase = phase.as_str().into();
    }

    /// The run finished: the checkpoint exists (its path rides).
    pub fn finish(&mut self, checkpoint: &Path) {
        self.advance(Phase::Done);
        self.info.checkpoint = Some(checkpoint.display().to_string());
    }

    /// The run failed loudly: the error rides, nothing is committed.
    pub fn fail(&mut self, error: &str) {
        self.advance(Phase::Failed);
        self.info.error = Some(error.to_string());
    }

    /// The sinks that (now) hold the checkpoint.
    pub fn delivered_to(&mut self, sinks: Vec<String>) {
        self.info.ferried_to = Some(sinks);
    }

    /// True while the run is still someone's business (not terminal).
    pub fn in_flight(&self) -> bool {
        Phase::parse(&self.info.phase).is_none_or(|p| !p.terminal())
    }

    /// The wire projection — clone freely, it is the face contract.
    pub fn snapshot(&self) -> RunInfo {
        self.info.clone()
    }

    /// Borrow the projection (for stats and renderers).
    pub fn info(&self) -> &RunInfo {
        &self.info
    }

    /// Mutate the projection — replay paths only; live runs use the
    /// transition methods.
    pub fn info_mut(&mut self) -> &mut RunInfo {
        &mut self.info
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[test]
    fn a_run_moves_forward_and_never_backward() {
        let mut run = Run::begin("ntfy::default", "r1");
        assert_eq!(run.info().phase, "imprint");
        run.advance(Phase::Pack);
        run.advance(Phase::Ferry);
        run.finish(Path::new("/cp/r1"));
        let snap = run.snapshot();
        assert_eq!(snap.phase, "done");
        assert_eq!(snap.checkpoint.as_deref(), Some("/cp/r1"));
        assert!(!run.in_flight(), "done is history");
    }

    #[test]
    fn terminal_runs_are_never_in_flight() {
        let mut run = Run::begin("ntfy::default", "r2");
        assert!(run.in_flight());
        run.fail("imprint failed");
        assert_eq!(run.info().phase, "failed");
        assert!(run.info().error.as_deref() == Some("imprint failed"));
        assert!(!run.in_flight());
    }

    #[test]
    #[should_panic(expected = "moves forward")]
    fn backward_is_refused() {
        let mut run = Run::begin("ntfy::default", "r3");
        run.advance(Phase::Ferry);
        run.advance(Phase::Pack); // refused: the run never rewinds
    }
}
