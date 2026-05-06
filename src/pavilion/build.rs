fn main() {
    // Force cargo to re-run this build script (and therefore
    // re-emit the asset cache `tauri::generate_context!()` reads
    // at compile time) whenever the frontend dist changes.
    // tauri-build's own rerun directives don't reliably catch the
    // case where dist was missing on a prior run and got fully
    // (re-)populated by a subsequent successful frontend build —
    // cargo then thinks nothing changed and the binary embeds a
    // stale (or empty) asset table, producing
    // `asset not found: index.html` at runtime.
    println!("cargo:rerun-if-changed=frontend/dist");
    println!("cargo:rerun-if-changed=frontend/dist/index.html");
    tauri_build::build()
}
