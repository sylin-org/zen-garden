//! Greenhouse API — offering upkeep module.
//!
//! Standalone module (like Pulse) providing an omni-catalog of all offerings
//! (installed, available, image-direct) and file-level CRUD for manifest
//! authoring and customization.
//!
//! Endpoints:
//! - `GET  /greenhouse`                          — standalone SPA page
//! - `GET  /api/v1/stone/greenhouse/catalog`     — unified offering inventory
//! - `GET  /api/v1/stone/greenhouse/file`        — read a manifest file
//! - `PUT  /api/v1/stone/greenhouse/file`        — write/create a manifest file
//! - `DELETE /api/v1/stone/greenhouse/file`       — delete custom file (reset to built-in)
//! - `GET  /api/v1/stone/greenhouse/containers`  — running offerings for picker
//! - `POST /api/v1/stone/greenhouse/validate`    — real-time manifest validation
//! - `POST /api/v1/stone/greenhouse/generate`    — generate manifest from inspection

use axum::{
    Json,
    extract::{Query, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse},
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::infra::embedded::EmbeddedManifests;
use crate::{AppState, bad_request, internal, not_found};
use garden_common::api_utils::ApiErrorResponse;
use garden_common::manifests::{generate, runtime_manifests_dir, validation};

const GREENHOUSE_HTML: &str = include_str!("../../../assets/greenhouse.html");

// ============================================================================
// DTOs
// ============================================================================

/// A running offering entry for the container picker UI.
#[derive(Debug, Serialize)]
pub struct ContainerEntry {
    pub name: String,
    pub image: String,
    pub status: String,
    pub ports: Vec<String>,
}

/// Request body for `POST /greenhouse/validate`.
#[derive(Debug, Deserialize)]
pub struct ValidateRequest {
    pub snippet_yaml: String,
    #[serde(default)]
    pub frontmatter_json: Option<String>,
}

/// Response body for `POST /greenhouse/validate`.
#[derive(Debug, Serialize)]
pub struct ValidateResponse {
    pub valid: bool,
    pub findings: Vec<validation::ValidationFinding>,
}

/// Request body for `POST /greenhouse/generate`.
#[derive(Debug, Deserialize)]
pub struct GenerateRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    pub inspection: serde_json::Value,
}

/// Response body for `POST /greenhouse/generate`.
#[derive(Debug, Serialize)]
pub struct GenerateResponse {
    pub name: String,
    pub snippet_yaml: String,
    pub frontmatter_json: String,
    pub compatibility_yaml: String,
    pub guidance_md: String,
}

/// A single offering in the greenhouse catalog.
#[derive(Debug, Serialize)]
pub struct CatalogEntry {
    /// Offering name (e.g. "ollama", "pihole", "my-nginx").
    pub name: String,
    /// Category (e.g. "ai", "networking").
    pub category: String,
    /// Description from frontmatter or compiled index.
    pub description: String,
    /// Docker image reference (e.g. "pihole/pihole:2024.07.0").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// Tags from frontmatter (e.g. ["dns", "ad-blocking"]).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Port mappings: name → (host, container).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<(String, u16, u16)>,
    /// Volume count.
    pub volume_count: usize,
    /// Source: "curated" (embedded), "custom" (user-authored).
    pub source: String,
    /// State: "installed" or "available".
    pub state: String,
    /// Whether the container is currently running (only meaningful when installed).
    pub running: bool,
    /// Compatibility check result.
    pub compatibility: CompatResult,
    /// Whether a runtime overlay exists for a built-in offering.
    pub customized: bool,
    /// Files that exist for this offering.
    pub files: Vec<FileEntry>,
}

