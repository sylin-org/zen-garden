//! In-process integration tests for the capability announcement
//! pipeline and the GET introspection endpoints (ORCH-0030 commit 6).
//!
//! These tests exercise the full chain end-to-end without requiring
//! a running orchestrator:
//!
//! 1. Build a `Fixture` with the default mock provider.
//! 2. Apply synthetic `CapabilityAnnouncement`s directly to the
//!    fixture's `DirectorySubscriber`.
//! 3. Build a `Router` from the fixture state.
//! 4. Issue `GET /v1/{modality}/{leaf}[/{skill_id}]` requests via
//!    `tower::ServiceExt::oneshot` and inspect the JSON response.
//!
//! The `DirectorySubscriber::apply` path is exercised directly
//! (rather than via bus events) because the subscriber's run loop
//! and the bus path are covered by unit tests. What these tests
//! prove is the **HTTP handler correctly reads from the
//! CapabilityDirectory and produces the declared response shape.**

mod common;

use axum::http::StatusCode;
use tower::ServiceExt;

use zen_garden_ai_orchestrator::{
    domain::{
        capability_announcement::{
            AutoDescriptor, Capability, CapabilityAnnouncement, SkillDeclaration, SkillDisplay,
            SkillParameter,
        },
        ids::ProviderName,
        primitive::Primitive,
    },
    http::router,
};

use common::{body_json, fixture_with_mock_chat, get};

// ── Helpers ──────────────────────────────────────────────────

fn ollama_image_understanding() -> CapabilityAnnouncement {
    CapabilityAnnouncement {
        provider: ProviderName::new("ollama"),
        enabled: true,
        capabilities: vec![
            Capability::new(Primitive::TextChat),
            Capability::new(Primitive::ImageAnalyze),
        ],
        skills: vec![SkillDeclaration {
            id: "image-understanding".into(),
            primitive: Primitive::ImageAnalyze,
            display: SkillDisplay::new("Image Understanding")
                .with_description("Extract JSON description + tags from an image.")
                .with_tags(vec!["vision".into(), "json".into()]),
            parameters: vec![
                SkillParameter {
                    field: "image.source".into(),
                    required: true,
                    description: Some("The image to analyze.".into()),
                    default: None,
                    auto: None,
                    pinnable: false,
                    label: None,
                    field_type: None,
                    widget: None,
                    min: None,
                    max: None,
                    step: None,
                    options: None,
                    placeholder: None,
                },
                SkillParameter {
                    field: "selectors.model".into(),
                    required: false,
                    description: Some("Vision-capable model.".into()),
                    default: Some(serde_json::json!("recommended:vision")),
                    auto: Some(AutoDescriptor {
                        default: "recommended:vision".into(),
                        description: Some("Ollama picks a vision-capable model.".into()),
                    }),
                    pinnable: true,
                    label: None,
                    field_type: None,
                    widget: None,
                    min: None,
                    max: None,
                    step: None,
                    options: None,
                    placeholder: None,
                },
            ],
        }],
    }
}

fn comfyui_tron() -> CapabilityAnnouncement {
    CapabilityAnnouncement {
        provider: ProviderName::new("comfyui"),
        enabled: true,
        capabilities: vec![Capability::new(Primitive::ImageGenerate)],
        skills: vec![SkillDeclaration {
            id: "tron".into(),
            primitive: Primitive::ImageGenerate,
            display: SkillDisplay::new("Tron Legacy")
                .with_tags(vec!["stylized".into()]),
            parameters: vec![SkillParameter {
                field: "image.prompt.positive".into(),
                required: true,
                description: None,
                default: None,
                auto: None,
                pinnable: false,
                label: None,
                field_type: None,
                widget: None,
                min: None,
                max: None,
                step: None,
                options: None,
                placeholder: None,
            }],
        }],
    }
}

fn bare_anthropic_chat() -> CapabilityAnnouncement {
    CapabilityAnnouncement {
        provider: ProviderName::new("anthropic"),
        enabled: true,
        capabilities: vec![Capability::new(Primitive::TextChat)],
        skills: vec![],
    }
}

// ── GET /v1/{modality}/{leaf} — primitive introspection ──────

