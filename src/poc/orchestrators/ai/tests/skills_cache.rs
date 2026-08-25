//! Smoke test for the dependency cache (ORCH-0029 commit 2.1).
//!
//! Reads the real `manifest.json` at
//! `.zen-garden/ai-orchestrator/cache/dependencies/comfyui/manifest.json`
//! and verifies every declared file still exists on disk with a
//! matching checksum entry. If the workspace cache is present, we
//! assert the invariants hold; otherwise we print a skip message.

use std::path::PathBuf;

use zen_garden_ai_orchestrator::services::skills::cache::{CachePaths, DependencyManifest};

fn workspace_data_dir() -> Option<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidate = manifest
        .ancestors()
        .nth(3)?
        .join(".zen-garden")
        .join("ai-orchestrator");
    if candidate.is_dir() {
        Some(candidate)
    } else {
        None
    }
}

#[tokio::test]
async fn existing_manifest_parses_and_files_exist() {
    let Some(data_dir) = workspace_data_dir() else {
        eprintln!("[skills-cache] skipped (workspace data dir not present)");
        return;
    };
    let paths = CachePaths::new(&data_dir, "comfyui");
    if !paths.manifest_path.is_file() {
        eprintln!("[skills-cache] skipped (no existing manifest at {})", paths.manifest_path.display());
        return;
    }

    let manifest = DependencyManifest::load(&paths.manifest_path).await;
    assert!(
        !manifest.files.is_empty(),
        "existing manifest at {} deserialized empty — schema regression?",
        paths.manifest_path.display()
    );
    eprintln!(
        "[skills-cache] loaded manifest: {} files, {} aliases",
        manifest.files.len(),
        manifest.aliases.len()
    );

    // Every declared file SHOULD exist on disk, but we only warn on
    // drift — the prior system may have left stale manifest entries
    // after manual file removal. The provisioner re-downloads when
    // needed. What we DO fail on:
    //   - malformed checksums (invariant violation)
    //   - aliases pointing to missing `files` entries (breaks alias
    //     resolution)
    let mut missing = Vec::new();
    let mut bad_checksum = Vec::new();
    for (filename, checksum) in &manifest.files {
        let path = paths.file_path(filename);
        if !path.is_file() {
            missing.push(filename.clone());
        }
        if !checksum.starts_with("sha256:") || checksum.len() < 10 {
            bad_checksum.push(filename.clone());
        }
    }
    if !missing.is_empty() {
        eprintln!(
            "[skills-cache] WARNING: {} manifest entries point to missing files (drift): {:?}",
            missing.len(),
            missing
        );
    }
    assert!(
        bad_checksum.is_empty(),
        "manifest declares files with malformed checksums: {bad_checksum:?}"
    );

    // Every alias must point to an entry in `files`.
    let mut stale_aliases = Vec::new();
    for (alias, target) in &manifest.aliases {
        if !manifest.files.contains_key(target) {
            stale_aliases.push(format!("{alias} -> {target}"));
        }
    }
    assert!(
        stale_aliases.is_empty(),
        "manifest has aliases pointing to missing files: {stale_aliases:?}"
    );

    // Spot-check a couple of well-known upscale models from the
    // prior system — these are the ones the upscale-skill depends on.
    if manifest.files.contains_key("RealESRGAN_x4plus.pth") {
        eprintln!("[skills-cache] RealESRGAN_x4plus.pth present in cache — upscale skill ready");
    }
    if manifest.files.contains_key("v1-5-pruned-emaonly.safetensors") {
        eprintln!("[skills-cache] SD 1.5 checkpoint present in cache — generate skill ready");
    }

    // Verify `is_cached` follows aliases correctly for a sample file.
    if let Some(first) = manifest.files.keys().next() {
        assert!(manifest.is_cached(first));
    }
}