/// Compatibility evaluation result for an offering on this stone.
#[derive(Debug, Serialize)]
pub struct CompatResult {
    /// "compatible", "warning", "incompatible", or "unknown".
    pub status: String,
    /// Human-readable reason (for warnings and failures).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// A single file in an offering's manifest bundle.
#[derive(Debug, Serialize)]
pub struct FileEntry {
    /// File type: "snippet", "frontmatter", "compatibility", "guidance",
    /// "research", "adopted", "adopted-guidance", "capabilities".
    pub file_type: String,
    /// File extension: "yaml", "json", "md".
    pub extension: String,
    /// Origin: "built-in", "custom", or "customized" (overlay of built-in).
    pub origin: String,
}

/// Query parameters for file CRUD endpoints.
#[derive(Debug, Deserialize)]
pub struct FileQuery {
    /// Offering name (e.g. "pihole").
    pub offering: String,
    /// File type (e.g. "guidance", "research", "snippet").
    #[serde(rename = "type")]
    pub file_type: String,
}

// ============================================================================
// Known file types and their extensions
// ============================================================================

/// Returns (suffix, extension) for a given file type name.
fn file_type_to_suffix(file_type: &str) -> Option<(&'static str, &'static str)> {
    match file_type {
        "snippet" => Some((".snippet.yaml", "yaml")),
        "frontmatter" => Some((".frontmatter.json", "json")),
        "compatibility" => Some((".compatibility.yaml", "yaml")),
        "guidance" => Some((".guidance.md", "md")),
        "research" => Some((".research.md", "md")),
        "adopted" => Some((".adopted.yaml", "yaml")),
        "adopted-guidance" => Some((".adopted.guidance.md", "md")),
        "capabilities" => Some((".capabilities.yaml", "yaml")),
        _ => None,
    }
}

/// All known file type suffixes for scanning.
const FILE_SUFFIXES: &[(&str, &str, &str)] = &[
    (".snippet.yaml", "snippet", "yaml"),
    (".frontmatter.json", "frontmatter", "json"),
    (".compatibility.yaml", "compatibility", "yaml"),
    (".guidance.md", "guidance", "md"),
    (".research.md", "research", "md"),
    (".adopted.yaml", "adopted", "yaml"),
    (".adopted.guidance.md", "adopted-guidance", "md"),
    (".capabilities.yaml", "capabilities", "yaml"),
];

// ============================================================================
// Page Handler
// ============================================================================

/// `GET /greenhouse` — Greenhouse standalone SPA.
pub async fn get_greenhouse_page() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        Html(GREENHOUSE_HTML),
    )
}

// ============================================================================
// Catalog
// ============================================================================

