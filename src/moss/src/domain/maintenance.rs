//! Caretaking: automated maintenance sweep pipeline
//!
//! Domain-pluggable maintenance system where each domain contributes
//! a sweeper function. The orchestrator runs them sequentially, times
//! each one, aggregates results, and delegates persistence to infra.
//!
//! ## Contract
//! Each sweeper: `async fn sweep_X(ctx: &Sweep) -> SweepReport`
//! Reports: status (Healthy/Degraded/Unhealthy/Failed) + notes[]
//!
//! ## Sweepers
//! - staging: cleans stale .staged files (>24h)
//! - docker: prunes dangling images
//! - binaries: removes old .backup files (>7d)
//! - task_history: cleans orphaned task entries

use crate::AppState;


// ============================================================================
// Types
// ============================================================================

/// Domain's self-assessment after sweep
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SweepStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Failed,
}

impl SweepStatus {
    fn severity(&self) -> u8 {
        match self {
            SweepStatus::Healthy => 0,
            SweepStatus::Degraded => 1,
            SweepStatus::Unhealthy => 2,
            SweepStatus::Failed => 3,
        }
    }
}

/// Single domain's sweep report
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SweepReport {
    pub domain: String,
    pub status: SweepStatus,
    pub duration_ms: u64,
    pub notes: Vec<String>,
}

/// Complete sweep run (persisted to disk as one JSON file)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SweepRun {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub duration_ms: u64,
    pub overall_status: SweepStatus,
    pub reports: Vec<SweepReport>,
}

/// Everything a sweeper needs — thin wrapper around AppState
pub struct Sweep<'a> {
    pub state: &'a AppState,
    pub task_persistence: &'a crate::infra::TaskStore,
}

// ============================================================================
// Orchestrator
// ============================================================================

/// Run all sweepers sequentially, collect results
pub async fn run_sweep(state: &AppState, task_persistence: &crate::infra::TaskStore) -> SweepRun {
    let ctx = Sweep {
        state,
        task_persistence,
    };
    let start = std::time::Instant::now();

    let reports = vec![
        run_one_sweeper(sweep_staging, &ctx).await,
        run_one_sweeper(sweep_docker, &ctx).await,
        run_one_sweeper(sweep_binaries, &ctx).await,
        run_one_sweeper(sweep_task_history, &ctx).await,
        run_one_sweeper(sweep_logs, &ctx).await,
    ];

    let duration_ms = start.elapsed().as_millis() as u64;
    let overall_status = worst_status(&reports);

    SweepRun {
        timestamp: chrono::Utc::now(),
        duration_ms,
        overall_status,
        reports,
    }
}

/// Run a single sweeper with timing
async fn run_one_sweeper<'a, F, Fut>(sweeper: F, ctx: &'a Sweep<'a>) -> SweepReport
where
    F: FnOnce(&'a Sweep<'a>) -> Fut,
    Fut: std::future::Future<Output = SweepReport>,
{
    let start = std::time::Instant::now();
    let mut report = sweeper(ctx).await;
    report.duration_ms = start.elapsed().as_millis() as u64;
    report
}

/// Compute worst status across all reports
fn worst_status(reports: &[SweepReport]) -> SweepStatus {
    reports
        .iter()
        .map(|r| &r.status)
        .max_by_key(|s| s.severity())
        .cloned()
        .unwrap_or(SweepStatus::Healthy)
}

// ============================================================================
// Sweepers
// ============================================================================

