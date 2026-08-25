//! Embedded SPA static files via rust-embed
//!
//! In release mode, static assets are compiled into the binary.
//! In debug mode, they are read from disk for hot-reload.

use rust_embed::Embed;

/// Embedded frontend assets (compiled from src/lantern/frontend/dist/)
#[derive(Embed)]
#[folder = "frontend/dist/"]
#[prefix = ""]
pub struct FrontendAssets;