/// `GET /api/v1/stone/greenhouse/catalog`
///
/// Returns a unified inventory of all offerings: installed services, available
/// manifests (embedded + runtime), with compatibility checks and file inventory.
pub async fn get_catalog(
    State(state): State<AppState>,
) -> Result<Json<Vec<CatalogEntry>>, (StatusCode, Json<ApiErrorResponse>)> {
    // 1. Scan embedded manifest files → group by offering name
    let mut embedded_files: HashMap<String, Vec<(String, String)>> = HashMap::new();
    let mut embedded_categories: HashMap<String, String> = HashMap::new();

    for path in EmbeddedManifests::iter() {
        let path_str = path.as_ref();
        if !path_str.starts_with("sw/") {
            continue;
        }

        // Extract offering name and file type from path like "sw/ai/ollama.snippet.yaml"
        let filename = match path_str.rsplit('/').next() {
            Some(f) => f,
            None => continue,
        };

        // Extract category from path (sw/{category}/{file})
        let parts: Vec<&str> = path_str.split('/').collect();
        let category = if parts.len() >= 3 {
            parts[1].to_string()
        } else {
            "custom".to_string()
        };

        // Skip category.json files
        if filename == "category.json" {
            continue;
        }

        // Match against known suffixes to find offering name + file type
        for &(suffix, file_type, ext) in FILE_SUFFIXES {
            if let Some(name) = filename.strip_suffix(suffix) {
                if name.is_empty() {
                    continue;
                }
                embedded_files
                    .entry(name.to_string())
                    .or_default()
                    .push((file_type.to_string(), ext.to_string()));
                embedded_categories
                    .entry(name.to_string())
                    .or_insert(category.clone());
                break;
            }
        }
    }

    // 2. Scan runtime manifests dir for custom/overlay files
    let rt_dir = runtime_manifests_dir();
    let sw_dir = PathBuf::from(&rt_dir).join("sw");
    let mut runtime_files: HashMap<String, Vec<(String, String)>> = HashMap::new();
    let mut runtime_categories: HashMap<String, String> = HashMap::new();

    if sw_dir.exists()
        && let Ok(categories) = tokio::fs::read_dir(&sw_dir).await
    {
        let mut cats = categories;
        while let Ok(Some(cat_entry)) = cats.next_entry().await {
            let cat_path = cat_entry.path();
            if !cat_path.is_dir() {
                continue;
            }
            let cat_name = cat_entry.file_name().to_string_lossy().to_string();

            if let Ok(files) = tokio::fs::read_dir(&cat_path).await {
                let mut fs = files;
                while let Ok(Some(file_entry)) = fs.next_entry().await {
                    let fname = file_entry.file_name().to_string_lossy().to_string();
                    if fname == "category.json" {
                        continue;
                    }
                    for &(suffix, file_type, ext) in FILE_SUFFIXES {
                        if let Some(name) = fname.strip_suffix(suffix) {
                            if name.is_empty() {
                                continue;
                            }
                            runtime_files
                                .entry(name.to_string())
                                .or_default()
                                .push((file_type.to_string(), ext.to_string()));
                            runtime_categories
                                .entry(name.to_string())
                                .or_insert(cat_name.clone());
                            break;
                        }
                    }
                }
            }
        }
    }

    // 3. Get installed offerings from AppState
    let (installed_names, installed_running) = {
        let installed = state.offerings.read().await;
        let names: HashSet<String> = installed.iter().map(|o| o.offering.clone()).collect();
        let running: HashSet<String> = installed
            .iter()
            .filter(|o| o.status.to_string() == "running")
            .map(|o| o.offering.clone())
            .collect();
        (names, running)
    };

    // 4. Get compiled offerings index for compatibility info
    // Extract rich metadata from compiled offerings
    struct OfferingMeta {
        description: String,
        image: String,
        tags: Vec<String>,
        ports: Vec<(String, u16, u16)>,
        volume_count: usize,
    }
    let (compat_map, meta_map) = {
        let compiled = state.catalog.compiled_snapshot().await;
        let compat: HashMap<String, CompatResult> = match compiled.as_ref() {
            Some(offerings) => offerings
                .iter()
                .map(|o| {
                    let status = match o.compatibility.decision.as_str() {
                        garden_common::constants::COMPAT_PASS
                        | garden_common::constants::COMPAT_FALLBACK => "compatible",
                        garden_common::constants::COMPAT_WARNING => "warning",
                        garden_common::constants::COMPAT_FAIL => "incompatible",
                        other => {
                            tracing::warn!(decision = other, offering = %o.name, "Unknown compatibility decision");
                            "unknown"
                        }
                    };
                    (
                        o.name.clone(),
                        CompatResult {
                            status: status.to_string(),
                            reason: o.compatibility.reason.clone(),
                        },
                    )
                })
                .collect(),
            None => HashMap::new(),
        };
        let meta: HashMap<String, OfferingMeta> = match compiled.as_ref() {
            Some(offerings) => offerings
                .iter()
                .map(|o| {
                    (
                        o.name.clone(),
                        OfferingMeta {
                            description: o.description.clone(),
                            image: o.image.clone(),
                            tags: o.tags.clone(),
                            ports: o.ports_vec_named(),
                            volume_count: o.volumes.len(),
                        },
                    )
                })
                .collect(),
            None => HashMap::new(),
        };
        (compat, meta)
    };

    // 5. Merge into unified catalog
    let all_names: HashSet<String> = embedded_files
        .keys()
        .chain(runtime_files.keys())
        .cloned()
        .collect();

    let mut catalog: Vec<CatalogEntry> = Vec::new();

    for name in &all_names {
        let has_embedded = embedded_files.contains_key(name);
        let has_runtime = runtime_files.contains_key(name);

        let source = if has_embedded { "curated" } else { "custom" };
        let customized = has_embedded && has_runtime;

        let category = embedded_categories
            .get(name)
            .or_else(|| runtime_categories.get(name))
            .cloned()
            .unwrap_or_else(|| "custom".to_string());

        let is_installed = installed_names.contains(name);
        let is_running = installed_running.contains(name);

        let compat = compat_map.get(name).map_or_else(
            || CompatResult {
                status: "unknown".to_string(),
                reason: None,
            },
            |c| CompatResult {
                status: c.status.clone(),
                reason: c.reason.clone(),
            },
        );

        let meta = meta_map.get(name);

        // Build file inventory with origin info
        let mut files = Vec::new();
        let mut seen_types: HashSet<String> = HashSet::new();

        // Runtime files (custom or overlay)
        if let Some(rt_files) = runtime_files.get(name) {
            for (ft, ext) in rt_files {
                let origin = if has_embedded
                    && embedded_files
                        .get(name)
                        .is_some_and(|ef| ef.iter().any(|(t, _)| t == ft))
                {
                    "customized"
                } else {
                    "custom"
                };
                files.push(FileEntry {
                    file_type: ft.clone(),
                    extension: ext.clone(),
                    origin: origin.to_string(),
                });
                seen_types.insert(ft.clone());
            }
        }

        // Embedded files (only add if not already covered by runtime overlay)
        if let Some(em_files) = embedded_files.get(name) {
            for (ft, ext) in em_files {
                if !seen_types.contains(ft) {
                    files.push(FileEntry {
                        file_type: ft.clone(),
                        extension: ext.clone(),
                        origin: "built-in".to_string(),
                    });
                }
            }
        }

        catalog.push(CatalogEntry {
            name: name.clone(),
            category,
            description: meta.map(|m| m.description.clone()).unwrap_or_default(),
            image: meta.map(|m| m.image.clone()),
            tags: meta.map(|m| m.tags.clone()).unwrap_or_default(),
            ports: meta.map(|m| m.ports.clone()).unwrap_or_default(),
            volume_count: meta.map(|m| m.volume_count).unwrap_or(0),
            source: source.to_string(),
            state: if is_installed {
                "installed".to_string()
            } else {
                "available".to_string()
            },
            running: is_running,
            compatibility: compat,
            customized,
            files,
        });
    }

    // Sort: installed first, then by name
    catalog.sort_by(|a, b| {
        let state_ord = |s: &str| -> u8 {
            match s {
                "installed" => 0,
                _ => 1,
            }
        };
        state_ord(&a.state)
            .cmp(&state_ord(&b.state))
            .then_with(|| a.name.cmp(&b.name))
    });

    Ok(Json(catalog))
}