/// Sweep staging directory: delete .staged files older than 24 hours
async fn sweep_staging(_ctx: &Sweep<'_>) -> SweepReport {
    let staging = garden_common::constants::paths::staging_dir();
    let staging_path = std::path::Path::new(&staging);

    if !staging_path.exists() {
        return SweepReport {
            domain: "staging".into(),
            status: SweepStatus::Healthy,
            duration_ms: 0,
            notes: vec!["Staging directory does not exist".into()],
        };
    }

    let cutoff = chrono::Utc::now() - chrono::Duration::hours(24);
    let mut cleaned = 0u32;
    let mut bytes_freed = 0u64;
    let mut errors = Vec::new();

    let mut dir = match tokio::fs::read_dir(staging_path).await {
        Ok(d) => d,
        Err(e) => {
            return SweepReport {
                domain: "staging".into(),
                status: SweepStatus::Failed,
                duration_ms: 0,
                notes: vec![format!("Failed to read staging directory: {}", e)],
            };
        }
    };

    while let Ok(Some(entry)) = dir.next_entry().await {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();

        if !name.ends_with(".staged") {
            continue;
        }

        let metadata = match tokio::fs::metadata(&path).await {
            Ok(m) => m,
            Err(_) => continue,
        };

        let modified: chrono::DateTime<chrono::Utc> = match metadata.modified() {
            Ok(t) => t.into(),
            Err(_) => continue,
        };

        if modified < cutoff {
            let size = metadata.len();
            match tokio::fs::remove_file(&path).await {
                Ok(()) => {
                    cleaned += 1;
                    bytes_freed += size;
                }
                Err(e) => {
                    errors.push(format!("Failed to remove {}: {}", name, e));
                }
            }
        }
    }

    let mut notes = Vec::new();
    if cleaned > 0 {
        notes.push(format!(
            "Cleaned {} stale staging file(s) ({})",
            cleaned,
            garden_common::format_bytes(bytes_freed)
        ));
    } else {
        notes.push("Staging directory clean".into());
    }
    notes.extend(errors.clone());

    SweepReport {
        domain: "staging".into(),
        status: if !errors.is_empty() {
            SweepStatus::Degraded
        } else {
            SweepStatus::Healthy
        },
        duration_ms: 0,
        notes,
    }
}

/// Sweep Docker: prune dangling images
async fn sweep_docker(ctx: &Sweep<'_>) -> SweepReport {
    if !ctx.state.subsystems.is_ready("docker") {
        return SweepReport {
            domain: "docker".into(),
            status: SweepStatus::Healthy,
            duration_ms: 0,
            notes: vec!["Docker unavailable, skipping".into()],
        };
    }

    match ctx.state.platform.docker.prune_dangling_images().await {
        Ok((count, bytes)) => {
            let mut notes = Vec::new();
            if count > 0 {
                notes.push(format!(
                    "Pruned {} dangling image(s) ({} reclaimed)",
                    count,
                    garden_common::format_bytes(bytes)
                ));
            } else {
                notes.push("No dangling images".into());
            }
            SweepReport {
                domain: "docker".into(),
                status: SweepStatus::Healthy,
                duration_ms: 0,
                notes,
            }
        }
        Err(e) => SweepReport {
            domain: "docker".into(),
            status: SweepStatus::Degraded,
            duration_ms: 0,
            notes: vec![format!("Docker prune failed: {}", e)],
        },
    }
}

