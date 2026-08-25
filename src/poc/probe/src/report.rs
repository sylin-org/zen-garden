//! Test reporting and output formatting

use crate::{Bag, StepResult};
use colored::Colorize;
use serde::{Deserialize, Serialize};

/// Report for a single test run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestReport {
    pub test_id: String,
    pub test_name: String,
    pub success: bool,
    pub duration_ms: u64,
    pub bag: Bag,
    pub error: Option<String>,
}

impl TestReport {
    /// Print a summary of the report
    pub fn print_summary(&self) {
        let status = if self.success {
            "PASS".green().bold()
        } else {
            "FAIL".red().bold()
        };

        println!(
            "{} {} ({}) - {}ms",
            status,
            self.test_name.bold(),
            self.test_id.dimmed(),
            self.duration_ms
        );

        // Print step history
        for step in self.bag.history() {
            let step_status = match &step.result {
                StepResult::Ok(_) => "✓".green(),
                StepResult::Failed(_) => "✗".red(),
                StepResult::Skipped(_) => "○".yellow(),
                StepResult::TimedOut { .. } => "⏱".red(),
            };

            println!(
                "    {} {} ({}ms)",
                step_status, step.description, step.duration_ms
            );

            // Print failure details
            if let StepResult::Failed(msg) = &step.result {
                println!("      {}: {}", "error".red(), msg);
            }
        }

        // Print error if test failed with exception
        if let Some(err) = &self.error {
            println!("    {}: {}", "error".red(), err);
        }

        println!();
    }

    /// Print detailed report including bag contents
    pub fn print_detailed(&self) {
        self.print_summary();

        println!("    {}", "Bag Contents:".dimmed());
        for key in self.bag.keys() {
            if let Some(value) = self.bag.get_raw(key) {
                let value_str = serde_json::to_string(value).unwrap_or_else(|_| "?".to_string());
                let truncated = if value_str.len() > 80 {
                    format!("{}...", &value_str[..77])
                } else {
                    value_str
                };
                println!("      {}: {}", key.cyan(), truncated);
            }
        }
        println!();
    }
}

/// Summary report for multiple tests
#[derive(Debug)]
pub struct SuiteReport {
    pub reports: Vec<TestReport>,
    pub total_duration_ms: u64,
}

impl SuiteReport {
    pub fn new(reports: Vec<TestReport>) -> Self {
        let total_duration_ms = reports.iter().map(|r| r.duration_ms).sum();
        Self {
            reports,
            total_duration_ms,
        }
    }

    pub fn passed(&self) -> usize {
        self.reports.iter().filter(|r| r.success).count()
    }

    pub fn failed(&self) -> usize {
        self.reports.iter().filter(|r| !r.success).count()
    }

    pub fn total(&self) -> usize {
        self.reports.len()
    }

    pub fn all_passed(&self) -> bool {
        self.reports.iter().all(|r| r.success)
    }

    pub fn print_summary(&self) {
        println!();
        println!("{}", "═".repeat(60));

        for report in &self.reports {
            report.print_summary();
        }

        println!("{}", "═".repeat(60));

        let passed = self.passed();
        let failed = self.failed();
        let total = self.total();

        let summary = format!("{} passed, {} failed, {} total", passed, failed, total);

        let summary_colored = if failed == 0 {
            summary.green().bold()
        } else {
            summary.red().bold()
        };

        println!(
            "Tests: {} | Duration: {}ms",
            summary_colored, self.total_duration_ms
        );
        println!();
    }
}
