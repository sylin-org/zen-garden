//! Garden Probe CLI - Integration test runner for Zen Garden

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use garden_probe::{Bag, LiveGarden, TestRegistry};
use garden_probe::report::SuiteReport;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "garden-probe")]
#[command(about = "Integration test harness for Zen Garden")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Stone endpoint to connect to (default: discovers via tended stone)
    #[arg(long, short = 'e', global = true)]
    endpoint: Option<String>,

    /// Verbose output
    #[arg(long, short = 'v', global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// List available tests
    List {
        /// Filter by category
        #[arg(long, short = 'c')]
        category: Option<String>,

        /// Filter by tag
        #[arg(long, short = 't')]
        tag: Option<String>,
    },

    /// Run tests
    Run {
        /// Test IDs or patterns (e.g., "smoke.*", "discovery.topology")
        tests: Vec<String>,

        /// Run all tests
        #[arg(long, short = 'a')]
        all: bool,

        /// Run only tests with this tag
        #[arg(long, short = 't')]
        tag: Option<String>,

        /// Run only tests in this category
        #[arg(long, short = 'c')]
        category: Option<String>,

        /// Show detailed output including bag contents
        #[arg(long, short = 'd')]
        detailed: bool,

        /// Set initial bag values (key=value)
        #[arg(long = "set", short = 's')]
        set_values: Vec<String>,
    },

    /// Discover garden and show available stones
    Discover,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Setup logging
    if cli.verbose {
        tracing_subscriber::fmt()
            .with_env_filter("garden_probe=debug,info")
            .init();
    }

    // Determine endpoint
    let endpoint = cli
        .endpoint
        .clone()
        .or_else(|| std::env::var("ZG_STONE").ok())
        .unwrap_or_else(|| "http://localhost:7185".to_string());

    match cli.command {
        Commands::List { category, tag } => {
            cmd_list(category, tag)?;
        }
        Commands::Run {
            tests,
            all,
            tag,
            category,
            detailed,
            set_values,
        } => {
            cmd_run(&endpoint, tests, all, tag, category, detailed, set_values).await?;
        }
        Commands::Discover => {
            cmd_discover(&endpoint).await?;
        }
    }

    Ok(())
}

fn cmd_list(category: Option<String>, tag: Option<String>) -> Result<()> {
    let registry = TestRegistry::new();

    println!();
    println!("{}", "Available Tests".bold());
    println!("{}", "═".repeat(60));

    let tests = if let Some(cat) = &category {
        registry.by_category(cat)
    } else if let Some(t) = &tag {
        registry.by_tag(t)
    } else {
        registry.all()
    };

    // Group by category
    let mut by_category: std::collections::HashMap<&str, Vec<_>> = std::collections::HashMap::new();
    for test in tests {
        by_category.entry(test.category).or_default().push(test);
    }

    let mut categories: Vec<_> = by_category.keys().copied().collect();
    categories.sort();

    for cat in categories {
        println!();
        println!("{}", cat.to_uppercase().cyan().bold());

        let mut tests = by_category.get(cat).unwrap().clone();
        tests.sort_by_key(|t| t.id);

        for test in tests {
            let tags = test.tags.join(", ");
            println!(
                "  {} - {}",
                test.id.green(),
                test.description.dimmed()
            );
            if !tags.is_empty() {
                println!("    tags: {}", tags.dimmed());
            }
        }
    }

    println!();
    println!(
        "Run tests with: {} run <test-id>",
        "garden-probe".bold()
    );
    println!();

    Ok(())
}

async fn cmd_run(
    endpoint: &str,
    tests: Vec<String>,
    all: bool,
    tag: Option<String>,
    category: Option<String>,
    detailed: bool,
    set_values: Vec<String>,
) -> Result<()> {
    // Connect to garden
    println!();
    println!("{} {}", "Connecting to:".dimmed(), endpoint);

    let garden = Arc::new(
        LiveGarden::discover(endpoint)
            .await
            .context("Failed to discover garden")?,
    );

    println!(
        "{} {} stones: {:?}",
        "Discovered:".dimmed(),
        garden.len(),
        garden.stone_names()
    );
    println!();

    // Build registry
    let registry = TestRegistry::new();

    // Determine which tests to run
    let test_ids: Vec<String> = if all {
        registry.all().iter().map(|t| t.id.to_string()).collect()
    } else if let Some(t) = &tag {
        registry.by_tag(t).iter().map(|t| t.id.to_string()).collect()
    } else if let Some(c) = &category {
        registry.by_category(c).iter().map(|t| t.id.to_string()).collect()
    } else if !tests.is_empty() {
        // Expand patterns
        let mut ids = Vec::new();
        for pattern in &tests {
            let found = registry.find(pattern);
            if found.is_empty() {
                eprintln!("{}: No tests match pattern '{}'", "warning".yellow(), pattern);
            }
            for test in found {
                ids.push(test.id.to_string());
            }
        }
        ids
    } else {
        // Default: run smoke tests
        registry.by_category("smoke").iter().map(|t| t.id.to_string()).collect()
    };

    if test_ids.is_empty() {
        println!("{}", "No tests to run.".yellow());
        println!("Use 'garden-probe list' to see available tests.");
        return Ok(());
    }

    println!(
        "{} {} test(s)",
        "Running:".bold(),
        test_ids.len()
    );
    println!("{}", "─".repeat(60));

    // Parse initial bag values
    let mut initial_bag = Bag::new();
    for kv in set_values {
        if let Some((key, value)) = kv.split_once('=') {
            // Try to parse as JSON, fall back to string
            let json_value: serde_json::Value = serde_json::from_str(value)
                .unwrap_or_else(|_| serde_json::Value::String(value.to_string()));
            initial_bag.put(key, json_value);
        }
    }

    // Run tests
    let mut reports = Vec::new();
    for test_id in &test_ids {
        let bag = initial_bag.clone();
        match registry.run_test(test_id, garden.clone(), bag).await {
            Ok(report) => {
                if detailed {
                    report.print_detailed();
                } else {
                    report.print_summary();
                }
                reports.push(report);
            }
            Err(e) => {
                eprintln!("{} {}: {}", "ERROR".red(), test_id, e);
            }
        }
    }

    // Summary
    let suite = SuiteReport::new(reports);
    suite.print_summary();

    // Exit code
    if !suite.all_passed() {
        std::process::exit(1);
    }

    Ok(())
}

async fn cmd_discover(endpoint: &str) -> Result<()> {
    println!();
    println!("{} {}", "Discovering garden from:".dimmed(), endpoint);
    println!();

    let garden = LiveGarden::discover(endpoint)
        .await
        .context("Failed to discover garden")?;

    println!("{}", "Garden Stones".bold());
    println!("{}", "═".repeat(60));

    for stone in &garden.stones {
        let is_tended = garden
            .tended
            .as_ref()
            .map(|t| t.name == stone.name)
            .unwrap_or(false);

        let tended_marker = if is_tended {
            " [tended]".green()
        } else {
            "".normal()
        };

        let healthy = if stone.is_healthy().await {
            "healthy".green()
        } else {
            "unreachable".red()
        };

        println!(
            "  {}{} - {} ({})",
            stone.name.cyan().bold(),
            tended_marker,
            stone.endpoint.dimmed(),
            healthy
        );
    }

    println!();
    println!("{}: {} stones", "Total".bold(), garden.len());
    println!();

    Ok(())
}