// ============================================================================
// File CRUD
// ============================================================================

/// Resolve the embedded path for an offering + file type.
/// Searches `sw/{category}/{offering}{suffix}` across embedded files.
fn find_embedded_path(offering: &str, suffix: &str) -> Option<String> {
    let target = format!("{offering}{suffix}");
    EmbeddedManifests::iter()
        .find(|p| {
            let path_str = p.as_ref();
            path_str.starts_with("sw/") && path_str.ends_with(&target)
        })
        .map(|p| p.to_string())
}

/// Resolve the runtime path for an offering + file type.
/// Searches `{runtime_dir}/sw/{category}/{offering}{suffix}`.
fn find_runtime_path(offering: &str, suffix: &str) -> Option<PathBuf> {
    let rt_dir = PathBuf::from(runtime_manifests_dir()).join("sw");
    if !rt_dir.exists() {
        return None;
    }
    // Walk category dirs
    if let Ok(entries) = std::fs::read_dir(&rt_dir) {
        for entry in entries.flatten() {
            let cat_path = entry.path();
            if !cat_path.is_dir() {
                continue;
            }
            let file_path = cat_path.join(format!("{offering}{suffix}"));
            if file_path.exists() {
                return Some(file_path);
            }
        }
    }
    None
}

/// Determine the category directory for an offering (embedded path, then runtime).
fn offering_category_dir(offering: &str) -> String {
    // Check embedded first
    for path in EmbeddedManifests::iter() {
        let path_str = path.as_ref();
        if !path_str.starts_with("sw/") {
            continue;
        }
        let parts: Vec<&str> = path_str.split('/').collect();
        if parts.len() >= 3
            && let Some(filename) = parts.last()
            && filename.starts_with(offering)
        {
            return parts[1].to_string();
        }
    }
    // Fallback: check runtime dir
    let rt_dir = PathBuf::from(runtime_manifests_dir()).join("sw");
    if rt_dir.exists()
        && let Ok(entries) = std::fs::read_dir(&rt_dir)
    {
        for entry in entries.flatten() {
            let cat_path = entry.path();
            if !cat_path.is_dir() {
                continue;
            }
            if let Ok(files) = std::fs::read_dir(&cat_path) {
                for file in files.flatten() {
                    let fname = file.file_name().to_string_lossy().to_string();
                    if fname.starts_with(offering) {
                        return cat_path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                    }
                }
            }
        }
    }
    "custom".to_string()
}

