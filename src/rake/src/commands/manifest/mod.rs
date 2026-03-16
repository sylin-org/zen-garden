//! Manifest authoring commands (OFFER-0006 Phase 2).
//!
//! Commands for creating, validating, and testing offering manifests:
//! - `manifest init` — scaffold manifest files from a Docker image
//! - `manifest validate` — validate manifest files for errors
//! - `manifest test` — test-deploy manifest on a stone
//! - `manifest export` — export a running offering's manifest
//! - `manifest enrich` — add compatibility/guidance templates

use crate::command_manifest::cmd;
use crate::context::Runtime;
use crate::commands::Command;
use anyhow::{Context, Result};
use async_trait::async_trait;
use crate::ui::rendering as ui;

// ============================================================================
// Types
// ============================================================================

/// Action variants for the manifest command.
#[derive(Debug, Clone)]
pub enum ManifestAction {
    /// Generate manifest files from a Docker image.
    Init {
        image_ref: String,
        output_dir: Option<String>,
        name: Option<String>,
        category: Option<String>,
    },
    /// Validate manifest files.
    Validate { path: String },
    /// Test-deploy manifest files on a stone.
    Test { path: String },
    /// Export a running offering's manifest files.
    Export {
        offering: String,
        output_dir: Option<String>,
    },
    /// Enrich existing manifest with templates.
    Enrich { path: String, auto: bool },
}

pub struct ManifestCommand {
    pub action: ManifestAction,
    pub quiet: bool,
}

// ============================================================================
// Constructors
// ============================================================================

impl ManifestCommand {
    pub fn init(
        image_ref: String,
        output_dir: Option<String>,
        name: Option<String>,
        category: Option<String>,
        quiet: bool,
    ) -> Self {
        Self {
            action: ManifestAction::Init {
                image_ref,
                output_dir,
                name,
                category,
            },
            quiet,
        }
    }

    pub fn validate(path: String, quiet: bool) -> Self {
        Self {
            action: ManifestAction::Validate { path },
            quiet,
        }
    }

    pub fn test(path: String, quiet: bool) -> Self {
        Self {
            action: ManifestAction::Test { path },
            quiet,
        }
    }

    pub fn export(offering: String, output_dir: Option<String>, quiet: bool) -> Self {
        Self {
            action: ManifestAction::Export {
                offering,
                output_dir,
            },
            quiet,
        }
    }

    pub fn enrich(path: String, auto: bool, quiet: bool) -> Self {
        Self {
            action: ManifestAction::Enrich { path, auto },
            quiet,
        }
    }
}

// ============================================================================
// Runtime trait
// ============================================================================

#[async_trait]
impl Command for ManifestCommand {
    fn name(&self) -> &'static str {
        cmd::MANIFEST_CMD
    }

    fn requires_endpoint(&self) -> bool {
        !matches!(
            &self.action,
            ManifestAction::Validate { .. } | ManifestAction::Enrich { .. }
        )
    }

    fn show_stone_header(&self) -> bool {
        false
    }

    async fn execute(&self, ctx: &Runtime) -> Result<()> {
        match &self.action {
            ManifestAction::Init {
                image_ref,
                output_dir,
                name,
                category,
            } => {
                execute_init(ctx, image_ref, output_dir.as_deref(), name.as_deref(), category.as_deref()).await
            }
            ManifestAction::Validate { path } => execute_validate(path).await,
            ManifestAction::Test { path } => execute_test(ctx, path).await,
            ManifestAction::Export {
                offering,
                output_dir,
            } => execute_export(ctx, offering, output_dir.as_deref()).await,
            ManifestAction::Enrich { path, auto } => execute_enrich(path, *auto).await,
        }
    }
}

// ============================================================================
// Init — scaffold manifest from Docker image
// ============================================================================

