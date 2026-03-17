// Binary-only modules (not available to library crate)
mod dispatch;
mod route;

// Use shared modules from the library
use garden_rake::ui::rendering as ui;
use garden_rake::cli_build::{build_clap_app, count_verbosity, extract_global_flags};
use garden_rake::command_manifest::MANIFEST;
use garden_rake::commands;
use garden_rake::enrollment;

use std::time::Duration;
use tracing_subscriber::EnvFilter;

#[cfg(test)]
mod discovery_tests;

#[cfg(test)]
mod recommendation_tests;

// Windows debug builds need larger stack for async/clap combination
#[cfg(all(windows, debug_assertions))]
fn main() -> anyhow::Result<()> {
    std::thread::Builder::new()
        .stack_size(4 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async_main())
        })?
        .join()
        .map_err(|_| anyhow::anyhow!("Thread panic"))?
}

#[cfg(not(all(windows, debug_assertions)))]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    async_main().await
}

async fn async_main() -> anyhow::Result<()> {
    // Validate command manifest in debug builds
    #[cfg(debug_assertions)]
    garden_rake::command_manifest::validate_manifest();

    let raw_args: Vec<String> = std::env::args().skip(1).collect();

    // Pre-parse verbosity level from raw args (before tracing init)
    let verbosity = count_verbosity(&raw_args);

    // Initialize tracing with appropriate level
    let env_filter = if verbosity > 0 {
        let level = match verbosity {
            1 => "info",
            2 => "debug",
            _ => "trace",
        };
        EnvFilter::new(format!(
            "garden_rake={},garden_common={},garden_moss={}",
            level, level, level
        ))
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"))
    };

    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    // Handle help query syntax: ?command or command?
    if !raw_args.is_empty() {
        let first_arg = &raw_args[0];

        let help_name = if first_arg.starts_with('?') {
            Some(first_arg.trim_start_matches('?'))
        } else if first_arg.ends_with('?') {
            Some(first_arg.trim_end_matches('?'))
        } else {
            None
        };

        if let Some(name) = help_name {
            if !name.is_empty() {
                if let Some(cmd) = MANIFEST.get(name) {
                    commands::help::display_command_detail(cmd, false, false);
                    return Ok(());
                } else {
                    eprintln!("Unknown command: {}", name);
                    std::process::exit(1);
                }
            }
        }
    }

    // Build Clap app from manifest and parse directly — no normalization layer
    let app = build_clap_app(&MANIFEST);

    let matches = app
        .try_get_matches_from(
            std::iter::once("garden-rake".to_string()).chain(raw_args),
        )
        .unwrap_or_else(|e| e.exit());

    // Extract global flags from ArgMatches
    let global = extract_global_flags(&matches);

    // Create pooled HTTP client with connection reuse
    let mut client_builder = reqwest::Client::builder()
        .pool_max_idle_per_host(10)
        .pool_idle_timeout(Duration::from_secs(90))
        .tcp_keepalive(Duration::from_secs(60))
        .connect_timeout(Duration::from_secs(30));

    // Add X-Quiet header if quiet mode is active
    if global.quiet {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("X-Quiet", "true".parse().unwrap());
        client_builder = client_builder.default_headers(headers);
    }

    // Configure mTLS if this machine is enrolled in a pond
    let hostname = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "unknown".to_string());

    if let Some((ca_cert_pem, client_cert_pem, client_key_pem)) =
        enrollment::load_tls_materials(&hostname)
    {
        if let Ok(ca_cert) = reqwest::Certificate::from_pem(ca_cert_pem.as_bytes()) {
            client_builder = client_builder.add_root_certificate(ca_cert);
        }
        let identity_pem = format!("{}\n{}", client_cert_pem, client_key_pem);
        if let Ok(identity) = reqwest::Identity::from_pem(identity_pem.as_bytes()) {
            client_builder = client_builder.identity(identity);
        }
        tracing::debug!(hostname = %hostname, "mTLS configured from pond enrollment certs");
    }

    let client = client_builder.build()?;
    let term = ui::TerminalInfo::detect();

    // Build Runtime once — encapsulates client, global flags, terminal info
    let rt = dispatch::Runtime::new(client, global.clone(), term);

    // Route to command handler
    match matches.subcommand() {
        Some((name, sub_matches)) => {
            if let Some(inv) = route::route(name, sub_matches, &global, &rt).await? {
                rt.execute(inv).await?;
            }
        }
        None => {
            // No subcommand — show command directory
            commands::help::display_all_commands(false, false);
        }
    }

    Ok(())
}