/// `GET /api/v1/stone/greenhouse/file?offering=pihole&type=guidance`
///
/// Reads a manifest file. Checks runtime directory first (custom/overlay),
/// falls back to embedded (built-in).
pub async fn get_file(
    Query(params): Query<FileQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiErrorResponse>)> {
    let (suffix, _ext) = file_type_to_suffix(&params.file_type).ok_or_else(|| {
        bad_request(
            "INVALID_FILE_TYPE",
            format!("Unknown file type: {}", params.file_type),
        )
    })?;

    // Try runtime dir first (overlay)
    if let Some(rt_path) = find_runtime_path(&params.offering, suffix) {
        let content = tokio::fs::read_to_string(&rt_path)
            .await
            .map_err(|e| internal("READ_FAILED", format!("Failed to read file: {e}")))?;
        return Ok((
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            content,
        ));
    }

    // Fall back to embedded
    if let Some(embedded_path) = find_embedded_path(&params.offering, suffix)
        && let Some(content) = EmbeddedManifests::get_string(&embedded_path)
    {
        return Ok((
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            content,
        ));
    }

    Err(not_found(
        "FILE_NOT_FOUND",
        format!(
            "No {} file found for offering '{}'",
            params.file_type, params.offering
        ),
    ))
}

/// `PUT /api/v1/stone/greenhouse/file?offering=pihole&type=guidance`
///
/// Writes a manifest file to the runtime directory. Creates directories
/// as needed. If the offering is built-in (embedded), this creates a
/// custom overlay.
pub async fn put_file(
    Query(params): Query<FileQuery>,
    body: String,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiErrorResponse>)> {
    let (suffix, _ext) = file_type_to_suffix(&params.file_type).ok_or_else(|| {
        bad_request(
            "INVALID_FILE_TYPE",
            format!("Unknown file type: {}", params.file_type),
        )
    })?;

    let category = offering_category_dir(&params.offering);
    let dir = PathBuf::from(runtime_manifests_dir())
        .join("sw")
        .join(&category);

    tokio::fs::create_dir_all(&dir).await.map_err(|e| {
        internal(
            "DIR_CREATE_FAILED",
            format!("Failed to create directory: {e}"),
        )
    })?;

    let file_path = dir.join(format!("{}{}", params.offering, suffix));
    tokio::fs::write(&file_path, &body)
        .await
        .map_err(|e| internal("WRITE_FAILED", format!("Failed to write file: {e}")))?;

    tracing::info!(
        offering = %params.offering,
        file_type = %params.file_type,
        path = %file_path.display(),
        "greenhouse file saved"
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "path": file_path.display().to_string(),
    })))
}

/// `DELETE /api/v1/stone/greenhouse/file?offering=pihole&type=guidance`
///
/// Deletes a custom/overlay file from the runtime directory. If a built-in
/// (embedded) version exists, it will show through again ("reset to default").
/// Returns 404 if no custom file exists.
pub async fn delete_file(
    Query(params): Query<FileQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiErrorResponse>)> {
    let (suffix, _ext) = file_type_to_suffix(&params.file_type).ok_or_else(|| {
        bad_request(
            "INVALID_FILE_TYPE",
            format!("Unknown file type: {}", params.file_type),
        )
    })?;

    let rt_path = find_runtime_path(&params.offering, suffix).ok_or_else(|| {
        not_found(
            "NO_CUSTOM_FILE",
            format!(
                "No custom {} file for '{}' to delete",
                params.file_type, params.offering
            ),
        )
    })?;

    tokio::fs::remove_file(&rt_path)
        .await
        .map_err(|e| internal("DELETE_FAILED", format!("Failed to delete file: {e}")))?;

    let has_builtin = find_embedded_path(&params.offering, suffix).is_some();

    tracing::info!(
        offering = %params.offering,
        file_type = %params.file_type,
        reset_to_builtin = has_builtin,
        "greenhouse file deleted"
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "reset_to_builtin": has_builtin,
    })))
}