async fn execute_init(
    ctx: &Runtime,
    image_ref: &str,
    output_dir: Option<&str>,
    name: Option<&str>,
    category: Option<&str>,
) -> Result<()> {
    let endpoint = ctx.endpoint.as_ref().context("endpoint required for init")?;
    let indent = " ".repeat(ui::constants::DEFAULT_INDENT);

    if !ctx.quiet {
        println!();
        println!("{}Inspecting image '{}'...", indent, image_ref);
    }

    // Call inspect endpoint
    let url = format!(
        "{}/api/v1/stone/offerings/inspect?image={}",
        endpoint.trim_end_matches('/'),
        urlencoding::encode(image_ref)
    );
    let response = ctx.client.get(&url).send().await?;
    let status = response.status();
    let body: serde_json::Value = response.json().await?;

    if !status.is_success() {
        let msg = body
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Image inspection failed");
        anyhow::bail!("{}", msg);
    }

    // Generate manifest files
    let generated =
        garden_common::manifests::generate::generate_from_inspection(name, category, &body)
            .context("Failed to generate manifest from inspection")?;

    // Write files
    let dir = output_dir.unwrap_or(".");
    tokio::fs::create_dir_all(dir)
        .await
        .with_context(|| format!("Failed to create output directory '{dir}'"))?;

    let files = [
        (
            format!("{}.snippet.yaml", generated.name),
            &generated.snippet_yaml,
        ),
        (
            format!("{}.frontmatter.json", generated.name),
            &generated.frontmatter_json,
        ),
        (
            format!("{}.compatibility.yaml", generated.name),
            &generated.compatibility_yaml,
        ),
        (
            format!("{}.guidance.md", generated.name),
            &generated.guidance_md,
        ),
    ];

    let mut written = Vec::new();
    for (filename, content) in &files {
        let path = std::path::Path::new(dir).join(filename);
        tokio::fs::write(&path, content)
            .await
            .with_context(|| format!("Failed to write {}", path.display()))?;
        written.push(filename.clone());
    }

    if !ctx.quiet {
        println!();
        println!(
            "{}{}  Manifest scaffolded for '{}'",
            indent,
            ui::status_indicator("success", ui::TerminalInfo::detect().supports_color),
            generated.name
        );
        println!();
        for f in &written {
            println!("{}  {}/{}", indent, dir, f);
        }
        println!();
        println!("{}Next steps:", indent);
        println!("{}  1. Review and edit the generated files", indent);
        println!(
            "{}  2. Validate: garden-rake manifest validate {}",
            indent, dir
        );
        println!(
            "{}  3. Test:     garden-rake manifest test {} --at <stone>",
            indent, dir
        );
    }

    Ok(())
}

// ============================================================================
// Validate — check manifest files for errors
// ============================================================================

async fn execute_validate(path: &str) -> Result<()> {
    use garden_common::manifests::validation;

    let indent = " ".repeat(ui::constants::DEFAULT_INDENT);
    let p = std::path::Path::new(path);
    let term = ui::TerminalInfo::detect();

    let result = if p.is_dir() {
        validation::validate_manifest_dir(p)?
    } else if p.is_file() {
        // Single file validation
        let content = std::fs::read_to_string(p)
            .with_context(|| format!("Cannot read {}", p.display()))?;
        let filename = p
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let findings = if filename.ends_with(".snippet.yaml") {
            validation::validate_snippet(&content, &filename)
        } else if filename.ends_with(".frontmatter.json") {
            validation::validate_frontmatter(&content, &filename)
        } else if filename.ends_with(".compatibility.yaml") {
            validation::validate_compatibility(&content, &filename)
        } else {
            anyhow::bail!(
                "Unknown manifest file type: {}. Expected .snippet.yaml, .frontmatter.json, or .compatibility.yaml",
                filename
            );
        };

        validation::ValidationResult {
            findings,
            files_checked: 1,
        }
    } else {
        anyhow::bail!("Path '{}' does not exist", path);
    };

    println!();
    println!(
        "{}Validated {} file(s)",
        indent, result.files_checked
    );
    println!();

    if result.findings.is_empty() {
        println!(
            "{}{}  No issues found",
            indent,
            ui::status_indicator("success", term.supports_color)
        );
        println!();
        return Ok(());
    }

    for finding in &result.findings {
        let severity_str = match finding.severity {
            validation::Severity::Error => {
                if term.supports_color {
                    "\x1b[31mERROR\x1b[0m"
                } else {
                    "ERROR"
                }
            }
            validation::Severity::Warning => {
                if term.supports_color {
                    "\x1b[33mWARN\x1b[0m "
                } else {
                    "WARN "
                }
            }
            validation::Severity::Info => {
                if term.supports_color {
                    "\x1b[36mINFO\x1b[0m "
                } else {
                    "INFO "
                }
            }
        };

        println!(
            "{}  {} [{}] {}: {}",
            indent, severity_str, finding.code, finding.file, finding.message
        );
    }

    println!();
    let errors = result.error_count();
    let warnings = result.warning_count();
    if errors > 0 {
        println!(
            "{}{}  {} error(s), {} warning(s)",
            indent,
            ui::status_indicator("error", term.supports_color),
            errors,
            warnings
        );
        println!();
        std::process::exit(1);
    } else {
        println!(
            "{}{}  {} warning(s), no errors",
            indent,
            ui::status_indicator("success", term.supports_color),
            warnings
        );
    }
    println!();

    Ok(())
}

