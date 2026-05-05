//! Wire types for the facilitator pipeline.
//!
//! `Suggestion` is what the UI renders. `SuggestionAction` is what
//! happens when the user clicks the primary button — the variant
//! is enough for the frontend to dispatch (Tend stone-X, navigate
//! to pond, etc.) without a separate per-action callback.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Suggestion {
    /// Session-stable id for telemetry / "this exact suggestion"
    /// dismissals. Rebuilt on each engine tick from the
    /// underlying state, so it stays the same as long as the
    /// state does.
    pub id: String,

    /// Kind discriminator used by `Settings::suppressed_kinds`
    /// to remember "Hide this kind" decisions across sessions.
    /// Always prefixed `"facilitator:"` to avoid colliding with
    /// announcer kind strings like `"stone_joined"`.
    pub kind: String,

    /// One-line title — banner heading.
    pub title: String,

    /// Two- to three-sentence body explaining the *why*.
    /// Tentative voice ("would," "could," "might be worth").
    pub body: String,

    /// Label for the primary-action button.
    pub action_label: String,

    /// What the action does. The frontend dispatches on this.
    pub action: SuggestionAction,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SuggestionAction {
    /// Tend the named stone (the one the suggestion is about).
    Tend { stone_id: String, stone_name: String },
    /// Open one of Pavilion's destinations.
    OpenView { view: String },
}
