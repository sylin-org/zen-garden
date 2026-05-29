//! Harvest-image reclaimer (STORAGE-0020).
//!
//! Removes leaked `zen-harvest/*` images — the tagged images a snapshot
//! capture commits and is supposed to dispose of, but which survive an
//! aborted capture or a pre-fix build. `prune_dangling_images` cannot
//! reclaim these (they are tagged, not dangling), so this reclaimer closes
//! that gap. Pure junk: reclaimed first, at every level.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::docker::ContainerRuntime;
use crate::domain::capacity::budget::ReclaimLevel;
use crate::domain::capacity::reclaimable::{Reclaimable, ReclaimPriority, Reclaimed};

const NAME: &str = "harvest-images";

/// Minimum age before a `zen-harvest/*` image is treated as a leak. A
/// just-committed image may be the `docker save` source of a capture in
/// flight; removing it would abort that capture. A capture's commit→save→
/// dispose window is far shorter than this, so anything older is genuinely
/// orphaned.
const MIN_LEAK_AGE_SECS: i64 = 15 * 60;

/// Reclaims disk by deleting leaked `zen-harvest/*` images.
pub struct HarvestImageReclaimer {
    container: Arc<ContainerRuntime>,
}

impl HarvestImageReclaimer {
    pub fn new(container: Arc<ContainerRuntime>) -> Self {
        Self { container }
    }
}

impl Reclaimable for HarvestImageReclaimer {
    fn name(&self) -> &'static str {
        NAME
    }

    fn priority(&self) -> ReclaimPriority {
        // Leaked images have no value — reclaim before touching backups.
        ReclaimPriority::Eager
    }

    fn reclaim<'a>(
        &'a self,
        _level: ReclaimLevel,
    ) -> Pin<Box<dyn Future<Output = Reclaimed> + Send + 'a>> {
        // Level-independent: a leaked image is junk whatever the pressure.
        Box::pin(async move {
            let images = match self.container.list_harvest_images().await {
                Ok(images) => images,
                Err(e) => {
                    return Reclaimed {
                        reclaimer: NAME,
                        items_removed: 0,
                        notes: vec![format!("listing harvest images failed (non-fatal): {e}")],
                    };
                }
            };

            let now = chrono::Utc::now().timestamp();
            let mut removed = 0usize;
            let mut spared_in_flight = 0usize;

            for image in images {
                if now.saturating_sub(image.created_unix) < MIN_LEAK_AGE_SECS {
                    spared_in_flight += 1;
                    continue;
                }
                // Remove by id with force: drops the image and all its tags,
                // idempotent on a 404.
                match self.container.remove_image(&image.id, true).await {
                    Ok(()) => removed += 1,
                    Err(e) => tracing::warn!(
                        image = %image.id,
                        error = %e,
                        "capacity: failed to remove leaked harvest image (non-fatal)"
                    ),
                }
            }

            let mut notes = Vec::new();
            if removed > 0 {
                notes.push(format!("removed {removed} leaked zen-harvest image(s)"));
            }
            if spared_in_flight > 0 {
                notes.push(format!(
                    "spared {spared_in_flight} recent image(s) (possible in-flight capture)"
                ));
            }

            Reclaimed {
                reclaimer: NAME,
                items_removed: removed,
                notes,
            }
        })
    }
}
