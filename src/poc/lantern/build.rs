use std::path::Path;

fn main() {
    // Capture build timestamp from environment or use "dev"
    let build_number = std::env::var("CARGO_BUILD_NUMBER").unwrap_or_else(|_| "dev".to_string());
    println!("cargo:rustc-env=BUILD_NUMBER={}", build_number);

    // Re-run if environment variable changes
    println!("cargo:rerun-if-env-changed=CARGO_BUILD_NUMBER");

    // Guarantee the embedded-frontend directory exists so rust-embed's
    // `#[derive(Embed)]` over `frontend/dist/` (src/infra/embedded.rs) can scan
    // it at compile time. The built SPA lands there only after the frontend
    // toolchain runs, and `frontend/dist` is gitignored — so a fresh clone or CI
    // checkout has none and `garden-lantern` would fail to compile.
    ensure_frontend_dist();
}

/// Create `frontend/dist/` if missing and drop a placeholder `index.html` when
/// the real SPA has not been built. A populated `frontend/dist/` is left as-is,
/// so release builds embed the real assets.
fn ensure_frontend_dist() {
    let dist = Path::new("frontend/dist");
    std::fs::create_dir_all(dist)
        .unwrap_or_else(|e| panic!("failed to create {}: {e}", dist.display()));

    let index = dist.join("index.html");
    if !index.exists() {
        let placeholder = concat!(
            "<!doctype html>\n",
            "<html lang=\"en\">\n",
            "<head><meta charset=\"utf-8\"><title>Lantern</title></head>\n",
            "<body>\n",
            "  <h1>Lantern dashboard</h1>\n",
            "  <p>The dashboard frontend has not been built. Build it under\n",
            "     <code>src/lantern/frontend</code> and rebuild so the assets are embedded.</p>\n",
            "</body>\n",
            "</html>\n",
        );
        std::fs::write(&index, placeholder)
            .unwrap_or_else(|e| panic!("failed to write placeholder {}: {e}", index.display()));
    }

    println!("cargo:rerun-if-changed=frontend/dist");
}