/// Sweep binaries: delete .backup files older than 7 days
async fn sweep_binaries(_ctx: &Sweep<'_>) -> SweepReport {
    // Binaries live alongside the running binary
    let binary_dir = match std::env::current_exe() {
        Ok(exe) => exe
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::path::PathBuf::from(".")),
        Err(_) => {
            return SweepReport {
                domain: "binaries".into(),
                status: SweepStatus::Healthy,
                duration_ms: 0,
                notes: vec!["Could not determine binary directory".into()],
            };
        }
    };

    let cutoff = chrono::Utc::now() - chrono::Duration::days(7);
    let mut cleaned = 0u32;
    let mut bytes_freed = 0u64;

    let mut dir = match tokio::fs::read_dir(&binary_dir).await {
        Ok(d) => d,
        Err(_) => {
            return SweepReport {
                domain: "binaries".into(),
                status: SweepStatus::Healthy,
                duration_ms: 0,
                notes: vec!["Binary directory not readable".into()],
            };
        }
    };

    while let Ok(Some(entry)) = dir.next_entry().await {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();

        if !name.ends_with(".backup") {
            continue;
        }

        let metadata = match tokio::fs::metadata(&path).await {
            Ok(m) => m,
            Err(_) => continue,
        };

        let modified: chrono::DateTime<chrono::Utc> = match metadata.modified() {
            Ok(t) => t.into(),
            Err(_) => continue,
        };

        if modified < cutoff {
            let size = metadata.len();
            if tokio::fs::remove_file(&path).await.is_ok() {
                cleaned += 1;
                bytes_freed += size;
            }
        }
    }

    let notes = if cleaned > 0 {
        vec![format!(
            "Cleaned {} stale backup(s) ({})",
            cleaned,
            garden_common::format_bytes(bytes_freed)
        )]
    } else {
        vec!["No stale backups".into()]
    };

    SweepReport {
        domain: "binaries".into(),
        status: SweepStatus::Healthy,
        duration_ms: 0,
        notes,
    }
}

/// Sweep task history: clean orphaned task entries
///
/// A task is orphaned when:
/// - Its offering no longer exists in the registry, AND
/// - Its last_run is older than 30 days
async fn sweep_task_history(ctx: &Sweep<'_>) -> SweepReport {
    let mut registry = match ctx.task_persistence.load_registry().await {
        Ok(r) => r,
        Err(e) => {
            return SweepReport {
                domain: "task_history".into(),
                status: SweepStatus::Healthy,
                duration_ms: 0,
                notes: vec![format!("No task registry to sweep: {}", e)],
            };
        }
    };

    let cutoff = chrono::Utc::now() - chrono::Duration::days(30);

    // Get current offering IDs
    let offering_ids: std::collections::HashSet<String> = {
        let offerings = ctx.state.offerings.read().await;
        offerings.iter().map(|o| o.offering_id.clone()).collect()
    };

    // Find orphaned tasks
    let orphaned_ids: Vec<String> = registry
        .tasks
        .iter()
        .filter(|(_, task)| {
            // Offering no longer exists
            if offering_ids.contains(&task.offering_id) {
                return false;
            }
            // And last run is old enough (or never ran)
            task.last_run
                .as_ref()
                .and_then(|lr| chrono::DateTime::parse_from_rfc3339(lr).ok())
                .map(|dt| dt < cutoff)
                .unwrap_or(true) // never ran = orphaned
        })
        .map(|(id, _)| id.clone())
        .collect();

    if orphaned_ids.is_empty() {
        return SweepReport {
            domain: "task_history".into(),
            status: SweepStatus::Healthy,
            duration_ms: 0,
            notes: vec!["No orphaned tasks".into()],
        };
    }

    let count = orphaned_ids.len();
    for id in &orphaned_ids {
        registry.remove(id);
    }

    // Persist cleaned registry
    if let Err(e) = ctx.task_persistence.save_registry(&registry).await {
        return SweepReport {
            domain: "task_history".into(),
            status: SweepStatus::Degraded,
            duration_ms: 0,
            notes: vec![format!(
                "Found {} orphaned task(s) but failed to save: {}",
                count, e
            )],
        };
    }

    SweepReport {
        domain: "task_history".into(),
        status: SweepStatus::Healthy,
        duration_ms: 0,
        notes: vec![format!("Cleaned {} orphaned task(s)", count)],
    }
}

