//! BackgroundTask: catalog-builder (ARCH-0015)
//!
//! One-shot task that builds the offerings catalog (manifest index) from
//! embedded and cached manifests.

use std::future::Future;
use std::pin::Pin;

use garden_common::console::{ConsoleEvent, EventCategory, EventStatus};

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct CatalogBuilderTask;

impl BackgroundTask for CatalogBuilderTask {
    fn name(&self) -> &'static str {
        "catalog-builder"
    }

    fn dependencies(&self) -> &'static [&'static str] {
        &["registry-loader"]
    }

    fn run(
        self: Box<Self>,
        mut ctx: TaskContext,
    ) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            if !ctx.deps.wait().await {
                return TaskOutcome::Cancelled;
            }

            tracing::info!("Building offerings catalog...");

            ctx.state.console.emit(ConsoleEvent::new(
                EventCategory::Manifests,
                EventStatus::Scanning,
                "Runtime templates".to_string(),
            ));

            match ctx.state.catalog.load().await {
                Ok(()) => {
                    let stats = ctx.state.catalog.stats().await;
                    tracing::info!(
                        offerings_count = stats.compiled_count,
                        "Offerings catalog loaded successfully"
                    );
                    ctx.state.console.emit(ConsoleEvent::new(
                        EventCategory::Manifests,
                        EventStatus::Loaded,
                        format!("{} manifests", stats.compiled_count),
                    ));
                }
                Err(e) => {
                    tracing::warn!(error = ?e, "Failed to build offerings catalog");
                    ctx.state.console.emit(ConsoleEvent::new(
                        EventCategory::Manifests,
                        EventStatus::Invalid,
                        "Catalog build failed".to_string(),
                    ));
                }
            }

            ctx.ready.signal();
            TaskOutcome::Completed
        })
    }
}