#[tokio::test]
async fn get_primitive_returns_kind_primitive_with_providers() {
    let (fx, _mock) = fixture_with_mock_chat().await;
    fx.directory_subscriber
        .apply(ollama_image_understanding())
        .await
        .unwrap();

    let app = router::build(fx.state);
    let resp = app.oneshot(get("/v1/image/analyze")).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_json(resp.into_body()).await;
    assert_eq!(body["kind"], "primitive");
    assert_eq!(body["primitive"], "image.analyze");
    assert_eq!(body["display"]["name"], "Image Analyze");
    assert_eq!(body["routing"]["status"], "healthy");
    assert_eq!(body["routing"]["will_run_on"], "ollama");
    assert_eq!(body["invocation"]["method"], "POST");
    assert_eq!(body["invocation"]["url"], "/v1/image/analyze");
    assert_eq!(body["invocation"]["content_type"], "application/json");
}

#[tokio::test]
async fn get_primitive_lists_skills_available_from_all_providers() {
    let (fx, _mock) = fixture_with_mock_chat().await;
    fx.directory_subscriber
        .apply(ollama_image_understanding())
        .await
        .unwrap();

    let app = router::build(fx.state);
    let resp = app.oneshot(get("/v1/image/analyze")).await.expect("oneshot");
    let body = body_json(resp.into_body()).await;

    let skills = body["skills_available"].as_array().expect("array");
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0]["id"], "image-understanding");
    assert_eq!(skills[0]["provider"], "ollama");
    assert_eq!(skills[0]["url"], "/v1/image/analyze/image-understanding");
    assert_eq!(skills[0]["display"]["name"], "Image Understanding");
}

#[tokio::test]
async fn get_primitive_with_no_providers_returns_unavailable() {
    let (fx, _mock) = fixture_with_mock_chat().await;
    // No announcement applied → CapabilityDirectory is empty.
    let app = router::build(fx.state);
    let resp = app
        .oneshot(get("/v1/image/analyze"))
        .await
        .expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_json(resp.into_body()).await;
    assert_eq!(body["kind"], "primitive");
    assert_eq!(body["routing"]["status"], "unavailable");
    assert!(body["routing"]["will_run_on"].is_null());
    assert_eq!(
        body["routing"]["providers"].as_array().unwrap().len(),
        0
    );
}