// ============================================================================
// Test — deploy manifest on a stone
// ============================================================================

async fn execute_test(ctx: &Runtime, path: &str) -> Result<()> {
    let endpoint = ctx.endpoint.as_ref().context("endpoint required for test")?;
    let indent = " ".repeat(ui::constants::DEFAULT_INDENT);
    let term = ui::TerminalInfo::detect();
    let p = std::path::Path::new(path);

    if !p.is_dir() {
        anyhow::bail!("Path '{}' must be a directory containing manifest files", path);
    }

    // Validate first
    let validation = garden_common::manifests::validation::validate_manifest_dir(p)?;
    if !validation.is_valid() {
        println!();
        println!(
            "{}{}  Manifest has {} error(s) — fix before testing",
            indent,
            ui::status_indicator("error", term.supports_color),
            validation.error_count()
        );
        for f in validation
            .findings
            .iter()
            .filter(|f| f.severity == garden_common::manifests::validation::Severity::Error)
        {
            println!("{}  [{}] {}: {}", indent, f.code, f.file, f.message);
        }
        println!();
        std::process::exit(1);
    }

    // Find and read snippet file
    let entries = std::fs::read_dir(p)?;
    let mut snippet_yaml = None;
    let mut frontmatter_json = None;
    let mut compatibility_yaml = None;
    let mut offering_name = None;

    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".snippet.yaml") {
            snippet_yaml = Some(std::fs::read_to_string(entry.path())?);
            offering_name = Some(name.trim_end_matches(".snippet.yaml").to_string());
        } else if name.ends_with(".frontmatter.json") {
            frontmatter_json = Some(std::fs::read_to_string(entry.path())?);
        } else if name.ends_with(".compatibility.yaml") {
            compatibility_yaml = Some(std::fs::read_to_string(entry.path())?);
        }
    }

    let snippet = snippet_yaml.context("No .snippet.yaml file found in directory")?;
    let name = offering_name.context("Could not determine offering name")?;

    println!();
    println!("{}Deploying test manifest '{}'...", indent, name);

    // POST to test endpoint
    let url = format!(
        "{}/api/v1/stone/manifests/test",
        endpoint.trim_end_matches('/')
    );

    let body = serde_json::json!({
        "name": name,
        "snippet_yaml": snippet,
        "frontmatter_json": frontmatter_json,
        "compatibility_yaml": compatibility_yaml,
    });

    let response = ctx.client.post(&url).json(&body).send().await?;
    let status = response.status();
    let resp_body: serde_json::Value = response.json().await?;

    if !status.is_success() {
        let msg = resp_body
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Test deployment failed");
        anyhow::bail!("{}", msg);
    }

    println!(
        "{}{}  Test deployment started",
        indent,
        ui::status_indicator("success", term.supports_color)
    );

    if let Some(job_id) = resp_body.get("job_id").and_then(|j| j.as_str()) {
        println!("{}  Job ID: {}", indent, job_id);
    }

    println!();
    println!(
        "{}To clean up: garden-rake remove {}",
        indent, name
    );
    println!();

    Ok(())
}

// ============================================================================
// Export — export running offering's manifest
// ============================================================================

async fn execute_export(
    ctx: &Runtime,
    offering: &str,
    output_dir: Option<&str>,
) -> Result<()> {
    let endpoint = ctx
        .endpoint
        .as_ref()
        .context("endpoint required for export")?;
    let indent = " ".repeat(ui::constants::DEFAULT_INDENT);
    let term = ui::TerminalInfo::detect();

    if !ctx.quiet {
        println!();
        println!("{}Exporting manifest for '{}'...", indent, offering);
    }

    let url = format!(
        "{}/api/v1/stone/offerings/{}/export",
        endpoint.trim_end_matches('/'),
        urlencoding::encode(offering)
    );
    let response = ctx.client.get(&url).send().await?;
    let status = response.status();
    let body: serde_json::Value = response.json().await?;

    if !status.is_success() {
        let msg = body
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Export failed");
        anyhow::bail!("{}", msg);
    }

    let name = body
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or(offering);

    let dir = output_dir.unwrap_or(".");
    tokio::fs::create_dir_all(dir)
        .await
        .with_context(|| format!("Failed to create output directory '{dir}'"))?;

    let files = [
        ("snippet_yaml", format!("{}.snippet.yaml", name)),
        ("frontmatter_json", format!("{}.frontmatter.json", name)),
        ("compatibility_yaml", format!("{}.compatibility.yaml", name)),
        ("guidance_md", format!("{}.guidance.md", name)),
    ];

    let mut written = Vec::new();
    for (key, filename) in &files {
        if let Some(content) = body.get(key).and_then(|v| v.as_str()) {
            if !content.is_empty() {
                let path = std::path::Path::new(dir).join(filename);
                tokio::fs::write(&path, content)
                    .await
                    .with_context(|| format!("Failed to write {}", path.display()))?;
                written.push(filename.clone());
            }
        }
    }

    if !ctx.quiet {
        println!();
        println!(
            "{}{}  Exported {} file(s) for '{}'",
            indent,
            ui::status_indicator("success", term.supports_color),
            written.len(),
            name
        );
        println!();
        for f in &written {
            println!("{}  {}/{}", indent, dir, f);
        }
        println!();
    }

    Ok(())
}

