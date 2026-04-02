//! Provisioning domain types — job queue for skill dependency management (ORCH-0024).
//!
//! Pure domain types — no I/O, no async.
//!
//! A provisioning job represents the work of downloading required models
//! and pushing them to a ComfyUI instance for a specific skill.

use serde::Serialize;
use std::time::Duration;

// ── Target (dedup key) ───────────────────────────────────────

/// Unique key for deduplication — one job per (skill, endpoint) pair.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, serde::Deserialize)]
pub struct ProvisioningTarget {
    pub skill: String,
    pub endpoint: String,
}

impl std::fmt::Display for ProvisioningTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.skill, self.endpoint)
    }
}

// ── Priority ─────────────────────────────────────────────────

/// Job priority. Lower ordinal = higher priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    /// User clicked "provision" in the dashboard.
    User = 0,
    /// Auto-discovery detected a missing skill.
    Discovery = 1,
}

// ── Job Status (state machine) ───────────────────────────────

/// Job lifecycle — state machine as enum (code-standards §8).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Running {
        #[serde(skip_serializing_if = "Option::is_none")]
        progress: Option<DownloadProgress>,
    },
    Completed {
        #[serde(serialize_with = "serialize_duration_ms")]
        duration: Duration,
    },
    Failed {
        reason: String,
        attempts: u32,
        /// Seconds until eligible for retry.
        #[serde(skip_serializing_if = "Option::is_none")]
        retry_in_secs: Option<u64>,
    },
}

/// Download progress for a running job.
#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub model: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
}

// ── Job ──────────────────────────────────────────────────────

/// A provisioning job.
#[derive(Debug, Clone, Serialize)]
pub struct ProvisioningJob {
    /// GUIDv7 identifier.
    pub id: String,
    /// What to provision.
    pub target: ProvisioningTarget,
    /// Job priority.
    pub priority: Priority,
    /// Current status.
    pub status: JobStatus,
    /// Stone display name (for dashboard).
    pub stone_name: String,
    /// Provider kind (e.g., "comfyui").
    pub provider: String,
    /// When the job was submitted (epoch ms for serialization).
    pub submitted_ms: u64,
}

// ── Backoff ──────────────────────────────────────────────────

/// Exponential backoff schedule for failed provisioning jobs.
///
/// 1min → 5min → 30min → 1hr (capped).
pub struct Backoff;

impl Backoff {
    const SCHEDULE: &[Duration] = &[
        Duration::from_secs(60),
        Duration::from_secs(300),
        Duration::from_secs(1800),
        Duration::from_secs(3600),
    ];

    /// Compute the delay for a given attempt count (1-indexed).
    pub fn delay(attempts: u32) -> Duration {
        let idx = ((attempts.saturating_sub(1)) as usize).min(Self::SCHEDULE.len() - 1);
        Self::SCHEDULE[idx]
    }
}

// ── Snapshot (API response) ──────────────────────────────────

/// Immutable snapshot of the provisioning queue state.
#[derive(Debug, Clone, Serialize)]
pub struct ProvisioningSnapshot {
    pub jobs: Vec<ProvisioningJob>,
    pub active: usize,
    pub queued: usize,
    pub max_concurrency: usize,
}

impl ProvisioningSnapshot {
    pub fn empty() -> Self {
        Self {
            jobs: Vec::new(),
            active: 0,
            queued: 0,
            max_concurrency: 2,
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────

fn serialize_duration_ms<S: serde::Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_u64(d.as_millis() as u64)
}

fn now_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl ProvisioningJob {
    pub fn new(
        target: ProvisioningTarget,
        priority: Priority,
        stone_name: String,
        provider: String,
    ) -> Self {
        Self {
            id: garden_common::utils::ids::generate_guidv7(),
            target,
            priority,
            status: JobStatus::Queued,
            stone_name,
            provider,
            submitted_ms: now_epoch_ms(),
        }
    }
}

// ── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_schedule() {
        assert_eq!(Backoff::delay(1), Duration::from_secs(60));
        assert_eq!(Backoff::delay(2), Duration::from_secs(300));
        assert_eq!(Backoff::delay(3), Duration::from_secs(1800));
        assert_eq!(Backoff::delay(4), Duration::from_secs(3600));
        assert_eq!(Backoff::delay(100), Duration::from_secs(3600)); // capped
    }

    #[test]
    fn priority_ordering() {
        assert!(Priority::User < Priority::Discovery);
    }

    #[test]
    fn target_display() {
        let t = ProvisioningTarget {
            skill: "image.generate".into(),
            endpoint: "http://192.168.1.119:8188".into(),
        };
        assert_eq!(t.to_string(), "image.generate@http://192.168.1.119:8188");
    }

    #[test]
    fn job_serialization() {
        let job = ProvisioningJob::new(
            ProvisioningTarget { skill: "image.upscale".into(), endpoint: "http://localhost:8188".into() },
            Priority::Discovery,
            "stone-crystal".into(),
            "comfyui".into(),
        );
        let json = serde_json::to_value(&job).unwrap();
        assert_eq!(json["status"]["status"], "queued");
        assert_eq!(json["priority"], "discovery");
        assert!(json["id"].as_str().unwrap().len() > 10);
    }

    #[test]
    fn snapshot_serialization() {
        let snap = ProvisioningSnapshot::empty();
        let json = serde_json::to_value(&snap).unwrap();
        assert_eq!(json["active"], 0);
        assert_eq!(json["queued"], 0);
        assert_eq!(json["max_concurrency"], 2);
    }
}