// ============================================================================
// Export
// ============================================================================

/// Query parameter for export endpoint.
#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    pub offering: String,
}

/// `GET /api/v1/stone/greenhouse/export?offering=pihole`
///
/// Returns all manifest files for an offering as a single JSON bundle.
/// Each file is included as a key-value pair (filename → content).
pub async fn export_offering(
    Query(params): Query<ExportQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiErrorResponse>)> {
    let mut bundle: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();

    for &(suffix, _file_type, _ext) in FILE_SUFFIXES {
        // Try runtime first, then embedded
        let content = if let Some(rt_path) = find_runtime_path(&params.offering, suffix) {
            tokio::fs::read_to_string(&rt_path).await.ok()
        } else if let Some(em_path) = find_embedded_path(&params.offering, suffix) {
            EmbeddedManifests::get_string(&em_path)
        } else {
            None
        };

        if let Some(text) = content {
            let filename = format!("{}{}", params.offering, suffix);
            bundle.insert(filename, serde_json::Value::String(text));
        }
    }

    if bundle.is_empty() {
        return Err(not_found(
            "OFFERING_NOT_FOUND",
            format!("No manifest files found for '{}'", params.offering),
        ));
    }

    Ok(Json(serde_json::Value::Object(bundle)))
}

// ============================================================================
// Existing Handlers (unchanged)
// ============================================================================

/// `GET /api/v1/stone/greenhouse/containers`
///
/// Lists managed offerings that are currently running, for the "pick a
/// container" source selector in the Greenhouse UI.
pub async fn list_containers_v1(
    State(state): State<AppState>,
) -> Result<Json<Vec<ContainerEntry>>, (StatusCode, Json<ApiErrorResponse>)> {
    let offerings = state.offerings.read().await;

    let entries: Vec<ContainerEntry> = offerings
        .iter()
        .filter(|o| o.is_managed())
        .map(|o| {
            let ports: Vec<String> = {
                let mut ps = Vec::new();
                if o.location.port > 0 {
                    ps.push(format!("{}:{}", o.location.port, o.location.port));
                }
                for (name, &host_port) in &o.location.port_map {
                    ps.push(format!("{host_port} ({name})"));
                }
                ps
            };

            ContainerEntry {
                name: o.name.to_string(),
                image: o.offering.clone(),
                status: o.status.to_string(),
                ports,
            }
        })
        .collect();

    Ok(Json(entries))
}

/// `POST /api/v1/stone/greenhouse/validate`
///
/// Validates a manifest snippet (and optional frontmatter) and returns
/// findings with severity levels. Used for real-time validation in the
/// Greenhouse form.
pub async fn validate_manifest_v1(
    Json(payload): Json<ValidateRequest>,
) -> Result<Json<ValidateResponse>, (StatusCode, Json<ApiErrorResponse>)> {
    let mut findings = validation::validate_snippet(&payload.snippet_yaml, "snippet.yaml");

    if let Some(ref fm) = payload.frontmatter_json {
        findings.extend(validation::validate_frontmatter(fm, "frontmatter.json"));
    }

    let valid = !findings
        .iter()
        .any(|f| f.severity == validation::Severity::Error);

    Ok(Json(ValidateResponse { valid, findings }))
}

/// `POST /api/v1/stone/greenhouse/generate`
///
/// Generates a full manifest file set from image inspection JSON.
/// The inspection payload is the same JSON returned by
/// `GET /api/v1/stone/offerings/inspect?image={ref}`.
pub async fn generate_manifest_v1(
    Json(payload): Json<GenerateRequest>,
) -> Result<Json<GenerateResponse>, (StatusCode, Json<ApiErrorResponse>)> {
    let result = generate::generate_from_inspection(
        payload.name.as_deref(),
        payload.category.as_deref(),
        &payload.inspection,
    )
    .map_err(|e| {
        bad_request(
            "GENERATION_FAILED",
            format!("Failed to generate manifest: {e}"),
        )
    })?;

    Ok(Json(GenerateResponse {
        name: result.name,
        snippet_yaml: result.snippet_yaml,
        frontmatter_json: result.frontmatter_json,
        compatibility_yaml: result.compatibility_yaml,
        guidance_md: result.guidance_md,
    }))
}