/// Sweep logs: delete rotated log files older than 7 days
async fn sweep_logs(_ctx: &Sweep<'_>) -> SweepReport {
    let logs_dir = garden_common::constants::paths::logs_dir();
    let logs_path = std::path::Path::new(&logs_dir);

    if !logs_path.exists() {
        return SweepReport {
            domain: "logs".into(),
            status: SweepStatus::Healthy,
            duration_ms: 0,
            notes: vec!["Logs directory does not exist".into()],
        };
    }

    let cutoff = chrono::Utc::now() - chrono::Duration::days(7);
    let mut cleaned = 0u32;
    let mut bytes_freed = 0u64;

    let mut dir = match tokio::fs::read_dir(logs_path).await {
        Ok(d) => d,
        Err(_) => {
            return SweepReport {
                domain: "logs".into(),
                status: SweepStatus::Healthy,
                duration_ms: 0,
                notes: vec!["Logs directory not readable".into()],
            };
        }
    };

    while let Ok(Some(entry)) = dir.next_entry().await {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();

        // Only delete rotated log files (garden-moss.log.YYYY-MM-DD)
        if !name.starts_with("garden-moss.log.") {
            continue;
        }

        let metadata = match tokio::fs::metadata(&path).await {
            Ok(m) => m,
            Err(_) => continue,
        };

        let modified: chrono::DateTime<chrono::Utc> = match metadata.modified() {
            Ok(t) => t.into(),
            Err(_) => continue,
        };

        if modified < cutoff {
            let size = metadata.len();
            if tokio::fs::remove_file(&path).await.is_ok() {
                cleaned += 1;
                bytes_freed += size;
            }
        }
    }

    let notes = if cleaned > 0 {
        vec![format!(
            "Cleaned {} stale log file(s) ({})",
            cleaned,
            garden_common::format_bytes(bytes_freed)
        )]
    } else {
        vec!["No stale log files".into()]
    };

    SweepReport {
        domain: "logs".into(),
        status: SweepStatus::Healthy,
        duration_ms: 0,
        notes,
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worst_status_empty() {
        assert_eq!(worst_status(&[]), SweepStatus::Healthy);
    }

    #[test]
    fn test_worst_status_all_healthy() {
        let reports = vec![
            SweepReport {
                domain: "a".into(),
                status: SweepStatus::Healthy,
                duration_ms: 0,
                notes: vec![],
            },
            SweepReport {
                domain: "b".into(),
                status: SweepStatus::Healthy,
                duration_ms: 0,
                notes: vec![],
            },
        ];
        assert_eq!(worst_status(&reports), SweepStatus::Healthy);
    }

    #[test]
    fn test_worst_status_mixed() {
        let reports = vec![
            SweepReport {
                domain: "a".into(),
                status: SweepStatus::Healthy,
                duration_ms: 0,
                notes: vec![],
            },
            SweepReport {
                domain: "b".into(),
                status: SweepStatus::Failed,
                duration_ms: 0,
                notes: vec![],
            },
            SweepReport {
                domain: "c".into(),
                status: SweepStatus::Degraded,
                duration_ms: 0,
                notes: vec![],
            },
        ];
        assert_eq!(worst_status(&reports), SweepStatus::Failed);
    }

    #[test]
    fn test_sweep_status_severity_ordering() {
        assert!(SweepStatus::Healthy.severity() < SweepStatus::Degraded.severity());
        assert!(SweepStatus::Degraded.severity() < SweepStatus::Unhealthy.severity());
        assert!(SweepStatus::Unhealthy.severity() < SweepStatus::Failed.severity());
    }

    #[test]
    fn test_sweep_run_serialization() {
        let run = SweepRun {
            timestamp: chrono::Utc::now(),
            duration_ms: 42,
            overall_status: SweepStatus::Healthy,
            reports: vec![SweepReport {
                domain: "test".into(),
                status: SweepStatus::Healthy,
                duration_ms: 10,
                notes: vec!["all good".into()],
            }],
        };
        let json = serde_json::to_string(&run).expect("serialize");
        let parsed: SweepRun = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.overall_status, SweepStatus::Healthy);
        assert_eq!(parsed.reports.len(), 1);
        assert_eq!(parsed.reports[0].domain, "test");
    }
}
