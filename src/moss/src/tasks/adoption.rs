//! Adoption task starters
//!
//! Thin wrappers that check configuration and spawn the auto-adoption
//! background task from `auto_adoption`.

use crate::{AppState, auto_adoption_task, infra};
use garden_common::console::{ConsoleEvent, ConsolePrinter, EventCategory, EventStatus};
use tokio_util::sync::CancellationToken;

/// Start auto-adoption task if enabled
pub fn start_auto_adoption(
    state: AppState,
    config: infra::MossConfig,
    console: &ConsolePrinter,
    token: CancellationToken,
) {
    let adoption_config = config.adoption();
    start_auto_adoption_with_config(state, adoption_config, console, token);
}

/// Start auto-adoption task with explicit AdoptionConfig
///
/// Use this variant when no MossConfig file is available - it will use
/// deployment profile detection to determine if adoption should be enabled.
pub fn start_auto_adoption_with_config(
    state: AppState,
    adoption_config: infra::AdoptionConfig,
    console: &ConsolePrinter,
    token: CancellationToken,
) {
    if adoption_config.is_enabled() {
        tracing::info!("Auto-adoption enabled, starting adoption background task");
        console.emit(ConsoleEvent::new(
            EventCategory::Config,
            EventStatus::Loaded,
            "Auto-adoption enabled",
        ));

        tokio::spawn(async move {
            auto_adoption_task(state, adoption_config, token).await;
        });
    } else {
        tracing::info!("Auto-adoption disabled (deployment profile or configuration)");
        console.emit(ConsoleEvent::new(
            EventCategory::Config,
            EventStatus::Loaded,
            "Auto-adoption disabled",
        ));
    }
}