// ============================================================================
// Enrich — add compatibility/guidance templates
// ============================================================================

async fn execute_enrich(path: &str, auto: bool) -> Result<()> {
    let indent = " ".repeat(ui::constants::DEFAULT_INDENT);
    let term = ui::TerminalInfo::detect();
    let p = std::path::Path::new(path);

    if !p.is_dir() {
        anyhow::bail!("Path '{}' must be a directory containing manifest files", path);
    }

    // Find existing files
    let entries = std::fs::read_dir(p)?;
    let mut has_compatibility = false;
    let mut has_guidance = false;
    let mut offering_name = None;

    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".snippet.yaml") {
            offering_name = Some(name.trim_end_matches(".snippet.yaml").to_string());
        } else if name.ends_with(".compatibility.yaml") {
            // Check if it's non-trivial (not just comments/empty)
            let content = std::fs::read_to_string(entry.path()).unwrap_or_default();
            let trimmed = content
                .lines()
                .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
                .count();
            has_compatibility = trimmed > 2; // More than just "version" and "compatibility_rules: []"
        } else if name.ends_with(".guidance.md") {
            let content = std::fs::read_to_string(entry.path()).unwrap_or_default();
            has_guidance = content.len() > 50; // Non-trivial content
        }
    }

    let name = offering_name.context("No .snippet.yaml file found — cannot determine offering name")?;
    let mut enriched = Vec::new();

    // Add compatibility template if missing
    if !has_compatibility {
        let should_add = if auto {
            true
        } else {
            print!(
                "{}Add compatibility.yaml template for '{}'? [Y/n]: ",
                indent, name
            );
            std::io::Write::flush(&mut std::io::stdout())?;
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            input.trim().is_empty() || input.trim().eq_ignore_ascii_case("y")
        };

        if should_add {
            let content =
                garden_common::manifests::generate::generate_from_inspection(
                    Some(&name),
                    None,
                    &serde_json::json!({"image": format!("{}:latest", name), "exposed_ports": [], "volumes": [], "environment": [], "labels": {}, "healthcheck": null, "architecture": "unknown"}),
                )?;
            let filepath = p.join(format!("{}.compatibility.yaml", name));
            tokio::fs::write(&filepath, &content.compatibility_yaml).await?;
            enriched.push(format!("{}.compatibility.yaml", name));
        }
    }

    // Add guidance template if missing
    if !has_guidance {
        let should_add = if auto {
            true
        } else {
            print!(
                "{}Add guidance.md template for '{}'? [Y/n]: ",
                indent, name
            );
            std::io::Write::flush(&mut std::io::stdout())?;
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            input.trim().is_empty() || input.trim().eq_ignore_ascii_case("y")
        };

        if should_add {
            let content =
                garden_common::manifests::generate::generate_from_inspection(
                    Some(&name),
                    None,
                    &serde_json::json!({"image": format!("{}:latest", name), "exposed_ports": [], "volumes": [], "environment": [], "labels": {}, "healthcheck": null, "architecture": "unknown"}),
                )?;
            let filepath = p.join(format!("{}.guidance.md", name));
            tokio::fs::write(&filepath, &content.guidance_md).await?;
            enriched.push(format!("{}.guidance.md", name));
        }
    }

    println!();
    if enriched.is_empty() {
        println!(
            "{}{}  Manifest already has compatibility and guidance files",
            indent,
            ui::status_indicator("success", term.supports_color)
        );
    } else {
        println!(
            "{}{}  Enriched with {} file(s):",
            indent,
            ui::status_indicator("success", term.supports_color),
            enriched.len()
        );
        for f in &enriched {
            println!("{}  {}/{}", indent, path, f);
        }
    }
    println!();

    Ok(())
}
