//! API command - display Moss HTTP API reference
//!
//! Queries GET /api/v1/manifest and displays formatted endpoint documentation

use anyhow::Result;
use garden_common::api_manifest::ApiManifest;

/// Execute API command - display API reference
pub async fn execute_api_command(
    endpoint: &str,
    category_filter: Option<String>,
    endpoint_filter: Option<String>,
    examples: bool,
) -> Result<()> {
    // Fetch manifest
    let url = format!("{}/api/v1/manifest", endpoint.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let manifest: ApiManifest = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?
        .json()
        .await?;

    // Filter and display
    if let Some(endpoint_path) = endpoint_filter {
        display_endpoint_detail(&manifest, &endpoint_path, examples)?;
    } else if let Some(category) = category_filter {
        display_category(&manifest, &category, examples)?;
    } else {
        display_all_categories(&manifest, examples)?;
    }

    Ok(())
}

fn display_all_categories(manifest: &ApiManifest, show_examples: bool) -> Result<()> {
    println!("=== MOSS API v{} ===", manifest.version);
    println!("Base URL: {}", manifest.base_url);
    println!();

    for category in &manifest.categories {
        if category.endpoints.is_empty() {
            continue;
        }

        println!("# {}", category.name);
        println!("  {}", category.description);
        println!();

        // Find endpoints in this category
        let category_endpoints: Vec<_> = manifest
            .endpoints
            .iter()
            .filter(|e| e.category == category.name)
            .collect();

        for endpoint in category_endpoints {
            println!("  {:6} {}", endpoint.method, endpoint.path);
            println!("     {}", endpoint.description);

            if show_examples && !endpoint.examples.is_empty() {
                println!("     Example: {}", endpoint.examples[0].curl);
            }
            println!();
        }
    }

    println!("Run `garden-rake api <endpoint>` for detailed docs and examples");
    println!("Run `garden-rake api --category <name>` to filter by category");

    Ok(())
}

fn display_category(
    manifest: &ApiManifest,
    category_name: &str,
    show_examples: bool,
) -> Result<()> {
    let category = manifest
        .categories
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case(category_name))
        .ok_or_else(|| anyhow::anyhow!("Category '{}' not found", category_name))?;

    println!("{} API", category.name);
    println!("{}", category.description);
    println!();

    let endpoints: Vec<_> = manifest
        .endpoints
        .iter()
        .filter(|e| e.category == category.name)
        .collect();

    for endpoint in endpoints {
        println!("  {:6} {}", endpoint.method, endpoint.path);
        println!("     {}", endpoint.description);

        if show_examples && !endpoint.examples.is_empty() {
            println!("     Example: {}", endpoint.examples[0].curl);
        }
        println!();
    }

    Ok(())
}

fn display_endpoint_detail(manifest: &ApiManifest, path: &str, show_examples: bool) -> Result<()> {
    let endpoint = manifest
        .endpoints
        .iter()
        .find(|e| e.path == path || e.path.contains(&path.replace('*', ":")))
        .ok_or_else(|| anyhow::anyhow!("Endpoint '{}' not found", path))?;

    println!("{:6} {}", endpoint.method, endpoint.path);
    println!("{}", endpoint.description);
    println!();

    // Path parameters
    if !endpoint.path_params.is_empty() {
        println!("Path Parameters:");
        for param in &endpoint.path_params {
            println!(
                "  {} ({}): {}",
                param.name, param.param_type, param.description
            );
        }
        println!();
    }

    // Query parameters
    if !endpoint.query_params.is_empty() {
        println!("Query Parameters:");
        for param in &endpoint.query_params {
            let required = if param.required {
                " (required)"
            } else {
                " (optional)"
            };
            println!("  {}{}: {}", param.name, required, param.description);
        }
        println!();
    }

    // Request body
    if let Some(body) = &endpoint.body_schema {
        println!("Request Body:");
        println!("  {}", body);
        println!();
    }

    // Response
    println!("Response Type:");
    println!("  {}", endpoint.response_type);
    println!();

    // Examples
    if show_examples && !endpoint.examples.is_empty() {
        println!("Examples:");
        for (i, example) in endpoint.examples.iter().enumerate() {
            if i > 0 {
                println!();
            }
            println!("  {}", example.title);
            if let Some(desc) = &example.description {
                println!("  {}", desc);
            }
            println!();
            println!("  $ {}", example.curl);
            println!();
            println!("  {}", example.response);
        }
        println!();
    }

    // Notes
    if !endpoint.notes.is_empty() {
        println!("Notes:");
        for note in &endpoint.notes {
            println!("  - {}", note);
        }
        println!();
    }

    Ok(())
}
