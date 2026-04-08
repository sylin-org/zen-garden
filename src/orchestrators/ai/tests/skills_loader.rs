//! Smoke test for the skills loader (ORCH-0029 commit 2b).
//!
//! Runs the loader against the workspace's real skill directory at
//! `.zen-garden/ai-orchestrator/skills/` and asserts that every
//! on-disk skill deserializes into a typed `SkillDefinition` with no
//! loader errors.
//!
//! This is gated on the workspace data dir existing — when running
//! in CI against a clean checkout, the test no-ops.

use std::path::PathBuf;

use zen_garden_ai_orchestrator::services::skills::loader;

/// Locate the workspace data dir relative to the orchestrator crate.
fn workspace_skills_dir() -> Option<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // From `src/orchestrators/ai` up to the repo root, then down to
    // the workspace data dir.
    let candidate = manifest
        .ancestors()
        .nth(3)? // repo root
        .join(".zen-garden")
        .join("ai-orchestrator")
        .join("skills");
    if candidate.is_dir() {
        Some(candidate)
    } else {
        None
    }
}

#[tokio::test]
async fn loader_parses_every_on_disk_skill() {
    let Some(skills_dir) = workspace_skills_dir() else {
        eprintln!("[skills-loader] skipped (workspace .zen-garden/ai-orchestrator/skills/ not present)");
        return;
    };
    eprintln!("[skills-loader] scanning {}", skills_dir.display());

    let loaded = loader::load_skills(&skills_dir).await;
    assert!(
        !loaded.is_empty(),
        "loader returned no skills from {} — expected at least the 6 embedded built-ins",
        skills_dir.display()
    );

    eprintln!("[skills-loader] loaded {} skills", loaded.len());
    for def in &loaded {
        eprintln!(
            "  {} / {} ({}): {} bindings, {} models, {} variants, selector={}",
            def.primitive.dotted(),
            def.moniker.as_str(),
            def.display_name,
            def.bindings.len(),
            def.required_models.len(),
            def.variants.as_ref().map(|v| v.len()).unwrap_or(0),
            def.model_selector.is_some(),
        );
        // Every loaded skill must have at least one workflow file.
        assert!(
            def.workflows.contains_key(&def.default_workflow),
            "skill `{}` declares default_workflow `{}` but it was not loaded from disk",
            def.moniker.as_str(),
            def.default_workflow
        );
    }

    // Per the workspace on-disk state as of ORCH-0029 drafting: there
    // should be 20 skills (6 embedded + 14 imported). Allow more
    // (import pipeline may add) but fail if we lost some during load.
    assert!(
        loaded.len() >= 20,
        "expected at least 20 skills from the workspace data dir, loaded {}",
        loaded.len()
    );

    // Upscale must be loaded, with four workflows (2x/4x/8x/16x).
    // The directory name `upscale` collides with the reserved primitive
    // leaf; the loader sanitizes it to `upscale-skill`.
    let upscale = loaded
        .iter()
        .find(|s| s.moniker.as_str() == "upscale-skill")
        .expect("`upscale-skill` (from `upscale/`) not loaded");
    assert_eq!(upscale.primitive.dotted(), "image.upscale");
    assert!(
        upscale.workflows.len() >= 4,
        "upscale should have 4 workflow files (2x/4x/8x/16x), found {}",
        upscale.workflows.len()
    );
    let variants = upscale
        .variants
        .as_ref()
        .expect("upscale should have variants from the workflow selector hoist");
    let variant_names: Vec<&str> = variants.iter().map(|v| v.value.as_str()).collect();
    for expected in ["upscale_2x", "upscale_4x", "upscale_8x", "upscale_16x"] {
        assert!(
            variant_names.contains(&expected),
            "upscale missing variant `{expected}`; got {variant_names:?}"
        );
    }
    // Upscale must have a model selector (Realistic vs Anime).
    let selector = upscale
        .model_selector
        .as_ref()
        .expect("upscale should have a model_selector");
    assert_eq!(selector.options.len(), 2, "upscale model selector should offer 2 options");

    // Generate must be loaded with its SD 1.5 checkpoint.
    // `generate` is reserved; sanitized to `generate-skill`.
    let generate = loaded
        .iter()
        .find(|s| s.moniker.as_str() == "generate-skill")
        .expect("`generate-skill` (from `generate/`) not loaded");
    assert_eq!(generate.primitive.dotted(), "image.generate");
    assert!(
        generate
            .required_models
            .iter()
            .any(|m| m.filename == "v1-5-pruned-emaonly.safetensors"),
        "generate should require the SD 1.5 checkpoint"
    );

    // Inpaint exposes the mask overlay via the MASK binding.
    let inpaint = loaded
        .iter()
        .find(|s| s.moniker.as_str() == "inpaint")
        .expect("`inpaint` skill not loaded");
    assert_eq!(inpaint.primitive.dotted(), "image.edit");
    let mask_binding = inpaint
        .bindings
        .iter()
        .find(|b| b.field.as_str() == "image.mask")
        .expect("inpaint should bind image.mask");
    assert_eq!(
        mask_binding.overlay.as_deref(),
        Some("source"),
        "inpaint mask should overlay on source"
    );

    // Tag skill (vision.tag) should have been translated to image.analyze.
    let tag = loaded
        .iter()
        .find(|s| s.moniker.as_str() == "tag")
        .expect("`tag` skill not loaded");
    assert_eq!(tag.primitive.dotted(), "image.analyze");

    // TTS skill (speech.tts) should have been translated to audio.generate
    // and expose its two engines as variants.
    let tts = loaded
        .iter()
        .find(|s| s.moniker.as_str() == "tts")
        .expect("`tts` skill not loaded");
    assert_eq!(tts.primitive.dotted(), "audio.generate");
    let tts_variants = tts
        .variants
        .as_ref()
        .expect("tts should have variants (tts + tts_f5)");
    assert_eq!(tts_variants.len(), 2);
}