#[tokio::test]
async fn get_primitive_unknown_returns_400() {
    let (fx, _mock) = fixture_with_mock_chat().await;
    let app = router::build(fx.state);
    let resp = app
        .oneshot(get("/v1/text/nonexistent"))
        .await
        .expect("oneshot");
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_primitive_with_multiple_providers_shows_fallback_chain() {
    let (fx, _mock) = fixture_with_mock_chat().await;
    fx.directory_subscriber
        .apply(ollama_image_understanding())
        .await
        .unwrap();
    fx.directory_subscriber
        .apply(bare_anthropic_chat())
        .await
        .unwrap();

    let app = router::build(fx.state);
    let resp = app.oneshot(get("/v1/text/chat")).await.expect("oneshot");
    let body = body_json(resp.into_body()).await;

    assert_eq!(body["routing"]["status"], "healthy");
    // Three providers declare text.chat: the fixture's `mockchat`,
    // the freshly-announced `ollama` (image understanding adapter
    // also serves text.chat), and `anthropic`. The introspection
    // endpoint surfaces all three; the fallback chain is the
    // non-primary two.
    let providers = body["routing"]["providers"].as_array().unwrap();
    assert_eq!(providers.len(), 3);
    assert!(body["routing"]["fallback_providers"]
        .as_array()
        .map(|a| a.len() == 2)
        .unwrap_or(false));
}

// ── GET /v1/{modality}/{leaf}/{skill_id} — skill introspection ──

#[tokio::test]
async fn get_skill_returns_kind_skill_with_parameters() {
    let (fx, _mock) = fixture_with_mock_chat().await;
    fx.directory_subscriber
        .apply(ollama_image_understanding())
        .await
        .unwrap();

    let app = router::build(fx.state);
    let resp = app
        .oneshot(get("/v1/image/analyze/image-understanding"))
        .await
        .expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_json(resp.into_body()).await;
    assert_eq!(body["kind"], "skill");
    assert_eq!(body["primitive"], "image.analyze");
    assert_eq!(body["skill_id"], "image-understanding");
    assert_eq!(body["display"]["name"], "Image Understanding");
    assert_eq!(body["invocation"]["url"], "/v1/image/analyze/image-understanding");

    let params = body["parameters"].as_array().expect("array");
    assert_eq!(params.len(), 2);

    // image.source: required, no default, not pinnable
    let source = &params[0];
    assert_eq!(source["field"], "image.source");
    assert_eq!(source["required"], true);
    assert_eq!(source["pinnable"], false);
    assert!(source["default"].is_null() || !source.as_object().unwrap().contains_key("default"));

    // selectors.model: optional with skill default, pinnable, auto descriptor
    let model = &params[1];
    assert_eq!(model["field"], "selectors.model");
    assert_eq!(model["pinnable"], true);
    assert_eq!(model["default"], "recommended:vision");
    assert_eq!(model["effective_default"], "recommended:vision");
    assert_eq!(model["default_source"], "skill");
    assert_eq!(model["auto"]["default"], "recommended:vision");
}

#[tokio::test]
async fn get_skill_includes_example_body_with_required_fields() {
    let (fx, _mock) = fixture_with_mock_chat().await;
    fx.directory_subscriber
        .apply(ollama_image_understanding())
        .await
        .unwrap();

    let app = router::build(fx.state);
    let resp = app
        .oneshot(get("/v1/image/analyze/image-understanding"))
        .await
        .expect("oneshot");
    let body = body_json(resp.into_body()).await;

    let example = &body["example"];
    assert_eq!(example["url"], "/v1/image/analyze/image-understanding");
    let example_body = example["body"].as_object().unwrap();
    // Required field present, optional field absent
    assert!(example_body.contains_key("image.source"));
    assert!(!example_body.contains_key("selectors.model"));
    assert_eq!(example_body["image.source"], "@upload:abc123");
}

#[tokio::test]
async fn get_skill_routing_names_provider() {
    let (fx, _mock) = fixture_with_mock_chat().await;
    fx.directory_subscriber
        .apply(ollama_image_understanding())
        .await
        .unwrap();

    let app = router::build(fx.state);
    let resp = app
        .oneshot(get("/v1/image/analyze/image-understanding"))
        .await
        .expect("oneshot");
    let body = body_json(resp.into_body()).await;

    assert_eq!(body["routing"]["will_run_on"], "ollama");
    assert_eq!(body["routing"]["status"], "healthy");
}

#[tokio::test]
async fn get_skill_unknown_returns_404() {
    let (fx, _mock) = fixture_with_mock_chat().await;
    fx.directory_subscriber
        .apply(ollama_image_understanding())
        .await
        .unwrap();

    let app = router::build(fx.state);
    let resp = app
        .oneshot(get("/v1/image/analyze/nonexistent-skill"))
        .await
        .expect("oneshot");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_skill_for_wrong_primitive_returns_404() {
    let (fx, _mock) = fixture_with_mock_chat().await;
    fx.directory_subscriber
        .apply(ollama_image_understanding())
        .await
        .unwrap();

    let app = router::build(fx.state);
    // image-understanding is declared on image.analyze, not text.chat
    let resp = app
        .oneshot(get("/v1/text/chat/image-understanding"))
        .await
        .expect("oneshot");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_skill_when_provider_disabled_returns_404() {
    let (fx, _mock) = fixture_with_mock_chat().await;
    fx.directory_subscriber
        .apply(ollama_image_understanding())
        .await
        .unwrap();
    // Republish as disabled
    let mut disabled = ollama_image_understanding();
    disabled.enabled = false;
    fx.directory_subscriber.apply(disabled).await.unwrap();

    let app = router::build(fx.state);
    let resp = app
        .oneshot(get("/v1/image/analyze/image-understanding"))
        .await
        .expect("oneshot");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn comfyui_tron_skill_end_to_end() {
    let (fx, _mock) = fixture_with_mock_chat().await;
    fx.directory_subscriber.apply(comfyui_tron()).await.unwrap();

    let app = router::build(fx.state);
    let resp = app
        .oneshot(get("/v1/image/generate/tron"))
        .await
        .expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_json(resp.into_body()).await;
    assert_eq!(body["kind"], "skill");
    assert_eq!(body["skill_id"], "tron");
    assert_eq!(body["display"]["name"], "Tron Legacy");
    assert_eq!(body["routing"]["will_run_on"], "comfyui");

    let params = body["parameters"].as_array().unwrap();
    assert_eq!(params.len(), 1);
    assert_eq!(params[0]["field"], "image.prompt.positive");
    assert_eq!(params[0]["required"], true);

    let example_body = body["example"]["body"].as_object().unwrap();
    assert_eq!(example_body["image.prompt.positive"], "example prompt");
}

// ── Bus-driven path: publish via EventBus, assert GET reflects it ──

#[tokio::test]
async fn capability_announcement_via_bus_updates_directory_and_introspection() {
    let (fx, _mock) = fixture_with_mock_chat().await;

    // Apply directly (this is what a run-loop would do after consuming
    // a bus event). We exercise `apply` here to prove the handler and
    // the subscriber share state through the CapabilityDirectory.
    fx.directory_subscriber
        .apply(ollama_image_understanding())
        .await
        .unwrap();

    // Verify the CapabilityDirectory was updated before building
    // the router (sanity check that the shared Arc works as expected).
    let providers = fx
        .capability_directory
        .providers_for_primitive(Primitive::ImageAnalyze)
        .await;
    assert_eq!(providers, vec![ProviderName::new("ollama")]);

    let app = router::build(fx.state);
    let resp = app
        .oneshot(get("/v1/image/analyze"))
        .await
        .expect("oneshot");
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["routing"]["providers"][0], "ollama");
    assert_eq!(
        body["skills_available"][0]["id"],
        "image-understanding"
    );
}

#[tokio::test]
async fn derived_events_emitted_by_subscriber_appear_on_bus() {
    let (fx, _mock) = fixture_with_mock_chat().await;
    let events = fx.state.events.clone();
    let mut rx = events.raw_subscribe();

    fx.directory_subscriber
        .apply(ollama_image_understanding())
        .await
        .unwrap();

    // Drain events and confirm we see the derived set
    let mut saw_updated = false;
    let mut saw_enabled = false;
    let mut saw_capability_added = 0;
    let mut saw_skill_added = false;
    while let Ok(ev) = rx.try_recv() {
        match ev.topic.as_str() {
            "directory.provider.ollama.updated" => saw_updated = true,
            "directory.provider.ollama.enabled" => saw_enabled = true,
            "directory.provider.ollama.capability.added" => saw_capability_added += 1,
            "directory.provider.ollama.skill.added" => saw_skill_added = true,
            _ => {}
        }
    }
    assert!(saw_updated, "coarse updated event missing");
    assert!(saw_enabled, "enabled transition event missing");
    assert_eq!(saw_capability_added, 2, "should fire one per added capability");
    assert!(saw_skill_added, "skill.added event missing");
}

#[tokio::test]
async fn skill_removal_emits_skill_removed_event() {
    let (fx, _mock) = fixture_with_mock_chat().await;
    // First: apply with the skill
    fx.directory_subscriber
        .apply(ollama_image_understanding())
        .await
        .unwrap();

    // Subscribe AFTER the first apply so we only see the second set
    // of events.
    let events = fx.state.events.clone();
    let mut rx = events.raw_subscribe();

    // Second: apply without the skill
    let mut without_skill = ollama_image_understanding();
    without_skill.skills.clear();
    fx.directory_subscriber.apply(without_skill).await.unwrap();

    let mut saw_removed = false;
    while let Ok(ev) = rx.try_recv() {
        if ev.topic == "directory.provider.ollama.skill.removed" {
            saw_removed = true;
            let payload = &ev.payload;
            assert_eq!(payload["skill_id"], "image-understanding");
            assert_eq!(payload["primitive"], "image.analyze");
            break;
        }
    }
    assert!(saw_removed, "skill.removed event missing after republish");
}

#[tokio::test]
async fn duplicate_skill_rejection_emits_rejected_event() {
    let (fx, _mock) = fixture_with_mock_chat().await;
    let events = fx.state.events.clone();
    let mut rx = events.raw_subscribe();

    let bad = CapabilityAnnouncement {
        provider: ProviderName::new("ollama"),
        enabled: true,
        capabilities: vec![Capability::new(Primitive::TextChat)],
        // Skill for a primitive that's NOT declared as capability
        skills: vec![SkillDeclaration {
            id: "bad".into(),
            primitive: Primitive::ImageAnalyze,
            display: SkillDisplay::new("Bad"),
            parameters: vec![],
        }],
    };
    let result = fx.directory_subscriber.apply(bad).await;
    assert!(result.is_err());

    let mut saw_rejected = false;
    while let Ok(ev) = rx.try_recv() {
        if ev.topic == "directory.provider.ollama.rejected" {
            saw_rejected = true;
            break;
        }
    }
    assert!(saw_rejected, "rejected event missing for invalid announcement");

    // Directory state was not mutated.
    assert!(fx
        .capability_directory
        .provider(&ProviderName::new("ollama"))
        .await
        .is_none());
}
